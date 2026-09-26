/*!
 * SOURCE OF TRUTH KEYWORDS: session actor tests, pipeline tests, full take test, Esc undo test, Esc discard test, device loss test, ASR error test, exit mid-take test, panic recovery test, audio retention after success test, sound cue test, paste-last hotkey test, hold mode test, Bluetooth hint test, rehearsal test
 * WHAT:  End-to-end tests of the session actor over port fakes: a real capture worker, ASR worker, polish chain,
 *        delivery and in-memory database, driven by fake hotkeys and a fake microphone, observed through the
 *        events, the database, the journal on disk and the fakes.
 * WHY:   02 §13 asks for pipeline tests of the take lifecycle with fake ports: the machine's table tests prove the
 *        decisions, these prove the actor carries them out (row before mic, Esc paired, WAV kept or deleted, text
 *        pasted, events emitted) with the real threads in between. Record presses are spaced past the 150 ms
 *        debounce; audio is fed in bursts the capture ring holds (under 2 s), so no test depends on how fast the
 *        worker drains. Every wait is bounded by the ChannelSink timeout, so a broken actor fails instead of hanging.
 * WHERE: `cargo test` (local gate).
 */

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use hound::WavReader;

use super::{
    DEVICE_LOST_TOAST, NOTHING_TO_PASTE_TOAST, START_FAILED_TOAST, SessionActor, SessionConfig,
    SessionEngines, SessionHandle, TAKE_FAILED_TOAST,
};
use crate::{
    pipeline::{
        asr::{AsrWorker, AsrWorkerConfig},
        delivery::{Delivery, DeliveryPorts},
        sound_cues::{SoundCues, render},
    },
    ports::{
        AsrEngine, EventSink,
        fakes::{
            ChannelSink, FakeAsrEngine, FakeAudioCapture, FakeClipboard, FakeForegroundApp,
            FakeHotkeyService, FakeNotifier, FakeSoundPlayer, FakeTextInserter, FakeVoiceActivity,
            FakeWorkerScheduler,
        },
    },
    registry::{
        self,
        engines::{BuildCtx, PARAKEET_TDT_V3},
        hotkeys::{CANCEL, PASTE_LAST, RECORD},
        notices::BLUETOOTH_MIC_NOTICE,
        settings::{keys, values},
        sounds::sound_for,
    },
    services::{self, Db, transcripts},
    types::{
        Accelerator, AcceleratorRequest, AppError, AppEvent, AppPaths, AppTarget, AsrLoadRequest,
        AudioTransport, CaptureFormat, DeliveryOutcome, EngineId, HistoryChangeReason,
        HistoryChanged, HotkeyAction, HotkeyCaps, HotkeyRehearsed, KeyState, Permission, PortError,
        ResourceKind, SessionCue, SessionRehearsal, SessionStatus, SessionUiInput, SessionView,
        SettingKey, SettingValue, SettingsSnapshot, SharedSettings, StaticStr, TranscriptId,
        TranscriptStatus, testing::TempDir,
    },
};

const STEREO_48K: CaptureFormat = CaptureFormat {
    sample_rate: 48_000,
    channels: 2,
};

/// Longer than the 150 ms record debounce, so a second press is never taken for a bounce.
const PAST_DEBOUNCE: Duration = Duration::from_millis(250);

const WAIT: Duration = Duration::from_secs(5);

/// `ms` of 48 kHz stereo audio at a constant `level` (speech above the fake detector's threshold).
fn audio(ms: usize, level: f32) -> Vec<f32> {
    vec![level; 48 * ms * 2]
}

fn speech(ms: usize) -> Vec<f32> {
    audio(ms, 0.3)
}

fn silence(ms: usize) -> Vec<f32> {
    audio(ms, 0.0)
}

/// An actor over fakes, running on its own runtime thread.
struct Rig {
    handle: SessionHandle,
    events: Arc<ChannelSink<AppEvent>>,
    audio: Arc<FakeAudioCapture>,
    hotkeys: Arc<FakeHotkeyService>,
    foreground: Arc<FakeForegroundApp>,
    notifier: Arc<FakeNotifier>,
    clipboard: Arc<FakeClipboard>,
    inserter: Arc<FakeTextInserter>,
    engine: Arc<FakeAsrEngine>,
    sounds: Arc<FakeSoundPlayer>,
    db: Db,
    paths: AppPaths,
    runner: Option<JoinHandle<()>>,
    _dir: TempDir,
}

impl Rig {
    /// Registry defaults in toggle mode (press to start, press again to stop), the engine loaded.
    fn start() -> Self {
        Self::with(
            settings_with(vec![(
                keys::HOTKEY_MODE,
                SettingValue::Enum(StaticStr::new(values::TOGGLE)),
            )]),
            None,
        )
    }

    /// `settings`, and the engine's load failing with `load_error` when given.
    fn with(settings: SettingsSnapshot, load_error: Option<PortError>) -> Self {
        Self::build(settings, load_error, None)
    }

