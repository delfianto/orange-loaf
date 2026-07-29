use serde::{Deserialize, Serialize};

use super::common::{build_prefix, build_suffix, build_two_pass_steps};
use super::{EncodeContext, EncodeStep, EncoderProfile};

/// x264's "Core" settings. `pix_fmt` is deliberately not user-configurable —
/// this codebase's x264 is 8-bit only (`yuv420p`), matching the prior art's
/// own hardcoded override; there's no 10-bit x264 path to expose.
///
/// `target_bitrate_kbps: Some(_)` switches rate control from CRF to
/// bitrate mode (`-b:v` instead of `-crf:v`); `crf` itself is left as-is
/// either way, so switching back doesn't lose whatever value was there.
/// `two_pass` only does anything when bitrate mode is active — ffmpeg's
/// classic `-pass 1`/`-pass 2` mechanism is a bitrate-targeting technique,
/// meaningless for CRF/quality-based encoding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct X264Settings {
    pub preset: String,
    pub tune: Option<String>,
    pub profile: Option<String>,
    pub crf: f32,
    #[serde(default)]
    pub target_bitrate_kbps: Option<u32>,
    #[serde(default)]
    pub two_pass: bool,
}

impl Default for X264Settings {
    fn default() -> Self {
        Self {
            preset: "medium".to_owned(),
            tune: None,
            profile: None,
            crf: 23.0,
            target_bitrate_kbps: None,
            two_pass: false,
        }
    }
}

impl EncoderProfile for X264Settings {
    fn build_steps(&self, ctx: &EncodeContext) -> Vec<EncodeStep> {
        let mut video_args = vec!["-c:v".to_owned(), "libx264".to_owned()];
        video_args.push("-preset:v".to_owned());
        video_args.push(self.preset.clone());
        if let Some(tune) = &self.tune {
            video_args.push("-tune:v".to_owned());
            video_args.push(tune.clone());
        }
        if let Some(profile) = &self.profile {
            video_args.push("-profile:v".to_owned());
            video_args.push(profile.clone());
        }
        if let Some(bitrate_kbps) = self.target_bitrate_kbps {
            video_args.push("-b:v".to_owned());
            video_args.push(format!("{bitrate_kbps}k"));
        } else {
            video_args.push("-crf:v".to_owned());
            video_args.push(self.crf.to_string());
        }
        video_args.push("-pix_fmt".to_owned());
        video_args.push("yuv420p".to_owned());

        if self.target_bitrate_kbps.is_some() && self.two_pass {
            return build_two_pass_steps(ctx, &video_args);
        }

        let mut args = build_prefix(ctx);
        args.extend(video_args);
        args.extend(build_suffix(ctx));
        vec![EncodeStep::Argv(args)]
    }
}

#[cfg(test)]
mod tests {
    use super::{EncodeContext, EncoderProfile, X264Settings};
    use crate::encoders::EncodeStep;

    #[test]
    fn builds_a_minimal_command_with_defaults() {
        let steps = X264Settings::default().build_steps(&EncodeContext::new("in.mkv", "out.mkv"));
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
                "libx264",
                "-preset:v",
                "medium",
                "-crf:v",
                "23",
                "-pix_fmt",
                "yuv420p",
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
    fn pix_fmt_is_always_yuv420p_regardless_of_settings() {
        let steps = X264Settings::default().build_steps(&EncodeContext::new("in.mkv", "out.mkv"));
        let EncodeStep::Argv(args) = steps.into_iter().next().unwrap();
        let pix_fmt_index = args.iter().position(|a| a == "-pix_fmt").unwrap();
        assert_eq!(args[pix_fmt_index + 1], "yuv420p");
    }

    #[test]
    fn bitrate_mode_emits_b_v_and_omits_crf() {
        let settings = X264Settings {
            target_bitrate_kbps: Some(4000),
            ..X264Settings::default()
        };
        let steps = settings.build_steps(&EncodeContext::new("in.mkv", "out.mkv"));
        let EncodeStep::Argv(args) = steps.into_iter().next().unwrap();
        assert!(args.windows(2).any(|w| w == ["-b:v", "4000k"]));
        assert!(!args.contains(&"-crf:v".to_owned()));
    }

    #[test]
    fn two_pass_only_activates_when_bitrate_mode_is_also_set() {
        let crf_with_two_pass = X264Settings {
            two_pass: true,
            ..X264Settings::default()
        };
        let steps = crf_with_two_pass.build_steps(&EncodeContext::new("in.mkv", "out.mkv"));
        assert_eq!(
            steps.len(),
            1,
            "two_pass alone (still in CRF mode) shouldn't split into two steps"
        );

        let bitrate_with_two_pass = X264Settings {
            target_bitrate_kbps: Some(4000),
            two_pass: true,
            ..X264Settings::default()
        };
        let steps = bitrate_with_two_pass.build_steps(&EncodeContext::new("in.mkv", "out.mkv"));
        assert_eq!(
            steps.len(),
            2,
            "bitrate mode + two_pass should split into two steps"
        );
    }

    /// Runs a real 2-pass encode end to end and confirms the *second*
    /// pass's real output actually lands near the requested bitrate — not
    /// just that ffmpeg exits 0, which a broken single-pass fallback would
    /// also do.
    #[test]
    #[ignore = "requires system ffmpeg"]
    #[allow(clippy::cast_precision_loss)]
    fn real_ffmpeg_two_pass_produces_output_near_the_target_bitrate() {
        let settings = X264Settings {
            preset: "ultrafast".to_owned(),
            target_bitrate_kbps: Some(500),
            two_pass: true,
            ..X264Settings::default()
        };
        let output = std::env::temp_dir()
            .join("orangeloaf-x264-two-pass-test.mkv")
            .display()
            .to_string();
        let _ = std::fs::remove_file(&output);
        let ctx = EncodeContext::new("tests/fixtures/sdr_8bit.mp4", output.clone());
        let steps = settings.build_steps(&ctx);
        assert_eq!(steps.len(), 2);

        for step in steps {
            let EncodeStep::Argv(args) = step;
            let result = std::process::Command::new("ffmpeg")
                .args(&args)
                .output()
                .expect("failed to run ffmpeg");
            assert!(
                result.status.success(),
                "ffmpeg failed on args {args:?}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }

        let probed = crate::probe::probe_video(std::path::Path::new("ffprobe"), &output)
            .expect("probing the two-pass output should succeed");
        let bitrate_kbps = probed
            .bitrate_bps
            .expect("a real encoded file should report a bitrate")
            as f64
            / 1000.0;
        // Generous tolerance: 2-pass targets an *average*, and a few
        // seconds of source content is nowhere near enough for the
        // rate-control algorithm to converge tightly, but it should still
        // be in the right ballpark, not wildly off (e.g. accidentally
        // still CRF-encoded).
        assert!(
            (100.0..=2000.0).contains(&bitrate_kbps),
            "expected the 2-pass output's real bitrate to be roughly near \
             the requested 500kbps target, got {bitrate_kbps}kbps"
        );

        let _ = std::fs::remove_file(&output);
    }
}
