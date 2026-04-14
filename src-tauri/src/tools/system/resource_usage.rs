//! `system.resource_usage` — quick health snapshot of the box. CPU + RAM
//! come from `sysinfo` (cross-platform, no setup); GPU stats are pulled by
//! shelling out to `nvidia-smi` so we do not pin an NVML build path. If
//! `nvidia-smi` is missing or fails, the tool reports CPU + RAM only and
//! says so explicitly rather than guessing.

use async_trait::async_trait;
use serde_json::{json, Value};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

use crate::tools::{Tool, ToolError, ToolResult};

pub struct ResourceUsage;

#[async_trait]
impl Tool for ResourceUsage {
    fn name(&self) -> &str {
        "system.resource_usage"
    }

    fn description(&self) -> &str {
        "Report a quick CPU / RAM / GPU usage snapshot. GPU is best-effort \
         and only available when nvidia-smi is on PATH."
    }

    fn parameters(&self) -> Value {
        json!({ "type": "object", "properties": {}, "additionalProperties": false })
    }

    async fn execute(&self, _args: Value) -> Result<ToolResult, ToolError> {
        let snapshot = tokio::task::spawn_blocking(read_snapshot)
            .await
            .map_err(|e| ToolError::execution(self.name(), format!("join error: {}", e)))?;

        let summary = snapshot.narrate();
        Ok(ToolResult::with_detail(summary, snapshot.to_detail_string()))
    }
}

#[derive(Debug)]
struct Snapshot {
    cpu_percent: f32,
    memory_used_gb: f64,
    memory_total_gb: f64,
    gpu: Option<GpuSnapshot>,
}

#[derive(Debug)]
struct GpuSnapshot {
    util_percent: u32,
    memory_used_mb: u32,
    memory_total_mb: u32,
}

impl Snapshot {
    fn narrate(&self) -> String {
        let mem_pct = if self.memory_total_gb > 0.0 {
            (self.memory_used_gb / self.memory_total_gb) * 100.0
        } else {
            0.0
        };
        let mut parts = vec![
            format!("CPU at {:.0}%", self.cpu_percent),
            format!(
                "RAM at {:.0}% ({:.1} of {:.1} GB)",
                mem_pct, self.memory_used_gb, self.memory_total_gb
            ),
        ];
        if let Some(g) = &self.gpu {
            parts.push(format!(
                "GPU at {}% ({} of {} MB VRAM)",
                g.util_percent, g.memory_used_mb, g.memory_total_mb
            ));
        } else {
            parts.push("GPU stats unavailable (nvidia-smi not found)".to_string());
        }
        parts.join(", ") + "."
    }

    fn to_detail_string(&self) -> String {
        json!({
            "cpu_percent": self.cpu_percent,
            "memory_used_gb": self.memory_used_gb,
            "memory_total_gb": self.memory_total_gb,
            "gpu": self.gpu.as_ref().map(|g| json!({
                "util_percent": g.util_percent,
                "memory_used_mb": g.memory_used_mb,
                "memory_total_mb": g.memory_total_mb,
            })),
        })
        .to_string()
    }
}

fn read_snapshot() -> Snapshot {
    // sysinfo CPU usage needs two refreshes a moment apart to compute deltas.
    let mut sys = System::new_with_specifics(
        RefreshKind::new()
            .with_cpu(CpuRefreshKind::everything())
            .with_memory(MemoryRefreshKind::everything()),
    );
    sys.refresh_cpu_usage();
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_cpu_usage();
    sys.refresh_memory();

    let cpu_percent = sys.global_cpu_usage();
    let total_bytes = sys.total_memory();
    let used_bytes = sys.used_memory();
    let memory_total_gb = total_bytes as f64 / 1024.0_f64.powi(3);
    let memory_used_gb = used_bytes as f64 / 1024.0_f64.powi(3);

    Snapshot {
        cpu_percent,
        memory_used_gb,
        memory_total_gb,
        gpu: read_gpu_via_nvidia_smi(),
    }
}

fn read_gpu_via_nvidia_smi() -> Option<GpuSnapshot> {
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu,memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next()?;
    let mut parts = line.split(',').map(str::trim);
    let util = parts.next()?.parse::<u32>().ok()?;
    let used = parts.next()?.parse::<u32>().ok()?;
    let total = parts.next()?.parse::<u32>().ok()?;
    Some(GpuSnapshot {
        util_percent: util,
        memory_used_mb: used,
        memory_total_mb: total,
    })
}
