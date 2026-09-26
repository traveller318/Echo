/*!
 * SOURCE OF TRUTH KEYWORDS: ASR loader, Loader, loader thread, echo-asr-load, load generation, superseded load, LoadedEngine, LoadOutcome, Loaded, load_engine
 * WHAT:  Loader: starts engine loads for the ASR worker. Each load gets the next generation number, tells the
 *        worker it is loading, then on its own short-lived `echo-asr-load` thread builds the engine and has the
 *        AcceleratorPicker load and warm it on the right accelerator; the outcome (an engine to install, the
 *        updated choice of the engine already serving when a background GPU measurement kept it, or the error)
 *        goes back to the worker's inbox as one message.
 * WHY:   Loading and warming take seconds (and `auto` may measure two accelerators), so they never run on the thread
 *        that transcribes: the current engine keeps serving takes until the new one is installed atomically
 *        (02 §8.1). Generations make a newer load or unload win over an older one that finishes late, and the
 *        picker checks the generation between steps so an abandoned measurement stops early instead of keeping a
 *        model loaded; a load that fails or is superseded unloads whatever it had opened. It is shared by the worker handle (loads from settings) and the worker thread itself (the
 *        CPU reload after a GPU stops working), so both follow the same numbering. A panic inside an engine is an
 *        `Internal` load error, never a dead thread.
 * WHERE: Owned by AsrWorker (worker.rs) and its WorkerLoop; engines come from the AsrBuilder (the registry in the
 *        app, fakes in tests); the picker is pipeline/asr/accelerator.rs.
 */

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
};

use parking_lot::Mutex;

use super::{
    accelerator::AcceleratorPicker,
    worker::{AsrBuilder, Message, prioritize},
};
use crate::{
    ports::{AsrEngine, WorkerScheduler},
    types::{
        Accelerator, AcceleratorChoice, AcceleratorRequest, AppError, AsrCaps, AsrLoadRequest,
        BringUpOutcome, EngineId, PortError, PortResult, WorkerPriority,
    },
};

/// An engine that finished loading and warming up.
pub(super) struct LoadedEngine {
    pub id: EngineId,
    pub engine: Arc<dyn AsrEngine>,
    pub caps: AsrCaps,
    /// Where it runs and why; updated in place when the background GPU measurement keeps this engine.
    pub choice: Mutex<AcceleratorChoice>,
    /// Its model folder, for a reload on another accelerator.
    pub model_dir: PathBuf,
    /// Set once a failure on its GPU asked for the CPU reload, so one lost GPU asks once.
    pub gpu_lost: AtomicBool,
}

impl LoadedEngine {
    pub fn accelerator(&self) -> Accelerator {
        self.choice.lock().accelerator
    }

    pub fn choice(&self) -> AcceleratorChoice {
        self.choice.lock().clone()
    }
}

/// What a finished load hands the worker.
pub(super) enum Loaded {
    /// A new engine to install.
    Engine(LoadedEngine),
    /// The background measurement kept the engine already serving; this now describes it.
    KeepCurrent(AcceleratorChoice),
}

/// A load that finished on the loader thread.
pub(super) struct LoadOutcome {
    pub generation: u64,
    pub engine_id: EngineId,
    pub result: PortResult<Loaded>,
    pub reply: Sender<PortResult<AcceleratorChoice>>,
}

/// Starts loads for one ASR worker; clones share the generation counter.
#[derive(Clone)]
pub(super) struct Loader {
    inbox: Sender<Message>,
    build: AsrBuilder,
    scheduler: Arc<dyn WorkerScheduler>,
    picker: AcceleratorPicker,
    generation: Arc<AtomicU64>,
}

impl Loader {
    pub fn new(
        inbox: Sender<Message>,
        build: AsrBuilder,
        scheduler: Arc<dyn WorkerScheduler>,
        picker: AcceleratorPicker,
    ) -> Self {
        Self {
            inbox,
            build,
            scheduler,
            picker,
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Claims the next generation: every load or unload started before it is now outdated.
    pub fn next_generation(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: Loader::start, background engine load, loader thread spawn
     * WHAT:  Announces the load to the worker, then builds, loads and warms `request` on a loader thread; the
     *        returned receiver gets the outcome (callers may drop it).
     * WHY:   The caller (startup, a command, the worker itself) never blocks on a multi-second load.
     * WHERE: AsrWorker::load; WorkerLoop after a GPU stopped working.
     */
    pub fn start(&self, request: AsrLoadRequest) -> Receiver<PortResult<AcceleratorChoice>> {
        let generation = self.next_generation();
        let (reply, outcome) = mpsc::channel();
        // A closed inbox means the worker is shutting down; the load is then pointless but harmless.
        let _ = self.inbox.send(Message::Loading {
            generation,
            engine_id: request.engine_id.clone(),
        });
        // The background GPU measurement must never slow a take; any other load is what the next take waits for.
        let priority = match request.accelerator {
            AcceleratorRequest::MeasureGpu { .. } => WorkerPriority::BelowNormal,
            _ => WorkerPriority::Normal,
        };
        let loader = self.clone();
        let engine_id = request.engine_id.clone();
        let fallback_reply = reply.clone();
        let spawned = thread::Builder::new()
            .name("echo-asr-load".to_owned())
            .spawn(move || {
                prioritize(loader.scheduler.as_ref(), priority, "ASR loader");
                let still_wanted = || loader.generation.load(Ordering::SeqCst) == generation;
                let result = catch_unwind(AssertUnwindSafe(|| {
                    load_engine(&loader.build, &loader.picker, &request, &still_wanted)
                }))
                .unwrap_or_else(|_| Err(panicked("loading the engine")));
                let _ = loader.inbox.send(Message::Loaded(Box::new(LoadOutcome {
                    generation,
                    engine_id: request.engine_id,
                    result,
                    reply,
                })));
            });
        if let Err(error) = spawned {
            let _ = self.inbox.send(Message::Loaded(Box::new(LoadOutcome {
                generation,
                engine_id,
                result: Err(PortError::new(AppError::Internal)
                    .with_detail(format!("the ASR loader thread could not start: {error}"))),
                reply: fallback_reply,
            })));
        }
        outcome
    }
}

/// Builds the engine and brings it up on its accelerator; runs on the loader thread.
fn load_engine(
    build: &AsrBuilder,
    picker: &AcceleratorPicker,
    request: &AsrLoadRequest,
    still_wanted: &dyn Fn() -> bool,
) -> PortResult<Loaded> {
    let engine = build(&request.engine_id)?;
    let outcome = picker
        .bring_up(
            &request.engine_id,
            engine.as_ref(),
            &request.model_dir,
            request.accelerator,
            still_wanted,
        )
        .inspect_err(|_| {
            // A failed or superseded bring-up may have left a session open (a measurement stops between steps).
            if let Err(error) = engine.unload() {
                tracing::warn!(
                    detail = error.detail(),
                    "a failed speech engine load did not unload cleanly"
                );
            }
        })?;
    Ok(match outcome {
        BringUpOutcome::Loaded(choice) => Loaded::Engine(LoadedEngine {
            id: request.engine_id.clone(),
            caps: engine.caps(),
            engine,
            choice: Mutex::new(choice),
            model_dir: request.model_dir.clone(),
            gpu_lost: AtomicBool::new(false),
        }),
        // The picker already unloaded this instance; dropping it frees the rest.
        BringUpOutcome::KeepCurrent(choice) => Loaded::KeepCurrent(choice),
    })
}

pub(super) fn panicked(during: &str) -> PortError {
    PortError::new(AppError::Internal)
        .with_detail(format!("the ASR engine panicked while {during}"))
}
