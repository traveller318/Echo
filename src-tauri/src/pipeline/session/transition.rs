/*!
 * SOURCE OF TRUTH KEYWORDS: transition, session state machine, pure transition function, take lifecycle, record debounce, Esc countdown, empty take rule, stale input, Esc pairing
 * WHAT:  `transition(state, input, now)` → (next state, effects): the whole take lifecycle of 02 §5 as one pure
 *        function. Hotkeys start, stop, pause and undo a take; worker replies move it through transcription and
 *        delivery; timers stop it at the longest take, discard it after the Esc countdown and return the pill to
 *        idle once a result has been shown.
 * WHY:   Pure (no I/O, no clock, no ids: they arrive in `now` and the inputs), so every state × input pair and every
 *        scenario is a table test (02 §13), and the actor that runs the effects holds no state of its own (02 §5).
 *        Rules that live here and nowhere else:
 *        - An input that is not valid in the phase, a reply from another take, or a timer that is no longer live
 *          changes nothing and logs one Ignored effect (02 §5 guards). An ignored Armed also closes the capture it
 *          reports (unless it is the current take's open one), so a microphone never stays open for nobody.
 *        - A record press within `debounce_ms` of the last accepted one is ignored in every phase (02 §5). In hold
 *          mode only the release stops; repeated presses while held are ignored.
 *        - A stop that arrives while the microphone is still opening is remembered and applied the moment it is
 *          open, so a quick tap is a (probably empty) take instead of a take that never stops.
 *        - A record press that turns out to be another shortcut (RecordInterrupted: Ctrl+Alt, then T) drops its
 *          take silently, row and audio included, while it is arming or has recorded under `interrupt_grace_ms`;
 *          later, in hold mode, it is a slip while dictating and stops the take like a release; in toggle mode the
 *          take keeps recording.
 *        - Esc is registered only on entering Recording and released on every exit from Recording or
 *          CancelPending: stop, discard, error, and an undo that must finalize (05 W10).
 *        - Recorded time excludes the countdown; the undo restarts the max-duration timer with what is left.
 *        - An undo after the hold key was released or the device was lost finalizes instead of resuming, because
 *          nothing would ever stop the take or feed it audio.
 *        - Device loss finalizes what was captured, never discards it (02 §5, 05 W12).
 *        - Text is joined by segment index whatever order ASR finished in; under `min_speech_ms` of speech (05 A4)
 *          or no text, the take is `empty` and nothing is delivered.
 *        - Errors keep the audio (row `failed` with its code, WAV kept, toast, pill error).
 *        - Effects are listed in execution order, the latency-critical one first (StopCapture on stop, the mic
 *          pause on Esc), and each row write is one UpdateRow.
 * WHERE: Called by the session actor (step 14) for each inbox input; covered by pipeline/session/tests.rs.
 */

use std::collections::BTreeMap;

use super::notices::{START_FAILED_TOAST, TAKE_FAILED_TOAST, stop_toast};
use crate::{
    pipeline::polish::join_segments,
    types::{
        AppError, ArmingTake, AsrOutput, CancelPendingTake, CaptureSummary, DeliveringTake,
        DeliveryOutcome, FinalizingTake, IgnoreReason, IgnoredInput, MonotonicMs, PolishOutcome,
        RecordClock, RecordMode, RecordingTake, SessionCue, SessionEffect, SessionInput,
        SessionPhase, SessionPolicy, SessionState, SessionTimer, SettledTake, StopCause, TakeData,
        TimerToken, TranscriptChange, TranscriptId, TranscriptStatus,
    },
};

