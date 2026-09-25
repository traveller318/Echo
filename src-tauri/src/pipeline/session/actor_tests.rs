/*!
 * SOURCE OF TRUTH KEYWORDS: session actor tests, pipeline tests, full take test, Esc undo test, Esc discard test, device loss test, ASR error test, exit mid-take test
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
    sync::Arc,
    thread::{self, JoinHandle},
    time::Duration,
};

use hound::WavReader;

use super::{
    DEVICE_LOST_TOAST, START_FAILED_TOAST, SessionActor, SessionConfig, SessionEngines,
    SessionHandle, TAKE_FAILED_TOAST,
};
use crate::{
    pipeline::{
        asr::{AsrWorker, AsrWorkerConfig},
        delivery::{Delivery, DeliveryPorts},
    },
    ports::{
        AsrEngine,
        fakes::{
            ChannelSink, FakeAsrEngine, FakeAudioCapture, FakeClipboard, FakeForegroundApp,
            FakeHotkeyService, FakeNotifier, FakeTextInserter, FakeVoiceActivity,
            FakeWorkerScheduler,
        },
    },
    registry::{
        self,
        engines::{BuildCtx, PARAKEET_TDT_V3},
        hotkeys::{CANCEL, RECORD},
        settings::{keys, values},
    },
    services::{Db, transcripts},
    types::{
        Accelerator, AppError, AppEvent, AppPaths, AppTarget, AsrLoadRequest, CaptureFormat,
        DeliveryOutcome, EngineId, HistoryChangeReason, HistoryChanged, Permission, PortError,
        ResourceKind, SessionStatus, SessionView, SettingKey, SettingValue, SettingsSnapshot,
        SharedSettings, StaticStr, TranscriptId, TranscriptStatus, testing::TempDir,
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
    notifier: Arc<FakeNotifier>,
    clipboard: Arc<FakeClipboard>,
    inserter: Arc<FakeTextInserter>,
    engine: Arc<FakeAsrEngine>,
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
        let dir = TempDir::new("session-actor");
        let paths = AppPaths::new(dir.join("data"), dir.join("resources"));
        let events = Arc::new(ChannelSink::default());
        let audio = Arc::new(FakeAudioCapture::new(STEREO_48K));
        let hotkeys = Arc::new(FakeHotkeyService::default());
        let notifier = Arc::new(FakeNotifier::default());
        let clipboard = Arc::new(FakeClipboard::with_text("before"));
        let inserter = Arc::new(FakeTextInserter::default());
        let engine = Arc::new(FakeAsrEngine::english());
        let db = Db::open_in_memory().unwrap();

        let shared = Arc::clone(&engine);
        let asr = AsrWorker::spawn(AsrWorkerConfig {
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
                accelerator: Accelerator::Cpu,
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
                foreground: Arc::new(FakeForegroundApp::focused(notepad())),
                notifier: Arc::clone(&notifier) as _,
                delivery: Delivery::new(DeliveryPorts {
                    clipboard: Arc::clone(&clipboard) as _,
                    inserter: Arc::clone(&inserter) as _,
                    notifier: Arc::clone(&notifier) as _,
                }),
                paths: paths.clone(),
                db: db.clone(),
                events: Arc::clone(&events) as _,
                engines: SessionEngines::new(
                    Arc::new(|| Ok(Box::new(FakeVoiceActivity::new(32)) as _)),
                    Arc::new(move |id: &EngineId| {
                        registry::engines::build_polisher(id, &polish_ctx)
                    }),
                ),
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
            notifier,
            clipboard,
            inserter,
            engine,
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
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.handle.shutdown(WAIT);
        if let Some(runner) = self.runner.take() {
            let _ = runner.join();
        }
    }
}

fn notepad() -> AppTarget {
    FakeForegroundApp::target("notepad.exe", false)
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
