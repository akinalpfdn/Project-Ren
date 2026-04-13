//! `system.volume` — get and set the master output volume on Windows.
//!
//! Uses the Core Audio APIs (`IMMDeviceEnumerator` → `IAudioEndpointVolume`)
//! through the `windows` crate. All COM calls happen inside a blocking
//! thread via `tokio::task::spawn_blocking` because COM apartments are
//! thread-bound.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tools::{Tool, ToolError, ToolResult};

pub struct VolumeControl;

#[derive(Debug, Clone, Copy)]
enum VolumeAction {
    Get,
    Set(u8),
    Mute(bool),
}

#[async_trait]
impl Tool for VolumeControl {
    fn name(&self) -> &str {
        "system.volume"
    }

    fn description(&self) -> &str {
        "Read or change the master output volume. Use action='get' to read the current level, \
         'set' with level 0-100 to change it, or 'mute' with muted=true/false to toggle mute."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["get", "set", "mute"],
                    "description": "What to do with the volume."
                },
                "level": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": 100,
                    "description": "Target volume percentage (required when action='set')."
                },
                "muted": {
                    "type": "boolean",
                    "description": "Whether to mute (true) or unmute (false). Required when action='mute'."
                }
            },
            "required": ["action"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        let action = parse_action(&args).map_err(|e| ToolError::invalid_args(self.name(), e))?;

        let result = tokio::task::spawn_blocking(move || apply_action(action))
            .await
            .map_err(|e| ToolError::execution("system.volume", format!("join error: {}", e)))?
            .map_err(|e| ToolError::execution("system.volume", e))?;

        Ok(result)
    }
}

fn parse_action(args: &Value) -> Result<VolumeAction, String> {
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or("missing 'action' (one of get|set|mute)")?;

    match action {
        "get" => Ok(VolumeAction::Get),
        "set" => {
            let level = args
                .get("level")
                .and_then(|v| v.as_u64())
                .ok_or("'set' requires integer 'level' between 0 and 100")?;
            if level > 100 {
                return Err("'level' must be between 0 and 100".into());
            }
            Ok(VolumeAction::Set(level as u8))
        }
        "mute" => {
            let muted = args
                .get("muted")
                .and_then(|v| v.as_bool())
                .ok_or("'mute' requires boolean 'muted'")?;
            Ok(VolumeAction::Mute(muted))
        }
        other => Err(format!("unknown action '{}'", other)),
    }
}

#[cfg(windows)]
fn apply_action(action: VolumeAction) -> Result<ToolResult, String> {
    use windows::Win32::Media::Audio::{
        eMultimedia, eRender, Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator,
        MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    // SAFETY: COM initialisation is balanced with `CoUninitialize` in the
    // `Drop` guard below. All interface pointers live only within this
    // function and are released by the `windows` crate's `Drop` impls.
    unsafe {
        let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
        if hr.is_err() && hr.0 != 0x00000001u32 as i32 {
            // RPC_E_CHANGED_MODE is fine — a prior init in a different
            // apartment just means we should skip uninit here.
            return Err(format!("CoInitializeEx failed: 0x{:08x}", hr.0));
        }
        let uninit_guard = ComUninit;

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("MMDeviceEnumerator: {}", e))?;

        let device = enumerator
            .GetDefaultAudioEndpoint(eRender, eMultimedia)
            .map_err(|e| format!("GetDefaultAudioEndpoint: {}", e))?;

        let endpoint: IAudioEndpointVolume = device
            .Activate(CLSCTX_ALL, None)
            .map_err(|e| format!("IAudioEndpointVolume activate: {}", e))?;

        let result = match action {
            VolumeAction::Get => {
                let level = endpoint
                    .GetMasterVolumeLevelScalar()
                    .map_err(|e| format!("GetMasterVolumeLevelScalar: {}", e))?;
                let muted = endpoint
                    .GetMute()
                    .map_err(|e| format!("GetMute: {}", e))?
                    .as_bool();
                let percent = (level * 100.0).round() as u8;
                let summary = if muted {
                    format!("Volume is at {}%, currently muted.", percent)
                } else {
                    format!("Volume is at {}%.", percent)
                };
                Ok(ToolResult::new(summary))
            }
            VolumeAction::Set(level) => {
                let scalar = (level as f32) / 100.0;
                endpoint
                    .SetMasterVolumeLevelScalar(scalar, std::ptr::null())
                    .map_err(|e| format!("SetMasterVolumeLevelScalar: {}", e))?;
                Ok(ToolResult::new(format!("Volume set to {}%.", level)))
            }
            VolumeAction::Mute(muted) => {
                endpoint
                    .SetMute(muted, std::ptr::null())
                    .map_err(|e| format!("SetMute: {}", e))?;
                Ok(ToolResult::new(
                    if muted { "Audio muted." } else { "Audio unmuted." }.to_string(),
                ))
            }
        };

        drop(uninit_guard);
        result
    }
}

#[cfg(not(windows))]
fn apply_action(_action: VolumeAction) -> Result<ToolResult, String> {
    Err("system volume control is only supported on Windows".into())
}

/// Runs `CoUninitialize` when dropped. Scoped to one `apply_action` call.
#[cfg(windows)]
struct ComUninit;

#[cfg(windows)]
impl Drop for ComUninit {
    fn drop(&mut self) {
        // SAFETY: Paired with the `CoInitializeEx` at the top of
        // `apply_action`; runs on the same thread.
        unsafe { windows::Win32::System::Com::CoUninitialize() };
    }
}