/// The next state of the session and the effects to run, in order, for `input` arriving at `now`.
pub fn transition(
    state: SessionState,
    input: SessionInput,
    now: MonotonicMs,
) -> (SessionState, Vec<SessionEffect>) {
    let SessionState {
        phase,
        last_record_press,
        last_timer,
    } = state;
    let mut step = Step {
        now,
        last_record_press,
        last_timer,
        effects: Vec::new(),
        ignored: false,
    };
    let armed = match &input {
        SessionInput::Armed { take, .. } => Some(*take),
        _ => None,
    };
    let phase = match stale(&phase, &input) {
        Some(reason) => step.reject(phase, input.name(), reason),
        None => step.apply(phase, input),
    };
    // A microphone that opened for a take which is no longer arming belongs to nobody: close it, unless it is
    // the current take's own open microphone (a duplicate reply).
    if let Some(take) = armed
        && step.ignored
        && !(phase.has_open_capture() && phase.take_id() == Some(take))
    {
        step.push(SessionEffect::AbortCapture { take });
    }
    let state = SessionState {
        phase,
        last_record_press: step.last_record_press,
        last_timer: step.last_timer,
    };
    (state, step.effects)
}

/// Why `input` cannot concern `phase` at all: a reply from another take or a timer that is not the live one.
fn stale(phase: &SessionPhase, input: &SessionInput) -> Option<IgnoreReason> {
    if let Some(take) = input.take()
        && phase.take_id() != Some(take)
    {
        return Some(IgnoreReason::StaleTake);
    }
    if let Some(timer) = input.timer()
        && phase.timer() != Some(timer)
    {
        return Some(IgnoreReason::StaleTimer);
    }
    None
}

/// What a phase being left still holds, so the exit releases exactly that.
#[derive(Clone, Copy)]
struct Held {
    /// The session hotkeys (Esc) are registered.
    esc: bool,
    /// A timer is live.
    timer: Option<TimerToken>,
    /// The microphone or the ASR take may still be open.
    capture: bool,
}

impl Held {
    const NOTHING: Self = Self {
        esc: false,
        timer: None,
        capture: false,
    };
}

/// A take that is ending in failure: what to write and release.
struct Failure {
    id: TranscriptId,
    policy: SessionPolicy,
    elapsed_ms: u64,
    held: Held,
    /// Audio was captured, so the toast points to a retry from History.
    recorded: bool,
}

/// One transition in progress: the clock reading, the counters it may advance and the effects so far.
struct Step {
    now: MonotonicMs,
    last_record_press: Option<MonotonicMs>,
    last_timer: TimerToken,
    effects: Vec<SessionEffect>,
    /// The input was ignored.
    ignored: bool,
}

impl Step {
    fn push(&mut self, effect: SessionEffect) {
        self.effects.push(effect);
    }

    fn publish(&mut self, phase: &SessionPhase) {
        self.push(SessionEffect::Publish(phase.view(self.now)));
    }

    fn start_timer(&mut self, kind: SessionTimer, after_ms: u64) -> TimerToken {
        self.last_timer = self.last_timer.next();
        self.push(SessionEffect::StartTimer {
            timer: self.last_timer,
            kind,
            after_ms,
        });
        self.last_timer
    }

    /// Leaves `phase` unchanged and logs why `input` was ignored.
    fn reject(
        &mut self,
        phase: SessionPhase,
        input: &'static str,
        reason: IgnoreReason,
    ) -> SessionPhase {
        self.ignored = true;
        self.push(SessionEffect::Ignored(IgnoredInput {
            input,
            status: phase.status(),
            reason,
        }));
        phase
    }

    /// A record press this soon after the last accepted one is a bounce.
    fn debounced(&self, policy: &SessionPolicy) -> bool {
        self.last_record_press
            .is_some_and(|last| self.now.saturating_since(last) < u64::from(policy.debounce_ms))
    }

    fn accept_press(&mut self) {
        self.last_record_press = Some(self.now);
    }

    fn release(&mut self, take: TranscriptId, held: Held) {
        if held.capture {
            self.push(SessionEffect::AbortCapture { take });
        }
        if held.esc {
            self.push(SessionEffect::UnregisterEsc);
        }
        if let Some(timer) = held.timer {
            self.push(SessionEffect::CancelTimer { timer });
        }
    }

    fn apply(&mut self, phase: SessionPhase, input: SessionInput) -> SessionPhase {
        match phase {
            SessionPhase::Idle => self.idle(input),
            SessionPhase::Arming(arming) => self.arming(arming, input),
            SessionPhase::Recording(recording) => self.recording(recording, input),
            SessionPhase::CancelPending(pending) => self.cancel_pending(pending, input),
            SessionPhase::Finalizing(finalizing) => self.finalizing(finalizing, input),
            SessionPhase::Delivering(delivering) => self.delivering(delivering, input),
            settled @ (SessionPhase::Done { .. }
            | SessionPhase::Discarded { .. }
            | SessionPhase::Failed { .. }) => self.settled(settled, input),
        }
    }

