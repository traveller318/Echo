/*!
 * SOURCE OF TRUTH KEYWORDS: SessionView, SessionStatus, SessionUiInput, DeliveryOutcome, take state, pill state, session_input
 * WHAT:  The read-only projection of the session state machine that the UI renders (SessionView), the UI-only
 *        input enum of `session_input` (SessionUiInput) and how a finished take was delivered (DeliveryOutcome).
 * WHY:   The session actor is the sole owner of recording state (02 §5); the UI only ever receives this view in
 *        `SessionStateChanged` and never keeps its own copy. SessionUiInput holds only UI-originated inputs, so the
 *        UI cannot fake a hotkey, timer or worker input (05 decision log). The internal state machine types
 *        (state, inputs, effects) belong to pipeline/session.
 * WHERE: Built by pipeline/session; emitted by the SessionStateChanged event; rendered by the pill (src/pill)
 *        and read once through `session_get_state`.
 */

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{AppError, TranscriptId};

/// Where a take is in its lifecycle (02 §5 diagram).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Idle,
    /// Hotkey pressed: the row is inserted and the mic is opening.
    Arming,
    Recording,
    /// Esc pressed: capture paused while the cancel countdown runs.
    CancelPending,
    /// Capture stopped: tail ASR, segment join and polish are running.
    Finalizing,
    /// Text is going to the clipboard and the target app.
    Delivering,
    Done,
    /// The countdown elapsed: row and audio were deleted.
    Discarded,
    /// The take failed; its audio and row are kept for retry.
    Failed,
}

/// How a finished take reached the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryOutcome {
    /// Pasted into the target app (and kept on the clipboard if the setting is on).
    Pasted,
    /// Only copied: the target could not receive a paste (elevated window, 05 W2) or auto-paste is off.
    Copied,
    /// VAD heard no speech; nothing was pasted (02 §6.1, 05 A4).
    NoSpeech,
}

/// Everything the UI may know about the current take.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SessionView {
    pub status: SessionStatus,
    /// The take's row, once it exists (from `Arming` on).
    pub transcript_id: Option<TranscriptId>,
    /// Recorded time so far, excluding time paused in `CancelPending`.
    pub elapsed_ms: u32,
    /// Time left before a pending cancel discards the take.
    pub countdown_remaining_ms: Option<u32>,
    /// Set when `status` is `Done`.
    pub outcome: Option<DeliveryOutcome>,
    /// Set when `status` is `Failed`.
    pub error: Option<AppError>,
}

impl SessionView {
    /// The view while no take is active.
    pub const IDLE: Self = Self {
        status: SessionStatus::Idle,
        transcript_id: None,
        elapsed_ms: 0,
        countdown_remaining_ms: None,
        outcome: None,
        error: None,
    };
}

/// Session inputs the UI is allowed to send through `session_input`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SessionUiInput {
    /// The pill's stop button: same effect as pressing the record hotkey while recording.
    Stop,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn idle_view_has_no_take_data() {
        assert_eq!(
            serde_json::to_value(SessionView::IDLE).unwrap(),
            json!({
                "status": "idle",
                "transcript_id": null,
                "elapsed_ms": 0,
                "countdown_remaining_ms": null,
                "outcome": null,
                "error": null,
            })
        );
    }

    #[test]
    fn ui_input_accepts_only_stop() {
        assert_eq!(
            serde_json::from_str::<SessionUiInput>("\"stop\"").unwrap(),
            SessionUiInput::Stop
        );
        assert!(serde_json::from_str::<SessionUiInput>("\"record_pressed\"").is_err());
    }

    #[test]
    fn statuses_use_snake_case_on_the_wire() {
        assert_eq!(
            serde_json::to_value(SessionStatus::CancelPending).unwrap(),
            json!("cancel_pending")
        );
        assert_eq!(
            serde_json::to_value(DeliveryOutcome::NoSpeech).unwrap(),
            json!("no_speech")
        );
    }
}
