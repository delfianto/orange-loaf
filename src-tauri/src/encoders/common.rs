use super::EncodeContext;

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
    args.push("-map".to_owned());
    args.push("0:a?".to_owned());

    if let Some(filter_graph) = ctx.filters.render() {
        args.push("-vf".to_owned());
        args.push(filter_graph);
    }

    args
}

/// Builds the argv shared by every ffmpeg-native encoder after its
/// codec-specific flags: audio passthrough (no audio settings UI yet — M7),
/// progress reporting, and the output path.
pub fn build_suffix(ctx: &EncodeContext) -> Vec<String> {
    vec![
        "-c:a".to_owned(),
        "copy".to_owned(),
        "-progress".to_owned(),
        "pipe:1".to_owned(),
        "-nostats".to_owned(),
        ctx.output.clone(),
    ]
}

#[cfg(test)]
mod tests {
    use super::{EncodeContext, build_prefix};

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
}