    /// Toggle mode, with the actor panicking the first time it publishes `status` (a bug inside the actor).
    fn panicking_on(status: SessionStatus) -> Self {
        Self::build(
            settings_with(vec![(
                keys::HOTKEY_MODE,
                SettingValue::Enum(StaticStr::new(values::TOGGLE)),
            )]),
            None,
            Some(status),
        )
    }

    fn build(
        settings: SettingsSnapshot,
        load_error: Option<PortError>,
        panic_on: Option<SessionStatus>,
    ) -> Self {
        let dir = TempDir::new("session-actor");
        let paths = AppPaths::new(dir.join("data"), dir.join("resources"));
        let events = Arc::new(ChannelSink::default());
        let audio = Arc::new(FakeAudioCapture::new(STEREO_48K));
        // The keyboard hook's caps: key-up and modifier-only chords (Ctrl+Alt, interrupted by another key).
        let hotkeys = Arc::new(FakeHotkeyService::new(HotkeyCaps {
            supports_release: true,
            supports_modifier_only: true,
        }));
        let foreground = Arc::new(FakeForegroundApp::focused(notepad()));
        let notifier = Arc::new(FakeNotifier::default());
        let clipboard = Arc::new(FakeClipboard::with_text("before"));
        let inserter = Arc::new(FakeTextInserter::default());
        let engine = Arc::new(FakeAsrEngine::english());
        let sounds = Arc::new(FakeSoundPlayer::default());
        let db = Db::open_in_memory().unwrap();
        // The table holds what the snapshot says, as in the app (bootstrap resolves the snapshot from it), so a
        // settings write during a test (a one-time notice's flag) re-resolves to the same settings.
        for (key, value) in settings.iter() {
            services::settings::set::set(&db, key, value).unwrap();
        }

        let shared = Arc::clone(&engine);
        let asr = AsrWorker::spawn(AsrWorkerConfig {
            accelerators: crate::pipeline::asr::AcceleratorPicker::without_gpu(),
            build: Arc::new(move |_: &EngineId| Ok(Arc::clone(&shared) as Arc<dyn AsrEngine>)),
            scheduler: Arc::new(FakeWorkerScheduler::default()),
            readiness: None,
        })
        .unwrap();
        if let Some(error) = load_error {
            engine.fail_next_load(error);
        }
        let _ = asr
            .load(AsrLoadRequest {
                engine_id: PARAKEET_TDT_V3,
                model_dir: paths.model_dir(&registry::models::PARAKEET_TDT_V3),
                accelerator: AcceleratorRequest::Fixed(Accelerator::Cpu),
            })
            .recv_timeout(WAIT)
            .unwrap();

        let polish_ctx = BuildCtx {
            paths: paths.clone(),
        };
        let (handle, inbox) = SessionHandle::new();
        let actor = SessionActor::new(
            SessionConfig {
                settings: SharedSettings::new(settings),
                audio: Arc::clone(&audio) as _,
                scheduler: Arc::new(FakeWorkerScheduler::default()),
                asr,
                hotkeys: Arc::clone(&hotkeys) as _,
                foreground: Arc::clone(&foreground) as _,
                notifier: Arc::clone(&notifier) as _,
                delivery: Delivery::new(DeliveryPorts {
                    clipboard: Arc::clone(&clipboard) as _,
                    inserter: Arc::clone(&inserter) as _,
                    notifier: Arc::clone(&notifier) as _,
                }),
                paths: paths.clone(),
                db: db.clone(),
                events: Arc::new(PanicOnPublish {
                    inner: Arc::clone(&events),
                    status: panic_on,
                    fired: AtomicBool::new(false),
                }),
                engines: SessionEngines::new(
                    Arc::new(|| Ok(Box::new(FakeVoiceActivity::new(32)) as _)),
                    Arc::new(move |id: &EngineId| {
                        registry::engines::build_polisher(id, &polish_ctx)
                    }),
                ),
                sounds: SoundCues::new(Arc::clone(&sounds) as _),
            },
            inbox,
        );
        let runner = thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .unwrap()
                .block_on(actor.run());
        });
        let rig = Self {
            handle,
            events,
            audio,
            hotkeys,
            foreground,
            notifier,
            clipboard,
            inserter,
            engine,
            sounds,
            db,
            paths,
            runner: Some(runner),
            _dir: dir,
        };
        rig.handle.prepare();
        // A query answered after Prepare proves the record hotkey is bound.
        assert_eq!(rig.view(), SessionView::IDLE);
        assert!(rig.hotkeys.binding(&RECORD).is_some());
        rig
    }

    fn view(&self) -> SessionView {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(self.handle.view())
            .unwrap()
    }

    fn press(&self) {
        assert!(self.hotkeys.press(&RECORD), "the record hotkey is bound");
    }

    fn esc(&self) {
        assert!(self.hotkeys.press(&CANCEL), "Esc is bound while recording");
    }

    fn feed(&self, samples: &[f32]) {
        assert!(
            self.audio.feed(samples),
            "the microphone is open and running"
        );
    }

    /// Waits for the session to publish `status`; returns that view and every event before it.
    fn wait_for(&self, status: SessionStatus) -> (SessionView, Vec<AppEvent>) {
        let mut before = Vec::new();
        loop {
            match self.events.next() {
                Some(AppEvent::SessionStateChanged(changed)) if changed.0.status == status => {
                    return (changed.0, before);
                }
                Some(event) => before.push(event),
                None => panic!("the session never reached {status:?}; saw {before:?}"),
            }
        }
    }

    /// Sets the session rehearsal; it applies from the next message the actor handles.
    fn rehearse(&self, rehearsal: SessionRehearsal) {
        self.handle.rehearse(rehearsal).unwrap();
    }

    /// Waits for the next HotkeyRehearsed; returns it and every event before it.
    fn wait_for_rehearsed(&self) -> (HotkeyRehearsed, Vec<AppEvent>) {
        let mut before = Vec::new();
        loop {
            match self.events.next() {
                Some(AppEvent::HotkeyRehearsed(rehearsed)) => return (rehearsed, before),
                Some(event) => before.push(event),
                None => panic!("no hotkey was rehearsed; saw {before:?}"),
            }
        }
    }

    /// Starts a take and returns its id once it is recording.
    fn record(&self) -> TranscriptId {
        self.press();
        let (view, _) = self.wait_for(SessionStatus::Recording);
        view.transcript_id.unwrap()
    }

    /// Presses the record hotkey again, past the debounce.
    fn stop(&self) {
        thread::sleep(PAST_DEBOUNCE);
        self.press();
    }

    /// The cues played so far, in order.
    fn cues(&self) -> Vec<SessionCue> {
        self.sounds
            .played()
            .iter()
            .filter_map(|clip| {
                SessionCue::ALL
                    .into_iter()
                    .find(|cue| sound_for(*cue).is_some_and(|sound| **clip == render(sound)))
            })
            .collect()
    }

    /// Records one toggle-mode take of `text` to the end (Done, then Idle).
    fn dictate(&self, text: &str) -> TranscriptId {
        self.engine.push_text(text);
        let take = self.record();
        self.feed(&speech(400));
        self.feed(&silence(700));
        self.stop();
        self.wait_for(SessionStatus::Done);
        self.wait_for(SessionStatus::Idle);
        take
    }
}

