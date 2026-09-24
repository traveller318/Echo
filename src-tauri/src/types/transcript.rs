/*!
 * SOURCE OF TRUTH KEYWORDS: Transcript, TranscriptSummary, TranscriptStatus, Page, PageCursor, history row, pagination
 * WHAT:  The history domain shapes: one take in full (Transcript), its list row (TranscriptSummary), its status,
 *        and the generic cursor-paginated Page<T>.
 * WHY:   Mirrors the `transcripts` table (02 §7.2) minus storage detail: the audio path is not exposed, only
 *        whether audio is kept (`has_audio`), because the WAV location is derived from the id by the pipeline.
 *        TranscriptStatus serializes exactly as the `status` column stores it. PageCursor is opaque so the
 *        keyset encoding can change inside services without touching the UI.
 * WHERE: Returned by history commands, carried by the TranscriptSaved event, built by services/transcripts.
 */

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{AppErrorCode, EngineId, Language, TranscriptId, UnixMs};

/// Lifecycle of a stored take; the wire form equals the `transcripts.status` column value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptStatus {
    /// Row inserted, audio being captured.
    Recording,
    /// Capture finished, ASR or polish running.
    Transcribing,
    Done,
    /// No speech was detected; nothing was pasted.
    Empty,
    Failed,
    /// Found unfinished at startup; the WAV was repaired and can be retried (02 §7.3).
    Recoverable,
}

impl TranscriptStatus {
    pub const ALL: [Self; 6] = [
        Self::Recording,
        Self::Transcribing,
        Self::Done,
        Self::Empty,
        Self::Failed,
        Self::Recoverable,
    ];

    /// The `transcripts.status` column value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Recording => "recording",
            Self::Transcribing => "transcribing",
            Self::Done => "done",
            Self::Empty => "empty",
            Self::Failed => "failed",
            Self::Recoverable => "recoverable",
        }
    }

    /// Parses a `transcripts.status` column value.
    pub fn parse(status: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == status)
    }

    /// Retention never deletes the audio of these takes (02 §7.3).
    pub const fn keeps_audio(self) -> bool {
        matches!(self, Self::Failed | Self::Recoverable)
    }
}

/// One history list row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct TranscriptSummary {
    pub id: TranscriptId,
    pub created_at: UnixMs,
    pub status: TranscriptStatus,
    /// Start of the final text, trimmed for the list; None until the take is transcribed.
    pub preview: Option<String>,
    pub duration_ms: Option<u32>,
    pub word_count: Option<u32>,
    /// Executable name of the app the text was delivered to.
    pub app_name: Option<String>,
    pub error_code: Option<AppErrorCode>,
    pub has_audio: bool,
}

/// One take in full.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Transcript {
    pub id: TranscriptId,
    pub created_at: UnixMs,
    pub status: TranscriptStatus,
    /// Joined ASR output before polish.
    pub raw_text: Option<String>,
    /// Delivered text after the polish chain.
    pub final_text: Option<String>,
    /// The WAV journal is still on disk, so the take can be retried.
    pub has_audio: bool,
    pub duration_ms: Option<u32>,
    /// Speech time as detected by VAD, used for speaking WPM.
    pub speech_ms: Option<u32>,
    pub word_count: Option<u32>,
    pub engine_id: Option<EngineId>,
    /// Polishers that ran, in chain order.
    pub polisher_ids: Vec<EngineId>,
    pub language: Option<Language>,
    /// Stop → delivered, in milliseconds.
    pub latency_ms: Option<u32>,
    pub app_name: Option<String>,
    pub error_code: Option<AppErrorCode>,
}

/// Opaque position after the last item of a page; pass it back to fetch the next page.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct PageCursor(String);

impl PageCursor {
    pub fn new(encoded: impl Into<String>) -> Self {
        Self(encoded.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A slice of a longer list, newest first unless the command says otherwise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Page<T> {
    pub items: Vec<T>,
    /// None when this is the last page.
    pub next_cursor: Option<PageCursor>,
}

impl<T> Page<T> {
    pub fn new(items: Vec<T>, next_cursor: Option<PageCursor>) -> Self {
        Self { items, next_cursor }
    }

    /// A page with nothing after it.
    pub fn last(items: Vec<T>) -> Self {
        Self::new(items, None)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn status_wire_form_matches_the_column_value() {
        for status in TranscriptStatus::ALL {
            assert_eq!(
                serde_json::to_value(status).unwrap(),
                json!(status.as_str())
            );
            assert_eq!(TranscriptStatus::parse(status.as_str()), Some(status));
        }
        assert_eq!(TranscriptStatus::parse("deleted"), None);
    }

    #[test]
    fn only_failed_and_recoverable_takes_keep_audio() {
        let kept: Vec<_> = TranscriptStatus::ALL
            .into_iter()
            .filter(|status| status.keeps_audio())
            .collect();
        assert_eq!(
            kept,
            [TranscriptStatus::Failed, TranscriptStatus::Recoverable]
        );
    }

    #[test]
    fn page_serializes_items_and_cursor() {
        let page = Page::new(vec![1_u32, 2], Some(PageCursor::new("abc")));
        assert_eq!(
            serde_json::to_value(&page).unwrap(),
            json!({ "items": [1, 2], "next_cursor": "abc" })
        );
        assert_eq!(
            serde_json::to_value(Page::last(Vec::<u32>::new())).unwrap(),
            json!({ "items": [], "next_cursor": null })
        );
    }
}
