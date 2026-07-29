//! Output filename templating (notes/plan.md M12). Two phases, mirroring
//! the prior art's own "placeholder-then-rename" design — kept because it
//! solves a real ordering problem, not because it's a familiar shape: the
//! output file must exist under *some* name before ffmpeg runs, but stats
//! like achieved file size are only known once it's done.
//!
//! - **Pre-encode** ([`resolve_pre_encode`]): `{variable}` placeholders
//!   resolve immediately from the source path/encoder settings/probed
//!   video. Any *post-encode* variable present in the template is left in
//!   the resolved filename as a literal, filesystem-safe placeholder token
//!   (`FFFSIZE`, ...) rather than being dropped or resolved to a guess.
//! - **Post-encode** ([`resolve_post_encode`]): once the output file
//!   actually exists, those placeholder tokens are string-replaced with
//!   the real computed values; the caller renames the file to match.
//!
//! Datetimes are UTC, not the user's local time zone — `time`'s local-time
//! lookup is gated behind a documented soundness caveat (unsound in a
//! multi-threaded process on some platforms), which isn't worth taking on
//! for what is, ultimately, just a filename timestamp.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, SystemTime};

use time::OffsetDateTime;

use crate::encoders::EncoderKind;
use crate::filters::FilterSpec;
use crate::probe::{OutputBitrates, VideoInfo};

/// Pre-encode variable name -> the literal placeholder token baked into the
/// filename when that name is used, standing in until [`resolve_post_encode`]
/// replaces it for real. Uppercase-and-`FF`-prefixed so it can never collide
/// with a real substituted value.
const POST_ENCODE_TOKENS: &[(&str, &str)] = &[
    ("encode_time", "FFETIME"),
    ("encode_end", "FFEEND"),
    ("filesize", "FFFSIZE"),
    ("filesize_mb", "FFFSMB"),
    ("video_bitrate", "FFVIDBIT"),
    ("audio_bitrate", "FFAUDBIT"),
    ("overall_bitrate", "FFALLBIT"),
];

/// `(name, description, example)` — the single source of truth for both the
/// Settings UI's variable chips and [`generate_example_preview`], so the two
/// can't silently drift apart.
const PRE_ENCODE_VARIABLE_INFO: &[(&str, &str, &str)] = &[
    ("source", "Source filename stem", "MyMovie"),
    (
        "datetime",
        "Date + time (UTC, queue time)",
        "2026-02-13T14-30-00",
    ),
    ("date", "Date only (UTC, queue time)", "2026-02-13"),
    ("time", "Time only (UTC, queue time)", "14-30-00"),
    ("rand_4", "4-char random hex", "a3f1"),
    ("rand_8", "8-char random hex", "a3f1b2c4"),
    ("encoder", "Short encoder id", "x265"),
    ("codec", "Full codec label", "HEVC (x265)"),
    ("preset", "Encoder preset", "medium"),
    ("crf", "CRF/quality value", "22"),
    (
        "bitrate",
        "Target bitrate (no encoder uses bitrate mode yet)",
        "N-A",
    ),
    ("pix_fmt", "Pixel format", "yuv420p10le"),
    (
        "resolution",
        "Output resolution, after crop/scale/rotate",
        "1920x1080",
    ),
    ("source_resolution", "Source resolution", "3840x2160"),
    ("source_duration", "Source duration", "01-23-45"),
    ("source_fps", "Source frame rate", "23.976"),
    ("source_codec", "Source video codec", "hevc"),
    ("source_bitrate", "Source overall bitrate", "45000k"),
    ("audio_codec", "First audio track's codec", "aac"),
    (
        "audio_channels",
        "First audio track's channel layout",
        "5.1",
    ),
    ("color_space", "Color space", "bt2020nc"),
    ("color_primaries", "Color primaries", "bt2020"),
    ("hdr", "HDR type, or SDR", "HDR10"),
];

const POST_ENCODE_VARIABLE_INFO: &[(&str, &str, &str)] = &[
    ("encode_time", "Encode duration", "00-15-32"),
    (
        "encode_end",
        "Finish timestamp (UTC)",
        "2026-02-13T14-45-32",
    ),
    ("filesize", "Human-readable output size", "1.5GB"),
    ("filesize_mb", "Output size in MB", "1536"),
    ("video_bitrate", "Achieved video bitrate", "4523k"),
    ("audio_bitrate", "Achieved audio bitrate", "320k"),
    ("overall_bitrate", "Achieved overall bitrate", "4843k"),
];

