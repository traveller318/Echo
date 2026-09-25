/*!
 * SOURCE OF TRUTH KEYWORDS: history commands, history_list, history_get, history_copy, history_delete, history_paste_last, History page, FTS search, cursor paging
 * WHAT:  The history command group (02 §4.3): `history_list` (one page of takes, newest first, optionally matching
 *        a full-text search), `history_get` (one take in full), `history_copy` (its text to the clipboard),
 *        `history_delete` (its row and WAV) and `history_paste_last` (the newest completed take into the focused
 *        app).
 * WHY:   The History page reads through these and stays fresh from HistoryChanged / TranscriptSaved, never by
 *        polling (02 §4.4). Handlers are thin: the factory already validated the input schema (search length,
 *        page size, cursor shape) and maps errors, services own the SQL, and pipeline/history.rs owns the rules
 *        (what text a take offers, WAV before row, the session's live take is refused with `Busy`, Echo's own
 *        window is never a paste target). Clipboard and file work runs on the blocking pool (05 W4). Delete
 *        announces HistoryChanged and MetricsChanged (a deleted take leaves the dashboard sums). Paste-last goes
 *        through the session actor, which owns the clipboard restore (05 W6).
 * WHERE: Registered through `ipc::commands::catalog`; called from the UI as `commands.historyList(…)`,
 *        `historyGet`, `historyCopy`, `historyDelete`, `historyPasteLast` (src/hooks/use-history.ts and the History
 *        route); the paste-last hotkey reaches the same actor path (pipeline/session/hotkey_input.rs).
 */

use std::num::NonZeroU32;

use crate::{
    ipc::{CommandCtx, factory::echo_command},
    pipeline::{blocking::run_blocking, history},
    services,
    types::{
        AppError, DeliveryOutcome, HistoryListInput, Page, PortError, Transcript, TranscriptInput,
        TranscriptSummary,
    },
};

echo_command! {
    /// One page of History, newest first: every take, or those whose text matches `search`.
    name: history_list,
    input: HistoryListInput,
    output: Page<TranscriptSummary>,
    permission: None,
    reentrancy: Shared,
    handler: list,
}

echo_command! {
    /// One take in full (both texts and every measurement).
    name: history_get,
    input: TranscriptInput,
    output: Transcript,
    permission: None,
    reentrancy: Shared,
    handler: get,
}

echo_command! {
    /// Copies a take's text to the clipboard (kept out of Windows clipboard history).
    name: history_copy,
    input: TranscriptInput,
    output: (),
    permission: None,
    reentrancy: Shared,
    handler: copy,
}

echo_command! {
    /// Deletes a take and its saved audio.
    name: history_delete,
    input: TranscriptInput,
    output: (),
    permission: None,
    reentrancy: Shared,
    handler: delete,
}

echo_command! {
    /// Pastes the newest completed take into the focused app again (copies it when pasting is not possible).
    name: history_paste_last,
    output: DeliveryOutcome,
    permission: None,
    reentrancy: Shared,
    handler: paste_last,
}

/// Reads one page of History.
pub async fn list(
    ctx: &CommandCtx,
    input: HistoryListInput,
) -> Result<Page<TranscriptSummary>, PortError> {
    let limit = NonZeroU32::new(input.limit).ok_or_else(|| {
        PortError::new(AppError::validation("limit", "Ask for at least one row."))
    })?;
    services::transcripts::list::list(ctx.db(), input.search_text(), input.cursor.as_ref(), limit)
}

/// Reads one take in full.
pub async fn get(ctx: &CommandCtx, input: TranscriptInput) -> Result<Transcript, PortError> {
    services::transcripts::get::get(ctx.db(), input.id)
}

/// Copies a take's text.
pub async fn copy(ctx: &CommandCtx, input: TranscriptInput) -> Result<(), PortError> {
    let db = ctx.db().clone();
    let delivery = ctx.delivery().clone();
    run_blocking("copying a take", move || {
        history::copy_take(&db, &delivery, input.id)
    })
    .await
}

