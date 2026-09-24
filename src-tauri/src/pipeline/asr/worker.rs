/*!
 * SOURCE OF TRUTH KEYWORDS: AsrWorker, ASR worker thread, echo-asr, engine load, warm up, engine swap, pinned engine, SegmentDone, readiness, AsrWorkerConfig
 * WHAT:  AsrWorker: the dedicated OS thread that owns the speech engine (02 §6.1). `load` builds, loads and warms an
 *        engine on a short-lived loader thread and installs it atomically; `begin_take` opens an AsrTake whose
 *        segments are transcribed in arrival order and reported as AsrEvents; `unload` frees the engine;
 *        `readiness` says what a take started now would get.
 * WHY:   Inference runs on its own thread at normal priority, never on the tokio pool, so it cannot stall I/O and the
 *        capture worker (above normal) always wins the CPU (05 A9). One thread handles every segment of every take
 *        in FIFO order, so results come out in submission order. Loading and the 1 s warm-up (05 A8) happen on a
 *        separate loader thread so a take in progress keeps transcribing on the current engine while a new one warms
 *        up; installing is one message on the worker, so the swap is atomic between two segments (02 §8.1). Each
 *        take is pinned to the engine that transcribed its first segment and the old engine is unloaded only when
 *        its last take lets go. Segments that arrive while the first engine is still loading wait in order instead
 *        of failing (a take right after launch); after a failed load they fail with the load's error (e.g.
 *        `ModelMissing`), so the session can keep the audio for retry. Loads are numbered, so an older load that
 *        finishes late never replaces a newer choice. A panic inside an engine becomes an `Internal` error for that
 *        segment or load, so one bad call never kills the thread every later take depends on. Engines come from an
 *        AsrBuilder: the registry in the app, fakes in tests. The thread blocks on its inbox when idle (no timers).
 * WHERE: Spawned by app/bootstrap into CommandCtx; `load` is called on RunEvent::Ready (startup) and by the engine
 *        switch; `begin_take` by the session actor; its takes are the capture worker's segment sink (take.rs).
 */

