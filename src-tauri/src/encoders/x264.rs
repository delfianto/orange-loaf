use serde::{Deserialize, Serialize};

use super::{EncodeContext, EncodeStep, EncoderProfile};

/// x264's "Core" settings. `pix_fmt` is deliberately not user-configurable —
/// this codebase's x264 is 8-bit only (`yuv420p`), matching the prior art's
/// own hardcoded override; there's no 10-bit x264 path to expose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct X264Settings {
    pub preset: String,
    pub tune: Option<String>,
    pub profile: Option<String>,
    pub crf: f32,
}

impl Default for X264Settings {
    fn default() -> Self {
        Self {
            preset: "medium".to_owned(),
            tune: None,
            profile: None,
            crf: 23.0,
        }
    }
}

impl EncoderProfile for X264Settings {
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
        args.push("libx264".to_owned());
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
        args.push("yuv420p".to_owned());

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
    use super::{EncodeContext, EncoderProfile, X264Settings};
    use crate::encoders::EncodeStep;
    use crate::filters::FilterSpec;

    #[test]
    fn builds_a_minimal_command_with_defaults() {
        let ctx = EncodeContext {
            source: "in.mkv".to_owned(),
            output: "out.mkv".to_owned(),
            filters: FilterSpec::default(),
        };
        let steps = X264Settings::default().build_steps(&ctx);
        let EncodeStep::Argv(args) = steps.into_iter().next().unwrap();
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
                "libx264",
                "-preset:v",
                "medium",
                "-crf:v",
                "23",
                "-pix_fmt",
                "yuv420p",
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
    fn pix_fmt_is_always_yuv420p_regardless_of_settings() {
        let ctx = EncodeContext {
            source: "in.mkv".to_owned(),
            output: "out.mkv".to_owned(),
            filters: FilterSpec::default(),
        };
        let steps = X264Settings::default().build_steps(&ctx);
        let EncodeStep::Argv(args) = steps.into_iter().next().unwrap();
        let pix_fmt_index = args.iter().position(|a| a == "-pix_fmt").unwrap();
        assert_eq!(args[pix_fmt_index + 1], "yuv420p");
    }
}
