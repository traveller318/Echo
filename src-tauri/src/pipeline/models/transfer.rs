/*!
 * SOURCE OF TRUTH KEYWORDS: running transfers, TransferSlots, TransferGuard, ProgressRelay, ModelProgress throttle 10 Hz, latest progress, cancel transfer
 * WHAT:  TransferSlots: the model transfers running now (download, import, check), one per model, each with its
 *        Cancellation and latest progress; TransferGuard frees the slot when the transfer ends however it ends;
 *        ProgressRelay: the EventSink handed to the ModelStore, which records every report and forwards
 *        ModelProgress to the UI at most once per interval (always on a phase change).
 * WHY:   02 §4.4: ModelProgress is at most 10 Hz, and the rate policy lives here, not in each adapter
 *        (ports/model_store.rs). The latest report is kept so `models_list` can tell a page opened mid-download
 *        where it is (the page then follows the events), and so a retry knows whether the failed attempt moved
 *        bytes. One slot per model makes a second transfer of the same model `Busy`, whoever started it (a
 *        command or the check after a failed load). A guard, not a manual remove, so a cancelled or panicking
 *        transfer can never leave a slot behind that would block the model forever.
 * WHERE: pipeline/models/manager.rs.
 */

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use parking_lot::Mutex;

use crate::{
    pipeline::cancel::Cancellation,
    ports::EventSink,
    types::{
        AppError, AppEvent, ByteCount, ModelId, ModelManifest, ModelPhase, ModelProgress,
        PortResult,
    },
};

struct Slot {
    cancel: Arc<Cancellation>,
    latest: Option<ModelProgress>,
}

/// The transfers running now, by model.
#[derive(Default)]
pub(super) struct TransferSlots {
    slots: Mutex<HashMap<ModelId, Slot>>,
}

impl TransferSlots {
    /// Claims `id`'s slot; `Busy` while another transfer of that model runs.
    pub fn begin(&self, id: &ModelId) -> PortResult<TransferGuard<'_>> {
        let mut slots = self.slots.lock();
        if slots.contains_key(id) {
            return Err(AppError::Busy.into());
        }
        let cancel = Arc::new(Cancellation::default());
        slots.insert(
            id.clone(),
            Slot {
                cancel: Arc::clone(&cancel),
                latest: None,
            },
        );
        Ok(TransferGuard {
            slots: self,
            id: id.clone(),
            cancel,
        })
    }

    /// Stops `id`'s transfer; false when none runs.
    pub fn cancel(&self, id: &ModelId) -> bool {
        self.slots
            .lock()
            .get(id)
            .map(|slot| slot.cancel.cancel())
            .is_some()
    }

    pub fn is_running(&self, id: &ModelId) -> bool {
        self.slots.lock().contains_key(id)
    }

    /// The last progress `id`'s running transfer reported.
    pub fn latest(&self, id: &ModelId) -> Option<ModelProgress> {
        self.slots
            .lock()
            .get(id)
            .and_then(|slot| slot.latest.clone())
    }

    fn record(&self, progress: &ModelProgress) {
        if let Some(slot) = self.slots.lock().get_mut(&progress.model_id) {
            slot.latest = Some(progress.clone());
        }
    }
}

/// Holds a model's transfer slot; dropping it frees the slot.
pub(super) struct TransferGuard<'a> {
    slots: &'a TransferSlots,
    id: ModelId,
    cancel: Arc<Cancellation>,
}

impl TransferGuard<'_> {
    pub fn cancellation(&self) -> &Cancellation {
        &self.cancel
    }
}

impl Drop for TransferGuard<'_> {
    fn drop(&mut self) {
        self.slots.slots.lock().remove(&self.id);
    }
}

/// Throttle state: when progress was last forwarded and in which phase, and the bytes last reported.
#[derive(Default)]
struct Forwarded {
    at: Option<Instant>,
    phase: Option<ModelPhase>,
    bytes: u64,
}

/**
 * SOURCE OF TRUTH KEYWORDS: ProgressRelay, throttle progress, forward ModelProgress, terminal phase emit
 * WHAT:  The sink one transfer reports into: records every report (in the slot and its own byte count), forwards at most one per `interval` (plus every
 *        phase change) as a ModelProgress event, and emits the pipeline's own phases (`waiting`, and the terminal
 *        ready / cancelled / failed) unthrottled.
 * WHY:   Adapters report per block (about 1 000 times for a 670 MB model); the UI needs 10 Hz. A phase change is
 *        never swallowed, so the page never shows "downloading" after verification started.
 * WHERE: manager.rs hands it to ModelStore::download / import / verify.
 */
