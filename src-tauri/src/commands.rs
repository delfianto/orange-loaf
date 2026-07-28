use std::path::{Path, PathBuf};

use tauri::{AppHandle, State};

use crate::encoders::tracks::{AudioTrackSelection, SubtitleTrackSelection};
use crate::encoders::{EncodeContext, EncodeStep, EncoderKind, EncoderProfile, Trim};
use crate::filters::FilterSpec;
use crate::job;
use crate::probe::{self, Hdr10Metadata, VideoInfo};
use crate::state::AppState;
use crate::thumbnail;
use crate::tools::{self, ToolStatus, detect_tool};

/// Derives an output path next to the source rather than overwriting it:
/// `movie.mp4` -> `movie-voltaic.mkv`. No naming-template system yet (M12) —
/// this is deliberately the simplest thing that produces a distinct,
/// predictable output path.
fn derive_output_path(source: &str) -> Result<String, String> {
    let path = Path::new(source);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("source path has no file name")?;
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    Ok(parent
        .join(format!("{stem}-voltaic.mkv"))
        .display()
        .to_string())
}

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

/// Probes a source video and returns the metadata Voltaic currently shows.
///
/// # Errors
///
/// Returns an error if ffprobe isn't configured/found, or if probing fails
/// (unreadable file, no video stream, ...).
///
/// # Panics
///
/// Panics if the config mutex is poisoned (i.e. a prior command panicked
/// while holding it).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn probe_video(state: State<'_, AppState>, path: &str) -> Result<VideoInfo, String> {
    let config = state.config.lock().expect("config mutex poisoned");
    let ffprobe = tools::ffprobe_path(&config)?;
    probe::probe_video(&ffprobe, path)
}

/// Extracts a preview thumbnail at `timestamp_secs` as a base64 JPEG data URI.
///
/// # Errors
///
/// Returns an error if ffmpeg isn't configured/found, or if frame extraction
/// fails.
///
/// # Panics
///
/// Panics if the config mutex is poisoned (i.e. a prior command panicked
/// while holding it).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn generate_thumbnail(
    state: State<'_, AppState>,
    path: &str,
    timestamp_secs: f64,
) -> Result<String, String> {
    let config = state.config.lock().expect("config mutex poisoned");
    let ffmpeg = tools::ffmpeg_path(&config)?;
    thumbnail::generate_thumbnail(&ffmpeg, path, timestamp_secs)
}

/// Starts an encode of `path` with the given encoder settings/filters as a
/// background job, returning immediately with a job id. Progress and
/// completion/failure are streamed as `encode:event` events (see
/// [`job::run`]) rather than returned from this command.
///
/// # Errors
///
/// Returns an error if ffmpeg isn't configured/found, or if the output path
/// can't be derived from `path`.
///
/// # Panics
///
/// Panics if the config mutex is poisoned (i.e. a prior command panicked
/// while holding it).
// Every parameter is a distinct, necessary piece of IPC input (job
// identity, encoder settings, and three independently-optional track/filter
// selections) — bundling them into a wrapper struct would just move the
// field count somewhere else, not reduce it.
#[allow(clippy::needless_pass_by_value, clippy::too_many_arguments)]
#[tauri::command]
pub fn start_encode(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    duration_secs: f64,
    settings: EncoderKind,
    filters: FilterSpec,
    trim: Trim,
    audio_tracks: Vec<AudioTrackSelection>,
    subtitle_tracks: Vec<SubtitleTrackSelection>,
    hdr10: Option<Hdr10Metadata>,
) -> Result<String, String> {
    let ffmpeg = {
        let config = state.config.lock().expect("config mutex poisoned");
        tools::ffmpeg_path(&config)?
    };
    let output = derive_output_path(&path)?;
    let trimmed_duration_secs = trimmed_duration(duration_secs, &trim);
    let ctx = EncodeContext {
        filters,
        audio_tracks,
        subtitle_tracks,
        hdr10,
        ..EncodeContext::new(path, output)
    }
    .with_trim(trim);
    let steps = settings.build_steps(&ctx);

    let job_id = uuid::Uuid::new_v4().to_string();
    let job_id_for_task = job_id.clone();
    tauri::async_runtime::spawn(async move {
        job::run(app, job_id_for_task, ffmpeg, steps, trimmed_duration_secs).await;
    });

    Ok(job_id)
}

/// Progress percentage should be relative to what's actually being encoded —
/// if the user trims to a 10s clip out of a 2-hour source, "50%" should mean
/// 5s in, not 1 hour in.
fn trimmed_duration(source_duration_secs: f64, trim: &Trim) -> f64 {
    let start = trim.start_time.unwrap_or(0.0);
    let end = trim.end_time.unwrap_or(source_duration_secs);
    (end - start).max(0.0)
}

/// Builds the exact command the current settings would run, without
/// executing anything — powers the live "Command Preview" section. Reuses
/// [`EncoderProfile::build_steps`] directly so the preview can never drift
/// from what `start_encode` actually runs.
///
/// # Errors
///
/// Returns an error if ffmpeg isn't configured/found, or if the output path
/// can't be derived from `path`.
///
/// # Panics
///
/// Panics if the config mutex is poisoned (i.e. a prior command panicked
/// while holding it).
// See `start_encode`'s equivalent comment — same reasoning applies here.
#[allow(clippy::needless_pass_by_value, clippy::too_many_arguments)]
#[tauri::command]
pub fn preview_command(
    state: State<'_, AppState>,
    path: &str,
    settings: EncoderKind,
    filters: FilterSpec,
    trim: Trim,
    audio_tracks: Vec<AudioTrackSelection>,
    subtitle_tracks: Vec<SubtitleTrackSelection>,
    hdr10: Option<Hdr10Metadata>,
) -> Result<Vec<String>, String> {
    let ffmpeg = {
        let config = state.config.lock().expect("config mutex poisoned");
        tools::ffmpeg_path(&config)?
    };
    let output = derive_output_path(path)?;
    let ctx = EncodeContext {
        filters,
        audio_tracks,
        subtitle_tracks,
        hdr10,
        ..EncodeContext::new(path, output)
    }
    .with_trim(trim);
    let steps = settings.build_steps(&ctx);
    let EncodeStep::Argv(mut args) = steps
        .into_iter()
        .next()
        .ok_or("encoder produced no steps")?;
    args.insert(0, ffmpeg.display().to_string());
    Ok(args)
}
