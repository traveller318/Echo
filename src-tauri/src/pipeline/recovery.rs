/*!
 * SOURCE OF TRUTH KEYWORDS: startup recovery, recover takes, crash recovery, stuck recording rows, recoverable take, repair WAV header, Recovered N takes toast, recovery_toasts
 * WHAT:  `recover`: at startup, every take a crash left `recording`/`transcribing` gets its WAV journal repaired
 *        (journal::repair) and becomes `recoverable` with its real duration, so History can retry it; returns a
 *        RecoveryReport. `recovery_toasts` / `announce`: the "Recovered N take(s)" toast (and one for takes whose
 *        audio could not be read), plus HistoryChanged { recovered }.
 * WHY:   02 §7.3 step 4 and 00 constraint 5 ("never lose a take"): the row is inserted before the microphone opens
 *        and the journal's header is rewritten every second, so after a crash the audio is on disk and only the
 *        header and the status are stale. Recovery runs in app/bootstrap before any window, command or session
 *        exists, so it can never race a live take or a retry for the same row. Each take is settled on its own:
 *        one unreadable journal never stops the rest. A journal with no samples at all (the crash came between
 *        the row insert and the first audio) holds nothing to retry, so it is stored `empty` like any silent take
 *        (05 A4) instead of promising a recovery. A journal that is missing or not a WAV cannot be retried either:
 *        the take is stored `failed` with `Storage`, its unusable file is removed and its audio path cleared, so
 *        History shows the take without a Retry that could only fail, and retention's orphan sweep would remove the
 *        file if the delete here failed. Recovered takes keep their audio until the user retries or deletes them:
 *        retention never touches `recoverable` (02 §7.3 step 5). Toasts are counted copy only, never transcript
 *        text; they are shown once the windows exist, since a toast raised before the shell is up can be lost
 *        (05 W19).
 * WHERE: `recover` from app/bootstrap::start; `announce` from app::run on RunEvent::Ready. Retry (pipeline/retry.rs)
 *        then turns a recoverable take into text.
 */

use crate::{
    pipeline::capture::journal,
    ports::{EventSink, Notifier},
    services::{self, Db},
    types::{
        AppError, AppEvent, AppPaths, HistoryChangeReason, HistoryChanged, PortResult,
        RecoveryReport, StaticStr, Toast, ToastKind, TranscriptChange, TranscriptRef,
        TranscriptSelector, TranscriptStatus, samples_to_ms,
    },
};

/// How one unfinished take was settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Settled {
    /// The journal was repaired and holds audio: `recoverable`.
    Recovered,
    /// The journal holds no audio: `empty`.
    Silent,
    /// The journal is missing or unreadable: `failed`.
    Unreadable,
}

/// Settles every take left `recording`/`transcribing`; fails only when the unfinished takes cannot be listed.
pub fn recover(db: &Db, paths: &AppPaths) -> PortResult<RecoveryReport> {
    let unfinished = services::transcripts::list::select(
        db,
        &TranscriptSelector {
            statuses: Some(TranscriptStatus::matching(TranscriptStatus::is_unfinished)),
            ..TranscriptSelector::default()
        },
    )?;
    let mut report = RecoveryReport::default();
    for take in unfinished {
        let (settled, changes) = settle(paths, &take);
        match services::transcripts::update::update(db, take.id, &changes) {
            Ok(()) => count(&mut report, settled),
            Err(error) => tracing::error!(
                take = %take.id,
                code = error.error().code().as_str(),
                detail = error.detail(),
                "an unfinished take could not be settled; the next start tries again"
            ),
        }
    }
    if report.changed() {
        tracing::info!(
            recovered = report.recovered,
            empty = report.empty,
            unreadable = report.unreadable,
            "startup recovery settled unfinished takes"
        );
    }
    Ok(report)
}

