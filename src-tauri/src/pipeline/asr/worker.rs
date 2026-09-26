/*!
 * SOURCE OF TRUTH KEYWORDS: AsrWorker, ASR worker thread, echo-asr, engine swap, pinned engine, SegmentDone, readiness, SpeechEngineStatus, AsrWorkerConfig, GPU lost fallback
 * WHAT:  AsrWorker: the dedicated OS thread that owns the speech engine (02 §6.1). `load` has the Loader build, load
 *        and warm an engine on its accelerator and installs it atomically; `begin_take` opens an AsrTake whose
 *        segments are transcribed in arrival order and reported as AsrEvents; `unload` frees the engine;
 *        `readiness` says what a take started now would get and `status` adds where the engine runs and why.
 * WHY:   Inference runs on its own thread at normal priority, never on the tokio pool, so it cannot stall I/O and the
 *        capture worker (above normal) always wins the CPU (05 A9). One thread handles every segment of every take
 *        in FIFO order, so results come out in submission order. Loading, warming and the accelerator measurement
 *        happen on the loader thread (loader.rs) so a take in progress keeps transcribing on the current engine
 *        while a new one comes up; installing is one message on the worker, so the swap is atomic between two
 *        segments (02 §8.1). Each take is pinned to the engine that transcribed its first segment and the old engine
 *        is unloaded only when its last take lets go. Segments that arrive while the first engine is still loading
 *        wait in order instead of failing (a take right after launch); after a failed load they fail with the
 *        load's error (e.g. `ModelMissing`), so the session can keep the audio for retry. Loads are numbered, so an
 *        older load that finishes late never replaces a newer choice. A panic inside an engine becomes an
 *        `Internal` error for that segment or load, so one bad call never kills the thread every later take depends
 *        on. A segment that fails on the current engine's GPU (a driver reset or a removed device leaves DirectML
 *        failing every call) asks once for the same engine on the CPU (05 A6): the failed segment's take keeps its
 *        audio for retry, and the next takes run on the CPU instead of all failing until a restart. The status (and
 *        the readiness event) is published whenever the readiness or the running accelerator changes, so the UI
 *        hears about a CPU fallback even when readiness stays `Ready`. An engine installed while `auto` is still
 *        `Measuring` (it runs on the CPU) starts the background GPU measurement right after install, as an
 *        ordinary numbered load: it either installs the faster GPU instance (a normal swap) or keeps this engine and
 *        updates where it says it runs. The thread blocks on its inbox when idle.
 * WHERE: Spawned by app/bootstrap into CommandCtx; `load` is called on RunEvent::Ready (startup), by the engine
 *        switch and by `engine_remeasure`; `begin_take` by the session actor; `status` by `engine_status`; its takes
 *        are the capture worker's segment sink (take.rs).
 */

