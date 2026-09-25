/*!
 * SOURCE OF TRUTH KEYWORDS: ReadinessRelay, ModelWatch, engine readiness events, ModelsChanged on readiness, failed load check, AsrReadiness sink
 * WHAT:  ReadinessRelay: the EventSink<AsrReadiness> the ASR worker reports into; it tells the page the models
 *        list changed (the speech engine started, finished or failed loading) and queues every load failure that
 *        could be a damaged model. ModelWatch: the task that takes those failures and has the ModelManager check
 *        the model (`check_after_failed_load`).
 * WHY:   The worker must exist before the manager (the manager holds it), yet the manager must hear the worker's
 *        failures, so the relay is built first with the sending half of a channel and the watch gets the other half
 *        once the manager exists (the RetentionHandle / RetentionSweeper split). The relay runs on the worker's
 *        thread, so it only sends: hashing happens on the watch task. `ModelMissing` needs no check (nothing to
 *        hash) and `Busy` is a superseded load, not a failure.
 * WHERE: app/bootstrap builds the relay into AsrWorkerConfig and spawns ModelWatch::run on Tauri's runtime.
 */

use std::sync::Arc;

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use super::ModelManager;
use crate::{
    ports::EventSink,
    types::{AppError, AppEvent, AsrReadiness, EngineId, ModelsChanged},
};

/// Relays speech engine readiness to the Models page and load failures to the ModelWatch.
pub struct ReadinessRelay {
    events: Arc<dyn EventSink<AppEvent>>,
    failures: UnboundedSender<EngineId>,
}

/// The receiving end of the relay's load failures, handed to ModelWatch.
pub struct LoadFailures(UnboundedReceiver<EngineId>);

impl ReadinessRelay {
    pub fn new(events: Arc<dyn EventSink<AppEvent>>) -> (Self, LoadFailures) {
        let (failures, receiver) = mpsc::unbounded_channel();
        (Self { events, failures }, LoadFailures(receiver))
    }
}

impl EventSink<AsrReadiness> for ReadinessRelay {
    fn emit(&self, readiness: AsrReadiness) {
        self.events.emit(ModelsChanged {}.into());
        if let AsrReadiness::Failed { engine_id, error } = readiness
            && !matches!(error, AppError::ModelMissing { .. } | AppError::Busy)
        {
            // The watch stops only when the app does; a closed channel just means nobody checks any more.
            let _ = self.failures.send(engine_id);
        }
    }
}

/// Checks the model of every engine that failed to load.
pub struct ModelWatch {
    manager: ModelManager,
    failures: LoadFailures,
}

impl ModelWatch {
    pub fn new(manager: ModelManager, failures: LoadFailures) -> Self {
        Self { manager, failures }
    }

    /// Runs until the relay is dropped (the ASR worker shut down).
    pub async fn run(mut self) {
        while let Some(engine_id) = self.failures.0.recv().await {
            self.manager.check_after_failed_load(&engine_id).await;
        }
    }
}
