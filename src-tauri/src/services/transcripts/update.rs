/*!
 * SOURCE OF TRUTH KEYWORDS: update transcript, TranscriptChange, set columns, status transition write, clear audio path, retry reset, update_each, bulk update, cleared_at
 * WHAT:  `update`: applies a list of TranscriptChange to one take in a single statement; fails with
 *        `NotFound { transcript }` when the id has no row. An empty list changes nothing. `update_each`: applies
 *        the same changes to every listed take in one transaction and returns how many rows changed (missing ids
 *        are skipped, not an error).
 * WHY:   Each pipeline stage writes a different set of columns; one statement per call keeps a stage's writes
 *        atomic and keeps column names spelled in one place. Which transitions are allowed is the session state
 *        machine's decision, not this verb's. Changing `final_text` refreshes the search index through the
 *        migration's trigger. `update_each` exists for clearing History: thousands of rows in one transaction
 *        instead of one commit each, and only the ids the caller already handled (their WAVs are gone), which a
 *        selector cannot express. A row deleted meanwhile (retention) is simply not counted.
 * WHERE: Called by the session actor (status, text, measurements), recovery (status), retry and retention
 *        (audio path); `update_each` by pipeline/history.rs (clear_history); by service tests.
 */

use rusqlite::{params_from_iter, types::Value};

use super::row;
use crate::{
    services::db::{Db, storage},
    types::{AppError, PortError, PortResult, ResourceKind, TranscriptChange, TranscriptId},
};

pub fn update(db: &Db, id: TranscriptId, changes: &[TranscriptChange]) -> PortResult<()> {
    if changes.is_empty() {
        return Ok(());
    }
    let (sql, mut values) = statement(changes)?;
    values.push(Value::Text(row::id_text(id)));
    let changed = db.write(|connection| connection.execute(&sql, params_from_iter(values)))?;
    if changed == 0 {
        return Err(PortError::new(AppError::NotFound {
            resource: ResourceKind::Transcript,
        }));
    }
    Ok(())
}

pub fn update_each(
    db: &Db,
    ids: &[TranscriptId],
    changes: &[TranscriptChange],
) -> PortResult<usize> {
    if ids.is_empty() || changes.is_empty() {
        return Ok(0);
    }
    let (sql, values) = statement(changes)?;
    db.write(|connection| {
        let mut statement = connection.prepare(&sql)?;
        let mut changed = 0;
        for id in ids {
            let bound = values
                .iter()
                .cloned()
                .chain(std::iter::once(Value::Text(row::id_text(*id))));
            changed += statement.execute(params_from_iter(bound))?;
        }
        Ok(changed)
    })
}

/// The `UPDATE … WHERE id = ?` statement for `changes` and the values bound before the id.
fn statement(changes: &[TranscriptChange]) -> PortResult<(String, Vec<Value>)> {
    let mut assignments = Vec::with_capacity(changes.len());
    let mut values = Vec::with_capacity(changes.len() + 1);
    for change in changes {
        let (column, value) = column_value(change).map_err(storage)?;
        assignments.push(format!("{column} = ?"));
        values.push(value);
    }
    let sql = format!(
        "UPDATE transcripts SET {} WHERE id = ?",
        assignments.join(", ")
    );
    Ok((sql, values))
}

