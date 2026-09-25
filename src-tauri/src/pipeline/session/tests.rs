/*!
 * SOURCE OF TRUTH KEYWORDS: session transition tests, state machine table test, every state every input, Esc pairing test, session scenarios, session fuzz
 * WHAT:  Tests of `transition`: every phase × every input against the 02 §5 table (both record modes), the
 *        invariants every transition keeps, the take scenarios (normal take, Esc discard, Esc undo, device loss,
 *        error, silence, hold mode, model missing, debounce, max duration, stale replies), and long seeded random
 *        runs that simulate the actor's resources (Esc registration, the timer slot, the microphone).
 * WHY:   02 §13 asks for the transition table to be tested exhaustively; the simulated resources prove the pairing
 *        rules the actor relies on (05 W10: Esc is never left registered; one timer slot; a microphone is never
 *        left open) over sequences no hand-written scenario would think of. One file for the folder's tests,
 *        like pipeline/asr/tests.rs.
 * WHERE: `cargo test` (pipeline::session::tests).
 */

use super::{
    DEVICE_LOST_TOAST, MAX_DURATION_TOAST, START_FAILED_TOAST, TAKE_FAILED_TOAST, transition,
};
use crate::types::{
    AppError, AppErrorCode, AppTarget, AsrOutput, CaptureSummary, DeliveryOutcome, EngineId,
    IgnoreReason, IgnoredInput, Language, ModelId, MonotonicMs, PolishOutcome, RecordMode,
    SessionCue, SessionEffect, SessionInput, SessionPhase, SessionPolicy, SessionState,
    SessionStatus, SessionTimer, SessionView, StopCause, TimerToken, TranscriptChange,
    TranscriptId, TranscriptStatus, WindowHandle,
};

const MINUTE_MS: u64 = 60_000;

fn at(millis: u64) -> MonotonicMs {
    MonotonicMs::from_millis(millis)
}

fn policy(mode: RecordMode) -> SessionPolicy {
    SessionPolicy {
        record_mode: mode,
        ..SessionPolicy::DEFAULT
    }
}

fn target() -> AppTarget {
    AppTarget {
        window: WindowHandle::from_raw(42),
        process_id: 7,
        exe_name: Some(String::from("notepad.exe")),
        work_area: None,
        elevated: false,
    }
}

fn output(text: &str) -> AsrOutput {
    AsrOutput {
        text: text.to_owned(),
        language: None,
    }
}

fn audio(speech_ms: u64) -> CaptureSummary {
    CaptureSummary {
        duration_ms: speech_ms + 1_000,
        speech_ms,
        ..CaptureSummary::default()
    }
}

fn polished(text: &str) -> PolishOutcome {
    PolishOutcome {
        text: text.to_owned(),
        polisher_ids: vec![EngineId::from_static("rules")],
        fallbacks: Vec::new(),
    }
}

/// Drives the machine the way the actor will: one input at a time, at an explicit time.
struct Rig {
    state: SessionState,
    now: u64,
    mode: RecordMode,
    effects: Vec<SessionEffect>,
    /// The id the last record press offered.
    offered: TranscriptId,
}

impl Rig {
    fn new(mode: RecordMode) -> Self {
        Self {
            state: SessionState::IDLE,
            now: 1_000,
            mode,
            effects: Vec::new(),
            offered: TranscriptId::generate(),
        }
    }

    fn send(&mut self, input: SessionInput) -> &[SessionEffect] {
        let (state, effects) = transition(self.state.clone(), input, at(self.now));
        self.state = state;
        self.effects = effects;
        &self.effects
    }

    fn after(&mut self, millis: u64) -> &mut Self {
        self.now += millis;
        self
    }

    fn status(&self) -> SessionStatus {
        self.state.phase.status()
    }

    fn take(&self) -> TranscriptId {
        self.state
            .phase
            .take_id()
            .expect("the current phase has a take")
    }

    fn timer(&self) -> TimerToken {
        self.state
            .phase
            .timer()
            .expect("the current phase has a timer")
    }

    fn view(&self) -> SessionView {
        self.state.phase.view(at(self.now))
    }

    fn press(&mut self) -> &[SessionEffect] {
        self.offered = TranscriptId::generate();
        let input = SessionInput::RecordPressed {
            next_take: self.offered,
            policy: policy(self.mode),
        };
        self.send(input)
    }

    fn armed(&mut self) -> &[SessionEffect] {
        let take = self.take();
        self.send(SessionInput::Armed {
            take,
            target: Some(target()),
        })
    }

    fn segment(&mut self, index: u32, text: &str) -> &[SessionEffect] {
        let take = self.take();
        self.send(SessionInput::SegmentDone {
            take,
            index,
            output: output(text),
        })
    }

    fn all_segments(&mut self, speech_ms: u64) -> &[SessionEffect] {
        let take = self.take();
        self.send(SessionInput::AllSegmentsDone {
            take,
            audio: audio(speech_ms),
        })
    }

    fn delivered(&mut self, outcome: DeliveryOutcome, text: &str) -> &[SessionEffect] {
        let take = self.take();
        self.send(SessionInput::Delivered {
            take,
            outcome,
            polish: polished(text),
        })
    }

    fn error(&mut self, error: AppError) -> &[SessionEffect] {
        let take = self.take();
        self.send(SessionInput::Error { take, error })
    }

    fn fire(&mut self, kind: SessionTimer) -> &[SessionEffect] {
        let timer = self.timer();
        self.send(kind.fired(timer))
    }

    /// Idle → Recording.
    fn recording(mode: RecordMode) -> Self {
        let mut rig = Self::new(mode);
        rig.press();
        rig.after(100).armed();
        rig
    }

    /// Idle → Recording → Finalizing, with one segment transcribed.
    fn finalizing(mode: RecordMode) -> Self {
        let mut rig = Self::recording(mode);
        rig.after(1_000).segment(0, "Hello there.");
        match mode {
            RecordMode::Toggle => rig.after(1_000).press(),
            RecordMode::Hold => rig.after(1_000).send(SessionInput::RecordReleased),
        };
        rig
    }

    fn delivering(mode: RecordMode) -> Self {
        let mut rig = Self::finalizing(mode);
        rig.after(50).all_segments(1_500);
        rig
    }
}

fn effect_count(effects: &[SessionEffect], pick: impl Fn(&SessionEffect) -> bool) -> usize {
    effects.iter().filter(|effect| pick(effect)).count()
}

