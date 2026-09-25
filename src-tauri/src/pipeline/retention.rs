/*!
 * SOURCE OF TRUTH KEYWORDS: retention sweep, audio retention, history retention, delete old recordings, orphan journals, release_after_success, RetentionSweeper, RetentionHandle, sweep_soon, daily sweep
 * WHAT:  02 §7.3 step 5. `sweep`: deletes the rows (and any WAV left) of successful takes older than
 *        `storage.history_retention_days`, the WAVs of successful takes older than `storage.audio_retention_days`,
 *        and journals no row points to; returns a RetentionReport. `release_after_success`: deletes a take's WAV
 *        the moment it succeeds when the audio setting is 0. RetentionSweeper runs `sweep` at startup, then daily
 *        and whenever a RetentionHandle asks (`sweep_soon`, e.g. after a storage setting changed), and announces
 *        what went (HistoryChanged { retention }, MetricsChanged).
 * WHY:   Only `done` and `empty` takes are ever swept (TranscriptStatus::is_swept_by_retention): failed and
 *        recoverable takes keep their audio for a retry and their row for the user, and an unfinished take is the
 *        session's (or recovery's), so the live take can never lose its journal. Each take goes on its own, WAV
 *        first and then the row (or its audio path), so a WAV that cannot be deleted leaves the row pointing at it
 *        and the next sweep tries again; nothing is orphaned unseen. Journals no row points to (a discard whose
 *        delete failed, a crash between the two writes) would otherwise fill the disk forever, so they are
 *        removed too, but only once they are older than ORPHAN_GRACE and only after the rows were read *after*
 *        the folder was listed: a take's row is inserted before its journal is created (02 §7.3 step 1), so any
 *        journal in the listing already has its row. Files whose names are not a take's journal are never
 *        touched. Metrics are SQL aggregates over the rows that remain (02 §7.4), so a row sweep changes the
 *        dashboard. The sweeper sleeps between sweeps (no timer while idle besides that one wait, 02 §6.2),
 *        does its file and database work on the blocking pool (a panic there is logged, and the next sweep
 *        still runs) and ends when every handle is gone.
 * WHERE: RetentionSweeper spawned by app/bootstrap after startup recovery; RetentionHandle in CommandCtx
 *        (settings_set / settings_reset re-sweep when `policy_changed`); `release_after_success` from the session
 *        runner when a take settles and from pipeline/retry.rs.
 */

use std::{collections::HashSet, fs, io, path::PathBuf, sync::Arc, time::Duration};

use tokio::sync::mpsc;

use crate::{
    pipeline::{blocking::run_blocking, capture::journal},
    ports::EventSink,
    registry,
    services::{self, Db},
    types::{
        AppError, AppEvent, AppPaths, HistoryChangeReason, HistoryChanged, MetricsChanged,
        PortResult, ResourceKind, RetentionPolicy, RetentionReport, SettingsSnapshot,
        SharedSettings, TranscriptChange, TranscriptId, TranscriptRef, TranscriptSelector,
        TranscriptStatus, UnixMs,
    },
};

/// Time between scheduled sweeps.
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// A journal no row points to is removed only once it is at least this old (1 hour).
pub const ORPHAN_GRACE_MS: i64 = 60 * 60 * 1000;

/// The statuses a sweep may delete, as a selector list.
fn swept_statuses() -> Option<Vec<TranscriptStatus>> {
    Some(TranscriptStatus::matching(
        TranscriptStatus::is_swept_by_retention,
    ))
}

/**
 * SOURCE OF TRUTH KEYWORDS: sweep, retention rules, history cut-off, audio cut-off, orphan sweep
 * WHAT:  Applies `policy` at `now`: history rows first (their WAVs go with them), then the audio of the successful
 *        takes that remain, then orphaned journals. Never fails: a step that cannot read its rows is logged and
 *        the other steps still run.
 * WHY:   Rows first, so audio the history sweep already removed is not visited twice. See the file header for
 *        which takes are eligible and why each is handled on its own.
 * WHERE: RetentionSweeper; tests.
 */
