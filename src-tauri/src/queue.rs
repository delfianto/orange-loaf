use std::path::PathBuf;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tokio::sync::oneshot;

use crate::commands::build_output_path;
use crate::encoders::tracks::{AudioTrackSelection, SubtitleTrackSelection};
use crate::encoders::{EncodeContext, EncoderKind, EncoderProfile, Trim};
use crate::filters::FilterSpec;
use crate::job::{self, EncodeStatus};
use crate::naming;
use crate::probe::VideoInfo;
use crate::state::AppState;
use crate::tools;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed { message: String },
    Cancelled,
}

impl From<EncodeStatus> for JobStatus {
    fn from(status: EncodeStatus) -> Self {
        match status {
            // `run_cancelable` never actually returns this — it's only ever
            // an intermediate event — but the conversion is total either way.
            EncodeStatus::Running => Self::Running,
            EncodeStatus::Completed => Self::Completed,
            EncodeStatus::Failed { message } => Self::Failed { message },
            EncodeStatus::Cancelled => Self::Cancelled,
        }
    }
}

/// One queued/running/finished encode. Carries a full, self-contained copy
/// of everything `EncodeContext` needs (settings/filters/trim/tracks/HDR),
/// the same way `HistoryEntry` will (notes/plan.md §4.10) — a queue row
/// must keep working even if the source's probed metadata or the app's
/// defaults change after it was enqueued.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub source: String,
    pub output: String,
    pub duration_secs: f64,
    pub settings: EncoderKind,
    pub filters: FilterSpec,
    pub trim: Trim,
    pub audio_tracks: Vec<AudioTrackSelection>,
    pub subtitle_tracks: Vec<SubtitleTrackSelection>,
    /// The source's full probe result, kept (not just its `hdr10` subset)
    /// so the naming template's source/audio/color/etc. variables still
    /// have real data to resolve from even though this job may run long
    /// after the source was originally probed (notes/plan.md M12).
    pub video_info: Option<VideoInfo>,
    pub status: JobStatus,
    /// The three fields below are all `#[serde(default)]` individually
    /// (not on the struct) so an old `queue.json` from before they existed
    /// still deserializes instead of silently falling back to
    /// `Queue::default()` — see `Queue::load_from` — which would look like
    /// every queued job had simply vanished.
    #[serde(default)]
    pub started_at_unix_ms: Option<u64>,
    #[serde(default)]
    pub finished_at_unix_ms: Option<u64>,
    /// Only ever set for a `Completed` job — measured once, right when it
    /// finishes, rather than re-statted on demand later (the file may have
    /// since moved/been deleted by the time a UI re-reads it).
    #[serde(default)]
    pub output_size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Queue {
    pub jobs: Vec<Job>,
    /// Whether to hold off starting new jobs — the single source of truth
    /// for both dispatch (`dispatch_next` checks it) and persistence (a
    /// paused queue stays paused across a restart, deliberately, since
    /// that's the whole point of pausing). Kept here rather than as a
    /// separate `AppState` field so there's exactly one place this can get
    /// out of sync with what's actually saved/shown.
    pub paused: bool,
}

impl Queue {
    /// Honors `ORANGELOAF_QUEUE` (mirroring `Config::file_path`'s
    /// `ORANGELOAF_CONFIG`), else a per-user data directory — deliberately
    /// *not* the config directory: this is runtime state, not a setting.
    pub fn file_path() -> Option<PathBuf> {
        if let Ok(path) = std::env::var("ORANGELOAF_QUEUE") {
            return Some(PathBuf::from(path));
        }
        directories::ProjectDirs::from("dev", "orangeloaf", "OrangeLoaf")
            .map(|dirs| dirs.data_dir().join("queue.json"))
    }

    pub fn load() -> Self {
        Self::file_path().map_or_else(Self::default, |path| Self::load_from(&path))
    }

