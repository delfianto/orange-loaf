pub mod common;
pub mod hdr;
pub mod svt_av1;
pub mod tracks;
pub mod videotoolbox;
pub mod x264;
pub mod x265;

use serde::{Deserialize, Serialize};

use crate::filters::FilterSpec;
use crate::probe::Hdr10Metadata;
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
    /// The source's own HDR10 metadata (from `probe_video`), carried through
    /// automatically when the target encoder supports it (see [`hdr`]) —
    /// there's no separate "preserve HDR" toggle yet, so any source that has
    /// this gets it re-injected on encode.
    pub hdr10: Option<Hdr10Metadata>,
}

impl EncodeContext {
    /// Builds a context with no trim, fast-seek on, every audio/subtitle
    /// track copied, and no HDR metadata (the common case, and nearly all
    /// call sites) — override individual fields with struct-update syntax
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
            hdr10: None,
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

/// Every encoder `OrangeLoaf` currently supports, as a tagged union rather than
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

impl EncoderKind {
    /// Short id — matches both this enum's own serde tag and the frontend's
    /// `EncoderName`. Used by the naming template's `{encoder}` variable
    /// (notes/plan.md M12).
    #[must_use]
    pub fn encoder_id(&self) -> &'static str {
        match self {
            Self::X265(_) => "x265",
            Self::X264(_) => "x264",
            Self::SvtAv1(_) => "svtAv1",
            Self::H264Videotoolbox(_) => "h264Videotoolbox",
            Self::HevcVideotoolbox(_) => "hevcVideotoolbox",
        }
    }

    /// Human label — matches the frontend's `ENCODER_NAMES` table. Used by
    /// the naming template's `{codec}` variable ("full codec name").
    #[must_use]
    pub fn display_label(&self) -> &'static str {
        match self {
            Self::X265(_) => "HEVC (x265)",
            Self::X264(_) => "AVC (x264)",
            Self::SvtAv1(_) => "AV1 (SVT-AV1)",
            Self::H264Videotoolbox(_) => "H.264 (VideoToolbox)",
            Self::HevcVideotoolbox(_) => "HEVC (VideoToolbox)",
        }
    }

    /// `None` for encoders with no preset concept — `SvtAv1`'s `speed`
    /// field is deliberately not aliased in here (see its own doc comment:
    /// it's a genuinely different concept that happens to map to ffmpeg's
    /// `-preset` flag) and `VideoToolbox` has no preset at all.
    #[must_use]
    pub fn preset(&self) -> Option<&str> {
        match self {
            Self::X265(s) => Some(&s.preset),
            Self::X264(s) => Some(&s.preset),
            Self::SvtAv1(_) | Self::H264Videotoolbox(_) | Self::HevcVideotoolbox(_) => None,
        }
    }

    /// `None` for `VideoToolbox`, which uses its own `quality` scale
    /// instead of a CRF (see `videotoolbox.rs`'s doc comment on the
    /// direction/units difference).
    #[must_use]
    pub fn crf(&self) -> Option<f32> {
        match self {
            Self::X265(s) => Some(s.crf),
            Self::X264(s) => Some(s.crf),
            Self::SvtAv1(s) => Some(s.crf),
            Self::H264Videotoolbox(_) | Self::HevcVideotoolbox(_) => None,
        }
    }

    /// Only x265 has a user-facing pixel-format setting today.
    #[must_use]
    pub fn pix_fmt(&self) -> Option<&str> {
        match self {
            Self::X265(s) => Some(&s.pix_fmt),
            Self::X264(_)
            | Self::SvtAv1(_)
            | Self::H264Videotoolbox(_)
            | Self::HevcVideotoolbox(_) => None,
        }
    }

    /// Always `None` today: no encoder in this app has a bitrate-mode
    /// setting yet (every one of them is CRF/quality-only, per M4's scope)
    /// — kept as a real per-variant method (matching `preset`/`crf`/
    /// `pix_fmt`'s call shape) rather than a naming-module constant, so
    /// it's obviously wrong in one place, not two, the day bitrate mode
    /// lands on some encoder and this needs to stop being a blanket `None`.
    #[must_use]
    #[allow(clippy::unused_self)]
    pub fn target_bitrate(&self) -> Option<f64> {
        None
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

    #[test]
    fn naming_accessors_are_populated_for_preset_and_crf_encoders() {
        let x265 = EncoderKind::X265(X265Settings::default());
        assert_eq!(x265.encoder_id(), "x265");
        assert_eq!(x265.preset(), Some("medium"));
        assert_eq!(x265.crf(), Some(22.0));
        assert_eq!(x265.pix_fmt(), Some("yuv420p10le"));
    }

    #[test]
    fn naming_accessors_are_none_for_concepts_svt_av1_and_videotoolbox_dont_have() {
        let svt = EncoderKind::SvtAv1(SvtAv1Settings::default());
        assert_eq!(svt.preset(), None, "svt-av1 has speed, not preset");
        assert_eq!(svt.crf(), Some(24.0));
        assert_eq!(svt.pix_fmt(), None);

        let vt = EncoderKind::H264Videotoolbox(H264VideoToolboxSettings::default());
        assert_eq!(vt.preset(), None);
        assert_eq!(vt.crf(), None, "videotoolbox has quality, not crf");
        assert_eq!(vt.pix_fmt(), None);
    }

    #[test]
    fn no_encoder_has_a_target_bitrate_yet() {
        for kind in [
            EncoderKind::X265(X265Settings::default()),
            EncoderKind::X264(X264Settings::default()),
            EncoderKind::SvtAv1(SvtAv1Settings::default()),
            EncoderKind::H264Videotoolbox(H264VideoToolboxSettings::default()),
            EncoderKind::HevcVideotoolbox(HevcVideoToolboxSettings::default()),
        ] {
            assert_eq!(kind.target_bitrate(), None);
        }
    }
}
