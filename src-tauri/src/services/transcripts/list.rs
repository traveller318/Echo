/*!
 * SOURCE OF TRUTH KEYWORDS: list transcripts, history_list, history page, full-text search, FTS5 match, cursor paging, keyset pagination, select transcripts
 * WHAT:  `list`: one page of history rows, newest first, limited to the rows a TranscriptSelector matches and
 *        optionally to a full-text search, continuing after a cursor. `select`: every take a TranscriptSelector matches, oldest first, as TranscriptRefs.
 * WHY:   Keyset paging on the ULID primary key (`id < cursor`) stays fast and stable at 100k+ rows while new takes
 *        arrive, unlike OFFSET (02 §7.2). The cursor is the last row's id, wrapped in the opaque PageCursor so the
 *        encoding can change here without touching the UI. Search goes through the FTS5 index (kept in sync by
 *        triggers); the user's words are turned into quoted prefix terms, so any input is a valid query (no FTS
 *        syntax errors, whose messages would echo the search text into the log) and "hel wor" finds "hello world".
 *        A search with no letters or digits matches nothing rather than everything. Which rows a page may show
 *        (e.g. History hiding no-speech takes) is the caller's rule, passed in as a selector and turned into SQL
 *        by the same builder as the bulk queries.
 * WHERE: history_list (history step); `select` by startup recovery and retention sweeps; service tests.
 */

use std::num::NonZeroU32;

use rusqlite::{params_from_iter, types::Value};

use super::{row, selector};
use crate::{
    services::db::Db,
    types::{
        AppError, Page, PageCursor, PortError, PortResult, TranscriptId, TranscriptRef,
        TranscriptSelector, TranscriptSummary,
    },
};

/**
 * SOURCE OF TRUTH KEYWORDS: history page query, search and cursor, next_cursor
 * WHAT:  Returns up to `limit` rows older than `cursor` (all rows when None) that `filter` matches and that match
 *        `search` (when set), and the cursor of the next page when more rows follow.
 * WHY:   One extra row is fetched to learn whether another page exists, so the last page reports no cursor and
 *        the UI never asks for an empty page. A cursor that is not an id this service produced is a validation
 *        error on `cursor`.
 * WHERE: history_list.
 */
pub fn list(
    db: &Db,
    filter: &TranscriptSelector,
    search: Option<&str>,
    cursor: Option<&PageCursor>,
    limit: NonZeroU32,
) -> PortResult<Page<TranscriptSummary>> {
    let mut sql = format!("SELECT {} FROM transcripts t", row::SUMMARY_COLUMNS);
    let mut params = vec![Value::Integer(row::PREVIEW_CHARS.into())];
    let (filtered, filter_params) = selector::condition(filter);
    let mut conditions = vec![filtered];
    params.extend(filter_params);
    if let Some(search) = search {
        let Some(expression) = match_expression(search) else {
            return Ok(Page::last(Vec::new()));
        };
        sql.push_str(" JOIN transcripts_fts ON transcripts_fts.rowid = t.rowid");
        conditions.push("transcripts_fts MATCH ?".to_owned());
        params.push(Value::Text(expression));
    }
    if let Some(cursor) = cursor {
        let after: TranscriptId = cursor.as_str().parse().map_err(|_| {
            PortError::new(AppError::validation(
                "cursor",
                "This page position is not valid.",
            ))
        })?;
        conditions.push("t.id < ?".to_owned());
        params.push(Value::Text(row::id_text(after)));
    }
    sql.push_str(" WHERE ");
    sql.push_str(&conditions.join(" AND "));
    sql.push_str(" ORDER BY t.id DESC LIMIT ?");
    params.push(Value::Integer(i64::from(limit.get()) + 1));

    let mut items = db.read(|connection| {
        connection
            .prepare_cached(&sql)?
            .query_map(params_from_iter(params), row::summary)?
            .collect::<rusqlite::Result<Vec<_>>>()
    })?;
    let page_size = usize::try_from(limit.get()).unwrap_or(usize::MAX);
    if items.len() <= page_size {
        return Ok(Page::last(items));
    }
    items.truncate(page_size);
    let next = items
        .last()
        .map(|last| PageCursor::new(row::id_text(last.id)));
    Ok(Page::new(items, next))
}

/// Every take `selector` matches, oldest first.
pub fn select(db: &Db, selector: &TranscriptSelector) -> PortResult<Vec<TranscriptRef>> {
    let (condition, params) = selector::condition(selector);
    let sql = format!(
        "SELECT {} FROM transcripts WHERE {condition} ORDER BY id",
        row::REF_COLUMNS
    );
    db.read(|connection| {
        connection
            .prepare(&sql)?
            .query_map(params_from_iter(params), row::reference)?
            .collect()
    })
}

