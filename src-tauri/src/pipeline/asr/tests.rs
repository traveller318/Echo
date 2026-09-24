/*!
 * SOURCE OF TRUTH KEYWORDS: ASR worker tests, FakeAsrEngine, segment ordering test, load failure test, engine swap test, cancel take test
 * WHAT:  The ASR worker against FakeAsrEngine: ordering, waiting for a load, load and segment errors, cancel, engine
 *        swap with pinned takes, superseded loads, unload, panics, language narrowing and thread priorities.
 * WHY:   02 §13 pipeline tests: every path the session actor will rely on is proven without a model or a microphone.
 *        Events come from the worker thread, so tests wait on a ChannelSink instead of sleeping.
 * WHERE: `cargo test`.
 */

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, Sender},
    },
    time::Duration,
};

use super::*;
use crate::{
    ports::{
        AsrEngine, EventSink,
        fakes::{ChannelSink, EVENT_TIMEOUT, FakeAsrEngine, FakeWorkerScheduler},
    },
    types::{
        Accelerator, AppError, AsrCaps, AsrEvent, AsrLoadRequest, AsrLoaded, AsrOutput,
        AsrReadiness, EngineId, Language, ModelId, PortError, PortResult, SpeechSegment,
        TranscriptId, WorkerPriority,
    },
};

const ENGINE_A: EngineId = EngineId::from_static("engine-a");
const ENGINE_B: EngineId = EngineId::from_static("engine-b");
const EN: Language = Language::from_static("en");

/// How long a test watches for an event that must not come.
const QUIET: Duration = Duration::from_millis(100);

/// A worker over scripted engines, its readiness events and its thread-priority requests.
struct Rig {
    worker: AsrWorker,
    engines: HashMap<EngineId, Arc<FakeAsrEngine>>,
    readiness: Arc<ChannelSink<AsrReadiness>>,
    scheduler: Arc<FakeWorkerScheduler>,
    /// Loads of these engines wait until the test sends on the matching gate.
    gates: Arc<Mutex<HashMap<EngineId, Receiver<()>>>>,
}

impl Rig {
    fn new() -> Self {
        let engines: HashMap<EngineId, Arc<FakeAsrEngine>> = [ENGINE_A, ENGINE_B]
            .into_iter()
            .map(|id| (id, Arc::new(FakeAsrEngine::english())))
            .collect();
        Self::with_builder(engines, None)
    }

    fn with_builder(
        engines: HashMap<EngineId, Arc<FakeAsrEngine>>,
        custom: Option<AsrBuilder>,
    ) -> Self {
        let readiness = Arc::new(ChannelSink::default());
        let scheduler = Arc::new(FakeWorkerScheduler::default());
        let gates: Arc<Mutex<HashMap<EngineId, Receiver<()>>>> = Arc::default();
        let build: AsrBuilder = match custom {
            Some(build) => build,
            None => {
                let engines = engines.clone();
                let gates = Arc::clone(&gates);
                Arc::new(move |id: &EngineId| {
                    let gate = gates.lock().unwrap().remove(id);
                    if let Some(gate) = gate {
                        gate.recv_timeout(EVENT_TIMEOUT).unwrap();
                    }
                    engines
                        .get(id)
                        .map(|engine| Arc::clone(engine) as Arc<dyn AsrEngine>)
                        .ok_or_else(|| {
                            PortError::new(AppError::Internal).with_detail("unknown engine")
                        })
                })
            }
        };
        let worker = AsrWorker::spawn(AsrWorkerConfig {
            build,
            scheduler: Arc::clone(&scheduler) as _,
            readiness: Some(Arc::clone(&readiness) as _),
        })
        .unwrap();
        Self {
            worker,
            engines,
            readiness,
            scheduler,
            gates,
        }
    }

    fn engine(&self, id: &EngineId) -> &FakeAsrEngine {
        &self.engines[id]
    }

    /// Makes the next load of `id` wait until the returned sender fires.
    fn hold_load(&self, id: EngineId) -> Sender<()> {
        let (open, gate) = mpsc::channel();
        self.gates.lock().unwrap().insert(id, gate);
        open
    }

    fn load(&self, id: EngineId) -> Receiver<PortResult<AsrLoaded>> {
        self.worker.load(request(id))
    }

    /// Loads `id` and waits for it to be installed.
    fn load_ready(&self, id: EngineId) {
        let outcome = self.load(id).recv_timeout(EVENT_TIMEOUT).unwrap();
        assert_eq!(
            outcome.map(|loaded| loaded.accelerator),
            Ok(Accelerator::Cpu)
        );
    }

