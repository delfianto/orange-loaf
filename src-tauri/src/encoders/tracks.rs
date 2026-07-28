use serde::{Deserialize, Serialize};

/// What to do with one audio track at encode time. `Copy` is the common
/// case (no re-encode); `Convert` re-encodes to a different codec, using
/// the quality→bitrate table below.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum AudioAction {
    Copy,
    Convert {
        codec: String,
        /// Index into the shared quality→bitrate table (0 = best/highest
        /// bitrate, 9 = worst/lowest) — see `bitrate_args` below. Meaningless
        /// for lossless codecs, which ignore it.
        quality: u8,
        /// `Some(n)` downmixes to `n` channels (e.g. 5.1 -> stereo);
        /// `None` keeps the source channel count.
        downmix: Option<u32>,
    },
}

/// One audio track's selection, as received from the frontend. `index` is
/// the *type-relative* stream index (matches ffmpeg's `0:a:{index}`, not
/// ffprobe's global stream `index`). `source_channels` is denormalized from
/// the probe result the frontend already has, so this type is self-contained
/// for argv building without `EncodeContext` needing to re-probe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioTrackSelection {
    pub index: u32,
    pub source_channels: u32,
    pub enabled: bool,
    pub action: AudioAction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleTrackSelection {
    pub index: u32,
    pub enabled: bool,
}

/// index 0-9, kbps *per channel* — the shared base every codec's formula
/// scales from (notes/plan.md Appendix A).
const BASE_KBPS_PER_CHANNEL: [u32; 10] = [120, 96, 72, 48, 24, 24, 16, 8, 8, 8];

/// Codecs with no bitrate/quality concept at all — re-encoding to one of
/// these emits no rate-control flag. `TrueHD` is grouped here (not with
/// ac3/eac3's bitrate formula, despite Appendix A's condensed table listing
/// it in both places) because it's fundamentally a lossless format; treating
/// it as lossy-with-a-fixed-formula would be the wrong reading of that
/// ambiguity.
const LOSSLESS_CODECS: &[&str] = &["flac", "truehd", "alac", "tta", "wavpack", "mlp"];

/// Builds the rate-control flags for re-encoding one audio track, per the
/// codec-specific formula in notes/plan.md Appendix A. `output_ordinal` is
/// this track's position among the *output* audio streams (not its input
/// index), since ffmpeg's per-stream options address outputs.
fn bitrate_args(codec: &str, quality: u8, channels: u32, output_ordinal: usize) -> Vec<String> {
    if LOSSLESS_CODECS.contains(&codec) {
        return Vec::new();
    }
    let i = usize::from(quality.min(9));
    let base = BASE_KBPS_PER_CHANNEL[i];
    let n = output_ordinal;
    match codec {
        "libopus" => vec![
            format!("-vbr:a:{n}"),
            "on".to_owned(),
            format!("-b:a:{n}"),
            format!("{}k", base * channels),
        ],
        "aac" => {
            const Q: [f32; 10] = [2.0, 1.8, 1.6, 1.4, 1.2, 1.0, 0.8, 0.6, 0.4, 0.2];
            vec![format!("-q:a:{n}"), Q[i].to_string()]
        }
        "libfdk_aac" => {
            const Q: [u8; 10] = [1, 1, 2, 2, 3, 3, 4, 4, 5, 5];
            vec![format!("-q:a:{n}"), Q[i].to_string()]
        }
        "libvorbis" => {
            const Q: [u8; 10] = [10, 9, 8, 7, 6, 5, 4, 3, 2, 1];
            vec![format!("-q:a:{n}"), Q[i].to_string()]
        }
        "libmp3lame" => vec![format!("-q:a:{n}"), i.to_string()],
        "ac3" | "eac3" => vec![format!("-b:a:{n}"), format!("{}k", base * channels * 4)],
        _ => vec![format!("-b:a:{n}"), format!("{}k", base * channels)],
    }
}

/// The enabled tracks, paired with their *output* ordinal (0-based position
/// among just the enabled tracks) — the single source of truth both
/// `audio_map_args` and `audio_codec_args` iterate, so the two independently
/// agree on which output stream number belongs to which track without
/// passing state between them.
fn enabled<T>(tracks: &[T], is_enabled: impl Fn(&T) -> bool) -> impl Iterator<Item = (usize, &T)> {
    tracks.iter().filter(move |t| is_enabled(t)).enumerate()
}

pub fn audio_map_args(tracks: &[AudioTrackSelection]) -> Vec<String> {
    if tracks.is_empty() {
        return vec!["-map".to_owned(), "0:a?".to_owned()];
    }
    enabled(tracks, |t| t.enabled)
        .flat_map(|(_, t)| ["-map".to_owned(), format!("0:a:{}", t.index)])
        .collect()
}

