use serde::{Deserialize, Serialize};

use super::common::{build_prefix, build_suffix};
use super::{EncodeContext, EncodeStep, EncoderProfile};

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
        if let Some(tune) = self.tune {
            args.push("-svtav1-params".to_owned());
            args.push(format!("tune={tune}"));
        }

        args.extend(build_suffix(ctx));
        vec![EncodeStep::Argv(args)]
    }
}

#[cfg(test)]
mod tests {
    use super::{EncodeContext, EncoderProfile, SvtAv1Settings};
    use crate::encoders::EncodeStep;

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
                "-c:v",
                "libsvtav1",
                "-preset",
                "7",
                "-crf",
                "24",
                "-c:a",
                "copy",
                "-progress",
                "pipe:1",
                "-nostats",
                "out.mkv",
            ]
        );
    }
}
