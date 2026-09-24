/*!
 * SOURCE OF TRUTH KEYWORDS: session actor, SessionActor, SessionHandle, SessionInbox, SessionConfig, SessionEngines, actor loop, sole owner of recording state, session_get_state, shutdown finalize
 * WHAT:  The session actor of 02 §5: one tokio task with an mpsc inbox that owns SessionState, feeds every input
 *        through the pure `transition` and hands the effects to the Runner. SessionHandle is the cloneable way in
 *        (the pill's Stop, the current view, prepare, shutdown); SessionConfig is what the actor works through
 *        (settings, ports, the ASR worker, delivery, paths, database, event sink, engine builders).
 * WHY:   There is exactly one owner of recording state and no copy anywhere else: the view `session_get_state`
 *        returns is computed from the state at the moment of the query, and every change is published as the full
 *        view. Inputs are handled one at a time, in order, and the inputs a transition's effects produce at once
 *        are fed before the next message, so no input can interleave with half-run effects. Time is a monotonic
 *        clock started with the actor (MonotonicMs), so a wall-clock change never bends a take's timings. The
 *        actor is a future the composition root spawns (`run`), not a thread it starts, so it runs on Tauri's
 *        runtime in the app and on a test runtime in tests; the inbox is created first (SessionHandle::new), so
 *        the handle can sit in CommandCtx before the actor is built. Engines come from builders (the registry in
 *        the app, fakes in tests), the same pattern as the ASR worker. The loop ends when every SessionHandle is
 *        gone or on Shutdown, which finalizes every open journal and then answers, so the app waits for the WAV
 *        header before it exits (02 §5).
 * WHERE: app/bootstrap builds it from the same ports as CommandCtx and spawns `run`; `prepare` on RunEvent::Ready,
 *        `shutdown` on RunEvent::Exit; ipc/commands/session.rs calls `view` and `ui_input`.
 */

use std::{
    collections::VecDeque,
    sync::{Arc, mpsc as std_mpsc},
    time::{Duration, Instant},
};

use tokio::sync::{mpsc, oneshot};

use super::{
    arm::VadBuilder,
    hotkey_input,
    inbox::{Message, Outbox, Receiver, Sender},
    runner::Runner,
    transition,
};
use crate::{
    pipeline::{asr::AsrWorker, delivery::Delivery},
    ports::{
        AudioCapture, EventSink, ForegroundApp, HotkeyService, Notifier, TextPolisher,
        WorkerScheduler,
    },
    registry::{self, engines::BuildCtx},
    services::Db,
    types::{
        AppError, AppEvent, AppPaths, EngineId, MonotonicMs, PortError, PortResult, SessionInput,
        SessionState, SessionUiInput, SessionView, SharedSettings,
    },
};

/// Builds a polish stage by its registry id.
pub type PolisherBuilder =
    Arc<dyn Fn(&EngineId) -> PortResult<Arc<dyn TextPolisher>> + Send + Sync>;

/// How the actor builds the engines a take uses besides ASR (which the ASR worker owns).
pub struct SessionEngines {
    /// A fresh voice activity detector.
    pub vad: VadBuilder,
    /// A polish stage by id.
    pub polisher: PolisherBuilder,
}

impl SessionEngines {
    /// Engines built through the registry (only the ones a take asks for are ever constructed, 02 §3.5).
    pub fn registry(ctx: BuildCtx) -> Self {
        let vad_ctx = ctx.clone();
        Self {
            vad: Arc::new(move || registry::engines::build_default_vad(&vad_ctx)),
            polisher: Arc::new(move |id: &EngineId| registry::engines::build_polisher(id, &ctx)),
        }
    }
}

/// Everything the session actor works through.
pub struct SessionConfig {
    /// The one live settings snapshot: read per press (policy), per take (language, device) and per delivery.
    pub settings: SharedSettings,
    pub audio: Arc<dyn AudioCapture>,
    /// Priorities of the capture worker threads.
    pub scheduler: Arc<dyn WorkerScheduler>,
    /// The speech engine's thread (shared with the commands that load engines).
    pub asr: AsrWorker,
    pub hotkeys: Arc<dyn HotkeyService>,
    pub foreground: Arc<dyn ForegroundApp>,
    pub notifier: Arc<dyn Notifier>,
    pub delivery: Delivery,
    pub paths: AppPaths,
    pub db: Db,
    /// SessionStateChanged, AudioLevel, TranscriptSaved, HistoryChanged, MetricsChanged.
    pub events: Arc<dyn EventSink<AppEvent>>,
    pub engines: SessionEngines,
}

