/*!
 * SOURCE OF TRUTH KEYWORDS: TextPolisher, polish, text cleanup, rule polisher, LLM polisher, prepare, polish chain stage, timeout fallback
 * WHAT:  TextPolisher rewrites a take's text (one stage of the polish chain), after an optional `prepare` that
 *        starts whatever the stage needs (a sidecar, a model); `unload` lets go of it again.
 * WHY:   Stages differ wildly in cost (`LatencyClass`): rules take microseconds, a local LLM hundreds of
 *        milliseconds over loopback HTTP. The methods are async (BoxFuture) so the pipeline can race a slow stage
 *        against its 2 s budget and keep the previous stage's text when it loses (02 §8.3); dropping the future
 *        cancels the call, so implementations must leave no half-done state behind. Output safety checks (length
 *        change, preambles, 05 A12) belong to the adapter that knows its model. `unload` exists because Windows
 *        cannot delete or replace a file a process has open: the model manager asks a stage to release its model
 *        before removing or reinstalling it; the next `prepare` or `polish` starts it again.
 * WHERE: Implemented by adapters/polish/rules/ (RulePolisher), adapters/polish/llama_server.rs
 *        (LlamaServerPolisher) and ports/fakes; built by registry engines entries; chained by pipeline/polish (PolishChain).
 */

use crate::types::{BoxFuture, PolishContext, PolisherCaps, PortResult};

/// One stage of the polish chain.
pub trait TextPolisher: Send + Sync {
    fn caps(&self) -> PolisherCaps;

    /// Gets ready for the first `polish` (e.g. starts and health-checks a sidecar, 05 A13). Idempotent and
    /// instant for stages that need nothing. Fails with `ModelMissing` when `caps().needs_model` and the model
    /// is not installed.
    fn prepare(&self) -> BoxFuture<'_, PortResult<()>>;

    /// Returns the polished text. Fails with `Polish` when the stage cannot produce trustworthy output.
    fn polish<'a>(
        &'a self,
        text: &'a str,
        context: &'a PolishContext,
    ) -> BoxFuture<'a, PortResult<String>>;

    /// Releases what `prepare` started (stops a sidecar, closes model files) at once; a no-op for stages that
    /// hold nothing. The stage stays usable: the next `prepare` or `polish` starts it again.
    fn unload(&self);
}
