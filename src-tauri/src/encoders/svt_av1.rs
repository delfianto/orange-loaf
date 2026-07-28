use serde::{Deserialize, Serialize};

use super::common::{build_prefix, build_suffix};
use super::{EncodeContext, EncodeStep, EncoderProfile, hdr};

/// SVT-AV1's "Core" settings.
///
/// `speed` is deliberately not named `preset`: the underlying settings
/// concept is genuinely called "speed" (0 slowest/best quality, 13
/// fastest), but the ffmpeg flag that carries it is `-preset` — a real
/// field-name/flag-name mismatch inherited from the prior art (see
/// notes/plan.md §4.3 Appendix A). Naming the Rust field to match the flag
/// instead would silently relabel what the number actually means.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SvtAv1Settings {
    pub speed: u8,
    pub crf: f32,
    pub tune: Option<u8>,
}

impl Default for SvtAv1Settings {
    fn default() -> Self {
        Self {
            speed: 7,
            crf: 24.0,
            tune: None,
        }
    }
}

impl EncoderProfile for SvtAv1Settings {
    fn build_steps(&self, ctx: &EncodeContext) -> Vec<EncodeStep> {
        let mut args = build_prefix(ctx);

        args.push("-c:v".to_owned());
        args.push("libsvtav1".to_owned());
        // Field is `speed`, flag is `-preset` — see the struct doc comment.
        args.push("-preset".to_owned());
        args.push(self.speed.to_string());
        args.push("-crf".to_owned());
        args.push(self.crf.to_string());

        // Both `tune` and HDR10 ride on the same `-svtav1-params` AVOption —
        // passing it twice would let the second call silently discard the
        // first, so every fragment is collected and joined into one flag.
        let mut svtav1_params = Vec::new();
        if let Some(tune) = self.tune {
            svtav1_params.push(format!("tune={tune}"));
        }
        if let Some(hdr10) = &ctx.hdr10 {
            svtav1_params.push(format!(
                "mastering-display={}",
                hdr::svtav1_mastering_display(hdr10)
            ));
            if let Some(cll) = hdr::content_light_level(hdr10) {
                svtav1_params.push(format!("content-light={cll}"));
            }
        }
        if !svtav1_params.is_empty() {
            args.push("-svtav1-params".to_owned());
            args.push(svtav1_params.join(":"));
        }

        args.extend(build_suffix(ctx));
        vec![EncodeStep::Argv(args)]
    }
}

#[cfg(test)]
mod tests {
    use super::{EncodeContext, EncoderProfile, SvtAv1Settings};
    use crate::encoders::EncodeStep;
    use crate::probe::Hdr10Metadata;

    fn sample_hdr10() -> Hdr10Metadata {
        Hdr10Metadata {
            red_x: 0.68,
            red_y: 0.32,
            green_x: 0.2649,
            green_y: 0.69,
            blue_x: 0.15,
            blue_y: 0.06,
            white_point_x: 0.3127,
            white_point_y: 0.329,
            max_luminance: 1000.0,
            min_luminance: 0.01,
            max_content_light: Some(1000),
            max_average_light: Some(400),
        }
    }

    #[test]
    fn the_speed_field_is_emitted_under_the_preset_flag() {
        let settings = SvtAv1Settings {
            speed: 4,
            ..SvtAv1Settings::default()
        };
        let steps = settings.build_steps(&EncodeContext::new("in.mkv", "out.mkv"));
        let EncodeStep::Argv(args) = steps.into_iter().next().unwrap();
        assert!(args.windows(2).any(|w| w == ["-preset", "4"]));
        assert!(!args.contains(&"-speed".to_owned()));
    }