    fn take(&self) -> (Arc<AsrTake>, Arc<ChannelSink<AsrEvent>>) {
        self.take_in(Some(EN))
    }

    fn take_in(&self, language: Option<Language>) -> (Arc<AsrTake>, Arc<ChannelSink<AsrEvent>>) {
        let events = Arc::new(ChannelSink::default());
        let take =
            self.worker
                .begin_take(TranscriptId::generate(), language, Arc::clone(&events) as _);
        (take, events)
    }
}

fn request(engine_id: EngineId) -> AsrLoadRequest {
    AsrLoadRequest {
        engine_id,
        model_dir: PathBuf::from("models").join("fake"),
        accelerator: Accelerator::Cpu,
    }
}

fn segment(index: u32, samples: usize) -> SpeechSegment {
    SpeechSegment {
        index,
        start_ms: u64::from(index) * 1_000,
        speech_ms: 500,
        samples: vec![0.0; samples],
    }
}

fn done(take: &AsrTake, index: u32, text: &str) -> AsrEvent {
    AsrEvent::SegmentDone {
        take: take.id(),
        index,
        output: AsrOutput {
            text: text.to_owned(),
            language: None,
        },
    }
}

fn failed_code(event: Option<AsrEvent>) -> Option<(u32, AppError)> {
    match event? {
        AsrEvent::SegmentFailed { index, error, .. } => Some((index, error.into_app_error())),
        _ => None,
    }
}

#[test]
fn segments_are_transcribed_in_order_then_the_take_drains() {
    let rig = Rig::new();
    rig.load_ready(ENGINE_A);
    let engine = rig.engine(&ENGINE_A);
    assert_eq!(engine.warm_ups(), 1, "warm-up runs once, right after load");
    for text in ["One.", "Two.", "Three."] {
        engine.push_text(text);
    }
    let (take, events) = rig.take();
    take.emit(segment(0, 100));
    take.emit(segment(1, 200));
    take.emit(segment(2, 300));
    take.finish();
    assert_eq!(events.next(), Some(done(&take, 0, "One.")));
    assert_eq!(events.next(), Some(done(&take, 1, "Two.")));
    assert_eq!(events.next(), Some(done(&take, 2, "Three.")));
    assert_eq!(events.next(), Some(AsrEvent::Drained { take: take.id() }));
    assert_eq!(
        engine
            .calls()
            .iter()
            .map(|call| call.samples)
            .collect::<Vec<_>>(),
        [100, 200, 300]
    );
    let requests = rig.scheduler.requests();
    for thread in ["echo-asr", "echo-asr-load"] {
        assert!(
            requests.contains(&(Some(thread.to_owned()), WorkerPriority::Normal)),
            "{thread} asks for normal priority: {requests:?}"
        );
    }
}

#[test]
fn readiness_follows_loading_then_ready() {
    let rig = Rig::new();
    assert_eq!(rig.worker.readiness(), AsrReadiness::Unloaded);
    rig.load_ready(ENGINE_A);
    assert_eq!(
        rig.readiness.next(),
        Some(AsrReadiness::Loading {
            engine_id: ENGINE_A
        })
    );
    let ready = AsrReadiness::Ready {
        engine_id: ENGINE_A,
        accelerator: Accelerator::Cpu,
    };
    assert_eq!(rig.readiness.next(), Some(ready.clone()));
    assert_eq!(rig.worker.readiness(), ready);
    assert_eq!(
        rig.engine(&ENGINE_A).loaded(),
        Some((PathBuf::from("models").join("fake"), Accelerator::Cpu))
    );
}

#[test]
fn segments_sent_during_the_first_load_wait_and_keep_their_order() {
    let rig = Rig::new();
    let open = rig.hold_load(ENGINE_A);
    let outcome = rig.load(ENGINE_A);
    let (take, events) = rig.take();
    take.emit(segment(0, 10));
    take.emit(segment(1, 10));
    take.finish();
    assert!(
        events.nothing_within(QUIET),
        "nothing runs before the engine is ready"
    );
    assert_eq!(
        rig.worker.readiness(),
        AsrReadiness::Loading {
            engine_id: ENGINE_A
        }
    );
    rig.engine(&ENGINE_A).push_text("First.");
    rig.engine(&ENGINE_A).push_text("Second.");
    open.send(()).unwrap();
    assert!(outcome.recv_timeout(EVENT_TIMEOUT).unwrap().is_ok());
    assert_eq!(events.next(), Some(done(&take, 0, "First.")));
    assert_eq!(events.next(), Some(done(&take, 1, "Second.")));
    assert_eq!(events.next(), Some(AsrEvent::Drained { take: take.id() }));
}

