use super::tracks;
use super::{EncodeContext, EncodeStep};

fn push_seek_args(args: &mut Vec<String>, ctx: &EncodeContext) {
    if let Some(start) = ctx.start_time {
        args.push("-ss".to_owned());
        args.push(start.to_string());
    }
    if let Some(end) = ctx.end_time {
        args.push("-to".to_owned());
        args.push(end.to_string());
    }
}

/// Builds the argv shared by every ffmpeg-native encoder up to (but not
/// including) the `-c:v ...` codec-specific flags: input, trim seek, stream
/// mapping, and the active filter graph (if any).
///
/// Trim seek placement follows the prior-art research (notes/plan.md
/// Appendix A): fast seek goes *before* `-i` (keyframe-snapping, cheap);
/// exact seek goes *after* all inputs (frame-accurate, slower — ffmpeg has
/// to decode from the nearest keyframe up to the requested point).
pub fn build_prefix(ctx: &EncodeContext) -> Vec<String> {
    let mut args = vec!["-y".to_owned()];

    if ctx.fast_seek {
        push_seek_args(&mut args, ctx);
    }
    args.push("-i".to_owned());
    args.push(ctx.source.clone());
    if !ctx.fast_seek {
        push_seek_args(&mut args, ctx);
    }

    args.push("-map".to_owned());
    args.push("0:v:0".to_owned());
    args.extend(tracks::audio_map_args(&ctx.audio_tracks));
    args.extend(tracks::subtitle_map_args(&ctx.subtitle_tracks));

    if let Some(filter_graph) = ctx.filters.render() {
        args.push("-vf".to_owned());
        args.push(filter_graph);
    }

    args
}

/// Builds the argv shared by every ffmpeg-native encoder after its
/// codec-specific flags: per-track audio/subtitle codec handling (see
/// [`tracks`]), progress reporting, and the output path.
pub fn build_suffix(ctx: &EncodeContext) -> Vec<String> {
    let mut args = tracks::audio_codec_args(&ctx.audio_tracks);
    args.extend(tracks::subtitle_codec_args(&ctx.subtitle_tracks));
    args.extend([
        "-progress".to_owned(),
        "pipe:1".to_owned(),
        "-nostats".to_owned(),
        ctx.output.clone(),
    ]);
    args
}

/// Builds ffmpeg's classic 2-pass argv pair — shared by every encoder that
/// supports `-pass 1`/`-pass 2` + `-passlogfile` (x264/x265; SVT-AV1's own
/// doc comment explains why it isn't offered this path). `video_args` is
/// the codec-specific middle section both passes share verbatim
/// (`-c:v libx264 -preset ... -b:v ...`) — the only difference between the
/// two steps is what comes before and after it.
///
/// Pass 1 discards its output entirely (`-f null -`) and skips audio/
/// subtitles altogether (`-an -sn`) — it exists purely to let the encoder
/// analyze the video ahead of pass 2's real encode, so nothing about
/// tracks or containers matters yet. Pass 2 is a normal encode in every
/// other respect (`build_suffix`'s full audio/subtitle codec handling and
/// output path), just reusing pass 1's stats via a shared `-passlogfile`
/// prefix — a fresh temp path per call, since two unrelated encodes
/// racing on the same stats file would corrupt each other's pass 1 data.
pub fn build_two_pass_steps(ctx: &EncodeContext, video_args: &[String]) -> Vec<EncodeStep> {
    let passlogfile = std::env::temp_dir()
        .join(format!(
            "orangeloaf-2pass-{}",
            uuid::Uuid::new_v4().simple()
        ))
        .display()
        .to_string();

    let mut pass1 = build_prefix(ctx);
    pass1.extend(video_args.iter().cloned());
    pass1.extend(
        [
            "-pass",
            "1",
            "-passlogfile",
            &passlogfile,
            "-an",
            "-sn",
            "-f",
            "null",
            "-",
        ]
        .map(str::to_owned),
    );

    let mut pass2 = build_prefix(ctx);
    pass2.extend(video_args.iter().cloned());
    pass2.extend([
        "-pass".to_owned(),
        "2".to_owned(),
        "-passlogfile".to_owned(),
        passlogfile,
    ]);
    pass2.extend(build_suffix(ctx));

    vec![EncodeStep::Argv(pass1), EncodeStep::Argv(pass2)]
}

