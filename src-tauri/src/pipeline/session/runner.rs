/*!
 * SOURCE OF TRUTH KEYWORDS: effect runner, Runner, run effect, absorb worker reply, TakeSlot, take resources, Esc guard, session timer, clipboard restore, polish chain per take, play sound cue, Bluetooth hint, paste-last hotkey toast, rehearsal
 * WHAT:  Runner: executes every SessionEffect through the ports, in the order the machine lists them, and turns the
 *        replies of the work it started (WorkerReply) back into SessionInputs. It holds the resources of the
 *        current take and nothing else: the open microphone and ASR take (TakeSlot), the Esc guard, the one timer,
 *        a spare voice detector and a pending clipboard restore (the polish chain is the shared PolishChains).
 * WHY:   The machine decides, the runner only does (02 §5): no copy of the state lives here, so the two can never
 *        disagree. Every effect is idempotent against a resource that is already gone (a second abort, a cancel
 *        after the timer fired), which is what lets the machine release everything on every exit path. Slow work
 *        runs off the actor and replies through the inbox: the Arm (a microphone and a detector open), the stop
 *        (joining the capture worker, then waiting for the ASR worker's `Drained`) and the delivery (polish, then
 *        clipboard and SendInput), so a stop, an Esc or a status query is never queued behind them. Blocking work
 *        runs on tokio's blocking pool and its panic becomes an `Internal` reply, so a take can never hang in a
 *        phase waiting for a reply that will not come. AllSegmentsDone is sent only once both the capture summary
 *        and `Drained` are in, whatever order they arrive in. Aborting a capture waits for its journal to be
 *        finalized before the next effect, so DeleteTake never races the capture worker for the WAV (a discard)
 *        and a failed take's audio is complete on disk. The detector a take used comes back for the next take, so
 *        Silero is loaded once, not per press; a detector that failed is rebuilt. The polish chain comes from the
 *        shared PolishChains (SessionEngines), rebuilt only when `polish_plan` changes, so retry uses the same one. With `keep_on_clipboard` off,
 *        the clipboard is given back CLIPBOARD_RESTORE_DELAY after the paste (05 W6); a delivery that starts
 *        before then, or the app exiting, restores it first, so a later take never saves the previous transcript
 *        as "the user's clipboard". A row write that fails is logged and the take goes on: the machine has
 *        already decided, and startup recovery repairs whatever status is left. A take that settles as a success
 *        loses its audio at once when `storage.audio_retention_days` is 0 (pipeline/retention.rs). After a panic
 *        the actor calls `abandon`, which releases everything held and fails the unfinished takes (02 §12).
 *        Cue effects play through SoundCues, which reads `general.sound_cues` at that moment. A take that opens a
 *        Bluetooth microphone shows the one-time Bluetooth hint (05 W11, NoticeBoard) without waiting on it. A
 *        paste-last from the hotkey has no caller to answer, so its failure toasts (NOTHING_TO_PASTE_TOAST when no
 *        take is finished yet). The runner also holds what the session rehearses for onboarding (rehearsal.rs): a
 *        rehearsed hotkey is reported as HotkeyRehearsed instead of routed, and a rehearsed take in an Echo window
 *        is delivered as `Shown` without touching the clipboard or any app. Transcript text is never logged (02 §10).
 * WHERE: Owned by the session actor (actor.rs): `run` for each effect of a transition, `absorb` for each
 *        WorkerReply, `paste_last` on Message::PasteLast, `prepare` on Message::Prepare, `warm` on Message::Warm,
 *        `refresh_hotkeys` on a power event, `shutdown` on Message::Shutdown, `abandon` after a caught panic.
 */

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::Duration,
};

use tokio::task::{JoinError, JoinHandle};

use super::{
    actor::SessionConfig,
    arm::{self, ArmOutcome, ArmRequest},
    inbox::{HotkeyForwarder, Outbox, PasteLastReply, WorkerReply},
    notices::{HOTKEY_UNAVAILABLE_TOAST, TAKE_FAILED_TOAST, paste_last_toast},
    rehearsal::Rehearsal,
};
use crate::{
    pipeline::{
        asr::AsrTake,
        capture::{Capture, CaptureOutcome, journal},
        delivery::{self, CLIPBOARD_RESTORE_DELAY, Delivery},
        history,
        hotkeys::SessionHotkeys,
        notices::NoticeBoard,
        polish::polish_context,
        retention,
    },
    ports::VoiceActivity,
    registry, services,
    types::{
        AppError, AppTarget, AsrEvent, AudioTransport, CaptureEvent, CaptureSummary,
        ClipboardRestore, DeliveryReport, EngineId, HistoryChangeReason, HistoryChanged,
        HotkeyEvent, Language, PolishOutcome, PortError, PortResult, SessionEffect, SessionInput,
        SessionRehearsal, SessionStateChanged, SettingsSnapshot, TargetRule, TimerToken,
        TranscriptChange, TranscriptId, TranscriptStatus,
    },
};