fn has<T>(items: &[T], pick: impl Fn(&T) -> bool) -> bool {
    items.iter().any(pick)
}

fn row_changes(effects: &[SessionEffect]) -> Vec<TranscriptChange> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            SessionEffect::UpdateRow { changes, .. } => Some(changes.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

fn ignored(effects: &[SessionEffect]) -> Option<IgnoredInput> {
    match effects {
        [SessionEffect::Ignored(ignored)] => Some(*ignored),
        _ => None,
    }
}

/// A fixture of each of the nine phases, reached from Idle through the machine itself.
fn fixtures(mode: RecordMode) -> Vec<Rig> {
    let idle = Rig::new(mode);
    let mut arming = Rig::new(mode);
    arming.press();
    let recording = Rig::recording(mode);
    let mut cancel_pending = Rig::recording(mode);
    cancel_pending.after(500).send(SessionInput::Esc);
    let finalizing = Rig::finalizing(mode);
    let delivering = Rig::delivering(mode);
    let mut done = Rig::delivering(mode);
    done.after(100)
        .delivered(DeliveryOutcome::Pasted, "Hello there. ");
    let mut discarded = Rig::recording(mode);
    discarded.after(500).send(SessionInput::Esc);
    discarded.after(3_000).fire(SessionTimer::Countdown);
    let mut failed = Rig::recording(mode);
    failed.after(500).error(AppError::Asr);
    let rigs = vec![
        idle,
        arming,
        recording,
        cancel_pending,
        finalizing,
        delivering,
        done,
        discarded,
        failed,
    ];
    let statuses: Vec<_> = rigs.iter().map(Rig::status).collect();
    assert_eq!(
        statuses,
        [
            SessionStatus::Idle,
            SessionStatus::Arming,
            SessionStatus::Recording,
            SessionStatus::CancelPending,
            SessionStatus::Finalizing,
            SessionStatus::Delivering,
            SessionStatus::Done,
            SessionStatus::Discarded,
            SessionStatus::Failed,
        ],
        "every phase has a fixture"
    );
    rigs
}

/// Every input, addressed to `phase`'s take and live timer when it has them.
fn every_input(phase: &SessionPhase, mode: RecordMode) -> Vec<SessionInput> {
    let take = phase.take_id().unwrap_or_else(TranscriptId::generate);
    let timer = phase.timer().unwrap_or_default();
    vec![
        SessionInput::RecordPressed {
            next_take: TranscriptId::generate(),
            policy: policy(mode),
        },
        SessionInput::RecordReleased,
        SessionInput::RecordInterrupted,
        SessionInput::Stop,
        SessionInput::Esc,
        SessionInput::Armed {
            take,
            target: Some(target()),
        },
        SessionInput::CountdownElapsed { timer },
        SessionInput::MaxDurationReached { timer },
        SessionInput::SettleElapsed { timer },
        SessionInput::SegmentDone {
            take,
            index: 9,
            output: output("More."),
        },
        SessionInput::AllSegmentsDone {
            take,
            audio: audio(2_000),
        },
        SessionInput::Delivered {
            take,
            outcome: DeliveryOutcome::Pasted,
            polish: polished("Hello there."),
        },
        SessionInput::Error {
            take,
            error: AppError::Storage,
        },
        SessionInput::DeviceLost { take },
        SessionInput::ModelMissing {
            take,
            model_id: ModelId::from_static("parakeet-tdt-0.6b-v3"),
        },
    ]
}

/// 02 §5: the status each phase moves to for each input (addressed to the current take and live timer).
fn expected(from: SessionStatus, input: &SessionInput, mode: RecordMode) -> SessionStatus {
    use SessionStatus as S;
    let hold = mode == RecordMode::Hold;
    match (from, input) {
        (S::Idle, SessionInput::RecordPressed { .. }) => S::Arming,
        (S::Arming, SessionInput::Armed { .. }) => S::Recording,
        (S::Arming, SessionInput::ModelMissing { .. } | SessionInput::Error { .. }) => S::Failed,
        (S::Recording, SessionInput::RecordPressed { .. }) if !hold => S::Finalizing,
        (S::Recording, SessionInput::RecordReleased | SessionInput::RecordInterrupted) if hold => {
            S::Finalizing
        }
        (
            S::Recording,
            SessionInput::Stop
            | SessionInput::MaxDurationReached { .. }
            | SessionInput::DeviceLost { .. },
        ) => S::Finalizing,
        (S::Recording, SessionInput::Esc) => S::CancelPending,
        (S::CancelPending, SessionInput::Esc) => S::Recording,
        (S::CancelPending, SessionInput::CountdownElapsed { .. }) => S::Discarded,
        (S::Finalizing, SessionInput::AllSegmentsDone { .. }) => S::Delivering,
        (S::Delivering, SessionInput::Delivered { .. }) => S::Done,
        (
            S::Recording | S::CancelPending | S::Finalizing | S::Delivering,
            SessionInput::Error { .. },
        ) => S::Failed,
        (S::Done | S::Discarded | S::Failed, SessionInput::RecordPressed { .. }) => S::Arming,
        (S::Done | S::Discarded | S::Failed, SessionInput::SettleElapsed { .. }) => S::Idle,
        (same, _) => same,
    }
}

/// The rules every single transition keeps, whatever the phase and input.
fn assert_invariants(
    before: &SessionState,
    input: &SessionInput,
    after: &SessionState,
    effects: &[SessionEffect],
) {
    let context = format!(
        "{} in {:?} → {:?}",
        input.name(),
        before.phase.status(),
        after.phase.status()
    );
    // 05 W10: Esc is registered exactly while the phase holds it.
    let registers = effect_count(effects, |effect| {
        matches!(effect, SessionEffect::RegisterEsc)
    });
    let releases = effect_count(effects, |effect| {
        matches!(effect, SessionEffect::UnregisterEsc)
    });
    assert!(registers <= 1 && releases <= 1, "{context}");
    assert_eq!(
        i32::from(before.phase.holds_session_hotkeys()) + i32::try_from(registers).unwrap()
            - i32::try_from(releases).unwrap(),
        i32::from(after.phase.holds_session_hotkeys()),
        "{context}: Esc registration is paired"
    );
    // The UI hears about every status change, and only about those.
    let publishes: Vec<_> = effects
        .iter()
        .filter_map(|effect| match effect {
            SessionEffect::Publish(view) => Some(view.clone()),
            _ => None,
        })
        .collect();
    if before.phase.status() == after.phase.status() {
        assert!(publishes.is_empty(), "{context}: nothing to publish");
    } else {
        assert_eq!(publishes.len(), 1, "{context}: one publish");
        assert_eq!(publishes[0].status, after.phase.status(), "{context}");
        assert_eq!(
            publishes[0].transcript_id,
            after.phase.take_id(),
            "{context}"
        );
    }
    // A live timer that is replaced or left behind is cancelled, unless it is the one that fired.
    if let Some(live) = before.phase.timer()
        && after.phase.timer() != Some(live)
        && input.timer() != Some(live)
    {
        assert!(
            effects.contains(&SessionEffect::CancelTimer { timer: live }),
            "{context}: the live timer is cancelled"
        );
    }
    if let Some(next) = after.phase.timer()
        && before.phase.timer() != Some(next)
    {
        assert!(
            has(effects, |effect| matches!(
                effect,
                SessionEffect::StartTimer { timer, .. } if *timer == next
            )),
            "{context}: the new timer is started"
        );
    }
    // Leaving an open microphone always closes it.
    if before.phase.has_open_capture() && !after.phase.has_open_capture() {
        assert!(
            has(effects, |effect| matches!(
                effect,
                SessionEffect::StopCapture { .. } | SessionEffect::AbortCapture { .. }
            )),
            "{context}: the microphone is closed"
        );
    }
    // An ignored input changes nothing but the log.
    if has(effects, |effect| {
        matches!(effect, SessionEffect::Ignored(_))
    }) {
        assert_eq!(before, after, "{context}: ignored inputs change nothing");
    }
}

#[test]
fn every_state_times_every_input_follows_the_table() {
    for mode in [RecordMode::Toggle, RecordMode::Hold] {
        for fixture in fixtures(mode) {
            for input in every_input(&fixture.state.phase, mode) {
                let before = fixture.state.clone();
                let from = before.phase.status();
                let want = expected(from, &input, mode);
                // Well past the debounce, so a press is judged by the table alone.
                let now = at(fixture.now + 10_000);
                let (after, effects) = transition(before.clone(), input.clone(), now);
                let context = format!("{mode:?}: {} in {from:?}", input.name());
                assert_eq!(after.phase.status(), want, "{context}");
                assert_invariants(&before, &input, &after, &effects);
                if want == from && after == before {
                    let ignored = ignored(effects.get(..1).unwrap_or_default())
                        .unwrap_or_else(|| panic!("{context}: an unchanged state logs why"));
                    assert_eq!(ignored.input, input.name(), "{context}");
                    assert_eq!(ignored.status, from, "{context}");
                }
            }
        }
    }
}

#[test]
fn inputs_that_only_update_the_take_emit_nothing() {
    // Accepted without a status change: a segment is stored, a stop is remembered for later.
    let mut recording = Rig::recording(RecordMode::Toggle);
    assert!(recording.after(10).segment(3, "Later.").is_empty());
    let mut finalizing = Rig::finalizing(RecordMode::Toggle);
    assert!(finalizing.after(10).segment(1, "Tail.").is_empty());
    let mut arming = Rig::new(RecordMode::Toggle);
    arming.press();
    assert!(arming.after(10).send(SessionInput::Stop).is_empty());
    let mut pending = Rig::recording(RecordMode::Hold);
    pending.after(10).send(SessionInput::Esc);
    assert!(
        pending
            .after(10)
            .send(SessionInput::RecordReleased)
            .is_empty()
    );
}

#[test]
fn a_normal_take_runs_every_effect_in_order() {
    let mut rig = Rig::new(RecordMode::Toggle);
    let arming_effects = rig.press().to_vec();
    let take = rig.offered;
    assert_eq!(rig.take(), take, "the offered id becomes the take");
    assert_eq!(
        arming_effects,
        [
            SessionEffect::Publish(SessionView {
                status: SessionStatus::Arming,
                transcript_id: Some(take),
                ..SessionView::IDLE
            }),
            SessionEffect::Arm { take },
        ]
    );

    let recording_effects = rig.after(120).armed().to_vec();
    let max_timer = rig.timer();
    assert_eq!(
        recording_effects,
        [
            SessionEffect::RegisterEsc,
            SessionEffect::StartTimer {
                timer: max_timer,
                kind: SessionTimer::MaxDuration,
                after_ms: 15 * MINUTE_MS,
            },
            SessionEffect::Publish(SessionView {
                status: SessionStatus::Recording,
                transcript_id: Some(take),
                ..SessionView::IDLE
            }),
            SessionEffect::Cue(SessionCue::Start),
        ]
    );

    // ASR finishes segments out of order; text is joined by index.
    assert!(rig.after(2_000).segment(1, "world").is_empty());
    assert!(rig.after(100).segment(0, "Hello.").is_empty());
    assert_eq!(rig.view().elapsed_ms, 2_100);

    let finalizing_effects = rig.after(2_900).press().to_vec();
    assert_eq!(
        finalizing_effects,
        [
            SessionEffect::StopCapture { take },
            SessionEffect::UnregisterEsc,
            SessionEffect::CancelTimer { timer: max_timer },
            SessionEffect::UpdateRow {
                take,
                changes: vec![TranscriptChange::Status(TranscriptStatus::Transcribing)],
            },
            SessionEffect::Cue(SessionCue::Stop),
            SessionEffect::Publish(SessionView {
                status: SessionStatus::Finalizing,
                transcript_id: Some(take),
                elapsed_ms: 5_000,
                ..SessionView::IDLE
            }),
        ]
    );

    // The tail segment arrives after the stop, with the detected language.
    let tail = SessionInput::SegmentDone {
        take,
        index: 2,
        output: AsrOutput {
            text: String::from("how are you"),
            language: Some(Language::from_static("en")),
        },
    };
    assert!(rig.after(40).send(tail).is_empty());
    let delivering_effects = rig.after(60).all_segments(3_200).to_vec();
    assert_eq!(
        delivering_effects,
        [
            SessionEffect::UpdateRow {
                take,
                changes: vec![
                    TranscriptChange::DurationMs(4_200),
                    TranscriptChange::SpeechMs(3_200),
                    TranscriptChange::RawText(Some(String::from("Hello. World how are you"))),
                    TranscriptChange::Language(Some(Language::from_static("en"))),
                ],
            },
            SessionEffect::Publish(SessionView {
                status: SessionStatus::Delivering,
                transcript_id: Some(take),
                elapsed_ms: 5_000,
                ..SessionView::IDLE
            }),
            SessionEffect::Deliver {
                take,
                text: String::from("Hello. World how are you"),
                target: Some(target()),
                language: Some(Language::from_static("en")),
            },
        ]
    );

    let done_effects = rig
        .after(100)
        .delivered(DeliveryOutcome::Pasted, "Hello. World, how are you? ")
        .to_vec();
    let settle_timer = rig.timer();
    assert_eq!(
        done_effects,
        [
            SessionEffect::UpdateRow {
                take,
                changes: vec![
                    TranscriptChange::Status(TranscriptStatus::Done),
                    TranscriptChange::FinalText(Some(String::from("Hello. World, how are you?"))),
                    TranscriptChange::WordCount(5),
                    TranscriptChange::PolisherIds(vec![EngineId::from_static("rules")]),
                    TranscriptChange::LatencyMs(200),
                ],
            },
            SessionEffect::TakeSettled { take },
            SessionEffect::StartTimer {
                timer: settle_timer,
                kind: SessionTimer::Settle,
                after_ms: 900,
            },
            SessionEffect::Publish(SessionView {
                status: SessionStatus::Done,
                transcript_id: Some(take),
                elapsed_ms: 5_000,
                outcome: Some(DeliveryOutcome::Pasted),
                ..SessionView::IDLE
            }),
        ]
    );

    assert_eq!(
        rig.after(900).fire(SessionTimer::Settle),
        [SessionEffect::Publish(SessionView::IDLE)]
    );
    assert_eq!(rig.state.phase, SessionPhase::Idle);
}

#[test]
fn esc_then_timeout_discards_the_row_and_the_audio() {
    let mut rig = Rig::recording(RecordMode::Toggle);
    let take = rig.take();
    let max_timer = rig.timer();
    rig.after(1_000).segment(0, "Scratch that.");
    let pause = rig.after(1_000).send(SessionInput::Esc).to_vec();
    let countdown = rig.timer();
    assert_eq!(
        pause,
        [
            SessionEffect::PauseCapture { take },
            SessionEffect::CancelTimer { timer: max_timer },
            SessionEffect::StartTimer {
                timer: countdown,
                kind: SessionTimer::Countdown,
                after_ms: 3_000,
            },
            SessionEffect::Publish(SessionView {
                status: SessionStatus::CancelPending,
                transcript_id: Some(take),
                elapsed_ms: 2_000,
                countdown_remaining_ms: Some(3_000),
                ..SessionView::IDLE
            }),
        ]
    );
    // The countdown drains while the recorded time stands still.
    rig.after(1_000);
    assert_eq!(rig.view().elapsed_ms, 2_000);
    assert_eq!(rig.view().countdown_remaining_ms, Some(2_000));

    let discard = rig.after(2_000).fire(SessionTimer::Countdown).to_vec();
    let settle = rig.timer();
    assert_eq!(
        discard,
        [
            SessionEffect::AbortCapture { take },
            SessionEffect::UnregisterEsc,
            SessionEffect::DeleteTake { take },
            SessionEffect::Cue(SessionCue::Cancel),
            SessionEffect::StartTimer {
                timer: settle,
                kind: SessionTimer::Settle,
                after_ms: 0,
            },
            SessionEffect::Publish(SessionView {
                status: SessionStatus::Discarded,
                elapsed_ms: 2_000,
                ..SessionView::IDLE
            }),
        ]
    );
    assert!(
        !has(&discard, |effect| matches!(
            effect,
            SessionEffect::UpdateRow { .. } | SessionEffect::TakeSettled { .. }
        )),
        "a discarded row is deleted, not updated"
    );
    // A reply for the discarded take can no longer land anywhere.
    let late = rig.after(10).send(SessionInput::SegmentDone {
        take,
        index: 1,
        output: output("late"),
    });
    assert_eq!(
        ignored(late).map(|i| i.reason),
        Some(IgnoreReason::StaleTake)
    );
    rig.after(0).fire(SessionTimer::Settle);
    assert_eq!(rig.status(), SessionStatus::Idle);
}

#[test]
fn esc_then_esc_resumes_and_keeps_the_text_before_and_after() {
    let mut rig = Rig::recording(RecordMode::Toggle);
    let take = rig.take();
    rig.after(4_000).segment(0, "First part.");
    rig.after(1_000).send(SessionInput::Esc);
    let countdown = rig.timer();
    // A segment that was already on its way while paused still counts.
    rig.after(500).segment(1, "still mine");
    let resume = rig.after(1_500).send(SessionInput::Esc).to_vec();
    let max_timer = rig.timer();
    assert_eq!(
        resume,
        [
            SessionEffect::ResumeCapture { take },
            SessionEffect::CancelTimer { timer: countdown },
            SessionEffect::StartTimer {
                timer: max_timer,
                kind: SessionTimer::MaxDuration,
                after_ms: 15 * MINUTE_MS - 5_000,
            },
            SessionEffect::Publish(SessionView {
                status: SessionStatus::Recording,
                transcript_id: Some(take),
                elapsed_ms: 5_000,
                ..SessionView::IDLE
            }),
        ]
    );
    // The countdown that was cancelled may still fire late: it must not discard the resumed take.
    let late = rig
        .after(1_000)
        .send(SessionInput::CountdownElapsed { timer: countdown })
        .to_vec();
    assert_eq!(
        ignored(&late).map(|i| i.reason),
        Some(IgnoreReason::StaleTimer)
    );
    assert_eq!(rig.status(), SessionStatus::Recording);
    assert_eq!(
        rig.view().elapsed_ms,
        6_000,
        "the pause is not recorded time"
    );

    rig.after(2_000).segment(2, "Second part.");
    rig.after(1_000).press();
    let effects = rig.after(50).all_segments(6_000).to_vec();
    assert!(
        effects.contains(&SessionEffect::Deliver {
            take,
            text: String::from("First part. Still mine Second part."),
            target: Some(target()),
            language: None,
        }),
        "{effects:?}"
    );
}

#[test]
fn device_loss_finalizes_what_was_captured() {
    let mut rig = Rig::recording(RecordMode::Toggle);
    let take = rig.take();
    rig.after(3_000).segment(0, "Before the cable went.");
    let effects = rig
        .after(10)
        .send(SessionInput::DeviceLost { take })
        .to_vec();
    assert_eq!(rig.status(), SessionStatus::Finalizing);
    assert_eq!(effects[0], SessionEffect::StopCapture { take });
    assert!(effects.contains(&SessionEffect::Toast(DEVICE_LOST_TOAST)));
    assert!(!has(&effects, |effect| matches!(
        effect,
        SessionEffect::DeleteTake { .. } | SessionEffect::AbortCapture { .. }
    )));
    rig.after(50).all_segments(2_000);
    assert_eq!(rig.status(), SessionStatus::Delivering);
}

#[test]
fn device_loss_while_the_microphone_opens_or_counts_down_still_finalizes() {
    // Lost while arming: the take stops the moment the microphone reports open.
    let mut arming = Rig::new(RecordMode::Toggle);
    arming.press();
    let take = arming.take();
    assert!(
        arming
            .after(10)
            .send(SessionInput::DeviceLost { take })
            .is_empty()
    );
    let effects = arming.after(10).armed().to_vec();
    assert_eq!(arming.status(), SessionStatus::Finalizing);
    assert!(!effects.contains(&SessionEffect::RegisterEsc));
    assert!(effects.contains(&SessionEffect::Toast(DEVICE_LOST_TOAST)));

    // Lost during the countdown: the undo cannot resume a vanished microphone, so it delivers.
    let mut pending = Rig::recording(RecordMode::Toggle);
    let take = pending.take();
    pending.after(1_000).send(SessionInput::Esc);
    pending.after(100).send(SessionInput::DeviceLost { take });
    assert_eq!(pending.status(), SessionStatus::CancelPending);
    let effects = pending.after(100).send(SessionInput::Esc).to_vec();
    assert_eq!(pending.status(), SessionStatus::Finalizing);
    assert!(effects.contains(&SessionEffect::UnregisterEsc));
    assert!(!effects.contains(&SessionEffect::ResumeCapture { take }));
}

#[test]
fn an_error_keeps_the_audio_and_marks_the_row_failed() {
    let mut rig = Rig::recording(RecordMode::Toggle);
    let take = rig.take();
    let max_timer = rig.timer();
    let effects = rig.after(2_000).error(AppError::Asr).to_vec();
    let settle = rig.timer();
    assert_eq!(
        effects,
        [
            SessionEffect::AbortCapture { take },
            SessionEffect::UnregisterEsc,
            SessionEffect::CancelTimer { timer: max_timer },
            SessionEffect::UpdateRow {
                take,
                changes: vec![
                    TranscriptChange::Status(TranscriptStatus::Failed),
                    TranscriptChange::ErrorCode(Some(AppErrorCode::Asr)),
                ],
            },
            SessionEffect::TakeSettled { take },
            SessionEffect::Toast(TAKE_FAILED_TOAST),
            SessionEffect::Cue(SessionCue::Error),
            SessionEffect::StartTimer {
                timer: settle,
                kind: SessionTimer::Settle,
                after_ms: 3_000,
            },
            SessionEffect::Publish(SessionView {
                status: SessionStatus::Failed,
                transcript_id: Some(take),
                elapsed_ms: 2_000,
                error: Some(AppError::Asr),
                ..SessionView::IDLE
            }),
        ]
    );
    assert!(
        !effects.contains(&SessionEffect::DeleteTake { take }),
        "the WAV is kept"
    );

    // Failing in delivery (busy clipboard, 05 W4) keeps the raw text already stored; nothing else is open.
    let mut delivering = Rig::delivering(RecordMode::Toggle);
    let take = delivering.take();
    let effects = delivering.after(30).error(AppError::Internal).to_vec();
    assert!(!has(&effects, |effect| matches!(
        effect,
        SessionEffect::AbortCapture { .. } | SessionEffect::UnregisterEsc
    )));
    assert!(
        row_changes(&effects).contains(&TranscriptChange::ErrorCode(Some(AppErrorCode::Internal)))
    );
    assert_eq!(delivering.view().transcript_id, Some(take));

    // Failing to open the microphone: no audio to retry, so the toast points at the microphone.
    let mut arming = Rig::new(RecordMode::Toggle);
    arming.press();
    let effects = arming.after(40).error(AppError::AudioDevice).to_vec();
    assert_eq!(arming.status(), SessionStatus::Failed);
    assert!(effects.contains(&SessionEffect::Toast(START_FAILED_TOAST)));
    assert!(!effects.contains(&SessionEffect::UnregisterEsc));
}

#[test]
fn a_silence_only_take_delivers_nothing() {
    for speech_ms in [0, 249] {
        let mut rig = Rig::finalizing(RecordMode::Toggle);
        let take = rig.take();
        let effects = rig.after(20).all_segments(speech_ms).to_vec();
        assert_eq!(rig.status(), SessionStatus::Done);
        assert_eq!(rig.view().outcome, Some(DeliveryOutcome::NoSpeech));
        assert!(!has(&effects, |effect| matches!(
            effect,
            SessionEffect::Deliver { .. }
        )));
        let changes = row_changes(&effects);
        assert!(changes.contains(&TranscriptChange::Status(TranscriptStatus::Empty)));
        assert!(
            !has(&changes, |change| matches!(
                change,
                TranscriptChange::RawText(_)
            )),
            "a hallucinated word from under 250 ms of speech is not stored (05 A4)"
        );
        assert!(effects.contains(&SessionEffect::TakeSettled { take }));
        assert!(has(&effects, |effect| matches!(
            effect,
            SessionEffect::StartTimer {
                after_ms: 3_000,
                ..
            }
        )));
    }
    // Exactly the threshold is speech.
    let mut rig = Rig::finalizing(RecordMode::Toggle);
    rig.after(20).all_segments(250);
    assert_eq!(rig.status(), SessionStatus::Delivering);

    // Speech but no words from ASR, or words that polish removed: still nothing delivered.
    let mut no_words = Rig::recording(RecordMode::Toggle);
    no_words.after(1_000).segment(0, "   ");
    no_words.after(1_000).press();
    no_words.after(20).all_segments(2_000);
    assert_eq!(no_words.view().outcome, Some(DeliveryOutcome::NoSpeech));
    let mut polished_away = Rig::delivering(RecordMode::Toggle);
    let effects = polished_away
        .after(20)
        .delivered(DeliveryOutcome::Pasted, " ")
        .to_vec();
    assert_eq!(
        polished_away.view().outcome,
        Some(DeliveryOutcome::NoSpeech)
    );
    assert!(row_changes(&effects).contains(&TranscriptChange::Status(TranscriptStatus::Empty)));
}

#[test]
fn hold_mode_records_while_held_and_stops_on_release() {
    let mut rig = Rig::new(RecordMode::Hold);
    rig.press();
    rig.after(100).armed();
    // The key repeats while held: every repeat is ignored, never a stop.
    for _ in 0..5 {
        let effects = rig.after(500).press().to_vec();
        assert_eq!(
            ignored(&effects).map(|i| i.reason),
            Some(IgnoreReason::NotValidNow)
        );
    }
    let effects = rig.after(500).send(SessionInput::RecordReleased).to_vec();
    assert_eq!(rig.status(), SessionStatus::Finalizing);
    assert!(effects.contains(&SessionEffect::UnregisterEsc));
    assert!(!has(&effects, |effect| matches!(
        effect,
        SessionEffect::Toast(_)
    )));

    // A tap released before the microphone opened stops the take as soon as it is.
    let mut tap = Rig::new(RecordMode::Hold);
    tap.press();
    tap.after(60).send(SessionInput::RecordReleased);
    assert_eq!(tap.status(), SessionStatus::Arming);
    tap.after(40).armed();
    assert_eq!(tap.status(), SessionStatus::Finalizing);

    // Released during the countdown: undoing delivers what was said instead of recording with no key held.
    let mut pending = Rig::recording(RecordMode::Hold);
    pending.after(1_000).send(SessionInput::Esc);
    pending.after(100).send(SessionInput::RecordReleased);
    pending.after(100).send(SessionInput::Esc);
    assert_eq!(pending.status(), SessionStatus::Finalizing);

    // Toggle mode ignores releases.
    let mut toggle = Rig::recording(RecordMode::Toggle);
    toggle.after(500).send(SessionInput::RecordReleased);
    assert_eq!(toggle.status(), SessionStatus::Recording);
}

/// A dropped take leaves no trace: no cue, no toast, no row write; its row and audio are deleted and the pill hides.
fn assert_dropped_silently(rig: &Rig, take: TranscriptId) {
    let effects = &rig.effects;
    assert_eq!(rig.status(), SessionStatus::Discarded);
    assert_eq!(rig.view().transcript_id, None);
    assert!(effects.contains(&SessionEffect::AbortCapture { take }));
    assert!(effects.contains(&SessionEffect::DeleteTake { take }));
    assert!(!has(effects, |effect| matches!(
        effect,
        SessionEffect::Cue(_)
            | SessionEffect::Toast(_)
            | SessionEffect::UpdateRow { .. }
            | SessionEffect::TakeSettled { .. }
    )));
    assert!(has(effects, |effect| matches!(
        effect,
        SessionEffect::StartTimer {
            kind: SessionTimer::Settle,
            after_ms: 0,
            ..
        }
    )));
}

#[test]
fn an_interrupted_press_drops_a_take_that_is_still_arming() {
    for mode in [RecordMode::Hold, RecordMode::Toggle] {
        let mut rig = Rig::new(mode);
        assert!(
            !has(rig.press(), |effect| matches!(
                effect,
                SessionEffect::Cue(_)
            )),
            "no chime before the microphone is open, so another shortcut stays silent"
        );
        let take = rig.take();
        assert!(
            rig.after(30)
                .send(SessionInput::RecordInterrupted)
                .is_empty()
        );
        assert_eq!(rig.status(), SessionStatus::Arming);
        // A release after it does not turn the drop back into a delivery.
        rig.after(20).send(SessionInput::RecordReleased);
        rig.after(50).armed();
        assert_dropped_silently(&rig, take);
        assert!(
            !rig.effects.contains(&SessionEffect::RegisterEsc),
            "Esc is never taken for a dropped take"
        );
    }
}

#[test]
fn an_interrupted_press_drops_a_young_take_and_releases_everything() {
    for mode in [RecordMode::Hold, RecordMode::Toggle] {
        let mut rig = Rig::recording(mode);
        let take = rig.take();
        let timer = rig.timer();
        rig.after(policy(mode).interrupt_grace_ms - 1)
            .send(SessionInput::RecordInterrupted);
        assert_dropped_silently(&rig, take);
        assert!(rig.effects.contains(&SessionEffect::UnregisterEsc));
        assert!(rig.effects.contains(&SessionEffect::CancelTimer { timer }));
        rig.fire(SessionTimer::Settle);
        assert_eq!(rig.status(), SessionStatus::Idle);
    }
}

#[test]
fn a_late_interruption_is_a_slip_while_dictating() {
    // Hold mode: the keys are no longer the chord, so the take stops and delivers what was said.
    let mut hold = Rig::recording(RecordMode::Hold);
    hold.after(1_000).segment(0, "Keep this.");
    hold.after(500).send(SessionInput::RecordInterrupted);
    assert_eq!(hold.status(), SessionStatus::Finalizing);
    assert!(
        hold.effects
            .contains(&SessionEffect::StopCapture { take: hold.take() })
    );

    // Toggle mode: the take was started by an earlier press and keeps recording.
    let mut toggle = Rig::recording(RecordMode::Toggle);
    let effects = toggle
        .after(1_500)
        .send(SessionInput::RecordInterrupted)
        .to_vec();
    assert_eq!(toggle.status(), SessionStatus::Recording);
    assert_eq!(
        ignored(&effects).map(|i| i.reason),
        Some(IgnoreReason::NotValidNow)
    );

    // During the Esc countdown in hold mode it counts as the release: undoing delivers.
    let mut pending = Rig::recording(RecordMode::Hold);
    pending.after(200).send(SessionInput::Esc);
    pending.after(100).send(SessionInput::RecordInterrupted);
    assert_eq!(pending.status(), SessionStatus::CancelPending);
    pending.after(100).send(SessionInput::Esc);
    assert_eq!(pending.status(), SessionStatus::Finalizing);
}

#[test]
fn a_missing_model_shows_set_up_without_touching_anything() {
    let mut rig = Rig::new(RecordMode::Toggle);
    rig.press();
    let take = rig.take();
    let model_id = ModelId::from_static("parakeet-tdt-0.6b-v3");
    let effects = rig
        .after(30)
        .send(SessionInput::ModelMissing {
            take,
            model_id: model_id.clone(),
        })
        .to_vec();
    let settle = rig.timer();
    let error = AppError::ModelMissing { model_id };
    assert_eq!(
        effects,
        [
            SessionEffect::Cue(SessionCue::Error),
            SessionEffect::StartTimer {
                timer: settle,
                kind: SessionTimer::Settle,
                after_ms: 3_000,
            },
            SessionEffect::Publish(SessionView {
                status: SessionStatus::Failed,
                error: Some(error),
                ..SessionView::IDLE
            }),
        ]
    );
    // Pressing again right away tries again, without waiting for the pill to clear.
    rig.after(500).press();
    assert_eq!(rig.status(), SessionStatus::Arming);
    assert!(
        rig.effects
            .contains(&SessionEffect::CancelTimer { timer: settle })
    );
}

#[test]
fn record_presses_are_debounced() {
    let mut rig = Rig::new(RecordMode::Toggle);
    rig.press();
    rig.after(20).armed();
    // A bounce 149 ms after the start press is not a stop.
    let bounce = rig.after(129).press().to_vec();
    assert_eq!(
        ignored(&bounce).map(|i| i.reason),
        Some(IgnoreReason::Debounced)
    );
    assert_eq!(rig.status(), SessionStatus::Recording);
    // At 150 ms it is.
    rig.after(1).press();
    assert_eq!(rig.status(), SessionStatus::Finalizing);

    // The anchor survives the take: a double tap that stops a take does not start the next one.
    let mut done = Rig::delivering(RecordMode::Toggle);
    done.after(10).delivered(DeliveryOutcome::Copied, "Hi.");
    let last_press = done.state.last_record_press.expect("a press was accepted");
    done.now = last_press.as_millis() + 100;
    let bounce = done.press().to_vec();
    assert_eq!(
        ignored(&bounce).map(|i| i.reason),
        Some(IgnoreReason::Debounced)
    );
    done.after(50).press();
    assert_eq!(done.status(), SessionStatus::Arming);

    // Pressed while the microphone opens: remembered as a stop, debounced like any press.
    let mut arming = Rig::new(RecordMode::Toggle);
    arming.press();
    assert_eq!(
        ignored(arming.after(50).press()).map(|i| i.reason),
        Some(IgnoreReason::Debounced)
    );
    assert!(arming.after(150).press().is_empty());
    arming.after(10).armed();
    assert_eq!(arming.status(), SessionStatus::Finalizing);
}

#[test]
fn the_longest_take_stops_by_itself_and_counts_only_recorded_time() {
    let mut rig = Rig::recording(RecordMode::Toggle);
    rig.after(10 * MINUTE_MS).send(SessionInput::Esc);
    // Ten minutes paused are not recorded time.
    rig.after(2_000).send(SessionInput::Esc);
    assert!(has(&rig.effects, |effect| matches!(
        effect,
        SessionEffect::StartTimer { kind: SessionTimer::MaxDuration, after_ms, .. }
            if *after_ms == 5 * MINUTE_MS
    )));
    let effects = rig
        .after(5 * MINUTE_MS)
        .fire(SessionTimer::MaxDuration)
        .to_vec();
    assert_eq!(rig.status(), SessionStatus::Finalizing);
    assert!(effects.contains(&SessionEffect::Toast(MAX_DURATION_TOAST)));
    assert_eq!(rig.view().elapsed_ms, 900_000);

    // An undo with no time left stops at once.
    let mut edge = Rig::new(RecordMode::Toggle);
    let short = SessionPolicy {
        max_duration_ms: 1_000,
        ..policy(RecordMode::Toggle)
    };
    edge.send(SessionInput::RecordPressed {
        next_take: TranscriptId::generate(),
        policy: short,
    });
    edge.after(10).armed();
    edge.after(1_000).send(SessionInput::Esc);
    edge.after(100).send(SessionInput::Esc);
    assert!(has(&edge.effects, |effect| matches!(
        effect,
        SessionEffect::StartTimer {
            kind: SessionTimer::MaxDuration,
            after_ms: 0,
            ..
        }
    )));
}

#[test]
fn replies_from_another_take_are_stale_and_a_stray_microphone_is_closed() {
    let mut rig = Rig::recording(RecordMode::Toggle);
    let other = TranscriptId::generate();
    let before = rig.state.clone();
    let effects = rig
        .after(10)
        .send(SessionInput::Armed {
            take: other,
            target: None,
        })
        .to_vec();
    assert_eq!(
        effects,
        [
            SessionEffect::Ignored(IgnoredInput {
                input: "Armed",
                status: SessionStatus::Recording,
                reason: IgnoreReason::StaleTake,
            }),
            SessionEffect::AbortCapture { take: other },
        ]
    );
    assert_eq!(rig.state, before);
    for input in [
        SessionInput::Error {
            take: other,
            error: AppError::Asr,
        },
        SessionInput::DeviceLost { take: other },
        SessionInput::SegmentDone {
            take: other,
            index: 0,
            output: output("not mine"),
        },
    ] {
        let effects = rig.after(10).send(input).to_vec();
        assert_eq!(
            ignored(&effects).map(|i| i.reason),
            Some(IgnoreReason::StaleTake)
        );
        assert_eq!(rig.status(), SessionStatus::Recording);
    }
    // The current take's own duplicate Armed is ignored without closing its microphone.
    let effects = rig.after(10).armed().to_vec();
    assert_eq!(
        ignored(&effects).map(|i| i.reason),
        Some(IgnoreReason::NotValidNow)
    );
}

#[test]
fn a_repeated_segment_index_keeps_the_latest_text() {
    let mut rig = Rig::recording(RecordMode::Toggle);
    rig.after(10).segment(0, "draft");
    rig.after(10).segment(0, "Final.");
    rig.after(200).press();
    let effects = rig.after(10).all_segments(1_000).to_vec();
    assert!(
        row_changes(&effects).contains(&TranscriptChange::RawText(Some(String::from("Final."))))
    );
}

#[test]
fn a_copied_take_is_done_with_its_outcome() {
    let mut rig = Rig::delivering(RecordMode::Toggle);
    rig.after(10)
        .delivered(DeliveryOutcome::Copied, "Copied text.");
    assert_eq!(rig.view().outcome, Some(DeliveryOutcome::Copied));
    assert!(has(&rig.effects, |effect| matches!(
        effect,
        SessionEffect::StartTimer { after_ms: 900, .. }
    )));
}

#[test]
fn stop_causes_come_from_the_input_that_stopped() {
    let cases = [
        (RecordMode::Toggle, SessionInput::Stop, StopCause::Ui),
        (
            RecordMode::Hold,
            SessionInput::RecordReleased,
            StopCause::Released,
        ),
    ];
    for (mode, input, cause) in cases {
        let mut rig = Rig::recording(mode);
        rig.after(500).send(input);
        match &rig.state.phase {
            SessionPhase::Finalizing(finalizing) => assert_eq!(finalizing.cause, cause),
            other => panic!("expected Finalizing, got {:?}", other.status()),
        }
    }
    let mut rig = Rig::recording(RecordMode::Toggle);
    rig.after(500).press();
    match &rig.state.phase {
        SessionPhase::Finalizing(finalizing) => assert_eq!(finalizing.cause, StopCause::Hotkey),
        other => panic!("expected Finalizing, got {:?}", other.status()),
    }
}

/// A small deterministic generator, so a failing random run replays exactly.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % bound
    }
}