    /// Runs the real command against a fixture and checks ffmpeg's own
    /// stderr for SVT-AV1's config summary line, confirming `-preset`/`-crf`
    /// were actually understood as speed 8 / CRF 35 — not just that ffmpeg
    /// exited 0 (verified manually first; see notes/plan.md M4).
    #[test]
    #[ignore = "requires system ffmpeg with libsvtav1"]
    fn real_ffmpeg_applies_speed_and_crf_as_preset_and_crf() {
        let settings = SvtAv1Settings {
            speed: 8,
            crf: 35.0,
            tune: None,
        };
        let output = std::env::temp_dir()
            .join("voltaic-svtav1-test.mkv")
            .display()
            .to_string();
        let ctx = EncodeContext::new("tests/fixtures/sdr_8bit.mp4", output);
        let EncodeStep::Argv(args) = settings.build_steps(&ctx).into_iter().next().unwrap();

        let output = std::process::Command::new("ffmpeg")
            .args(&args)
            .output()
            .expect("failed to run ffmpeg");
        assert!(
            output.status.success(),
            "ffmpeg failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("preset / tune / pred struct") && stderr.contains(": 8 /"),
            "expected SVT-AV1 to report preset 8, got:\n{stderr}"
        );
        assert!(
            stderr.contains("CRF / 35.00"),
            "expected SVT-AV1 to report CRF 35.00, got:\n{stderr}"
        );
    }

    #[test]
    fn builds_a_minimal_command_with_defaults() {
        let steps = SvtAv1Settings::default().build_steps(&EncodeContext::new("in.mkv", "out.mkv"));
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
                "-map",
                "0:s?",
                "-c:v",
                "libsvtav1",
                "-preset",
                "7",
                "-crf",
                "24",
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
    fn tune_and_hdr10_merge_into_one_svtav1_params_flag_not_two() {
        let settings = SvtAv1Settings {
            tune: Some(0),
            ..SvtAv1Settings::default()
        };
        let ctx = EncodeContext {
            hdr10: Some(sample_hdr10()),
            ..EncodeContext::new("in.mkv", "out.mkv")
        };
        let EncodeStep::Argv(args) = settings.build_steps(&ctx).into_iter().next().unwrap();

        let occurrences = args.iter().filter(|a| *a == "-svtav1-params").count();
        assert_eq!(
            occurrences, 1,
            "a second -svtav1-params flag would silently discard the first"
        );
        let idx = args
            .iter()
            .position(|a| a == "-svtav1-params")
            .expect("expected a -svtav1-params flag");
        let value = &args[idx + 1];
        assert!(value.contains("tune=0"));
        assert!(value.contains("mastering-display=G("));
        assert!(value.contains("content-light=1000,400"));
    }

    /// Converts the real HEVC HDR10 fixture (whose mastering-display is
    /// reported in HEVC's /50000,/10000 fixed-point scale) into SVT-AV1
    /// (plain-decimal scale) and re-probes the *output* — this is the
    /// specific "inverse rescale" path notes/plan.md §1.2 warns is a likely
    /// bug spot if copied carelessly. An epsilon tolerance (not exact
    /// equality, unlike the x265 round-trip test) is deliberate: the value
    /// passes through a 4-decimal-place string *and* AV1's own 1/65536
    /// fixed-point re-quantization, both of which are real, expected, small
    /// precision losses — not the point being tested.
    #[test]
    #[ignore = "requires system ffmpeg/ffprobe with libsvtav1"]
    fn real_ffmpeg_encode_preserves_hdr10_across_the_hevc_to_av1_rescale() {
        let source_hdr10 =
            crate::probe::probe_video(std::path::Path::new("ffprobe"), "tests/fixtures/hdr10.mkv")
                .expect("probing the fixture should succeed")
                .hdr10
                .expect("fixture has HDR10 metadata");

        let output = std::env::temp_dir()
            .join("voltaic-svtav1-hdr10-test.mkv")
            .display()
            .to_string();
        let ctx = EncodeContext {
            hdr10: Some(source_hdr10),
            ..EncodeContext::new("tests/fixtures/hdr10.mkv", output.clone())
        };
        let EncodeStep::Argv(args) = SvtAv1Settings::default()
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

        let close = |a: f64, b: f64| (a - b).abs() < 0.001;
        assert!(close(output_hdr10.red_x, source_hdr10.red_x));
        assert!(close(output_hdr10.red_y, source_hdr10.red_y));
        assert!(close(output_hdr10.green_x, source_hdr10.green_x));
        assert!(close(output_hdr10.green_y, source_hdr10.green_y));
        assert!(close(
            output_hdr10.white_point_x,
            source_hdr10.white_point_x
        ));
        assert!(close(
            output_hdr10.max_luminance,
            source_hdr10.max_luminance
        ));
        assert_eq!(
            output_hdr10.max_content_light,
            source_hdr10.max_content_light
        );
        assert_eq!(
            output_hdr10.max_average_light,
            source_hdr10.max_average_light
        );
    }
}