/// Inputs produced while running effects or absorbing replies, fed to the machine in order.
pub(super) type FollowUps = VecDeque<SessionInput>;

/// What the runner holds for one take.
#[derive(Default)]
struct TakeSlot {
    /// The take's row exists (false only when Arm failed before inserting it).
    row: bool,
    /// The engine the take was started for.
    engine: Option<EngineId>,
    capture: Option<Capture>,
    asr: Option<Arc<AsrTake>>,
    /// Set by StopCapture until AllSegmentsDone is sent.
    finishing: Option<Finishing>,
}

/// A stop in progress: AllSegmentsDone needs both the capture summary and the ASR worker's `Drained`.
#[derive(Default)]
struct Finishing {
    summary: Option<CaptureSummary>,
    drained: bool,
}

/// The one live timer.
struct LiveTimer {
    token: TimerToken,
    task: JoinHandle<()>,
}

/// A clipboard restore waiting for CLIPBOARD_RESTORE_DELAY.
struct PendingRestore {
    restore: ClipboardRestore,
    task: JoinHandle<()>,
}

pub(super) struct Runner {
    config: SessionConfig,
    outbox: Outbox,
    takes: HashMap<TranscriptId, TakeSlot>,
    esc: Option<SessionHotkeys>,
    timer: Option<LiveTimer>,
    /// A detector ready for the next take.
    detector: Option<Box<dyn VoiceActivity>>,
    restore: Option<PendingRestore>,
    /// One-time hints (the Bluetooth microphone one).
    notices: NoticeBoard,
    /// What the session rehearses for onboarding (off unless asked).
    rehearsal: Rehearsal,
}

impl Runner {
    pub fn new(config: SessionConfig, outbox: Outbox) -> Self {
        let notices = NoticeBoard {
            settings: config.settings.clone(),
            db: config.db.clone(),
            notifier: Arc::clone(&config.notifier),
            events: Arc::clone(&config.events),
        };
        Self {
            notices,
            config,
            outbox,
            takes: HashMap::new(),
            esc: None,
            timer: None,
            detector: None,
            restore: None,
            rehearsal: Rehearsal::default(),
        }
    }

    /// The settings in effect now.
    pub fn settings(&self) -> Arc<SettingsSnapshot> {
        self.config.settings.current()
    }

