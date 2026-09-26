/*!
 * SOURCE OF TRUTH KEYWORDS: session_retry, retry take, re-transcribe saved audio, retry from WAV, RetryDeps, drip-fed segments, recoverable take, failed take retry
 * WHAT:  `retry`: re-runs a stored take from its WAV journal through the same pipeline a live take uses (the
 *        segmenter, the ASR worker, the shared polish chain) and rewrites its row with the new outcome: `done` with
 *        text, or `empty`. Returns the take's updated list row and announces it (TranscriptSaved, HistoryChanged,
 *        MetricsChanged). Nothing is pasted: the app the take was meant for is long gone; History offers Copy.
 * WHY:   02 §7.3 "never lose a take": a failed or recovered take is only useful once its audio becomes text, and a
 *        retry must hear exactly what the first pass heard (05 A2), so it replays the journal through the same
 *        Segmenter and the same engine check (`usable_engine`), and stores the row by the same rules as the state
 *        machine (pipeline/session/rows.rs). Segments are fed one at a time, each after the previous result, so a
 *        live take that starts meanwhile waits behind at most one retry segment on the shared ASR thread instead of
 *        a whole backlog (its stop → paste budget, 02 §6.2). The take the session still owns is refused (`Busy`);
 *        every journal gets its header repaired first (journal::repair, idempotent): a crash, or a panic that failed
 *        the take right before the process died, can leave the last second uncounted. The
 *        row is written only once everything succeeded, so a failed retry leaves the take exactly as it was (its
 *        error toast says why), and the stored error code is cleared on success. The engine that ran is stored,
 *        so a retry after switching engines records the new one (01 "re-run with a different engine"). Latency
 *        stays as it was: a retry has no stop → paste moment. A retry that succeeds with `storage.audio_retention_days`
 *        at 0 loses its audio like a live take (pipeline/retention.rs). Transcript text is never logged (02 §10).
 * WHERE: ipc/commands/session.rs (`session_retry`); built from CommandCtx (the same ASR worker, SessionEngines,
 *        paths, database and event sink as the session actor).
 */

use std::{
    collections::BTreeMap,
    sync::{Arc, mpsc},
    time::Duration,
};

use super::{
    asr::{self, AsrWorker, EngineCheck},
    blocking::run_blocking,
    capture::{self, journal},
    history,
    polish::{join_segments, polish_context},
    retention,
    session::{SessionEngines, rows},
};
use crate::{
    ports::EventSink,
    registry, services,
    services::Db,
    types::{
        AppError, AppEvent, AppPaths, AsrEvent, AsrOutput, CaptureSummary, EngineId, PortError,
        PortResult, ResourceKind, SessionView, SettingsSnapshot, TranscriptChange, TranscriptId,
        TranscriptStatus, TranscriptSummary,
    },
};

/// How long one segment may wait for its result, including a model load that segment triggered.
const SEGMENT_TIMEOUT: Duration = Duration::from_secs(120);

/// Everything a retry works through; the same instances the session actor uses.
#[derive(Clone)]
pub struct RetryDeps {
    pub asr: AsrWorker,
    pub engines: SessionEngines,
    pub paths: AppPaths,
    pub db: Db,
    pub events: Arc<dyn EventSink<AppEvent>>,
}

/// What the audio of a take came to after segmentation and speech recognition.
struct Heard {
    engine: EngineId,
    audio: CaptureSummary,
    /// Segment outputs by index.
    outputs: BTreeMap<u32, AsrOutput>,
}

/**
 * SOURCE OF TRUTH KEYWORDS: retry entry point, retry row rewrite, retry outcome
 * WHAT:  Retries take `id` under `settings`; `session` is the session's view now (its live take is refused).
 * WHY:   See the file header. Errors keep their meaning for the UI: `NotFound { transcript }` (deleted),
 *        `NotFound { recording }` (no audio kept), `Busy` (still in progress), `ModelMissing`, `Asr`, `Storage`.
 * WHERE: session_retry.
 */