pub fn sweep(db: &Db, paths: &AppPaths, policy: RetentionPolicy, now: UnixMs) -> RetentionReport {
    let mut report = RetentionReport::default();
    if let Some(cutoff) = policy.history_cutoff(now) {
        let old = selected(
            db,
            &TranscriptSelector {
                statuses: swept_statuses(),
                created_before: Some(cutoff),
                has_audio: None,
            },
            "history",
        );
        report.rows_removed = count(old.iter().filter(|take| remove_take(db, paths, take)));
    }
    let with_audio = selected(
        db,
        &TranscriptSelector {
            statuses: swept_statuses(),
            created_before: Some(policy.audio_cutoff(now)),
            has_audio: Some(true),
        },
        "audio",
    );
    report.audio_removed = count(
        with_audio
            .iter()
            .filter(|take| remove_audio(db, paths, take.id)),
    );
    report.orphans_removed = remove_orphans(db, paths, now);
    report
}

/**
 * SOURCE OF TRUTH KEYWORDS: release_after_success, audio retention zero, delete WAV after success
 * WHAT:  When `policy` deletes audio on success (`storage.audio_retention_days` = 0) and take `id` is stored as
 *        a success (`done` or `empty`) that still has audio, deletes its WAV and clears its audio path; true when
 *        it did.
 * WHY:   "0 = delete right after success" (02 §3.3) must not wait for the daily sweep. Reading the stored status
 *        (not the session's view of it) applies the same eligibility rule as the sweep, so a failed take keeps
 *        its audio whatever the setting says.
 * WHERE: The session runner when a take settles (before it announces the row); pipeline/retry.rs after a retry
 *        succeeded.
 */
pub fn release_after_success(
    db: &Db,
    paths: &AppPaths,
    policy: RetentionPolicy,
    id: TranscriptId,
) -> PortResult<bool> {
    if !policy.deletes_audio_on_success() {
        return Ok(false);
    }
    let take = services::transcripts::get::get(db, id)?;
    if !take.status.is_swept_by_retention() || !take.has_audio {
        return Ok(false);
    }
    journal::remove(&paths.recording(id))?;
    services::transcripts::update::update(db, id, &[TranscriptChange::AudioPath(None)])?;
    Ok(true)
}

/// A storage setting changed the policy, so a sweep should apply it now rather than tomorrow.
pub fn policy_changed(before: &SettingsSnapshot, after: &SettingsSnapshot) -> bool {
    registry::settings::retention_policy(before) != registry::settings::retention_policy(after)
}

/// HistoryChanged { retention } when rows changed, MetricsChanged when rows went.
pub fn announce(report: RetentionReport, events: &dyn EventSink<AppEvent>) {
    if report.changed_history() {
        events.emit(
            HistoryChanged {
                reason: HistoryChangeReason::Retention,
            }
            .into(),
        );
    }
    if report.changed_metrics() {
        events.emit(MetricsChanged {}.into());
    }
}

/// The takes `selector` matches; an unreadable selection is logged and treated as none.
fn selected(db: &Db, selector: &TranscriptSelector, step: &'static str) -> Vec<TranscriptRef> {
    services::transcripts::list::select(db, selector).unwrap_or_else(|error| {
        tracing::error!(
            step,
            code = error.error().code().as_str(),
            detail = error.detail(),
            "the retention sweep could not read its takes"
        );
        Vec::new()
    })
}

/// A whole take: WAV first, then the row. True when the row went.
fn remove_take(db: &Db, paths: &AppPaths, take: &TranscriptRef) -> bool {
    if take.audio_path.is_some() && !remove_file(paths, take.id) {
        return false;
    }
    match services::transcripts::delete::delete(db, take.id) {
        Ok(()) => true,
        // Deleted meanwhile (History): nothing left to do, and nothing this sweep removed.
        Err(error) if is_missing_row(error.error()) => false,
        Err(error) => {
            tracing::warn!(take = %take.id, detail = error.detail(), "an old take's row could not be deleted");
            false
        }
    }
}

/// A take's audio: WAV first, then the audio path. True when the audio went.
fn remove_audio(db: &Db, paths: &AppPaths, id: TranscriptId) -> bool {
    if !remove_file(paths, id) {
        return false;
    }
    match services::transcripts::update::update(db, id, &[TranscriptChange::AudioPath(None)]) {
        Ok(()) => true,
        Err(error) if is_missing_row(error.error()) => false,
        Err(error) => {
            tracing::warn!(take = %id, detail = error.detail(), "an old take's audio path could not be cleared");
            false
        }
    }
}

fn remove_file(paths: &AppPaths, id: TranscriptId) -> bool {
    match journal::remove(&paths.recording(id)) {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(take = %id, detail = error.detail(), "an old take's audio could not be deleted; the next sweep retries");
            false
        }
    }
}