    /// What the session rehearses from now on.
    pub fn rehearse(&mut self, rehearsal: SessionRehearsal) {
        tracing::debug!(?rehearsal, "session rehearsal");
        self.rehearsal.set(rehearsal);
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: rehearsed hotkey, HotkeyRehearsed emit, hotkey test without a take
     * WHAT:  Under a hotkey rehearsal with an Echo window focused, reports `event` as HotkeyRehearsed and returns
     *        true (the event is consumed); otherwise returns false and the actor routes it as usual.
     * WHY:   Onboarding's hotkey step shows each press without starting a take (rehearsal.rs decides when that
     *        applies). Reading the focused window is a couple of Win32 calls, fast enough for the actor's turn.
     * WHERE: The actor, for every hotkey event while no take is in progress.
     */
    pub fn rehearsed_hotkey(&self, event: &HotkeyEvent) -> bool {
        match self
            .rehearsal
            .hotkey(event, self.config.foreground.as_ref())
        {
            Some(rehearsed) => {
                self.config.events.emit(rehearsed.into());
                true
            }
            None => false,
        }
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: runner prepare, listen hotkeys, bind record hotkey, hotkey gate bind
     * WHAT:  Starts listening to the hotkey port and binds the always-on hotkeys through the gate (unless paused).
     * WHY:   Runs once the windows exist (05 W19: nothing taken from other apps before the UI). A hotkey that cannot
     *        be bound (another app owns it, 05 W7) is logged and toasted once by the gate, and Echo keeps running: the
     *        user can rebind it in Settings. Nothing heavy loads here; `warm` does that a moment later.
     * WHERE: The actor, on Message::Prepare (app::run on RunEvent::Ready).
     */
    pub fn prepare(&mut self) {
        let listened = self
            .config
            .hotkeys
            .listen(Arc::new(HotkeyForwarder(self.outbox.clone())));
        match listened {
            Ok(()) => {
                self.config.hotkey_gate.bind();
            }
            Err(error) => {
                tracing::error!(
                    detail = error.detail(),
                    "hotkeys cannot be received; dictation cannot start"
                );
                self.toast(&HOTKEY_UNAVAILABLE_TOAST);
            }
        }
    }

    /// Re-registers the always-on hotkeys after a power or session change (05 W8).
    pub fn refresh_hotkeys(&self) {
        self.config.hotkey_gate.refresh();
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: runner warm, warm detector, warm polish chain, deferred warm-up
     * WHAT:  Builds a voice detector ahead of the first take and builds and readies the polish chain.
     * WHY:   So the first take is as fast as the tenth, without loading ONNX Runtime or starting an LLM sidecar while
     *        Windows is still starting (05 W19); a take that comes first builds what it needs itself.
     * WHERE: The actor, on Message::Warm (app/bootstrap, STARTUP_IDLE_DELAY after the windows exist).
     */
    pub fn warm(&mut self) {
        let settings = self.settings();
        if self.detector.is_none() {
            let build = Arc::clone(&self.config.engines.vad);
            spawn_reply(
                &self.outbox,
                move || build(),
                |built| match built {
                    Ok(Ok(detector)) => Some(WorkerReply::Detector(detector)),
                    Ok(Err(error)) => {
                        tracing::warn!(
                            detail = error.detail(),
                            "the voice detector could not be built ahead; the first take builds it"
                        );
                        None
                    }
                    Err(_) => None,
                },
            );
        }
        let chain = self.config.engines.polish.for_settings(&settings);
        tokio::spawn(async move {
            chain.prepare().await;
        });
    }

    /// Runs one effect; inputs it produces at once (a failed pause) are pushed to `follow`.
    pub async fn run(&mut self, effect: SessionEffect, follow: &mut FollowUps) {
        match effect {
            SessionEffect::Publish(view) => {
                self.config.events.emit(SessionStateChanged(view).into())
            }
            SessionEffect::Arm { take, target } => self.arm(take, target).await,
            SessionEffect::RegisterEsc => self.register_esc(),
            SessionEffect::UnregisterEsc => self.esc = None,
            SessionEffect::StartTimer {
                timer,
                kind,
                after_ms,
            } => self.start_timer(timer, kind.fired(timer), after_ms),
            SessionEffect::CancelTimer { timer } => self.cancel_timer(timer),
            SessionEffect::PauseCapture { take } => {
                self.with_capture(take, follow, "pause", Capture::pause);
            }
            SessionEffect::ResumeCapture { take } => {
                self.with_capture(take, follow, "resume", Capture::resume);
            }
            SessionEffect::StopCapture { take } => self.stop_capture(take, follow),
            SessionEffect::AbortCapture { take } => self.abort_capture(take).await,
            SessionEffect::UpdateRow { take, changes } => self.update_row(take, &changes),
            SessionEffect::DeleteTake { take } => self.delete_take(take),
            SessionEffect::Deliver {
                take,
                text,
                target,
                language,
            } => self.deliver(take, text, target, language, follow),
            SessionEffect::TakeSettled { take } => self.take_settled(take).await,
            SessionEffect::Toast(toast) => self.toast(&toast),
            SessionEffect::Cue(cue) => {
                tracing::trace!(?cue, "session sound cue");
                self.config.sounds.play(cue, &self.settings());
            }
            SessionEffect::Ignored(ignored) => tracing::debug!(
                input = ignored.input,
                status = ?ignored.status,
                reason = ?ignored.reason,
                "session input ignored"
            ),
        }
    }

    /// Turns the reply of work started earlier into inputs (pushed to `follow`), or keeps what it hands back.
    pub fn absorb(&mut self, reply: WorkerReply, follow: &mut FollowUps) {
        match reply {
            WorkerReply::Arm(outcome) => self.armed(*outcome, follow),
            WorkerReply::Capture { take, event } => match event {
                CaptureEvent::DeviceLost => {
                    tracing::warn!(%take, "the microphone was disconnected mid-take");
                    follow.push_back(SessionInput::DeviceLost { take });
                }
                CaptureEvent::Failed(error) => follow.push_back(failure(take, error, "capture")),
            },
            WorkerReply::Asr(event) => self.asr_event(event, follow),
            WorkerReply::CaptureFinished { take, outcome } => {
                self.capture_finished(take, *outcome, follow);
            }
            WorkerReply::Delivered {
                take,
                polish,
                result,
            } => self.delivered(take, polish, result, follow),
            WorkerReply::Timer(input) => follow.push_back(input),
            WorkerReply::Detector(detector) => {
                self.detector.get_or_insert(detector);
            }
            WorkerReply::PastedLast { result, reply } => self.pasted_last(result, reply),
        }
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: runner shutdown, finalize journal on exit, release on exit, restore clipboard on exit
     * WHAT:  Stops the timer, leaves Esc to the process exit, finalizes the journal of every open take and gives the
     *        clipboard back if a restore was pending.
     * WHY:   02 §5: an app shutdown during Recording finalizes the WAV header before exit, so the take is complete
     *        on disk; its row stays `recording`/`transcribing` for startup recovery (02 §7.3). Unregistering Esc
     *        here would wait on the event loop that is shutting down, and Windows frees the process's hotkeys
     *        anyway (SessionHotkeys::abandon).
     * WHERE: The actor, on Message::Shutdown (app::run on RunEvent::Exit).
     */
    pub async fn shutdown(&mut self) {
        if let Some(timer) = self.timer.take() {
            timer.task.abort();
        }
        if let Some(esc) = self.esc.take() {
            esc.abandon();
        }
        let takes: Vec<_> = self.takes.drain().collect();
        for (take, slot) in takes {
            if let Some(asr) = &slot.asr {
                asr.cancel();
            }
            if let Some(capture) = slot.capture {
                tracing::info!(%take, "finalizing the recording before exit");
                self.finish_now(capture).await;
            }
        }
        if let Some(pending) = self.restore.take() {
            pending.task.abort();
            let delivery = self.config.delivery.clone();
            // A failed join only means the restore did not run; exit goes on.
            let _ =
                tokio::task::spawn_blocking(move || restore_clipboard(&delivery, &pending.restore))
                    .await;
        }
    }

    // ---- Arm -------------------------------------------------------------------------------------------------

    async fn arm(&mut self, take: TranscriptId, target: TargetRule) {
        // A new take starts only from a settled phase, so anything still held belongs to an ended take.
        let stale: Vec<_> = self.takes.drain().collect();
        for (old, slot) in stale {
            tracing::warn!(take = %old, "releasing what an ended take still held");
            if let Some(asr) = &slot.asr {
                asr.cancel();
            }
            if let Some(capture) = slot.capture {
                self.finish_now(capture).await;
            }
        }
        self.takes.insert(take, TakeSlot::default());
        let request = ArmRequest {
            take,
            target,
            settings: self.settings(),
            detector: self.detector.take(),
            build_detector: Arc::clone(&self.config.engines.vad),
            foreground: Arc::clone(&self.config.foreground),
            audio: Arc::clone(&self.config.audio),
            scheduler: Arc::clone(&self.config.scheduler),
            asr: self.config.asr.clone(),
            db: self.config.db.clone(),
            paths: self.config.paths.clone(),
            events: Arc::clone(&self.config.events),
            outbox: self.outbox.clone(),
        };
        spawn_reply(
            &self.outbox,
            move || arm::arm(request),
            move |outcome| {
                let outcome = outcome.unwrap_or_else(|error| ArmOutcome::Failed {
                    take,
                    error: panicked("starting the take", &error),
                    // The panic may have come after the insert; a write to a missing row is only logged.
                    row: true,
                });
                Some(WorkerReply::Arm(Box::new(outcome)))
            },
        );
    }

    fn armed(&mut self, outcome: ArmOutcome, follow: &mut FollowUps) {
        let take = outcome.take();
        match outcome {
            ArmOutcome::Armed { target, open, .. } => {
                if open.capture.transport() == AudioTransport::Bluetooth {
                    self.notices
                        .show_once(&registry::notices::BLUETOOTH_MIC_NOTICE);
                }
                let slot = self.takes.entry(take).or_default();
                slot.row = true;
                slot.engine = Some(open.engine);
                slot.capture = Some(open.capture);
                slot.asr = Some(open.asr);
                follow.push_back(SessionInput::Armed { take, target });
            }
            ArmOutcome::ModelMissing { model_id, .. } => {
                tracing::warn!(%model_id, "dictation needs the speech model, which is not installed");
                self.takes.remove(&take);
                follow.push_back(SessionInput::ModelMissing { take, model_id });
            }
            ArmOutcome::Failed { error, row, .. } => {
                self.takes.entry(take).or_default().row = row;
                follow.push_back(failure(take, error, "start"));
            }
        }
    }

    // ---- Esc and timers --------------------------------------------------------------------------------------

    fn register_esc(&mut self) {
        // Replacing a guard releases the previous binding first; the machine pairs these, so this is only a guard.
        self.esc = None;
        match SessionHotkeys::bind(Arc::clone(&self.config.hotkeys), &self.settings()) {
            Ok(guard) => self.esc = Some(guard),
            Err(error) => tracing::warn!(
                code = error.error().code().as_str(),
                detail = error.detail(),
                "Esc could not be registered; this take can only be stopped"
            ),
        }
    }

    fn start_timer(&mut self, token: TimerToken, fired: SessionInput, after_ms: u64) {
        if let Some(previous) = self.timer.take() {
            previous.task.abort();
        }
        let outbox = self.outbox.clone();
        let task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(after_ms)).await;
            outbox.reply(WorkerReply::Timer(fired));
        });
        self.timer = Some(LiveTimer { token, task });
    }