pub async fn retry(
    deps: &RetryDeps,
    settings: Arc<SettingsSnapshot>,
    session: &SessionView,
    id: TranscriptId,
) -> PortResult<TranscriptSummary> {
    history::ensure_not_live(session, id)?;
    let take = services::transcripts::get::get(&deps.db, id)?;
    if !take.has_audio {
        return Err(no_recording());
    }
    let engine = match asr::usable_engine(&deps.asr, &settings, &deps.paths)? {
        EngineCheck::Usable(engine) => engine,
        EngineCheck::ModelMissing(model_id) => {
            return Err(PortError::new(AppError::ModelMissing { model_id }));
        }
    };
    let heard = {
        let deps = deps.clone();
        let settings = Arc::clone(&settings);
        run_blocking("retrying the take", move || {
            hear(&deps, &settings, id, engine)
        })
        .await?
    };
    let changes = outcome(deps, &settings, heard).await?;
    services::transcripts::update::update(&deps.db, id, &changes)?;
    let policy = registry::settings::retention_policy(&settings);
    if let Err(error) = retention::release_after_success(&deps.db, &deps.paths, policy, id) {
        tracing::warn!(
            take = %id,
            detail = error.detail(),
            "the retried take's audio could not be deleted after success; the retention sweep retries"
        );
    }
    history::announce_saved(&deps.db, deps.events.as_ref(), id);
    tracing::info!(take = %id, "the take was retried");
    services::transcripts::get::summary(&deps.db, id)
}

/// Reads the journal, cuts it into segments and transcribes them one at a time; runs on the blocking pool.
fn hear(
    deps: &RetryDeps,
    settings: &SettingsSnapshot,
    id: TranscriptId,
    engine: EngineId,
) -> PortResult<Heard> {
    let path = deps.paths.recording(id);
    if !path.is_file() {
        return Err(no_recording());
    }
    // Idempotent on a finalized journal; recounts one a crash (or a panic right before the process died) left stale.
    journal::repair(&path)?;
    let samples = journal::read(&path)?;
    let replay = capture::replay(
        &samples,
        (deps.engines.vad)()?,
        asr::segment_policy(&engine),
    )?;
    let (sender, results) = mpsc::channel();
    let asr_take = deps.asr.begin_take(
        id,
        registry::settings::language_preference(settings),
        Arc::new(ResultSink(sender)),
    );
    let mut outputs = BTreeMap::new();
    for segment in replay.segments {
        let index = segment.index;
        asr_take.emit(segment);
        outputs.insert(index, wait_for(&results, id, index)?);
    }
    // Dropping the take releases its engine pin on the worker.
    drop(asr_take);
    Ok(Heard {
        engine,
        audio: replay.summary,
        outputs,
    })
}

/// Waits for segment `index` of take `id` and returns its text, or its error.
fn wait_for(
    results: &mpsc::Receiver<AsrEvent>,
    id: TranscriptId,
    index: u32,
) -> PortResult<AsrOutput> {
    loop {
        let event = results.recv_timeout(SEGMENT_TIMEOUT).map_err(|_| {
            PortError::new(AppError::Asr).with_detail(format!(
                "segment {index} of the retry got no result in time"
            ))
        })?;
        match event {
            AsrEvent::SegmentDone {
                take,
                index: done,
                output,
            } if take == id && done == index => return Ok(output),
            AsrEvent::SegmentFailed {
                take,
                index: failed,
                error,
            } if take == id && failed == index => return Err(error),
            // Nothing else is expected on this take's sink; skip it rather than guess.
            _ => {}
        }
    }
}

/// The row writes for what was heard: polished and `done`, or `empty`.
async fn outcome(
    deps: &RetryDeps,
    settings: &SettingsSnapshot,
    heard: Heard,
) -> PortResult<Vec<TranscriptChange>> {
    let Heard {
        engine,
        audio,
        outputs,
    } = heard;
    let text = join_segments(outputs.values().map(|output| output.text.as_str()));
    let language = outputs.values().find_map(|output| output.language.clone());
    let mut changes = rows::measured(&audio);
    changes.extend([
        TranscriptChange::EngineId(engine.clone()),
        TranscriptChange::ErrorCode(None),
        TranscriptChange::Language(language.clone()),
    ]);
    let min_speech_ms = registry::settings::session_policy(settings).min_speech_ms;
    if rows::is_empty(&audio, &text, min_speech_ms) {
        changes.extend(empty_changes(Vec::new()));
        changes.push(TranscriptChange::RawText(None));
        return Ok(changes);
    }
    changes.push(TranscriptChange::RawText(Some(text.clone())));
    let caps = registry::engines::find(&engine)
        .and_then(|entry| entry.asr_caps())
        .ok_or_else(|| {
            PortError::new(AppError::Internal)
                .with_detail("the retry's engine is not a registered ASR engine")
        })?;
    let context = polish_context(settings, caps, language);
    let chain = deps.engines.polish.for_settings(settings);
    let polish = chain.run(&text, &context).await;
    match rows::completed(&polish.text) {
        Some(completed) => {
            changes.extend(completed);
            changes.push(TranscriptChange::PolisherIds(polish.polisher_ids));
        }
        None => changes.extend(empty_changes(polish.polisher_ids)),
    }
    Ok(changes)
}

