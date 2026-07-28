use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

/// Everything Voltaic currently knows how to show about a probed source
/// video. Deliberately small — only the fields the Convert screen actually
/// renders today; more fields join this (and the ffprobe deserialization
/// below) as the features that need them land.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    pub duration_secs: f64,
    pub width: u32,
    pub height: u32,
    pub codec_name: String,
    pub pix_fmt: Option<String>,
    pub bit_depth: u8,
    pub frame_rate: Option<f64>,
    pub hdr10: bool,
    pub audio_tracks: Vec<AudioTrack>,
    pub subtitle_tracks: Vec<SubtitleTrack>,
}

/// One audio stream as reported by ffprobe. `index` is *type-relative*
/// (0 = first audio stream) to match ffmpeg's own `-map 0:a:{index}`
/// addressing — deliberately not ffprobe's global stream `index`, which
/// counts video/audio/subtitle/data streams together.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioTrack {
    pub index: u32,
    pub codec_name: String,
    pub channels: u32,
    pub language: Option<String>,
    pub title: Option<String>,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleTrack {
    pub index: u32,
    pub codec_name: String,
    pub language: Option<String>,
    pub title: Option<String>,
    pub is_default: bool,
}

#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    #[serde(default)]
    streams: Vec<FfprobeStream>,
    format: FfprobeFormat,
}

#[derive(Debug, Deserialize, Default)]
struct FfprobeStream {
    codec_type: String,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    pix_fmt: Option<String>,
    bits_per_raw_sample: Option<String>,
    color_primaries: Option<String>,
    r_frame_rate: Option<String>,
    channels: Option<u32>,
    #[serde(default)]
    disposition: FfprobeDisposition,
    #[serde(default)]
    tags: FfprobeTags,
}

#[derive(Debug, Deserialize, Default)]
struct FfprobeDisposition {
    #[serde(default)]
    attached_pic: u8,
    #[serde(default)]
    default: u8,
}