    fn cancel_timer(&mut self, token: TimerToken) {
        if self.timer.as_ref().is_some_and(|live| live.token == token)
            && let Some(live) = self.timer.take()
        {
            live.task.abort();
        }
    }

    // ---- Capture ---------------------------------------------------------------------------------------------

    /// Runs `action` on the take's open capture; a failure fails the take.
    fn with_capture(
        &mut self,
        take: TranscriptId,
        follow: &mut FollowUps,
        what: &'static str,
        action: fn(&mut Capture) -> PortResult<()>,
    ) {
        let Some(capture) = self
            .takes
            .get_mut(&take)
            .and_then(|slot| slot.capture.as_mut())
        else {
            return;
        };
        if let Err(error) = action(capture) {
            follow.push_back(failure(take, error, what));
        }
    }

    /// Closes the microphone off the actor; the reply and the ASR worker's `Drained` complete the stop.
    fn stop_capture(&mut self, take: TranscriptId, follow: &mut FollowUps) {
        let Some(slot) = self.takes.get_mut(&take) else {
            return;
        };
        let asr = slot.asr.clone();
        let mut finishing = Finishing::default();
        match slot.capture.take() {
            Some(capture) => spawn_reply(
                &self.outbox,
                move || {
                    // Every segment reaches the ASR take before `finish` asks for `Drained`.
                    let outcome = capture.finish();
                    if let Some(asr) = asr {
                        asr.finish();
                    }
                    outcome
                },
                move |outcome| {
                    let outcome = outcome.unwrap_or_else(|error| CaptureOutcome {
                        summary: CaptureSummary::default(),
                        vad: None,
                        failure: Some(panicked("closing the microphone", &error)),
                    });
                    Some(WorkerReply::CaptureFinished {
                        take,
                        outcome: Box::new(outcome),
                    })
                },
            ),
            None => {
                // Nothing was open: the take has no audio beyond what was already summarized.
                finishing.summary = Some(CaptureSummary::default());
                match asr {
                    Some(asr) => asr.finish(),
                    None => finishing.drained = true,
                }
            }
        }
        slot.finishing = Some(finishing);
        self.check_finished(take, follow);
    }

