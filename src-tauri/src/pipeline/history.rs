/*!
 * SOURCE OF TRUTH KEYWORDS: history pipeline, delete take, clear history, copy take, paste last, take text, announce saved, announce deleted, announce cleared, live take guard
 * WHAT:  What History does with stored takes beyond reading them: `delete_take` (WAV, then row), `clear_history`
 *        (every listed take leaves History, its text and audio erased, its measurements kept), `copy_take` (its
 *        text to the clipboard), `paste_last` (the newest completed take's text into the focused app),
 *        `take_text` (the text a take offers), `ensure_not_live` (refuse the take the session still owns) and the
 *        events that tell every window (`announce_saved`, `announce_deleted`, `announce_cleared`).
 * WHY:   Business rules on files and rows live in the pipeline, never in services (one verb, one table) or
 *        commands (thin), so the History commands, the session actor and later the tray or a hotkey share them.
 *        Delete removes the WAV before the row: if the WAV cannot be removed the row stays, so History still shows
 *        the take and the user can try again instead of an orphaned file lingering unseen (00 constraint 5 cuts
 *        both ways). Clearing History must not reset the dashboard or the streak, which are aggregates over the
 *        rows (02 §7.4), so it erases a row's content and marks it `cleared_at` instead of deleting it. Copy offers the polished text, else the raw text a failed delivery left (02 §7.3: the raw
 *        text is stored before delivery), and refuses a take with neither (`NotFound { transcript_text }`).
 *        Paste-last pastes exactly what a take would have pasted (the stored text plus the trailing space the
 *        chain adds), into the focused window unless that window is Echo's own (then it copies, like a take with
 *        no target). Clipboard work blocks for up to the adapter's retries (05 W4), so callers run these on the
 *        blocking pool. Transcript text is never logged (02 §10).
 * WHERE: ipc/commands/history.rs (history_get, history_copy, history_delete, history_clear); the session runner (paste_last,
 *        announce_saved after a take settles); pipeline/retry.rs (announce_saved).
 */

use crate::{
    pipeline::{capture::journal, delivery::Delivery},
    ports::{EventSink, ForegroundApp},
    registry, services,
    services::Db,
    types::{
        AppError, AppEvent, AppPaths, AppTarget, DeliveryReport, HistoryChangeReason,
        HistoryChanged, MetricsChanged, PortError, PortResult, ResourceKind, SessionView,
        SettingsSnapshot, Transcript, TranscriptChange, TranscriptId, TranscriptSaved,
        TranscriptSelector, TranscriptStatus, UnixMs,
    },
};

/// The text a stored take offers: its polished text, else the raw text of a take whose delivery failed.
pub fn take_text(take: &Transcript) -> Option<&str> {
    [take.final_text.as_deref(), take.raw_text.as_deref()]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|text| !text.is_empty())
}

/// `Busy` when `id` is the take the session is still recording, transcribing or delivering.
pub fn ensure_not_live(session: &SessionView, id: TranscriptId) -> PortResult<()> {
    if session.is_live(id) {
        return Err(PortError::new(AppError::Busy)
            .with_detail("the take is still in progress in the session"));
    }
    Ok(())
}

/// Deletes a stored take: its journal (if any), then its row.
pub fn delete_take(db: &Db, paths: &AppPaths, id: TranscriptId) -> PortResult<()> {
    // Existence first, so a missing take is NotFound rather than a silent no-op on the file.
    services::transcripts::get::get(db, id)?;
    journal::remove(&paths.recording(id))?;
    services::transcripts::delete::delete(db, id)
}

/**
 * SOURCE OF TRUTH KEYWORDS: clear_history, clear all takes, erase text and audio, keep metrics, keep streak, cleared_at
 * WHAT:  Erases every take History lists (settled ones, except `live`, the take the session still owns): WAV
 *        first, then one transaction blanks the text, audio path and app and stamps `cleared_at = now`, which
 *        hides the row from History. Returns how many takes were cleared.
 * WHY:   The rows stay because the dashboard and streak are computed from them; only what the user said and
 *        where it went is erased (status, time, counts, durations and latency are what the aggregates read). WAV
 *        before row, as in `delete_take`: a take whose WAV cannot be removed stays listed and untouched, the rest
 *        are still cleared, and the first failure is returned so the user hears about it and can clear again.
 * WHERE: history_clear (ipc/commands/history.rs), on the blocking pool.
 */