/// Waits up to WAIT for `condition`, for effects that publish no event (a paste-last, a toast).
fn eventually(condition: impl Fn() -> bool) -> bool {
    let deadline = std::time::Instant::now() + WAIT;
    while std::time::Instant::now() < deadline {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    condition()
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.handle.shutdown(WAIT);
        if let Some(runner) = self.runner.take() {
            let _ = runner.join();
        }
    }
}

/// Forwards every event to the rig's channel, except that it panics once, instead of forwarding, when the actor
/// publishes `status`: the panic strikes on the actor's own task, in the middle of running a transition's effects.
struct PanicOnPublish {
    inner: Arc<ChannelSink<AppEvent>>,
    status: Option<SessionStatus>,
    fired: AtomicBool,
}

impl EventSink<AppEvent> for PanicOnPublish {
    fn emit(&self, event: AppEvent) {
        if let AppEvent::SessionStateChanged(changed) = &event
            && Some(changed.0.status) == self.status
            && !self.fired.swap(true, Ordering::SeqCst)
        {
            panic!("publishing {:?} went wrong", changed.0.status);
        }
        self.inner.emit(event);
    }
}

fn notepad() -> AppTarget {
    FakeForegroundApp::target("notepad.exe", false)
}

/// Echo's own main window: the focused window belongs to this process.
fn echo_window() -> AppTarget {
    AppTarget {
        process_id: std::process::id(),
        ..FakeForegroundApp::target("echo.exe", false)
    }
}

fn settings_with(changes: Vec<(SettingKey, SettingValue)>) -> SettingsSnapshot {
    registry::settings::resolve(changes)
}

fn history(reason: HistoryChangeReason) -> AppEvent {
    AppEvent::HistoryChanged(HistoryChanged { reason })
}