    fn idle(&mut self, input: SessionInput) -> SessionPhase {
        let name = input.name();
        match input {
            SessionInput::RecordPressed { next_take, policy } => {
                if self.debounced(&policy) {
                    return self.reject(SessionPhase::Idle, name, IgnoreReason::Debounced);
                }
                self.start(next_take, policy)
            }
            _ => self.reject(SessionPhase::Idle, name, IgnoreReason::NotValidNow),
        }
    }

    /// Done, Discarded and Failed: shown until their hold ends, or replaced at once by a new take.
    fn settled(&mut self, phase: SessionPhase, input: SessionInput) -> SessionPhase {
        let name = input.name();
        match input {
            SessionInput::RecordPressed { next_take, policy } => {
                if self.debounced(&policy) {
                    return self.reject(phase, name, IgnoreReason::Debounced);
                }
                if let Some(timer) = phase.timer() {
                    self.push(SessionEffect::CancelTimer { timer });
                }
                self.start(next_take, policy)
            }
            SessionInput::SettleElapsed { .. } => {
                let idle = SessionPhase::Idle;
                self.publish(&idle);
                idle
            }
            _ => self.reject(phase, name, IgnoreReason::NotValidNow),
        }
    }

    /// A record press starts a take: the pill shows at once, then the row and the microphone open.
    fn start(&mut self, id: TranscriptId, policy: SessionPolicy) -> SessionPhase {
        self.accept_press();
        let phase = SessionPhase::Arming(ArmingTake {
            id,
            policy,
            stop: None,
            interrupted: false,
        });
        self.publish(&phase);
        self.push(SessionEffect::Cue(SessionCue::Start));
        self.push(SessionEffect::Arm { take: id });
        phase
    }

    /// Ends in a result phase shown for `hold_ms`.
    fn settle(
        &mut self,
        id: Option<TranscriptId>,
        elapsed_ms: u64,
        hold_ms: u32,
        result: impl FnOnce(SettledTake) -> SessionPhase,
    ) -> SessionPhase {
        let timer = self.start_timer(SessionTimer::Settle, u64::from(hold_ms));
        let phase = result(SettledTake {
            id,
            elapsed_ms,
            timer,
        });
        self.publish(&phase);
        phase
    }

    fn arming(&mut self, mut arming: ArmingTake, input: SessionInput) -> SessionPhase {
        let name = input.name();
        let mode = arming.policy.record_mode;
        match input {
            SessionInput::Armed { target, .. } => {
                let take = TakeData {
                    id: arming.id,
                    policy: arming.policy,
                    target,
                    segments: BTreeMap::new(),
                };
                if arming.interrupted {
                    let held = Held {
                        capture: true,
                        ..Held::NOTHING
                    };
                    return self.drop_take(&take, 0, held);
                }
                match arming.stop {
                    Some(cause) => self.finalize(take, 0, cause, Held::NOTHING),
                    None => self.begin_recording(take),
                }
            }
            SessionInput::RecordPressed { policy, .. } if mode == RecordMode::Toggle => {
                if self.debounced(&policy) {
                    return self.reject(
                        SessionPhase::Arming(arming),
                        name,
                        IgnoreReason::Debounced,
                    );
                }
                self.accept_press();
                arming.stop.get_or_insert(StopCause::Hotkey);
                SessionPhase::Arming(arming)
            }
            SessionInput::RecordReleased if mode == RecordMode::Hold => {
                arming.stop.get_or_insert(StopCause::Released);
                SessionPhase::Arming(arming)
            }
            SessionInput::RecordInterrupted => {
                arming.interrupted = true;
                SessionPhase::Arming(arming)
            }
            SessionInput::Stop => {
                arming.stop.get_or_insert(StopCause::Ui);
                SessionPhase::Arming(arming)
            }
            SessionInput::DeviceLost { .. } => {
                arming.stop = Some(StopCause::DeviceLost);
                SessionPhase::Arming(arming)
            }
            SessionInput::ModelMissing { model_id, .. } => {
                // Arm touched nothing: no row, no audio, so nothing to write or release.
                self.push(SessionEffect::Cue(SessionCue::Error));
                self.settle(None, 0, arming.policy.notice_hold_ms, |settled| {
                    SessionPhase::Failed {
                        settled,
                        error: AppError::ModelMissing { model_id },
                    }
                })
            }
            SessionInput::Error { error, .. } => self.fail(
                Failure {
                    id: arming.id,
                    policy: arming.policy,
                    elapsed_ms: 0,
                    held: Held {
                        capture: true,
                        ..Held::NOTHING
                    },
                    recorded: false,
                },
                error,
            ),
            _ => self.reject(
                SessionPhase::Arming(arming),
                name,
                IgnoreReason::NotValidNow,
            ),
        }
    }