pub(super) struct ProgressRelay<'a> {
    slots: &'a TransferSlots,
    events: &'a dyn EventSink<AppEvent>,
    manifest: &'a ModelManifest,
    interval: Duration,
    forwarded: Mutex<Forwarded>,
}

impl<'a> ProgressRelay<'a> {
    pub fn new(
        slots: &'a TransferSlots,
        events: &'a dyn EventSink<AppEvent>,
        manifest: &'a ModelManifest,
        interval: Duration,
    ) -> Self {
        Self {
            slots,
            events,
            manifest,
            interval,
            forwarded: Mutex::new(Forwarded::default()),
        }
    }

    /// Bytes the transfer has reached so far (kept after its slot is freed, for the terminal report).
    pub fn bytes(&self) -> u64 {
        self.forwarded.lock().bytes
    }

    /// Reports the transfer's first phase at `bytes` (what a resumed download already has), so a page opened
    /// before the adapter's first report already sees it running.
    pub fn start(&self, phase: ModelPhase, bytes: u64) {
        self.forwarded.lock().bytes = bytes;
        self.announce(phase);
    }

    /// Reports `phase` at the bytes reached so far, unthrottled (waiting, or the terminal phase).
    pub fn announce(&self, phase: ModelPhase) {
        let progress = ModelProgress {
            model_id: self.manifest.id.clone(),
            bytes: ByteCount::new(match phase {
                ModelPhase::Ready => self.manifest.total_bytes().get(),
                _ => self.bytes(),
            }),
            total: self.manifest.total_bytes(),
            phase,
        };
        self.slots.record(&progress);
        *self.forwarded.lock() = Forwarded {
            at: Some(Instant::now()),
            phase: Some(phase),
            bytes: progress.bytes.get(),
        };
        self.events.emit(progress.into());
    }
}

impl EventSink<ModelProgress> for ProgressRelay<'_> {
    fn emit(&self, progress: ModelProgress) {
        self.slots.record(&progress);
        let now = Instant::now();
        let forward = {
            let mut forwarded = self.forwarded.lock();
            forwarded.bytes = progress.bytes.get();
            let due = forwarded.phase != Some(progress.phase)
                || forwarded
                    .at
                    .is_none_or(|at| now.duration_since(at) >= self.interval);
            if due {
                forwarded.at = Some(now);
                forwarded.phase = Some(progress.phase);
            }
            due
        };
        if forward {
            self.events.emit(progress.into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ports::fakes::RecordingSink,
        registry::models::{self, PARAKEET_TDT_V3},
    };

    fn progress(bytes: u64, phase: ModelPhase) -> ModelProgress {
        ModelProgress {
            model_id: PARAKEET_TDT_V3,
            bytes: ByteCount::new(bytes),
            total: ByteCount::new(1_000),
            phase,
        }
    }

    fn forwarded(sink: &RecordingSink<AppEvent>) -> Vec<(u64, ModelPhase)> {
        sink.events()
            .into_iter()
            .filter_map(|event| match event {
                AppEvent::ModelProgress(progress) => Some((progress.bytes.get(), progress.phase)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn reports_are_throttled_but_phase_changes_always_pass() {
        let slots = TransferSlots::default();
        let sink = RecordingSink::default();
        let manifest = models::find(&PARAKEET_TDT_V3).unwrap();
        let _guard = slots.begin(&manifest.id).unwrap();
        let relay = ProgressRelay::new(&slots, &sink, manifest, Duration::from_secs(60));
        for bytes in [10, 20, 30] {
            relay.emit(progress(bytes, ModelPhase::Transferring));
        }
        relay.emit(progress(1_000, ModelPhase::Verifying));
        relay.announce(ModelPhase::Ready);
        assert_eq!(
            forwarded(&sink),
            [
                (10, ModelPhase::Transferring),
                (1_000, ModelPhase::Verifying),
                (manifest.total_bytes().get(), ModelPhase::Ready),
            ]
        );
        assert_eq!(
            slots.latest(&manifest.id).map(|latest| latest.phase),
            Some(ModelPhase::Ready)
        );
    }

    #[test]
    fn one_transfer_per_model_and_the_guard_frees_the_slot() {
        let slots = TransferSlots::default();
        let id = PARAKEET_TDT_V3;
        let guard = slots.begin(&id).unwrap();
        assert!(slots.is_running(&id));
        assert!(slots.begin(&id).is_err());
        assert!(slots.cancel(&id));
        assert!(guard.cancellation().is_cancelled());
        drop(guard);
        assert!(!slots.is_running(&id));
        assert!(!slots.cancel(&id));
        assert!(slots.begin(&id).is_ok());
    }
}
