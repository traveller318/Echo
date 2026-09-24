/*!
 * SOURCE OF TRUTH KEYWORDS: get transcript, history_get, latest transcript, paste last, transcript by id
 * WHAT:  `get`: one take in full by id (`NotFound { transcript }` if absent). `latest`: the newest take with a
 *        given status, if any.
 * WHY:   The history detail view, copy and retry read a take by id; paste-last reads the newest completed take.
 *        Newest is by id, which is a ULID and therefore creation order (02 §7.2), so no extra index is needed.
 * WHERE: history_get / history_copy / session_retry / history_paste_last (their steps); service tests.
 */

use rusqlite::{OptionalExtension, params};

use super::row;
use crate::{
    services::db::Db,
    types::{
        AppError, PortError, PortResult, ResourceKind, Transcript, TranscriptId, TranscriptStatus,
    },
};

pub fn get(db: &Db, id: TranscriptId) -> PortResult<Transcript> {
    let sql = format!(
        "SELECT {} FROM transcripts WHERE id = ?1",
        row::TRANSCRIPT_COLUMNS
    );
    db.read(|connection| {
        connection
            .prepare_cached(&sql)?
            .query_row(params![row::id_text(id)], row::transcript)
            .optional()
    })?
    .ok_or(PortError::new(AppError::NotFound {
        resource: ResourceKind::Transcript,
    }))
}

pub fn latest(db: &Db, status: TranscriptStatus) -> PortResult<Option<Transcript>> {
    let sql = format!(
        "SELECT {} FROM transcripts WHERE status = ?1 ORDER BY id DESC LIMIT 1",
        row::TRANSCRIPT_COLUMNS
    );
    db.read(|connection| {
        connection
            .prepare_cached(&sql)?
            .query_row(params![status.as_str()], row::transcript)
            .optional()
    })
}

#[cfg(test)]
mod tests {
    use super::super::{
        fixtures::{done, id_at, new_take},
        insert,
    };
    use super::*;

    #[test]
    fn get_returns_the_take_or_not_found() {
        let db = Db::open_in_memory().unwrap();
        let id = done(&db, 1_000, "First take.", 2, 100);
        let take = get(&db, id).unwrap();
        assert_eq!(take.final_text.as_deref(), Some("First take."));
        assert_eq!(take.status, TranscriptStatus::Done);
        assert_eq!(
            get(&db, id_at(2_000)).unwrap_err().error(),
            &AppError::NotFound {
                resource: ResourceKind::Transcript
            }
        );
    }

    #[test]
    fn latest_is_the_newest_take_with_the_status() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(latest(&db, TranscriptStatus::Done).unwrap(), None);
        done(&db, 1_000, "Older.", 1, 100);
        let newest = done(&db, 3_000, "Newest.", 1, 100);
        insert::insert(&db, &new_take(5_000)).unwrap();
        let found = latest(&db, TranscriptStatus::Done).unwrap().unwrap();
        assert_eq!(found.id, newest);
        assert_eq!(
            latest(&db, TranscriptStatus::Recording)
                .unwrap()
                .unwrap()
                .id,
            id_at(5_000)
        );
    }

    #[test]
    fn a_status_this_build_does_not_know_is_a_storage_error_not_a_guess() {
        let db = Db::open_in_memory().unwrap();
        let take = new_take(1_000);
        insert::insert(&db, &take).unwrap();
        db.write(|connection| connection.execute("UPDATE transcripts SET status = 'archived'", []))
            .unwrap();
        assert_eq!(get(&db, take.id).unwrap_err().error(), &AppError::Storage);
    }
}
