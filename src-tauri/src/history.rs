use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::encoders::EncoderKind;
use crate::filters::FilterSpec;
use crate::state::AppState;
use crate::{thumbnail, tools};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum HistoryStatus {
    Completed,
    Failed { message: String },
}

/// A completed-or-failed job's full settings, not just a display summary —
/// this is what makes "Apply" (re-using an old job's exact encoder
/// settings) possible at all (notes/plan.md §4.10). `source`/`output` are
/// kept for *display* only, never re-applied — see this module's `apply`
/// doc comment for why.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub source: String,
    pub output: String,
    pub settings: EncoderKind,
    pub filters: FilterSpec,
    pub completed_at_unix_ms: u64,
    pub status: HistoryStatus,
    /// Best-effort; `None` if the source is no longer readable by the time
    /// the thumbnail is generated (e.g. it lived on removable media).
    pub thumbnail: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct History {
    pub entries: Vec<HistoryEntry>,
}

/// Keeps the persisted file (and the in-memory list) from growing without
/// bound across months of use — old entries are the least useful ones.
const MAX_ENTRIES: usize = 200;

impl History {
    pub fn file_path() -> Option<PathBuf> {
        if let Ok(path) = std::env::var("VOLTAIC_HISTORY") {
            return Some(PathBuf::from(path));
        }
        directories::ProjectDirs::from("dev", "voltaic", "Voltaic")
            .map(|dirs| dirs.data_dir().join("history.json"))
    }

    pub fn load() -> Self {
        Self::file_path().map_or_else(Self::default, |path| Self::load_from(&path))
    }

    fn load_from(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|contents| serde_json::from_str(&contents).ok())
            .unwrap_or_default()
    }

    /// # Errors
    ///
    /// Returns an error if the history file location can't be determined,
    /// or if writing fails.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::file_path().ok_or("could not determine the history file location")?;
        self.save_to(&path)
    }

    fn save_to(&self, path: &std::path::Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let contents = serde_json::to_string_pretty(self).map_err(|err| err.to_string())?;
        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, &contents).map_err(|err| err.to_string())?;
        std::fs::rename(&tmp_path, path).map_err(|err| err.to_string())
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// Records one finished job (queue-sourced only for now — see this
/// milestone's scope note in notes/plan.md M11) and emits the updated list.
/// A thumbnail is generated best-effort at `thumbnail_at_secs` (the job's
/// trim start, if any) — its failure doesn't block recording the entry.
#[allow(clippy::too_many_arguments)]
pub fn record<R: Runtime>(
    app: &AppHandle<R>,
    source: String,
    output: String,
    settings: EncoderKind,
    filters: FilterSpec,
    status: HistoryStatus,
    thumbnail_at_secs: f64,
) {
    let state = app.state::<AppState>();
    let thumbnail = {
        let config = state.config.lock().expect("config mutex poisoned");
        tools::ffmpeg_path(&config).ok()
    }
    .and_then(|ffmpeg| thumbnail::generate_thumbnail(&ffmpeg, &source, thumbnail_at_secs).ok());

    let mut history = state.history.lock().expect("history mutex poisoned");
    history.entries.insert(
        0,
        HistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            source,
            output,
            settings,
            filters,
            completed_at_unix_ms: now_unix_ms(),
            status,
            thumbnail,
        },
    );
    history.entries.truncate(MAX_ENTRIES);
    let _ = history.save();
    let _ = app.emit("history:snapshot", &*history);
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn list_history(state: State<'_, AppState>) -> History {
    state
        .history
        .lock()
        .expect("history mutex poisoned")
        .clone()
}

/// # Errors
///
/// Returns an error if persisting the updated history fails.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn delete_history_entry(state: State<'_, AppState>, id: &str) -> Result<History, String> {
    let mut history = state.history.lock().expect("history mutex poisoned");
    history.entries.retain(|e| e.id != id);
    history.save()?;
    Ok(history.clone())
}

/// # Errors
///
/// Returns an error if persisting the updated history fails.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn clear_history(state: State<'_, AppState>) -> Result<History, String> {
    let mut history = state.history.lock().expect("history mutex poisoned");
    history.entries.clear();
    history.save()?;
    Ok(history.clone())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedHistoryEntry {
    pub settings: EncoderKind,
    pub filters: FilterSpec,
}

/// Deliberately only transfers encoder settings + filters — not trim, not
/// audio/subtitle track selections, and not the source/output paths. Trim
/// points and track-index selections are meaningless (or actively wrong)
/// applied to a different video than the one they were configured against:
/// a trim range valid for a 2-hour source is nonsensical on a 5-minute one,
/// and "track index 2" can refer to a completely different language on a
/// video with a different track layout. This is the same reasoning that
/// makes `Profile` apply re-evaluate audio *rules* against the new source's
/// probed tracks rather than replaying old literal selections — history
/// has no rules to re-evaluate, so those fields just don't transfer at all.
///
/// # Errors
///
/// Returns an error if no entry has `id`.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn apply_history_entry(
    state: State<'_, AppState>,
    id: &str,
) -> Result<AppliedHistoryEntry, String> {
    let history = state.history.lock().expect("history mutex poisoned");
    let entry = history
        .entries
        .iter()
        .find(|e| e.id == id)
        .ok_or_else(|| format!("no such history entry: {id}"))?;
    Ok(AppliedHistoryEntry {
        settings: entry.settings.clone(),
        filters: entry.filters.clone(),
    })
}
