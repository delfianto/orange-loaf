use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The default output naming template — a close match for the placeholder
/// `derive_output_path` behavior this replaces (notes/plan.md M12), just
/// expressed as a real, user-editable template instead of a hardcoded
/// suffix.
pub(crate) fn default_naming_template() -> String {
    "{source}-{encoder}".to_owned()
}

/// DaisyUI/Tailwind spacing scale to apply across list-heavy views (Queue,
/// History) — a real, if small, visual difference (see `app.css`), not a
/// no-op preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UiDensity {
    #[default]
    Comfortable,
    Compact,
}

/// Persisted only for now — no logging framework exists anywhere in this
/// codebase yet (there's nothing today that would honor this value), the
/// same "can't reference a setting with no other way to apply it" situation
/// M10's scope note describes for `AdvancedOptions`. Kept as a real Settings
/// control anyway since §5.2 calls for one, ready for a real logging setup
/// to consume later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

/// Persisted user configuration.
///
/// Grows one field at a time as the milestone that needs it lands, rather
/// than pre-declaring an unused schema up front.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub ffmpeg_path: Option<PathBuf>,
    pub ffprobe_path: Option<PathBuf>,
    #[serde(default = "default_naming_template")]
    pub output_naming_template: String,
    /// A *deny* list, not an allow list, deliberately: an empty list (the
    /// default) means "every codec `AUDIO_CODECS` knows about is enabled"
    /// without this Rust struct needing its own copy of that frontend
    /// table to default against — see notes/plan.md M12 scope note.
    pub disabled_audio_codecs: Vec<String>,
    pub ui_density: UiDensity,
    /// A plain persisted tag today — retranslating the UI from it is M13's
    /// job (notes/plan.md), not this one's.
    pub language: String,
    /// No update-checker exists yet anywhere in this app; this is a real,
    /// persisted preference with nothing behind it yet, same as
    /// `log_level`.
    pub check_for_updates: bool,
    pub log_level: LogLevel,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ffmpeg_path: None,
            ffprobe_path: None,
            output_naming_template: default_naming_template(),
            disabled_audio_codecs: Vec::new(),
            ui_density: UiDensity::default(),
            language: "en".to_owned(),
            check_for_updates: true,
            log_level: LogLevel::default(),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::{Config, LogLevel, UiDensity};

    /// An M1-era `config.json` (before M12 added these fields) has none of
    /// them — loading it must backfill the real defaults, not empty
    /// strings/zero values, or every upgrading user's naming template
    /// would silently become blank.
    #[test]
    fn deserializing_a_pre_m12_config_backfills_real_defaults_not_zero_values() {
        let config: Config = serde_json::from_str(r#"{"ffmpegPath": "/usr/bin/ffmpeg"}"#)
            .expect("should deserialize with only the pre-M12 fields present");
        assert_eq!(config.output_naming_template, "{source}-{encoder}");
        assert_eq!(config.disabled_audio_codecs, Vec::<String>::new());
        assert_eq!(config.ui_density, UiDensity::Comfortable);
        assert_eq!(config.language, "en");
        assert!(config.check_for_updates);
        assert_eq!(config.log_level, LogLevel::Info);
    }

    #[test]
    fn ui_density_and_log_level_serialize_as_bare_camel_case_strings() {
        assert_eq!(serde_json::to_value(UiDensity::Compact).unwrap(), "compact");
        assert_eq!(serde_json::to_value(LogLevel::Warn).unwrap(), "warn");
    }
}
