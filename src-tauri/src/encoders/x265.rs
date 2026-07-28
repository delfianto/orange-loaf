use serde::{Deserialize, Serialize};

use super::common::{build_prefix, build_suffix};
use super::{EncodeContext, EncodeStep, EncoderProfile, hdr};

/// x265's "Core" settings — the fields a user tunes for a normal encode.
/// HDR10+ dynamic metadata, adaptive quantization, B-frame tuning, and the
/// other advanced x265 params are still not covered; see notes/plan.md
/// Appendix B for the full ~28-control inventory this is deliberately a
/// subset of. Static HDR10 (mastering-display/CLL) *is* covered — see
/// `build_steps`'s use of `ctx.hdr10`.
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
        let mut args = build_prefix(ctx);

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

        if let Some(hdr10) = &ctx.hdr10 {
            let mut params = vec![
                format!("master-display={}", hdr::x265_master_display(hdr10)),
                "hdr10=1".to_owned(),
            ];
            if let Some(cll) = hdr::content_light_level(hdr10) {
                params.push(format!("max-cll={cll}"));
            }
            args.push("-x265-params".to_owned());
            args.push(params.join(":"));
        }

        args.extend(build_suffix(ctx));
        vec![EncodeStep::Argv(args)]
    }
}

#[cfg(test)]
mod tests {
    use super::{EncodeContext, EncoderProfile, X265Settings};
    use crate::encoders::EncodeStep;
    use crate::filters::{Crop, FilterSpec};
    use crate::probe::Hdr10Metadata;

    fn sample_hdr10() -> Hdr10Metadata {
        Hdr10Metadata {
            red_x: 35400.0 / 50000.0,
            red_y: 14600.0 / 50000.0,
            green_x: 8500.0 / 50000.0,
            green_y: 39850.0 / 50000.0,
            blue_x: 6550.0 / 50000.0,
            blue_y: 2300.0 / 50000.0,
            white_point_x: 15635.0 / 50000.0,
            white_point_y: 16450.0 / 50000.0,
            max_luminance: 10_000_000.0 / 10000.0,
            min_luminance: 50.0 / 10000.0,
            max_content_light: Some(1000),
            max_average_light: Some(400),
        }
    }

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

    #[test]
    fn builds_a_minimal_command_with_defaults() {
        let args = argv(
            &X265Settings::default(),
            &EncodeContext::new("in.mkv", "out.mkv"),
        );
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
                "-map",
                "0:s?",
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
                "-c:s",
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
        let args = argv(&with_extras, &EncodeContext::new("in.mkv", "out.mkv"));
        assert!(args.windows(2).any(|w| w == ["-tune:v", "grain"]));
        assert!(args.windows(2).any(|w| w == ["-profile:v", "main10"]));

        let without_extras = argv(
            &X265Settings::default(),
            &EncodeContext::new("in.mkv", "out.mkv"),
        );
        assert!(!without_extras.contains(&"-tune:v".to_owned()));
        assert!(!without_extras.contains(&"-profile:v".to_owned()));
    }

    #[test]
    fn includes_a_vf_flag_only_when_filters_are_active() {
        let no_filters = argv(
            &X265Settings::default(),
            &EncodeContext::new("in.mkv", "out.mkv"),
        );
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
        let ctx = EncodeContext {
            filters: cropped,
            ..EncodeContext::new("in.mkv", "out.mkv")
        };
        let with_filters = argv(&X265Settings::default(), &ctx);
        let vf_index = with_filters
            .iter()
            .position(|a| a == "-vf")
            .expect("expected a -vf flag");
        assert_eq!(with_filters[vf_index + 1], "crop=iw-1-1:ih-1-1:1:1");
    }

    #[test]
    fn omits_x265_params_when_source_has_no_hdr10_metadata() {
        let args = argv(
            &X265Settings::default(),
            &EncodeContext::new("in.mkv", "out.mkv"),
        );
        assert!(!args.contains(&"-x265-params".to_owned()));
    }

    #[test]
    fn includes_master_display_hdr10_flag_and_max_cll_when_hdr10_is_present() {
        let ctx = EncodeContext {
            hdr10: Some(sample_hdr10()),
            ..EncodeContext::new("in.mkv", "out.mkv")
        };
        let args = argv(&X265Settings::default(), &ctx);
        let idx = args
            .iter()
            .position(|a| a == "-x265-params")
            .expect("expected a -x265-params flag");
        let value = &args[idx + 1];
        assert!(value.starts_with("master-display=G(8500,39850)"));
        assert!(value.contains(":hdr10=1"));
        assert!(value.contains(":max-cll=1000,400"));
    }

    /// Encodes the real HDR10 fixture through x265 using its own probed
    /// metadata, then re-probes the *output* — the plan's own M8 acceptance
    /// criterion ("retains correct mastering-display/CLL in x265... verified
    /// with ffprobe") — rather than trusting the formatter's output string
    /// looks plausible.
    #[test]
    #[ignore = "requires system ffmpeg/ffprobe"]
    fn real_ffmpeg_encode_preserves_hdr10_mastering_display_and_cll() {
        let source_hdr10 =
            crate::probe::probe_video(std::path::Path::new("ffprobe"), "tests/fixtures/hdr10.mkv")
                .expect("probing the fixture should succeed")
                .hdr10
                .expect("fixture has HDR10 metadata");

        let output = std::env::temp_dir()
            .join("voltaic-x265-hdr10-test.mkv")
            .display()
            .to_string();
        let ctx = EncodeContext {
            hdr10: Some(source_hdr10),
            ..EncodeContext::new("tests/fixtures/hdr10.mkv", output.clone())
        };
        let EncodeStep::Argv(args) = X265Settings::default()
            .build_steps(&ctx)
            .into_iter()
            .next()
            .unwrap();

        let result = std::process::Command::new("ffmpeg")
            .args(&args)
            .output()
            .expect("failed to run ffmpeg");
        assert!(
            result.status.success(),
            "ffmpeg failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );

        let output_hdr10 = crate::probe::probe_video(std::path::Path::new("ffprobe"), &output)
            .expect("probing the output should succeed")
            .hdr10
            .expect("output should still carry HDR10 metadata");

        assert_eq!(output_hdr10, source_hdr10);
    }
}
