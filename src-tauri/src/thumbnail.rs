use std::path::Path;
use std::process::Command;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

/// Extracts a single downscaled JPEG frame at `timestamp_secs` and returns it
/// as a `data:` URI the frontend can bind directly to an `<img>` src.
///
/// Uses `-ss` before `-i` (fast/keyframe-snapping seek) since this is a
/// preview thumbnail, not a frame-accurate export.
///
/// # Errors
///
/// Returns an error if ffmpeg can't be run or produces no output (for
/// example, a `timestamp_secs` past the end of the video).
pub fn generate_thumbnail(
    ffmpeg: &Path,
    source: &str,
    timestamp_secs: f64,
) -> Result<String, String> {
    let timestamp_secs = timestamp_secs.max(0.0);
    let output = Command::new(ffmpeg)
        .args([
            "-y",
            "-ss",
            &format!("{timestamp_secs:.3}"),
            "-i",
            source,
            "-frames:v",
            "1",
            "-vf",
            "scale='min(480,iw)':-2",
            "-f",
            "image2",
            "-c:v",
            "mjpeg",
            "pipe:1",
        ])
        .output()
        .map_err(|err| format!("failed to run ffmpeg: {err}"))?;

    if output.stdout.is_empty() {
        return Err("ffmpeg produced no thumbnail frame".to_owned());
    }

    Ok(format!(
        "data:image/jpeg;base64,{}",
        BASE64.encode(output.stdout)
    ))
}

#[cfg(test)]
mod tests {
    use super::generate_thumbnail;

    #[test]
    #[ignore = "requires system ffmpeg"]
    fn generates_a_thumbnail_from_a_real_video() {
        let uri = generate_thumbnail(
            std::path::Path::new("ffmpeg"),
            "tests/fixtures/sdr_8bit.mp4",
            0.5,
        )
        .expect("thumbnail generation should succeed");
        assert!(uri.starts_with("data:image/jpeg;base64,"));
        assert!(
            uri.len() > 100,
            "expected a real JPEG payload, got {} chars",
            uri.len()
        );
    }
}