    /// The microphone is open: take Esc and start counting toward the longest take.
    fn begin_recording(&mut self, take: TakeData) -> SessionPhase {
        self.push(SessionEffect::RegisterEsc);
        let timer = self.start_timer(SessionTimer::MaxDuration, take.policy.max_duration_ms);
        let phase = SessionPhase::Recording(RecordingTake {
            take,
            clock: RecordClock::started(self.now),
            timer,
        });
        self.publish(&phase);
        phase
    }

    fn recording(&mut self, mut recording: RecordingTake, input: SessionInput) -> SessionPhase {
        let name = input.name();
        let mode = recording.take.policy.record_mode;
        match input {
            SessionInput::RecordPressed { policy, .. } if mode == RecordMode::Toggle => {
                if self.debounced(&policy) {
                    return self.reject(
                        SessionPhase::Recording(recording),
                        name,
                        IgnoreReason::Debounced,
                    );
                }
                self.accept_press();
                self.stop_recording(recording, StopCause::Hotkey)
            }
            SessionInput::RecordReleased if mode == RecordMode::Hold => {
                self.stop_recording(recording, StopCause::Released)
            }
            SessionInput::RecordInterrupted => self.interrupted(recording, name),
            SessionInput::Stop => self.stop_recording(recording, StopCause::Ui),
            SessionInput::MaxDurationReached { .. } => {
                self.stop_recording(recording, StopCause::MaxDuration)
            }
            SessionInput::DeviceLost { .. } => {
                self.stop_recording(recording, StopCause::DeviceLost)
            }
            SessionInput::Esc => self.pause(recording),
            SessionInput::SegmentDone { index, output, .. } => {
                record_segment(&mut recording.take, index, output);
                SessionPhase::Recording(recording)
            }
            SessionInput::Error { error, .. } => {
                let elapsed_ms = recording.clock.elapsed_ms(self.now);
                self.fail(
                    Failure {
                        id: recording.take.id,
                        policy: recording.take.policy,
                        elapsed_ms,
                        held: Held {
                            esc: true,
                            timer: Some(recording.timer),
                            capture: true,
                        },
                        recorded: true,
                    },
                    error,
                )
            }
            _ => self.reject(
                SessionPhase::Recording(recording),
                name,
                IgnoreReason::NotValidNow,
            ),
        }
    }

    /// The record press was another shortcut: a young take is dropped; an older one stops in hold mode (a slip
    /// while dictating) and keeps recording in toggle mode.
    fn interrupted(&mut self, recording: RecordingTake, name: &'static str) -> SessionPhase {
        let elapsed_ms = recording.clock.elapsed_ms(self.now);
        if elapsed_ms < recording.take.policy.interrupt_grace_ms {
            let held = Held {
                esc: true,
                timer: Some(recording.timer),
                capture: true,
            };
            return self.drop_take(&recording.take, elapsed_ms, held);
        }
        match recording.take.policy.record_mode {
            RecordMode::Hold => self.stop_recording(recording, StopCause::Released),
            RecordMode::Toggle => self.reject(
                SessionPhase::Recording(recording),
                name,
                IgnoreReason::NotValidNow,
            ),
        }
    }