/// The way into the session actor; clones share it.
#[derive(Clone)]
pub struct SessionHandle {
    inbox: Sender,
}

/// The inbox the actor will read, created with its handle.
pub struct SessionInbox {
    sender: Sender,
    receiver: Receiver,
}

impl SessionHandle {
    /// A handle and the inbox its actor reads (build the actor with `SessionActor::new`).
    pub fn new() -> (Self, SessionInbox) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (
            Self {
                inbox: sender.clone(),
            },
            SessionInbox { sender, receiver },
        )
    }

    /// Sends a pill input; `Internal` when the actor has stopped.
    pub fn ui_input(&self, input: SessionUiInput) -> PortResult<()> {
        self.send(Message::Ui(input))
    }

    /// The session as the UI should show it now.
    pub async fn view(&self) -> PortResult<SessionView> {
        let (reply, answer) = oneshot::channel();
        self.send(Message::View(reply))?;
        answer.await.map_err(|_| stopped())
    }

    /// The windows exist: start listening to hotkeys and warm up what the first take needs.
    pub fn prepare(&self) {
        if self.send(Message::Prepare).is_err() {
            tracing::error!("the session actor stopped before it could prepare");
        }
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: SessionHandle::shutdown, exit during recording, finalize WAV before exit
     * WHAT:  Asks the actor to finalize every open recording and stop; waits up to `timeout` for it. True when it
     *        finished in time.
     * WHY:   Called from the event loop thread as the app exits, so it blocks (bounded) instead of awaiting; the
     *        actor never needs that thread to finish (Esc is left to the process exit).
     * WHERE: app::run on RunEvent::Exit.
     */
    pub fn shutdown(&self, timeout: Duration) -> bool {
        let (reply, done) = std_mpsc::channel();
        self.send(Message::Shutdown(reply)).is_ok() && done.recv_timeout(timeout).is_ok()
    }

    fn send(&self, message: Message) -> PortResult<()> {
        self.inbox.send(message).map_err(|_| stopped())
    }
}

fn stopped() -> PortError {
    PortError::new(AppError::Internal).with_detail("the session actor has stopped")
}

/// The actor: its state, what runs its effects, its clock and its inbox.
pub struct SessionActor {
    state: SessionState,
    runner: Runner,
    started: Instant,
    receiver: Receiver,
}

impl SessionActor {
    /// Builds the actor over `inbox`; nothing runs until `run` is spawned.
    pub fn new(config: SessionConfig, inbox: SessionInbox) -> Self {
        let SessionInbox { sender, receiver } = inbox;
        // The actor keeps only a weak sender, so the inbox closes when the last SessionHandle is gone.
        let outbox = Outbox::new(&sender);
        drop(sender);
        Self {
            state: SessionState::IDLE,
            runner: Runner::new(config, outbox),
            started: Instant::now(),
            receiver,
        }
    }

    /// Handles messages until every handle is dropped or Shutdown arrives.
    pub async fn run(mut self) {
        tracing::info!("the session actor is running");
        while let Some(message) = self.receiver.recv().await {
            match message {
                Message::Hotkey(event) => {
                    if let Some(input) = hotkey_input::input_for(&event, &self.runner.settings()) {
                        self.feed(VecDeque::from([input])).await;
                    }
                }
                Message::Ui(SessionUiInput::Stop) => {
                    self.feed(VecDeque::from([SessionInput::Stop])).await;
                }
                Message::Worker(reply) => {
                    let mut inputs = VecDeque::new();
                    self.runner.absorb(reply, &mut inputs);
                    self.feed(inputs).await;
                }
                Message::View(reply) => {
                    // The asker may have given up (a closed window); nothing to do then.
                    let _ = reply.send(self.state.phase.view(self.now()));
                }
                Message::Prepare => self.runner.prepare(),
                Message::Shutdown(reply) => {
                    self.runner.shutdown().await;
                    let _ = reply.send(());
                    break;
                }
            }
        }
        tracing::info!("the session actor stopped");
    }

    /// Feeds `inputs` to the machine in order, running each transition's effects before the next input.
    async fn feed(&mut self, mut inputs: VecDeque<SessionInput>) {
        while let Some(input) = inputs.pop_front() {
            let name = input.name();
            let (state, effects) = transition(std::mem::take(&mut self.state), input, self.now());
            self.state = state;
            tracing::debug!(
                input = name,
                status = ?self.state.phase.status(),
                effects = effects.len(),
                "session input"
            );
            for effect in effects {
                self.runner.run(effect, &mut inputs).await;
            }
        }
    }

    fn now(&self) -> MonotonicMs {
        MonotonicMs::from_millis(
            u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
        )
    }
}
