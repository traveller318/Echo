/*!
 * SOURCE OF TRUTH KEYWORDS: event payloads, SessionStateChanged, AudioLevel, TranscriptSaved, HistoryChanged, MetricsChanged, SettingsChanged, ModelProgress
 * WHAT:  The payload struct of every Rust → UI event in 02 §4.4. Each struct name is the event name.
 * WHY:   Rust owns domain state and pushes it as typed events; the UI reads once through a command and then stays
 *        fresh from these, never polling (02 §4.4). The payloads are plain data here; the Tauri event wiring
 *        (`tauri_specta::Event` impls and the catalog) lives in registry/events.rs so this layer stays free of
 *        framework traits.
 * WHERE: Emitted by pipeline/ and ipc/commands through the registry event catalog; received in the UI through
 *        `events.<name>` in the generated src/bindings.ts.
 */

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{
    ByteCount, ModelId, ModelPhase, SessionView, SettingKey, SettingValue, TranscriptSummary,
};

/// The session changed state; carries the full view, so the UI never merges partial updates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SessionStateChanged(pub SessionView);

/// Input level while recording, at most 30 Hz.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct AudioLevel {
    /// Root mean square of the last frame, 0 to 1.
    pub rms: f32,
}

/// A take was written to history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct TranscriptSaved(pub TranscriptSummary);

/// Why the history list changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum HistoryChangeReason {
    Inserted,
    Updated,
    Deleted,
    /// Startup recovery marked unfinished takes recoverable.
    Recovered,
    /// A retention sweep removed audio or rows.
    Retention,
}

/// History rows changed; list queries should refetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct HistoryChanged {
    pub reason: HistoryChangeReason,
}

/// Dashboard aggregates changed; metric queries should refetch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct MetricsChanged {}

/// A setting was written or reset; `value` is the effective value after the change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SettingsChanged {
    pub key: SettingKey,
    pub value: SettingValue,
}

/// Progress of a model download or import, at most 10 Hz.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ModelProgress {
    pub model_id: ModelId,
    pub bytes: ByteCount,
    pub total: ByteCount,
    pub phase: ModelPhase,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn newtype_payloads_serialize_as_their_inner_value() {
        assert_eq!(
            serde_json::to_value(SessionStateChanged(SessionView::IDLE)).unwrap(),
            serde_json::to_value(SessionView::IDLE).unwrap()
        );
    }

    #[test]
    fn empty_payload_is_an_object() {
        assert_eq!(serde_json::to_value(MetricsChanged {}).unwrap(), json!({}));
    }
}
