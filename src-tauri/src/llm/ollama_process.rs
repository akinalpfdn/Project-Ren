use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use tracing::{error, info, warn};

use crate::config::{app_data_dir, bin_dir, models_dir};
use crate::config::defaults::{
    OLLAMA_HEALTH_CHECK_INTERVAL_MS, OLLAMA_HEALTH_CHECK_TIMEOUT_SECS,
    OLLAMA_PINNED_VERSION, OLLAMA_PORT_PROBE_MAX, OLLAMA_PREFERRED_PORT,
};

/// Selected port stored here after start() succeeds.
static ACTIVE_PORT: AtomicU16 = AtomicU16::new(0);

/// Returns the port Ollama child is currently listening on.
/// Zero if not started yet.
pub fn active_port() -> u16 {
    ACTIVE_PORT.load(Ordering::Relaxed)
}

/// Returns the download URL for the pinned Ollama binary.
pub fn ollama_download_url() -> String {
    format!(
        "https://github.com/ollama/ollama/releases/download/v{}/ollama-windows-amd64.exe",
        OLLAMA_PINNED_VERSION
    )
}

/// Returns `%APPDATA%\Ren\bin\ollama.exe`.
pub fn ollama_exe_path() -> Result<std::path::PathBuf> {
    Ok(bin_dir()?.join("ollama.exe"))
}

/// Spawns `ollama serve` as a managed child process.
///
/// - Binds to a non-default port (probed starting from `OLLAMA_PREFERRED_PORT`).
/// - Sets `OLLAMA_MODELS` to Ren's private model directory.
/// - On Windows, attaches child to a Job Object so it is killed if Ren crashes.
/// - Polls `/api/tags` until ready or timeout.
///
/// Returns the child handle. Caller must call `terminate()` on shutdown.
pub async fn start() -> Result<Child> {
    let exe = ollama_exe_path()?;

    if !exe.exists() {
        anyhow::bail!(
            "Ollama binary not found at {}. Run first-time setup.",
            exe.display()
        );
    }

    let port = find_free_port().await?;
    let models_path = models_dir()?.join("ollama");
    let log_path = app_data_dir()?.join("logs").join("ollama.log");

    std::fs::create_dir_all(&models_path)?;

    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("Cannot open Ollama log: {}", log_path.display()))?;
    let log_file_stderr = log_file.try_clone()?;

    info!("Starting Ollama on port {} (models: {})", port, models_path.display());

    let mut child = Command::new(&exe)
        .arg("serve")
        .env("OLLAMA_HOST", format!("127.0.0.1:{}", port))
        .env("OLLAMA_MODELS", &models_path)
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file_stderr))
        .spawn()
        .with_context(|| format!("Failed to spawn {}", exe.display()))?;

    // Attach to Windows Job Object so child dies with parent
    #[cfg(windows)]
    attach_job_object(child.id())?;

    ACTIVE_PORT.store(port, Ordering::Relaxed);

    // Wait for Ollama to be ready
    health_check(port).await.map_err(|e| {
        let _ = child.kill();
        e
    })?;

    info!("Ollama ready on port {}", port);
    Ok(child)
}

/// Gracefully terminates the Ollama child process.
pub fn terminate(child: &mut Child) {
    if let Err(e) = child.kill() {
        warn!("Failed to kill Ollama child: {}", e);
    } else {
        let _ = child.wait();
        info!("Ollama child terminated");
    }
    ACTIVE_PORT.store(0, Ordering::Relaxed);
}

/// Polls `GET /api/tags` until 200 OK or timeout.
async fn health_check(port: u16) -> Result<()> {
    let url = format!("http://127.0.0.1:{}/api/tags", port);
    let client = reqwest::Client::new();
    let timeout = Duration::from_secs(OLLAMA_HEALTH_CHECK_TIMEOUT_SECS);
    let interval = Duration::from_millis(OLLAMA_HEALTH_CHECK_INTERVAL_MS);
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => return Ok(()),
            Ok(resp) => {
                warn!("Ollama health check got HTTP {}", resp.status());
            }
            Err(_) => {} // Not ready yet — keep trying
        }

        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!(
                "Ollama did not become ready within {}s on port {}",
                OLLAMA_HEALTH_CHECK_TIMEOUT_SECS,
                port
            );
        }

        tokio::time::sleep(interval).await;
    }
}

/// Probes for a free port starting at `OLLAMA_PREFERRED_PORT`.
async fn find_free_port() -> Result<u16> {
    for port in OLLAMA_PREFERRED_PORT..=OLLAMA_PORT_PROBE_MAX {
        if is_port_free(port).await {
            return Ok(port);
        }
        info!("Port {} occupied, trying next", port);
    }
    anyhow::bail!(
        "No free port found in range {}–{}",
        OLLAMA_PREFERRED_PORT,
        OLLAMA_PORT_PROBE_MAX
    );
}

async fn is_port_free(port: u16) -> bool {
    tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port))
        .await
        .is_ok()
}

/// Attach child process to a Windows Job Object.
/// When the parent (Ren) exits for any reason, the OS kills all job members.
#[cfg(windows)]
fn attach_job_object(pid: u32) -> Result<()> {
    use windows::Win32::{
        Foundation::HANDLE,
        System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW,
            JobObjectExtendedLimitInformation, QueryInformationJobObject,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        },
        System::Threading::{OpenProcess, PROCESS_ALL_ACCESS},
    };

    unsafe {
        let job = CreateJobObjectW(None, None)
            .context("CreateJobObjectW failed")?;

        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
        .context("SetInformationJobObject failed")?;

        let process = OpenProcess(PROCESS_ALL_ACCESS, false, pid)
            .context("OpenProcess failed for Ollama child")?;

        AssignProcessToJobObject(job, process)
            .context("AssignProcessToJobObject failed")?;

        info!("Ollama child (pid {}) attached to Job Object", pid);
    }

    Ok(())
}
