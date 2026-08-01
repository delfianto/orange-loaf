use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

/// Everything `OrangeLoaf` currently knows how to show about a probed source
/// video. Deliberately small — only the fields the Convert screen actually
/// renders today; more fields join this (and the ffprobe deserialization
/// below) as the features that need them land.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    pub duration_secs: f64,
    pub width: u32,
    pub height: u32,
    pub codec_name: String,
    pub pix_fmt: Option<String>,
    pub bit_depth: u8,
    pub frame_rate: Option<f64>,
    /// Overall (format-level) bitrate, as ffprobe reports it — `None` for
    /// containers that don't carry one (e.g. some `.mkv`s leave it out).
    pub bitrate_bps: Option<u64>,
    pub color_space: Option<String>,
    pub color_primaries: Option<String>,
    pub color_transfer: Option<String>,
    pub hdr10: Option<Hdr10Metadata>,
    pub audio_tracks: Vec<AudioTrack>,
    pub subtitle_tracks: Vec<SubtitleTrack>,
}

/// HDR10 static metadata, in the plain units the source's mastering-display
/// SEI/OBU actually describes — chromaticity as CIE 1931 xy (0..1), luminance
/// in nits — rather than any one codec's own fixed-point wire scale. Each
/// target encoder's own formatter (`encoders::hdr`) converts from here into
/// whatever scale *that* codec's HDR string wants; ffprobe's reported
/// fractions are already codec-native scaled integers (HEVC: /50000
/// chromaticity, /10000 luminance; AV1: /65536 chromaticity, its own
/// luminance scale — verified empirically, not assumed identical).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hdr10Metadata {
    pub red_x: f64,
    pub red_y: f64,
    pub green_x: f64,
    pub green_y: f64,
    pub blue_x: f64,
    pub blue_y: f64,
    pub white_point_x: f64,
    pub white_point_y: f64,
    /// Nits.
    pub max_luminance: f64,
    /// Nits.
    pub min_luminance: f64,
    /// Content light level is separate SEI/OBU data from mastering-display
    /// and, in principle, independently present-or-absent — `None` when
    /// this source has mastering-display metadata but no CLL.
    pub max_content_light: Option<u32>,
    pub max_average_light: Option<u32>,
}

/// One audio stream as reported by ffprobe. `index` is *type-relative*
/// (0 = first audio stream) to match ffmpeg's own `-map 0:a:{index}`
/// addressing — deliberately not ffprobe's global stream `index`, which
/// counts video/audio/subtitle/data streams together.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioTrack {
    pub index: u32,
    pub codec_name: String,
    pub channels: u32,
    pub language: Option<String>,
    pub title: Option<String>,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    color_space: Option<String>,
    color_transfer: Option<String>,
    r_frame_rate: Option<String>,
    channels: Option<u32>,
    bit_rate: Option<String>,
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
    bit_rate: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct FfprobeFrameSideData {
    side_data_type: Option<String>,
    red_x: Option<String>,
    red_y: Option<String>,
    green_x: Option<String>,
    green_y: Option<String>,
    blue_x: Option<String>,
    blue_y: Option<String>,
    white_point_x: Option<String>,
    white_point_y: Option<String>,
    min_luminance: Option<String>,
    max_luminance: Option<String>,
    max_content: Option<u32>,
    max_average: Option<u32>,
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

    if let Some(reported) = bits_per_raw_sample.and_then(|raw| raw.parse::<u8>().ok())
        && reported > 0
    {
        return reported;
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

/// Parses ffprobe's `"num/den"` fraction strings (used for both frame rate
/// and, below, HDR10 mastering-display chromaticity/luminance) into a plain
/// f64.
fn parse_fraction(raw: &str) -> Option<f64> {
    let (num, den) = raw.split_once('/')?;
    let num: f64 = num.parse().ok()?;
    let den: f64 = den.parse().ok()?;
    if den == 0.0 { None } else { Some(num / den) }
}

/// Reads the *first* "Mastering display metadata" / "Content light level
/// metadata" side-data entries out of a probed frame's side-data list —
/// ffprobe exposes both as sibling entries in the same list, each present
/// independently of the other.
fn hdr10_from_side_data(side_data_list: &[FfprobeFrameSideData]) -> Option<Hdr10Metadata> {
    let mastering = side_data_list
        .iter()
        .find(|s| s.side_data_type.as_deref() == Some("Mastering display metadata"))?;
    let light_level = side_data_list
        .iter()
        .find(|s| s.side_data_type.as_deref() == Some("Content light level metadata"));

    Some(Hdr10Metadata {
        red_x: parse_fraction(mastering.red_x.as_deref()?)?,
        red_y: parse_fraction(mastering.red_y.as_deref()?)?,
        green_x: parse_fraction(mastering.green_x.as_deref()?)?,
        green_y: parse_fraction(mastering.green_y.as_deref()?)?,
        blue_x: parse_fraction(mastering.blue_x.as_deref()?)?,
        blue_y: parse_fraction(mastering.blue_y.as_deref()?)?,
        white_point_x: parse_fraction(mastering.white_point_x.as_deref()?)?,
        white_point_y: parse_fraction(mastering.white_point_y.as_deref()?)?,
        max_luminance: parse_fraction(mastering.max_luminance.as_deref()?)?,
        min_luminance: parse_fraction(mastering.min_luminance.as_deref()?)?,
        max_content_light: light_level.and_then(|l| l.max_content),
        max_average_light: light_level.and_then(|l| l.max_average),
    })
}

/// Runs ffprobe's frame-level probe on the first video stream and extracts
/// HDR10 static (mastering-display + content-light) metadata, if present.
/// This is a separate call from the main `-show_streams` probe because
/// this data is exposed by ffprobe as frame side-data, not a stream-level
/// field.
fn extract_hdr10(ffprobe: &Path, source: &str) -> Option<Hdr10Metadata> {
    let output = Command::new(ffprobe)
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
        .ok()?;
    let parsed: FfprobeFramesOutput = serde_json::from_slice(&output.stdout).ok()?;
    parsed
        .frames
        .iter()
        .find_map(|frame| hdr10_from_side_data(&frame.side_data_list))
}

/// Runs ffprobe's format+streams probe (the shape both [`probe_video`] and
/// [`probe_output_bitrates`] need) and parses its JSON output.
fn run_ffprobe_format_and_streams(ffprobe: &Path, source: &str) -> Result<FfprobeOutput, String> {
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

    serde_json::from_slice(&output.stdout)
        .map_err(|err| format!("failed to parse ffprobe output: {err}"))
}

/// Probes a source video with ffprobe and returns the subset of its
/// metadata `OrangeLoaf` currently uses.
///
/// # Errors
///
/// Returns an error if ffprobe can't be run, its output can't be parsed,
/// or the source has no real (non-attached-picture) video stream.
pub fn probe_video(ffprobe: &Path, source: &str) -> Result<VideoInfo, String> {
    let parsed = run_ffprobe_format_and_streams(ffprobe, source)?;

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
            .and_then(parse_fraction),
        bitrate_bps: parsed
            .format
            .bit_rate
            .as_deref()
            .and_then(|b| b.parse().ok()),
        color_space: video_stream.color_space.clone(),
        color_primaries: video_stream.color_primaries.clone(),
        color_transfer: video_stream.color_transfer.clone(),
        hdr10: extract_hdr10(ffprobe, source),
        audio_tracks: audio_tracks(&parsed.streams),
        subtitle_tracks: subtitle_tracks(&parsed.streams),
    })
}