/// What the actor would hold after running `effects`: the Esc registration, the timer slot and the microphone.
#[derive(Default)]
struct Resources {
    esc: bool,
    timer: Option<(TimerToken, SessionTimer)>,
    microphone: Option<TranscriptId>,
    /// The take an Arm effect is opening; the reply opens its microphone.
    arming: Option<TranscriptId>,
}

impl Resources {
    fn run(&mut self, effects: &[SessionEffect]) {
        for effect in effects {
            match effect {
                SessionEffect::RegisterEsc => {
                    assert!(!self.esc, "Esc registered twice");
                    self.esc = true;
                }
                SessionEffect::UnregisterEsc => {
                    assert!(self.esc, "Esc released while not registered");
                    self.esc = false;
                }
                SessionEffect::StartTimer { timer, kind, .. } => self.timer = Some((*timer, *kind)),
                SessionEffect::CancelTimer { timer } => {
                    if self.timer.is_some_and(|(live, _)| live == *timer) {
                        self.timer = None;
                    }
                }
                SessionEffect::Arm { take } => self.arming = Some(*take),
                SessionEffect::StopCapture { take } | SessionEffect::AbortCapture { take }
                    if self.microphone == Some(*take) =>
                {
                    self.microphone = None;
                }
                _ => {}
            }
        }
    }
}

