/*!
 * SOURCE OF TRUTH KEYWORDS: insert transcript, new take row, row before mic, transcripts insert
 * WHAT:  `insert`: writes a new take's row from a NewTranscript; every column not in it starts NULL.
 * WHY:   02 §7.3 step 1: the row exists (`recording`) before the microphone produces audio, so a crash at any later
 *        point leaves a row that startup recovery can find. A duplicate id is a storage error, never an overwrite.
 * WHERE: Called by the session actor when a take starts; by service tests.
 */

use rusqlite::params;

use super::row;
use crate::{
    services::db::Db,
    types::{NewTranscript, PortResult},
};

pub fn insert(db: &Db, take: &NewTranscript) -> PortResult<()> {
    db.write(|connection| {
        connection
            .prepare_cached(
                "INSERT INTO transcripts (id, created_at, status, audio_path, engine_id, app_name) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?
            .execute(params![
                row::id_text(take.id),
                take.created_at.as_millis(),
                take.status.as_str(),
                take.audio_path,
                take.engine_id.as_ref().map(|engine| engine.as_str()),
                take.app_name,
            ])
            .map(drop)
    })
}

#[cfg(test)]
mod tests {
    use super::super::{fixtures::new_take, get};
    use super::*;
    use crate::types::{AppError, AppPaths, EngineId, TranscriptStatus};

    #[test]
    fn a_new_take_is_stored_with_only_its_known_columns() {
        let db = Db::open_in_memory().unwrap();
        let take = new_take(1_000);
        insert(&db, &take).unwrap();

        let stored = get::get(&db, take.id).unwrap();
        assert_eq!(stored.id, take.id);
        assert_eq!(stored.created_at, take.created_at);
        assert_eq!(stored.status, TranscriptStatus::Recording);
        assert!(stored.has_audio);
        assert_eq!(
            stored.engine_id,
            Some(EngineId::from_static("parakeet-tdt-0.6b-v3"))
        );
        assert_eq!(stored.app_name.as_deref(), Some("notepad.exe"));
        assert_eq!(stored.final_text, None);
        assert_eq!(stored.word_count, None);
        assert!(stored.polisher_ids.is_empty());
        assert_eq!(take.audio_path, Some(AppPaths::recording_name(take.id)));
    }

    #[test]
    fn a_duplicate_id_is_a_storage_error() {
        let db = Db::open_in_memory().unwrap();
        let take = new_take(1_000);
        insert(&db, &take).unwrap();
        let error = insert(&db, &take).unwrap_err();
        assert_eq!(error.error(), &AppError::Storage);
        assert!(error.detail().unwrap().contains("UNIQUE"));
    }
}
