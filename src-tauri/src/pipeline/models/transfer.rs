/*!
 * SOURCE OF TRUTH KEYWORDS: running transfers, TransferSlots, TransferGuard, ProgressRelay, PartProgress, ModelProgress throttle 10 Hz, latest progress, cancel transfer, combined install set progress
 * WHAT:  TransferSlots: the model transfers running now (download, import, check), one per model, each with its
 *        Cancellation and latest progress; TransferGuard frees the slot when the transfer ends however it ends;
 *        ProgressRelay: the sink one card's transfer reports into, which records every report and forwards
 *        ModelProgress to the UI at most once per interval (always on a phase change); PartProgress: the EventSink
 *        handed to the ModelStore for one manifest of a card's install set, which places that manifest's bytes in
 *        the card's combined progress.
 * WHY:   02 §4.4: ModelProgress is at most 10 Hz, and the rate policy lives here, not in each adapter
 *        (ports/model_store.rs). The latest report is kept so `models_list` can tell a page opened mid-download
 *        where it is (the page then follows the events), and so a retry knows whether the failed attempt moved
 *        bytes. One slot per model makes a second transfer of the same model `Busy`, whoever started it (a
 *        command or the check after a failed load). A guard, not a manual remove, so a cancelled or panicking
 *        transfer can never leave a slot behind that would block the model forever. A card can move several
 *        manifests (an LLM and its runtime, 02 §8.2): the page sees one stream under the card's model id whose total
 *        is the whole set, each manifest's own bytes scaled into its share (its adapter may count an archive or its
 *        unpacked files), and only the last manifest's checking and installing phases, so the bar never runs
 *        backwards or flashes "Installing" half-way. Every manifest of a set holds its own slot, so two cards that
 *        share a runtime can never write into the same staging folder at once.
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
    types::{AppError, AppEvent, ByteCount, ModelId, ModelPhase, ModelProgress, PortResult},
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
        self.begin_set(id, &[])
    }

    /// Claims the slots of `id` and every id in `with` (its requirements) together, sharing `id`'s
    /// cancellation; `Busy` (claiming none) while any of them is being transferred.
    pub fn begin_set(&self, id: &ModelId, with: &[ModelId]) -> PortResult<TransferGuard<'_>> {
        let mut slots = self.slots.lock();
        let mut ids: Vec<ModelId> = Vec::with_capacity(with.len() + 1);
        for claimed in std::iter::once(id).chain(with) {
            if slots.contains_key(claimed) {
                return Err(AppError::Busy.into());
            }
            if !ids.contains(claimed) {
                ids.push(claimed.clone());
            }
        }
        let cancel = Arc::new(Cancellation::default());
        for claimed in &ids {
            slots.insert(
                claimed.clone(),
                Slot {
                    cancel: Arc::clone(&cancel),
                    latest: None,
                },
            );
        }
        Ok(TransferGuard {
            slots: self,
            ids,
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

/// Holds a transfer's slots (the card's model and its requirements); dropping it frees them.
pub(super) struct TransferGuard<'a> {
    slots: &'a TransferSlots,
    ids: Vec<ModelId>,
    cancel: Arc<Cancellation>,
}

impl TransferGuard<'_> {
    pub fn cancellation(&self) -> &Cancellation {
        &self.cancel
    }
}

impl Drop for TransferGuard<'_> {
    fn drop(&mut self) {
        let mut slots = self.slots.slots.lock();
        for id in &self.ids {
            slots.remove(id);
        }
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
    /// The card's model: every report is announced under it.
    id: ModelId,
    /// Bytes of the whole transfer (every manifest of the set).
    total: ByteCount,
    interval: Duration,
    forwarded: Mutex<Forwarded>,
}

impl<'a> ProgressRelay<'a> {
    pub fn new(
        slots: &'a TransferSlots,
        events: &'a dyn EventSink<AppEvent>,
        id: ModelId,
        total: ByteCount,
        interval: Duration,
    ) -> Self {
        Self {
            slots,
            events,
            id,
            total,
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
            model_id: self.id.clone(),
            bytes: ByteCount::new(match phase {
                ModelPhase::Ready => self.total.get(),
                _ => self.bytes(),
            }),
            total: self.total,
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

/**
 * SOURCE OF TRUTH KEYWORDS: PartProgress, one manifest of an install set, scaled progress, offset bytes
 * WHAT:  The sink one manifest of a set reports into: its bytes, scaled from the adapter's own total to the
 *        manifest's share `budget` and placed after `offset`, go to the relay under the card's id; before the last
 *        manifest, checking and installing read as transferring.
 * WHY:   See the file header. Scaling keeps the bar honest whatever the adapter counts (an archive's bytes while it
 *        downloads, its unpacked files while they are checked).
 * WHERE: manager.rs, one per manifest of a download, import or check.
 */
pub(super) struct PartProgress<'r, 'a> {
    pub relay: &'r ProgressRelay<'a>,
    /// The bytes of the manifests before this one.
    pub offset: u64,
    /// This manifest's share of the relay's total.
    pub budget: u64,
    /// Whether this is the set's last manifest (the card's own model).
    pub last: bool,
}

