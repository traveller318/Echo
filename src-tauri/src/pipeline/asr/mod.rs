/*!
 * SOURCE OF TRUTH KEYWORDS: ASR pipeline, AsrWorker, AsrTake, AsrWorkerConfig, AcceleratorPicker, load_request, effective_language, usable_engine, EngineCheck, reload_on_change
 * WHAT:  Speech recognition in the pipeline: the ASR worker thread that owns the engine (worker.rs), the loads it
 *        runs on a loader thread (loader.rs), where each engine runs: fixed, measured or remembered, with the CPU
 *        fallback (accelerator.rs), the per-take handle segments flow through (take.rs), the rules that turn settings and caps into a load request and a
 *        per-segment language (plan.rs), whether speech can be transcribed now and how it is cut
 *        (engine_check.rs), and the engine swap a settings change asks for (switch.rs).
 * WHY:   02 §6.1: segments are transcribed on a dedicated thread while the user is still speaking, results come back
 *        tagged with their take and index, and the engine can be swapped without disturbing a take (02 §8.1). The
 *        pipeline only ever holds `Arc<dyn AsrEngine>` built by the registry, so a new engine is an adapter plus an
 *        entry (02 §8.4). Split by responsibility, like pipeline/capture.
 * WHERE: app/bootstrap spawns the worker and starts the first load; the session actor opens takes and
 *        checks the engine before a take (as does retry); the model manager's
 *        engine switch calls `load` with `load_request`.
 */

mod accelerator;
mod engine_check;
mod loader;
mod plan;
mod switch;
mod take;
mod worker;

#[cfg(test)]
mod accelerator_tests;
#[cfg(test)]
mod tests;

pub use accelerator::AcceleratorPicker;
pub use engine_check::{EngineCheck, request_load, segment_policy, usable_engine};
pub use plan::{effective_accelerator, effective_language, load_request};
pub use switch::{reload_on_change, remeasure};
pub use take::AsrTake;
pub use worker::{AsrBuilder, AsrWorker, AsrWorkerConfig};
