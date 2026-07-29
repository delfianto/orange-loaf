use std::path::{Path, PathBuf};
use std::time::SystemTime;

use tauri::{AppHandle, State};

use crate::config::Config;
use crate::encoders::tracks::{AudioTrackSelection, SubtitleTrackSelection};
use crate::encoders::{EncodeContext, EncodeStep, EncoderKind, EncoderProfile, Trim};
use crate::filters::FilterSpec;
use crate::job::{self, EncodeStatus};
use crate::naming;
use crate::probe::{self, VideoInfo};
use crate::state::AppState;
use crate::thumbnail;
use crate::tools::{self, ToolStatus, detect_tool};

/// Resolves the configured naming template (falling back to the built-in
/// default if it's been cleared to blank) into a real output path. Written
/// into `default_output_dir` when one is configured, otherwise next to the
/// source (the historical behavior). Replaces the old hardcoded `movie.mp4`
/// -> `movie-orangeloaf.mkv` placeholder now that a real template system
/// exists (notes/plan.md M12).
pub(crate) fn build_output_path(
    template: &str,
    source: &str,
    settings: &EncoderKind,
    filters: &FilterSpec,
    video_info: Option<&VideoInfo>,
    default_output_dir: Option<&Path>,
) -> String {
    let path = Path::new(source);
    let parent = default_output_dir
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or_else(|| path.parent().unwrap_or_else(|| Path::new("")));
    let effective_template = if template.trim().is_empty() {
        crate::config::default_naming_template()
    } else {
        template.to_owned()
    };
    let stem = naming::resolve_pre_encode(
        &effective_template,
        &naming::PreEncodeInputs {
            source_path: source,
            settings,
            filters,
            video: video_info,
        },
    );
    parent.join(format!("{stem}.mkv")).display().to_string()
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

/// Detects the status of every external tool `OrangeLoaf` depends on.
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

/// Whether ffmpeg's real, built-in `h264_nvenc` hardware encoder actually
/// works on this machine right now (real GPU + driver, not just "ffmpeg was
/// compiled with NVENC support") — powers the persistent status bar's
/// "NVENC Available" indicator (`design/mockups/*.html`).
///
/// # Panics
///
/// Panics if the config mutex is poisoned (i.e. a prior command panicked
/// while holding it).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn detect_nvenc(state: State<'_, AppState>) -> bool {
    let config = state.config.lock().expect("config mutex poisoned");
    tools::ffmpeg_path(&config).is_ok_and(|ffmpeg| tools::detect_nvenc(&ffmpeg))
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

/// Probes a source video and returns the metadata `OrangeLoaf` currently shows.
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
/// Returns an error if ffmpeg isn't configured/found.
///
/// # Panics
///
/// Panics if the config mutex is poisoned (i.e. a prior command panicked
/// while holding it).
// Every parameter is a distinct, necessary piece of IPC input (job
// identity, encoder settings, and several independently-optional
// track/filter/probe selections) — bundling them into a wrapper struct
// would just move the field count somewhere else, not reduce it.
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
    video_info: Option<VideoInfo>,
) -> Result<String, String> {
    let (ffmpeg, ffprobe, naming_template, default_output_dir) = {
        let config = state.config.lock().expect("config mutex poisoned");
        (
            tools::ffmpeg_path(&config)?,
            tools::ffprobe_path(&config).ok(),
            config.output_naming_template.clone(),
            config.default_output_dir.clone(),
        )
    };
    let output = build_output_path(
        &naming_template,
        &path,
        &settings,
        &filters,
        video_info.as_ref(),
        default_output_dir.as_deref(),
    );
    let hdr10 = video_info.as_ref().and_then(|v| v.hdr10);
    let trimmed_duration_secs = trimmed_duration(duration_secs, &trim);
    let ctx = EncodeContext {
        filters,
        audio_tracks,
        subtitle_tracks,
        hdr10,
        ..EncodeContext::new(path, output.clone())
    }
    .with_trim(trim);
    let steps = settings.build_steps(&ctx);

    let job_id = uuid::Uuid::new_v4().to_string();
    let job_id_for_task = job_id.clone();
    let started_at = SystemTime::now();
    tauri::async_runtime::spawn(async move {
        let outcome = job::run(app, job_id_for_task, ffmpeg, steps, trimmed_duration_secs).await;
        // Best-effort and only relevant if the template actually used a
        // post-encode variable — see `naming::finalize_output_naming`.
        // There's no `Job`/History record on this direct-encode path (M11
        // scope note) to update with the renamed path, so the file itself
        // is the only place this result is observable — which is fine,
        // since that's exactly where a naming template's job is done.
        if let (EncodeStatus::Completed, Some(ffprobe)) = (outcome, ffprobe) {
            let _ = naming::finalize_output_naming(&ffprobe, &output, started_at);
        }
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

/// Builds the exact command(s) the current settings would run, without
/// executing anything — powers the live "Command Preview" section. Reuses
/// [`EncoderProfile::build_steps`] directly so the preview can never drift
/// from what `start_encode` actually runs. Returns one argv per step
/// (almost always exactly one; two for a 2-pass bitrate encode) rather
/// than assuming the first step is the whole story — a 2-pass preview that
/// silently showed only pass 1's command would be actively misleading,
/// not just incomplete.
///
/// # Errors
///
/// Returns an error if ffmpeg isn't configured/found.
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
    video_info: Option<VideoInfo>,
) -> Result<Vec<Vec<String>>, String> {
    let (ffmpeg, naming_template, default_output_dir) = {
        let config = state.config.lock().expect("config mutex poisoned");
        (
            tools::ffmpeg_path(&config)?,
            config.output_naming_template.clone(),
            config.default_output_dir.clone(),
        )
    };
    let output = build_output_path(
        &naming_template,
        path,
        &settings,
        &filters,
        video_info.as_ref(),
        default_output_dir.as_deref(),
    );
    let hdr10 = video_info.as_ref().and_then(|v| v.hdr10);
    let ctx = EncodeContext {
        filters,
        audio_tracks,
        subtitle_tracks,
        hdr10,
        ..EncodeContext::new(path, output)
    }
    .with_trim(trim);
    let steps = settings.build_steps(&ctx);
    Ok(steps
        .into_iter()
        .map(|EncodeStep::Argv(mut args)| {
            args.insert(0, ffmpeg.display().to_string());
            args
        })
        .collect())
}

/// Returns the current persisted configuration, for the Settings screen.
///
/// # Panics
///
/// Panics if the config mutex is poisoned (i.e. a prior command panicked
/// while holding it).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> Config {
    state.config.lock().expect("config mutex poisoned").clone()
}

/// Replaces the whole persisted configuration with `config` — the Settings
/// screen always sends back its full working copy rather than one field at
/// a time (unlike `set_tool_path`, which stays a narrow setter since
/// Browse-for-binary is its own already-shipped flow with its own
/// re-detection side effect).
///
/// # Errors
///
/// Returns an error if saving the updated config to disk fails.
///
/// # Panics
///
/// Panics if the config mutex is poisoned (i.e. a prior command panicked
/// while holding it).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn update_config(state: State<'_, AppState>, config: Config) -> Result<(), String> {
    config.save()?;
    *state.config.lock().expect("config mutex poisoned") = config;
    Ok(())
}

/// Lists every naming-template variable, for the Settings screen's
/// clickable variable chips.
#[tauri::command]
pub fn list_naming_variables() -> Vec<naming::NamingVariableInfo> {
    naming::list_variables()
}

/// Renders `template` with canned example values — the Settings screen's
/// live naming-template preview.
#[tauri::command]
pub fn preview_naming_template(template: &str) -> String {
    naming::generate_example_preview(template)
}