    /// # Errors
    ///
    /// Returns an error if the queue file location can't be determined, or
    /// if writing fails.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::file_path().ok_or("could not determine the queue file location")?;
        self.save_to(&path)
    }

    /// The actual load logic, taking an explicit path rather than resolving
    /// one itself — this is what makes it testable against a scratch
    /// tempfile without touching `ORANGELOAF_QUEUE`/real user state (setting
    /// an env var for the duration of a test needs `unsafe` under edition
    /// 2024, which this crate forbids — see `tools.rs` for the same
    /// reasoning applied to config path resolution).
    fn load_from(path: &std::path::Path) -> Self {
        let mut queue: Self = std::fs::read_to_string(path)
            .ok()
            .and_then(|contents| serde_json::from_str(&contents).ok())
            .unwrap_or_default();
        // A job that was "Running" when the app last exited obviously isn't
        // running now — the process behind it is gone. Reset it to Queued
        // rather than leaving a permanently-stuck status (notes/plan.md
        // §4.9's "reset stuck running state left over from a crash" — this
        // pass does that reset unconditionally rather than prompting first).
        for job in &mut queue.jobs {
            if matches!(job.status, JobStatus::Running) {
                job.status = JobStatus::Queued;
            }
        }
        queue
    }

    fn save_to(&self, path: &std::path::Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let contents = serde_json::to_string_pretty(self).map_err(|err| err.to_string())?;
        // Atomic write (temp file in the same directory, then rename) so a
        // crash/power-loss mid-write can't leave a truncated, unparseable
        // queue.json behind (notes/plan.md §4.9). The temp name includes a
        // random component — a fixed name would let two concurrent writers
        // to the same `path` (which only happens today when multiple
        // independent test `Queue`s alias the same real on-disk path, since
        // in the running app `state.queue`'s mutex already serializes every
        // real save) race: one's rename can consume the temp file the other
        // is still about to rename, failing with a spurious "not found."
        let tmp_path = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4().simple()));
        std::fs::write(&tmp_path, &contents).map_err(|err| err.to_string())?;
        std::fs::rename(&tmp_path, path).map_err(|err| err.to_string())
    }
}

fn emit_snapshot<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    let queue = state.queue.lock().expect("queue mutex poisoned");
    let _ = app.emit("queue:snapshot", &*queue);
}

fn finish_job<R: Runtime>(app: &AppHandle<R>, job_id: &str, status: JobStatus) {
    let state = app.state::<AppState>();
    let finished_job = {
        let mut queue = state.queue.lock().expect("queue mutex poisoned");
        let job = queue.jobs.iter_mut().find(|j| j.id == job_id);
        let finished_job = job.as_ref().map(|j| ((*j).clone(), status.clone()));
        if let Some(job) = job {
            job.finished_at_unix_ms = Some(crate::history::unix_ms_from(SystemTime::now()));
            if matches!(status, JobStatus::Completed) {
                job.output_size_bytes = std::fs::metadata(&job.output).ok().map(|m| m.len());
            }
            job.status = status;
        }
        let _ = queue.save();
        finished_job
    };
    *state
        .running_cancel
        .lock()
        .expect("running_cancel mutex poisoned") = None;
    emit_snapshot(app, &state);

    // History records completed/failed queue jobs only — a job that was
    // *cancelled* was intentionally aborted, not an outcome worth keeping a
    // record of (notes/plan.md M11 scope note).
    if let Some((job, status)) = finished_job {
        let history_status = match status {
            JobStatus::Completed => Some(crate::history::HistoryStatus::Completed),
            JobStatus::Failed { message } => {
                Some(crate::history::HistoryStatus::Failed { message })
            }
            JobStatus::Queued | JobStatus::Running | JobStatus::Cancelled => None,
        };
        if let Some(history_status) = history_status {
            let thumbnail_at_secs = job.trim.start_time.unwrap_or(0.0);
            crate::history::record(
                app,
                job.source,
                job.output,
                job.settings,
                job.filters,
                history_status,
                thumbnail_at_secs,
            );
        }
    }
}