/// Repairs the take's journal and returns the row writes that settle it.
fn settle(paths: &AppPaths, take: &TranscriptRef) -> (Settled, Vec<TranscriptChange>) {
    let path = paths.recording(take.id);
    if take.audio_path.is_none() || !path.is_file() {
        tracing::warn!(take = %take.id, "an unfinished take has no journal on disk");
        return unreadable(paths, take);
    }
    match journal::repair(&path) {
        Ok(0) => (
            Settled::Silent,
            vec![
                TranscriptChange::Status(TranscriptStatus::Empty),
                TranscriptChange::DurationMs(0),
                TranscriptChange::SpeechMs(0),
                TranscriptChange::WordCount(0),
                TranscriptChange::ErrorCode(None),
            ],
        ),
        Ok(samples) => (
            Settled::Recovered,
            vec![
                TranscriptChange::Status(TranscriptStatus::Recoverable),
                TranscriptChange::DurationMs(
                    u32::try_from(samples_to_ms(samples)).unwrap_or(u32::MAX),
                ),
                TranscriptChange::ErrorCode(None),
            ],
        ),
        Err(error) => {
            tracing::warn!(
                take = %take.id,
                detail = error.detail(),
                "an unfinished take's journal could not be repaired"
            );
            unreadable(paths, take)
        }
    }
}

/// A take whose audio cannot be retried: its file goes and the row says so.
fn unreadable(paths: &AppPaths, take: &TranscriptRef) -> (Settled, Vec<TranscriptChange>) {
    if let Err(error) = journal::remove(&paths.recording(take.id)) {
        tracing::warn!(
            take = %take.id,
            detail = error.detail(),
            "an unreadable journal could not be deleted; the retention sweep retries"
        );
    }
    (
        Settled::Unreadable,
        vec![
            TranscriptChange::Status(TranscriptStatus::Failed),
            TranscriptChange::ErrorCode(Some(AppError::Storage.code())),
            TranscriptChange::AudioPath(None),
        ],
    )
}

fn count(report: &mut RecoveryReport, settled: Settled) {
    let counter = match settled {
        Settled::Recovered => &mut report.recovered,
        Settled::Silent => &mut report.empty,
        Settled::Unreadable => &mut report.unreadable,
    };
    *counter = counter.saturating_add(1);
}

/**
 * SOURCE OF TRUTH KEYWORDS: recovery_toasts, Recovered 1 take, couldn't recover take, counted toast copy
 * WHAT:  The toasts a RecoveryReport raises: "Recovered N take(s)" for recoverable takes and "Couldn't recover N
 *        take(s)" for takes whose audio was unreadable; none for empty takes (nothing was said, 05 A4).
 * WHY:   01 "no silent failures": the user learns a crash cut a take off and where to get the text back. Built
 *        owned (StaticStr from a String) because the copy carries the count.
 * WHERE: `announce`; tests.
 */
pub fn recovery_toasts(report: RecoveryReport) -> Vec<Toast> {
    let mut toasts = Vec::new();
    if report.recovered > 0 {
        let (title, body) = counted(
            report.recovered,
            ("Recovered 1 take", "Recovered {n} takes"),
            (
                "Echo closed before it was transcribed. Retry it in History.",
                "Echo closed before they were transcribed. Retry them in History.",
            ),
        );
        toasts.push(Toast {
            kind: ToastKind::Info,
            title,
            body,
        });
    }
    if report.unreadable > 0 {
        let (title, body) = counted(
            report.unreadable,
            ("Couldn't recover 1 take", "Couldn't recover {n} takes"),
            (
                "Its audio was missing or damaged. It's marked failed in History.",
                "Their audio was missing or damaged. They're marked failed in History.",
            ),
        );
        toasts.push(Toast {
            kind: ToastKind::Warning,
            title,
            body,
        });
    }
    toasts
}

/// The singular or plural (title, body), with `{n}` in the plural title replaced by `n`.
fn counted(
    n: u32,
    (one_title, many_title): (&'static str, &'static str),
    (one_body, many_body): (&'static str, &'static str),
) -> (StaticStr, StaticStr) {
    if n == 1 {
        (StaticStr::new(one_title), StaticStr::new(one_body))
    } else {
        (
            StaticStr::from(many_title.replace("{n}", &n.to_string())),
            StaticStr::new(many_body),
        )
    }
}