/// One entry in the naming-variable reference list the Settings UI renders
/// as clickable chips.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NamingVariableInfo {
    pub name: String,
    pub description: String,
    pub example: String,
    pub post_encode: bool,
}

#[must_use]
pub fn list_variables() -> Vec<NamingVariableInfo> {
    PRE_ENCODE_VARIABLE_INFO
        .iter()
        .map(|&(name, description, example)| NamingVariableInfo {
            name: name.to_owned(),
            description: description.to_owned(),
            example: example.to_owned(),
            post_encode: false,
        })
        .chain(
            POST_ENCODE_VARIABLE_INFO
                .iter()
                .map(|&(name, description, example)| NamingVariableInfo {
                    name: name.to_owned(),
                    description: description.to_owned(),
                    example: example.to_owned(),
                    post_encode: true,
                }),
        )
        .collect()
}

/// Renders `template` using canned example values (no real video needed) —
/// powers the Settings screen's live preview.
#[must_use]
pub fn generate_example_preview(template: &str) -> String {
    let variables: HashMap<&str, String> = PRE_ENCODE_VARIABLE_INFO
        .iter()
        .chain(POST_ENCODE_VARIABLE_INFO.iter())
        .map(|&(name, _, example)| (name, example.to_owned()))
        .collect();
    substitute(template, &variables)
}

/// Replaces every well-formed `{word}` placeholder found in `variables`;
/// anything else (an unknown name, or a `{`/`}` that isn't a clean
/// identifier) is left exactly as written rather than dropped, so a typo in
/// a user's template fails loudly (a literal `{typo}` in the resulting
/// filename) instead of silently vanishing.
fn substitute(template: &str, variables: &HashMap<&str, String>) -> String {
    let mut result = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        result.push_str(&rest[..open]);
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find('}') else {
            result.push('{');
            rest = after_open;
            continue;
        };
        let name = &after_open[..close];
        let is_identifier =
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if is_identifier {
            if let Some(value) = variables.get(name) {
                result.push_str(value);
            } else {
                result.push('{');
                result.push_str(name);
                result.push('}');
            }
        } else {
            result.push('{');
            result.push_str(name);
            result.push('}');
        }
        rest = &after_open[close + 1..];
    }
    result.push_str(rest);
    result
}

/// Replaces characters invalid (or awkward) in filenames on any of
/// Windows/macOS/Linux with `-`, so a value drawn from real-world media
/// (titles/languages/etc. can contain almost anything) can't produce an
/// unwritable path.
fn sanitize_component(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect()
}

/// Filesystems commonly cap filenames around 255 bytes; this stays well
/// under that without trying to reproduce the prior art's exact
/// directory-aware budget math (notes/plan.md M12 scope note).
const MAX_STEM_LEN: usize = 200;

fn truncate_stem(stem: &str) -> String {
    if stem.chars().count() <= MAX_STEM_LEN {
        stem.to_owned()
    } else {
        stem.chars().take(MAX_STEM_LEN).collect()
    }
}

fn channel_layout_label(channels: u32) -> String {
    match channels {
        1 => "mono".to_owned(),
        2 => "stereo".to_owned(),
        6 => "5.1".to_owned(),
        8 => "7.1".to_owned(),
        other => other.to_string(),
    }
}

/// Voltaic only ever detects HDR10 (mastering-display SEI/OBU) today — see
/// `probe::Hdr10Metadata` and notes/plan.md M8's scope note on HDR10+/Dolby
/// Vision. The HLG check is a real, independently-probed signal (the
/// transfer characteristic ffprobe reports), not a guess.
fn hdr_label(video: &VideoInfo) -> &'static str {
    if video.hdr10.is_some() {
        return "HDR10";
    }
    match video.color_transfer.as_deref() {
        Some("arib-std-b67") => "HLG",
        _ => "SDR",
    }
}

fn format_bitrate_bps(bps: u64) -> String {
    format!("{}k", bps / 1000)
}