/// Starts the next `Queued` job, if nothing is already running and the
/// queue isn't paused — called after every enqueue, every job completion
/// (chaining to the next one), every resume, and once at app startup (to
/// pick back up whatever survived from the last session).
pub fn dispatch_next<R: Runtime>(app: AppHandle<R>) {
    let state = app.state::<AppState>();

    let already_running = state
        .running_cancel
        .lock()
        .expect("running_cancel mutex poisoned")
        .is_some();
    if already_running {
        return;
    }

    let job = {
        let mut queue = state.queue.lock().expect("queue mutex poisoned");
        if queue.paused {
            return;
        }
        let Some(job) = queue
            .jobs
            .iter_mut()
            .find(|j| matches!(j.status, JobStatus::Queued))
        else {
            return;
        };
        job.status = JobStatus::Running;
        job.started_at_unix_ms = Some(crate::history::unix_ms_from(SystemTime::now()));
        let job = job.clone();
        let _ = queue.save();
        job
    };
    emit_snapshot(&app, &state);

    let ffmpeg = {
        let config = state.config.lock().expect("config mutex poisoned");
        tools::ffmpeg_path(&config)
    };
    let ffmpeg = match ffmpeg {
        Ok(path) => path,
        Err(err) => {
            finish_job(&app, &job.id, JobStatus::Failed { message: err });
            dispatch_next(app);
            return;
        }
    };
    // Best-effort only: needed for the post-encode naming pass, which
    // already no-ops when the template has no post-encode variables — a
    // missing ffprobe just means that pass is skipped too, not that the
    // encode itself can't proceed.
    let ffprobe = {
        let config = state.config.lock().expect("config mutex poisoned");
        tools::ffprobe_path(&config).ok()
    };

    let ctx = EncodeContext {
        filters: job.filters.clone(),
        audio_tracks: job.audio_tracks.clone(),
        subtitle_tracks: job.subtitle_tracks.clone(),
        hdr10: job.video_info.as_ref().and_then(|v| v.hdr10),
        ..EncodeContext::new(job.source.clone(), job.output.clone())
    }
    .with_trim(job.trim);
    let steps = job.settings.build_steps(&ctx);

    let (cancel_tx, cancel_rx) = oneshot::channel();
    *state
        .running_cancel
        .lock()
        .expect("running_cancel mutex poisoned") = Some((job.id.clone(), cancel_tx));

    let job_id = job.id.clone();
    let duration_secs = job.duration_secs;
    let output = job.output.clone();
    let started_at = SystemTime::now();
    tauri::async_runtime::spawn(async move {
        let outcome = job::run_cancelable(
            app.clone(),
            job_id.clone(),
            ffmpeg,
            steps,
            duration_secs,
            cancel_rx,
        )
        .await;

        // Placeholder-token rename (notes/plan.md M12) happens here, before
        // `finish_job` reads this job's `output` back out of the queue for
        // its History entry — so a renamed file and its History record
        // never disagree about the filename.
        if let (EncodeStatus::Completed, Some(ffprobe)) = (&outcome, &ffprobe) {
            let renamed = naming::finalize_output_naming(ffprobe, &output, started_at);
            if renamed != output {
                let state = app.state::<AppState>();
                let mut queue = state.queue.lock().expect("queue mutex poisoned");
                if let Some(job) = queue.jobs.iter_mut().find(|j| j.id == job_id) {
                    job.output = renamed;
                }
                let _ = queue.save();
            }
        }

        finish_job(&app, &job_id, outcome.into());
        dispatch_next(app);
    });
}

// tauri::command's argument extraction only supports owned `State<'_, T>`,
// not `&State<'_, T>` — clippy's by-reference suggestion doesn't apply to
// any command in this file, for the same reason (see commands.rs).
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn list_queue(state: State<'_, AppState>) -> Queue {
    state.queue.lock().expect("queue mutex poisoned").clone()
}

/// Adds a new job (built from the Convert screen's current settings,
/// mirroring `start_encode`'s parameters exactly) to the end of the queue,
/// then triggers dispatch in case nothing else is running. Extracted from
/// the `#[tauri::command]` wrapper (which can't be generic — see
/// `cancel_job`/`cancel_job_impl`'s equivalent split) specifically so tests
/// can call it with a `MockRuntime` handle and exercise the real
/// naming-template resolution path, not just the command's simpler
/// hand-built-`Job` shortcut.
#[allow(clippy::too_many_arguments)]
fn enqueue_job_impl<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    source: String,
    duration_secs: f64,
    settings: EncoderKind,
    filters: FilterSpec,
    trim: Trim,
    audio_tracks: Vec<AudioTrackSelection>,
    subtitle_tracks: Vec<SubtitleTrackSelection>,
    video_info: Option<VideoInfo>,
) -> Result<Queue, String> {
    let naming_template = state
        .config
        .lock()
        .expect("config mutex poisoned")
        .output_naming_template
        .clone();
    let output = build_output_path(
        &naming_template,
        &source,
        &settings,
        &filters,
        video_info.as_ref(),
    );
    let job = Job {
        id: uuid::Uuid::new_v4().to_string(),
        source,
        output,
        duration_secs,
        settings,
        filters,
        trim,
        audio_tracks,
        subtitle_tracks,
        video_info,
        status: JobStatus::Queued,
        started_at_unix_ms: None,
        finished_at_unix_ms: None,
        output_size_bytes: None,
    };
    let snapshot = {
        let mut queue = state.queue.lock().expect("queue mutex poisoned");
        queue.jobs.push(job);
        queue.save()?;
        queue.clone()
    };
    dispatch_next(app.clone());
    Ok(snapshot)
}