pub fn audio_codec_args(tracks: &[AudioTrackSelection]) -> Vec<String> {
    if tracks.is_empty() {
        return vec!["-c:a".to_owned(), "copy".to_owned()];
    }
    enabled(tracks, |t| t.enabled)
        .flat_map(|(n, t)| {
            let mut args = vec![format!("-c:a:{n}")];
            match &t.action {
                AudioAction::Copy => args.push("copy".to_owned()),
                AudioAction::Convert {
                    codec,
                    quality,
                    downmix,
                } => {
                    args.push(codec.clone());
                    let channels = downmix.unwrap_or(t.source_channels);
                    args.extend(bitrate_args(codec, *quality, channels, n));
                    if let Some(down_to) = downmix {
                        args.push(format!("-ac:a:{n}"));
                        args.push(down_to.to_string());
                    }
                }
            }
            args
        })
        .collect()
}

pub fn subtitle_map_args(tracks: &[SubtitleTrackSelection]) -> Vec<String> {
    if tracks.is_empty() {
        return vec!["-map".to_owned(), "0:s?".to_owned()];
    }
    enabled(tracks, |t| t.enabled)
        .flat_map(|(_, t)| ["-map".to_owned(), format!("0:s:{}", t.index)])
        .collect()
}

pub fn subtitle_codec_args(tracks: &[SubtitleTrackSelection]) -> Vec<String> {
    if tracks.is_empty() {
        return vec!["-c:s".to_owned(), "copy".to_owned()];
    }
    // Copy-only in this pass — burn-in needs picture-vs-text filter-chain
    // ordering (notes/plan.md §1.3) and its own verification, deferred.
    enabled(tracks, |t| t.enabled)
        .flat_map(|(n, _)| [format!("-c:s:{n}"), "copy".to_owned()])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        AudioAction, AudioTrackSelection, SubtitleTrackSelection, audio_codec_args, audio_map_args,
        subtitle_codec_args, subtitle_map_args,
    };

    /// Pins the exact wire shape the frontend's TS union must match — same
    /// reasoning as `EncoderKind`'s equivalent test in `encoders/mod.rs`:
    /// written once here instead of guessed twice.
    #[test]
    fn audio_action_serializes_with_the_expected_camel_case_tag_values() {
        assert_eq!(
            serde_json::to_value(AudioAction::Copy).unwrap(),
            serde_json::json!({ "action": "copy" })
        );
        assert_eq!(
            serde_json::to_value(AudioAction::Convert {
                codec: "aac".to_owned(),
                quality: 3,
                downmix: Some(2),
            })
            .unwrap(),
            serde_json::json!({
                "action": "convert",
                "codec": "aac",
                "quality": 3,
                "downmix": 2,
            })
        );
    }

    #[test]
    fn empty_audio_selection_falls_back_to_the_legacy_copy_everything_wildcard() {
        assert_eq!(audio_map_args(&[]), vec!["-map", "0:a?"]);
        assert_eq!(audio_codec_args(&[]), vec!["-c:a", "copy"]);
    }

    #[test]
    fn empty_subtitle_selection_defaults_to_copy_everything() {
        assert_eq!(subtitle_map_args(&[]), vec!["-map", "0:s?"]);
        assert_eq!(subtitle_codec_args(&[]), vec!["-c:s", "copy"]);
    }

    #[test]
    fn disabled_tracks_are_mapped_to_nothing() {
        let tracks = vec![AudioTrackSelection {
            index: 0,
            source_channels: 2,
            enabled: false,
            action: AudioAction::Copy,
        }];
        assert_eq!(audio_map_args(&tracks), Vec::<String>::new());
        assert_eq!(audio_codec_args(&tracks), Vec::<String>::new());
    }

    #[test]
    fn output_ordinals_skip_disabled_tracks_rather_than_reusing_input_index() {
        // Input track 0 disabled, input track 1 enabled: track 1 must land
        // on OUTPUT ordinal 0 (the first, and only, output audio stream) —
        // reusing its own input index (1) here would ask ffmpeg to set
        // `-c:a:1` on an output that only has one stream (index 0).
        let tracks = vec![
            AudioTrackSelection {
                index: 0,
                source_channels: 2,
                enabled: false,
                action: AudioAction::Copy,
            },
            AudioTrackSelection {
                index: 1,
                source_channels: 2,
                enabled: true,
                action: AudioAction::Copy,
            },
        ];
        assert_eq!(audio_map_args(&tracks), vec!["-map", "0:a:1"]);
        assert_eq!(audio_codec_args(&tracks), vec!["-c:a:0", "copy"]);
    }

    #[test]
    fn convert_action_emits_codec_and_bitrate_flags() {
        let tracks = vec![AudioTrackSelection {
            index: 0,
            source_channels: 2,
            enabled: true,
            action: AudioAction::Convert {
                codec: "aac".to_owned(),
                quality: 0,
                downmix: None,
            },
        }];
        let args = audio_codec_args(&tracks);
        assert_eq!(args, vec!["-c:a:0", "aac", "-q:a:0", "2"]);
    }

    #[test]
    fn downmix_uses_the_downmixed_channel_count_for_bitrate_and_adds_ac() {
        let tracks = vec![AudioTrackSelection {
            index: 0,
            source_channels: 6,
            enabled: true,
            action: AudioAction::Convert {
                codec: "ac3".to_owned(),
                quality: 3, // base 48 kbps/channel
                downmix: Some(2),
            },
        }];
        let args = audio_codec_args(&tracks);
        // ac3 formula is base * channels * 4; downmixed channels (2), not
        // the source's 6, must be what the bitrate scales from.
        assert_eq!(
            args,
            vec!["-c:a:0", "ac3", "-b:a:0", "384k", "-ac:a:0", "2"]
        );
    }

    #[test]
    fn lossless_codecs_emit_no_rate_control_flag() {
        let tracks = vec![AudioTrackSelection {
            index: 0,
            source_channels: 2,
            enabled: true,
            action: AudioAction::Convert {
                codec: "flac".to_owned(),
                quality: 0,
                downmix: None,
            },
        }];
        assert_eq!(audio_codec_args(&tracks), vec!["-c:a:0", "flac"]);
    }

    #[test]
    fn subtitle_tracks_follow_the_same_output_ordinal_rule() {
        let tracks = vec![
            SubtitleTrackSelection {
                index: 0,
                enabled: false,
            },
            SubtitleTrackSelection {
                index: 1,
                enabled: true,
            },
        ];
        assert_eq!(subtitle_map_args(&tracks), vec!["-map", "0:s:1"]);
        assert_eq!(subtitle_codec_args(&tracks), vec!["-c:s:0", "copy"]);
    }

    /// Runs a real multi-track encode against `tests/fixtures/multitrack.mkv`
    /// (1 video + 2 audio [eng aac, jpn aac] + 2 subtitle [eng, spa srt]),
    /// disabling one track of each type and converting the other, then
    /// verifies the *output* file — via real ffprobe, not just argv
    /// inspection — has exactly the tracks that should survive, with the
    /// right codec/language. This is the scenario the output-ordinal
    /// remapping logic above exists for: naively reusing input indices here
    /// would ask ffmpeg to set options on output streams that don't exist.
    #[test]
    #[ignore = "requires system ffmpeg/ffprobe"]
    fn real_ffmpeg_round_trips_a_disable_and_convert_selection() {
        use super::super::EncodeContext;
        use super::super::common::{build_prefix, build_suffix};

        let output = std::env::temp_dir()
            .join("voltaic-tracks-test.mkv")
            .display()
            .to_string();
        let ctx = EncodeContext {
            audio_tracks: vec![
                AudioTrackSelection {
                    index: 0,
                    source_channels: 2,
                    enabled: false,
                    action: AudioAction::Copy,
                },
                AudioTrackSelection {
                    index: 1,
                    source_channels: 2,
                    enabled: true,
                    action: AudioAction::Convert {
                        codec: "libmp3lame".to_owned(),
                        quality: 4,
                        downmix: None,
                    },
                },
            ],
            subtitle_tracks: vec![
                SubtitleTrackSelection {
                    index: 0,
                    enabled: false,
                },
                SubtitleTrackSelection {
                    index: 1,
                    enabled: true,
                },
            ],
            ..EncodeContext::new("tests/fixtures/multitrack.mkv", output.clone())
        };

        let mut args = build_prefix(&ctx);
        args.extend(["-c:v".to_owned(), "libx264".to_owned()]);
        args.extend(build_suffix(&ctx));

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
        let streams = parsed["streams"].as_array().expect("streams array");

        let audio: Vec<&serde_json::Value> = streams
            .iter()
            .filter(|s| s["codec_type"] == "audio")
            .collect();
        assert_eq!(
            audio.len(),
            1,
            "the disabled (eng) audio track must be gone"
        );
        assert_eq!(audio[0]["codec_name"], "mp3");
        assert_eq!(
            audio[0]["tags"]["language"], "jpn",
            "language metadata should survive re-encoding, not just copy"
        );

        let subs: Vec<&serde_json::Value> = streams
            .iter()
            .filter(|s| s["codec_type"] == "subtitle")
            .collect();
        assert_eq!(
            subs.len(),
            1,
            "the disabled (eng) subtitle track must be gone"
        );
        assert_eq!(subs[0]["tags"]["language"], "spa");
    }
}
