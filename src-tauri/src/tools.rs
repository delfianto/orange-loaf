use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

use crate::config::Config;

/// Resolves the ffmpeg binary to actually invoke, or an error message
/// suitable for surfacing to the user if it can't be found.
///
/// # Errors
///
/// Returns an error if ffmpeg can't be located via configured path, env
/// override, or `PATH`.
pub fn ffmpeg_path(config: &Config) -> Result<PathBuf, String> {
    locate_binary("ffmpeg", "FFMPEG", config.ffmpeg_path.as_ref())
        .ok_or_else(|| "FFmpeg not found — set its path in Settings".to_owned())
}

/// Resolves the ffprobe binary to actually invoke, or an error message
/// suitable for surfacing to the user if it can't be found.
///
/// # Errors
///
/// Returns an error if ffprobe can't be located via configured path, env
/// override, or `PATH`.
pub fn ffprobe_path(config: &Config) -> Result<PathBuf, String> {
    locate_binary("ffprobe", "FFPROBE", config.ffprobe_path.as_ref())
        .ok_or_else(|| "FFprobe not found — set its path in Settings".to_owned())
}

/// Detection result for a single external tool (ffmpeg, ffprobe, ...).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub name: String,
    pub found: bool,
    pub path: Option<String>,
    pub version: Option<String>,
}

/// Picks a binary path by, in order: an explicit user-configured path (if it
/// still exists), an environment-variable override (if it still exists),
/// then a `PATH` lookup by `binary_name`.
///
/// Split out from [`locate_binary`] as a pure function (env value passed in,
/// not read here) so the priority order is unit-testable without mutating
/// real process environment variables.
fn resolve_binary(
    binary_name: &str,
    configured: Option<&PathBuf>,
    env_override: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(path) = configured
        && path.is_file()
    {
        return Some(path.clone());
    }
    if let Some(path) = env_override
        && path.is_file()
    {
        return Some(path.to_path_buf());
    }
    which::which(binary_name).ok()
}

/// Locates a binary by, in order: an explicit user-configured path, a
/// `ORANGELOAF_<ENV_SUFFIX>` environment variable override, then a `PATH` lookup.
///
/// Shared by tool-status detection (this module) and anything that needs to
/// actually invoke the binary (probing, thumbnailing, encoding, ...).
pub fn locate_binary(
    binary_name: &str,
    env_suffix: &str,
    configured: Option<&PathBuf>,
) -> Option<PathBuf> {
    let env_override = std::env::var(format!("ORANGELOAF_{env_suffix}"))
        .ok()
        .map(PathBuf::from);
    resolve_binary(binary_name, configured, env_override.as_deref())
}

/// Runs `<path> -version` and returns the first line of output, which for
/// both ffmpeg and ffprobe is a one-line self-identification string.
fn probe_version(path: &Path) -> Option<String> {
    let output = Command::new(path).arg("-version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(str::to_owned)
}

/// Detects a single tool's location and version.
pub fn detect_tool(
    display_name: &str,
    binary_name: &str,
    env_suffix: &str,
    configured: Option<&PathBuf>,
) -> ToolStatus {
    let path = locate_binary(binary_name, env_suffix, configured);
    let version = path.as_deref().and_then(probe_version);
    ToolStatus {
        name: display_name.to_owned(),
        found: path.is_some(),
        path: path.map(|p| p.display().to_string()),
        version,
    }
}

/// Whether `ffmpeg`'s `h264_nvenc` encoder actually works right now — not
/// just whether ffmpeg was *compiled* with NVENC support (which
/// `-encoders`/`-hide_banner` would tell you, but says nothing about
/// whether a real NVIDIA GPU + driver capable of using it are actually
/// present). Deliberately checked the same way `detect_tool` checks
/// ffmpeg/ffprobe themselves: by really invoking it, not by asking it to
/// self-report — a 1-frame synthetic encode is cheap enough to run on
/// every check and is the only way to know NVENC will actually work.
///
/// NVENC is ffmpeg's own built-in hardware encoder, distinct from (and far
/// simpler than) the rigaya `NVEncC.exe` family the prior-art research
/// covers (notes/plan.md §1.3) — that family is Windows-only external
/// binaries and stays out of scope for the reasons already recorded for
/// M5/M12; this is plain cross-platform `ffmpeg -c:v h264_nvenc`, which
/// works identically on Linux (the actual target platform for hardware
/// encoding in this project — see M14's Linux-only scope note) and
/// Windows alike, given a real NVIDIA GPU.
#[must_use]
pub fn detect_nvenc(ffmpeg: &Path) -> bool {
    Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=black:size=64x64:duration=0.1",
            "-frames:v",
            "1",
            "-c:v",
            "h264_nvenc",
            "-f",
            "null",
            "-",
        ])
        .output()
        .is_ok_and(|output| output.status.success())
}

#[cfg(test)]
mod tests {
    use super::resolve_binary;
    use std::path::PathBuf;

    /// Creates an empty file at `dir/name` and returns its path. `resolve_binary`
    /// only ever checks `is_file()`, so the file never needs real content.
    fn touch(dir: &std::path::Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"").expect("failed to create test fixture file");
        path
    }

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "orangeloaf-tools-test-{label}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("failed to create temp test dir");
        dir
    }

    #[test]
    fn configured_path_wins_when_it_exists() {
        let dir = temp_dir("configured-wins");
        let configured = touch(&dir, "configured-tool");
        let env_override = touch(&dir, "env-tool");

        let found = resolve_binary(
            "definitely-not-a-real-binary",
            Some(&configured),
            Some(&env_override),
        );

        assert_eq!(found, Some(configured));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn env_override_wins_when_configured_path_is_missing() {
        let dir = temp_dir("env-wins");
        let missing_configured = dir.join("does-not-exist");
        let env_override = touch(&dir, "env-tool");

        let found = resolve_binary(
            "definitely-not-a-real-binary",
            Some(&missing_configured),
            Some(&env_override),
        );

        assert_eq!(found, Some(env_override));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn falls_through_to_path_lookup_and_returns_none_for_unknown_binary() {
        let found = resolve_binary("definitely-not-a-real-binary-orangeloaf-test", None, None);

        assert_eq!(found, None);
    }

    /// Real, not mocked: this dev machine has no NVIDIA GPU, so a genuine
    /// `h264_nvenc` encode attempt must fail and `detect_nvenc` must report
    /// `false` — confirms the negative path actually works rather than
    /// assuming it would. (The positive path — a real NVENC-capable
    /// machine reporting `true` — isn't verifiable here for the same
    /// reason; see notes/plan.md's scope note on this.)
    #[test]
    #[ignore = "requires system ffmpeg"]
    fn detect_nvenc_is_false_on_a_machine_with_no_nvidia_gpu() {
        assert!(!super::detect_nvenc(std::path::Path::new("ffmpeg")));
    }
}
