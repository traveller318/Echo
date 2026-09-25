/*!
 * SOURCE OF TRUTH KEYWORDS: SessionState, SessionPhase, SessionInput, SessionEffect, SessionPolicy, TimerToken, SessionTimer, StopCause, SessionCue, IgnoredInput
 * WHAT:  The data of the session state machine (02 §5): its state (SessionState: the phase of the current take plus
 *        the debounce anchor and the timer counter), every input it accepts (SessionInput), every effect it asks the
 *        actor to carry out (SessionEffect), and the per-take policy it decides with (SessionPolicy).
 * WHY:   `pipeline::session::transition` is a pure function over these shapes, so the whole take lifecycle is
 *        table-tested without a microphone, a database or a clock (02 §13). Time, fresh ids and the settings in
 *        effect arrive inside inputs (MonotonicMs, `RecordPressed { next_take, policy }`), never read. Each phase
 *        holds only what it needs, so an impossible combination (a countdown while finalizing, an outcome while
 *        recording) cannot be written. The policy is copied into the take when it starts, so a setting changed
 *        mid-take (hold ↔ toggle, the countdown) never changes the rules of a take already running. There is one
 *        timer slot: a phase has at most one live timer (max duration, cancel countdown or result hold), and every
 *        start gets a fresh TimerToken, so a timer that fires after it was cancelled is recognised as stale instead
 *        of acting on the wrong phase. Row writes are TranscriptChange lists, the exact shape services/transcripts
 *        applies, so the machine decides every column and the actor just writes them. Inputs and effects can carry
 *        transcript text (AsrOutput, raw and final text), so they are never logged with `{:?}`: logs use `name()`
 *        (02 §10).
 * WHERE: Built and consumed by pipeline/session/transition.rs; driven by the session actor (step 14), which stamps
 *        inputs from hotkeys, the pill, timers, the capture and ASR workers and delivery, and runs the effects
 *        through ports. SessionPolicy is read from settings by registry::settings::session_policy.
 */

use std::collections::BTreeMap;

use super::{
    AppError, AppTarget, AsrOutput, CaptureSummary, DeliveryOutcome, Language, ModelId,
    MonotonicMs, PolishOutcome, RecordMode, SessionStatus, SessionView, Toast, TranscriptChange,
    TranscriptId,
};