/// A take that holds no text after all.
fn empty_changes(polisher_ids: Vec<EngineId>) -> [TranscriptChange; 4] {
    [
        TranscriptChange::Status(TranscriptStatus::Empty),
        TranscriptChange::FinalText(None),
        TranscriptChange::WordCount(0),
        TranscriptChange::PolisherIds(polisher_ids),
    ]
}

fn no_recording() -> PortError {
    PortError::new(AppError::NotFound {
        resource: ResourceKind::Recording,
    })
}

/// The retry's ASR sink: every event goes to the waiting blocking thread.
struct ResultSink(mpsc::Sender<AsrEvent>);

impl EventSink<AsrEvent> for ResultSink {
    fn emit(&self, event: AsrEvent) {
        // The retry stops listening only when it has failed or finished; later events have nobody to tell.
        let _ = self.0.send(event);
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs::OpenOptions,
        io::{Seek, SeekFrom, Write},
    };

    use super::*;
    use crate::{
        pipeline::{asr::AsrWorkerConfig, capture::journal::Journal},
        ports::{
            AsrEngine,
            fakes::{FakeAsrEngine, FakeVoiceActivity, FakeWorkerScheduler, RecordingSink},
        },
        registry::{
            engines::{BuildCtx, PARAKEET_TDT_V3},
            settings::{defaults, keys, resolve},
        },
        types::{
            AppErrorCode, HistoryChangeReason, HistoryChanged, NewTranscript, SessionStatus,
            SettingValue, TranscriptSaved, UnixMs, testing::TempDir,
        },
    };

    /// 16-bit samples the fake detector hears as speech, and as silence.
    const SPEECH: i16 = 16_000;
    const SILENCE: i16 = 0;

    struct Fixture {
        deps: RetryDeps,
        engine: Arc<FakeAsrEngine>,
        events: Arc<RecordingSink<AppEvent>>,
        _dir: TempDir,
    }

    fn fixture() -> Fixture {
        let dir = TempDir::new("retry");
        let paths = AppPaths::new(dir.join("data"), dir.join("resources"));
        let engine = Arc::new(FakeAsrEngine::english());
        let shared = Arc::clone(&engine);
        let asr = AsrWorker::spawn(AsrWorkerConfig {
            accelerators: crate::pipeline::asr::AcceleratorPicker::without_gpu(),
            build: Arc::new(move |_: &EngineId| Ok(Arc::clone(&shared) as Arc<dyn AsrEngine>)),
            scheduler: Arc::new(FakeWorkerScheduler::default()),
            readiness: None,
        })
        .unwrap();
        let polish_ctx = BuildCtx {
            paths: paths.clone(),
        };
        let events = Arc::new(RecordingSink::default());
        Fixture {
            deps: RetryDeps {
                asr,
                engines: SessionEngines::new(
                    Arc::new(|| Ok(Box::new(FakeVoiceActivity::new(32)) as _)),
                    Arc::new(move |id: &EngineId| {
                        registry::engines::build_polisher(id, &polish_ctx)
                    }),
                ),
                paths,
                db: Db::open_in_memory().unwrap(),
                events: Arc::clone(&events) as _,
            },
            engine,
            events,
            _dir: dir,
        }
    }