/// The column a change writes and the value it binds (NULL for a cleared column).
fn column_value(change: &TranscriptChange) -> rusqlite::Result<(&'static str, Value)> {
    let text = |value: &Option<String>| value.clone().map_or(Value::Null, Value::Text);
    Ok(match change {
        TranscriptChange::Status(status) => ("status", Value::Text(status.as_str().to_owned())),
        TranscriptChange::RawText(value) => ("raw_text", text(value)),
        TranscriptChange::FinalText(value) => ("final_text", text(value)),
        TranscriptChange::AudioPath(value) => ("audio_path", text(value)),
        TranscriptChange::DurationMs(value) => ("duration_ms", Value::Integer(i64::from(*value))),
        TranscriptChange::SpeechMs(value) => ("speech_ms", Value::Integer(i64::from(*value))),
        TranscriptChange::WordCount(value) => ("word_count", Value::Integer(i64::from(*value))),
        TranscriptChange::EngineId(engine) => ("engine_id", Value::Text(engine.to_string())),
        TranscriptChange::PolisherIds(ids) => {
            ("polisher_ids", Value::Text(row::polisher_ids_text(ids)?))
        }
        TranscriptChange::Language(language) => (
            "language",
            language
                .as_ref()
                .map_or(Value::Null, |language| Value::Text(language.to_string())),
        ),
        TranscriptChange::LatencyMs(value) => ("latency_ms", Value::Integer(i64::from(*value))),
        TranscriptChange::AppName(value) => ("app_name", text(value)),
        TranscriptChange::ErrorCode(code) => (
            "error_code",
            code.map_or(Value::Null, |code| Value::Text(code.as_str().to_owned())),
        ),
        TranscriptChange::ClearedAt(at) => ("cleared_at", Value::Integer(at.as_millis())),
    })
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::super::{
        fixtures::{done, id_at, new_take},
        get, insert, list,
    };
    use super::*;
    use crate::types::{
        AppErrorCode, EngineId, Language, TranscriptSelector, TranscriptStatus, UnixMs,
    };

    #[test]
    fn every_change_lands_in_its_column_and_cleared_columns_become_null() {
        let db = Db::open_in_memory().unwrap();
        let take = new_take(5_000);
        insert::insert(&db, &take).unwrap();
        update(
            &db,
            take.id,
            &[
                TranscriptChange::Status(TranscriptStatus::Failed),
                TranscriptChange::RawText(Some("hello world".to_owned())),
                TranscriptChange::FinalText(Some("Hello, world.".to_owned())),
                TranscriptChange::DurationMs(2_500),
                TranscriptChange::SpeechMs(1_800),
                TranscriptChange::WordCount(2),
                TranscriptChange::EngineId(EngineId::from_static("other-engine")),
                TranscriptChange::PolisherIds(vec![
                    EngineId::from_static("rules"),
                    EngineId::from_static("qwen3-1.7b"),
                ]),
                TranscriptChange::Language(Some(Language::from_static("en"))),
                TranscriptChange::LatencyMs(180),
                TranscriptChange::AppName(Some("code.exe".to_owned())),
                TranscriptChange::ErrorCode(Some(AppErrorCode::Asr)),
            ],
        )
        .unwrap();
        let stored = get::get(&db, take.id).unwrap();
        assert_eq!(stored.status, TranscriptStatus::Failed);
        assert_eq!(stored.raw_text.as_deref(), Some("hello world"));
        assert_eq!(stored.final_text.as_deref(), Some("Hello, world."));
        assert_eq!(
            (
                stored.duration_ms,
                stored.speech_ms,
                stored.word_count,
                stored.latency_ms
            ),
            (Some(2_500), Some(1_800), Some(2), Some(180))
        );
        assert_eq!(
            stored.engine_id,
            Some(EngineId::from_static("other-engine"))
        );
        assert_eq!(
            stored.polisher_ids,
            [
                EngineId::from_static("rules"),
                EngineId::from_static("qwen3-1.7b")
            ]
        );
        assert_eq!(stored.language, Some(Language::from_static("en")));
        assert_eq!(stored.app_name.as_deref(), Some("code.exe"));
        assert_eq!(stored.error_code, Some(AppErrorCode::Asr));

        update(
            &db,
            take.id,
            &[
                TranscriptChange::AudioPath(None),
                TranscriptChange::ErrorCode(None),
                TranscriptChange::Language(None),
                TranscriptChange::PolisherIds(Vec::new()),
            ],
        )
        .unwrap();
        let cleared = get::get(&db, take.id).unwrap();
        assert!(!cleared.has_audio);
        assert_eq!(cleared.error_code, None);
        assert_eq!(cleared.language, None);
        assert!(cleared.polisher_ids.is_empty());
        assert_eq!(cleared.final_text.as_deref(), Some("Hello, world."));
    }

    #[test]
    fn a_missing_take_is_not_found_and_an_empty_update_is_a_no_op() {
        let db = Db::open_in_memory().unwrap();
        let error = update(
            &db,
            id_at(1),
            &[TranscriptChange::Status(TranscriptStatus::Done)],
        )
        .unwrap_err();
        assert_eq!(
            error.error(),
            &AppError::NotFound {
                resource: ResourceKind::Transcript
            }
        );
        assert!(update(&db, id_at(1), &[]).is_ok());
    }

    #[test]
    fn update_each_changes_every_listed_take_and_skips_missing_ones() {
        let db = Db::open_in_memory().unwrap();
        let first = done(&db, 1_000, "First note.", 2, 100);
        let second = done(&db, 2_000, "Second note.", 2, 100);
        let untouched = done(&db, 3_000, "Third note.", 2, 100);
        let changed = update_each(
            &db,
            &[first, second, id_at(9_000)],
            &[
                TranscriptChange::FinalText(None),
                TranscriptChange::ClearedAt(UnixMs::from_millis(5_000)),
            ],
        )
        .unwrap();
        assert_eq!(changed, 2);
        assert_eq!(get::get(&db, first).unwrap().final_text, None);
        assert_eq!(get::get(&db, second).unwrap().word_count, Some(2));
        let still_listed = list::list(
            &db,
            &TranscriptSelector {
                cleared: Some(false),
                ..TranscriptSelector::default()
            },
            None,
            None,
            NonZeroU32::MIN.saturating_add(9),
        )
        .unwrap();
        assert_eq!(
            still_listed
                .items
                .iter()
                .map(|row| row.id)
                .collect::<Vec<_>>(),
            [untouched]
        );
        let searched = list::list(
            &db,
            &TranscriptSelector::default(),
            Some("note"),
            None,
            NonZeroU32::MIN.saturating_add(9),
        )
        .unwrap();
        assert_eq!(
            searched.items.len(),
            1,
            "cleared text leaves the search index"
        );
        assert_eq!(
            update_each(&db, &[], &[TranscriptChange::FinalText(None)]).unwrap(),
            0
        );
        assert_eq!(update_each(&db, &[first], &[]).unwrap(), 0);
    }
}