fn is_missing_row(error: &AppError) -> bool {
    matches!(
        error,
        AppError::NotFound {
            resource: ResourceKind::Transcript
        }
    )
}

/**
 * SOURCE OF TRUTH KEYWORDS: orphan journals, stray WAV files, recordings folder cleanup, list then select
 * WHAT:  Deletes `recordings/<id>.wav` files older than ORPHAN_GRACE_MS whose take has no row with audio; returns
 *        how many went.
 * WHY:   The folder is listed before the rows are read, so a journal created after its row (always the order)
 *        is never mistaken for an orphan. Anything that is not named like a take's journal is left alone.
 * WHERE: `sweep`.
 */
fn remove_orphans(db: &Db, paths: &AppPaths, now: UnixMs) -> u32 {
    let dir = paths.recordings_dir();
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return 0,
        Err(error) => {
            tracing::warn!(%error, "the recordings folder could not be listed");
            return 0;
        }
    };
    let oldest_kept = UnixMs::from_millis(now.as_millis().saturating_sub(ORPHAN_GRACE_MS));
    let candidates: Vec<(TranscriptId, PathBuf)> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let id = journal_id(entry.file_name().to_str()?)?;
            (id.created_at() < oldest_kept).then(|| (id, entry.path()))
        })
        .collect();
    if candidates.is_empty() {
        return 0;
    }
    let Ok(with_audio) = services::transcripts::list::select(
        db,
        &TranscriptSelector {
            has_audio: Some(true),
            ..TranscriptSelector::default()
        },
    ) else {
        tracing::warn!("the takes with audio could not be read; orphaned journals are kept");
        return 0;
    };
    let kept: HashSet<TranscriptId> = with_audio.into_iter().map(|take| take.id).collect();
    count(
        candidates
            .iter()
            .filter(|(id, _)| !kept.contains(id))
            .filter(|(id, path)| match journal::remove(path) {
                Ok(()) => {
                    tracing::info!(take = %id, "removed a journal no take points to");
                    true
                }
                Err(error) => {
                    tracing::warn!(take = %id, detail = error.detail(), "an orphaned journal could not be deleted");
                    false
                }
            }),
    )
}

/// The take a file name is the journal of, when it is exactly `AppPaths::recording_name(id)`.
fn journal_id(name: &str) -> Option<TranscriptId> {
    let id: TranscriptId = name.strip_suffix(".wav")?.parse().ok()?;
    (AppPaths::recording_name(id) == name).then_some(id)
}

fn count<T>(items: impl Iterator<Item = T>) -> u32 {
    u32::try_from(items.count()).unwrap_or(u32::MAX)
}

/// What the sweeper works through; the same instances the rest of the app uses.
#[derive(Clone)]
pub struct RetentionDeps {
    /// Read at every sweep, so a changed setting applies to the next one.
    pub settings: SharedSettings,
    pub paths: AppPaths,
    pub db: Db,
    /// HistoryChanged and MetricsChanged after a sweep that removed something.
    pub events: Arc<dyn EventSink<AppEvent>>,
}

/// Asks the sweeper for a sweep now; clones share it. The sweeper stops once every handle is gone.
#[derive(Clone)]
pub struct RetentionHandle {
    wake: mpsc::UnboundedSender<()>,
}

impl RetentionHandle {
    /// A handle and the sweeper it wakes (spawn `RetentionSweeper::run` to start sweeping).
    pub fn new(deps: RetentionDeps) -> (Self, RetentionSweeper) {
        let (wake, woken) = mpsc::unbounded_channel();
        (
            Self { wake },
            RetentionSweeper {
                deps,
                woken,
                interval: SWEEP_INTERVAL,
            },
        )
    }