#[test]
fn a_missing_model_fails_every_segment_with_its_error() {
    let rig = Rig::new();
    let missing = AppError::ModelMissing {
        model_id: ModelId::from_static("model-a"),
    };
    rig.engine(&ENGINE_A)
        .fail_next_load(PortError::new(missing.clone()).with_detail("no files"));
    let outcome = rig.load(ENGINE_A).recv_timeout(EVENT_TIMEOUT).unwrap();
    assert_eq!(
        outcome.err().map(PortError::into_app_error),
        Some(missing.clone())
    );
    assert_eq!(
        rig.worker.readiness(),
        AsrReadiness::Failed {
            engine_id: ENGINE_A,
            error: missing.clone()
        }
    );
    let (take, events) = rig.take();
    take.emit(segment(0, 10));
    take.finish();
    assert_eq!(failed_code(events.next()), Some((0, missing)));
    assert_eq!(events.next(), Some(AsrEvent::Drained { take: take.id() }));
}

#[test]
fn a_failed_segment_does_not_stop_the_next_one() {
    let rig = Rig::new();
    rig.load_ready(ENGINE_A);
    let engine = rig.engine(&ENGINE_A);
    engine.push_result(Err(
        PortError::new(AppError::Asr).with_detail("inference failed")
    ));
    engine.push_text("Still here.");
    let (take, events) = rig.take();
    take.emit(segment(0, 10));
    take.emit(segment(1, 10));
    assert_eq!(failed_code(events.next()), Some((0, AppError::Asr)));
    assert_eq!(events.next(), Some(done(&take, 1, "Still here.")));
}

#[test]
fn a_take_without_any_engine_fails_at_once() {
    let rig = Rig::new();
    let (take, events) = rig.take();
    take.emit(segment(0, 10));
    assert_eq!(failed_code(events.next()), Some((0, AppError::Asr)));
}

#[test]
fn a_cancelled_take_reports_nothing_more() {
    let rig = Rig::new();
    let open = rig.hold_load(ENGINE_A);
    let outcome = rig.load(ENGINE_A);
    let (take, events) = rig.take();
    take.emit(segment(0, 10));
    take.emit(segment(1, 10));
    take.cancel();
    take.finish();
    open.send(()).unwrap();
    assert!(outcome.recv_timeout(EVENT_TIMEOUT).unwrap().is_ok());
    assert!(events.nothing_within(QUIET));
    assert!(
        rig.engine(&ENGINE_A).calls().is_empty(),
        "no inference for a discarded take"
    );
}

/// 02 §8.1: a take in progress finishes on the old engine; the old engine is unloaded once that take lets go.
#[test]
fn a_take_finishes_on_the_engine_it_started_with() {
    let rig = Rig::new();
    rig.load_ready(ENGINE_A);
    let (old, old_events) = rig.take();
    rig.engine(&ENGINE_A).push_text("Old one.");
    old.emit(segment(0, 10));
    assert_eq!(old_events.next(), Some(done(&old, 0, "Old one.")));

    rig.load_ready(ENGINE_B);
    assert!(
        rig.engine(&ENGINE_A).loaded().is_some(),
        "the pinned take keeps engine A loaded"
    );
    let (new, new_events) = rig.take();
    rig.engine(&ENGINE_A).push_text("Old two.");
    rig.engine(&ENGINE_B).push_text("New one.");
    old.emit(segment(1, 10));
    new.emit(segment(0, 10));
    assert_eq!(old_events.next(), Some(done(&old, 1, "Old two.")));
    assert_eq!(new_events.next(), Some(done(&new, 0, "New one.")));
    assert_eq!(rig.engine(&ENGINE_A).calls().len(), 2);
    assert_eq!(rig.engine(&ENGINE_B).calls().len(), 1);

    // Finishing lets go of the pin; the next take's drain proves the worker got that far.
    old.finish();
    assert_eq!(
        old_events.next(),
        Some(AsrEvent::Drained { take: old.id() })
    );
    new.finish();
    assert_eq!(
        new_events.next(),
        Some(AsrEvent::Drained { take: new.id() })
    );
    assert_eq!(
        rig.engine(&ENGINE_A).loaded(),
        None,
        "engine A unloads after its last take"
    );
    assert!(rig.engine(&ENGINE_B).loaded().is_some());
}

