/*!
 * SOURCE OF TRUTH KEYWORDS: polish pipeline, PolishChain, PolishChains, polish_plan, polish_context, join_segments, polish chain order
 * WHAT:  Text cleanup in the pipeline: joining a take's segments (join.rs), deciding the stages and the per-take
 *        context from settings, the registry and the engine's caps (plan.rs), and running the built stages with
 *        their fallbacks and the trailing space (chain.rs), and the one chain the app shares (shared.rs).
 * WHY:   02 §8.3: one fixed order (rules, then the opt-in LLM, then the trailing space), every stage a TextPolisher
 *        the registry builds, so a new stage is an adapter plus an entry and the chain never names one. Split by
 *        responsibility like pipeline/asr.
 * WHERE: The session actor joins SegmentDone texts and runs the shared chain per take before delivery; retry
 *        (pipeline/retry.rs) runs the same chain over re-transcribed text.
 */

mod chain;
mod join;
mod plan;
mod shared;

#[cfg(test)]
mod tests;

pub use chain::PolishChain;
pub use join::join_segments;
pub use plan::{polish_context, polish_plan};
pub use shared::{PolishChains, PolisherBuilder};