    fn capture_finished(
        &mut self,
        take: TranscriptId,
        outcome: CaptureOutcome,
        follow: &mut FollowUps,
    ) {
        let CaptureOutcome {
            summary,
            vad,
            failure: failed,
        } = outcome;
        if let Some(vad) = vad {
            self.detector.get_or_insert(vad);
        }
        if summary.dropped_ms > 0 {
            tracing::warn!(%take, dropped_ms = summary.dropped_ms, "audio was lost while the machine was busy");
        }
        if let Some(error) = failed {
            follow.push_back(failure(take, error, "finishing the recording"));
            return;
        }
        if let Some(finishing) = self
            .takes
            .get_mut(&take)
            .and_then(|slot| slot.finishing.as_mut())
        {
            finishing.summary = Some(summary);
        }
        self.check_finished(take, follow);
    }

    /// Sends AllSegmentsDone once the capture summary and `Drained` are both in.
    fn check_finished(&mut self, take: TranscriptId, follow: &mut FollowUps) {
        let Some(slot) = self.takes.get_mut(&take) else {
            return;
        };
        let audio = match &slot.finishing {
            Some(Finishing {
                summary: Some(summary),
                drained: true,
            }) => *summary,
            _ => return,
        };
        slot.finishing = None;
        follow.push_back(SessionInput::AllSegmentsDone { take, audio });
    }

    /// Stops transcribing the take and finalizes its journal before the next effect runs.
    async fn abort_capture(&mut self, take: TranscriptId) {
        let Some(slot) = self.takes.get_mut(&take) else {
            return;
        };
        if let Some(asr) = &slot.asr {
            asr.cancel();
        }
        slot.finishing = None;
        if let Some(capture) = slot.capture.take() {
            self.finish_now(capture).await;
        }
    }

    /// Closes `capture` on the blocking pool and waits for it, keeping its detector for the next take.
    async fn finish_now(&mut self, capture: Capture) {
        match tokio::task::spawn_blocking(move || capture.finish()).await {
            Ok(outcome) => {
                if let Some(vad) = outcome.vad {
                    self.detector.get_or_insert(vad);
                }
                if let Some(error) = outcome.failure {
                    tracing::warn!(
                        detail = error.detail(),
                        "the recording closed after a failure"
                    );
                }
            }
            Err(error) => tracing::error!(%error, "closing the microphone panicked"),
        }
    }