/// Press, speak two sentences with a pause between them, press again: the text is pasted into the target and the
/// row holds every column.
#[test]
fn a_full_take_is_transcribed_polished_pasted_and_stored() {
    let rig = Rig::start();
    rig.engine.push_text("Hello there.");
    rig.engine.push_text("How are you?");
    rig.press();
    let (recording, arming) = rig.wait_for(SessionStatus::Recording);
    let take = recording.transcript_id.unwrap();
    assert!(
        arming.contains(&history(HistoryChangeReason::Inserted)),
        "History hears of the row as soon as it is inserted"
    );
    assert_eq!(
        transcripts::get::get(&rig.db, take).unwrap().status,
        TranscriptStatus::Recording,
        "the row exists before any audio (02 §7.3)"
    );
    assert!(
        rig.hotkeys.binding(&CANCEL).is_some(),
        "Esc is registered while recording"
    );
    rig.feed(&speech(400));
    rig.feed(&silence(700));
    rig.feed(&speech(300));
    rig.stop();
    let (done, before) = rig.wait_for(SessionStatus::Done);

    assert_eq!(done.outcome, Some(DeliveryOutcome::Pasted));
    assert_eq!(done.transcript_id, Some(take));
    let pasted = "Hello there. How are you? ";
    assert_eq!(rig.inserter.insertions(), [(notepad(), pasted.to_owned())]);
    assert_eq!(rig.clipboard.text().as_deref(), Some(pasted));
    assert_eq!(rig.engine.calls().len(), 2, "one call per segment");

    let row = transcripts::get::get(&rig.db, take).unwrap();
    assert_eq!(row.status, TranscriptStatus::Done);
    assert_eq!(row.raw_text.as_deref(), Some("Hello there. How are you?"));
    assert_eq!(row.final_text.as_deref(), Some("Hello there. How are you?"));
    assert_eq!(row.word_count, Some(5));
    assert_eq!(row.duration_ms, Some(1_400));
    assert!(row.speech_ms.is_some_and(|speech| speech >= 600));
    assert_eq!(row.engine_id, Some(PARAKEET_TDT_V3));
    assert_eq!(row.polisher_ids, [registry::engines::RULE_POLISHER]);
    assert!(row.latency_ms.is_some());
    assert_eq!(row.app_name.as_deref(), Some("notepad.exe"));
    assert_eq!(row.error_code, None);
    assert_eq!(
        WavReader::open(rig.paths.recording(take)).unwrap().len(),
        22_400,
        "the journal holds the whole take at 16 kHz"
    );

    assert!(before.contains(&history(HistoryChangeReason::Updated)));
    assert!(before.contains(&AppEvent::MetricsChanged(Default::default())));
    assert!(before.iter().any(|event| matches!(
        event,
        AppEvent::TranscriptSaved(saved) if saved.0.id == take && saved.0.status == TranscriptStatus::Done
    )));
    assert!(
        before
            .iter()
            .any(|event| matches!(event, AppEvent::AudioLevel(_)))
    );
    assert_eq!(
        rig.hotkeys.binding(&CANCEL),
        None,
        "Esc is released after the take"
    );
    assert!(!rig.audio.is_open());
    assert_eq!(rig.wait_for(SessionStatus::Idle).0, SessionView::IDLE);
}

/// Esc pauses the microphone; a second Esc resumes it, and the text from before and after the pause is kept.
#[test]
fn esc_then_esc_resumes_and_keeps_the_text_from_both_sides() {
    let rig = Rig::start();
    rig.engine.push_text("First part.");
    rig.engine.push_text("Second part.");
    let take = rig.record();
    rig.feed(&speech(400));
    rig.feed(&silence(700));
    rig.esc();
    let (pending, _) = rig.wait_for(SessionStatus::CancelPending);
    assert!(pending.countdown_remaining_ms.is_some());
    assert!(rig.audio.is_paused());
    assert!(
        !rig.audio.feed(&speech(200)),
        "nothing is recorded during the countdown"
    );

    rig.esc();
    rig.wait_for(SessionStatus::Recording);
    assert!(!rig.audio.is_paused());
    rig.feed(&speech(300));
    rig.stop();
    let (done, _) = rig.wait_for(SessionStatus::Done);
    assert_eq!(done.outcome, Some(DeliveryOutcome::Pasted));
    let row = transcripts::get::get(&rig.db, take).unwrap();
    assert_eq!(row.final_text.as_deref(), Some("First part. Second part."));
    assert_eq!(rig.hotkeys.binding(&CANCEL), None);
}

/// The pill's ✕ is Esc: the first press starts the countdown, the second (its undo) resumes the same take.
#[test]
fn the_pill_cancel_acts_as_esc_and_a_second_press_resumes() {
    let rig = Rig::start();
    rig.engine.push_text("Kept.");
    let take = rig.record();
    rig.feed(&speech(400));
    rig.handle.ui_input(SessionUiInput::Cancel).unwrap();
    let (pending, _) = rig.wait_for(SessionStatus::CancelPending);
    assert_eq!(pending.transcript_id, Some(take));
    assert!(rig.audio.is_paused());

    rig.handle.ui_input(SessionUiInput::Cancel).unwrap();
    let (resumed, _) = rig.wait_for(SessionStatus::Recording);
    assert_eq!(resumed.transcript_id, Some(take));
    assert!(!rig.audio.is_paused());
    rig.stop();
    let (done, _) = rig.wait_for(SessionStatus::Done);
    assert_eq!(done.outcome, Some(DeliveryOutcome::Pasted));
}