fn format_filesize(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    #[allow(clippy::cast_precision_loss)]
    let bytes_f = bytes as f64;
    if bytes_f >= GIB {
        format!("{:.1}GB", bytes_f / GIB)
    } else if bytes_f >= MIB {
        format!("{:.1}MB", bytes_f / MIB)
    } else {
        format!("{:.1}KB", bytes_f / KIB)
    }
}

fn format_duration_hms(total_secs: f64) -> String {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let total = total_secs.max(0.0).round() as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    format!("{hours:02}-{minutes:02}-{seconds:02}")
}

fn format_fps(fps: f64) -> String {
    format!("{fps:.3}")
}

/// Trims a fixed-precision rendering down to its meaningful digits, so `22`
/// prints as `"22"` and `22.5` prints as `"22.5"` — avoids relying on
/// float `Display`'s own (version-sensitive) trailing-zero behavior.
fn format_trimmed(value: f64) -> String {
    let rendered = format!("{value:.2}");
    let trimmed = rendered.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn format_date_parts(dt: OffsetDateTime) -> (i32, u8, u8, u8, u8, u8) {
    (
        dt.year(),
        u8::from(dt.month()),
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second(),
    )
}

fn format_datetime(dt: OffsetDateTime) -> String {
    let (y, mo, d, h, mi, s) = format_date_parts(dt);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}-{mi:02}-{s:02}")
}

fn format_date(dt: OffsetDateTime) -> String {
    let (y, mo, d, ..) = format_date_parts(dt);
    format!("{y:04}-{mo:02}-{d:02}")
}

fn format_time(dt: OffsetDateTime) -> String {
    let (.., h, mi, s) = format_date_parts(dt);
    format!("{h:02}-{mi:02}-{s:02}")
}

/// Computes the *output* resolution implied by `filters` (crop, then scale
/// — matching `FilterSpec::render`'s own stage order — then a rotate-90/270
/// axis swap), falling back at each stage to the source dimensions when
/// that stage isn't configured. Pure and independent of any actual ffmpeg
/// run, so it's exactly as accurate as the filter graph itself is.
fn resolve_output_resolution(
    source_width: u32,
    source_height: u32,
    filters: &FilterSpec,
) -> (u32, u32) {
    let (cropped_w, cropped_h) =
        filters
            .crop
            .as_ref()
            .map_or((source_width, source_height), |crop| {
                (
                    source_width.saturating_sub(crop.left + crop.right),
                    source_height.saturating_sub(crop.top + crop.bottom),
                )
            });

    let (scaled_w, scaled_h) = match (filters.scale_width, filters.scale_height) {
        (None, None) => (cropped_w, cropped_h),
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, scale_other_axis(w, cropped_w, cropped_h)),
        (None, Some(h)) => (scale_other_axis(h, cropped_h, cropped_w), h),
    };

    if matches!(filters.rotate_degrees % 360, 90 | 270) {
        (scaled_h, scaled_w)
    } else {
        (scaled_w, scaled_h)
    }
}

/// Given one known output dimension and the source's own two dimensions
/// (the axis matching `known`, and the other one), derives the other output
/// dimension by preserving the source aspect ratio — the same math
/// `-2`-in-the-scale-filter performs at encode time (`filters.rs`).
fn scale_other_axis(known: u32, source_same_axis: u32, source_other_axis: u32) -> u32 {
    if source_same_axis == 0 {
        return source_other_axis;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let derived = (f64::from(known) * f64::from(source_other_axis) / f64::from(source_same_axis))
        .round() as u32;
    derived
}

/// Everything [`resolve_pre_encode`] needs beyond the template string
/// itself. `video` is `None` when a template is being resolved (e.g. a
/// live Settings-screen preview) without any source actually loaded.
pub struct PreEncodeInputs<'a> {
    pub source_path: &'a str,
    pub settings: &'a EncoderKind,
    pub filters: &'a FilterSpec,
    pub video: Option<&'a VideoInfo>,
}

