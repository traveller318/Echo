/*!
 * SOURCE OF TRUTH KEYWORDS: transcripts service, history table, take rows, transcripts verbs, insert update get list delete aggregate
 * WHAT:  The `transcripts` table's verbs, one file each: insert, update, get, list (history page and bulk
 *        select), delete (one row or a selection) and aggregate (raw sums for the dashboard metrics), plus the
 *        shared row codec and selector condition they build on.
 * WHY:   One verb, one table, no business rules, no call into another service (root CLAUDE.md §3): statuses,
 *        retention cut-offs and metric formulas are decided by the pipeline and commands, which pass the values
 *        in. The FTS index is maintained by the migration's triggers, so no verb here writes `transcripts_fts`.
 * WHERE: `use crate::services::transcripts` from pipeline/ (session actor, recovery, retention) and ipc/commands
 *        (history, metrics).
 */

pub mod aggregate;
pub mod delete;
pub mod get;
pub mod insert;
pub mod list;
mod row;
mod selector;
pub mod update;

/**
 * SOURCE OF TRUTH KEYWORDS: transcripts test fixtures, sample take, id_at, completed take
 * WHAT:  Builders for the service tests: a take id stamped at a given millisecond, a fresh `recording` row and a
 *        completed (`done`) take with text and measurements.
 * WHY:   ULIDs made within one millisecond are not ordered, so tests that assert newest-first order stamp each id
 *        at its own time; completing a take goes through the real insert and update verbs.
 * WHERE: Tests of every verb in this folder.
 */
#[cfg(test)]
mod fixtures {
    use ulid::Ulid;

    use super::{insert, update};
    use crate::{
        services::db::Db,
        types::{
            AppPaths, EngineId, NewTranscript, TranscriptChange, TranscriptId, TranscriptStatus,
            UnixMs,
        },
    };

    pub fn id_at(millis: u64) -> TranscriptId {
        Ulid::from_parts(millis, 7).to_string().parse().unwrap()
    }

    pub fn new_take(millis: u64) -> NewTranscript {
        let id = id_at(millis);
        NewTranscript {
            id,
            created_at: UnixMs::from_millis(i64::try_from(millis).unwrap()),
            status: TranscriptStatus::Recording,
            audio_path: Some(AppPaths::recording_name(id)),
            engine_id: Some(EngineId::from_static("parakeet-tdt-0.6b-v3")),
            app_name: Some("notepad.exe".to_owned()),
        }
    }

    /// Inserts a take at `millis` and completes it with `text` and the given measurements.
    pub fn done(db: &Db, millis: u64, text: &str, words: u32, latency_ms: u32) -> TranscriptId {
        let take = new_take(millis);
        insert::insert(db, &take).unwrap();
        update::update(
            db,
            take.id,
            &[
                TranscriptChange::Status(TranscriptStatus::Done),
                TranscriptChange::RawText(Some(text.to_owned())),
                TranscriptChange::FinalText(Some(text.to_owned())),
                TranscriptChange::WordCount(words),
                TranscriptChange::DurationMs(words * 500),
                TranscriptChange::SpeechMs(words * 400),
                TranscriptChange::LatencyMs(latency_ms),
            ],
        )
        .unwrap();
        take.id
    }
}