/**
 * SOURCE OF TRUTH KEYWORDS: SessionPolicy, record debounce, cancel countdown, max duration, min speech, empty take, result hold, interrupt grace
 * WHAT:  The rules one take runs under: the record mode, the 150 ms record debounce, the Esc countdown, the longest
 *        take, the least speech that is delivered, how long each result stays on the pill, and how young a take
 *        must be to be dropped when its record press turns out to be another app's shortcut.
 * WHY:   Countdown, longest take and mode are settings (02 §3.3); the rest are fixed product rules kept in one
 *        const so tests and the actor agree: the debounce (02 §5), the empty-take threshold (05 A4: under 250 ms of
 *        speech the engine hallucinates a word), the ✓ hold (04 §4: 900 ms) and the error hold (04 §4: 3 s, also
 *        used for "No speech detected" and "Model not installed", which must stay long enough to read). A
 *        discarded take has no pill state of its own (02 §5: "hide pill"), so it holds for 0 ms. A shortcut such
 *        as Ctrl+Alt+T is typed within a second of its modifiers going down, so an interruption inside
 *        `interrupt_grace_ms` of recorded time drops the take; a later one is a slip while dictating.
 * WHERE: registry::settings::session_policy builds it from a snapshot; carried by SessionInput::RecordPressed and
 *        stored in the take; read by pipeline/session/transition.rs.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionPolicy {
    pub record_mode: RecordMode,
    /// A record press this soon after the previous accepted one is ignored, in ms.
    pub debounce_ms: u32,
    /// How long Esc waits before the take is discarded, in ms (`session.cancel_countdown_ms`).
    pub cancel_countdown_ms: u32,
    /// Recorded time after which the take stops by itself, in ms (`session.max_duration_min`).
    pub max_duration_ms: u64,
    /// A take with less speech than this delivers nothing and is stored as `empty`, in ms.
    pub min_speech_ms: u64,
    /// How long a delivered take's ✓ stays up, in ms.
    pub done_hold_ms: u32,
    /// How long an error, "No speech detected" or "Model not installed" stays up, in ms.
    pub notice_hold_ms: u32,
    /// How long a discarded take stays in `Discarded`, in ms.
    pub discard_hold_ms: u32,
    /// Recorded time under which an interrupted record press drops its take, in ms.
    pub interrupt_grace_ms: u64,
}

impl SessionPolicy {
    /// The product rules with every setting at its default.
    pub const DEFAULT: Self = Self {
        record_mode: RecordMode::Hold,
        debounce_ms: 150,
        cancel_countdown_ms: 3_000,
        max_duration_ms: 15 * 60_000,
        min_speech_ms: 250,
        done_hold_ms: 900,
        notice_hold_ms: 3_000,
        discard_hold_ms: 0,
        interrupt_grace_ms: 1_000,
    };
}

impl Default for SessionPolicy {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Identifies one started timer; a timer input whose token is not the live one is stale.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct TimerToken(u32);

impl TimerToken {
    /// The token after this one (wraps after 4 billion timers, long after any earlier one has fired).
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

/// What a started timer measures, and so which input it sends when it fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionTimer {
    /// Recorded time reached `max_duration_ms` → MaxDurationReached.
    MaxDuration,
    /// The Esc countdown ran out → CountdownElapsed.
    Countdown,
    /// A result (✓, error, discard) has been shown long enough → SettleElapsed.
    Settle,
}

impl SessionTimer {
    /// The input the actor sends when a timer of this kind started with `timer` fires.
    pub const fn fired(self, timer: TimerToken) -> SessionInput {
        match self {
            Self::MaxDuration => SessionInput::MaxDurationReached { timer },
            Self::Countdown => SessionInput::CountdownElapsed { timer },
            Self::Settle => SessionInput::SettleElapsed { timer },
        }
    }
}

/// Why recording stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StopCause {
    /// The record hotkey was pressed again (toggle mode).
    Hotkey,
    /// The record hotkey was released (hold mode).
    Released,
    /// The pill's stop button (`session_input(Stop)`).
    Ui,
    /// Recorded time reached the longest-take setting.
    MaxDuration,
    /// The microphone disappeared (05 W12): what was captured is still delivered.
    DeviceLost,
}

/// A sound cue the actor plays when sound cues are on (`general.sound_cues`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionCue {
    /// A take is starting.
    Start,
    /// Recording stopped; the text is on its way.
    Stop,
    /// The take was discarded.
    Cancel,
    /// The take could not start or failed.
    Error,
}

/// Why the machine ignored an input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IgnoreReason {
    /// The input has no meaning in the current phase (02 §5 guards).
    NotValidNow,
    /// A record press within `debounce_ms` of the previous accepted one.
    Debounced,
    /// The input belongs to a take that is no longer the current one.
    StaleTake,
    /// The timer that fired was cancelled or replaced.
    StaleTimer,
}

/// An ignored input, for the log. Carries the input's name only, never its payload (02 §10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IgnoredInput {
    pub input: &'static str,
    pub status: SessionStatus,
    pub reason: IgnoreReason,
}

/**
 * SOURCE OF TRUTH KEYWORDS: RecordClock, recorded time, elapsed excluding pause, pause clock, resume clock
 * WHAT:  Recorded time of a take: the time banked before the last pause plus, while running, the time since.
 * WHY:   Elapsed time and the longest-take limit count only recorded time, never the Esc countdown (the mic is
 *        paused then), so the clock is paused and resumed with the capture.
 * WHERE: RecordingTake and CancelPendingTake; read for SessionView.elapsed_ms and the max-duration timer.
 */
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct RecordClock {
    /// Recorded time up to the last pause, in ms.
    pub banked_ms: u64,
    /// When recording last resumed; None while paused.
    pub running_since: Option<MonotonicMs>,
}

