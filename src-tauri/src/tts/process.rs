//! Lifecycle for the `ren-tts` sidecar.
//!
//! Mirrors `llm::ollama_process` exactly so the supervision pattern is
//! consistent across all child processes Ren spawns. The sidecar owns its
//! own CUDA context (ORT-bundled) and never collides with whisper.cpp's
//! CUDA backend living inside the main app.

use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use tracing::{info, warn};

use crate::config::{app_data_dir, bin_dir};

/// Bind range reserved for the sidecar — well clear of Ollama's 11500..11520.
const PORT_RANGE_START: u16 = 11530;
const PORT_RANGE_END: u16 = 11550;
const HEALTH_TIMEOUT_SECS: u64 = 30;
const HEALTH_INTERVAL_MS: u64 = 200;

static ACTIVE_PORT: AtomicU16 = AtomicU16::new(0);

pub fn active_port() -> u16 {
    ACTIVE_PORT.load(Ordering::Relaxed)
}

pub fn base_url() -> Option<String> {
    let port = active_port();
    if port == 0 {
        None
    } else {
        Some(format!("http://127.0.0.1:{}", port))
    }
}

/// Returns the path to the bundled `ren-tts.exe`.
///
/// In dev builds Cargo emits the sidecar to `target/<profile>/ren-tts.exe`
/// next to `ren.exe`, so we resolve relative to the current executable.
/// Falls back to `%APPDATA%\Ren\bin\ren-tts.exe` for installed users.
pub fn sidecar_exe_path() -> Result<std::path::PathBuf> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();

    if let Ok(current) = std::env::current_exe() {
        if let Some(dir) = current.parent() {
            candidates.push(dir.join("ren-tts.exe"));
        }
    }
    candidates.push(bin_dir()?.join("ren-tts.exe"));

    candidates
        .into_iter()
        .find(|p| p.exists())
        .ok_or_else(|| anyhow::anyhow!("ren-tts.exe not found next to ren.exe or in bin dir"))
}

/// Spawns `ren-tts.exe`, attaches it to a Job Object, and waits for it to
/// answer `/health`. Returns the child handle so the caller can keep ownership
/// and eventually call [`terminate`].
pub async fn start(default_voice: &str) -> Result<Child> {
    let exe = sidecar_exe_path()?;
    let port = find_free_port().await?;
    let log_path = app_data_dir()?.join("logs").join("ren-tts.log");

    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("Cannot open ren-tts log: {}", log_path.display()))?;
    let log_file_stderr = log_file.try_clone()?;

    info!(
        "Starting ren-tts sidecar on port {} (binary: {})",
        port,
        exe.display()
    );

    let mut child = Command::new(&exe)
        .arg("--port")
        .arg(port.to_string())
        .arg("--default-voice")
        .arg(default_voice)
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file_stderr))
        .spawn()
        .with_context(|| format!("Failed to spawn {}", exe.display()))?;

    #[cfg(windows)]
    attach_job_object(child.id())?;

    ACTIVE_PORT.store(port, Ordering::Relaxed);

    health_check(port).await.map_err(|e| {
        let _ = child.kill();
        ACTIVE_PORT.store(0, Ordering::Relaxed);
        e
    })?;

    info!("ren-tts ready on port {}", port);
    Ok(child)
}

pub fn terminate(child: &mut Child) {
    if let Err(e) = child.kill() {
        warn!("Failed to kill ren-tts child: {}", e);
    } else {
        let _ = child.wait();
        info!("ren-tts child terminated");
    }
    ACTIVE_PORT.store(0, Ordering::Relaxed);
}

async fn health_check(port: u16) -> Result<()> {
    let url = format!("http://127.0.0.1:{}/health", port);
    let client = reqwest::Client::new();
    let timeout = Duration::from_secs(HEALTH_TIMEOUT_SECS);
    let interval = Duration::from_millis(HEALTH_INTERVAL_MS);
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        if let Ok(resp) = client.get(&url).send().await {
            if resp.status().is_success() {
                return Ok(());
            }
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!(
                "ren-tts did not become ready within {}s on port {}",
                HEALTH_TIMEOUT_SECS,
                port
            );
        }
        tokio::time::sleep(interval).await;
    }
}

async fn find_free_port() -> Result<u16> {
    for port in PORT_RANGE_START..=PORT_RANGE_END {
        if tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port))
            .await
            .is_ok()
        {
            return Ok(port);
        }
    }
    anyhow::bail!(
        "No free port for ren-tts in {}..={}",
        PORT_RANGE_START,
        PORT_RANGE_END
    )
}

#[cfg(windows)]
fn attach_job_object(pid: u32) -> Result<()> {
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_ALL_ACCESS};

    unsafe {
        let job = CreateJobObjectW(None, None).context("CreateJobObjectW failed")?;

        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
        .context("SetInformationJobObject failed")?;

        let process =
            OpenProcess(PROCESS_ALL_ACCESS, false, pid).context("OpenProcess failed")?;

        AssignProcessToJobObject(job, process).context("AssignProcessToJobObject failed")?;
        info!("ren-tts child (pid {}) attached to Job Object", pid);
    }
    Ok(())
}