    fn run<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(future)
    }

    /// `ms` of 16 kHz samples of one value.
    fn stretch(ms: usize, value: i16) -> Vec<i16> {
        vec![value; ms * 16]
    }

    /// A stored take in `status` whose journal holds `audio`.
    fn stored_take(fixture: &Fixture, status: TranscriptStatus, audio: &[i16]) -> TranscriptId {
        let id = TranscriptId::generate();
        services::transcripts::insert::insert(
            &fixture.deps.db,
            &NewTranscript {
                id,
                created_at: UnixMs::now(),
                status: TranscriptStatus::Recording,
                audio_path: Some(AppPaths::recording_name(id)),
                engine_id: Some(PARAKEET_TDT_V3),
                app_name: Some("notepad.exe".to_owned()),
            },
        )
        .unwrap();
        let mut changes = vec![TranscriptChange::Status(status)];
        if status == TranscriptStatus::Failed {
            changes.push(TranscriptChange::ErrorCode(Some(AppErrorCode::Asr)));
        }
        services::transcripts::update::update(&fixture.deps.db, id, &changes).unwrap();
        let mut journal = Journal::create(&fixture.deps.paths.recording(id)).unwrap();
        journal.append(audio).unwrap();
        journal.finalize().unwrap();
        id
    }

    fn retry_now(fixture: &Fixture, id: TranscriptId) -> PortResult<TranscriptSummary> {
        run(retry(
            &fixture.deps,
            Arc::new(defaults()),
            &SessionView::IDLE,
            id,
        ))
    }

    fn two_sentences() -> Vec<i16> {
        let mut audio = stretch(1_000, SPEECH);
        audio.extend(stretch(1_000, SILENCE));
        audio.extend(stretch(1_000, SPEECH));
        audio.extend(stretch(300, SILENCE));
        audio
    }

    #[test]
    fn a_failed_take_is_transcribed_polished_and_stored_as_done() {
        let fixture = fixture();
        let id = stored_take(&fixture, TranscriptStatus::Failed, &two_sentences());
        fixture.engine.push_text("Hello there.");
        fixture.engine.push_text("General Kenobi.");

        let row = retry_now(&fixture, id).unwrap();
        assert_eq!(row.status, TranscriptStatus::Done);
        assert_eq!(row.word_count, Some(4));
        assert_eq!(row.error_code, None);
        assert!(row.has_audio, "a retry keeps the audio");

        let take = services::transcripts::get::get(&fixture.deps.db, id).unwrap();
        assert_eq!(
            take.final_text.as_deref(),
            Some("Hello there. General Kenobi.")
        );
        assert_eq!(
            take.raw_text.as_deref(),
            Some("Hello there. General Kenobi.")
        );
        assert_eq!(take.duration_ms, Some(3_300));
        assert!(take.speech_ms.is_some_and(|ms| ms >= 1_900));
        assert_eq!(take.engine_id, Some(PARAKEET_TDT_V3));
        assert!(!take.polisher_ids.is_empty());
        assert_eq!(fixture.engine.calls().len(), 2, "one call per segment");

        let events = fixture.events.events();
        assert!(
            matches!(&events[0], AppEvent::TranscriptSaved(TranscriptSaved(saved)) if saved.id == id)
        );
        assert!(events.contains(&AppEvent::from(HistoryChanged {
            reason: HistoryChangeReason::Updated
        })));
    }

    #[test]
    fn silent_audio_becomes_an_empty_take_without_asking_the_engine() {
        let fixture = fixture();
        let id = stored_take(&fixture, TranscriptStatus::Failed, &stretch(2_000, SILENCE));
        let row = retry_now(&fixture, id).unwrap();
        assert_eq!(row.status, TranscriptStatus::Empty);
        assert_eq!(row.error_code, None);
        let take = services::transcripts::get::get(&fixture.deps.db, id).unwrap();
        assert_eq!(take.final_text, None);
        assert_eq!(take.word_count, Some(0));
        assert!(fixture.engine.calls().is_empty());
    }

    #[test]
    fn a_crashed_take_gets_its_header_repaired_before_the_retry() {
        let fixture = fixture();
        let id = stored_take(&fixture, TranscriptStatus::Recording, &two_sentences());
        // A killed process leaves header sizes that count nothing.
        let mut file = OpenOptions::new()
            .write(true)
            .open(fixture.deps.paths.recording(id))
            .unwrap();
        for offset in [4, 40] {
            file.seek(SeekFrom::Start(offset)).unwrap();
            file.write_all(&0_u32.to_le_bytes()).unwrap();
        }
        drop(file);
        fixture.engine.push_text("Hello there.");
        fixture.engine.push_text("General Kenobi.");

        let row = retry_now(&fixture, id).unwrap();
        assert_eq!(row.status, TranscriptStatus::Done);
        assert_eq!(row.duration_ms, Some(3_300));
    }

    /// Zeroes the journal's RIFF and `data` sizes, as a process killed before its first header rewrite leaves them.
    fn zero_header(fixture: &Fixture, id: TranscriptId) {
        let mut file = OpenOptions::new()
            .write(true)
            .open(fixture.deps.paths.recording(id))
            .unwrap();
        for offset in [4, 40] {
            file.seek(SeekFrom::Start(offset)).unwrap();
            file.write_all(&0_u32.to_le_bytes()).unwrap();
        }
    }

    #[test]
    fn a_failed_take_with_a_stale_header_is_recounted_and_zero_day_audio_goes_after_success() {
        let fixture = fixture();
        // The panic hook failed the take, then the process died before the journal was finalized.
        let id = stored_take(&fixture, TranscriptStatus::Failed, &two_sentences());
        zero_header(&fixture, id);
        fixture.engine.push_text("Hello there.");
        fixture.engine.push_text("General Kenobi.");
        let settings = resolve([(keys::AUDIO_RETENTION_DAYS, SettingValue::Int(0))]);

        let row = run(retry(
            &fixture.deps,
            Arc::new(settings),
            &SessionView::IDLE,
            id,
        ))
        .unwrap();
        assert_eq!(row.status, TranscriptStatus::Done);
        assert_eq!(row.duration_ms, Some(3_300), "every sample was heard");
        assert!(
            !row.has_audio,
            "0 days deletes the audio right after success"
        );
        assert!(!fixture.deps.paths.recording(id).exists());
    }

    #[test]
    fn a_failed_retry_leaves_the_take_as_it_was() {
        let fixture = fixture();
        let id = stored_take(&fixture, TranscriptStatus::Failed, &two_sentences());
        fixture.engine.push_result(Err(
            PortError::new(AppError::Asr).with_detail("fake failure")
        ));

        assert_eq!(retry_now(&fixture, id).unwrap_err().error(), &AppError::Asr);
        let take = services::transcripts::get::get(&fixture.deps.db, id).unwrap();
        assert_eq!(take.status, TranscriptStatus::Failed);
        assert_eq!(take.error_code, Some(AppErrorCode::Asr));
        assert!(fixture.events.events().is_empty());
    }

    #[test]
    fn takes_without_audio_unknown_takes_and_the_live_take_are_refused() {
        let fixture = fixture();
        assert_eq!(
            retry_now(&fixture, TranscriptId::generate())
                .unwrap_err()
                .error(),
            &AppError::NotFound {
                resource: ResourceKind::Transcript
            }
        );

        let swept = stored_take(&fixture, TranscriptStatus::Done, &two_sentences());
        services::transcripts::update::update(
            &fixture.deps.db,
            swept,
            &[TranscriptChange::AudioPath(None)],
        )
        .unwrap();
        assert_eq!(
            retry_now(&fixture, swept).unwrap_err().error(),
            &AppError::NotFound {
                resource: ResourceKind::Recording
            }
        );

        let vanished = stored_take(&fixture, TranscriptStatus::Failed, &two_sentences());
        journal::remove(&fixture.deps.paths.recording(vanished)).unwrap();
        assert_eq!(
            retry_now(&fixture, vanished).unwrap_err().error(),
            &AppError::NotFound {
                resource: ResourceKind::Recording
            }
        );

        let live = stored_take(&fixture, TranscriptStatus::Transcribing, &two_sentences());
        let session = SessionView {
            status: SessionStatus::Finalizing,
            transcript_id: Some(live),
            ..SessionView::IDLE
        };
        let refused = run(retry(&fixture.deps, Arc::new(defaults()), &session, live));
        assert_eq!(refused.unwrap_err().error(), &AppError::Busy);
        assert!(fixture.engine.calls().is_empty());
    }
}