pub fn clear_history(
    db: &Db,
    paths: &AppPaths,
    live: Option<TranscriptId>,
    now: UnixMs,
) -> PortResult<usize> {
    let listed = services::transcripts::list::select(
        db,
        &TranscriptSelector {
            statuses: Some(TranscriptStatus::matching(
                TranscriptStatus::is_cleared_with_history,
            )),
            cleared: Some(false),
            ..TranscriptSelector::default()
        },
    )?;
    let mut failure = None;
    let mut erased = Vec::with_capacity(listed.len());
    for take in listed.iter().filter(|take| Some(take.id) != live) {
        if take.audio_path.is_some()
            && let Err(error) = journal::remove(&paths.recording(take.id))
        {
            tracing::warn!(take = %take.id, detail = error.detail(), "a take's audio could not be deleted; it stays in History");
            failure.get_or_insert(error);
            continue;
        }
        erased.push(take.id);
    }
    let cleared = services::transcripts::update::update_each(
        db,
        &erased,
        &[
            TranscriptChange::RawText(None),
            TranscriptChange::FinalText(None),
            TranscriptChange::AudioPath(None),
            TranscriptChange::AppName(None),
            TranscriptChange::ClearedAt(now),
        ],
    )?;
    failure.map_or(Ok(cleared), Err)
}

/// Puts a stored take's text on the clipboard (excluded from Windows clipboard history, 05 W5).
pub fn copy_take(db: &Db, delivery: &Delivery, id: TranscriptId) -> PortResult<()> {
    let take = services::transcripts::get::get(db, id)?;
    let text = take_text(&take).ok_or_else(no_text)?;
    delivery.copy(text)
}

/**
 * SOURCE OF TRUTH KEYWORDS: paste last transcript, paste-last, newest done take, re-paste, own window target
 * WHAT:  Delivers the newest `done` take's text to the focused window under the delivery settings in effect
 *        (paste, or copy with the usual reason and toast); `NotFound { transcript_text }` when no take is done.
 * WHY:   The same Delivery a take uses, so elevated targets, auto-paste off and keep-on-clipboard behave exactly
 *        as they do after dictation. A paste into Echo's own window would type into Echo (e.g. its search field),
 *        so Echo's own window counts as no target. The caller schedules the returned clipboard restore (05 W6).
 * WHERE: The session runner (history_paste_last through SessionHandle::paste_last; the paste-last hotkey).
 */
pub fn paste_last(
    db: &Db,
    delivery: &Delivery,
    foreground: &dyn ForegroundApp,
    settings: &SettingsSnapshot,
) -> PortResult<DeliveryReport> {
    let take =
        services::transcripts::get::latest(db, TranscriptStatus::Done)?.ok_or_else(no_text)?;
    let text = take_text(&take).ok_or_else(no_text)?;
    let pasted = if registry::settings::trailing_space(settings) {
        format!("{text} ")
    } else {
        text.to_owned()
    };
    let target = foreground
        .current()
        .unwrap_or_else(|error| {
            tracing::warn!(
                detail = error.detail(),
                "the focused window could not be read; the text will be copied"
            );
            None
        })
        .filter(|target| !is_own_window(target));
    delivery.deliver(
        &pasted,
        target.as_ref(),
        registry::settings::delivery_policy(settings),
    )
}

/// The window belongs to Echo itself.
fn is_own_window(target: &AppTarget) -> bool {
    target.process_id == std::process::id()
}

/// TranscriptSaved (the take's list row), HistoryChanged and MetricsChanged for a take whose row changed.
pub fn announce_saved(db: &Db, events: &dyn EventSink<AppEvent>, id: TranscriptId) {
    match services::transcripts::get::summary(db, id) {
        Ok(summary) => events.emit(TranscriptSaved(summary).into()),
        Err(error) => {
            tracing::error!(take = %id, detail = error.detail(), "the saved take could not be read back");
        }
    }
    events.emit(
        HistoryChanged {
            reason: HistoryChangeReason::Updated,
        }
        .into(),
    );
    events.emit(MetricsChanged {}.into());
}