/// Esc and the countdown runs out: the row and the WAV are gone, nothing is pasted.
#[test]
fn esc_and_waiting_discards_the_row_and_the_audio() {
    let rig = Rig::with(
        settings_with(vec![(keys::CANCEL_COUNTDOWN_MS, SettingValue::Int(1_000))]),
        None,
    );
    let take = rig.record();
    rig.feed(&speech(400));
    rig.esc();
    rig.wait_for(SessionStatus::CancelPending);
    let (discarded, before) = rig.wait_for(SessionStatus::Discarded);

    assert_eq!(discarded.transcript_id, None);
    assert!(before.contains(&history(HistoryChangeReason::Deleted)));
    assert_eq!(
        transcripts::get::get(&rig.db, take).unwrap_err().error(),
        &AppError::NotFound {
            resource: ResourceKind::Transcript
        }
    );
    assert!(!rig.paths.recording(take).exists(), "the WAV is deleted");
    assert!(rig.inserter.insertions().is_empty());
    assert_eq!(rig.hotkeys.binding(&CANCEL), None);
    assert!(!rig.audio.is_open());
    assert!(rig.engine.calls().is_empty());
    assert_eq!(rig.cues(), [SessionCue::Start, SessionCue::Cancel]);
}

/// The microphone is unplugged mid-take: what was said is still delivered, with a toast.
#[test]
fn device_loss_delivers_what_was_captured() {
    let rig = Rig::start();
    rig.engine.push_text("Kept words.");
    let take = rig.record();
    rig.feed(&speech(400));
    rig.audio.lose_device();
    let (done, _) = rig.wait_for(SessionStatus::Done);

    assert_eq!(done.outcome, Some(DeliveryOutcome::Pasted));
    let row = transcripts::get::get(&rig.db, take).unwrap();
    assert_eq!(row.status, TranscriptStatus::Done);
    assert_eq!(row.final_text.as_deref(), Some("Kept words."));
    assert!(rig.notifier.toasts().contains(&DEVICE_LOST_TOAST));
    assert_eq!(rig.hotkeys.binding(&CANCEL), None);
}

/// The engine fails on a segment: the take fails with its code, the WAV stays for a retry, a toast says so.
#[test]
fn an_asr_error_fails_the_take_and_keeps_the_audio() {
    let rig = Rig::start();
    rig.engine.push_result(Err(
        PortError::new(AppError::Asr).with_detail("inference failed")
    ));
    let take = rig.record();
    rig.feed(&speech(400));
    rig.feed(&silence(700));
    let (failed, _) = rig.wait_for(SessionStatus::Failed);

    assert_eq!(failed.error, Some(AppError::Asr));
    let row = transcripts::get::get(&rig.db, take).unwrap();
    assert_eq!(row.status, TranscriptStatus::Failed);
    assert_eq!(row.error_code, Some(AppError::Asr.code()));
    assert!(row.has_audio);
    assert!(
        WavReader::open(rig.paths.recording(take)).unwrap().len() > 0,
        "the finalized WAV is kept for retry"
    );
    assert!(rig.notifier.toasts().contains(&TAKE_FAILED_TOAST));
    assert!(rig.inserter.insertions().is_empty());
    assert_eq!(rig.hotkeys.binding(&CANCEL), None);
    assert!(!rig.audio.is_open());
}

/// Only silence: nothing is pasted, the row is `empty`, the pill says no speech.
#[test]
fn a_silent_take_pastes_nothing_and_is_stored_empty() {
    let rig = Rig::start();
    let take = rig.record();
    rig.feed(&silence(600));
    rig.stop();
    let (done, _) = rig.wait_for(SessionStatus::Done);

    assert_eq!(done.outcome, Some(DeliveryOutcome::NoSpeech));
    assert_eq!(
        transcripts::get::get(&rig.db, take).unwrap().status,
        TranscriptStatus::Empty
    );
    assert!(rig.inserter.insertions().is_empty());
    assert_eq!(rig.clipboard.text().as_deref(), Some("before"));
    assert!(rig.engine.calls().is_empty());
}

/// The model is not installed: the pill says so, and no row, no microphone.
#[test]
fn a_missing_model_touches_nothing() {
    let model_id = registry::models::PARAKEET_TDT_V3;
    let rig = Rig::with(
        registry::settings::defaults(),
        Some(PortError::new(AppError::ModelMissing {
            model_id: model_id.clone(),
        })),
    );
    rig.press();
    let (failed, _) = rig.wait_for(SessionStatus::Failed);

    assert_eq!(failed.error, Some(AppError::ModelMissing { model_id }));
    assert_eq!(failed.transcript_id, None);
    assert_eq!(rig.audio.starts(), 0);
    assert_eq!(
        transcripts::get::latest(&rig.db, TranscriptStatus::Recording).unwrap(),
        None
    );
    assert_eq!(rig.hotkeys.binding(&CANCEL), None);
}

/// The pinned microphone was unplugged since it was chosen: the take records from the Windows default instead.
#[test]
fn a_pinned_microphone_that_is_gone_falls_back_to_the_default() {
    let rig = Rig::with(
        settings_with(vec![(
            keys::INPUT_DEVICE,
            SettingValue::Device(Some(StaticStr::new("usb-mic"))),
        )]),
        None,
    );
    rig.record();
    assert_eq!(rig.audio.last_device(), Some(None));
    assert!(rig.audio.is_open());
}

