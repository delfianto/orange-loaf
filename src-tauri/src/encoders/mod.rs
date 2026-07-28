pub mod svt_av1;
pub mod x264;
pub mod x265;

use serde::{Deserialize, Serialize};

use crate::filters::FilterSpec;
use svt_av1::SvtAv1Settings;
use x264::X264Settings;
use x265::X265Settings;

/// Everything a specific encoder's [`EncoderProfile::build_steps`] needs to
/// know about the job that isn't its own codec settings.
pub struct EncodeContext {
    pub source: String,
    pub output: String,
    pub filters: FilterSpec,
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
}

impl EncoderProfile for EncoderKind {
    fn build_steps(&self, ctx: &EncodeContext) -> Vec<EncodeStep> {
        match self {
            Self::X265(settings) => settings.build_steps(ctx),
            Self::X264(settings) => settings.build_steps(ctx),
            Self::SvtAv1(settings) => settings.build_steps(ctx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EncoderKind;
    use super::svt_av1::SvtAv1Settings;
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
    }

    #[test]
    fn tag_and_settings_fields_are_flattened_at_the_same_level() {
        let value = serde_json::to_value(EncoderKind::X265(X265Settings::default())).unwrap();
        assert_eq!(value["preset"], "medium");
        assert_eq!(value["crf"], 22.0);
    }
}
