pub mod x265;

use crate::filters::FilterSpec;

/// Everything a specific encoder's [`EncoderProfile::build_steps`] needs to
/// know about the job that isn't its own codec settings.
pub struct EncodeContext {
    pub source: String,
    pub output: String,
    pub filters: FilterSpec,
}

/// One command to actually run. Currently always a plain argv list — the
/// `gifski`-style real-pipe variant (see notes/plan.md §4.3) joins this enum
/// when a hybrid-pipe encoder is implemented; until then there's nothing to
/// pipe into, so it isn't modeled yet.
pub enum EncodeStep {
    Argv(Vec<String>),
}

/// Implemented by every encoder's settings type. Mirrors the prior-art
/// research's `build(fastflix) -> List[Command]` shape (notes/plan.md §1.3):
/// a strategy per encoder that turns its own settings plus shared context
/// into an ordered list of steps (more than one for e.g. future two-pass
/// support).
pub trait EncoderProfile {
    fn build_steps(&self, ctx: &EncodeContext) -> Vec<EncodeStep>;
}