impl RecordClock {
    /// A clock that starts running at `now`.
    pub const fn started(now: MonotonicMs) -> Self {
        Self {
            banked_ms: 0,
            running_since: Some(now),
        }
    }

    /// Recorded time at `now`, in ms.
    pub const fn elapsed_ms(&self, now: MonotonicMs) -> u64 {
        match self.running_since {
            Some(since) => self.banked_ms.saturating_add(now.saturating_since(since)),
            None => self.banked_ms,
        }
    }

    /// This clock stopped at `now`.
    #[must_use]
    pub const fn paused(self, now: MonotonicMs) -> Self {
        Self {
            banked_ms: self.elapsed_ms(now),
            running_since: None,
        }
    }

    /// This clock running again from `now` (a running clock keeps running).
    #[must_use]
    pub const fn resumed(self, now: MonotonicMs) -> Self {
        match self.running_since {
            Some(_) => self,
            None => Self {
                banked_ms: self.banked_ms,
                running_since: Some(now),
            },
        }
    }
}

/// What a take accumulates from the moment its microphone is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TakeData {
    pub id: TranscriptId,
    pub policy: SessionPolicy,
    /// The window that had focus when the take started; None when nothing had focus.
    pub target: Option<AppTarget>,
    /// Transcribed segments by index; joined in index order, whatever order ASR finished them in (02 §6.1).
    pub segments: BTreeMap<u32, AsrOutput>,
}

/// The record hotkey was pressed: the row is being inserted and the microphone opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArmingTake {
    pub id: TranscriptId,
    pub policy: SessionPolicy,
    /// A stop that arrived before the microphone was open; the take finalizes as soon as it is.
    pub stop: Option<StopCause>,
    /// The press turned out to be another shortcut: the take is dropped as soon as the microphone is open.
    pub interrupted: bool,
}

/// The microphone is open and the take is being transcribed while the user speaks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingTake {
    pub take: TakeData,
    pub clock: RecordClock,
    /// The max-duration timer.
    pub timer: TimerToken,
}

/// Esc was pressed: the microphone is paused and the countdown runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelPendingTake {
    pub take: TakeData,
    /// Paused at the Esc press.
    pub clock: RecordClock,
    /// The countdown timer.
    pub timer: TimerToken,
    pub countdown_ends_at: MonotonicMs,
    /// Set when recording cannot simply resume on the second Esc (hold released, device lost): the take is
    /// delivered instead.
    pub stop_on_undo: Option<StopCause>,
}

/// Recording stopped: the tail segment is being transcribed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizingTake {
    pub take: TakeData,
    /// Recorded time, frozen at the stop.
    pub elapsed_ms: u64,
    /// The stop, the start of `latency_ms`.
    pub stopped_at: MonotonicMs,
    pub cause: StopCause,
}

/// The joined text is being polished and delivered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveringTake {
    pub id: TranscriptId,
    pub policy: SessionPolicy,
    pub elapsed_ms: u64,
    pub stopped_at: MonotonicMs,
}

/// A take that has ended and is shown until its hold timer fires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettledTake {
    /// None when no row was ever written (the model was missing).
    pub id: Option<TranscriptId>,
    pub elapsed_ms: u64,
    /// The hold timer.
    pub timer: TimerToken,
}

/// Where the current take is (02 §5 diagram); one variant per SessionStatus.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SessionPhase {
    #[default]
    Idle,
    Arming(ArmingTake),
    Recording(RecordingTake),
    CancelPending(CancelPendingTake),
    Finalizing(FinalizingTake),
    Delivering(DeliveringTake),
    Done {
        settled: SettledTake,
        outcome: DeliveryOutcome,
    },
    Discarded {
        settled: SettledTake,
    },
    Failed {
        settled: SettledTake,
        error: AppError,
    },
}

impl SessionPhase {
    pub const fn status(&self) -> SessionStatus {
        match self {
            Self::Idle => SessionStatus::Idle,
            Self::Arming(_) => SessionStatus::Arming,
            Self::Recording(_) => SessionStatus::Recording,
            Self::CancelPending(_) => SessionStatus::CancelPending,
            Self::Finalizing(_) => SessionStatus::Finalizing,
            Self::Delivering(_) => SessionStatus::Delivering,
            Self::Done { .. } => SessionStatus::Done,
            Self::Discarded { .. } => SessionStatus::Discarded,
            Self::Failed { .. } => SessionStatus::Failed,
        }
    }