    fn asr_event(&mut self, event: AsrEvent, follow: &mut FollowUps) {
        match event {
            AsrEvent::SegmentDone {
                take,
                index,
                output,
            } => follow.push_back(SessionInput::SegmentDone {
                take,
                index,
                output,
            }),
            AsrEvent::SegmentFailed { take, index, error } => {
                tracing::warn!(%take, index, "a segment could not be transcribed");
                follow.push_back(failure(take, error, "transcription"));
            }
            AsrEvent::Drained { take } => {
                if let Some(finishing) = self
                    .takes
                    .get_mut(&take)
                    .and_then(|slot| slot.finishing.as_mut())
                {
                    finishing.drained = true;
                }
                self.check_finished(take, follow);
            }
        }
    }

    // ---- Rows --------------------------------------------------------------------------------------------------

    /// True unless the take never got a row (Arm failed before inserting it).
    fn has_row(&self, take: TranscriptId) -> bool {
        self.takes.get(&take).is_none_or(|slot| slot.row)
    }

    fn update_row(&self, take: TranscriptId, changes: &[TranscriptChange]) {
        if !self.has_row(take) {
            return;
        }
        if let Err(error) = services::transcripts::update::update(&self.config.db, take, changes) {
            tracing::error!(
                %take,
                code = error.error().code().as_str(),
                detail = error.detail(),
                "the take's row could not be updated"
            );
        }
    }

    /// A discarded take: its row and its WAV are deleted (the capture was finalized by the AbortCapture before).
    fn delete_take(&mut self, take: TranscriptId) {
        let row = self.has_row(take);
        self.takes.remove(&take);
        if row && let Err(error) = services::transcripts::delete::delete(&self.config.db, take) {
            tracing::error!(%take, detail = error.detail(), "the discarded take's row could not be deleted");
        }
        if let Err(error) = journal::remove(&self.config.paths.recording(take)) {
            tracing::error!(%take, detail = error.detail(), "the discarded take's audio could not be deleted");
        }
        self.config.events.emit(
            HistoryChanged {
                reason: HistoryChangeReason::Deleted,
            }
            .into(),
        );
    }

    /// The row reached its final state: the take is released and the UI told.
    async fn take_settled(&mut self, take: TranscriptId) {
        // No slot means nothing is held; the row itself still exists.
        let row = match self.takes.remove(&take) {
            Some(slot) => {
                if let Some(capture) = slot.capture {
                    self.finish_now(capture).await;
                }
                slot.row
            }
            None => true,
        };
        if row {
            self.release_audio_after_success(take);
            history::announce_saved(&self.config.db, self.config.events.as_ref(), take);
        }
    }

