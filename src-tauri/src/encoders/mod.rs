pub mod common;
pub mod svt_av1;
pub mod tracks;
pub mod videotoolbox;
pub mod x264;
pub mod x265;

use serde::{Deserialize, Serialize};

use crate::filters::FilterSpec;
use svt_av1::SvtAv1Settings;
use tracks::{AudioTrackSelection, SubtitleTrackSelection};
use videotoolbox::{H264VideoToolboxSettings, HevcVideoToolboxSettings};
use x264::X264Settings;
use x265::X265Settings;

/// Everything a specific encoder's [`EncoderProfile::build_steps`] needs to
/// know about the job that isn't its own codec settings.
pub struct EncodeContext {
    pub source: String,
    pub output: String,
    pub filters: FilterSpec,
    /// Trim range in source seconds; `None` means "from the start"/"to the end".
    pub start_time: Option<f64>,
    pub end_time: Option<f64>,
    /// Whether trim seeks fast (keyframe-snapping, `-ss` before `-i`) or exact
    /// (`-ss`/`-to` after `-i`) — see [`common::build_prefix`].
    pub fast_seek: bool,
    /// Empty means "copy every audio/subtitle track" (see [`tracks`]) — the
    /// same default as before per-track selection existed at all.
    pub audio_tracks: Vec<AudioTrackSelection>,
    pub subtitle_tracks: Vec<SubtitleTrackSelection>,
}

impl EncodeContext {
    /// Builds a context with no trim, fast-seek on, and every audio/
    /// subtitle track copied (the common case, and nearly all call sites) —
    /// override individual fields with struct-update syntax
    /// (`..EncodeContext::new(...)`) for the rest.
    pub fn new(source: impl Into<String>, output: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            output: output.into(),
            filters: FilterSpec::default(),
            start_time: None,
            end_time: None,
            fast_seek: true,
            audio_tracks: Vec::new(),
            subtitle_tracks: Vec::new(),
        }
    }

    /// Applies a [`Trim`] received over IPC to this context.
    pub fn with_trim(mut self, trim: Trim) -> Self {
        self.start_time = trim.start_time;
        self.end_time = trim.end_time;
        self.fast_seek = trim.fast_seek;
        self
    }
}

/// The trim range for a job, as received from the frontend — kept as its
/// own IPC-facing type (rather than passing three loose params) for the
/// same reason [`FilterSpec`] is: one clearly-named thing to bind a form to.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trim {
    pub start_time: Option<f64>,
    pub end_time: Option<f64>,
    pub fast_seek: bool,
}

impl Default for Trim {
    fn default() -> Self {
        Self {
            start_time: None,
            end_time: None,
            fast_seek: true,
        }
    }
}

/// One command to actually run. Currently always a plain argv list — the
/// `gifski`-style real-pipe variant (see notes/plan.md §4.3) joins this enum
/// when a hybrid-pipe encoder is implemented; until then there's nothing to
/// pipe into, so it isn't modeled yet.
pub enum EncodeStep {
    Argv(Vec<String>),
}

/// Implemented by every encoder's settings type. Mirrors the prior-art
/// research's `build(fastflix) -> List[Command]` shape (notes/plan.md §1.3):
/// a strategy per encoder that turns its own settings plus shared context
/// into an ordered list of steps (more than one for e.g. future two-pass
/// support).
pub trait EncoderProfile {
    fn build_steps(&self, ctx: &EncodeContext) -> Vec<EncodeStep>;
}

/// Every encoder Voltaic currently supports, as a tagged union rather than
/// ~30 `Option<XSettings>` sibling fields — see notes/plan.md §4.4 on why
/// the prior art's untagged-union-of-options shape needed workarounds
/// (reverse-lookup-by-name, reassign-after-construction) that a tagged enum
/// avoids by construction. `name` matches the frontend's encoder id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "name", rename_all = "camelCase")]
pub enum EncoderKind {
    X265(X265Settings),
    X264(X264Settings),
    SvtAv1(SvtAv1Settings),
    H264Videotoolbox(H264VideoToolboxSettings),
    HevcVideotoolbox(HevcVideoToolboxSettings),
}

impl EncoderProfile for EncoderKind {
    fn build_steps(&self, ctx: &EncodeContext) -> Vec<EncodeStep> {
        match self {
            Self::X265(settings) => settings.build_steps(ctx),
            Self::X264(settings) => settings.build_steps(ctx),
            Self::SvtAv1(settings) => settings.build_steps(ctx),
            Self::H264Videotoolbox(settings) => settings.build_steps(ctx),
            Self::HevcVideotoolbox(settings) => settings.build_steps(ctx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EncoderKind;
    use super::svt_av1::SvtAv1Settings;
    use super::videotoolbox::{H264VideoToolboxSettings, HevcVideoToolboxSettings};
    use super::x264::X264Settings;
    use super::x265::X265Settings;

    /// Pins the exact wire shape the frontend's TS union must match —
    /// written once here instead of guessed twice (in Rust and in TS).
    #[test]
    fn tagged_enum_serializes_with_the_expected_camel_case_tag_values() {
        assert_eq!(
            serde_json::to_value(EncoderKind::X265(X265Settings::default())).unwrap()["name"],
            "x265"
        );
        assert_eq!(
            serde_json::to_value(EncoderKind::X264(X264Settings::default())).unwrap()["name"],
            "x264"
        );
        assert_eq!(
            serde_json::to_value(EncoderKind::SvtAv1(SvtAv1Settings::default())).unwrap()["name"],
            "svtAv1"
        );
        assert_eq!(
            serde_json::to_value(EncoderKind::H264Videotoolbox(
                H264VideoToolboxSettings::default()
            ))
            .unwrap()["name"],
            "h264Videotoolbox"
        );
        assert_eq!(
            serde_json::to_value(EncoderKind::HevcVideotoolbox(
                HevcVideoToolboxSettings::default()
            ))
            .unwrap()["name"],
            "hevcVideotoolbox"
        );
    }

    #[test]
    fn tag_and_settings_fields_are_flattened_at_the_same_level() {
        let value = serde_json::to_value(EncoderKind::X265(X265Settings::default())).unwrap();
        assert_eq!(value["preset"], "medium");
        assert_eq!(value["crf"], 22.0);
    }
}