    /// The current take's id, when there is one.
    pub const fn take_id(&self) -> Option<TranscriptId> {
        match self {
            Self::Idle => None,
            Self::Arming(arming) => Some(arming.id),
            Self::Recording(recording) => Some(recording.take.id),
            Self::CancelPending(pending) => Some(pending.take.id),
            Self::Finalizing(finalizing) => Some(finalizing.take.id),
            Self::Delivering(delivering) => Some(delivering.id),
            Self::Done { settled, .. }
            | Self::Discarded { settled }
            | Self::Failed { settled, .. } => settled.id,
        }
    }

    /// The take still in progress (Arming → Delivering), when there is one; a settled take is not live.
    pub const fn live_take_id(&self) -> Option<TranscriptId> {
        if self.status().is_in_progress() {
            self.take_id()
        } else {
            None
        }
    }

    /// The token of the phase's live timer, when it has one.
    pub const fn timer(&self) -> Option<TimerToken> {
        match self {
            Self::Recording(recording) => Some(recording.timer),
            Self::CancelPending(pending) => Some(pending.timer),
            Self::Done { settled, .. }
            | Self::Discarded { settled }
            | Self::Failed { settled, .. } => Some(settled.timer),
            Self::Idle | Self::Arming(_) | Self::Finalizing(_) | Self::Delivering(_) => None,
        }
    }

    /// The session hotkeys (Esc) are registered in this phase, and only in it (05 W10).
    pub const fn holds_session_hotkeys(&self) -> bool {
        matches!(self, Self::Recording(_) | Self::CancelPending(_))
    }

    /// The take's microphone is open (live or paused) in this phase, and only in it.
    pub const fn has_open_capture(&self) -> bool {
        matches!(self, Self::Recording(_) | Self::CancelPending(_))
    }

    /// Recorded time at `now`, in ms.
    pub const fn elapsed_ms(&self, now: MonotonicMs) -> u64 {
        match self {
            Self::Idle | Self::Arming(_) => 0,
            Self::Recording(recording) => recording.clock.elapsed_ms(now),
            Self::CancelPending(pending) => pending.clock.elapsed_ms(now),
            Self::Finalizing(finalizing) => finalizing.elapsed_ms,
            Self::Delivering(delivering) => delivering.elapsed_ms,
            Self::Done { settled, .. }
            | Self::Discarded { settled }
            | Self::Failed { settled, .. } => settled.elapsed_ms,
        }
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: SessionPhase::view, SessionView projection, pill view, elapsed_ms, countdown_remaining_ms
     * WHAT:  The SessionView the UI renders for this phase at `now`.
     * WHY:   The view is derived, never stored, so it cannot drift from the phase (the actor holds no second copy,
     *        02 §5). Times saturate into u32 (49 days), far beyond the 60-minute longest take.
     * WHERE: SessionEffect::Publish built by pipeline/session/transition.rs; `session_get_state` (step 14).
     */
    pub fn view(&self, now: MonotonicMs) -> SessionView {
        let countdown_remaining_ms = match self {
            Self::CancelPending(pending) => Some(saturate_u32(
                pending.countdown_ends_at.saturating_since(now),
            )),
            _ => None,
        };
        SessionView {
            status: self.status(),
            transcript_id: self.take_id(),
            elapsed_ms: saturate_u32(self.elapsed_ms(now)),
            countdown_remaining_ms,
            outcome: match self {
                Self::Done { outcome, .. } => Some(*outcome),
                _ => None,
            },
            error: match self {
                Self::Failed { error, .. } => Some(error.clone()),
                _ => None,
            },
        }
    }
}

/// The session state machine's whole state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionState {
    pub phase: SessionPhase,
    /// When the last accepted record press happened (the debounce anchor, kept across takes).
    pub last_record_press: Option<MonotonicMs>,
    /// The most recently issued timer token.
    pub last_timer: TimerToken,
}