    /// Drops a take nobody meant to start: release what it holds, delete its row and audio, hide the pill. No cue
    /// and no toast, since the user was typing another shortcut.
    fn drop_take(&mut self, take: &TakeData, elapsed_ms: u64, held: Held) -> SessionPhase {
        self.release(take.id, held);
        self.push(SessionEffect::DeleteTake { take: take.id });
        // No id: the row no longer exists, so any late reply for it is stale.
        self.settle(None, elapsed_ms, take.policy.discard_hold_ms, |settled| {
            SessionPhase::Discarded { settled }
        })
    }

    fn stop_recording(&mut self, recording: RecordingTake, cause: StopCause) -> SessionPhase {
        let elapsed_ms = recording.clock.elapsed_ms(self.now);
        let held = Held {
            esc: true,
            timer: Some(recording.timer),
            capture: false,
        };
        self.finalize(recording.take, elapsed_ms, cause, held)
    }

    /// Esc: pause the microphone first, then swap the max-duration timer for the countdown.
    fn pause(&mut self, recording: RecordingTake) -> SessionPhase {
        let RecordingTake { take, clock, timer } = recording;
        self.push(SessionEffect::PauseCapture { take: take.id });
        self.push(SessionEffect::CancelTimer { timer });
        let countdown_ms = u64::from(take.policy.cancel_countdown_ms);
        let timer = self.start_timer(SessionTimer::Countdown, countdown_ms);
        let phase = SessionPhase::CancelPending(CancelPendingTake {
            take,
            clock: clock.paused(self.now),
            timer,
            countdown_ends_at: self.now.plus_ms(countdown_ms),
            stop_on_undo: None,
        });
        self.publish(&phase);
        phase
    }

    fn cancel_pending(
        &mut self,
        mut pending: CancelPendingTake,
        input: SessionInput,
    ) -> SessionPhase {
        let name = input.name();
        let mode = pending.take.policy.record_mode;
        match input {
            SessionInput::Esc => self.undo(pending),
            SessionInput::CountdownElapsed { .. } => self.discard(pending),
            SessionInput::RecordReleased | SessionInput::RecordInterrupted
                if mode == RecordMode::Hold =>
            {
                pending.stop_on_undo.get_or_insert(StopCause::Released);
                SessionPhase::CancelPending(pending)
            }
            SessionInput::DeviceLost { .. } => {
                pending.stop_on_undo = Some(StopCause::DeviceLost);
                SessionPhase::CancelPending(pending)
            }
            SessionInput::SegmentDone { index, output, .. } => {
                record_segment(&mut pending.take, index, output);
                SessionPhase::CancelPending(pending)
            }
            SessionInput::Error { error, .. } => {
                let elapsed_ms = pending.clock.elapsed_ms(self.now);
                self.fail(
                    Failure {
                        id: pending.take.id,
                        policy: pending.take.policy,
                        elapsed_ms,
                        held: Held {
                            esc: true,
                            timer: Some(pending.timer),
                            capture: true,
                        },
                        recorded: true,
                    },
                    error,
                )
            }
            _ => self.reject(
                SessionPhase::CancelPending(pending),
                name,
                IgnoreReason::NotValidNow,
            ),
        }
    }

    /// The second Esc: keep the take. Recording resumes with everything said before the pause, unless it can
    /// no longer continue, in which case what was said is delivered.
    fn undo(&mut self, pending: CancelPendingTake) -> SessionPhase {
        let CancelPendingTake {
            take,
            clock,
            timer,
            stop_on_undo,
            ..
        } = pending;
        if let Some(cause) = stop_on_undo {
            let held = Held {
                esc: true,
                timer: Some(timer),
                capture: false,
            };
            return self.finalize(take, clock.elapsed_ms(self.now), cause, held);
        }
        self.push(SessionEffect::ResumeCapture { take: take.id });
        self.push(SessionEffect::CancelTimer { timer });
        let clock = clock.resumed(self.now);
        let remaining_ms = take
            .policy
            .max_duration_ms
            .saturating_sub(clock.elapsed_ms(self.now));
        let timer = self.start_timer(SessionTimer::MaxDuration, remaining_ms);
        let phase = SessionPhase::Recording(RecordingTake { take, clock, timer });
        self.publish(&phase);
        phase
    }