    /// With `storage.audio_retention_days` at 0, a take that succeeded loses its audio now, before it is announced,
    /// so History never offers a Retry for audio that is about to go (the journal was closed above).
    fn release_audio_after_success(&self, take: TranscriptId) {
        let policy = registry::settings::retention_policy(&self.settings());
        if let Err(error) =
            retention::release_after_success(&self.config.db, &self.config.paths, policy, take)
        {
            tracing::warn!(
                %take,
                detail = error.detail(),
                "the take's audio could not be deleted after success; the retention sweep retries"
            );
        }
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: runner abandon, panic recovery, fail live take after panic, release everything held
     * WHAT:  After the actor caught a panic: stops the timer, releases Esc, closes every open capture (finalizing its
     *        journal) and ASR take, then marks every take it held, and `live`, failed with `Internal` if its row is
     *        still `recording`/`transcribing`; announces each and toasts once.
     * WHY:   02 §12: a panic fails the take in progress with its audio kept, and the app keeps running. The machine's
     *        state may be half-updated after a panic, so the runner's own record of what it holds decides what to
     *        release, and the stored status decides what to fail: a take that already settled (done, empty) is
     *        never rewritten, and a row the Arm never inserted is simply not found. A row the still-running Arm
     *        inserts later stays `recording` and startup recovery settles it (02 §7.3).
     * WHERE: SessionActor::recover_from_panic (actor.rs).
     */
    pub async fn abandon(&mut self, live: Option<TranscriptId>) {
        if let Some(timer) = self.timer.take() {
            timer.task.abort();
        }
        self.esc = None;
        let held: Vec<_> = self.takes.drain().collect();
        let mut takes = Vec::with_capacity(held.len() + 1);
        for (take, slot) in held {
            if let Some(asr) = &slot.asr {
                asr.cancel();
            }
            if let Some(capture) = slot.capture {
                self.finish_now(capture).await;
            }
            takes.push(take);
        }
        if let Some(live) = live.filter(|live| !takes.contains(live)) {
            takes.push(live);
        }
        let mut failed = false;
        for take in takes {
            failed |= self.fail_unfinished_row(take);
        }
        if failed {
            self.toast(&TAKE_FAILED_TOAST);
        }
    }

    /// Marks `take` failed with `Internal` when its row is still unfinished; true when it did.
    fn fail_unfinished_row(&self, take: TranscriptId) -> bool {
        let db = &self.config.db;
        let unfinished = match services::transcripts::get::get(db, take) {
            Ok(row) => row.status.is_unfinished(),
            // The Arm had not inserted it yet: there is nothing to fail.
            Err(error) if matches!(error.error(), AppError::NotFound { .. }) => false,
            Err(error) => {
                tracing::warn!(%take, detail = error.detail(), "the abandoned take's row could not be read");
                false
            }
        };
        if !unfinished {
            return false;
        }
        let changes = [
            TranscriptChange::Status(TranscriptStatus::Failed),
            TranscriptChange::ErrorCode(Some(AppError::Internal.code())),
        ];
        match services::transcripts::update::update(db, take, &changes) {
            Ok(()) => {
                history::announce_saved(db, self.config.events.as_ref(), take);
                true
            }
            Err(error) => {
                tracing::error!(%take, detail = error.detail(), "the abandoned take could not be marked failed; startup recovery settles it");
                false
            }
        }
    }

    // ---- Delivery ------------------------------------------------------------------------------------------------

    /**
     * SOURCE OF TRUTH KEYWORDS: deliver take, polish then paste, PolishContext per take, delivery off the actor
     * WHAT:  Polishes the joined text with the take's engine caps and language, then delivers it to the take's
     *        target under the delivery settings in effect now (or keeps it in Echo for a rehearsed take); replies
     *        Delivered with the polish outcome.
     * WHY:   Polish may wait up to the slow-stage timeout (an LLM) and the paste brings a window forward, so both run
     *        in a task; a polish that panics delivers the unpolished text rather than nothing (02 §8.3: a stage never
     *        blocks delivery). The settings are read now, so a toggle changed during the take applies to it.
     * WHERE: SessionEffect::Deliver.
     */
    fn deliver(
        &mut self,
        take: TranscriptId,
        text: String,
        target: Option<AppTarget>,
        language: Option<Language>,
        follow: &mut FollowUps,
    ) {
        let settings = self.settings();
        let caps = self
            .takes
            .get(&take)
            .and_then(|slot| slot.engine.as_ref())
            .and_then(registry::engines::find)
            .and_then(|entry| entry.asr_caps());
        let Some(caps) = caps else {
            follow.push_back(failure(
                take,
                PortError::new(AppError::Internal)
                    .with_detail("the take's engine is not a registered ASR engine"),
                "delivery",
            ));
            return;
        };
        let context = polish_context(&settings, caps, language);
        let chain = self.config.engines.polish.for_settings(&settings);
        let policy = registry::settings::delivery_policy(&settings);
        let in_app = self.rehearsal.keeps_in_app(target.as_ref());
        let pending = self.restore.take().map(|pending| {
            pending.task.abort();
            pending.restore
        });
        let delivery = self.config.delivery.clone();
        let outbox = self.outbox.clone();
        tokio::spawn(async move {
            let raw = text.clone();
            let polish = tokio::spawn(async move { chain.run(&text, &context).await })
                .await
                .unwrap_or_else(|error| {
                    tracing::error!(%error, "polishing panicked; the unpolished text is delivered");
                    PolishOutcome {
                        text: raw.trim().to_owned(),
                        polisher_ids: Vec::new(),
                        fallbacks: Vec::new(),
                    }
                });
            let delivered = polish.text.clone();
            let result = tokio::task::spawn_blocking(move || {
                if let Some(previous) = pending {
                    restore_clipboard(&delivery, &previous);
                }
                if in_app {
                    return Ok(delivery::keep_in_app(&delivered));
                }
                delivery.deliver(&delivered, target.as_ref(), policy)
            })
            .await
            .unwrap_or_else(|error| Err(panicked("delivering the text", &error)));
            outbox.reply(WorkerReply::Delivered {
                take,
                polish,
                result,
            });
        });
    }

    fn delivered(
        &mut self,
        take: TranscriptId,
        polish: PolishOutcome,
        result: PortResult<DeliveryReport>,
        follow: &mut FollowUps,
    ) {
        match result {
            Ok(report) => {
                if let Some(reason) = report.copy_reason {
                    tracing::info!(%take, ?reason, "the text was copied instead of pasted");
                }
                if let Some(restore) = report.restore {
                    self.schedule_restore(restore);
                }
                follow.push_back(SessionInput::Delivered {
                    take,
                    outcome: report.outcome,
                    polish,
                });
            }
            Err(error) => follow.push_back(failure(take, error, "delivery")),
        }
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: runner paste_last, paste-last delivery, clipboard restore ownership
     * WHAT:  Delivers the newest completed take to the window `target` names on the blocking pool (after giving back
     *        a clipboard restore still pending); the reply schedules its own restore and answers the caller.
     * WHY:   Same restore rules as a take's delivery (05 W6), owned by the one runner; it never touches recording
     *        state, so it is not a machine input.
     * WHERE: The actor, on Message::PasteLast.
     */
    pub fn paste_last(&mut self, reply: Option<PasteLastReply>, target: TargetRule) {
        let settings = self.settings();
        let pending = self.restore.take().map(|pending| {
            pending.task.abort();
            pending.restore
        });
        let delivery = self.config.delivery.clone();
        let db = self.config.db.clone();
        let foreground = Arc::clone(&self.config.foreground);
        spawn_reply(
            &self.outbox,
            move || {
                if let Some(previous) = pending {
                    restore_clipboard(&delivery, &previous);
                }
                history::paste_last(&db, &delivery, foreground.as_ref(), target, &settings)
            },
            move |result| {
                Some(WorkerReply::PastedLast {
                    result: result
                        .unwrap_or_else(|error| Err(panicked("pasting the last take", &error))),
                    reply,
                })
            },
        );
    }

    fn pasted_last(&mut self, result: PortResult<DeliveryReport>, reply: Option<PasteLastReply>) {
        let answer = result.map(|report| {
            if let Some(restore) = report.restore {
                self.schedule_restore(restore);
            }
            report.outcome
        });
        if let Err(error) = &answer {
            tracing::warn!(
                code = error.error().code().as_str(),
                detail = error.detail(),
                "the last take could not be pasted"
            );
        }
        match reply {
            Some(reply) => {
                // The caller may have given up; the delivery happened either way.
                let _ = reply.send(answer);
            }
            // The hotkey and the tray have no window to answer in, so a failure is a toast.
            None => {
                if let Err(error) = &answer {
                    self.toast(&paste_last_toast(error.error()));
                }
            }
        }
    }

    fn schedule_restore(&mut self, restore: ClipboardRestore) {
        let delivery = self.config.delivery.clone();
        let later = restore.clone();
        let task = tokio::spawn(async move {
            tokio::time::sleep(CLIPBOARD_RESTORE_DELAY).await;
            let _ = tokio::task::spawn_blocking(move || restore_clipboard(&delivery, &later)).await;
        });
        if let Some(previous) = self.restore.replace(PendingRestore { restore, task }) {
            previous.task.abort();
        }
    }

    fn toast(&self, toast: &crate::types::Toast) {
        if let Err(error) = self.config.notifier.toast(toast) {
            tracing::warn!(detail = error.detail(), "a toast could not be shown");
        }
    }
}

/// The input that fails `take` with `error`; its detail is logged here, only its code reaches the machine.
fn failure(take: TranscriptId, error: PortError, during: &'static str) -> SessionInput {
    tracing::error!(
        %take,
        during,
        code = error.error().code().as_str(),
        detail = error.detail(),
        "the take failed"
    );
    SessionInput::Error {
        take,
        error: error.into_app_error(),
    }
}

/// The error a panicked blocking call becomes.
fn panicked(during: &str, error: &JoinError) -> PortError {
    PortError::new(AppError::Internal).with_detail(format!("{during} panicked: {error}"))
}

/// Gives the clipboard back; the outcome only matters to the log.
fn restore_clipboard(delivery: &Delivery, restore: &ClipboardRestore) {
    match delivery.restore_clipboard(restore) {
        Ok(true) => tracing::debug!("the previous clipboard was restored"),
        Ok(false) => tracing::debug!("the clipboard changed after the paste; it was left alone"),
        Err(error) => tracing::warn!(
            detail = error.detail(),
            "the previous clipboard could not be restored"
        ),
    }
}

/// Runs `work` on the blocking pool and posts what `reply` makes of its result (None posts nothing).
fn spawn_reply<T: Send + 'static>(
    outbox: &Outbox,
    work: impl FnOnce() -> T + Send + 'static,
    reply: impl FnOnce(Result<T, JoinError>) -> Option<WorkerReply> + Send + 'static,
) {
    let outbox = outbox.clone();
    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(work).await;
        if let Some(message) = reply(result) {
            outbox.reply(message);
        }
    });
}