impl SessionState {
    /// No take has run yet.
    pub const IDLE: Self = Self {
        phase: SessionPhase::Idle,
        last_record_press: None,
        last_timer: TimerToken(0),
    };
}

/**
 * SOURCE OF TRUTH KEYWORDS: SessionInput, RecordPressed, RecordReleased, RecordInterrupted, Esc, Armed, SegmentDone, AllSegmentsDone, Delivered, DeviceLost, ModelMissing
 * WHAT:  Everything that can happen to a take, stamped by the actor: hotkeys and the pill (RecordPressed,
 *        RecordReleased, RecordInterrupted, Stop, Esc), timers (CountdownElapsed, MaxDurationReached, SettleElapsed) and the replies
 *        of the effects the actor runs (Armed, SegmentDone, AllSegmentsDone, Delivered, Error, DeviceLost,
 *        ModelMissing).
 * WHY:   Replies name their take and timers their token, so a late reply from an ended take or a timer that was
 *        cancelled can never act on the current one (the machine ignores it as stale). RecordPressed carries the id
 *        a new take would get and the policy in effect, because minting an id and reading settings are side
 *        effects the pure machine cannot do; a press that does not start a take simply drops them.
 * WHERE: Built by the session actor; consumed by pipeline::session::transition.
 */
#[derive(Debug, Clone, PartialEq)]
pub enum SessionInput {
    /// The record hotkey went down.
    RecordPressed {
        next_take: TranscriptId,
        policy: SessionPolicy,
    },
    /// The record hotkey came back up (only with `HotkeyCaps.supports_release`).
    RecordReleased,
    /// Another key joined the held (modifier-only) record hotkey: the press was part of another shortcut.
    RecordInterrupted,
    /// The pill's stop button.
    Stop,
    /// The Esc session hotkey.
    Esc,
    /// The Arm effect finished: the row exists and the microphone is open.
    Armed {
        take: TranscriptId,
        target: Option<AppTarget>,
    },
    CountdownElapsed {
        timer: TimerToken,
    },
    MaxDurationReached {
        timer: TimerToken,
    },
    SettleElapsed {
        timer: TimerToken,
    },
    /// Segment `index` was transcribed.
    SegmentDone {
        take: TranscriptId,
        index: u32,
        output: AsrOutput,
    },
    /// After StopCapture: the capture is closed and every segment is transcribed.
    AllSegmentsDone {
        take: TranscriptId,
        audio: CaptureSummary,
    },
    /// The Deliver effect finished: the polished text went where `outcome` says.
    Delivered {
        take: TranscriptId,
        outcome: DeliveryOutcome,
        polish: PolishOutcome,
    },
    /// Something the take depends on failed (capture, ASR, polish, delivery, storage).
    Error {
        take: TranscriptId,
        error: AppError,
    },
    /// The microphone disappeared mid-take (05 W12).
    DeviceLost {
        take: TranscriptId,
    },
    /// The Arm effect found the ASR engine's model not installed; it touched nothing.
    ModelMissing {
        take: TranscriptId,
        model_id: ModelId,
    },
}