/// Deletes a take unless the session is still working on it.
pub async fn delete(ctx: &CommandCtx, input: TranscriptInput) -> Result<(), PortError> {
    history::ensure_not_live(&ctx.session().view().await?, input.id)?;
    let db = ctx.db().clone();
    let paths = ctx.paths().clone();
    run_blocking("deleting a take", move || {
        history::delete_take(&db, &paths, input.id)
    })
    .await?;
    history::announce_deleted(ctx.events());
    Ok(())
}

/// Pastes the newest completed take again.
pub async fn paste_last(ctx: &CommandCtx, (): ()) -> Result<DeliveryOutcome, PortError> {
    ctx.session().paste_last().await
}

#[cfg(test)]
mod tests {
    use std::{fs, thread};

    use super::*;
    use crate::{
        ipc::{factory, testing},
        ports::fakes::FakePrivacyConsent,
        registry,
        types::{
            AppEvent, AppPaths, CommandSpec, EngineId, HistoryChangeReason, HistoryChanged,
            MetricsChanged, NewTranscript, PageCursor, Reentrancy, ResourceKind, TranscriptChange,
            TranscriptId, TranscriptStatus, UnixMs,
        },
    };

    const fn spec(name: &'static str) -> CommandSpec {
        CommandSpec {
            name,
            permission: None,
            reentrancy: Reentrancy::Shared,
        }
    }