/// HistoryChanged and MetricsChanged for a take the user deleted.
pub fn announce_deleted(events: &dyn EventSink<AppEvent>) {
    events.emit(
        HistoryChanged {
            reason: HistoryChangeReason::Deleted,
        }
        .into(),
    );
    events.emit(MetricsChanged {}.into());
}

/// HistoryChanged for a cleared History; the metrics did not change, so no MetricsChanged.
pub fn announce_cleared(events: &dyn EventSink<AppEvent>) {
    events.emit(
        HistoryChanged {
            reason: HistoryChangeReason::Cleared,
        }
        .into(),
    );
}

fn no_text() -> PortError {
    PortError::new(AppError::NotFound {
        resource: ResourceKind::TranscriptText,
    })
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::Arc};

    use ulid::Ulid;

    use super::*;
    use crate::{
        pipeline::delivery::DeliveryPorts,
        ports::fakes::{
            FakeClipboard, FakeForegroundApp, FakeNotifier, FakeTextInserter, RecordingSink,
        },
        registry::settings::{defaults, keys, resolve},
        types::{
            DeliveryOutcome, EngineId, NewTranscript, SessionStatus, SettingValue,
            TranscriptChange, UnixMs, testing::TempDir,
        },
    };

    struct Fixture {
        db: Db,
        paths: AppPaths,
        _dir: TempDir,
        clipboard: Arc<FakeClipboard>,
        inserter: Arc<FakeTextInserter>,
        delivery: Delivery,
    }

    fn fixture() -> Fixture {
        let dir = TempDir::new("history-pipeline");
        let paths = AppPaths::new(dir.path().join("data"), dir.path().join("resources"));
        let clipboard = Arc::new(FakeClipboard::default());
        let inserter = Arc::new(FakeTextInserter::default());
        let delivery = Delivery::new(DeliveryPorts {
            clipboard: Arc::clone(&clipboard) as _,
            inserter: Arc::clone(&inserter) as _,
            notifier: Arc::new(FakeNotifier::default()),
        });
        Fixture {
            db: Db::open_in_memory().unwrap(),
            paths,
            _dir: dir,
            clipboard,
            inserter,
            delivery,
        }
    }

    /// A take created at `millis` with the given status and texts, and a journal file on disk.
    fn take(
        fixture: &Fixture,
        millis: u64,
        status: TranscriptStatus,
        raw: Option<&str>,
        text: Option<&str>,
    ) -> TranscriptId {
        let id: TranscriptId = Ulid::from_parts(millis, 1).to_string().parse().unwrap();
        services::transcripts::insert::insert(
            &fixture.db,
            &NewTranscript {
                id,
                created_at: UnixMs::from_millis(i64::try_from(millis).unwrap()),
                status: TranscriptStatus::Recording,
                audio_path: Some(AppPaths::recording_name(id)),
                engine_id: Some(EngineId::from_static("parakeet-tdt-0.6b-v3")),
                app_name: None,
            },
        )
        .unwrap();
        services::transcripts::update::update(
            &fixture.db,
            id,
            &[
                TranscriptChange::Status(status),
                TranscriptChange::RawText(raw.map(str::to_owned)),
                TranscriptChange::FinalText(text.map(str::to_owned)),
            ],
        )
        .unwrap();
        let wav = fixture.paths.recording(id);
        fs::create_dir_all(wav.parent().unwrap()).unwrap();
        fs::write(&wav, b"RIFF").unwrap();
        id
    }

    #[test]
    fn take_text_prefers_the_polished_text_then_the_raw_text() {
        let fixture = fixture();
        let done = take(
            &fixture,
            1_000,
            TranscriptStatus::Done,
            Some("raw"),
            Some(" Final. "),
        );
        let failed = take(
            &fixture,
            2_000,
            TranscriptStatus::Failed,
            Some("Heard."),
            None,
        );
        let silent = take(&fixture, 3_000, TranscriptStatus::Empty, None, Some("  "));
        let read = |id| services::transcripts::get::get(&fixture.db, id).unwrap();
        assert_eq!(take_text(&read(done)), Some("Final."));
        assert_eq!(take_text(&read(failed)), Some("Heard."));
        assert_eq!(take_text(&read(silent)), None);
    }

    #[test]
    fn delete_removes_the_journal_and_the_row() {
        let fixture = fixture();
        let id = take(&fixture, 1_000, TranscriptStatus::Done, None, Some("Bye."));
        delete_take(&fixture.db, &fixture.paths, id).unwrap();
        assert!(!fixture.paths.recording(id).exists());
        assert!(services::transcripts::get::get(&fixture.db, id).is_err());
        assert_eq!(
            delete_take(&fixture.db, &fixture.paths, id)
                .unwrap_err()
                .error(),
            &AppError::NotFound {
                resource: ResourceKind::Transcript
            }
        );
    }

    #[test]
    fn clear_erases_listed_takes_but_keeps_their_measurements() {
        let fixture = fixture();
        let done = take(
            &fixture,
            1_000,
            TranscriptStatus::Done,
            Some("raw"),
            Some("Kept?"),
        );
        services::transcripts::update::update(
            &fixture.db,
            done,
            &[
                TranscriptChange::WordCount(1),
                TranscriptChange::LatencyMs(300),
                TranscriptChange::AppName(Some("notepad.exe".to_owned())),
            ],
        )
        .unwrap();
        let failed = take(
            &fixture,
            2_000,
            TranscriptStatus::Failed,
            Some("Heard."),
            None,
        );
        let silent = take(&fixture, 3_000, TranscriptStatus::Empty, None, None);
        let unfinished = take(&fixture, 4_000, TranscriptStatus::Transcribing, None, None);
        let live = take(&fixture, 5_000, TranscriptStatus::Done, None, Some("Live."));

        let cleared = clear_history(
            &fixture.db,
            &fixture.paths,
            Some(live),
            UnixMs::from_millis(9_000),
        )
        .unwrap();
        assert_eq!(cleared, 2);
        let read = |id| services::transcripts::get::get(&fixture.db, id).unwrap();
        let erased = read(done);
        assert_eq!(
            (erased.raw_text, erased.final_text, erased.app_name),
            (None, None, None)
        );
        assert!(!erased.has_audio);
        assert_eq!(erased.status, TranscriptStatus::Done);
        assert_eq!((erased.word_count, erased.latency_ms), (Some(1), Some(300)));
        assert!(!fixture.paths.recording(done).exists());
        assert!(!fixture.paths.recording(failed).exists());
        assert_eq!(read(failed).raw_text, None);
        for kept in [silent, unfinished, live] {
            assert!(fixture.paths.recording(kept).exists(), "{kept}");
        }
        assert_eq!(read(live).final_text.as_deref(), Some("Live."));

        let listed = services::transcripts::list::select(
            &fixture.db,
            &TranscriptSelector {
                cleared: Some(false),
                ..TranscriptSelector::default()
            },
        )
        .unwrap();
        assert_eq!(
            listed.iter().map(|row| row.id).collect::<Vec<_>>(),
            [silent, unfinished, live]
        );
        assert_eq!(
            clear_history(
                &fixture.db,
                &fixture.paths,
                Some(live),
                UnixMs::from_millis(9_500)
            )
            .unwrap(),
            0,
            "a second clear finds nothing left"
        );
    }

    #[test]
    fn copy_puts_the_text_on_the_clipboard_or_says_there_is_none() {
        let fixture = fixture();
        let done = take(
            &fixture,
            1_000,
            TranscriptStatus::Done,
            None,
            Some("Copy me."),
        );
        copy_take(&fixture.db, &fixture.delivery, done).unwrap();
        assert_eq!(fixture.clipboard.text().as_deref(), Some("Copy me."));

        let empty = take(&fixture, 2_000, TranscriptStatus::Empty, None, None);
        assert_eq!(
            copy_take(&fixture.db, &fixture.delivery, empty)
                .unwrap_err()
                .error(),
            &AppError::NotFound {
                resource: ResourceKind::TranscriptText
            }
        );
    }

    #[test]
    fn paste_last_pastes_the_newest_done_take_with_the_trailing_space() {
        let fixture = fixture();
        let foreground =
            FakeForegroundApp::focused(FakeForegroundApp::target("notepad.exe", false));
        assert_eq!(
            paste_last(&fixture.db, &fixture.delivery, &foreground, &defaults())
                .unwrap_err()
                .error(),
            &AppError::NotFound {
                resource: ResourceKind::TranscriptText
            }
        );

        take(
            &fixture,
            1_000,
            TranscriptStatus::Done,
            None,
            Some("Older."),
        );
        take(
            &fixture,
            2_000,
            TranscriptStatus::Done,
            None,
            Some("Newest."),
        );
        take(
            &fixture,
            3_000,
            TranscriptStatus::Failed,
            Some("Failed."),
            None,
        );
        let report = paste_last(&fixture.db, &fixture.delivery, &foreground, &defaults()).unwrap();
        assert_eq!(report.outcome, DeliveryOutcome::Pasted);
        assert_eq!(fixture.inserter.insertions().len(), 1);
        assert_eq!(fixture.clipboard.text().as_deref(), Some("Newest. "));

        let no_space = resolve([(keys::TRAILING_SPACE, SettingValue::Bool(false))]);
        paste_last(&fixture.db, &fixture.delivery, &foreground, &no_space).unwrap();
        assert_eq!(fixture.clipboard.text().as_deref(), Some("Newest."));
    }

    #[test]
    fn echo_own_window_is_never_a_paste_target() {
        let fixture = fixture();
        take(&fixture, 1_000, TranscriptStatus::Done, None, Some("Mine."));
        let mut own = FakeForegroundApp::target("echo.exe", false);
        own.process_id = std::process::id();
        let foreground = FakeForegroundApp::focused(own);
        let report = paste_last(&fixture.db, &fixture.delivery, &foreground, &defaults()).unwrap();
        assert_eq!(report.outcome, DeliveryOutcome::Copied);
        assert!(fixture.inserter.insertions().is_empty());
    }

    #[test]
    fn only_the_session_s_unsettled_take_is_refused() {
        let id = TranscriptId::generate();
        let live = SessionView {
            status: SessionStatus::Finalizing,
            transcript_id: Some(id),
            ..SessionView::IDLE
        };
        assert_eq!(
            ensure_not_live(&live, id).unwrap_err().error(),
            &AppError::Busy
        );
        assert!(ensure_not_live(&live, TranscriptId::generate()).is_ok());
        assert!(ensure_not_live(&SessionView::IDLE, id).is_ok());
    }

    #[test]
    fn announcements_name_the_change() {
        let fixture = fixture();
        let id = take(
            &fixture,
            1_000,
            TranscriptStatus::Done,
            None,
            Some("Saved."),
        );
        let sink = RecordingSink::<AppEvent>::default();
        announce_saved(&fixture.db, &sink, id);
        let events = sink.events();
        assert!(
            matches!(&events[0], AppEvent::TranscriptSaved(TranscriptSaved(row)) if row.id == id)
        );
        assert_eq!(
            events[1],
            AppEvent::from(HistoryChanged {
                reason: HistoryChangeReason::Updated
            })
        );
        assert_eq!(events[2], AppEvent::from(MetricsChanged {}));

        let sink = RecordingSink::<AppEvent>::default();
        announce_cleared(&sink);
        assert_eq!(
            sink.events(),
            [AppEvent::from(HistoryChanged {
                reason: HistoryChangeReason::Cleared
            })]
        );

        let sink = RecordingSink::<AppEvent>::default();
        announce_deleted(&sink);
        assert_eq!(
            sink.events(),
            [
                AppEvent::from(HistoryChanged {
                    reason: HistoryChangeReason::Deleted
                }),
                AppEvent::from(MetricsChanged {})
            ]
        );
    }
}
