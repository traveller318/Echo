/*!
 * SOURCE OF TRUTH KEYWORDS: session actor, SessionActor, SessionHandle, SessionInbox, SessionConfig, SessionEngines, PanicReporter, actor loop, sole owner of recording state, session_get_state, shutdown finalize, panic supervision
 * WHAT:  The session actor of 02 §5: one tokio task with an mpsc inbox that owns SessionState, feeds every input
 *        through the pure `transition` and hands the effects to the Runner. SessionHandle is the cloneable way in
 *        (the pill's Stop, the current view, paste-last, prepare, shutdown); SessionConfig is what the actor works through
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
 *        header before it exits (02 §5). A panic while handling a message is caught (pipeline/unwind.rs): the live
 *        take is failed with its audio kept and the actor starts over at Idle, so one bug never leaves Echo deaf
 *        to its hotkeys; a panic anywhere else reaches the actor as a PanicReporter message (02 §12).
 * WHERE: app/bootstrap builds it from the same ports as CommandCtx and spawns `run`; `prepare` on RunEvent::Ready,
 *        `shutdown` on RunEvent::Exit; ipc/commands/session.rs calls `view` and `ui_input`; the panic hook
 *        (app/panics.rs) holds a PanicReporter.
 */

use std::{
    collections::VecDeque,
    ops::ControlFlow,
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
    pipeline::{
        asr::AsrWorker,
        delivery::Delivery,
        polish::{PolishChains, PolisherBuilder},
        unwind::catch_unwind,
    },
    ports::{AudioCapture, EventSink, ForegroundApp, HotkeyService, Notifier, WorkerScheduler},
    registry::{self, engines::BuildCtx},
    services::Db,
    types::{
        AppError, AppEvent, AppPaths, DeliveryOutcome, EngineId, MonotonicMs, PortError,
        PortResult, SessionEffect, SessionInput, SessionPhase, SessionState, SessionUiInput,
        SessionView, SharedSettings,
    },
};

/**
 * SOURCE OF TRUTH KEYWORDS: SessionEngines, take engines, VadBuilder, PolishChains, shared with retry
 * WHAT:  The engines a take uses besides ASR (which the ASR worker owns): a builder for fresh voice activity
 *        detectors and the app's one polish chain. Clones share the chain.
 * WHY:   A live take and a History retry must segment and polish the same way (02 §8.3), and the chain may own a
 *        sidecar that must exist once (05 A13), so the actor and CommandCtx hold clones of the same engines.
 * WHERE: Built by app/bootstrap (`registry`) or tests (`new` over fakes); held by SessionConfig and CommandCtx.
 */
#[derive(Clone)]
pub struct SessionEngines {
    /// A fresh voice activity detector.
    pub vad: VadBuilder,
    /// The polish chain for the settings in effect.
    pub polish: PolishChains,
}

impl SessionEngines {
    /// Engines from a detector builder and a polish stage builder.
    pub fn new(vad: VadBuilder, polisher: PolisherBuilder) -> Self {
        Self {
            vad,
            polish: PolishChains::new(polisher),
        }
    }