use std::{
    collections::{HashMap, VecDeque},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::Ordering,
        mpsc::{self, Receiver, Sender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use parking_lot::Mutex;

use super::{
    accelerator::AcceleratorPicker,
    loader::{LoadOutcome, Loaded, LoadedEngine, Loader, panicked},
    plan::effective_language,
    take::{AsrTake, TakeShared},
};
use crate::{
    ports::{AsrEngine, EventSink, WorkerScheduler},
    registry::{self, engines::BuildCtx},
    types::{
        Accelerator, AcceleratorChoice, AcceleratorReason, AcceleratorRequest, AppError, AsrEvent,
        AsrLoadRequest, AsrReadiness, EngineId, Language, PortError, PortResult,
        SpeechEngineStatus, SpeechSegment, TranscriptId, WorkerPriority,
    },
};

/// Builds an unloaded engine for a registry id.
pub type AsrBuilder = Arc<dyn Fn(&EngineId) -> PortResult<Arc<dyn AsrEngine>> + Send + Sync>;

/// What the ASR worker is built from.
pub struct AsrWorkerConfig {
    /// Constructs engines by id (the registry's `build_asr` in the app).
    pub build: AsrBuilder,
    /// Decides, measures and remembers which accelerator each engine runs on.
    pub accelerators: AcceleratorPicker,
    /// Sets the worker and loader threads to normal priority (05 A9).
    pub scheduler: Arc<dyn WorkerScheduler>,
    /// Receives every readiness or accelerator change; None when nobody listens yet.
    pub readiness: Option<Arc<dyn EventSink<AsrReadiness>>>,
}

impl AsrWorkerConfig {
    /// Engines built through the registry with `ctx` (only the requested engine is ever constructed, 02 §3.5).
    pub fn registry(
        ctx: BuildCtx,
        accelerators: AcceleratorPicker,
        scheduler: Arc<dyn WorkerScheduler>,
        readiness: Option<Arc<dyn EventSink<AsrReadiness>>>,
    ) -> Self {
        Self {
            build: Arc::new(move |id: &EngineId| registry::engines::build_asr(id, &ctx)),
            accelerators,
            scheduler,
            readiness,
        }
    }
}

/// Messages about one take, handled in arrival order (and held back together while the first engine loads).
pub(super) enum TakeMessage {
    Segment {
        take: Arc<TakeShared>,
        segment: SpeechSegment,
    },
    Finish {
        take: Arc<TakeShared>,
    },
    Release {
        take: TranscriptId,
    },
}

/// Everything the worker thread receives.
pub(super) enum Message {
    Take(TakeMessage),
    Loading {
        generation: u64,
        engine_id: EngineId,
    },
    Loaded(Box<LoadOutcome>),
    Unload {
        generation: u64,
    },
    Shutdown,
}

/// Handle to the ASR worker; clones share the one thread.
#[derive(Clone)]
pub struct AsrWorker {
    inner: Arc<Inner>,
}

struct Inner {
    inbox: Sender<Message>,
    status: Arc<Mutex<SpeechEngineStatus>>,
    loader: Loader,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl AsrWorker {
    /// Starts the worker thread with no engine; `load` gives it one.
    pub fn spawn(config: AsrWorkerConfig) -> PortResult<Self> {
        let AsrWorkerConfig {
            build,
            accelerators,
            scheduler,
            readiness: sink,
        } = config;
        let (inbox, receiver) = mpsc::channel();
        let status = Arc::new(Mutex::new(SpeechEngineStatus::UNLOADED));
        let loader = Loader::new(inbox.clone(), build, Arc::clone(&scheduler), accelerators);
        let state = WorkerLoop {
            inbox: receiver,
            status: Arc::clone(&status),
            sink,
            loader: loader.clone(),
            current: None,
            pins: HashMap::new(),
            latest: 0,
            loading: None,
            failure: None,
            deferred: VecDeque::new(),
        };
        let thread = thread::Builder::new()
            .name("echo-asr".to_owned())
            .spawn(move || {
                prioritize(scheduler.as_ref(), WorkerPriority::Normal, "ASR worker");
                state.run();
            })
            .map_err(|error| {
                PortError::new(AppError::Internal)
                    .with_detail(format!("the ASR worker could not start: {error}"))
            })?;
        Ok(Self {
            inner: Arc::new(Inner {
                inbox,
                status,
                loader,
                thread: Mutex::new(Some(thread)),
            }),
        })
    }

    /// What a take started now would get.
    pub fn readiness(&self) -> AsrReadiness {
        self.inner.status.lock().readiness.clone()
    }

    /// Readiness plus where the ready engine runs and why.
    pub fn status(&self) -> SpeechEngineStatus {
        self.inner.status.lock().clone()
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: AsrWorker::load, background engine load, engine switch
     * WHAT:  Builds `request.engine_id`, loads its model on the accelerator the request resolves to and warms it on a
     *        loader thread, then installs it; the returned receiver gets where it runs (callers may drop it). A newer
     *        `load` or `unload` supersedes this one (`Busy`).
     * WHY:   The caller (startup, a command) never blocks on a multi-second load; the current engine keeps serving
     *        takes until the new one is warm.
     * WHERE: app/bootstrap on RunEvent::Ready; the engine switch (settings effects, models_set_active);
     *        engine_remeasure.
     */
    pub fn load(&self, request: AsrLoadRequest) -> Receiver<PortResult<AcceleratorChoice>> {
        self.inner.loader.start(request)
    }

    /// Drops the current engine (freeing its memory once no take uses it) and cancels any load in flight.
    pub fn unload(&self) {
        let generation = self.inner.loader.next_generation();
        self.send(Message::Unload { generation });
    }

    /// Opens a take: segments emitted into it are transcribed in order and reported to `events`. `language` is the
    /// user's preference (None = auto-detect), narrowed to the caps of the engine that runs the take.
    pub fn begin_take(
        &self,
        id: TranscriptId,
        language: Option<Language>,
        events: Arc<dyn EventSink<AsrEvent>>,
    ) -> Arc<AsrTake> {
        Arc::new(AsrTake::new(id, language, events, self.inner.inbox.clone()))
    }

    fn send(&self, message: Message) {
        // The worker only stops when the last handle drops, so while `self` exists the send succeeds.
        let _ = self.inner.inbox.send(message);
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        let _ = self.inbox.send(Message::Shutdown);
        if let Some(thread) = self.thread.lock().take()
            && thread.join().is_err()
        {
            tracing::error!("the ASR worker thread panicked");
        }
    }
}

/// The background GPU measurement an engine installed while `Measuring` asks for, with its CPU timing.
fn measure_request(engine: &LoadedEngine) -> Option<AsrLoadRequest> {
    let choice = engine.choice();
    if choice.reason != AcceleratorReason::Measuring {
        return None;
    }
    let cpu = *choice.benchmark?.timing(Accelerator::Cpu)?;
    Some(AsrLoadRequest {
        engine_id: engine.id.clone(),
        model_dir: engine.model_dir.clone(),
        accelerator: AcceleratorRequest::MeasureGpu { cpu },
    })
}

/// Applies `priority` to the calling thread; a refusal only costs scheduling, so it is logged.
pub(super) fn prioritize(scheduler: &dyn WorkerScheduler, priority: WorkerPriority, thread: &str) {
    if let Err(error) = scheduler.prioritize_current_thread(priority) {
        tracing::warn!(
            thread,
            detail = error.detail(),
            "thread runs at default priority"
        );
    }
}

fn millis(elapsed: Duration) -> u64 {
    u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
}

/// The worker thread's state.
struct WorkerLoop {
    inbox: Receiver<Message>,
    status: Arc<Mutex<SpeechEngineStatus>>,
    sink: Option<Arc<dyn EventSink<AsrReadiness>>>,
    /// Starts the CPU reload after the GPU stopped working.
    loader: Loader,
    /// The engine new takes are pinned to.
    current: Option<Arc<LoadedEngine>>,
    /// The engine each open take started on.
    pins: HashMap<TranscriptId, Arc<LoadedEngine>>,
    /// The newest load or unload request; results of older loads are discarded.
    latest: u64,
    /// The engine the newest load is bringing up, while it runs.
    loading: Option<EngineId>,
    /// Why the newest load failed, while no engine is installed.
    failure: Option<(EngineId, PortError)>,
    /// Take messages held back while the first engine loads, in arrival order.
    deferred: VecDeque<TakeMessage>,
}

impl WorkerLoop {
    fn run(mut self) {
        while let Ok(message) = self.inbox.recv() {
            match message {
                Message::Take(message) => {
                    if self.waiting() || !self.deferred.is_empty() {
                        self.deferred.push_back(message);
                    } else {
                        self.handle(message);
                    }
                }
                Message::Loading {
                    generation,
                    engine_id,
                } => {
                    if generation > self.latest {
                        self.latest = generation;
                        self.loading = Some(engine_id);
                        self.publish();
                    }
                }
                Message::Loaded(outcome) => self.install(*outcome),
                Message::Unload { generation } => {
                    if generation > self.latest {
                        self.latest = generation;
                        self.loading = None;
                        self.failure = None;
                        if let Some(engine) = self.current.take() {
                            self.retire(engine);
                        }
                        self.publish();
                        self.replay();
                    }
                }
                Message::Shutdown => break,
            }
        }
        if let Some(engine) = self.current.take() {
            self.retire(engine);
        }
        for (_, engine) in std::mem::take(&mut self.pins) {
            self.retire(engine);
        }
    }

    /// Segments must wait: no engine yet, but one is loading.
    fn waiting(&self) -> bool {
        self.current.is_none() && self.loading.is_some()
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: install engine, atomic engine swap, superseded load, load failure readiness
     * WHAT:  Applies a finished load: the newest one replaces the current engine (the old one retires once its takes
     *        let go) or records the failure; an outdated one is discarded and its caller told `Busy`. Held-back
     *        segments then run (or fail) in order.
     * WHY:   Everything happens between two messages on the one thread that transcribes, so no segment ever sees a
     *        half-installed engine (02 §8.1). A failed switch keeps the working engine.
     * WHERE: WorkerLoop::run on Message::Loaded.
     */
    fn install(&mut self, outcome: LoadOutcome) {
        let LoadOutcome {
            generation,
            engine_id,
            result,
            reply,
        } = outcome;
        if generation != self.latest {
            if let Ok(Loaded::Engine(engine)) = result {
                self.retire(Arc::new(engine));
            }
            let _ = reply.send(Err(PortError::new(AppError::Busy).with_detail(format!(
                "loading `{engine_id}` was superseded by a newer request"
            ))));
            return;
        }
        self.loading = None;
        let mut measure_gpu = None;
        let reported = match result {
            Ok(Loaded::KeepCurrent(choice)) => {
                match &self.current {
                    Some(current) if current.id == choice.engine_id => {
                        tracing::info!(
                            engine = %engine_id,
                            accelerator = ?choice.accelerator,
                            reason = ?choice.reason,
                            "the speech engine stays where it runs"
                        );
                        *current.choice.lock() = choice.clone();
                    }
                    _ => {
                        tracing::debug!(engine = %engine_id, "a measurement finished for an engine no longer loaded")
                    }
                }
                Ok(choice)
            }
            Ok(Loaded::Engine(engine)) => {
                let choice = engine.choice();
                tracing::info!(
                    engine = %engine_id,
                    accelerator = ?choice.accelerator,
                    reason = ?choice.reason,
                    load_ms = choice.bring_up.load_ms,
                    warm_up_ms = choice.bring_up.warm_up_ms,
                    "speech engine ready"
                );
                self.failure = None;
                measure_gpu = measure_request(&engine);
                if let Some(old) = self.current.replace(Arc::new(engine)) {
                    self.retire(old);
                }
                Ok(choice)
            }
            Err(error) => {
                tracing::warn!(
                    engine = %engine_id,
                    code = error.error().code().as_str(),
                    detail = error.detail(),
                    "speech engine did not load"
                );
                if self.current.is_none() {
                    self.failure = Some((engine_id, error.clone()));
                }
                Err(error)
            }
        };
        // Status first, so a caller woken by the reply already reads the new state.
        self.publish();
        // The caller may have stopped waiting; the engine is installed either way.
        let _ = reply.send(reported);
        self.replay();
        if let Some(request) = measure_gpu {
            // The second half of auto: a second instance measures the GPU while this one serves takes.
            drop(self.loader.start(request));
        }
    }

    /// Runs held-back take messages in order until the worker has to wait again.
    fn replay(&mut self) {
        while !self.waiting() {
            let Some(message) = self.deferred.pop_front() else {
                return;
            };
            self.handle(message);
        }
    }

    fn handle(&mut self, message: TakeMessage) {
        match message {
            TakeMessage::Segment { take, segment } => self.transcribe(&take, &segment),
            TakeMessage::Finish { take } => {
                if !take.is_cancelled() {
                    take.events.emit(AsrEvent::Drained { take: take.id });
                }
                self.unpin(take.id);
            }
            TakeMessage::Release { take } => self.unpin(take),
        }
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: transcribe segment, SegmentDone, SegmentFailed, per-segment timing
     * WHAT:  Transcribes one segment on the take's pinned engine (pinning the current one on first use) in the take's
     *        effective language, and reports SegmentDone or SegmentFailed.
     * WHY:   Timing per segment goes to the debug log (02 §12) with ids and durations only, never text (02 §10).
     * WHERE: WorkerLoop::handle.
     */
    fn transcribe(&mut self, take: &TakeShared, segment: &SpeechSegment) {
        if take.is_cancelled() {
            return;
        }
        let index = segment.index;
        let engine = self.engine_for(take.id);
        let result = engine.as_ref().map_err(Clone::clone).and_then(|engine| {
            let language = effective_language(take.language.as_ref(), &engine.caps);
            let started = Instant::now();
            let result = catch_unwind(AssertUnwindSafe(|| {
                engine
                    .engine
                    .transcribe(&segment.samples, language.as_ref())
            }))
            .unwrap_or_else(|_| Err(panicked("transcribing")));
            tracing::debug!(
                take = %take.id,
                index,
                engine = %engine.id,
                accelerator = ?engine.accelerator(),
                audio_ms = segment.duration_ms(),
                asr_ms = millis(started.elapsed()),
                ok = result.is_ok(),
                "segment transcribed"
            );
            result
        });
        let event = match result {
            Ok(output) => AsrEvent::SegmentDone {
                take: take.id,
                index,
                output,
            },
            Err(error) => {
                tracing::warn!(
                    take = %take.id,
                    index,
                    code = error.error().code().as_str(),
                    detail = error.detail(),
                    "segment failed"
                );
                if let Ok(engine) = &engine {
                    self.fall_back_if_gpu_lost(engine);
                }
                AsrEvent::SegmentFailed {
                    take: take.id,
                    index,
                    error,
                }
            }
        };
        take.events.emit(event);
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: GPU lost, device removed, DirectML failure mid-session, CPU reload, CpuAfterGpuLoss
     * WHAT:  After a segment failed on `engine`: when it is the current engine and runs on the GPU, asks (once) for the
     *        same engine and model on the CPU.
     * WHY:   A GPU that resets or disappears fails every later call of its session, so without this every take would
     *        fail until a restart. The reload is an ordinary load, so the current engine keeps its pinned takes and
     *        the swap is atomic; a newer settings load still supersedes it. An engine that is no longer current
     *        (a take pinned to an old one) is left alone: new takes already use another engine.
     * WHERE: WorkerLoop::transcribe on a failed segment.
     */
    fn fall_back_if_gpu_lost(&self, engine: &Arc<LoadedEngine>) {
        let is_current = self
            .current
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, engine));
        if !is_current
            || engine.accelerator() != Accelerator::Gpu
            || engine.gpu_lost.swap(true, Ordering::SeqCst)
        {
            return;
        }
        tracing::warn!(engine = %engine.id, "the GPU stopped working; reloading the speech engine on the CPU");
        // The outcome arrives as a Loaded message on this thread; nobody waits for the reply.
        drop(self.loader.start(AsrLoadRequest {
            engine_id: engine.id.clone(),
            model_dir: engine.model_dir.clone(),
            accelerator: AcceleratorRequest::CpuAfterGpuLoss,
        }));
    }

    /// The engine `take` runs on: its pin, or the current engine (pinned now); the load error when there is none.
    fn engine_for(&mut self, take: TranscriptId) -> PortResult<Arc<LoadedEngine>> {
        if let Some(engine) = self.pins.get(&take) {
            return Ok(Arc::clone(engine));
        }
        let engine = self.current.clone().ok_or_else(|| match &self.failure {
            Some((_, error)) => error.clone(),
            None => PortError::new(AppError::Asr).with_detail("no speech engine is loaded"),
        })?;
        self.pins.insert(take, Arc::clone(&engine));
        Ok(engine)
    }

    fn unpin(&mut self, take: TranscriptId) {
        if let Some(engine) = self.pins.remove(&take) {
            self.retire(engine);
        }
    }

    /// Unloads `engine` if nothing else holds it (not current, no take pinned to it); otherwise just lets go.
    fn retire(&self, engine: Arc<LoadedEngine>) {
        if let Some(engine) = Arc::into_inner(engine) {
            match catch_unwind(AssertUnwindSafe(|| engine.engine.unload())) {
                Ok(Ok(())) => tracing::info!(engine = %engine.id, "speech engine unloaded"),
                Ok(Err(error)) => tracing::warn!(
                    engine = %engine.id,
                    detail = error.detail(),
                    "speech engine did not unload cleanly"
                ),
                Err(_) => {
                    tracing::error!(engine = %engine.id, "speech engine panicked while unloading")
                }
            }
        }
    }

    /// Recomputes the status and reports it when the readiness or where the engine runs changed.
    fn publish(&self) {
        let readiness = match (&self.current, &self.loading, &self.failure) {
            (Some(engine), _, _) => AsrReadiness::Ready {
                engine_id: engine.id.clone(),
                accelerator: engine.accelerator(),
            },
            (None, Some(engine_id), _) => AsrReadiness::Loading {
                engine_id: engine_id.clone(),
            },
            (None, None, Some((engine_id, error))) => AsrReadiness::Failed {
                engine_id: engine_id.clone(),
                error: error.error().clone(),
            },
            (None, None, None) => AsrReadiness::Unloaded,
        };
        let next = SpeechEngineStatus {
            readiness,
            accelerator: self.current.as_ref().map(|engine| engine.choice()),
        };
        let mut status = self.status.lock();
        if *status != next {
            *status = next.clone();
            drop(status);
            if let Some(sink) = &self.sink {
                sink.emit(next.readiness);
            }
        }
    }
}