/// Shows the report's toasts and tells the windows History changed; does nothing when recovery changed nothing.
pub fn announce(report: RecoveryReport, notifier: &dyn Notifier, events: &dyn EventSink<AppEvent>) {
    if !report.changed() {
        return;
    }
    for toast in recovery_toasts(report) {
        if let Err(error) = notifier.toast(&toast) {
            tracing::warn!(
                detail = error.detail(),
                "the recovery toast could not be shown"
            );
        }
    }
    events.emit(
        HistoryChanged {
            reason: HistoryChangeReason::Recovered,
        }
        .into(),
    );
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, OpenOptions},
        io::{Seek, SeekFrom, Write},
    };

    use super::*;
    use crate::{
        pipeline::capture::journal::Journal,
        ports::fakes::{FakeNotifier, RecordingSink},
        types::{AppErrorCode, EngineId, NewTranscript, TranscriptId, UnixMs, testing::TempDir},
    };

    struct Fixture {
        db: Db,
        paths: AppPaths,
        _dir: TempDir,
    }

    fn fixture() -> Fixture {
        let dir = TempDir::new("recovery");
        Fixture {
            db: Db::open_in_memory().unwrap(),
            paths: AppPaths::new(dir.join("data"), dir.join("resources")),
            _dir: dir,
        }
    }

    /// A take row in `status` whose journal holds `samples` samples; the journal is left as a killed process
    /// leaves it when `crashed` (header rewritten only at the last full second, the rest appended raw).
    fn take(
        fixture: &Fixture,
        status: TranscriptStatus,
        samples: usize,
        crashed: bool,
    ) -> TranscriptId {
        let id = TranscriptId::generate();
        services::transcripts::insert::insert(
            &fixture.db,
            &NewTranscript {
                id,
                created_at: UnixMs::now(),
                status: TranscriptStatus::Recording,
                audio_path: Some(AppPaths::recording_name(id)),
                engine_id: Some(EngineId::from_static("parakeet-tdt-0.6b-v3")),
                app_name: None,
            },
        )
        .unwrap();
        services::transcripts::update::update(&fixture.db, id, &[TranscriptChange::Status(status)])
            .unwrap();
        let path = fixture.paths.recording(id);
        if crashed {
            // Only a header counting nothing reached the disk, then raw samples: what a kill between rewrites leaves.
            Journal::create(&path).unwrap().finalize().unwrap();
            let mut file = OpenOptions::new().append(true).open(&path).unwrap();
            for _ in 0..samples {
                file.write_all(&1_000_i16.to_le_bytes()).unwrap();
            }
            // Half a sample torn off by the kill.
            file.write_all(&[9]).unwrap();
        } else {
            let mut journal = Journal::create(&path).unwrap();
            journal.append(&vec![1_000_i16; samples]).unwrap();
            journal.finalize().unwrap();
        }
        id
    }

    fn stored(fixture: &Fixture, id: TranscriptId) -> crate::types::Transcript {
        services::transcripts::get::get(&fixture.db, id).unwrap()
    }

    #[test]
    fn stuck_takes_with_a_truncated_wav_become_recoverable_with_their_real_duration() {
        let fixture = fixture();
        let recording = take(&fixture, TranscriptStatus::Recording, 24_000, true);
        let transcribing = take(&fixture, TranscriptStatus::Transcribing, 8_000, false);
        let done = take(&fixture, TranscriptStatus::Done, 16_000, false);

        let report = recover(&fixture.db, &fixture.paths).unwrap();
        assert_eq!(
            report,
            RecoveryReport {
                recovered: 2,
                empty: 0,
                unreadable: 0
            }
        );

        let row = stored(&fixture, recording);
        assert_eq!(row.status, TranscriptStatus::Recoverable);
        assert_eq!(row.duration_ms, Some(1_500));
        assert!(row.has_audio);
        assert_eq!(row.error_code, None);
        assert_eq!(
            journal::read(&fixture.paths.recording(recording))
                .unwrap()
                .len(),
            24_000,
            "every sample that reached the disk is counted, the torn one is dropped"
        );
        assert_eq!(
            stored(&fixture, transcribing).status,
            TranscriptStatus::Recoverable
        );
        assert_eq!(stored(&fixture, done).status, TranscriptStatus::Done);

        let again = recover(&fixture.db, &fixture.paths).unwrap();
        assert_eq!(
            again,
            RecoveryReport::default(),
            "recovered takes are settled"
        );
    }

    #[test]
    fn a_journal_without_audio_is_empty_and_a_missing_or_broken_one_is_failed() {
        let fixture = fixture();
        let silent = take(&fixture, TranscriptStatus::Recording, 0, true);
        let missing = take(&fixture, TranscriptStatus::Recording, 1_000, false);
        journal::remove(&fixture.paths.recording(missing)).unwrap();
        let broken = take(&fixture, TranscriptStatus::Transcribing, 1_000, false);
        fs::write(fixture.paths.recording(broken), b"not a journal").unwrap();

        let report = recover(&fixture.db, &fixture.paths).unwrap();
        assert_eq!(
            report,
            RecoveryReport {
                recovered: 0,
                empty: 1,
                unreadable: 2
            }
        );

        let empty = stored(&fixture, silent);
        assert_eq!(empty.status, TranscriptStatus::Empty);
        assert_eq!((empty.duration_ms, empty.word_count), (Some(0), Some(0)));
        for id in [missing, broken] {
            let row = stored(&fixture, id);
            assert_eq!(row.status, TranscriptStatus::Failed);
            assert_eq!(row.error_code, Some(AppErrorCode::Storage));
            assert!(!row.has_audio, "no Retry for audio that cannot be read");
            assert!(!fixture.paths.recording(id).exists());
        }
    }

    #[test]
    fn a_header_that_was_zeroed_is_repaired_in_place() {
        let fixture = fixture();
        let id = take(&fixture, TranscriptStatus::Recording, 16_000, false);
        let mut file = OpenOptions::new()
            .write(true)
            .open(fixture.paths.recording(id))
            .unwrap();
        for offset in [4, 40] {
            file.seek(SeekFrom::Start(offset)).unwrap();
            file.write_all(&0_u32.to_le_bytes()).unwrap();
        }
        drop(file);
        recover(&fixture.db, &fixture.paths).unwrap();
        let row = stored(&fixture, id);
        assert_eq!(row.status, TranscriptStatus::Recoverable);
        assert_eq!(row.duration_ms, Some(1_000));
    }

    #[test]
    fn toasts_count_takes_in_calm_copy() {
        assert!(recovery_toasts(RecoveryReport::default()).is_empty());
        assert!(
            recovery_toasts(RecoveryReport {
                empty: 3,
                ..RecoveryReport::default()
            })
            .is_empty(),
            "nothing was said in an empty take"
        );
        let one = recovery_toasts(RecoveryReport {
            recovered: 1,
            ..RecoveryReport::default()
        });
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].title.as_str(), "Recovered 1 take");
        assert_eq!(one[0].kind, ToastKind::Info);

        let many = recovery_toasts(RecoveryReport {
            recovered: 3,
            empty: 0,
            unreadable: 2,
        });
        assert_eq!(many[0].title.as_str(), "Recovered 3 takes");
        assert!(many[0].body.contains("them"));
        assert_eq!(many[1].title.as_str(), "Couldn't recover 2 takes");
        assert_eq!(many[1].kind, ToastKind::Warning);
    }

    #[test]
    fn announce_toasts_and_refreshes_history_only_when_something_changed() {
        let notifier = FakeNotifier::default();
        let events = RecordingSink::<AppEvent>::default();
        announce(RecoveryReport::default(), &notifier, &events);
        assert!(notifier.toasts().is_empty());
        assert!(events.events().is_empty());

        announce(
            RecoveryReport {
                recovered: 1,
                ..RecoveryReport::default()
            },
            &notifier,
            &events,
        );
        assert_eq!(notifier.toasts()[0].title.as_str(), "Recovered 1 take");
        assert_eq!(
            events.events(),
            [AppEvent::from(HistoryChanged {
                reason: HistoryChangeReason::Recovered
            })]
        );
    }
}