#[test]
fn random_runs_never_leak_esc_timers_or_the_microphone() {
    for seed in 0..40_u64 {
        let mode = if seed % 2 == 0 {
            RecordMode::Toggle
        } else {
            RecordMode::Hold
        };
        let mut random = Lcg(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut state = SessionState::IDLE;
        let mut now = 0_u64;
        let mut held = Resources::default();
        let old_take = TranscriptId::generate();
        let mut next_index = 0_u32;
        for _ in 0..3_000 {
            now += random.next(400);
            let current = state.phase.take_id().unwrap_or(old_take);
            let take = if random.next(10) == 0 {
                old_take
            } else {
                current
            };
            // Never the live token: a live timer only ever fires its own kind (case 6).
            let stale_timer = state.phase.timer().unwrap_or_default().next();
            let input = match random.next(15) {
                0 | 1 => SessionInput::RecordPressed {
                    next_take: TranscriptId::generate(),
                    policy: SessionPolicy {
                        cancel_countdown_ms: 1_000,
                        max_duration_ms: 5_000,
                        ..policy(mode)
                    },
                },
                2 => SessionInput::RecordReleased,
                3 => SessionInput::Stop,
                4 => SessionInput::Esc,
                5 => match held.arming.take() {
                    // The Arm effect replies once: open, missing model or error.
                    Some(arming) => match random.next(6) {
                        0 => SessionInput::ModelMissing {
                            take: arming,
                            model_id: ModelId::from_static("m"),
                        },
                        1 => SessionInput::Error {
                            take: arming,
                            error: AppError::AudioDevice,
                        },
                        _ => {
                            held.microphone = Some(arming);
                            SessionInput::Armed {
                                take: arming,
                                target: None,
                            }
                        }
                    },
                    None => SessionInput::Armed {
                        take: old_take,
                        target: None,
                    },
                },
                // The live timer fires: a one-shot timer leaves the slot before its input arrives.
                6 => match held.timer.take() {
                    Some((timer, kind)) => kind.fired(timer),
                    None => SessionInput::CountdownElapsed { timer: stale_timer },
                },
                7 => SessionInput::MaxDurationReached { timer: stale_timer },
                8 => SessionInput::SettleElapsed { timer: stale_timer },
                9 => {
                    next_index += 1;
                    SessionInput::SegmentDone {
                        take,
                        index: next_index % 7,
                        output: output("word"),
                    }
                }
                10 => SessionInput::AllSegmentsDone {
                    take,
                    audio: audio(random.next(600)),
                },
                11 => SessionInput::Delivered {
                    take,
                    outcome: DeliveryOutcome::Pasted,
                    polish: polished("Words."),
                },
                12 => SessionInput::Error {
                    take,
                    error: AppError::Polish,
                },
                13 => SessionInput::DeviceLost { take },
                _ => SessionInput::ModelMissing {
                    take,
                    model_id: ModelId::from_static("m"),
                },
            };
            let before = state.clone();
            let (after, effects) = transition(before.clone(), input.clone(), at(now));
            assert_invariants(&before, &input, &after, &effects);
            held.run(&effects);
            let context = format!("seed {seed}: {} → {:?}", input.name(), after.phase.status());
            assert_eq!(
                held.esc,
                after.phase.holds_session_hotkeys(),
                "{context}: Esc registered exactly while recording or counting down"
            );
            assert_eq!(
                held.timer.map(|(timer, _)| timer),
                after.phase.timer(),
                "{context}: one timer slot"
            );
            if held.microphone.is_some() {
                assert!(
                    after.phase.has_open_capture() && after.phase.take_id() == held.microphone,
                    "{context}: an open microphone always belongs to the current take"
                );
            }
            state = after;
        }
    }
}
