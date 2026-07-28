use serde::{Deserialize, Serialize};

use super::{EncodeContext, EncodeStep, EncoderProfile};

/// x265's "Core" settings — the fields a user tunes for a normal encode.
/// HDR10/HDR10+, adaptive quantization, B-frame tuning, and the other
/// advanced x265 params land in M8; see notes/plan.md Appendix B for the
/// full ~28-control inventory this is deliberately a subset of.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct X265Settings {
    pub preset: String,
    pub tune: Option<String>,
    pub profile: Option<String>,
    pub crf: f32,
    pub pix_fmt: String,
}

impl Default for X265Settings {
    fn default() -> Self {
        Self {
            preset: "medium".to_owned(),
            tune: None,
            profile: None,
            crf: 22.0,
            pix_fmt: "yuv420p10le".to_owned(),
        }
    }
}

impl EncoderProfile for X265Settings {
    fn build_steps(&self, ctx: &EncodeContext) -> Vec<EncodeStep> {
        let mut args: Vec<String> = ["-y", "-i", &ctx.source, "-map", "0:v:0", "-map", "0:a?"]
            .into_iter()
            .map(str::to_owned)
            .collect();

        if let Some(filter_graph) = ctx.filters.render() {
            args.push("-vf".to_owned());
            args.push(filter_graph);
        }

        args.push("-c:v".to_owned());
        args.push("libx265".to_owned());
        args.push("-preset:v".to_owned());
        args.push(self.preset.clone());
        if let Some(tune) = &self.tune {
            args.push("-tune:v".to_owned());
            args.push(tune.clone());
        }
        if let Some(profile) = &self.profile {
            args.push("-profile:v".to_owned());
            args.push(profile.clone());
        }
        args.push("-crf:v".to_owned());
        args.push(self.crf.to_string());
        args.push("-pix_fmt".to_owned());
        args.push(self.pix_fmt.clone());

        // No audio settings UI yet (M7) — copy whatever audio exists through
        // unchanged so a first real encode is a usable file, not a silent one.
        args.push("-c:a".to_owned());
        args.push("copy".to_owned());

        args.push("-progress".to_owned());
        args.push("pipe:1".to_owned());
        args.push("-nostats".to_owned());
        args.push(ctx.output.clone());

        vec![EncodeStep::Argv(args)]
    }
}

#[cfg(test)]
mod tests {
    use super::{EncodeContext, EncoderProfile, X265Settings};
    use crate::encoders::EncodeStep;
    use crate::filters::{Crop, FilterSpec};

    fn argv(settings: &X265Settings, ctx: &EncodeContext) -> Vec<String> {
        let steps = settings.build_steps(ctx);
        assert_eq!(
            steps.len(),
            1,
            "x265 without two-pass should be exactly one step"
        );
        let EncodeStep::Argv(args) = steps.into_iter().next().unwrap();
        args
    }

    fn ctx(filters: FilterSpec) -> EncodeContext {
        EncodeContext {
            source: "in.mkv".to_owned(),
            output: "out.mkv".to_owned(),
            filters,
        }
    }

    #[test]
    fn builds_a_minimal_command_with_defaults() {
        let args = argv(&X265Settings::default(), &ctx(FilterSpec::default()));
        assert_eq!(
            args,
            vec![
                "-y",
                "-i",
                "in.mkv",
                "-map",
                "0:v:0",
                "-map",
                "0:a?",
                "-c:v",
                "libx265",
                "-preset:v",
                "medium",
                "-crf:v",
                "22",
                "-pix_fmt",
                "yuv420p10le",
                "-c:a",
                "copy",
                "-progress",
                "pipe:1",
                "-nostats",
                "out.mkv",
            ]
        );
    }

    #[test]
    fn omits_tune_and_profile_flags_when_unset_but_includes_them_when_set() {
        let with_extras = X265Settings {
            tune: Some("grain".to_owned()),
            profile: Some("main10".to_owned()),
            ..X265Settings::default()
        };
        let args = argv(&with_extras, &ctx(FilterSpec::default()));
        assert!(args.windows(2).any(|w| w == ["-tune:v", "grain"]));
        assert!(args.windows(2).any(|w| w == ["-profile:v", "main10"]));

        let without_extras = argv(&X265Settings::default(), &ctx(FilterSpec::default()));
        assert!(!without_extras.contains(&"-tune:v".to_owned()));
        assert!(!without_extras.contains(&"-profile:v".to_owned()));
    }

    #[test]
    fn includes_a_vf_flag_only_when_filters_are_active() {
        let no_filters = argv(&X265Settings::default(), &ctx(FilterSpec::default()));
        assert!(!no_filters.contains(&"-vf".to_owned()));

        let cropped = FilterSpec {
            crop: Some(Crop {
                top: 1,
                bottom: 1,
                left: 1,
                right: 1,
            }),
            ..FilterSpec::default()
        };
        let with_filters = argv(&X265Settings::default(), &ctx(cropped));
        let vf_index = with_filters
            .iter()
            .position(|a| a == "-vf")
            .expect("expected a -vf flag");
        assert_eq!(with_filters[vf_index + 1], "crop=iw-1-1:ih-1-1:1:1");
    }
}