fn insert_identity_and_random_variables(variables: &mut HashMap<&str, String>, source_path: &str) {
    let now = OffsetDateTime::now_utc();
    let source_stem = Path::new(source_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(source_path);
    variables.insert("source", source_stem.to_owned());
    variables.insert("datetime", format_datetime(now));
    variables.insert("date", format_date(now));
    variables.insert("time", format_time(now));
    let rand = uuid::Uuid::new_v4().simple().to_string();
    variables.insert("rand_4", rand[..4].to_owned());
    variables.insert("rand_8", rand[..8].to_owned());
}

fn insert_encoder_variables(variables: &mut HashMap<&str, String>, settings: &EncoderKind) {
    variables.insert("encoder", settings.encoder_id().to_owned());
    variables.insert("codec", settings.display_label().to_owned());
    variables.insert(
        "preset",
        settings
            .preset()
            .map_or_else(|| "N-A".to_owned(), ToOwned::to_owned),
    );
    variables.insert(
        "crf",
        settings
            .crf()
            .map_or_else(|| "N-A".to_owned(), |c| format_trimmed(f64::from(c))),
    );
    variables.insert(
        "bitrate",
        settings.target_bitrate().map_or_else(
            || "N-A".to_owned(),
            |b| format!("{}k", (b / 1000.0).round()),
        ),
    );
    variables.insert(
        "pix_fmt",
        settings
            .pix_fmt()
            .map_or_else(|| "N-A".to_owned(), ToOwned::to_owned),
    );
}

/// `None` fills in `"N-A"` for every video-derived variable — used both
/// when no source has been probed at all, and (via `Some`) for the real
/// values once one has.
fn insert_video_variables(
    variables: &mut HashMap<&str, String>,
    video: Option<&VideoInfo>,
    filters: &FilterSpec,
) {
    let Some(video) = video else {
        for key in [
            "source_resolution",
            "resolution",
            "source_duration",
            "source_fps",
            "source_codec",
            "source_bitrate",
            "color_space",
            "color_primaries",
            "hdr",
            "audio_codec",
            "audio_channels",
        ] {
            variables.insert(key, "N-A".to_owned());
        }
        return;
    };

    variables.insert(
        "source_resolution",
        format!("{}x{}", video.width, video.height),
    );
    let (out_w, out_h) = resolve_output_resolution(video.width, video.height, filters);
    variables.insert("resolution", format!("{out_w}x{out_h}"));
    variables.insert("source_duration", format_duration_hms(video.duration_secs));
    variables.insert(
        "source_fps",
        video
            .frame_rate
            .map_or_else(|| "N-A".to_owned(), format_fps),
    );
    variables.insert("source_codec", video.codec_name.clone());
    variables.insert(
        "source_bitrate",
        video
            .bitrate_bps
            .map_or_else(|| "N-A".to_owned(), format_bitrate_bps),
    );
    variables.insert(
        "color_space",
        video
            .color_space
            .clone()
            .unwrap_or_else(|| "N-A".to_owned()),
    );
    variables.insert(
        "color_primaries",
        video
            .color_primaries
            .clone()
            .unwrap_or_else(|| "N-A".to_owned()),
    );
    variables.insert("hdr", hdr_label(video).to_owned());
    if let Some(first_audio) = video.audio_tracks.first() {
        variables.insert("audio_codec", first_audio.codec_name.clone());
        variables.insert("audio_channels", channel_layout_label(first_audio.channels));
    } else {
        variables.insert("audio_codec", "N-A".to_owned());
        variables.insert("audio_channels", "N-A".to_owned());
    }
}

/// Resolves every pre-encode `{variable}` in `template`. Post-encode
/// variables (`{filesize}` and friends) resolve to their literal
/// placeholder token, to be replaced for real by [`resolve_post_encode`]
/// once the output actually exists — see this module's own doc comment.
#[must_use]
pub fn resolve_pre_encode(template: &str, inputs: &PreEncodeInputs<'_>) -> String {
    let mut variables: HashMap<&str, String> = HashMap::new();

    insert_identity_and_random_variables(&mut variables, inputs.source_path);
    insert_encoder_variables(&mut variables, inputs.settings);
    insert_video_variables(&mut variables, inputs.video, inputs.filters);
    for &(name, token) in POST_ENCODE_TOKENS {
        variables.insert(name, token.to_owned());
    }

    let sanitized: HashMap<&str, String> = variables
        .into_iter()
        .map(|(k, v)| (k, sanitize_component(&v)))
        .collect();
    truncate_stem(&substitute(template, &sanitized))
}

/// Whether `name` (a filename stem already produced by
/// [`resolve_pre_encode`]) contains any post-encode placeholder token —
/// lets a caller skip the extra output-ffprobe pass entirely in the (most)
/// common case where a template only uses pre-encode variables.
#[must_use]
pub fn has_post_encode_placeholders(name: &str) -> bool {
    POST_ENCODE_TOKENS
        .iter()
        .any(|&(_, token)| name.contains(token))
}

/// Real, achieved stats about a finished output file — gathered by the
/// caller (file size via `std::fs::metadata`, bitrates via
/// `probe::probe_output_bitrates`) since this module has no I/O of its own.
#[derive(Debug, Clone, Copy, Default)]
pub struct OutputStats {
    pub size_bytes: u64,
    pub bitrates: OutputBitrates,
}

/// Replaces every post-encode placeholder token present in `name` with its
/// real, now-known value.
#[must_use]
pub fn resolve_post_encode(
    name: &str,
    stats: OutputStats,
    elapsed: Duration,
    finished_at: SystemTime,
) -> String {
    name.replace("FFETIME", &format_duration_hms(elapsed.as_secs_f64()))
        .replace(
            "FFEEND",
            &format_datetime(OffsetDateTime::from(finished_at)),
        )
        .replace("FFFSIZE", &format_filesize(stats.size_bytes))
        .replace("FFFSMB", &(stats.size_bytes / (1024 * 1024)).to_string())
        .replace(
            "FFVIDBIT",
            &stats
                .bitrates
                .video_bps
                .map_or_else(|| "N-A".to_owned(), format_bitrate_bps),
        )
        .replace(
            "FFAUDBIT",
            &stats
                .bitrates
                .audio_bps
                .map_or_else(|| "N-A".to_owned(), format_bitrate_bps),
        )
        .replace(
            "FFALLBIT",
            &stats
                .bitrates
                .overall_bps
                .map_or_else(|| "N-A".to_owned(), format_bitrate_bps),
        )
}

/// Ties the pieces together for one finished output file, shared by both
/// the direct-encode path (`commands::start_encode`) and the queue runner
/// (`queue::dispatch_next`) so post-encode naming can't drift between the
/// two. Skips the extra ffprobe pass entirely when the filename has no
/// post-encode placeholders at all (the common case). Best-effort
/// throughout: any failure (source vanished, ffprobe unavailable, rename
/// denied) leaves the file under its already-valid placeholder-token name
/// rather than losing or corrupting it — returns the original path
/// unchanged in that case.
#[must_use]
pub fn finalize_output_naming(ffprobe: &Path, output_path: &str, started_at: SystemTime) -> String {
    let path = Path::new(output_path);
    let (Some(parent), Some(file_stem), Some(extension)) = (
        path.parent(),
        path.file_stem().and_then(|s| s.to_str()),
        path.extension().and_then(|s| s.to_str()),
    ) else {
        return output_path.to_owned();
    };
    if !has_post_encode_placeholders(file_stem) {
        return output_path.to_owned();
    }

    let Ok(metadata) = std::fs::metadata(path) else {
        return output_path.to_owned();
    };
    let bitrates = crate::probe::probe_output_bitrates(ffprobe, output_path).unwrap_or_default();
    let stats = OutputStats {
        size_bytes: metadata.len(),
        bitrates,
    };
    let elapsed = SystemTime::now()
        .duration_since(started_at)
        .unwrap_or_default();
    let new_stem = resolve_post_encode(file_stem, stats, elapsed, SystemTime::now());
    let new_path = parent.join(format!("{new_stem}.{extension}"));

    match std::fs::rename(path, &new_path) {
        Ok(()) => new_path.display().to_string(),
        Err(_) => output_path.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        OutputStats, PreEncodeInputs, channel_layout_label, finalize_output_naming,
        generate_example_preview, has_post_encode_placeholders, hdr_label, list_variables,
        resolve_output_resolution, resolve_post_encode, resolve_pre_encode, sanitize_component,
        substitute, truncate_stem,
    };
    use crate::encoders::EncoderKind;
    use crate::encoders::svt_av1::SvtAv1Settings;
    use crate::encoders::videotoolbox::H264VideoToolboxSettings;
    use crate::encoders::x265::X265Settings;
    use crate::filters::{Crop, FilterSpec};
    use crate::probe::{AudioTrack, Hdr10Metadata, OutputBitrates, SubtitleTrack, VideoInfo};
    use std::collections::HashMap;
    use std::time::{Duration, SystemTime};

    fn sample_video() -> VideoInfo {
        VideoInfo {
            duration_secs: 5025.0, // 1h 23m 45s
            width: 3840,
            height: 2160,
            codec_name: "hevc".to_owned(),
            pix_fmt: Some("yuv420p10le".to_owned()),
            bit_depth: 10,
            frame_rate: Some(23.976),
            bitrate_bps: Some(45_000_000),
            color_space: Some("bt2020nc".to_owned()),
            color_primaries: Some("bt2020".to_owned()),
            color_transfer: Some("smpte2084".to_owned()),
            hdr10: Some(Hdr10Metadata {
                red_x: 0.708,
                red_y: 0.292,
                green_x: 0.17,
                green_y: 0.797,
                blue_x: 0.131,
                blue_y: 0.046,
                white_point_x: 0.3127,
                white_point_y: 0.329,
                max_luminance: 1000.0,
                min_luminance: 0.005,
                max_content_light: Some(1000),
                max_average_light: Some(400),
            }),
            audio_tracks: vec![AudioTrack {
                index: 0,
                codec_name: "eac3".to_owned(),
                channels: 6,
                language: Some("eng".to_owned()),
                title: None,
                is_default: true,
            }],
            subtitle_tracks: Vec::<SubtitleTrack>::new(),
        }
    }

    #[test]
    fn substitute_replaces_known_variables_and_leaves_unknown_ones_literal() {
        let mut vars = HashMap::new();
        vars.insert("source", "Movie".to_owned());
        assert_eq!(substitute("{source}-{unknown}", &vars), "Movie-{unknown}");
    }

    #[test]
    fn substitute_leaves_an_unclosed_brace_literal() {
        let vars = HashMap::new();
        assert_eq!(substitute("a{b", &vars), "a{b");
    }

    #[test]
    fn substitute_ignores_non_identifier_braces() {
        let vars = HashMap::new();
        assert_eq!(substitute("{not an id}", &vars), "{not an id}");
    }

    #[test]
    fn sanitize_replaces_path_hostile_characters() {
        assert_eq!(
            sanitize_component("a/b\\c:d*e?f\"g<h>i|j"),
            "a-b-c-d-e-f-g-h-i-j"
        );
    }

    #[test]
    fn sanitize_leaves_ordinary_characters_alone() {
        assert_eq!(sanitize_component("My Movie (2026)"), "My Movie (2026)");
    }

    #[test]
    fn truncate_stem_leaves_short_names_alone() {
        assert_eq!(truncate_stem("short"), "short");
    }

    #[test]
    fn truncate_stem_caps_very_long_names() {
        let long = "a".repeat(500);
        assert_eq!(truncate_stem(&long).len(), 200);
    }

    #[test]
    fn channel_layout_maps_common_counts_to_names() {
        assert_eq!(channel_layout_label(1), "mono");
        assert_eq!(channel_layout_label(2), "stereo");
        assert_eq!(channel_layout_label(6), "5.1");
        assert_eq!(channel_layout_label(8), "7.1");
        assert_eq!(channel_layout_label(4), "4");
    }

    #[test]
    fn hdr_label_prefers_hdr10_then_hlg_then_sdr() {
        let mut video = sample_video();
        assert_eq!(hdr_label(&video), "HDR10");

        video.hdr10 = None;
        video.color_transfer = Some("arib-std-b67".to_owned());
        assert_eq!(hdr_label(&video), "HLG");

        video.color_transfer = Some("bt709".to_owned());
        assert_eq!(hdr_label(&video), "SDR");
    }

    #[test]
    fn output_resolution_falls_back_to_source_when_nothing_is_configured() {
        assert_eq!(
            resolve_output_resolution(1920, 1080, &FilterSpec::default()),
            (1920, 1080)
        );
    }

    #[test]
    fn output_resolution_reflects_crop() {
        let filters = FilterSpec {
            crop: Some(Crop {
                top: 100,
                bottom: 100,
                left: 0,
                right: 0,
            }),
            ..FilterSpec::default()
        };
        assert_eq!(resolve_output_resolution(1920, 1080, &filters), (1920, 880));
    }

    #[test]
    fn output_resolution_derives_the_unset_scale_axis_from_source_aspect_ratio() {
        let filters = FilterSpec {
            scale_width: Some(1280),
            ..FilterSpec::default()
        };
        assert_eq!(resolve_output_resolution(1920, 1080, &filters), (1280, 720));
    }

    #[test]
    fn output_resolution_uses_both_scale_dimensions_verbatim_when_both_are_set() {
        let filters = FilterSpec {
            scale_width: Some(640),
            scale_height: Some(480),
            ..FilterSpec::default()
        };
        assert_eq!(resolve_output_resolution(1920, 1080, &filters), (640, 480));
    }

    #[test]
    fn output_resolution_swaps_axes_on_a_90_or_270_degree_rotation() {
        let rotated_90 = FilterSpec {
            rotate_degrees: 90,
            ..FilterSpec::default()
        };
        assert_eq!(
            resolve_output_resolution(1920, 1080, &rotated_90),
            (1080, 1920)
        );

        let rotated_270 = FilterSpec {
            rotate_degrees: 270,
            ..FilterSpec::default()
        };
        assert_eq!(
            resolve_output_resolution(1920, 1080, &rotated_270),
            (1080, 1920)
        );

        let rotated_180 = FilterSpec {
            rotate_degrees: 180,
            ..FilterSpec::default()
        };
        assert_eq!(
            resolve_output_resolution(1920, 1080, &rotated_180),
            (1920, 1080)
        );
    }

    #[test]
    fn resolve_pre_encode_substitutes_every_documented_pre_encode_variable() {
        let settings = EncoderKind::X265(X265Settings::default());
        let filters = FilterSpec::default();
        let video = sample_video();
        let inputs = PreEncodeInputs {
            source_path: "/movies/My Movie.mkv",
            settings: &settings,
            filters: &filters,
            video: Some(&video),
        };

        for var in list_variables().into_iter().filter(|v| !v.post_encode) {
            let template = format!("{{{}}}", var.name);
            let resolved = resolve_pre_encode(&template, &inputs);
            assert_ne!(
                resolved, template,
                "variable {{{}}} did not resolve to anything",
                var.name
            );
            assert!(
                !resolved.contains('{'),
                "variable {{{}}} left a stray brace: {resolved}",
                var.name
            );
        }
    }

    #[test]
    fn resolve_pre_encode_reads_source_from_the_file_stem() {
        let settings = EncoderKind::X265(X265Settings::default());
        let filters = FilterSpec::default();
        let inputs = PreEncodeInputs {
            source_path: "/movies/My Movie.mkv",
            settings: &settings,
            filters: &filters,
            video: None,
        };
        assert_eq!(resolve_pre_encode("{source}", &inputs), "My Movie");
    }

    #[test]
    fn resolve_pre_encode_reports_n_a_for_every_video_variable_when_no_video_is_loaded() {
        let settings = EncoderKind::SvtAv1(SvtAv1Settings::default());
        let filters = FilterSpec::default();
        let inputs = PreEncodeInputs {
            source_path: "clip.mp4",
            settings: &settings,
            filters: &filters,
            video: None,
        };
        assert_eq!(resolve_pre_encode("{source_resolution}", &inputs), "N-A");
        assert_eq!(resolve_pre_encode("{hdr}", &inputs), "N-A");
        // svt-av1 has no preset/pix_fmt concept either way, video or not.
        assert_eq!(resolve_pre_encode("{preset}", &inputs), "N-A");
        assert_eq!(resolve_pre_encode("{pix_fmt}", &inputs), "N-A");
    }

    #[test]
    fn resolve_pre_encode_reads_encoder_and_quality_fields() {
        let settings = EncoderKind::X265(X265Settings::default());
        let filters = FilterSpec::default();
        let inputs = PreEncodeInputs {
            source_path: "clip.mp4",
            settings: &settings,
            filters: &filters,
            video: None,
        };
        assert_eq!(resolve_pre_encode("{encoder}", &inputs), "x265");
        assert_eq!(resolve_pre_encode("{codec}", &inputs), "HEVC (x265)");
        assert_eq!(resolve_pre_encode("{preset}", &inputs), "medium");
        assert_eq!(resolve_pre_encode("{crf}", &inputs), "22");
    }

    #[test]
    fn resolve_pre_encode_reads_first_audio_track_codec_and_channel_layout() {
        let settings = EncoderKind::X265(X265Settings::default());
        let filters = FilterSpec::default();
        let video = sample_video();
        let inputs = PreEncodeInputs {
            source_path: "clip.mp4",
            settings: &settings,
            filters: &filters,
            video: Some(&video),
        };
        assert_eq!(resolve_pre_encode("{audio_codec}", &inputs), "eac3");
        assert_eq!(resolve_pre_encode("{audio_channels}", &inputs), "5.1");
    }

    #[test]
    fn resolve_pre_encode_leaves_post_encode_placeholders_as_literal_tokens() {
        let settings = EncoderKind::H264Videotoolbox(H264VideoToolboxSettings::default());
        let filters = FilterSpec::default();
        let inputs = PreEncodeInputs {
            source_path: "clip.mp4",
            settings: &settings,
            filters: &filters,
            video: None,
        };
        let resolved = resolve_pre_encode("{source}-{filesize}", &inputs);
        assert_eq!(resolved, "clip-FFFSIZE");
        assert!(has_post_encode_placeholders(&resolved));
    }

    #[test]
    fn resolve_pre_encode_sanitizes_path_hostile_source_stems() {
        let settings = EncoderKind::X265(X265Settings::default());
        let filters = FilterSpec::default();
        let inputs = PreEncodeInputs {
            source_path: "weird:name?.mp4",
            settings: &settings,
            filters: &filters,
            video: None,
        };
        assert_eq!(resolve_pre_encode("{source}", &inputs), "weird-name-");
    }

    #[test]
    fn has_post_encode_placeholders_is_false_for_an_ordinary_resolved_name() {
        assert!(!has_post_encode_placeholders("My Movie-x265-crf22"));
    }

    #[test]
    fn resolve_post_encode_replaces_every_token_with_a_real_value() {
        let stats = OutputStats {
            size_bytes: 1_610_612_736, // 1.5 GiB
            bitrates: OutputBitrates {
                overall_bps: Some(4_843_000),
                video_bps: Some(4_523_000),
                audio_bps: Some(320_000),
            },
        };
        let elapsed = Duration::from_secs(15 * 60 + 32);
        let finished_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_770_000_000);
        let name = "Movie-FFETIME-FFEEND-FFFSIZE-FFFSMB-FFVIDBIT-FFAUDBIT-FFALLBIT";
        let resolved = resolve_post_encode(name, stats, elapsed, finished_at);
        assert!(
            !resolved.contains("FF"),
            "a token was left unresolved: {resolved}"
        );
        assert!(resolved.contains("00-15-32"));
        assert!(resolved.contains("1.5GB"));
        assert!(resolved.contains("1536"));
        assert!(resolved.contains("4523k"));
        assert!(resolved.contains("320k"));
        assert!(resolved.contains("4843k"));
    }

    #[test]
    fn resolve_post_encode_reports_n_a_for_missing_bitrates() {
        let stats = OutputStats {
            size_bytes: 1024,
            bitrates: OutputBitrates::default(),
        };
        let resolved =
            resolve_post_encode("FFVIDBIT", stats, Duration::from_secs(1), SystemTime::now());
        assert_eq!(resolved, "N-A");
    }

    #[test]
    #[ignore = "requires system ffprobe"]
    fn finalize_output_naming_renames_a_real_file_and_resolves_placeholders() {
        let dir =
            std::env::temp_dir().join(format!("voltaic-naming-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("should create scratch dir");
        let original = dir.join("Movie-FFFSIZE.mkv");
        std::fs::write(
            &original,
            b"not a real video, just needs to exist as a file",
        )
        .expect("should write scratch file");

        let result = finalize_output_naming(
            std::path::Path::new("ffprobe"),
            original.to_str().unwrap(),
            SystemTime::now(),
        );

        assert!(
            !result.contains("FFFSIZE"),
            "placeholder left unresolved: {result}"
        );
        assert!(std::path::Path::new(&result).exists());
        assert!(
            !original.exists(),
            "the placeholder-named file should have been renamed away"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn generate_example_preview_resolves_every_variable_with_canned_values() {
        let template: String = list_variables()
            .into_iter()
            .map(|v| format!("{{{}}}", v.name))
            .collect::<Vec<_>>()
            .join("_");
        let preview = generate_example_preview(&template);
        assert!(
            !preview.contains('{'),
            "left a variable unresolved: {preview}"
        );
    }
}