#[cfg(test)]
mod tests {
    use super::{EncodeContext, EncodeStep, build_prefix, build_two_pass_steps};

    #[test]
    fn fast_seek_places_ss_and_to_before_the_input() {
        let ctx = EncodeContext {
            start_time: Some(5.0),
            end_time: Some(10.0),
            fast_seek: true,
            ..EncodeContext::new("in.mkv", "out.mkv")
        };
        let args = build_prefix(&ctx);
        let i_index = args.iter().position(|a| a == "-i").unwrap();
        let ss_index = args.iter().position(|a| a == "-ss").unwrap();
        assert!(ss_index < i_index, "fast seek's -ss should come before -i");
        assert_eq!(args[ss_index + 1], "5");
    }

    #[test]
    fn exact_seek_places_ss_and_to_after_the_input() {
        let ctx = EncodeContext {
            start_time: Some(5.0),
            end_time: Some(10.0),
            fast_seek: false,
            ..EncodeContext::new("in.mkv", "out.mkv")
        };
        let args = build_prefix(&ctx);
        let i_index = args.iter().position(|a| a == "-i").unwrap();
        let ss_index = args.iter().position(|a| a == "-ss").unwrap();
        assert!(ss_index > i_index, "exact seek's -ss should come after -i");
    }

    #[test]
    fn omits_seek_flags_entirely_when_no_trim_is_set() {
        let args = build_prefix(&EncodeContext::new("in.mkv", "out.mkv"));
        assert!(!args.contains(&"-ss".to_owned()));
        assert!(!args.contains(&"-to".to_owned()));
    }

    #[test]
    fn start_only_trim_omits_to() {
        let ctx = EncodeContext {
            start_time: Some(3.0),
            ..EncodeContext::new("in.mkv", "out.mkv")
        };
        let args = build_prefix(&ctx);
        assert!(args.contains(&"-ss".to_owned()));
        assert!(!args.contains(&"-to".to_owned()));
    }

    fn find_value_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .map(String::as_str)
    }

    #[test]
    fn two_pass_steps_share_one_passlogfile_and_split_pass_1_and_2_correctly() {
        let ctx = EncodeContext::new("in.mkv", "out.mkv");
        let video_args = vec![
            "-c:v".to_owned(),
            "libx264".to_owned(),
            "-b:v".to_owned(),
            "4000k".to_owned(),
        ];
        let steps = build_two_pass_steps(&ctx, &video_args);
        assert_eq!(steps.len(), 2, "two-pass should produce exactly two steps");
        let [EncodeStep::Argv(pass1), EncodeStep::Argv(pass2)] =
            <[_; 2]>::try_from(steps).unwrap_or_else(|_| unreachable!("length asserted above"));

        assert_eq!(find_value_after(&pass1, "-pass"), Some("1"));
        assert!(
            pass1.contains(&"-an".to_owned()),
            "pass 1 should skip audio"
        );
        assert!(
            pass1.contains(&"-sn".to_owned()),
            "pass 1 should skip subtitles"
        );
        assert_eq!(
            find_value_after(&pass1, "-f"),
            Some("null"),
            "pass 1's output should be discarded"
        );
        assert!(
            !pass1.contains(&"out.mkv".to_owned()),
            "pass 1 shouldn't reference the real output path at all"
        );

        assert_eq!(find_value_after(&pass2, "-pass"), Some("2"));
        assert!(
            pass2.contains(&"out.mkv".to_owned()),
            "pass 2 should be the one real encode producing the actual output"
        );

        let pass1_log = find_value_after(&pass1, "-passlogfile");
        let pass2_log = find_value_after(&pass2, "-passlogfile");
        assert!(pass1_log.is_some());
        assert_eq!(
            pass1_log, pass2_log,
            "both passes must agree on the same stats file"
        );
    }
}