    /// The countdown ran out: the take, its row and its audio are gone.
    fn discard(&mut self, pending: CancelPendingTake) -> SessionPhase {
        let id = pending.take.id;
        self.push(SessionEffect::AbortCapture { take: id });
        self.push(SessionEffect::UnregisterEsc);
        self.push(SessionEffect::DeleteTake { take: id });
        self.push(SessionEffect::Cue(SessionCue::Cancel));
        let elapsed_ms = pending.clock.elapsed_ms(self.now);
        // No id: the row no longer exists, so any late reply for it is stale.
        self.settle(
            None,
            elapsed_ms,
            pending.take.policy.discard_hold_ms,
            |settled| SessionPhase::Discarded { settled },
        )
    }

    /// Recording stopped: close the microphone first (the tail segment is the stop → paste latency), release
    /// what the phase held, mark the row as transcribing.
    fn finalize(
        &mut self,
        take: TakeData,
        elapsed_ms: u64,
        cause: StopCause,
        held: Held,
    ) -> SessionPhase {
        self.push(SessionEffect::StopCapture { take: take.id });
        self.release(take.id, held);
        self.push(SessionEffect::UpdateRow {
            take: take.id,
            changes: vec![TranscriptChange::Status(TranscriptStatus::Transcribing)],
        });
        self.push(SessionEffect::Cue(SessionCue::Stop));
        if let Some(toast) = stop_toast(cause) {
            self.push(SessionEffect::Toast(toast));
        }
        let phase = SessionPhase::Finalizing(FinalizingTake {
            take,
            elapsed_ms,
            stopped_at: self.now,
            cause,
        });
        self.publish(&phase);
        phase
    }

    fn finalizing(&mut self, mut finalizing: FinalizingTake, input: SessionInput) -> SessionPhase {
        let name = input.name();
        match input {
            SessionInput::SegmentDone { index, output, .. } => {
                record_segment(&mut finalizing.take, index, output);
                SessionPhase::Finalizing(finalizing)
            }
            SessionInput::AllSegmentsDone { audio, .. } => self.transcribed(finalizing, &audio),
            SessionInput::Error { error, .. } => self.fail(
                Failure {
                    id: finalizing.take.id,
                    policy: finalizing.take.policy,
                    elapsed_ms: finalizing.elapsed_ms,
                    held: Held {
                        capture: true,
                        ..Held::NOTHING
                    },
                    recorded: true,
                },
                error,
            ),
            _ => self.reject(
                SessionPhase::Finalizing(finalizing),
                name,
                IgnoreReason::NotValidNow,
            ),
        }
    }

    /// Every segment is in: store what was measured and heard, then deliver it, or end as `empty` when there
    /// was too little speech or no text (05 A4).
    fn transcribed(&mut self, finalizing: FinalizingTake, audio: &CaptureSummary) -> SessionPhase {
        let FinalizingTake {
            take,
            elapsed_ms,
            stopped_at,
            ..
        } = finalizing;
        let text = join_segments(take.segments.values().map(|output| output.text.as_str()));
        let mut changes = vec![
            TranscriptChange::DurationMs(saturate_u32(audio.duration_ms)),
            TranscriptChange::SpeechMs(saturate_u32(audio.speech_ms)),
        ];
        if audio.speech_ms < take.policy.min_speech_ms || text.is_empty() {
            changes.push(TranscriptChange::Status(TranscriptStatus::Empty));
            self.push(SessionEffect::UpdateRow {
                take: take.id,
                changes,
            });
            self.push(SessionEffect::TakeSettled { take: take.id });
            return self.settle(
                Some(take.id),
                elapsed_ms,
                take.policy.notice_hold_ms,
                |settled| SessionPhase::Done {
                    settled,
                    outcome: DeliveryOutcome::NoSpeech,
                },
            );
        }
        let language = take
            .segments
            .values()
            .find_map(|output| output.language.clone());
        changes.push(TranscriptChange::RawText(Some(text.clone())));
        if language.is_some() {
            changes.push(TranscriptChange::Language(language.clone()));
        }
        // The raw text is stored before delivery, so it is in History even if delivery fails (02 §7.3).
        self.push(SessionEffect::UpdateRow {
            take: take.id,
            changes,
        });
        let phase = SessionPhase::Delivering(DeliveringTake {
            id: take.id,
            policy: take.policy,
            elapsed_ms,
            stopped_at,
        });
        self.publish(&phase);
        self.push(SessionEffect::Deliver {
            take: take.id,
            text,
            target: take.target,
            language,
        });
        phase
    }

