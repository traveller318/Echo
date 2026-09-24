/*!
 * SOURCE OF TRUTH KEYWORDS: event payloads, AppEvent, SessionStateChanged, AudioLevel, TranscriptSaved, HistoryChanged, MetricsChanged, SettingsChanged, ModelProgress, AppearanceChanged
 * WHAT:  The payload struct of every Rust → UI event in 02 §4.4 (each struct name is the event name), and AppEvent,
 *        the envelope emitters hand to the event sink.
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
    AppearanceView, ByteCount, ModelId, ModelPhase, SessionView, SettingKey, SettingValue,
    TranscriptSummary,
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

/// The theme, transparency or backdrop changed; carries the full view, so windows never merge partial updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct AppearanceChanged(pub AppearanceView);

/**
 * SOURCE OF TRUTH KEYWORDS: AppEvent, any event, event envelope, emit event, EventSink AppEvent, From payload
 * WHAT:  One value that can carry any event payload above; `From<Payload>` for each, so emitters write
 *        `ctx.emit(SettingsChanged { .. })`.
 * WHY:   Commands and the pipeline emit through one `EventSink<AppEvent>` port instead of holding a Tauri handle
 *        or one sink per event type, so they stay testable with a RecordingSink and never import Tauri. The
 *        variant name is the event name; registry/events.rs matches on it exhaustively when it emits, so a new
 *        payload added here without a catalog entry (or the reverse) fails to compile.
 * WHERE: Built by ipc/commands and pipeline/; emitted by app/events.rs (TauriEventSink) through
 *        `registry::events::emit`; recorded by ports/fakes RecordingSink in tests.
 */
#[derive(Debug, Clone, PartialEq)]
pub enum AppEvent {
    SessionStateChanged(SessionStateChanged),
    AudioLevel(AudioLevel),
    TranscriptSaved(TranscriptSaved),
    HistoryChanged(HistoryChanged),
    MetricsChanged(MetricsChanged),
    SettingsChanged(SettingsChanged),
    ModelProgress(ModelProgress),
    AppearanceChanged(AppearanceChanged),
}

/// `From<Payload> for AppEvent` for every payload, so the variant is never named twice at an emit site.
macro_rules! app_event_from {
    ($($payload:ident),* $(,)?) => {
        $(impl From<$payload> for AppEvent {
            fn from(payload: $payload) -> Self {
                Self::$payload(payload)
            }
        })*
    };
}

app_event_from![
    SessionStateChanged,
    AudioLevel,
    TranscriptSaved,
    HistoryChanged,
    MetricsChanged,
    SettingsChanged,
    ModelProgress,
    AppearanceChanged,
];

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
    fn payloads_convert_into_their_own_variant() {
        assert_eq!(
            AppEvent::from(MetricsChanged {}),
            AppEvent::MetricsChanged(MetricsChanged {})
        );
        assert!(matches!(
            AppEvent::from(HistoryChanged {
                reason: HistoryChangeReason::Deleted
            }),
            AppEvent::HistoryChanged(_)
        ));
    }

    #[test]
    fn empty_payload_is_an_object() {
        assert_eq!(serde_json::to_value(MetricsChanged {}).unwrap(), json!({}));
    }
}
