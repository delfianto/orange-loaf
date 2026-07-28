use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Persisted user configuration.
///
/// This is intentionally small right now — only the fields the app actually
/// uses today. Later milestones (profiles, history, output naming, ...) add
/// their own fields as those features land, rather than pre-declaring an
/// unused schema now.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub ffmpeg_path: Option<PathBuf>,
    pub ffprobe_path: Option<PathBuf>,
}

impl Config {
    /// Resolves where the config file lives, honoring `VOLTAIC_CONFIG` if set.
    pub fn file_path() -> Option<PathBuf> {
        if let Ok(path) = std::env::var("VOLTAIC_CONFIG") {
            return Some(PathBuf::from(path));
        }
        directories::ProjectDirs::from("dev", "voltaic", "Voltaic")
            .map(|dirs| dirs.config_dir().join("config.json"))
    }

    /// Loads the config from disk, falling back to defaults if it doesn't
    /// exist yet or fails to parse.
    pub fn load() -> Self {
        Self::file_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|contents| serde_json::from_str(&contents).ok())
            .unwrap_or_default()
    }

    /// Writes the config to disk, creating the parent directory if needed.
    ///
    /// # Errors
    ///
    /// Returns an error if the config directory can't be determined, or if
    /// creating the directory / writing the file fails.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::file_path().ok_or("could not determine the config file location")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let contents = serde_json::to_string_pretty(self).map_err(|err| err.to_string())?;
        std::fs::write(path, contents).map_err(|err| err.to_string())
    }
}