use std::{
    collections::{HashMap, VecDeque},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use parking_lot::Mutex;

use super::{
    plan::effective_language,
    take::{AsrTake, TakeShared},
};
use crate::{
    ports::{AsrEngine, EventSink, WorkerScheduler},
    registry::{self, engines::BuildCtx},
    types::{
        Accelerator, AppError, AsrCaps, AsrEvent, AsrLoadRequest, AsrLoaded, AsrReadiness,
        EngineId, Language, PortError, PortResult, SpeechSegment, TranscriptId, WorkerPriority,
    },
};

/// Builds an unloaded engine for a registry id.
pub type AsrBuilder = Arc<dyn Fn(&EngineId) -> PortResult<Arc<dyn AsrEngine>> + Send + Sync>;

/// What the ASR worker is built from.
pub struct AsrWorkerConfig {
    /// Constructs engines by id (the registry's `build_asr` in the app).
    pub build: AsrBuilder,
    /// Sets the worker and loader threads to normal priority (05 A9).
    pub scheduler: Arc<dyn WorkerScheduler>,
    /// Receives every readiness change; None when nobody listens yet.
    pub readiness: Option<Arc<dyn EventSink<AsrReadiness>>>,
}

impl AsrWorkerConfig {
    /// Engines built through the registry with `ctx` (only the requested engine is ever constructed, 02 §3.5).
    pub fn registry(
        ctx: BuildCtx,
        scheduler: Arc<dyn WorkerScheduler>,
        readiness: Option<Arc<dyn EventSink<AsrReadiness>>>,
    ) -> Self {
        Self {
            build: Arc::new(move |id: &EngineId| registry::engines::build_asr(id, &ctx)),
            scheduler,
            readiness,
        }
    }
}

/// An engine that finished loading and warming up.
struct LoadedEngine {
    id: EngineId,
    engine: Arc<dyn AsrEngine>,
    caps: AsrCaps,
    accelerator: Accelerator,
}

/// A load that finished on the loader thread.
pub(super) struct LoadOutcome {
    generation: u64,
    engine_id: EngineId,
    result: PortResult<(LoadedEngine, AsrLoaded)>,
    reply: Sender<PortResult<AsrLoaded>>,
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
    readiness: Arc<Mutex<AsrReadiness>>,
    generation: AtomicU64,
    build: AsrBuilder,
    scheduler: Arc<dyn WorkerScheduler>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl AsrWorker {
    /// Starts the worker thread with no engine; `load` gives it one.
    pub fn spawn(config: AsrWorkerConfig) -> PortResult<Self> {
        let AsrWorkerConfig {
            build,
            scheduler,
            readiness: sink,
        } = config;
        let (inbox, receiver) = mpsc::channel();
        let readiness = Arc::new(Mutex::new(AsrReadiness::Unloaded));
        let state = WorkerLoop {
            inbox: receiver,
            readiness: Arc::clone(&readiness),
            sink,
            current: None,
            pins: HashMap::new(),
            latest: 0,
            loading: None,
            failure: None,
            deferred: VecDeque::new(),
        };
        let thread_scheduler = Arc::clone(&scheduler);
        let thread = thread::Builder::new()
            .name("echo-asr".to_owned())
            .spawn(move || {
                prioritize(thread_scheduler.as_ref(), "ASR worker");
                state.run();
            })
            .map_err(|error| {
                PortError::new(AppError::Internal)
                    .with_detail(format!("the ASR worker could not start: {error}"))
            })?;
        Ok(Self {
            inner: Arc::new(Inner {
                inbox,
                readiness,
                generation: AtomicU64::new(0),
                build,
                scheduler,
                thread: Mutex::new(Some(thread)),
            }),
        })
    }

    /// What a take started now would get.
    pub fn readiness(&self) -> AsrReadiness {
        self.inner.readiness.lock().clone()
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: AsrWorker::load, background engine load, loader thread, engine switch
     * WHAT:  Builds `request.engine_id`, loads its model and warms it up on a loader thread, then installs it; the
     *        returned receiver gets the outcome (callers may drop it). A newer `load` or `unload` supersedes this one
     *        (`Busy`).
     * WHY:   The caller (startup, a command) never blocks on a multi-second load; the current engine keeps serving
     *        takes until the new one is warm.
     * WHERE: app/bootstrap on RunEvent::Ready; the engine switch (models_set_active).
     */
    pub fn load(&self, request: AsrLoadRequest) -> Receiver<PortResult<AsrLoaded>> {
        let generation = self.inner.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let (reply, outcome) = mpsc::channel();
        self.send(Message::Loading {
            generation,
            engine_id: request.engine_id.clone(),
        });
        let inbox = self.inner.inbox.clone();
        let build = Arc::clone(&self.inner.build);
        let scheduler = Arc::clone(&self.inner.scheduler);
        let engine_id = request.engine_id.clone();
        let fallback_reply = reply.clone();
        let spawned = thread::Builder::new()
            .name("echo-asr-load".to_owned())
            .spawn(move || {
                prioritize(scheduler.as_ref(), "ASR loader");
                let engine_id = request.engine_id.clone();
                let result =
                    catch_unwind(AssertUnwindSafe(|| load_engine(build.as_ref(), &request)))
                        .unwrap_or_else(|_| Err(panicked("loading the engine")));
                // A closed inbox means the app is shutting down; the engine is dropped with the result.
                let _ = inbox.send(Message::Loaded(Box::new(LoadOutcome {
                    generation,
                    engine_id,
                    result,
                    reply,
                })));
            });
        if let Err(error) = spawned {
            self.send(Message::Loaded(Box::new(LoadOutcome {
                generation,
                engine_id,
                result: Err(PortError::new(AppError::Internal)
                    .with_detail(format!("the ASR loader thread could not start: {error}"))),
                reply: fallback_reply,
            })));
        }
        outcome
    }

    /// Drops the current engine (freeing its memory once no take uses it) and cancels any load in flight.
    pub fn unload(&self) {
        let generation = self.inner.generation.fetch_add(1, Ordering::SeqCst) + 1;
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

/// Builds, loads and warms one engine; runs on the loader thread.
fn load_engine(
    build: &(dyn Fn(&EngineId) -> PortResult<Arc<dyn AsrEngine>> + Send + Sync),
    request: &AsrLoadRequest,
) -> PortResult<(LoadedEngine, AsrLoaded)> {
    let engine = build(&request.engine_id)?;
    let started = Instant::now();
    let accelerator = engine.load(&request.model_dir, request.accelerator)?;
    let load_ms = millis(started.elapsed());
    let started = Instant::now();
    engine.warm_up()?;
    let warm_up_ms = millis(started.elapsed());
    Ok((
        LoadedEngine {
            id: request.engine_id.clone(),
            caps: engine.caps(),
            engine,
            accelerator,
        },
        AsrLoaded {
            accelerator,
            load_ms,
            warm_up_ms,
        },
    ))
}

/// Applies normal priority to the calling thread; a refusal only costs scheduling, so it is logged.
fn prioritize(scheduler: &dyn WorkerScheduler, thread: &str) {
    if let Err(error) = scheduler.prioritize_current_thread(WorkerPriority::Normal) {
        tracing::warn!(
            thread,
            detail = error.detail(),
            "thread runs at default priority"
        );
    }
}

fn panicked(during: &str) -> PortError {
    PortError::new(AppError::Internal)
        .with_detail(format!("the ASR engine panicked while {during}"))
}

fn millis(elapsed: Duration) -> u64 {
    u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
}

/// The worker thread's state.
struct WorkerLoop {
    inbox: Receiver<Message>,
    readiness: Arc<Mutex<AsrReadiness>>,
    sink: Option<Arc<dyn EventSink<AsrReadiness>>>,
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
            if let Ok((engine, _)) = result {
                self.retire(Arc::new(engine));
            }
            let _ = reply.send(Err(PortError::new(AppError::Busy).with_detail(format!(
                "loading `{engine_id}` was superseded by a newer request"
            ))));
            return;
        }
        self.loading = None;
        let reported = match result {
            Ok((engine, loaded)) => {
                tracing::info!(
                    engine = %engine_id,
                    accelerator = ?loaded.accelerator,
                    load_ms = loaded.load_ms,
                    warm_up_ms = loaded.warm_up_ms,
                    "speech engine ready"
                );
                self.failure = None;
                if let Some(old) = self.current.replace(Arc::new(engine)) {
                    self.retire(old);
                }
                Ok(loaded)
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
        // Readiness first, so a caller woken by the reply already reads the new state.
        self.publish();
        // The caller may have stopped waiting; the engine is installed either way.
        let _ = reply.send(reported);
        self.replay();
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
        let result = self.engine_for(take.id).and_then(|engine| {
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
                AsrEvent::SegmentFailed {
                    take: take.id,
                    index,
                    error,
                }
            }
        };
        take.events.emit(event);
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

    /// Recomputes readiness and reports it when it changed.
    fn publish(&self) {
        let next = match (&self.current, &self.loading, &self.failure) {
            (Some(engine), _, _) => AsrReadiness::Ready {
                engine_id: engine.id.clone(),
                accelerator: engine.accelerator,
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
        let mut readiness = self.readiness.lock();
        if *readiness != next {
            *readiness = next.clone();
            drop(readiness);
            if let Some(sink) = &self.sink {
                sink.emit(next);
            }
        }
    }
}