/// # Errors
///
/// Returns an error if persisting the updated queue fails.
#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
#[tauri::command]
pub fn enqueue_job(
    app: AppHandle,
    state: State<'_, AppState>,
    source: String,
    duration_secs: f64,
    settings: EncoderKind,
    filters: FilterSpec,
    trim: Trim,
    audio_tracks: Vec<AudioTrackSelection>,
    subtitle_tracks: Vec<SubtitleTrackSelection>,
    video_info: Option<VideoInfo>,
) -> Result<Queue, String> {
    enqueue_job_impl(
        &app,
        &state,
        source,
        duration_secs,
        settings,
        filters,
        trim,
        audio_tracks,
        subtitle_tracks,
        video_info,
    )
}

/// Removes a `Queued`/`Completed`/`Failed`/`Cancelled` job. A `Running` job
/// must be cancelled first — there's no process to reap otherwise.
///
/// # Errors
///
/// Returns an error if no job has `job_id`, if it's currently running, or
/// if persisting the updated queue fails.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn remove_from_queue(state: State<'_, AppState>, job_id: &str) -> Result<Queue, String> {
    let mut queue = state.queue.lock().expect("queue mutex poisoned");
    let Some(index) = queue.jobs.iter().position(|j| j.id == job_id) else {
        return Err(format!("no such job: {job_id}"));
    };
    if matches!(queue.jobs[index].status, JobStatus::Running) {
        return Err("cancel the job before removing it".to_owned());
    }
    queue.jobs.remove(index);
    queue.save()?;
    Ok(queue.clone())
}

/// Swaps a `Queued` job one position up or down. Reordering a job that's
/// already running or finished isn't meaningful, so this only ever moves
/// among the queued ones — reordering past the queued/finished boundary
/// (e.g. moving a queued job above a completed one) is a silent no-op
/// rather than an error, since the *up*/*down* buttons only appear on
/// queued rows in the first place (§2.2 mockup) — a client sending a
/// nonsensical move isn't a real user action to report on.
///
/// # Errors
///
/// Returns an error if no job has `job_id`, `direction` isn't `"up"`/
/// `"down"`, or persisting the updated queue fails.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn move_job(
    state: State<'_, AppState>,
    job_id: &str,
    direction: &str,
) -> Result<Queue, String> {
    let mut queue = state.queue.lock().expect("queue mutex poisoned");
    let Some(index) = queue.jobs.iter().position(|j| j.id == job_id) else {
        return Err(format!("no such job: {job_id}"));
    };
    if !matches!(queue.jobs[index].status, JobStatus::Queued) {
        return Ok(queue.clone());
    }
    let target = match direction {
        "up" => index.checked_sub(1),
        "down" => (index + 1 < queue.jobs.len()).then_some(index + 1),
        other => return Err(format!("unknown move direction: {other}")),
    };
    if let Some(target) = target {
        if matches!(queue.jobs[target].status, JobStatus::Queued) {
            queue.jobs.swap(index, target);
            queue.save()?;
        }
    }
    Ok(queue.clone())
}

/// Cancels a job: if it's running, this kills the in-flight ffmpeg process
/// (asynchronously — the queue snapshot reflects the final `Cancelled`
/// status once the process actually exits, not immediately on this call
/// returning); if it's merely queued, it's marked `Cancelled` in place with
/// no process involved at all.
fn cancel_job_impl<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    job_id: &str,
) -> Result<Queue, String> {
    let is_running_job = {
        let mut running = state
            .running_cancel
            .lock()
            .expect("running_cancel mutex poisoned");
        if running.as_ref().is_some_and(|(id, _)| id == job_id) {
            if let Some((_, sender)) = running.take() {
                let _ = sender.send(());
            }
            true
        } else {
            false
        }
    };
    if !is_running_job {
        let mut queue = state.queue.lock().expect("queue mutex poisoned");
        if let Some(job) = queue.jobs.iter_mut().find(|j| j.id == job_id) {
            if matches!(job.status, JobStatus::Queued) {
                job.status = JobStatus::Cancelled;
            }
        }
        queue.save()?;
    }
    emit_snapshot(app, state);
    Ok(state.queue.lock().expect("queue mutex poisoned").clone())
}

/// # Errors
///
/// Returns an error if persisting the updated queue fails.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn cancel_job(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: String,
) -> Result<Queue, String> {
    cancel_job_impl(&app, &state, &job_id)
}

