/*!
 * SOURCE OF TRUTH KEYWORDS: transcripts row codec, column decode, TranscriptSummary row, Transcript row, TranscriptRef row, column encode, preview
 * WHAT:  The mapping between `transcripts` rows and the types/ shapes: the column lists each read selects, the
 *        decoders that build Transcript, TranscriptSummary and TranscriptRef from a row, and the encoders for
 *        ids, statuses, error codes and polisher lists.
 * WHY:   Every read verb decodes the same columns the same way, so the list, get and select verbs cannot drift
 *        from each other or from the schema (02 §7.2). A column value this build does not understand (a status
 *        written by a newer build, a malformed id) fails the read as a conversion error instead of being guessed.
 *        The audio path never leaves the service as a path for the UI: rows expose `has_audio` only.
 * WHERE: services/transcripts/{insert, update, get, list, delete}.
 */

use rusqlite::{
    Row,
    types::{FromSqlError, Type},
};

use crate::types::{
    AppErrorCode, EngineId, Language, Transcript, TranscriptId, TranscriptRef, TranscriptStatus,
    TranscriptSummary, UnixMs,
};

/// Characters of `final_text` a history row carries as its preview.
pub(super) const PREVIEW_CHARS: u32 = 160;

/// Columns `summary` decodes, in order; `?` is bound to PREVIEW_CHARS. Prefixed for queries that join FTS.
pub(super) const SUMMARY_COLUMNS: &str = "t.id, t.created_at, t.status, substr(t.final_text, 1, ?), \
     t.duration_ms, t.word_count, t.app_name, t.error_code, t.audio_path IS NOT NULL";

/// Columns `transcript` decodes, in order.
pub(super) const TRANSCRIPT_COLUMNS: &str = "id, created_at, status, raw_text, final_text, \
     audio_path IS NOT NULL, duration_ms, speech_ms, word_count, engine_id, polisher_ids, language, \
     latency_ms, app_name, error_code";

/// Columns `reference` decodes, in order.
pub(super) const REF_COLUMNS: &str = "id, created_at, status, audio_path";

pub(super) fn summary(row: &Row<'_>) -> rusqlite::Result<TranscriptSummary> {
    Ok(TranscriptSummary {
        id: id(row, 0)?,
        created_at: UnixMs::from_millis(row.get(1)?),
        status: status(row, 2)?,
        preview: row
            .get::<_, Option<String>>(3)?
            .map(|text| text.trim().to_owned())
            .filter(|text| !text.is_empty()),
        duration_ms: row.get(4)?,
        word_count: row.get(5)?,
        app_name: row.get(6)?,
        error_code: error_code(row, 7)?,
        has_audio: row.get(8)?,
    })
}

pub(super) fn transcript(row: &Row<'_>) -> rusqlite::Result<Transcript> {
    Ok(Transcript {
        id: id(row, 0)?,
        created_at: UnixMs::from_millis(row.get(1)?),
        status: status(row, 2)?,
        raw_text: row.get(3)?,
        final_text: row.get(4)?,
        has_audio: row.get(5)?,
        duration_ms: row.get(6)?,
        speech_ms: row.get(7)?,
        word_count: row.get(8)?,
        engine_id: row.get::<_, Option<String>>(9)?.map(EngineId::from),
        polisher_ids: polisher_ids(row, 10)?,
        language: row.get::<_, Option<String>>(11)?.map(Language::from),
        latency_ms: row.get(12)?,
        app_name: row.get(13)?,
        error_code: error_code(row, 14)?,
    })
}

pub(super) fn reference(row: &Row<'_>) -> rusqlite::Result<TranscriptRef> {
    Ok(TranscriptRef {
        id: id(row, 0)?,
        created_at: UnixMs::from_millis(row.get(1)?),
        status: status(row, 2)?,
        audio_path: row.get(3)?,
    })
}

/// The stored form of a take id (its 26-character ULID text).
pub(super) fn id_text(id: TranscriptId) -> String {
    id.to_string()
}

/// The stored form of a polisher list: a JSON array of engine ids.
pub(super) fn polisher_ids_text(ids: &[EngineId]) -> rusqlite::Result<String> {
    serde_json::to_string(ids)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

fn id(row: &Row<'_>, index: usize) -> rusqlite::Result<TranscriptId> {
    row.get::<_, String>(index)?.parse().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}

fn status(row: &Row<'_>, index: usize) -> rusqlite::Result<TranscriptStatus> {
    TranscriptStatus::parse(&row.get::<_, String>(index)?).ok_or_else(|| unknown(index))
}

fn error_code(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<AppErrorCode>> {
    row.get::<_, Option<String>>(index)?
        .map(|code| AppErrorCode::parse(&code).ok_or_else(|| unknown(index)))
        .transpose()
}

fn polisher_ids(row: &Row<'_>, index: usize) -> rusqlite::Result<Vec<EngineId>> {
    row.get::<_, Option<String>>(index)?
        .map_or(Ok(Vec::new()), |text| {
            serde_json::from_str(&text).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
            })
        })
}

/// A text column holding a value this build does not know.
fn unknown(index: usize) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        Type::Text,
        Box::new(FromSqlError::InvalidType),
    )
}