/// The microphone cannot open (privacy switch off): the row is kept as failed with the reason, and a toast says to
/// check the microphone.
#[test]
fn a_microphone_that_cannot_open_fails_the_take_with_its_reason() {
    let rig = Rig::start();
    let denied = AppError::PermissionDenied {
        permission: Permission::Microphone,
    };
    rig.audio.fail_next_start(denied.clone().into());
    rig.press();
    let (failed, _) = rig.wait_for(SessionStatus::Failed);

    assert_eq!(failed.error, Some(denied.clone()));
    let take = failed.transcript_id.unwrap();
    let row = transcripts::get::get(&rig.db, take).unwrap();
    assert_eq!(row.status, TranscriptStatus::Failed);
    assert_eq!(row.error_code, Some(denied.code()));
    assert!(rig.notifier.toasts().contains(&START_FAILED_TOAST));
    assert!(!rig.audio.is_open());
    assert_eq!(rig.hotkeys.binding(&CANCEL), None, "Esc was never taken");
}

/// Hold mode: recording lasts while the keys are held; the release delivers.
#[test]
fn in_hold_mode_the_release_stops_the_take() {
    let rig = Rig::with(
        settings_with(vec![(
            keys::HOTKEY_MODE,
            SettingValue::Enum(StaticStr::new(values::HOLD)),
        )]),
        None,
    );
    rig.engine.push_text("Held.");
    let take = rig.record();
    rig.feed(&speech(400));
    assert!(rig.hotkeys.release(&RECORD));
    rig.wait_for(SessionStatus::Done);
    assert_eq!(
        transcripts::get::get(&rig.db, take)
            .unwrap()
            .final_text
            .as_deref(),
        Some("Held.")
    );
}

/// The app exits mid-take: the WAV header is finalized first, and the row is left for startup recovery.
#[test]
fn exiting_while_recording_finalizes_the_journal() {
    let rig = Rig::start();
    let take = rig.record();
    rig.feed(&speech(400));
    // Closing the capture drains its ring before the journal is finalized, so everything fed is on disk.
    assert!(rig.handle.shutdown(WAIT));

    assert_eq!(
        WavReader::open(rig.paths.recording(take)).unwrap().len(),
        6_400
    );
    assert_eq!(
        transcripts::get::get(&rig.db, take).unwrap().status,
        TranscriptStatus::Recording,
        "startup recovery finds it (02 §7.3)"
    );
    assert!(!rig.audio.is_open());
}

/// A panic anywhere in the process (reported by the panic hook) fails the live take with its audio kept, and the
/// next take works.
#[test]
fn a_reported_panic_fails_the_live_take_and_keeps_its_audio() {
    let rig = Rig::start();
    let take = rig.record();
    rig.feed(&speech(400));
    rig.handle.panic_reporter().report();
    let (failed, _) = rig.wait_for(SessionStatus::Failed);

    assert_eq!(failed.error, Some(AppError::Internal));
    assert_eq!(failed.transcript_id, Some(take));
    let row = transcripts::get::get(&rig.db, take).unwrap();
    assert_eq!(row.status, TranscriptStatus::Failed);
    assert_eq!(row.error_code, Some(AppError::Internal.code()));
    assert!(row.has_audio);
    assert!(
        WavReader::open(rig.paths.recording(take)).unwrap().len() > 0,
        "the finalized WAV is kept for retry"
    );
    assert!(rig.notifier.toasts().contains(&TAKE_FAILED_TOAST));
    assert!(rig.inserter.insertions().is_empty());
    assert_eq!(rig.hotkeys.binding(&CANCEL), None);
    assert!(!rig.audio.is_open());

    rig.wait_for(SessionStatus::Idle);
    thread::sleep(PAST_DEBOUNCE);
    assert_ne!(rig.record(), take, "dictation keeps working");
}

/// A reported panic with no take in progress changes nothing.
#[test]
fn a_reported_panic_while_idle_is_ignored() {
    let rig = Rig::start();
    rig.handle.panic_reporter().report();
    assert_eq!(rig.view(), SessionView::IDLE);
    assert!(rig.notifier.toasts().is_empty());
}

/// The actor itself panics while running a transition's effects: it fails the take (audio kept), starts over at
/// Idle and keeps answering hotkeys.
#[test]
fn a_panic_inside_the_actor_fails_the_take_and_the_session_starts_over() {
    let rig = Rig::panicking_on(SessionStatus::Recording);
    rig.press();
    rig.wait_for(SessionStatus::Idle);

    let row = transcripts::get::latest(&rig.db, TranscriptStatus::Failed)
        .unwrap()
        .expect("the take was marked failed");
    assert_eq!(row.error_code, Some(AppError::Internal.code()));
    assert!(row.has_audio);
    assert!(
        rig.paths.recording(row.id).is_file(),
        "the journal is finalized and kept"
    );
    assert!(rig.notifier.toasts().contains(&TAKE_FAILED_TOAST));
    assert!(!rig.audio.is_open(), "the microphone was released");
    assert_eq!(rig.hotkeys.binding(&CANCEL), None, "Esc was released");
    assert_eq!(rig.view(), SessionView::IDLE);

    thread::sleep(PAST_DEBOUNCE);
    let next = rig.record();
    assert_ne!(next, row.id, "dictation keeps working after the panic");
}