/// Stops dispatching *new* jobs once the current one finishes. This is
/// deliberately not "pause the current encode" — that would need OS-level
/// process suspension, which the legacy app's own docs warn doesn't work
/// for every encoder family (VCE/NVENC) — see notes/plan.md M9's scope
/// notes for why that's a separate, deferred capability.
///
/// # Errors
///
/// Returns an error if persisting the updated queue fails.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn pause_queue(app: AppHandle, state: State<'_, AppState>) -> Result<Queue, String> {
    let snapshot = {
        let mut queue = state.queue.lock().expect("queue mutex poisoned");
        queue.paused = true;
        queue.save()?;
        queue.clone()
    };
    emit_snapshot(&app, &state);
    Ok(snapshot)
}

/// # Errors
///
/// Returns an error if persisting the updated queue fails.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn resume_queue(app: AppHandle, state: State<'_, AppState>) -> Result<Queue, String> {
    let snapshot = {
        let mut queue = state.queue.lock().expect("queue mutex poisoned");
        queue.paused = false;
        queue.save()?;
        queue.clone()
    };
    // `dispatch_next` only emits a snapshot if it actually starts a job —
    // if the queue is empty or everything's already finished, resuming
    // still needs to be reflected (the `paused` flag itself changed).
    emit_snapshot(&app, &state);
    dispatch_next(app);
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::{Job, JobStatus, Queue, cancel_job_impl, dispatch_next, enqueue_job_impl};
    use crate::encoders::EncoderKind;
    use crate::encoders::x264::X264Settings;
    use crate::filters::FilterSpec;
    use crate::probe::VideoInfo;
    use crate::state::AppState;
    use std::sync::Mutex;
    use tauri::Manager;

    #[test]
    fn save_to_load_from_round_trips_and_resets_a_running_job_to_queued() {
        let dir =
            std::env::temp_dir().join(format!("orangeloaf-queue-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("failed to create scratch dir");
        let path = dir.join("queue.json");

        let mut queue = Queue::default();
        queue.jobs.push(Job {
            id: "job-1".to_owned(),
            source: "in.mkv".to_owned(),
            output: "out.mkv".to_owned(),
            duration_secs: 12.0,
            settings: EncoderKind::X264(X264Settings::default()),
            filters: FilterSpec::default(),
            trim: crate::encoders::Trim::default(),
            audio_tracks: Vec::new(),
            subtitle_tracks: Vec::new(),
            video_info: None,
            status: JobStatus::Running,
            started_at_unix_ms: None,
            finished_at_unix_ms: None,
            output_size_bytes: None,
        });
        queue.save_to(&path).expect("save should succeed");

        let loaded = Queue::load_from(&path);
        assert_eq!(loaded.jobs.len(), 1);
        assert!(
            matches!(loaded.jobs[0].status, JobStatus::Queued),
            "a Running job from a previous session should reset to Queued on load, got {:?}",
            loaded.jobs[0].status
        );
        assert_eq!(loaded.jobs[0].id, "job-1");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A pre-file-size-tracking `queue.json` has none of the three new
    /// timestamp/size fields on its jobs — deserializing it must still
    /// succeed (each field backfilling to `None` individually), not fail
    /// and fall back to `Queue::default()`, which would silently wipe
    /// every job an upgrading user had queued (see the fields' own doc
    /// comment on `Job`).
    #[test]
    fn deserializing_a_pre_file_size_tracking_job_backfills_none_not_an_empty_queue() {
        let json = r#"{"jobs": [{
            "id": "job-1",
            "source": "in.mkv",
            "output": "out.mkv",
            "durationSecs": 12.0,
            "settings": {"name": "x264", "preset": "medium", "tune": null, "profile": null, "crf": 23.0},
            "filters": {
                "deinterlace": false,
                "crop": null,
                "scaleWidth": null,
                "scaleHeight": null,
                "rotateDegrees": 0,
                "horizontalFlip": false,
                "verticalFlip": false
            },
            "trim": {"startTime": null, "endTime": null, "fastSeek": true},
            "audioTracks": [],
            "subtitleTracks": [],
            "videoInfo": null,
            "status": {"state": "completed"}
        }], "paused": false}"#;
        let queue: Queue =
            serde_json::from_str(json).expect("should deserialize with the new fields absent");
        assert_eq!(queue.jobs.len(), 1);
        assert_eq!(queue.jobs[0].started_at_unix_ms, None);
        assert_eq!(queue.jobs[0].finished_at_unix_ms, None);
        assert_eq!(queue.jobs[0].output_size_bytes, None);
    }

    fn test_job(source: &str, output: &str) -> Job {
        Job {
            id: uuid::Uuid::new_v4().to_string(),
            source: source.to_owned(),
            output: output.to_owned(),
            duration_secs: 2.0,
            settings: EncoderKind::X264(X264Settings {
                preset: "ultrafast".to_owned(),
                tune: None,
                profile: None,
                crf: 30.0,
            }),
            filters: FilterSpec::default(),
            trim: crate::encoders::Trim::default(),
            audio_tracks: Vec::new(),
            subtitle_tracks: Vec::new(),
            video_info: None,
            status: JobStatus::Queued,
            started_at_unix_ms: None,
            finished_at_unix_ms: None,
            output_size_bytes: None,
        }
    }

    fn mock_state() -> AppState {
        AppState {
            config: Mutex::new(crate::config::Config::default()),
            queue: Mutex::new(Queue::default()),
            running_cancel: Mutex::new(None),
            profiles: Mutex::new(crate::profiles::Profiles::default()),
            history: Mutex::new(crate::history::History::default()),
        }
    }

    /// Runs two real encodes through the actual runner (`dispatch_next` ->
    /// `job::run_cancelable` -> real ffmpeg), using `tauri::test::mock_app`
    /// so `AppState`/`AppHandle` work exactly as they do in the real app —
    /// no disk I/O (queue built in-memory, bypassing `Queue::load`/`save`'s
    /// real file paths entirely) and no mocked encoder logic. Verifies the
    /// property that actually matters for a *queue* rather than a single
    /// job: jobs run one at a time, in order, and both produce real output.
    #[test]
    #[ignore = "requires system ffmpeg"]
    #[allow(clippy::too_many_lines)]
    fn real_queue_runs_two_jobs_sequentially_and_produces_real_output_files() {
        let app = tauri::test::mock_app();
        app.manage(mock_state());
        let handle = app.handle().clone();

        let output1 = std::env::temp_dir()
            .join("orangeloaf-queue-test-1.mkv")
            .display()
            .to_string();
        let output2 = std::env::temp_dir()
            .join("orangeloaf-queue-test-2.mkv")
            .display()
            .to_string();
        let _ = std::fs::remove_file(&output1);
        let _ = std::fs::remove_file(&output2);

        {
            let state = handle.state::<AppState>();
            let mut queue = state.queue.lock().expect("queue mutex poisoned");
            queue
                .jobs
                .push(test_job("tests/fixtures/sdr_8bit.mp4", &output1));
            queue
                .jobs
                .push(test_job("tests/fixtures/sdr_8bit.mp4", &output2));
        }

        dispatch_next(handle.clone());

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let (statuses, running_count) = {
                let state = handle.state::<AppState>();
                let queue = state.queue.lock().expect("queue mutex poisoned");
                let statuses: Vec<String> = queue
                    .jobs
                    .iter()
                    .map(|j| format!("{:?}", j.status))
                    .collect();
                let running_count = queue
                    .jobs
                    .iter()
                    .filter(|j| matches!(j.status, JobStatus::Running))
                    .count();
                (statuses, running_count)
            };
            assert!(
                running_count <= 1,
                "more than one job was Running at once: {statuses:?}"
            );
            let all_done = statuses
                .iter()
                .all(|s| s != "Queued" && !s.starts_with("Running"));
            if all_done {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "queue did not finish within 30s, last statuses: {statuses:?}"
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        let state = handle.state::<AppState>();
        {
            let queue = state.queue.lock().expect("queue mutex poisoned");
            for job in &queue.jobs {
                assert!(
                    matches!(job.status, JobStatus::Completed),
                    "job {} ended as {:?}",
                    job.id,
                    job.status
                );
                // Real values, not just "some Some(_)" — this is what
                // actually powers the Queue screen's completed-row
                // size/duration display (notes/plan.md's UI-gap audit).
                let started = job
                    .started_at_unix_ms
                    .expect("a completed job should have recorded when it started");
                let finished = job
                    .finished_at_unix_ms
                    .expect("a completed job should have recorded when it finished");
                assert!(
                    finished >= started,
                    "job {} finished ({finished}) before it started ({started})",
                    job.id
                );
                let real_size = std::fs::metadata(&job.output)
                    .expect("output file should exist")
                    .len();
                assert_eq!(
                    job.output_size_bytes,
                    Some(real_size),
                    "job {}'s recorded size should match its real output file",
                    job.id
                );
            }
        }
        assert!(std::path::Path::new(&output1).exists());
        assert!(std::path::Path::new(&output2).exists());

        // Both completions should have gone through `finish_job` ->
        // `history::record` — verify the actual recorded state, not just
        // that the queue itself progressed (a silently-failed history hook
        // wouldn't otherwise be caught by this test at all). `history` is a
        // *separate* mutex from `queue`, updated a moment after the queue
        // status itself (see `finish_job`), so this needs its own short
        // poll rather than assuming it's already settled the instant the
        // queue reports both jobs done.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let recorded = state
                .history
                .lock()
                .expect("history mutex poisoned")
                .entries
                .len();
            if recorded == 2 {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "expected 2 history entries within 5s of queue completion, got {recorded}"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        let history = state.history.lock().expect("history mutex poisoned");
        assert!(
            history
                .entries
                .iter()
                .all(|e| matches!(e.status, crate::history::HistoryStatus::Completed))
        );
        assert!(
            history
                .entries
                .iter()
                .all(|e| e.source == "tests/fixtures/sdr_8bit.mp4")
        );

        let _ = std::fs::remove_file(&output1);
        let _ = std::fs::remove_file(&output2);
    }

    /// Encodes a real, deliberately slow-to-encode source (1280x720, 8s,
    /// x265 `veryslow`) so there's a reliable multi-second window to cancel
    /// it mid-flight, then asserts three things a mocked-out test couldn't:
    /// the real ffmpeg process actually dies (cancellation returns well
    /// before the ~8s natural runtime), the job's final status is
    /// `Cancelled` (not stuck `Running` or misreported `Failed`), and the
    /// queue automatically moves on to the next job afterward.
    #[test]
    #[ignore = "requires system ffmpeg, and takes a few real seconds"]
    #[allow(clippy::too_many_lines)]
    fn real_cancel_kills_the_running_process_and_the_queue_moves_on() {
        let source = std::env::temp_dir().join(format!(
            "orangeloaf-cancel-src-{}.mkv",
            uuid::Uuid::new_v4()
        ));
        let generated = std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=1280x720:rate=30:duration=8",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
            ])
            .arg(&source)
            .output()
            .expect("failed to generate the slow-to-encode source");
        assert!(
            generated.status.success(),
            "failed to generate source: {}",
            String::from_utf8_lossy(&generated.stderr)
        );

        let app = tauri::test::mock_app();
        app.manage(mock_state());
        let handle = app.handle().clone();

        let slow_output = std::env::temp_dir()
            .join("orangeloaf-queue-cancel-test.mkv")
            .display()
            .to_string();
        let next_output = std::env::temp_dir()
            .join("orangeloaf-queue-cancel-test-next.mkv")
            .display()
            .to_string();
        let _ = std::fs::remove_file(&slow_output);
        let _ = std::fs::remove_file(&next_output);

        let slow_job = Job {
            settings: EncoderKind::X265(crate::encoders::x265::X265Settings {
                preset: "veryslow".to_owned(),
                ..crate::encoders::x265::X265Settings::default()
            }),
            duration_secs: 8.0,
            ..test_job(&source.display().to_string(), &slow_output)
        };
        let slow_job_id = slow_job.id.clone();
        let next_job = test_job("tests/fixtures/sdr_8bit.mp4", &next_output);

        {
            let state = handle.state::<AppState>();
            let mut queue = state.queue.lock().expect("queue mutex poisoned");
            queue.jobs.push(slow_job);
            queue.jobs.push(next_job);
        }

        dispatch_next(handle.clone());

        // Give ffmpeg a real moment to actually start before cancelling —
        // cancelling before it's even spawned wouldn't exercise the kill
        // path at all.
        std::thread::sleep(std::time::Duration::from_millis(500));

        let cancel_started = std::time::Instant::now();
        cancel_job_impl(&handle, &handle.state::<AppState>(), &slow_job_id)
            .expect("cancel_job_impl should succeed");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            let status = {
                let state = handle.state::<AppState>();
                let queue = state.queue.lock().expect("queue mutex poisoned");
                queue
                    .jobs
                    .iter()
                    .find(|j| j.id == slow_job_id)
                    .map(|j| format!("{:?}", j.status))
            };
            if status.as_deref().is_some_and(|s| s != "Running") {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "cancelled job never left Running, last status: {status:?}"
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let cancel_elapsed = cancel_started.elapsed();
        assert!(
            cancel_elapsed < std::time::Duration::from_secs(6),
            "cancellation took {cancel_elapsed:?} — too close to the source's 8s natural \
             duration to be confident the process was actually killed rather than left to finish"
        );

        {
            let state = handle.state::<AppState>();
            let queue = state.queue.lock().expect("queue mutex poisoned");
            let slow = queue
                .jobs
                .iter()
                .find(|j| j.id == slow_job_id)
                .expect("slow job should still be in the queue");
            assert!(
                matches!(slow.status, JobStatus::Cancelled),
                "expected Cancelled, got {:?}",
                slow.status
            );
        }

        // The queue should have moved on to the next job on its own.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let done = {
                let state = handle.state::<AppState>();
                let queue = state.queue.lock().expect("queue mutex poisoned");
                queue.jobs.iter().any(|j| {
                    j.source == "tests/fixtures/sdr_8bit.mp4"
                        && matches!(j.status, JobStatus::Completed)
                })
            };
            if done {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "queue did not move on to the next job after cancellation"
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        let _ = std::fs::remove_file(&source);
        let _ = std::fs::remove_file(&slow_output);
        let _ = std::fs::remove_file(&next_output);
    }

    /// Exercises the naming-template system (notes/plan.md M12) through the
    /// real `enqueue_job` command and a real `dispatch_next` run, not just
    /// `naming::resolve_pre_encode`/`resolve_post_encode` in isolation —
    /// this is what actually proves the two phases are wired together
    /// correctly end to end: `enqueue_job` must resolve pre-encode
    /// variables into the job's initial `output` (with any post-encode
    /// variable left as a literal placeholder token), and once the real
    /// encode finishes, `dispatch_next` must rename the real file *and*
    /// update the queue's own `job.output` to match — a mismatch between
    /// the two would leave "Open Containing Folder"/History pointing at a
    /// file that no longer exists under that name.
    #[test]
    #[ignore = "requires system ffmpeg"]
    fn real_enqueue_resolves_naming_template_pre_and_post_encode() {
        let app = tauri::test::mock_app();
        let mut state = mock_state();
        state.config = Mutex::new(crate::config::Config {
            output_naming_template:
                "orangeloaf-naming-it-{source}-{encoder}-{source_resolution}-{filesize}".to_owned(),
            ..crate::config::Config::default()
        });
        app.manage(state);
        let handle = app.handle().clone();

        let video_info = VideoInfo {
            duration_secs: 2.0,
            width: 320,
            height: 240,
            codec_name: "h264".to_owned(),
            pix_fmt: Some("yuv420p".to_owned()),
            bit_depth: 8,
            frame_rate: Some(25.0),
            bitrate_bps: None,
            color_space: None,
            color_primaries: None,
            color_transfer: None,
            hdr10: None,
            audio_tracks: Vec::new(),
            subtitle_tracks: Vec::new(),
        };

        let queue_after_enqueue = enqueue_job_impl(
            &handle,
            &handle.state::<AppState>(),
            "tests/fixtures/sdr_8bit.mp4".to_owned(),
            2.0,
            EncoderKind::X264(X264Settings {
                preset: "ultrafast".to_owned(),
                tune: None,
                profile: None,
                crf: 30.0,
            }),
            FilterSpec::default(),
            crate::encoders::Trim::default(),
            Vec::new(),
            Vec::new(),
            Some(video_info),
        )
        .expect("enqueue should succeed");

        let job_id = queue_after_enqueue.jobs[0].id.clone();
        let pre_encode_output = queue_after_enqueue.jobs[0].output.clone();
        assert!(
            pre_encode_output.contains("orangeloaf-naming-it-sdr_8bit-x264-320x240-FFFSIZE"),
            "pre-encode resolution didn't produce the expected stem: {pre_encode_output}"
        );

        dispatch_next(handle.clone());

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let final_output = loop {
            let state = handle.state::<AppState>();
            let queue = state.queue.lock().expect("queue mutex poisoned");
            let job = queue
                .jobs
                .iter()
                .find(|j| j.id == job_id)
                .expect("job should still be in the queue");
            match &job.status {
                JobStatus::Completed => break job.output.clone(),
                JobStatus::Failed { message } => panic!("job failed: {message}"),
                _ => {}
            }
            assert!(
                std::time::Instant::now() < deadline,
                "job did not finish within 30s"
            );
            drop(queue);
            std::thread::sleep(std::time::Duration::from_millis(50));
        };

        assert!(
            !final_output.contains("FFFSIZE"),
            "post-encode placeholder was never resolved: {final_output}"
        );
        assert!(
            std::path::Path::new(&final_output).exists(),
            "renamed output file should exist at {final_output}"
        );

        let _ = std::fs::remove_file(&final_output);
    }
}