#[derive(Debug, Deserialize, Default)]
struct FfprobeTags {
    language: Option<String>,
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFrameSideData {
    side_data_type: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct FfprobeFrame {
    #[serde(default)]
    side_data_list: Vec<FfprobeFrameSideData>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFramesOutput {
    #[serde(default)]
    frames: Vec<FfprobeFrame>,
}

/// A real, decodable video stream: not a cover-art image encoded as a video
/// stream (ffprobe reports embedded cover art as a video-typed stream with
/// `disposition.attached_pic` set, rather than as an attachment stream).
fn is_real_video_stream(stream: &FfprobeStream) -> bool {
    stream.codec_type == "video" && stream.disposition.attached_pic == 0
}

/// Enumerates every stream of `codec_type` and assigns each a type-relative
/// index (0, 1, 2, ...) — this is what ffmpeg's own `-map 0:a:{index}` /
/// `-map 0:s:{index}` addressing counts, unlike ffprobe's `index` field,
/// which counts across all stream types together.
fn audio_tracks(streams: &[FfprobeStream]) -> Vec<AudioTrack> {
    streams
        .iter()
        .filter(|s| s.codec_type == "audio")
        .enumerate()
        .map(|(index, s)| AudioTrack {
            index: u32::try_from(index).unwrap_or(u32::MAX),
            codec_name: s.codec_name.clone().unwrap_or_else(|| "unknown".to_owned()),
            channels: s.channels.unwrap_or(2),
            language: s.tags.language.clone(),
            title: s.tags.title.clone(),
            is_default: s.disposition.default != 0,
        })
        .collect()
}

fn subtitle_tracks(streams: &[FfprobeStream]) -> Vec<SubtitleTrack> {
    streams
        .iter()
        .filter(|s| s.codec_type == "subtitle")
        .enumerate()
        .map(|(index, s)| SubtitleTrack {
            index: u32::try_from(index).unwrap_or(u32::MAX),
            codec_name: s.codec_name.clone().unwrap_or_else(|| "unknown".to_owned()),
            language: s.tags.language.clone(),
            title: s.tags.title.clone(),
            is_default: s.disposition.default != 0,
        })
        .collect()
}

/// Guesses bit depth, preferring ffprobe's own `bits_per_raw_sample` when it
/// reports one, then falling back to known 10/12-bit pixel formats, then a
/// BT.2020-implies-10-bit heuristic.
fn guess_bit_depth(
    bits_per_raw_sample: Option<&str>,
    pix_fmt: Option<&str>,
    color_primaries: Option<&str>,
) -> u8 {
    const TEN_BIT: &[&str] = &[
        "yuv420p10le",
        "yuv422p10le",
        "yuv444p10le",
        "gbrp10le",
        "gray10le",
        "p010le",
    ];
    const TWELVE_BIT: &[&str] = &[
        "yuv420p12le",
        "yuv422p12le",
        "yuv444p12le",
        "gbrp12le",
        "gray12le",
    ];

    if let Some(reported) = bits_per_raw_sample.and_then(|raw| raw.parse::<u8>().ok()) {
        if reported > 0 {
            return reported;
        }
    }
    if let Some(fmt) = pix_fmt {
        if TWELVE_BIT.contains(&fmt) {
            return 12;
        }
        if TEN_BIT.contains(&fmt) {
            return 10;
        }
    }
    if color_primaries.is_some_and(|p| p.starts_with("bt2020")) {
        return 10;
    }
    8
}

/// Parses ffprobe's `"num/den"` frame-rate strings into a plain f64.
fn parse_frame_rate(raw: &str) -> Option<f64> {
    let (num, den) = raw.split_once('/')?;
    let num: f64 = num.parse().ok()?;
    let den: f64 = den.parse().ok()?;
    if den == 0.0 { None } else { Some(num / den) }
}

/// Runs ffprobe's frame-level probe on the first video stream and reports
/// whether it carries HDR10 static (mastering-display) metadata. This is a
/// separate call from the main `-show_streams` probe because mastering-display
/// data is exposed by ffprobe as frame side-data, not a stream-level field.
fn detect_hdr10(ffprobe: &Path, source: &str) -> bool {
    let Ok(output) = Command::new(ffprobe)
        .args([
            "-loglevel",
            "panic",
            "-select_streams",
            "v:0",
            "-print_format",
            "json",
            "-show_frames",
            "-read_intervals",
            "%+#1",
            "-show_entries",
            "frame=side_data_list",
            source,
        ])
        .output()
    else {
        return false;
    };
    let Ok(parsed) = serde_json::from_slice::<FfprobeFramesOutput>(&output.stdout) else {
        return false;
    };
    parsed.frames.iter().any(|frame| {
        frame.side_data_list.iter().any(|side_data| {
            side_data.side_data_type.as_deref() == Some("Mastering display metadata")
        })
    })
}

/// Probes a source video with ffprobe and returns the subset of its
/// metadata Voltaic currently uses.
///
/// # Errors
///
/// Returns an error if ffprobe can't be run, its output can't be parsed,
/// or the source has no real (non-attached-picture) video stream.
pub fn probe_video(ffprobe: &Path, source: &str) -> Result<VideoInfo, String> {
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            source,
        ])
        .output()
        .map_err(|err| format!("failed to run ffprobe: {err}"))?;

    if !output.status.success() {
        return Err(format!("ffprobe exited with an error probing {source}"));
    }

    let parsed: FfprobeOutput = serde_json::from_slice(&output.stdout)
        .map_err(|err| format!("failed to parse ffprobe output: {err}"))?;

    let video_stream = parsed
        .streams
        .iter()
        .find(|stream| is_real_video_stream(stream))
        .ok_or("no video stream found")?;

    let duration_secs = parsed
        .format
        .duration
        .as_deref()
        .and_then(|d| d.parse::<f64>().ok())
        .unwrap_or(0.0);

    Ok(VideoInfo {
        duration_secs,
        width: video_stream.width.unwrap_or(0),
        height: video_stream.height.unwrap_or(0),
        codec_name: video_stream
            .codec_name
            .clone()
            .unwrap_or_else(|| "unknown".to_owned()),
        pix_fmt: video_stream.pix_fmt.clone(),
        bit_depth: guess_bit_depth(
            video_stream.bits_per_raw_sample.as_deref(),
            video_stream.pix_fmt.as_deref(),
            video_stream.color_primaries.as_deref(),
        ),
        frame_rate: video_stream
            .r_frame_rate
            .as_deref()
            .and_then(parse_frame_rate),
        hdr10: detect_hdr10(ffprobe, source),
        audio_tracks: audio_tracks(&parsed.streams),
        subtitle_tracks: subtitle_tracks(&parsed.streams),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        FfprobeDisposition, FfprobeStream, FfprobeTags, audio_tracks, guess_bit_depth,
        parse_frame_rate, probe_video, subtitle_tracks,
    };
    use std::path::Path;

    #[test]
    fn prefers_reported_bits_per_raw_sample() {
        assert_eq!(guess_bit_depth(Some("12"), Some("yuv420p10le"), None), 12);
    }

    #[test]
    fn guesses_bit_depth_from_pix_fmt_when_not_reported() {
        assert_eq!(guess_bit_depth(None, Some("yuv420p"), None), 8);
        assert_eq!(guess_bit_depth(None, Some("yuv420p10le"), None), 10);
        assert_eq!(guess_bit_depth(None, Some("yuv420p12le"), None), 12);
    }

    #[test]
    fn falls_back_to_bt2020_heuristic_when_pix_fmt_is_unknown() {
        assert_eq!(guess_bit_depth(None, None, Some("bt2020")), 10);
        assert_eq!(guess_bit_depth(None, None, Some("bt709")), 8);
        assert_eq!(guess_bit_depth(None, None, None), 8);
    }

    #[test]
    fn parses_fractional_frame_rates() {
        assert_eq!(parse_frame_rate("30000/1001"), Some(30000.0 / 1001.0));
        assert_eq!(parse_frame_rate("25/1"), Some(25.0));
        assert_eq!(parse_frame_rate("25/0"), None);
        assert_eq!(parse_frame_rate("not-a-fraction"), None);
    }

    fn stream(codec_type: &str, codec_name: &str) -> FfprobeStream {
        FfprobeStream {
            codec_type: codec_type.to_owned(),
            codec_name: Some(codec_name.to_owned()),
            ..FfprobeStream::default()
        }
    }

    #[test]
    fn audio_and_subtitle_indices_are_type_relative_not_ffprobe_global_index() {
        // video, audio, subtitle, audio, subtitle — interspersed on purpose:
        // the second audio stream must land on type-relative index 1, not
        // ffprobe's global stream index (3).
        let streams = vec![
            stream("video", "hevc"),
            stream("audio", "aac"),
            stream("subtitle", "subrip"),
            stream("audio", "ac3"),
            stream("subtitle", "ass"),
        ];

        let audio = audio_tracks(&streams);
        assert_eq!(audio.len(), 2);
        assert_eq!(audio[0].index, 0);
        assert_eq!(audio[0].codec_name, "aac");
        assert_eq!(audio[1].index, 1);
        assert_eq!(audio[1].codec_name, "ac3");

        let subtitles = subtitle_tracks(&streams);
        assert_eq!(subtitles.len(), 2);
        assert_eq!(subtitles[0].index, 0);
        assert_eq!(subtitles[0].codec_name, "subrip");
        assert_eq!(subtitles[1].index, 1);
        assert_eq!(subtitles[1].codec_name, "ass");
    }

    #[test]
    fn audio_track_reads_language_title_channels_and_default_disposition() {
        let streams = vec![FfprobeStream {
            codec_type: "audio".to_owned(),
            codec_name: Some("aac".to_owned()),
            channels: Some(6),
            tags: FfprobeTags {
                language: Some("eng".to_owned()),
                title: Some("Commentary".to_owned()),
            },
            disposition: FfprobeDisposition {
                default: 1,
                ..FfprobeDisposition::default()
            },
            ..FfprobeStream::default()
        }];

        let audio = audio_tracks(&streams);
        assert_eq!(audio.len(), 1);
        assert_eq!(audio[0].channels, 6);
        assert_eq!(audio[0].language.as_deref(), Some("eng"));
        assert_eq!(audio[0].title.as_deref(), Some("Commentary"));
        assert!(audio[0].is_default);
    }

    #[test]
    fn audio_track_defaults_to_stereo_when_ffprobe_omits_channels() {
        let streams = vec![stream("audio", "opus")];
        assert_eq!(audio_tracks(&streams)[0].channels, 2);
    }

    // The tests below shell out to the real system ffprobe against the small
    // fixtures in tests/fixtures/, so they're real integration coverage of
    // `probe_video` end to end, not just the pure helpers above. Ignored by
    // default since they depend on ffprobe being on PATH.

    #[test]
    #[ignore = "requires system ffprobe"]
    fn probes_a_real_8bit_sdr_video() {
        let info = probe_video(Path::new("ffprobe"), "tests/fixtures/sdr_8bit.mp4")
            .expect("probe should succeed");
        assert_eq!(info.width, 320);
        assert_eq!(info.height, 240);
        assert_eq!(info.bit_depth, 8);
        assert!(!info.hdr10);
        assert!((1.5..2.5).contains(&info.duration_secs));
    }

    #[test]
    #[ignore = "requires system ffprobe"]
    fn probes_a_real_10bit_video() {
        let info = probe_video(Path::new("ffprobe"), "tests/fixtures/sdr_10bit.mp4")
            .expect("probe should succeed");
        assert_eq!(info.bit_depth, 10);
        assert_eq!(info.codec_name, "hevc");
    }
}