    /// Sweeps as soon as the sweeper is free; requests that arrive during a sweep collapse into one more.
    pub fn sweep_soon(&self) {
        // Without a running sweeper (tests, shutdown) there is nothing to wake.
        let _ = self.wake.send(());
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: RetentionSweeper, startup sweep, daily sweep loop, sweep on request
 * WHAT:  The task that sweeps once when it starts, then after every SWEEP_INTERVAL or `sweep_soon`, whichever comes
 *        first; ends when every RetentionHandle is dropped.
 * WHY:   A future the composition root spawns (like the session actor), so tests run it on their own runtime.
 * WHERE: Spawned by app/bootstrap on Tauri's runtime.
 */
pub struct RetentionSweeper {
    deps: RetentionDeps,
    woken: mpsc::UnboundedReceiver<()>,
    interval: Duration,
}

impl RetentionSweeper {
    /// Sweeps now, then on every interval or request until the handles are gone.
    pub async fn run(mut self) {
        loop {
            self.sweep_now().await;
            match tokio::time::timeout(self.interval, self.woken.recv()).await {
                Ok(None) => break,
                Ok(Some(())) | Err(_) => while self.woken.try_recv().is_ok() {},
            }
        }
        tracing::info!("the retention sweeper stopped");
    }

    /// Test hook: whether a sweep was requested since the last check (the requests are consumed).
    #[cfg(test)]
    pub fn take_requests(&mut self) -> bool {
        let mut requested = false;
        while self.woken.try_recv().is_ok() {
            requested = true;
        }
        requested
    }

    /// One sweep under the settings in effect now, announced.
    async fn sweep_now(&self) {
        let deps = self.deps.clone();
        let policy = registry::settings::retention_policy(&deps.settings.current());
        let swept = run_blocking("the retention sweep", move || {
            Ok(sweep(&deps.db, &deps.paths, policy, UnixMs::now()))
        })
        .await;
        match swept {
            Ok(report) => {
                if report != RetentionReport::default() {
                    tracing::info!(
                        audio_removed = report.audio_removed,
                        rows_removed = report.rows_removed,
                        orphans_removed = report.orphans_removed,
                        "retention sweep finished"
                    );
                }
                announce(report, self.deps.events.as_ref());
            }
            Err(error) => tracing::error!(
                detail = error.detail(),
                "the retention sweep failed; the next one tries again"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use ulid::Ulid;

    use super::*;
    use crate::{
        pipeline::capture::journal::Journal,
        ports::fakes::RecordingSink,
        registry::settings::{defaults, keys, resolve},
        types::{AppErrorCode, EngineId, NewTranscript, SettingValue, testing::TempDir},
    };

    const DAY: i64 = 86_400_000;
    /// "Now" in every test: day 1000.
    const NOW: UnixMs = UnixMs::from_millis(1_000 * DAY);

    struct Fixture {
        db: Db,
        paths: AppPaths,
        _dir: TempDir,
    }

    fn fixture() -> Fixture {
        let dir = TempDir::new("retention");
        Fixture {
            db: Db::open_in_memory().unwrap(),
            paths: AppPaths::new(dir.join("data"), dir.join("resources")),
            _dir: dir,
        }
    }

    /// A take created `days_ago` days before NOW, stored in `status`, with a journal on disk.
    fn take(fixture: &Fixture, days_ago: i64, status: TranscriptStatus) -> TranscriptId {
        let created = NOW.as_millis() - days_ago * DAY;
        let id: TranscriptId =
            Ulid::from_parts(u64::try_from(created).unwrap(), Ulid::generate().random())
                .to_string()
                .parse()
                .unwrap();
        services::transcripts::insert::insert(
            &fixture.db,
            &NewTranscript {
                id,
                created_at: UnixMs::from_millis(created),
                status: TranscriptStatus::Recording,
                audio_path: Some(AppPaths::recording_name(id)),
                engine_id: Some(EngineId::from_static("parakeet-tdt-0.6b-v3")),
                app_name: None,
            },
        )
        .unwrap();
        let mut changes = vec![TranscriptChange::Status(status)];
        if status == TranscriptStatus::Failed {
            changes.push(TranscriptChange::ErrorCode(Some(AppErrorCode::Asr)));
        }
        services::transcripts::update::update(&fixture.db, id, &changes).unwrap();
        let mut journal = Journal::create(&fixture.paths.recording(id)).unwrap();
        journal.append(&[1, 2, 3]).unwrap();
        journal.finalize().unwrap();
        id
    }

    fn policy(audio_days: u32, history_days: u32) -> RetentionPolicy {
        RetentionPolicy {
            audio_days,
            history_days,
        }
    }

    fn row(fixture: &Fixture, id: TranscriptId) -> Option<crate::types::Transcript> {
        services::transcripts::get::get(&fixture.db, id).ok()
    }

    fn wav(fixture: &Fixture, id: TranscriptId) -> bool {
        fixture.paths.recording(id).exists()
    }

    #[test]
    fn audio_older_than_the_setting_goes_but_the_row_stays() {
        let fixture = fixture();
        let old_done = take(&fixture, 8, TranscriptStatus::Done);
        let old_empty = take(&fixture, 8, TranscriptStatus::Empty);
        let fresh = take(&fixture, 6, TranscriptStatus::Done);

        let report = sweep(&fixture.db, &fixture.paths, policy(7, 0), NOW);
        assert_eq!(report.audio_removed, 2);
        assert_eq!(report.rows_removed, 0);
        for id in [old_done, old_empty] {
            assert!(!wav(&fixture, id));
            let stored = row(&fixture, id).unwrap();
            assert!(!stored.has_audio);
        }
        assert!(wav(&fixture, fresh));
        assert!(row(&fixture, fresh).unwrap().has_audio);
    }

    #[test]
    fn failed_recoverable_and_unfinished_takes_are_never_touched() {
        let fixture = fixture();
        let protected = [
            take(&fixture, 400, TranscriptStatus::Failed),
            take(&fixture, 400, TranscriptStatus::Recoverable),
            take(&fixture, 400, TranscriptStatus::Recording),
            take(&fixture, 400, TranscriptStatus::Transcribing),
        ];
        let report = sweep(&fixture.db, &fixture.paths, policy(0, 1), NOW);
        assert_eq!(report, RetentionReport::default());
        for id in protected {
            assert!(wav(&fixture, id), "{id}");
            assert!(row(&fixture, id).unwrap().has_audio, "{id}");
        }
    }

    #[test]
    fn zero_audio_days_sweeps_every_successful_take_and_zero_history_days_keeps_rows_forever() {
        let fixture = fixture();
        let today = take(&fixture, 0, TranscriptStatus::Done);
        let ancient = take(&fixture, 900, TranscriptStatus::Done);
        // A moment after the newest take succeeded (a cut-off is "created strictly before").
        let later = UnixMs::from_millis(NOW.as_millis() + 1);
        let report = sweep(&fixture.db, &fixture.paths, policy(0, 0), later);
        assert_eq!(report.audio_removed, 2);
        assert_eq!(report.rows_removed, 0, "history 0 keeps rows forever");
        assert!(!wav(&fixture, today) && !wav(&fixture, ancient));
        assert!(row(&fixture, ancient).is_some());
    }

    #[test]
    fn history_older_than_the_setting_loses_its_rows_and_their_audio() {
        let fixture = fixture();
        let old = take(&fixture, 31, TranscriptStatus::Done);
        let old_empty = take(&fixture, 45, TranscriptStatus::Empty);
        let old_failed = take(&fixture, 45, TranscriptStatus::Failed);
        let recent = take(&fixture, 29, TranscriptStatus::Done);

        let report = sweep(&fixture.db, &fixture.paths, policy(365, 30), NOW);
        assert_eq!(report.rows_removed, 2);
        assert_eq!(report.audio_removed, 0);
        for id in [old, old_empty] {
            assert!(row(&fixture, id).is_none());
            assert!(!wav(&fixture, id));
        }
        assert!(row(&fixture, old_failed).is_some() && wav(&fixture, old_failed));
        assert!(row(&fixture, recent).is_some() && wav(&fixture, recent));
    }

    #[test]
    fn orphaned_journals_go_once_they_are_old_and_nothing_else_does() {
        let fixture = fixture();
        let owned = take(&fixture, 1, TranscriptStatus::Failed);
        let orphan = take(&fixture, 1, TranscriptStatus::Done);
        services::transcripts::delete::delete(&fixture.db, orphan).unwrap();
        // A row that lost its path while its file stayed is an orphan too.
        let unreferenced = take(&fixture, 1, TranscriptStatus::Failed);
        services::transcripts::update::update(
            &fixture.db,
            unreferenced,
            &[TranscriptChange::AudioPath(None)],
        )
        .unwrap();
        // Minted a minute before NOW: inside the grace period, so it may still be getting its row.
        let young: TranscriptId =
            Ulid::from_parts(u64::try_from(NOW.as_millis() - 60_000).unwrap(), 1)
                .to_string()
                .parse()
                .unwrap();
        Journal::create(&fixture.paths.recording(young))
            .unwrap()
            .finalize()
            .unwrap();
        let foreign = fixture.paths.recordings_dir().join("notes.wav");
        fs::write(&foreign, b"mine").unwrap();

        let report = sweep(&fixture.db, &fixture.paths, policy(365, 0), NOW);
        assert_eq!(report.orphans_removed, 2);
        assert!(!report.changed_history(), "orphans have no row to refresh");
        assert!(!wav(&fixture, orphan) && !wav(&fixture, unreferenced));
        assert!(wav(&fixture, owned));
        assert!(wav(&fixture, young));
        assert!(foreign.exists());
    }

    #[test]
    fn a_missing_recordings_folder_is_nothing_to_sweep() {
        let fixture = fixture();
        assert_eq!(
            sweep(&fixture.db, &fixture.paths, policy(0, 1), NOW),
            RetentionReport::default()
        );
    }

    #[test]
    fn release_after_success_deletes_only_a_successful_take_s_audio_and_only_at_zero_days() {
        let fixture = fixture();
        let done = take(&fixture, 0, TranscriptStatus::Done);
        let failed = take(&fixture, 0, TranscriptStatus::Failed);
        let paths = &fixture.paths;
        assert!(!release_after_success(&fixture.db, paths, policy(7, 30), done).unwrap());
        assert!(wav(&fixture, done));

        assert!(!release_after_success(&fixture.db, paths, policy(0, 30), failed).unwrap());
        assert!(wav(&fixture, failed));

        assert!(release_after_success(&fixture.db, paths, policy(0, 30), done).unwrap());
        assert!(!wav(&fixture, done));
        assert!(!row(&fixture, done).unwrap().has_audio);
        assert!(
            !release_after_success(&fixture.db, paths, policy(0, 30), done).unwrap(),
            "nothing left to release"
        );
        assert_eq!(
            release_after_success(&fixture.db, paths, policy(0, 30), TranscriptId::generate())
                .unwrap_err()
                .error(),
            &AppError::NotFound {
                resource: ResourceKind::Transcript
            }
        );
    }

    #[test]
    fn only_a_storage_change_asks_for_a_sweep() {
        let before = defaults();
        let audio = resolve([(keys::AUDIO_RETENTION_DAYS, SettingValue::Int(0))]);
        let unrelated = resolve([(keys::TRAILING_SPACE, SettingValue::Bool(false))]);
        assert!(policy_changed(&before, &audio));
        assert!(!policy_changed(&before, &unrelated));
    }

    #[test]
    fn announce_names_what_changed() {
        let events = RecordingSink::<AppEvent>::default();
        announce(
            RetentionReport {
                orphans_removed: 4,
                ..RetentionReport::default()
            },
            &events,
        );
        assert!(events.events().is_empty());
        announce(
            RetentionReport {
                rows_removed: 1,
                ..RetentionReport::default()
            },
            &events,
        );
        assert_eq!(
            events.events(),
            [
                AppEvent::from(HistoryChanged {
                    reason: HistoryChangeReason::Retention
                }),
                AppEvent::from(MetricsChanged {})
            ]
        );
    }

    #[test]
    fn the_sweeper_sweeps_at_start_and_on_request_then_stops_with_its_handles() {
        let fixture = fixture();
        let settings = SharedSettings::new(resolve([
            (keys::AUDIO_RETENTION_DAYS, SettingValue::Int(0)),
            (keys::HISTORY_RETENTION_DAYS, SettingValue::Int(0)),
        ]));
        let events = Arc::new(RecordingSink::<AppEvent>::default());
        let (handle, sweeper) = RetentionHandle::new(RetentionDeps {
            settings,
            paths: fixture.paths.clone(),
            db: fixture.db.clone(),
            events: Arc::clone(&events) as _,
        });
        let first = take(&fixture, 1, TranscriptStatus::Done);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let task = runtime.spawn(sweeper.run());
        runtime.block_on(async {
            for _ in 0..500 {
                if !wav(&fixture, first) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
        assert!(!wav(&fixture, first), "the startup sweep ran");

        let second = take(&fixture, 1, TranscriptStatus::Empty);
        handle.sweep_soon();
        runtime.block_on(async {
            for _ in 0..500 {
                if !wav(&fixture, second) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
        assert!(!wav(&fixture, second), "a request sweeps again");
        assert!(events.events().contains(&AppEvent::from(HistoryChanged {
            reason: HistoryChangeReason::Retention
        })));

        drop(handle);
        runtime
            .block_on(async { tokio::time::timeout(Duration::from_secs(5), task).await })
            .unwrap()
            .unwrap();
    }
}
