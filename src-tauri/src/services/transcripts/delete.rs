/*!
 * SOURCE OF TRUTH KEYWORDS: delete transcript, history_delete, discard take, delete matching, history retention, bulk delete
 * WHAT:  `delete`: removes one take's row (`NotFound { transcript }` if absent). `delete_matching`: removes every
 *        row a TranscriptSelector matches and returns them (oldest first) so the caller can delete their journals.
 * WHY:   The row and its WAV are removed together by the caller: services never touch files (02 §3.2), so bulk
 *        deletes report exactly which rows went, via `RETURNING`, in the same statement that removed them. The
 *        FTS entry goes with the row through the migration's trigger. Which rows may be deleted (never
 *        failed/recoverable audio, the retention cut-off) is the caller's rule, expressed in the selector.
 * WHERE: history_delete and Esc discard (single row); history retention sweep (matching); service tests.
 */

use rusqlite::{params, params_from_iter};

use super::{row, selector};
use crate::{
    services::db::Db,
    types::{
        AppError, PortError, PortResult, ResourceKind, TranscriptId, TranscriptRef,
        TranscriptSelector,
    },
};

pub fn delete(db: &Db, id: TranscriptId) -> PortResult<()> {
    let deleted = db.write(|connection| {
        connection
            .prepare_cached("DELETE FROM transcripts WHERE id = ?1")?
            .execute(params![row::id_text(id)])
    })?;
    if deleted == 0 {
        return Err(PortError::new(AppError::NotFound {
            resource: ResourceKind::Transcript,
        }));
    }
    Ok(())
}

pub fn delete_matching(db: &Db, selector: &TranscriptSelector) -> PortResult<Vec<TranscriptRef>> {
    let (condition, params) = selector::condition(selector);
    let sql = format!(
        "DELETE FROM transcripts WHERE {condition} RETURNING {}",
        row::REF_COLUMNS
    );
    let mut deleted = db.write(|connection| {
        connection
            .prepare(&sql)?
            .query_map(params_from_iter(params), row::reference)?
            .collect::<rusqlite::Result<Vec<_>>>()
    })?;
    // RETURNING yields rows in an unspecified order.
    deleted.sort_by_key(|take| take.id);
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::super::{
        fixtures::{done, id_at, new_take},
        get, insert, list,
    };
    use super::*;
    use crate::types::{TranscriptStatus, UnixMs};

    fn searchable(db: &Db, word: &str) -> usize {
        list::list(db, Some(word), None, NonZeroU32::MIN)
            .unwrap()
            .items
            .len()
    }

    #[test]
    fn delete_removes_the_row_and_its_search_entry() {
        let db = Db::open_in_memory().unwrap();
        let id = done(&db, 1_000, "Remember the milk.", 3, 100);
        assert_eq!(searchable(&db, "milk"), 1);
        delete(&db, id).unwrap();
        assert!(get::get(&db, id).is_err());
        assert_eq!(searchable(&db, "milk"), 0);
        assert_eq!(
            delete(&db, id).unwrap_err().error(),
            &AppError::NotFound {
                resource: ResourceKind::Transcript
            }
        );
    }

    #[test]
    fn delete_matching_removes_only_the_selection_and_reports_it() {
        let db = Db::open_in_memory().unwrap();
        done(&db, 1_000, "Old note.", 2, 100);
        insert::insert(&db, &new_take(2_000)).unwrap();
        let kept = done(&db, 5_000, "New note.", 2, 100);

        let removed = delete_matching(
            &db,
            &TranscriptSelector {
                created_before: Some(UnixMs::from_millis(3_000)),
                ..TranscriptSelector::default()
            },
        )
        .unwrap();
        let removed_ids: Vec<_> = removed.iter().map(|take| take.id).collect();
        assert_eq!(removed_ids, [id_at(1_000), id_at(2_000)]);
        assert_eq!(removed[0].status, TranscriptStatus::Done);
        assert!(removed[1].audio_path.is_some());
        assert_eq!(searchable(&db, "old"), 0);
        assert_eq!(get::get(&db, kept).unwrap().id, kept);

        let nothing = delete_matching(
            &db,
            &TranscriptSelector {
                statuses: Some(Vec::new()),
                ..TranscriptSelector::default()
            },
        )
        .unwrap();
        assert!(nothing.is_empty());
        assert_eq!(get::get(&db, kept).unwrap().id, kept);
    }
}
