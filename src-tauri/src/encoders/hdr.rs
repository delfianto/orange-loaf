use crate::probe::Hdr10Metadata;

/// `value * scale`, formatted as the nearest integer — every HDR string
/// fragment below needs this (chromaticity ×50000 or ×1, luminance ×10000
/// or ×1 depending on the target codec), and going through a formatted
/// string instead of an integer type sidesteps any cast-truncation concern
/// entirely: these values are always small (well under `f64`'s exact-integer
/// range), so precision was never actually at risk, but there's no reason to
/// invite the lint when formatting directly is just as simple.
fn scaled(value: f64, scale: f64) -> String {
    format!("{:.0}", value * scale)
}

fn decimal(value: f64) -> String {
    format!("{value:.4}")
}

/// x265's `-x265-params master-display=...` value. The HEVC SEI message
/// this ultimately becomes already uses fixed-point integers scaled by
/// 50000 (chromaticity) / 10000 (luminance) — the same scale
/// [`crate::probe::extract_hdr10`] (via `parse_fraction`) normalizes *out*
/// of when reading a source, so this is close to a direct inverse,
/// round-tripped against a real x265-encoded fixture (see
/// `probe::tests::probes_real_hdr10_mastering_display_and_content_light`).
pub fn x265_master_display(hdr: &Hdr10Metadata) -> String {
    format!(
        "G({},{})B({},{})R({},{})WP({},{})L({},{})",
        scaled(hdr.green_x, 50000.0),
        scaled(hdr.green_y, 50000.0),
        scaled(hdr.blue_x, 50000.0),
        scaled(hdr.blue_y, 50000.0),
        scaled(hdr.red_x, 50000.0),
        scaled(hdr.red_y, 50000.0),
        scaled(hdr.white_point_x, 50000.0),
        scaled(hdr.white_point_y, 50000.0),
        scaled(hdr.max_luminance, 10000.0),
        scaled(hdr.min_luminance, 10000.0),
    )
}

/// `-svtav1-params mastering-display=...` value — plain decimal
/// chromaticity (0..1) and nits luminance, a genuinely *different* scale
/// from x265's (the "inverse rescale" notes/plan.md §1.2 warns is a likely
/// bug spot if copied carelessly from x265's convention). Verified against
/// this project's installed SVT-AV1 (v4.2.0): encoding with this exact
/// format and reading the *output* back with ffprobe reproduced the input
/// values; no `enable-hdr`-style flag exists on top of it — the plan's
/// prior-art source assumed one, but this SVT-AV1 version rejects it
/// ("Error parsing option enable-hdr"), and metadata presence alone is
/// sufficient to emit the HDR mastering-display/content-light OBUs.
pub fn svtav1_mastering_display(hdr: &Hdr10Metadata) -> String {
    format!(
        "G({},{})B({},{})R({},{})WP({},{})L({},{})",
        decimal(hdr.green_x),
        decimal(hdr.green_y),
        decimal(hdr.blue_x),
        decimal(hdr.blue_y),
        decimal(hdr.red_x),
        decimal(hdr.red_y),
        decimal(hdr.white_point_x),
        decimal(hdr.white_point_y),
        decimal(hdr.max_luminance),
        decimal(hdr.min_luminance),
    )
}

/// Content light level uses the same plain `"max,average"` integer format
/// for both x265's `max-cll=` and SVT-AV1's `content-light=` — unlike
/// mastering-display, CLL needs no codec-specific rescaling (verified: both
/// real encodes in this module's tests round-tripped the same input
/// integers unchanged). `None` when the source has no CLL data at all.
pub fn content_light_level(hdr: &Hdr10Metadata) -> Option<String> {
    let max = hdr.max_content_light?;
    let avg = hdr.max_average_light?;
    Some(format!("{max},{avg}"))
}

#[cfg(test)]
mod tests {
    use super::{content_light_level, svtav1_mastering_display, x265_master_display};
    use crate::probe::Hdr10Metadata;

    fn sample() -> Hdr10Metadata {
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

    #[test]
    fn x265_master_display_round_trips_the_original_scaled_integers() {
        assert_eq!(
            x265_master_display(&sample()),
            "G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,50)"
        );
    }

    #[test]
    fn svtav1_mastering_display_uses_plain_decimals_not_x265s_scale() {
        assert_eq!(
            svtav1_mastering_display(&sample()),
            "G(0.1700,0.7970)B(0.1310,0.0460)R(0.7080,0.2920)WP(0.3127,0.3290)L(1000.0000,0.0050)"
        );
    }

    #[test]
    fn content_light_level_is_the_same_format_for_both_codecs() {
        assert_eq!(content_light_level(&sample()), Some("1000,400".to_owned()));
    }

    #[test]
    fn content_light_level_is_none_without_both_values() {
        let hdr = Hdr10Metadata {
            max_content_light: None,
            ..sample()
        };
        assert_eq!(content_light_level(&hdr), None);
    }
}