impl SessionInput {
    /// The input's name, for logs (never its payload).
    pub const fn name(&self) -> &'static str {
        match self {
            Self::RecordPressed { .. } => "RecordPressed",
            Self::RecordReleased => "RecordReleased",
            Self::RecordInterrupted => "RecordInterrupted",
            Self::Stop => "Stop",
            Self::Esc => "Esc",
            Self::Armed { .. } => "Armed",
            Self::CountdownElapsed { .. } => "CountdownElapsed",
            Self::MaxDurationReached { .. } => "MaxDurationReached",
            Self::SettleElapsed { .. } => "SettleElapsed",
            Self::SegmentDone { .. } => "SegmentDone",
            Self::AllSegmentsDone { .. } => "AllSegmentsDone",
            Self::Delivered { .. } => "Delivered",
            Self::Error { .. } => "Error",
            Self::DeviceLost { .. } => "DeviceLost",
            Self::ModelMissing { .. } => "ModelMissing",
        }
    }

    /// The take a reply belongs to; None for hotkey, pill and timer inputs.
    pub const fn take(&self) -> Option<TranscriptId> {
        match self {
            Self::Armed { take, .. }
            | Self::SegmentDone { take, .. }
            | Self::AllSegmentsDone { take, .. }
            | Self::Delivered { take, .. }
            | Self::Error { take, .. }
            | Self::DeviceLost { take }
            | Self::ModelMissing { take, .. } => Some(*take),
            Self::RecordPressed { .. }
            | Self::RecordReleased
            | Self::RecordInterrupted
            | Self::Stop
            | Self::Esc
            | Self::CountdownElapsed { .. }
            | Self::MaxDurationReached { .. }
            | Self::SettleElapsed { .. } => None,
        }
    }

    /// The token of a timer input; None for every other input.
    pub const fn timer(&self) -> Option<TimerToken> {
        match self {
            Self::CountdownElapsed { timer }
            | Self::MaxDurationReached { timer }
            | Self::SettleElapsed { timer } => Some(*timer),
            _ => None,
        }
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: SessionEffect, effect runner contract, Arm, StopCapture, AbortCapture, UpdateRow, DeleteTake, Deliver, RegisterEsc, StartTimer
 * WHAT:  Everything the machine asks the actor to do, in the order it must be done.
 * WHY:   The actor runs effects in order and holds only resources (the capture, the ASR take, the Esc guard, the
 *        timer), never a copy of the state. Effects that finish later reply with an input: Arm → Armed / Error /
 *        ModelMissing, StopCapture → AllSegmentsDone, Deliver → Delivered / Error. Every effect is idempotent
 *        against a resource that is already gone (a second AbortCapture, a CancelTimer after the timer fired), so
 *        every exit path can release everything it may hold. RegisterEsc and UnregisterEsc are always paired: the
 *        machine emits RegisterEsc only on entering Recording from Arming and UnregisterEsc on every exit from
 *        Recording or CancelPending (05 W10).
 * WHERE: Returned by pipeline::session::transition; run by the session actor's effect runner (step 14).
 */
#[derive(Debug, Clone, PartialEq)]
pub enum SessionEffect {
    /// Emit SessionStateChanged with this view.
    Publish(SessionView),
    /// In this order: read the foreground target, check the ASR engine's model (missing → reply ModelMissing and
    /// touch nothing), insert the take's row as `recording`, open the microphone with the journal and the ASR take;
    /// reply Armed, or Error after releasing whatever was opened.
    Arm {
        take: TranscriptId,
    },
    /// Register the session hotkeys (Esc).
    RegisterEsc,
    /// Release the session hotkeys.
    UnregisterEsc,
    /// Start the one timer slot (replacing a live timer); when it fires, send `kind.fired(timer)`.
    StartTimer {
        timer: TimerToken,
        kind: SessionTimer,
        after_ms: u64,
    },
    /// Stop the timer started with `timer`, if it is still live.
    CancelTimer {
        timer: TimerToken,
    },
    /// Pause the microphone (the Esc countdown).
    PauseCapture {
        take: TranscriptId,
    },
    ResumeCapture {
        take: TranscriptId,
    },
    /// Close the microphone, flush the tail segment to ASR and finish the ASR take; reply AllSegmentsDone once
    /// every segment is in.
    StopCapture {
        take: TranscriptId,
    },
    /// Close the microphone and finalize the journal without transcribing the rest; cancel the ASR take. The WAV
    /// is kept.
    AbortCapture {
        take: TranscriptId,
    },
    /// Apply these column changes to the take's row in one statement.
    UpdateRow {
        take: TranscriptId,
        changes: Vec<TranscriptChange>,
    },
    /// Delete the take's row and its WAV (a discarded take).
    DeleteTake {
        take: TranscriptId,
    },
    /// Polish `text`, deliver it to `target` under the delivery settings, reply Delivered or Error.
    Deliver {
        take: TranscriptId,
        text: String,
        target: Option<AppTarget>,
        language: Option<Language>,
    },
    /// The take's row reached its final state: emit TranscriptSaved, HistoryChanged and MetricsChanged.
    TakeSettled {
        take: TranscriptId,
    },
    /// Show a native toast.
    Toast(Toast),
    /// Play a sound cue when `general.sound_cues` is on.
    Cue(SessionCue),
    /// Log an ignored input.
    Ignored(IgnoredInput),
}

impl SessionEffect {
    /// The effect's name, for logs (never its payload).
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Publish(_) => "Publish",
            Self::Arm { .. } => "Arm",
            Self::RegisterEsc => "RegisterEsc",
            Self::UnregisterEsc => "UnregisterEsc",
            Self::StartTimer { .. } => "StartTimer",
            Self::CancelTimer { .. } => "CancelTimer",
            Self::PauseCapture { .. } => "PauseCapture",
            Self::ResumeCapture { .. } => "ResumeCapture",
            Self::StopCapture { .. } => "StopCapture",
            Self::AbortCapture { .. } => "AbortCapture",
            Self::UpdateRow { .. } => "UpdateRow",
            Self::DeleteTake { .. } => "DeleteTake",
            Self::Deliver { .. } => "Deliver",
            Self::TakeSettled { .. } => "TakeSettled",
            Self::Toast(_) => "Toast",
            Self::Cue(_) => "Cue",
            Self::Ignored(_) => "Ignored",
        }
    }
}