/// The subset of an already-encoded *output* file's bitrates the naming
/// template's post-encode variables need (notes/plan.md M12) — deliberately
/// not folded into [`VideoInfo`], which describes a *source* being
/// configured for encoding, not a finished result being reported on.
// The shared `_bps` suffix is the unit, not accidental redundancy — losing
// it would make call sites like `stats.bitrates.video` ambiguous about
// scale (bps vs. the kbps naming-template output actually renders).
#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Copy, Default)]
pub struct OutputBitrates {
    pub overall_bps: Option<u64>,
    pub video_bps: Option<u64>,
    pub audio_bps: Option<u64>,
}

/// # Errors
///
/// Returns an error if ffprobe can't be run or its output can't be parsed.
pub fn probe_output_bitrates(ffprobe: &Path, source: &str) -> Result<OutputBitrates, String> {
    let parsed = run_ffprobe_format_and_streams(ffprobe, source)?;
    Ok(OutputBitrates {
        overall_bps: parsed
            .format
            .bit_rate
            .as_deref()
            .and_then(|b| b.parse().ok()),
        video_bps: parsed
            .streams
            .iter()
            .find(|s| is_real_video_stream(s))
            .and_then(|s| s.bit_rate.as_deref())
            .and_then(|b| b.parse().ok()),
        audio_bps: parsed
            .streams
            .iter()
            .find(|s| s.codec_type == "audio")
            .and_then(|s| s.bit_rate.as_deref())
            .and_then(|b| b.parse().ok()),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        FfprobeDisposition, FfprobeFrameSideData, FfprobeStream, FfprobeTags, audio_tracks,
        guess_bit_depth, hdr10_from_side_data, parse_fraction, probe_output_bitrates, probe_video,
        subtitle_tracks,
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
        assert_eq!(parse_fraction("30000/1001"), Some(30000.0 / 1001.0));
        assert_eq!(parse_fraction("25/1"), Some(25.0));
        assert_eq!(parse_fraction("25/0"), None);
        assert_eq!(parse_fraction("not-a-fraction"), None);
    }

    /// Real values captured from ffprobe against an actual x265-encoded
    /// HDR10 clip (`-x265-params master-display=G(8500,39850)B(6550,2300)
    /// R(35400,14600)WP(15635,16450)L(10000000,50):max-cll=1000,400`) — not
    /// guessed field names/scales.
    fn real_hevc_mastering_display_side_data() -> Vec<FfprobeFrameSideData> {
        vec![
            FfprobeFrameSideData {
                side_data_type: Some("Mastering display metadata".to_owned()),
                red_x: Some("35400/50000".to_owned()),
                red_y: Some("14600/50000".to_owned()),
                green_x: Some("8500/50000".to_owned()),
                green_y: Some("39850/50000".to_owned()),
                blue_x: Some("6550/50000".to_owned()),
                blue_y: Some("2300/50000".to_owned()),
                white_point_x: Some("15635/50000".to_owned()),
                white_point_y: Some("16450/50000".to_owned()),
                min_luminance: Some("50/10000".to_owned()),
                max_luminance: Some("10000000/10000".to_owned()),
                ..FfprobeFrameSideData::default()
            },
            FfprobeFrameSideData {
                side_data_type: Some("Content light level metadata".to_owned()),
                max_content: Some(1000),
                max_average: Some(400),
                ..FfprobeFrameSideData::default()
            },
        ]
    }

    // Exact equality is intentional, not sloppy: both sides are a *single*
    // division of the same integers, so IEEE-754's correctly-rounded
    // division guarantees a bit-identical result either way — this isn't
    // the classic "0.1 + 0.2" trap, which only bites when two independently
    // *rounded* values are combined.
    #[test]
    #[allow(clippy::float_cmp)]
    fn extracts_real_hevc_mastering_display_and_content_light_values() {
        let hdr10 = hdr10_from_side_data(&real_hevc_mastering_display_side_data())
            .expect("mastering display metadata should be found");
        assert_eq!(hdr10.red_x, 0.708);
        assert_eq!(hdr10.red_y, 0.292);
        assert_eq!(hdr10.green_x, 0.17);
        assert_eq!(hdr10.green_y, 0.797);
        assert_eq!(hdr10.blue_x, 0.131);
        assert_eq!(hdr10.blue_y, 0.046);
        assert_eq!(hdr10.white_point_x, 0.3127);
        assert_eq!(hdr10.white_point_y, 0.329);
        assert_eq!(hdr10.max_luminance, 1000.0);
        assert_eq!(hdr10.min_luminance, 0.005);
        assert_eq!(hdr10.max_content_light, Some(1000));
        assert_eq!(hdr10.max_average_light, Some(400));
    }

    #[test]
    fn mastering_display_without_a_sibling_content_light_entry_is_still_extracted() {
        let side_data = vec![
            real_hevc_mastering_display_side_data()
                .into_iter()
                .next()
                .unwrap(),
        ];
        let hdr10 = hdr10_from_side_data(&side_data).expect("should still extract");
        assert_eq!(hdr10.max_content_light, None);
        assert_eq!(hdr10.max_average_light, None);
    }

    #[test]
    fn no_mastering_display_entry_means_no_hdr10() {
        let side_data = vec![FfprobeFrameSideData {
            side_data_type: Some("Content light level metadata".to_owned()),
            max_content: Some(1000),
            max_average: Some(400),
            ..FfprobeFrameSideData::default()
        }];
        assert!(hdr10_from_side_data(&side_data).is_none());
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
        assert!(info.hdr10.is_none());
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

    /// `tests/fixtures/hdr10.mkv` was encoded with x265 with a hand-chosen
    /// master-display/max-cll string, so the expected values here are known
    /// ground truth, not just "whatever probing happens to return." Exact
    /// float equality is safe here for the same reason noted on
    /// `extracts_real_hevc_mastering_display_and_content_light_values`.
    #[test]
    #[ignore = "requires system ffprobe"]
    #[allow(clippy::float_cmp)]
    fn probes_real_hdr10_mastering_display_and_content_light() {
        let info = probe_video(Path::new("ffprobe"), "tests/fixtures/hdr10.mkv")
            .expect("probe should succeed");
        let hdr10 = info.hdr10.expect("fixture has HDR10 metadata");
        assert_eq!(hdr10.green_x, 8500.0 / 50000.0);
        assert_eq!(hdr10.green_y, 39850.0 / 50000.0);
        assert_eq!(hdr10.max_luminance, 10_000_000.0 / 10000.0);
        assert_eq!(hdr10.min_luminance, 50.0 / 10000.0);
        assert_eq!(hdr10.max_content_light, Some(1000));
        assert_eq!(hdr10.max_average_light, Some(400));
    }

    #[test]
    #[ignore = "requires system ffprobe"]
    fn probes_a_real_video_and_reports_bitrate_and_color_metadata() {
        let info = probe_video(Path::new("ffprobe"), "tests/fixtures/sdr_8bit.mp4")
            .expect("probe should succeed");
        // The fixture is a tiny synthetic clip — its exact bitrate isn't the
        // point, only that ffprobe's format-level figure actually made it
        // through into `VideoInfo` rather than being silently dropped.
        assert!(info.bitrate_bps.is_some());
    }

    #[test]
    #[ignore = "requires system ffprobe"]
    fn probe_output_bitrates_reports_real_overall_and_per_stream_values() {
        let bitrates = probe_output_bitrates(Path::new("ffprobe"), "tests/fixtures/sdr_8bit.mp4")
            .expect("probe should succeed");
        assert!(bitrates.overall_bps.is_some());
        assert!(bitrates.video_bps.is_some());
    }
}