/// The search as an FTS5 query: each whitespace-separated word as a quoted prefix term, all required.
fn match_expression(search: &str) -> Option<String> {
    let terms: Vec<String> = search
        .split_whitespace()
        .filter(|word| word.chars().any(char::is_alphanumeric))
        .map(|word| format!("\"{}\"*", word.replace('"', "\"\"")))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

#[cfg(test)]
mod tests {
    use super::super::{
        fixtures::{done, id_at, new_take},
        insert, update,
    };
    use super::*;
    use crate::types::{TranscriptChange, TranscriptStatus, UnixMs};

    const ANY: TranscriptSelector = TranscriptSelector {
        statuses: None,
        created_before: None,
        has_audio: None,
        cleared: None,
    };

    fn limit(value: u32) -> NonZeroU32 {
        NonZeroU32::new(value).unwrap()
    }

    fn ids(page: &Page<TranscriptSummary>) -> Vec<TranscriptId> {
        page.items.iter().map(|item| item.id).collect()
    }

    #[test]
    fn pages_run_newest_first_and_the_last_page_has_no_cursor() {
        let db = Db::open_in_memory().unwrap();
        for millis in 1..=5 {
            done(&db, millis * 1_000, "Take.", 1, 100);
        }
        let first = list(&db, &ANY, None, None, limit(2)).unwrap();
        assert_eq!(ids(&first), [id_at(5_000), id_at(4_000)]);
        let second = list(&db, &ANY, None, first.next_cursor.as_ref(), limit(2)).unwrap();
        assert_eq!(ids(&second), [id_at(3_000), id_at(2_000)]);
        let third = list(&db, &ANY, None, second.next_cursor.as_ref(), limit(2)).unwrap();
        assert_eq!(ids(&third), [id_at(1_000)]);
        assert_eq!(third.next_cursor, None);

        let exact = list(&db, &ANY, None, None, limit(5)).unwrap();
        assert_eq!(exact.items.len(), 5);
        assert_eq!(exact.next_cursor, None, "no empty page after an exact fit");
    }

    #[test]
    fn rows_carry_a_trimmed_preview_and_whether_audio_is_kept() {
        let db = Db::open_in_memory().unwrap();
        let long = format!("  {}", "word ".repeat(100));
        let id = done(&db, 1_000, &long, 100, 100);
        update::update(&db, id, &[TranscriptChange::AudioPath(None)]).unwrap();
        insert::insert(&db, &new_take(2_000)).unwrap();

        let page = list(&db, &ANY, None, None, limit(10)).unwrap();
        let [recording, completed] = page.items.as_slice() else {
            panic!("expected two rows");
        };
        assert_eq!(recording.status, TranscriptStatus::Recording);
        assert_eq!(recording.preview, None);
        assert!(recording.has_audio);
        let preview = completed.preview.as_deref().unwrap();
        assert!(preview.starts_with("word"));
        assert!(preview.chars().count() <= 160);
        assert!(!completed.has_audio);
        assert_eq!(completed.word_count, Some(100));
        assert_eq!(completed.app_name.as_deref(), Some("notepad.exe"));
    }

    #[test]
    fn search_matches_words_and_prefixes_and_follows_text_changes() {
        let db = Db::open_in_memory().unwrap();
        let hello = done(&db, 1_000, "Hello world from Echo.", 4, 100);
        let meeting = done(&db, 2_000, "Schedule the meeting for Tuesday.", 5, 100);
        done(&db, 3_000, "Nothing relevant here.", 3, 100);

        assert_eq!(
            ids(&list(&db, &ANY, Some("hello"), None, limit(10)).unwrap()),
            [hello]
        );
        assert_eq!(
            ids(&list(&db, &ANY, Some("hel wor"), None, limit(10)).unwrap()),
            [hello]
        );
        assert_eq!(
            ids(&list(&db, &ANY, Some("MEET"), None, limit(10)).unwrap()),
            [meeting]
        );
        assert!(
            list(&db, &ANY, Some("hello tuesday"), None, limit(10))
                .unwrap()
                .items
                .is_empty()
        );

        update::update(
            &db,
            meeting,
            &[TranscriptChange::FinalText(Some("Cancel it.".to_owned()))],
        )
        .unwrap();
        assert!(
            list(&db, &ANY, Some("meeting"), None, limit(10))
                .unwrap()
                .items
                .is_empty()
        );
        assert_eq!(
            ids(&list(&db, &ANY, Some("cancel"), None, limit(10)).unwrap()),
            [meeting]
        );
    }

    #[test]
    fn search_text_can_never_break_the_query() {
        let db = Db::open_in_memory().unwrap();
        let quoted = done(
            &db,
            1_000,
            "She said \"ok\" AND left (NEAR the door).",
            7,
            100,
        );
        for search in [
            "\"ok\"", "AND", "(near", "door)", "ok*", "-left", "she:said", "ok\"",
        ] {
            let page = list(&db, &ANY, Some(search), None, limit(10)).unwrap();
            assert_eq!(ids(&page), [quoted], "{search}");
        }
        for search in ["!!!", "\"", "   ", "*"] {
            assert!(
                list(&db, &ANY, Some(search), None, limit(10))
                    .unwrap()
                    .items
                    .is_empty(),
                "{search}"
            );
        }
    }

    #[test]
    fn search_pages_with_the_cursor_too() {
        let db = Db::open_in_memory().unwrap();
        for millis in 1..=3 {
            done(&db, millis * 1_000, "Echo note.", 2, 100);
        }
        done(&db, 4_000, "Other.", 1, 100);
        let first = list(&db, &ANY, Some("echo"), None, limit(2)).unwrap();
        assert_eq!(ids(&first), [id_at(3_000), id_at(2_000)]);
        let second = list(
            &db,
            &ANY,
            Some("echo"),
            first.next_cursor.as_ref(),
            limit(2),
        )
        .unwrap();
        assert_eq!(ids(&second), [id_at(1_000)]);
        assert_eq!(second.next_cursor, None);
    }

    #[test]
    fn the_filter_hides_rows_across_pages_and_searches() {
        let db = Db::open_in_memory().unwrap();
        let first = done(&db, 1_000, "Echo one.", 2, 100);
        insert::insert(&db, &new_take(2_000)).unwrap();
        update::update(
            &db,
            id_at(2_000),
            &[TranscriptChange::Status(TranscriptStatus::Empty)],
        )
        .unwrap();
        let third = done(&db, 3_000, "Echo three.", 2, 100);
        let spoken = TranscriptSelector {
            statuses: Some(vec![TranscriptStatus::Done]),
            ..TranscriptSelector::default()
        };

        let page = list(&db, &spoken, None, None, limit(1)).unwrap();
        assert_eq!(ids(&page), [third]);
        let rest = list(&db, &spoken, None, page.next_cursor.as_ref(), limit(1)).unwrap();
        assert_eq!(ids(&rest), [first]);
        assert_eq!(rest.next_cursor, None);
        assert_eq!(
            ids(&list(&db, &spoken, Some("echo"), None, limit(10)).unwrap()),
            [third, first]
        );
        assert_eq!(
            list(&db, &ANY, None, None, limit(10)).unwrap().items.len(),
            3
        );
    }

    #[test]
    fn a_foreign_cursor_is_a_validation_error() {
        let db = Db::open_in_memory().unwrap();
        let error = list(&db, &ANY, None, Some(&PageCursor::new("page-2")), limit(10)).unwrap_err();
        let AppError::Validation { field, .. } = error.error() else {
            panic!("expected a validation error");
        };
        assert_eq!(field, "cursor");
    }

    #[test]
    fn select_returns_matching_refs_oldest_first() {
        let db = Db::open_in_memory().unwrap();
        insert::insert(&db, &new_take(1_000)).unwrap();
        let old_done = done(&db, 2_000, "Done.", 1, 100);
        insert::insert(&db, &new_take(3_000)).unwrap();
        update::update(
            &db,
            id_at(3_000),
            &[TranscriptChange::Status(TranscriptStatus::Transcribing)],
        )
        .unwrap();

        let unfinished = select(
            &db,
            &TranscriptSelector {
                statuses: Some(vec![
                    TranscriptStatus::Recording,
                    TranscriptStatus::Transcribing,
                ]),
                ..TranscriptSelector::default()
            },
        )
        .unwrap();
        let unfinished_ids: Vec<_> = unfinished.iter().map(|take| take.id).collect();
        assert_eq!(unfinished_ids, [id_at(1_000), id_at(3_000)]);
        assert_eq!(unfinished[1].status, TranscriptStatus::Transcribing);
        assert_eq!(
            unfinished[0].audio_path.as_deref(),
            Some(format!("{}.wav", id_at(1_000)).as_str())
        );

        let old_with_audio = select(
            &db,
            &TranscriptSelector {
                statuses: Some(vec![TranscriptStatus::Done]),
                created_before: Some(UnixMs::from_millis(2_001)),
                has_audio: Some(true),
                cleared: None,
            },
        )
        .unwrap();
        assert_eq!(old_with_audio.len(), 1);
        assert_eq!(old_with_audio[0].id, old_done);
        assert!(
            select(
                &db,
                &TranscriptSelector {
                    statuses: Some(Vec::new()),
                    ..TranscriptSelector::default()
                }
            )
            .unwrap()
            .is_empty()
        );
    }
}