/// `value` as u32, saturating.
fn saturate_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_counts_only_recorded_time() {
        let at = MonotonicMs::from_millis;
        let clock = RecordClock::started(at(1_000));
        assert_eq!(clock.elapsed_ms(at(3_000)), 2_000);
        let paused = clock.paused(at(3_000));
        assert_eq!(paused.elapsed_ms(at(9_000)), 2_000);
        let resumed = paused.resumed(at(9_000));
        assert_eq!(resumed.elapsed_ms(at(9_500)), 2_500);
        assert_eq!(resumed.resumed(at(9_400)), resumed);
    }

    #[test]
    fn every_timer_kind_fires_its_own_input() {
        let token = TimerToken::default().next();
        assert_eq!(
            SessionTimer::MaxDuration.fired(token),
            SessionInput::MaxDurationReached { timer: token }
        );
        assert_eq!(
            SessionTimer::Countdown.fired(token),
            SessionInput::CountdownElapsed { timer: token }
        );
        assert_eq!(
            SessionTimer::Settle.fired(token),
            SessionInput::SettleElapsed { timer: token }
        );
        assert_eq!(SessionTimer::Settle.fired(token).timer(), Some(token));
        assert_ne!(token, token.next());
        assert_eq!(TimerToken(u32::MAX).next(), TimerToken(0));
    }

    #[test]
    fn idle_view_matches_the_ipc_idle_view() {
        assert_eq!(
            SessionState::IDLE.phase.view(MonotonicMs::ZERO),
            SessionView::IDLE
        );
        assert_eq!(SessionState::default(), SessionState::IDLE);
        assert_eq!(SessionPolicy::default(), SessionPolicy::DEFAULT);
    }

    #[test]
    fn countdown_view_counts_down_and_saturates() {
        let take = TakeData {
            id: TranscriptId::generate(),
            policy: SessionPolicy::DEFAULT,
            target: None,
            segments: BTreeMap::new(),
        };
        let phase = SessionPhase::CancelPending(CancelPendingTake {
            take,
            clock: RecordClock {
                banked_ms: 4_000,
                running_since: None,
            },
            timer: TimerToken::default(),
            countdown_ends_at: MonotonicMs::from_millis(10_000),
            stop_on_undo: None,
        });
        let view = phase.view(MonotonicMs::from_millis(8_500));
        assert_eq!(view.status, SessionStatus::CancelPending);
        assert_eq!(view.elapsed_ms, 4_000);
        assert_eq!(view.countdown_remaining_ms, Some(1_500));
        assert_eq!(
            phase
                .view(MonotonicMs::from_millis(11_000))
                .countdown_remaining_ms,
            Some(0)
        );
        assert_eq!(saturate_u32(u64::MAX), u32::MAX);
    }
}