    /// Engines built through the registry (only the ones a take asks for are ever constructed, 02 §3.5).
    pub fn registry(ctx: BuildCtx) -> Self {
        let vad_ctx = ctx.clone();
        Self::new(
            Arc::new(move || registry::engines::build_default_vad(&vad_ctx)),
            Arc::new(move |id: &EngineId| registry::engines::build_polisher(id, &ctx)),
        )
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

    /**
     * SOURCE OF TRUTH KEYWORDS: SessionHandle::paste_last, history_paste_last, re-deliver newest take
     * WHAT:  Asks the actor to deliver the newest completed take to the focused window again; answers what reached
     *        the user (pasted or copied) or why nothing did.
     * WHY:   Paste-last writes the clipboard like a take's delivery, so it goes through the runner that owns the
     *        pending clipboard restore (05 W6): one owner, so a restore never puts back a transcript as "the user's
     *        clipboard".
     * WHERE: ipc/commands/history.rs (history_paste_last); the paste-last hotkey posts Message::PasteLast(None).
     */
    pub async fn paste_last(&self) -> PortResult<DeliveryOutcome> {
        let (reply, answer) = oneshot::channel();
        self.send(Message::PasteLast(Some(reply)))?;
        answer.await.map_err(|_| stopped())?
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

    /// A weak reporter the panic hook holds, so a panic anywhere fails the live take (02 §12).
    pub fn panic_reporter(&self) -> PanicReporter {
        PanicReporter(Outbox::new(&self.inbox))
    }

    fn send(&self, message: Message) -> PortResult<()> {
        self.inbox.send(message).map_err(|_| stopped())
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: PanicReporter, panic hook to session, report panic, fail take on panic
 * WHAT:  Tells the session actor that code panicked somewhere in the process; the actor then fails its live take.
 * WHY:   The panic hook runs on whatever thread panicked (a worker, a command, the actor itself) and knows no take:
 *        only the actor owns recording state (02 §5), so the hook only posts a message and the actor decides.
 *        Posting never blocks and holds only a weak sender, so the hook can call it from any thread, at any time,
 *        without keeping the actor alive at exit. A panic the actor catches itself is recovered first; the report
 *        that follows then finds no live take and does nothing.
 * WHERE: Built by SessionHandle::panic_reporter; held by the panic hook app/bootstrap installs (app/panics.rs).
 */
#[derive(Clone)]
pub struct PanicReporter(Outbox);

impl PanicReporter {
    /// Reports a panic; does nothing once the actor has stopped.
    pub fn report(&self) {
        self.0.post(Message::Panicked);
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

    /// Handles messages until every handle is dropped or Shutdown arrives; a panic while handling one fails the
    /// live take and the actor carries on (02 §12).
    pub async fn run(mut self) {
        tracing::info!("the session actor is running");
        while let Some(message) = self.receiver.recv().await {
            match catch_unwind(self.handle(message)).await {
                Some(ControlFlow::Continue(())) => {}
                Some(ControlFlow::Break(())) => break,
                None => self.recover_from_panic().await,
            }
        }
        tracing::info!("the session actor stopped");
    }

    /// Handles one message; Break after Shutdown.
    async fn handle(&mut self, message: Message) -> ControlFlow<()> {
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
            Message::PasteLast(reply) => self.runner.paste_last(reply),
            Message::Prepare => self.runner.prepare(),
            Message::Panicked => self.fail_live_take().await,
            Message::Shutdown(reply) => {
                self.runner.shutdown().await;
                let _ = reply.send(());
                return ControlFlow::Break(());
            }
        }
        ControlFlow::Continue(())
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: fail live take on panic, panic elsewhere, SessionInput Error Internal
     * WHAT:  Fails the take in progress (Arming → Delivering) with `Internal` through the machine, like any error:
     *        everything is released, the row becomes `failed` with its audio kept, a toast and the pill say so.
     * WHY:   02 §12: after a panic anywhere, the state the take depends on can no longer be trusted, so the take
     *        is ended safely and can be retried from History. Going through `transition` keeps the machine the one
     *        place that decides; the reply the panicking work sends later is then stale and ignored.
     * WHERE: Message::Panicked (PanicReporter, installed by app/bootstrap in the panic hook).
     */
    async fn fail_live_take(&mut self) {
        if let Some(take) = self.state.phase.live_take_id() {
            tracing::error!(%take, "a panic ends the take in progress; its audio is kept for a retry");
            self.feed(VecDeque::from([SessionInput::Error {
                take,
                error: AppError::Internal,
            }]))
            .await;
        }
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: recover_from_panic, actor supervision, session restart after panic, Idle after panic
     * WHAT:  After a panic while handling a message: the runner abandons everything it holds (failing unfinished
     *        takes, audio kept), the machine starts over at Idle and the Idle view is published (the pill leaves).
     * WHY:   The panic may have struck between a transition and its effects, so the phase and what the runner holds
     *        may disagree; the runner releases everything it holds plus the take the phase names, and starting over
     *        at Idle keeps hotkeys working without a restart. The state is only replaced after `transition`
     *        returns (`feed`), so the phase is never lost; the debounce anchor and timer counter survive, so no
     *        stale timer token is ever reused. If releasing panics too, the state still resets and the next take
     *        releases what is left (Arm drains the runner).
     * WHERE: `run`.
     */
    async fn recover_from_panic(&mut self) {
        let live = self.state.phase.live_take_id();
        tracing::error!(take = ?live, "the session panicked; the take in progress is failed and the session starts over");
        if catch_unwind(self.runner.abandon(live)).await.is_none() {
            tracing::error!("releasing the take after a panic panicked as well");
        }
        let SessionState {
            last_record_press,
            last_timer,
            ..
        } = std::mem::take(&mut self.state);
        self.state = SessionState {
            phase: SessionPhase::Idle,
            last_record_press,
            last_timer,
        };
        let idle = SessionEffect::Publish(self.state.phase.view(self.now()));
        if catch_unwind(self.runner.run(idle, &mut VecDeque::new()))
            .await
            .is_none()
        {
            tracing::error!("the Idle view could not be published after a panic");
        }
    }

    /// Feeds `inputs` to the machine in order, running each transition's effects before the next input.
    async fn feed(&mut self, mut inputs: VecDeque<SessionInput>) {
        while let Some(input) = inputs.pop_front() {
            let name = input.name();
            // A clone, not a take: if `transition` panics, the state before this input is still known, so the
            // panic recovery can fail exactly the take that was live (inputs are rare, the state is small).
            let (state, effects) = transition(self.state.clone(), input, self.now());
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