    fn delivering(&mut self, delivering: DeliveringTake, input: SessionInput) -> SessionPhase {
        let name = input.name();
        match input {
            SessionInput::Delivered {
                outcome, polish, ..
            } => self.delivered(&delivering, outcome, polish),
            SessionInput::Error { error, .. } => self.fail(
                Failure {
                    id: delivering.id,
                    policy: delivering.policy,
                    elapsed_ms: delivering.elapsed_ms,
                    held: Held::NOTHING,
                    recorded: true,
                },
                error,
            ),
            _ => self.reject(
                SessionPhase::Delivering(delivering),
                name,
                IgnoreReason::NotValidNow,
            ),
        }
    }

    /// Delivery finished: store the final text, what polished it and the stop → delivered latency.
    fn delivered(
        &mut self,
        delivering: &DeliveringTake,
        outcome: DeliveryOutcome,
        polish: PolishOutcome,
    ) -> SessionPhase {
        let latency_ms = saturate_u32(self.now.saturating_since(delivering.stopped_at));
        let final_text = polish.text.trim();
        // Polish can leave nothing (a filler-only take): nothing was delivered, so the take is empty.
        let (outcome, hold_ms, mut changes) =
            if outcome == DeliveryOutcome::NoSpeech || final_text.is_empty() {
                (
                    DeliveryOutcome::NoSpeech,
                    delivering.policy.notice_hold_ms,
                    vec![TranscriptChange::Status(TranscriptStatus::Empty)],
                )
            } else {
                let words = final_text.split_whitespace().count();
                (
                    outcome,
                    delivering.policy.done_hold_ms,
                    vec![
                        TranscriptChange::Status(TranscriptStatus::Done),
                        TranscriptChange::FinalText(Some(final_text.to_owned())),
                        TranscriptChange::WordCount(u32::try_from(words).unwrap_or(u32::MAX)),
                    ],
                )
            };
        changes.push(TranscriptChange::PolisherIds(polish.polisher_ids));
        changes.push(TranscriptChange::LatencyMs(latency_ms));
        self.push(SessionEffect::UpdateRow {
            take: delivering.id,
            changes,
        });
        self.push(SessionEffect::TakeSettled {
            take: delivering.id,
        });
        self.settle(
            Some(delivering.id),
            delivering.elapsed_ms,
            hold_ms,
            |settled| SessionPhase::Done { settled, outcome },
        )
    }

    /// Any error: release everything, keep the audio, mark the row failed with the error's code, tell the user.
    fn fail(&mut self, failure: Failure, error: AppError) -> SessionPhase {
        let Failure {
            id,
            policy,
            elapsed_ms,
            held,
            recorded,
        } = failure;
        self.release(id, held);
        self.push(SessionEffect::UpdateRow {
            take: id,
            changes: vec![
                TranscriptChange::Status(TranscriptStatus::Failed),
                TranscriptChange::ErrorCode(Some(error.code())),
            ],
        });
        self.push(SessionEffect::TakeSettled { take: id });
        self.push(SessionEffect::Toast(if recorded {
            TAKE_FAILED_TOAST
        } else {
            START_FAILED_TOAST
        }));
        self.push(SessionEffect::Cue(SessionCue::Error));
        self.settle(Some(id), elapsed_ms, policy.notice_hold_ms, |settled| {
            SessionPhase::Failed { settled, error }
        })
    }
}

/// Keeps a segment's text under its index; a repeated index keeps the latest text.
fn record_segment(take: &mut TakeData, index: u32, output: AsrOutput) {
    take.segments.insert(index, output);
}

/// `value` as u32, saturating (49 days of milliseconds, far beyond any take).
fn saturate_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