    const LIST: CommandSpec = spec("history_list");
    const GET: CommandSpec = spec("history_get");
    const COPY: CommandSpec = spec("history_copy");
    const DELETE: CommandSpec = spec("history_delete");
    const PASTE_LAST: CommandSpec = spec("history_paste_last");

    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(future)
    }

    fn harness() -> testing::Harness {
        testing::harness(
            registry::settings::defaults(),
            FakePrivacyConsent::granted(),
        )
    }

    /// A completed take stamped at `millis` with `text`, whose journal is on disk.
    fn done(ctx: &CommandCtx, millis: u64, text: &str) -> TranscriptId {
        // Ordered by time, but unique across tests that share the harness folder.
        let id: TranscriptId = ulid::Ulid::from_parts(millis, ulid::Ulid::generate().random())
            .to_string()
            .parse()
            .unwrap();
        services::transcripts::insert::insert(
            ctx.db(),
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
            ctx.db(),
            id,
            &[
                TranscriptChange::Status(TranscriptStatus::Done),
                TranscriptChange::FinalText(Some(text.to_owned())),
                TranscriptChange::WordCount(
                    u32::try_from(text.split_whitespace().count()).unwrap(),
                ),
            ],
        )
        .unwrap();
        let wav = ctx.paths().recording(id);
        fs::create_dir_all(wav.parent().unwrap()).unwrap();
        fs::write(&wav, b"RIFF").unwrap();
        id
    }

    fn list_input(
        search: Option<&str>,
        cursor: Option<PageCursor>,
        limit: u32,
    ) -> HistoryListInput {
        HistoryListInput {
            search: search.map(str::to_owned),
            cursor,
            limit,
        }
    }

    #[test]
    fn list_pages_and_searches_through_the_factory() {
        let harness = harness();
        let ctx = &harness.ctx;
        let first = done(ctx, 1_000, "Buy milk tomorrow.");
        let second = done(ctx, 2_000, "Call the dentist.");
        let third = done(ctx, 3_000, "Milk and bread.");

        let page = block_on(factory::run(ctx, &LIST, list_input(None, None, 2), list)).unwrap();
        let ids: Vec<_> = page.items.iter().map(|row| row.id).collect();
        assert_eq!(ids, [third, second]);
        let next = block_on(factory::run(
            ctx,
            &LIST,
            list_input(None, page.next_cursor, 2),
            list,
        ))
        .unwrap();
        assert_eq!(
            next.items.iter().map(|row| row.id).collect::<Vec<_>>(),
            [first]
        );
        assert_eq!(next.next_cursor, None);

        let milk = block_on(factory::run(
            ctx,
            &LIST,
            list_input(Some("  milk "), None, 10),
            list,
        ))
        .unwrap();
        assert_eq!(
            milk.items.iter().map(|row| row.id).collect::<Vec<_>>(),
            [third, first]
        );

        let blank = block_on(factory::run(
            ctx,
            &LIST,
            list_input(Some("   "), None, 10),
            list,
        ))
        .unwrap();
        assert_eq!(blank.items.len(), 3, "a blank search lists everything");
    }

    #[test]
    fn list_input_bounds_are_enforced_by_the_factory() {
        let harness = harness();
        let ctx = &harness.ctx;
        let too_long = "a".repeat(HistoryListInput::MAX_SEARCH_CHARS + 1);
        for (input, field) in [
            (list_input(None, None, 0), "limit"),
            (
                list_input(None, None, HistoryListInput::MAX_LIMIT + 1),
                "limit",
            ),
            (list_input(Some(&too_long), None, 10), "search"),
            (list_input(None, Some(PageCursor::new("")), 10), "cursor"),
            (
                list_input(None, Some(PageCursor::new("x".repeat(65))), 10),
                "cursor",
            ),
        ] {
            let error = block_on(factory::run(ctx, &LIST, input, list)).unwrap_err();
            let AppError::Validation { field: failed, .. } = &error else {
                panic!("expected a validation error, got {error:?}");
            };
            assert_eq!(failed, field);
        }
        let foreign = block_on(factory::run(
            ctx,
            &LIST,
            list_input(None, Some(PageCursor::new("page-two")), 10),
            list,
        ))
        .unwrap_err();
        assert!(matches!(foreign, AppError::Validation { ref field, .. } if field == "cursor"));
    }

    #[test]
    fn get_and_copy_read_the_take() {
        let harness = harness();
        let ctx = &harness.ctx;
        let id = done(ctx, 1_000, "Copy this.");
        let take = block_on(factory::run(ctx, &GET, TranscriptInput { id }, get)).unwrap();
        assert_eq!(take.final_text.as_deref(), Some("Copy this."));

        block_on(factory::run(ctx, &COPY, TranscriptInput { id }, copy)).unwrap();
        assert_eq!(harness.clipboard.text().as_deref(), Some("Copy this."));

        let missing = TranscriptInput {
            id: TranscriptId::generate(),
        };
        assert_eq!(
            block_on(factory::run(ctx, &GET, missing, get)),
            Err(AppError::NotFound {
                resource: ResourceKind::Transcript
            })
        );
    }

    #[test]
    fn delete_removes_row_and_audio_and_announces_it() {
        let harness = harness();
        let actor = harness.session_actor;
        let runner = thread::spawn(move || block_on(actor.run()));
        let ctx = &harness.ctx;
        let id = done(ctx, 1_000, "Forget me.");

        block_on(factory::run(ctx, &DELETE, TranscriptInput { id }, delete)).unwrap();
        assert!(!ctx.paths().recording(id).exists());
        assert!(services::transcripts::get::get(ctx.db(), id).is_err());
        let events = harness.events.events();
        assert!(events.contains(&AppEvent::from(HistoryChanged {
            reason: HistoryChangeReason::Deleted
        })));
        assert!(events.contains(&AppEvent::from(MetricsChanged {})));
        assert_eq!(
            block_on(factory::run(ctx, &DELETE, TranscriptInput { id }, delete)),
            Err(AppError::NotFound {
                resource: ResourceKind::Transcript
            })
        );

        assert!(ctx.session().shutdown(std::time::Duration::from_secs(5)));
        runner.join().unwrap();
    }

    #[test]
    fn paste_last_goes_through_the_session_actor() {
        let harness = harness();
        let actor = harness.session_actor;
        let runner = thread::spawn(move || block_on(actor.run()));
        let ctx = &harness.ctx;
        assert_eq!(
            block_on(factory::run(ctx, &PASTE_LAST, (), paste_last)),
            Err(AppError::NotFound {
                resource: ResourceKind::TranscriptText
            })
        );
        done(ctx, 1_000, "Again please.");
        assert_eq!(
            block_on(factory::run(ctx, &PASTE_LAST, (), paste_last)),
            Ok(DeliveryOutcome::Pasted)
        );
        assert_eq!(harness.inserter.insertions().len(), 1);

        assert!(ctx.session().shutdown(std::time::Duration::from_secs(5)));
        runner.join().unwrap();
    }
}