#[test]
fn a_load_overtaken_by_a_newer_one_is_discarded() {
    let rig = Rig::new();
    let open = rig.hold_load(ENGINE_A);
    let first = rig.load(ENGINE_A);
    rig.load_ready(ENGINE_B);
    open.send(()).unwrap();
    assert_eq!(
        first
            .recv_timeout(EVENT_TIMEOUT)
            .unwrap()
            .err()
            .map(PortError::into_app_error),
        Some(AppError::Busy)
    );
    assert_eq!(
        rig.worker.readiness(),
        AsrReadiness::Ready {
            engine_id: ENGINE_B,
            accelerator: Accelerator::Cpu
        }
    );
    // The discarded engine is unloaded, not left resident.
    let (take, events) = rig.take();
    take.finish();
    assert_eq!(events.next(), Some(AsrEvent::Drained { take: take.id() }));
    assert_eq!(rig.engine(&ENGINE_A).loaded(), None);
}

#[test]
fn a_failed_switch_keeps_the_working_engine() {
    let rig = Rig::new();
    rig.load_ready(ENGINE_A);
    rig.engine(&ENGINE_B)
        .fail_next_load(PortError::new(AppError::ModelCorrupt {
            model_id: ModelId::from_static("model-b"),
        }));
    assert!(
        rig.load(ENGINE_B)
            .recv_timeout(EVENT_TIMEOUT)
            .unwrap()
            .is_err()
    );
    assert!(rig.worker.readiness().is_ready());
    rig.engine(&ENGINE_A).push_text("Still A.");
    let (take, events) = rig.take();
    take.emit(segment(0, 10));
    assert_eq!(events.next(), Some(done(&take, 0, "Still A.")));
}

#[test]
fn unload_frees_the_engine_and_later_segments_fail() {
    let rig = Rig::new();
    rig.load_ready(ENGINE_A);
    rig.worker.unload();
    let (take, events) = rig.take();
    take.emit(segment(0, 10));
    assert_eq!(failed_code(events.next()), Some((0, AppError::Asr)));
    assert_eq!(rig.worker.readiness(), AsrReadiness::Unloaded);
    assert_eq!(rig.engine(&ENGINE_A).loaded(), None);
}

#[test]
fn the_language_is_narrowed_to_what_the_engine_accepts() {
    let rig = Rig::new();
    rig.load_ready(ENGINE_A);
    let engine = rig.engine(&ENGINE_A);
    engine.push_text("a");
    engine.push_text("b");
    // The English-only fake has no auto-detect and would refuse `None` or German.
    let (auto, auto_events) = rig.take_in(None);
    auto.emit(segment(0, 10));
    assert_eq!(auto_events.next(), Some(done(&auto, 0, "a")));
    let (german, german_events) = rig.take_in(Some(Language::from_static("de")));
    german.emit(segment(0, 10));
    assert_eq!(german_events.next(), Some(done(&german, 0, "b")));
    assert!(engine.calls().iter().all(|call| call.language == Some(EN)));
}

/// An engine that panics on every call.
struct Panicking;

impl AsrEngine for Panicking {
    fn caps(&self) -> AsrCaps {
        FakeAsrEngine::english().caps()
    }
    fn load(&self, _: &std::path::Path, accelerator: Accelerator) -> PortResult<Accelerator> {
        Ok(accelerator)
    }
    fn warm_up(&self) -> PortResult<()> {
        Ok(())
    }
    fn unload(&self) -> PortResult<()> {
        Ok(())
    }
    fn transcribe(&self, _: &[f32], _: Option<&Language>) -> PortResult<AsrOutput> {
        panic!("engine bug")
    }
}

#[test]
fn a_panicking_engine_fails_the_segment_not_the_worker() {
    let rig = Rig::with_builder(
        HashMap::new(),
        Some(Arc::new(|_: &EngineId| Ok(Arc::new(Panicking) as Arc<dyn AsrEngine>)) as AsrBuilder),
    );
    rig.load_ready(ENGINE_A);
    let (take, events) = rig.take();
    take.emit(segment(0, 10));
    take.finish();
    assert_eq!(failed_code(events.next()), Some((0, AppError::Internal)));
    assert_eq!(events.next(), Some(AsrEvent::Drained { take: take.id() }));
}

#[test]
fn a_panicking_build_is_a_failed_load() {
    let rig = Rig::with_builder(
        HashMap::new(),
        Some(
            Arc::new(|_: &EngineId| -> PortResult<Arc<dyn AsrEngine>> { panic!("registry bug") })
                as AsrBuilder,
        ),
    );
    let outcome = rig.load(ENGINE_A).recv_timeout(EVENT_TIMEOUT).unwrap();
    assert_eq!(
        outcome.err().map(PortError::into_app_error),
        Some(AppError::Internal)
    );
    assert!(matches!(
        rig.worker.readiness(),
        AsrReadiness::Failed { .. }
    ));
}
