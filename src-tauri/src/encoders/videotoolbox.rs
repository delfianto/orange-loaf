use serde::{Deserialize, Serialize};

use super::common::{build_prefix, build_suffix};
use super::{EncodeContext, EncodeStep, EncoderProfile};

/// Shared `VideoToolbox` argv assembly — the h264/hevc `VideoToolbox` encoders
/// are, per the prior-art research, "byte-identical twins except the codec
/// string," so the duplication a separate struct-per-codec would cause
/// (unlike x264 vs x265, which genuinely differ in flags) is a real case
/// for one shared builder instead of two copies.
///
/// `quality` is `VideoToolbox`'s own `-q:v` scale: 0-100, **higher is better**
/// quality/bitrate — the opposite direction from x264/x265's CRF. Verified
/// empirically (not assumed from docs): encoding the same source at q=20/50/90
/// produced 11KiB/17KiB/93KiB outputs.
fn build_args(ctx: &EncodeContext, codec: &str, pix_fmt: &str, quality: f32) -> Vec<String> {
    let mut args = build_prefix(ctx);

    args.push("-c:v".to_owned());
    args.push(codec.to_owned());
    args.push("-q:v".to_owned());
    args.push(quality.to_string());
    args.push("-pix_fmt".to_owned());
    args.push(pix_fmt.to_owned());

    args.extend(build_suffix(ctx));
    args
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct H264VideoToolboxSettings {
    pub quality: f32,
}

impl Default for H264VideoToolboxSettings {
    fn default() -> Self {
        Self { quality: 60.0 }
    }
}

impl EncoderProfile for H264VideoToolboxSettings {
    fn build_steps(&self, ctx: &EncodeContext) -> Vec<EncodeStep> {
        vec![EncodeStep::Argv(build_args(
            ctx,
            "h264_videotoolbox",
            "yuv420p",
            self.quality,
        ))]
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HevcVideoToolboxSettings {
    pub quality: f32,
}

impl Default for HevcVideoToolboxSettings {
    fn default() -> Self {
        Self { quality: 60.0 }
    }
}

impl EncoderProfile for HevcVideoToolboxSettings {
    fn build_steps(&self, ctx: &EncodeContext) -> Vec<EncodeStep> {
        // p010le (10-bit) matches the prior art's default for HEVC VideoToolbox;
        // H.264 VideoToolbox stays 8-bit (yuv420p) since H.264 has no 10-bit
        // profile in this pipeline.
        vec![EncodeStep::Argv(build_args(
            ctx,
            "hevc_videotoolbox",
            "p010le",
            self.quality,
        ))]
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EncodeContext, EncoderProfile, H264VideoToolboxSettings, HevcVideoToolboxSettings,
    };
    use crate::encoders::EncodeStep;

    #[test]
    fn h264_uses_yuv420p_and_the_h264_codec() {
        let EncodeStep::Argv(args) = H264VideoToolboxSettings::default()
            .build_steps(&EncodeContext::new("in.mkv", "out.mkv"))
            .into_iter()
            .next()
            .unwrap();
        assert!(args.windows(2).any(|w| w == ["-c:v", "h264_videotoolbox"]));
        assert!(args.windows(2).any(|w| w == ["-pix_fmt", "yuv420p"]));
    }

    #[test]
    fn hevc_uses_p010le_and_the_hevc_codec() {
        let EncodeStep::Argv(args) = HevcVideoToolboxSettings::default()
            .build_steps(&EncodeContext::new("in.mkv", "out.mkv"))
            .into_iter()
            .next()
            .unwrap();
        assert!(args.windows(2).any(|w| w == ["-c:v", "hevc_videotoolbox"]));
        assert!(args.windows(2).any(|w| w == ["-pix_fmt", "p010le"]));
    }

    #[test]
    fn quality_is_emitted_under_q_v() {
        let settings = HevcVideoToolboxSettings { quality: 42.0 };
        let EncodeStep::Argv(args) = settings
            .build_steps(&EncodeContext::new("in.mkv", "out.mkv"))
            .into_iter()
            .next()
            .unwrap();
        assert!(args.windows(2).any(|w| w == ["-q:v", "42"]));
    }

    /// Runs both real `VideoToolbox` codecs against a fixture and confirms the
    /// output is genuinely playable HEVC/H.264 at the source resolution —
    /// this is real Apple Silicon/Intel hardware encoding, not a mock.
    #[test]
    #[ignore = "requires macOS VideoToolbox hardware encoding"]
    fn real_ffmpeg_produces_valid_output_for_both_codecs() {
        let cases: [(Vec<EncodeStep>, &str); 2] = [
            (
                H264VideoToolboxSettings::default().build_steps(&real_ctx("h264")),
                "h264",
            ),
            (
                HevcVideoToolboxSettings::default().build_steps(&real_ctx("hevc")),
                "hevc",
            ),
        ];
        for (steps, expected_codec_name) in cases {
            let EncodeStep::Argv(args) = steps.into_iter().next().unwrap();
            let output = real_ctx(expected_codec_name).output;

            let result = std::process::Command::new("ffmpeg")
                .args(&args)
                .output()
                .expect("failed to run ffmpeg");
            assert!(
                result.status.success(),
                "ffmpeg failed: {}",
                String::from_utf8_lossy(&result.stderr)
            );

            let probe = std::process::Command::new("ffprobe")
                .args([
                    "-v",
                    "quiet",
                    "-print_format",
                    "json",
                    "-show_streams",
                    &output,
                ])
                .output()
                .expect("failed to run ffprobe");
            let parsed: serde_json::Value =
                serde_json::from_slice(&probe.stdout).expect("invalid ffprobe json");
            assert_eq!(parsed["streams"][0]["codec_name"], expected_codec_name);
            assert_eq!(parsed["streams"][0]["width"], 320);
        }
    }

    fn real_ctx(label: &str) -> EncodeContext {
        let output = std::env::temp_dir()
            .join(format!("voltaic-vt-{label}-test.mkv"))
            .display()
            .to_string();
        EncodeContext::new("tests/fixtures/sdr_8bit.mp4", output)
    }
}