impl EventSink<ModelProgress> for PartProgress<'_, '_> {
    fn emit(&self, progress: ModelProgress) {
        let reported = u128::from(progress.bytes.get().min(progress.total.get()));
        let scaled = reported * u128::from(self.budget) / u128::from(progress.total.get().max(1));
        let within = u64::try_from(scaled)
            .unwrap_or(self.budget)
            .min(self.budget);
        let phase = match progress.phase {
            ModelPhase::Verifying | ModelPhase::Installing if !self.last => {
                ModelPhase::Transferring
            }
            phase => phase,
        };
        self.relay.report(ModelProgress {
            model_id: self.relay.id.clone(),
            bytes: ByteCount::new(self.offset.saturating_add(within)),
            total: self.relay.total,
            phase,
        });
    }
}

impl ProgressRelay<'_> {
    /// Records `progress` and forwards it when due (every phase change, else once per interval).
    fn report(&self, progress: ModelProgress) {
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
        registry::models::{LLAMA_CPP_VULKAN, PARAKEET_TDT_V3, QWEN3_1_7B_Q4},
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
        let _guard = slots.begin(&PARAKEET_TDT_V3).unwrap();
        let relay = ProgressRelay::new(
            &slots,
            &sink,
            PARAKEET_TDT_V3,
            ByteCount::new(1_000),
            Duration::from_secs(60),
        );
        let part = PartProgress {
            relay: &relay,
            offset: 0,
            budget: 1_000,
            last: true,
        };
        for bytes in [10, 20, 30] {
            part.emit(progress(bytes, ModelPhase::Transferring));
        }
        part.emit(progress(1_000, ModelPhase::Verifying));
        relay.announce(ModelPhase::Ready);
        assert_eq!(
            forwarded(&sink),
            [
                (10, ModelPhase::Transferring),
                (1_000, ModelPhase::Verifying),
                (1_000, ModelPhase::Ready),
            ]
        );
        assert_eq!(
            slots.latest(&PARAKEET_TDT_V3).map(|latest| latest.phase),
            Some(ModelPhase::Ready)
        );
    }

    #[test]
    fn a_set_reports_one_stream_scaled_into_each_share() {
        let slots = TransferSlots::default();
        let sink = RecordingSink::default();
        let _guard = slots
            .begin_set(&QWEN3_1_7B_Q4, &[LLAMA_CPP_VULKAN])
            .unwrap();
        // A runtime worth 100 bytes of the card, then a model worth 900.
        let relay = ProgressRelay::new(
            &slots,
            &sink,
            QWEN3_1_7B_Q4,
            ByteCount::new(1_000),
            Duration::ZERO,
        );
        let runtime = PartProgress {
            relay: &relay,
            offset: 0,
            budget: 100,
            last: false,
        };
        // The adapter counts the runtime's archive: 500 of 1 000 is half of its share.
        runtime.emit(ModelProgress {
            model_id: LLAMA_CPP_VULKAN,
            bytes: ByteCount::new(500),
            total: ByteCount::new(1_000),
            phase: ModelPhase::Transferring,
        });
        runtime.emit(ModelProgress {
            model_id: LLAMA_CPP_VULKAN,
            bytes: ByteCount::new(1_000),
            total: ByteCount::new(1_000),
            phase: ModelPhase::Installing,
        });
        let model = PartProgress {
            relay: &relay,
            offset: 100,
            budget: 900,
            last: true,
        };
        model.emit(ModelProgress {
            model_id: QWEN3_1_7B_Q4,
            bytes: ByteCount::new(450),
            total: ByteCount::new(900),
            phase: ModelPhase::Transferring,
        });
        model.emit(ModelProgress {
            model_id: QWEN3_1_7B_Q4,
            bytes: ByteCount::new(900),
            total: ByteCount::new(900),
            phase: ModelPhase::Installing,
        });
        assert_eq!(
            forwarded(&sink),
            [
                (50, ModelPhase::Transferring),
                (100, ModelPhase::Transferring),
                (550, ModelPhase::Transferring),
                (1_000, ModelPhase::Installing),
            ]
        );
        assert!(sink.events().iter().all(|event| matches!(
            event,
            AppEvent::ModelProgress(progress) if progress.model_id == QWEN3_1_7B_Q4
        )));
        // Both manifests are claimed: neither can start a second transfer.
        assert!(slots.begin(&LLAMA_CPP_VULKAN).is_err());
        assert!(slots.begin(&QWEN3_1_7B_Q4).is_err());
    }

    #[test]
    fn a_set_claims_nothing_when_one_member_is_busy() {
        let slots = TransferSlots::default();
        let runtime = slots.begin(&LLAMA_CPP_VULKAN).unwrap();
        assert!(
            slots
                .begin_set(&QWEN3_1_7B_Q4, &[LLAMA_CPP_VULKAN])
                .is_err()
        );
        assert!(!slots.is_running(&QWEN3_1_7B_Q4));
        drop(runtime);
        let set = slots
            .begin_set(&QWEN3_1_7B_Q4, &[LLAMA_CPP_VULKAN])
            .unwrap();
        assert!(slots.cancel(&QWEN3_1_7B_Q4));
        assert!(set.cancellation().is_cancelled());
        drop(set);
        assert!(!slots.is_running(&LLAMA_CPP_VULKAN));
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