/// With `storage.audio_retention_days` at 0, a take that succeeds loses its audio as soon as it settles.
#[test]
fn zero_day_audio_retention_deletes_the_journal_right_after_success() {
    let rig = Rig::with(
        settings_with(vec![
            (
                keys::HOTKEY_MODE,
                SettingValue::Enum(StaticStr::new(values::TOGGLE)),
            ),
            (keys::AUDIO_RETENTION_DAYS, SettingValue::Int(0)),
        ]),
        None,
    );
    rig.engine.push_text("Delete me after.");
    let take = rig.record();
    rig.feed(&speech(400));
    rig.feed(&silence(700));
    rig.stop();
    rig.wait_for(SessionStatus::Done);
    rig.wait_for(SessionStatus::Idle);

    let row = transcripts::get::get(&rig.db, take).unwrap();
    assert_eq!(row.status, TranscriptStatus::Done);
    assert!(!row.has_audio);
    assert!(!rig.paths.recording(take).exists());
}

/// The start chime waits for the open microphone and the stop chime follows the stop; with sound cues off, nothing
/// plays.
#[test]
fn sound_cues_mark_the_start_and_the_stop_of_a_take() {
    let rig = Rig::start();
    rig.dictate("Chimed.");
    assert_eq!(rig.cues(), [SessionCue::Start, SessionCue::Stop]);

    let quiet = Rig::with(
        settings_with(vec![
            (
                keys::HOTKEY_MODE,
                SettingValue::Enum(StaticStr::new(values::TOGGLE)),
            ),
            (keys::SOUND_CUES, SettingValue::Bool(false)),
        ]),
        None,
    );
    quiet.dictate("Silent.");
    assert!(quiet.sounds.played().is_empty());
}

/// The paste-last hotkey pastes the newest finished take into the focused window again, the same way
/// `history_paste_last` does.
#[test]
fn the_paste_last_hotkey_pastes_the_newest_take_again() {
    let rig = Rig::start();
    assert!(
        rig.hotkeys.binding(&PASTE_LAST).is_some(),
        "paste-last is bound once the session is prepared"
    );
    rig.dictate("First.");
    rig.dictate("Second.");
    assert!(rig.hotkeys.press(&PASTE_LAST));
    assert!(eventually(|| rig.inserter.insertions().len() == 3));
    assert_eq!(
        rig.inserter.insertions().last(),
        Some(&(notepad(), String::from("Second. ")))
    );
    assert_eq!(
        rig.cues(),
        [
            SessionCue::Start,
            SessionCue::Stop,
            SessionCue::Start,
            SessionCue::Stop
        ],
        "a paste-last plays no cue"
    );
    assert_eq!(rig.view(), SessionView::IDLE, "a paste-last starts no take");
}

/// With nothing dictated yet, the paste-last hotkey says so in a toast instead of failing silently.
#[test]
fn the_paste_last_hotkey_with_an_empty_history_toasts() {
    let rig = Rig::start();
    assert!(rig.hotkeys.press(&PASTE_LAST));
    assert!(eventually(|| rig
        .notifier
        .toasts()
        .contains(&NOTHING_TO_PASTE_TOAST)));
    assert!(rig.inserter.insertions().is_empty());
}

/// Hold mode: holding the record chord and adding the paste-last key (Ctrl+Alt, then V) drops the take the chord
/// began, silently, and pastes the last take.
#[test]
fn in_hold_mode_the_paste_last_chord_drops_the_new_take_and_pastes() {
    let rig = Rig::with(
        settings_with(vec![(
            keys::HOTKEY_MODE,
            SettingValue::Enum(StaticStr::new(values::HOLD)),
        )]),
        None,
    );
    rig.engine.push_text("Held words.");
    rig.record();
    rig.feed(&speech(400));
    assert!(rig.hotkeys.release(&RECORD));
    rig.wait_for(SessionStatus::Done);
    rig.wait_for(SessionStatus::Idle);
    let cues_before = rig.cues();

    thread::sleep(PAST_DEBOUNCE);
    rig.press();
    let (arming, _) = rig.wait_for(SessionStatus::Arming);
    let dropped = arming.transcript_id.unwrap();
    assert!(rig.hotkeys.interrupt(&RECORD));
    assert!(rig.hotkeys.press(&PASTE_LAST));
    rig.wait_for(SessionStatus::Discarded);

    assert!(eventually(|| rig.inserter.insertions().len() == 2));
    assert_eq!(
        rig.inserter.insertions().last(),
        Some(&(notepad(), String::from("Held words. ")))
    );
    assert_eq!(
        transcripts::get::get(&rig.db, dropped).unwrap_err().error(),
        &AppError::NotFound {
            resource: ResourceKind::Transcript
        },
        "the take the chord began leaves no row"
    );
    assert!(!rig.paths.recording(dropped).exists());
    assert!(!rig.audio.is_open());
    let cues_after = rig.cues();
    assert!(
        !cues_after[cues_before.len()..].contains(&SessionCue::Stop),
        "a dropped take never chimes a stop"
    );
}

