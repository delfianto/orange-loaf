use std::path::PathBuf;

use tauri::State;

use crate::state::AppState;
use crate::tools::{ToolStatus, detect_tool};

fn detect_all(state: &AppState) -> Vec<ToolStatus> {
    let config = state.config.lock().expect("config mutex poisoned");
    vec![
        detect_tool("FFmpeg", "ffmpeg", "FFMPEG", config.ffmpeg_path.as_ref()),
        detect_tool(
            "FFprobe",
            "ffprobe",
            "FFPROBE",
            config.ffprobe_path.as_ref(),
        ),
    ]
}

/// Detects the status of every external tool Voltaic depends on.
///
/// # Panics
///
/// Panics if the config mutex is poisoned (i.e. a prior command panicked
/// while holding it).
// tauri::command's argument extraction only supports owned `State<'_, T>`, not
// `&State<'_, T>` — clippy's by-reference suggestion doesn't apply here.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn detect_tools(state: State<'_, AppState>) -> Vec<ToolStatus> {
    detect_all(&state)
}

/// Sets a manually-chosen path for the given tool and persists it, then
/// returns the freshly re-detected status of every tool.
///
/// # Errors
///
/// Returns an error if `tool` isn't a known tool name, or if saving the
/// updated config to disk fails.
///
/// # Panics
///
/// Panics if the config mutex is poisoned (i.e. a prior command panicked
/// while holding it).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn set_tool_path(
    state: State<'_, AppState>,
    tool: &str,
    path: String,
) -> Result<Vec<ToolStatus>, String> {
    {
        let mut config = state.config.lock().expect("config mutex poisoned");
        match tool {
            "ffmpeg" => config.ffmpeg_path = Some(PathBuf::from(path)),
            "ffprobe" => config.ffprobe_path = Some(PathBuf::from(path)),
            other => return Err(format!("unknown tool: {other}")),
        }
        config.save()?;
    }
    Ok(detect_all(&state))
}
