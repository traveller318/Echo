/*!
 * SOURCE OF TRUTH KEYWORDS: ASR pipeline, AsrWorker, AsrTake, AsrWorkerConfig, AsrBuilder, load_request, effective_language, effective_accelerator
 * WHAT:  Speech recognition in the pipeline: the ASR worker thread that owns the engine (worker.rs), the per-take
 *        handle segments flow through (take.rs), and the rules that turn settings and caps into a load request and a
 *        per-segment language (plan.rs).
 * WHY:   02 §6.1: segments are transcribed on a dedicated thread while the user is still speaking, results come back
 *        tagged with their take and index, and the engine can be swapped without disturbing a take (02 §8.1). The
 *        pipeline only ever holds `Arc<dyn AsrEngine>` built by the registry, so a new engine is an adapter plus an
 *        entry (02 §8.4). Split by responsibility, like pipeline/capture.
 * WHERE: app/bootstrap spawns the worker and starts the first load; the session actor opens takes; the model manager's
 *        engine switch calls `load` with `load_request`.
 */

mod plan;
mod take;
mod worker;

#[cfg(test)]
mod tests;

pub use plan::{effective_accelerator, effective_language, load_request};
pub use take::AsrTake;
pub use worker::{AsrBuilder, AsrWorker, AsrWorkerConfig};