/// A take on a Bluetooth microphone shows the hint once, remembers it in its hidden setting, and never again.
#[test]
fn a_bluetooth_microphone_gets_its_hint_once() {
    let rig = Rig::start();
    rig.audio.set_default_transport(AudioTransport::Bluetooth);
    rig.dictate("One.");
    rig.dictate("Two.");

    let hints = rig
        .notifier
        .toasts()
        .into_iter()
        .filter(|toast| *toast == BLUETOOTH_MIC_NOTICE.toast)
        .count();
    assert_eq!(hints, 1);
    assert!(
        services::settings::get::all(&rig.db)
            .unwrap()
            .contains(&(BLUETOOTH_MIC_NOTICE.shown.clone(), SettingValue::Bool(true)))
    );
}

/// A wired microphone never shows the Bluetooth hint.
#[test]
fn a_wired_microphone_gets_no_bluetooth_hint() {
    let rig = Rig::start();
    rig.audio.set_default_transport(AudioTransport::Usb);
    rig.dictate("Wired.");
    assert!(!rig.notifier.toasts().contains(&BLUETOOTH_MIC_NOTICE.toast));
    assert!(
        !services::settings::get::all(&rig.db)
            .unwrap()
            .contains(&(BLUETOOTH_MIC_NOTICE.shown.clone(), SettingValue::Bool(true)))
    );
}

/// Onboarding's hotkey test: with Echo focused, presses are reported and start nothing; the same press in another
/// app starts a take, so a rehearsal left on never breaks dictation elsewhere.
#[test]
fn a_hotkey_rehearsal_reports_presses_in_echo_and_records_elsewhere() {
    let rig = Rig::start();
    rig.foreground.set(Some(echo_window()));
    rig.rehearse(SessionRehearsal::Hotkey);
    rig.press();
    let (rehearsed, before) = rig.wait_for_rehearsed();
    assert_eq!(
        rehearsed,
        HotkeyRehearsed {
            hotkey: RECORD,
            action: HotkeyAction::Record,
            state: KeyState::Pressed,
        }
    );
    assert!(before.is_empty(), "nothing else happened: {before:?}");
    assert_eq!(rig.view(), SessionView::IDLE);
    assert!(
        !rig.audio.is_open(),
        "no microphone opened for a rehearsed press"
    );

    rig.foreground.set(Some(notepad()));
    thread::sleep(PAST_DEBOUNCE);
    let take = rig.record();
    assert_eq!(
        transcripts::get::get(&rig.db, take).unwrap().status,
        TranscriptStatus::Recording
    );
}

/// A rehearsal never swallows the keys of a take already running: Esc still starts the cancel countdown.
#[test]
fn a_hotkey_rehearsal_leaves_a_running_take_alone() {
    let rig = Rig::start();
    rig.record();
    rig.foreground.set(Some(echo_window()));
    rig.rehearse(SessionRehearsal::Hotkey);
    rig.esc();
    rig.wait_for(SessionStatus::CancelPending);
}

/// Onboarding's practice take: it runs end to end, is stored, and its text stays in Echo (Shown) with the
/// clipboard and every app untouched; turned off, the next take pastes again.
#[test]
fn a_take_rehearsal_in_echo_shows_the_text_instead_of_pasting_it() {
    let rig = Rig::start();
    rig.foreground.set(Some(echo_window()));
    rig.rehearse(SessionRehearsal::Take);
    rig.engine.push_text("Practice makes perfect.");
    let take = rig.record();
    rig.feed(&speech(400));
    rig.feed(&silence(700));
    rig.stop();
    let (done, _) = rig.wait_for(SessionStatus::Done);
    assert_eq!(done.outcome, Some(DeliveryOutcome::Shown));
    assert_eq!(done.transcript_id, Some(take));
    assert!(rig.inserter.insertions().is_empty(), "nothing was pasted");
    assert_eq!(
        rig.clipboard.text().as_deref(),
        Some("before"),
        "the clipboard is untouched"
    );
    let row = transcripts::get::get(&rig.db, take).unwrap();
    assert_eq!(row.status, TranscriptStatus::Done);
    assert_eq!(row.final_text.as_deref(), Some("Practice makes perfect."));
    rig.wait_for(SessionStatus::Idle);

    rig.rehearse(SessionRehearsal::Off);
    rig.foreground.set(Some(notepad()));
    thread::sleep(PAST_DEBOUNCE);
    rig.dictate("Back to work.");
    assert_eq!(
        rig.inserter.insertions(),
        [(notepad(), String::from("Back to work. "))]
    );
}
