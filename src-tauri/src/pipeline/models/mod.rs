/*!
 * SOURCE OF TRUTH KEYWORDS: model manager pipeline, ModelManager, ModelDeps, ModelPolicy, ReadinessRelay, ModelWatch, transfers, ModelProgress throttle
 * WHAT:  Model management orchestration (02 §8.2): the ModelManager behind the `models_*` commands (manager.rs),
 *        the running transfers and their throttled progress (transfer.rs) and the readiness relay + watch that
 *        checks a model after its engine failed to load (watch.rs).
 * WHY:   Split by responsibility like pipeline/asr: what a command does, how a transfer is tracked and reported,
 *        and what reacts to the speech engine. Works through ports only (ModelStore, FolderPicker), so the
 *        adapter and the dialog can be swapped and every path is tested with the port fakes.
 * WHERE: Built by app/bootstrap; used by ipc/commands/models.rs.
 */

mod manager;
mod transfer;
mod watch;

#[cfg(test)]
mod tests;

pub use manager::{ModelDeps, ModelManager, ModelPolicy};
pub use watch::{LoadFailures, ModelWatch, ReadinessRelay};
