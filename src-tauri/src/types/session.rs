/*!
 * SOURCE OF TRUTH KEYWORDS: SessionView, SessionStatus, SessionUiInput, DeliveryOutcome, SessionRehearsal, take state, pill state, session_input, session_rehearse
 * WHAT:  The read-only projection of the session state machine that the UI renders (SessionView), the UI-only
 *        input enum of `session_input` (SessionUiInput), how a finished take was delivered (DeliveryOutcome) and
 *        what the session rehearses for onboarding (SessionRehearsal, `session_rehearse`).
 * WHY:   The session actor is the sole owner of recording state (02 §5); the UI only ever receives this view in
 *        `SessionStateChanged` and never keeps its own copy. SessionUiInput holds only UI-originated inputs, so the
 *        UI cannot fake a hotkey, timer or worker input (05 decision log). The internal state machine types
 *        (state, inputs, effects) are in types/session_machine.rs; this view is projected from them.
 * WHERE: Built by SessionPhase::view (types/session_machine.rs) in pipeline/session; emitted by the
 *        SessionStateChanged event; rendered by the pill (src/pill) and read once through `session_get_state`.
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
    /// Capture stopped: the tail segment is being transcribed.
    Finalizing,
    /// The joined text is being polished and delivered to the clipboard and the target app.
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
    /// A rehearsed take (onboarding's practice take): the text stays in Echo for the window that asked for the
    /// rehearsal to show; neither the clipboard nor any app was touched.
    Shown,
}

/// Everything the UI may know about the current take.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SessionView {
    pub status: SessionStatus,
    /// The take's row (from `Arming` on); None when no row exists (a missing model, a discarded take).
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

impl SessionStatus {
    /// A take is between its hotkey press and its settled result: its row and journal may still be written.
    pub const fn is_in_progress(self) -> bool {
        matches!(
            self,
            Self::Arming
                | Self::Recording
                | Self::CancelPending
                | Self::Finalizing
                | Self::Delivering
        )
    }
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

    /**
     * SOURCE OF TRUTH KEYWORDS: live take, take in progress, is_live, history action guard
     * WHAT:  True when `id` is the take the session is still recording, transcribing or delivering.
     * WHY:   The session actor owns that take's row and WAV until it settles (02 §5); deleting or retrying it from
     *        History meanwhile would race the actor's writes, so those commands refuse it with `Busy`.
     * WHERE: history_delete and session_retry (pipeline/history.rs, pipeline/retry.rs callers).
     */
    pub fn is_live(&self, id: TranscriptId) -> bool {
        self.status.is_in_progress() && self.transcript_id == Some(id)
    }
}

/// Session inputs the UI is allowed to send through `session_input`; every variant is valid as sent, so the
/// factory's validation only checks the shape (serde already refused anything else).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type, garde::Validate,
)]
#[serde(rename_all = "snake_case")]
pub enum SessionUiInput {
    /// The pill's stop button: same effect as pressing the record hotkey while recording.
    Stop,
}

/**
 * SOURCE OF TRUTH KEYWORDS: SessionRehearsal, session_rehearse, rehearsal mode, hotkey test, practice take, onboarding try it, keep text in Echo
 * WHAT:  What the session rehearses while Echo's own window has focus: nothing (Off), the hotkeys (every press is
 *        reported as HotkeyRehearsed and starts no take), or a take (it runs as usual, but its text stays in Echo,
 *        DeliveryOutcome::Shown, instead of being pasted or copied).
 * WHY:   Onboarding must prove the hotkey and the whole take work before the user relies on them (01 §7) without
 *        recording a test press or pasting into Echo itself. A rehearsal only ever applies while an Echo window is
 *        in the foreground, so one left on by a window that closed or hid can never swallow a hotkey or keep text
 *        from the app the user dictates into. It is session configuration, not recording state: the machine is
 *        untouched, the actor only decides what a hotkey or a delivery means (02 §5 stays the one owner).
 * WHERE: Sent by `session_rehearse` (ipc/commands/session.rs) from onboarding's hotkey and practice steps; held by
 *        the session actor (pipeline/session/rehearsal.rs).
 */
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Type, garde::Validate,
)]
#[serde(rename_all = "snake_case")]
pub enum SessionRehearsal {
    /// Hotkeys and takes behave normally.
    #[default]
    Off,
    /// Hotkey presses are reported (HotkeyRehearsed) and start nothing.
    Hotkey,
    /// Takes run fully and their text is shown in Echo instead of delivered.
    Take,
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
    fn rehearsal_is_off_by_default_and_snake_case_on_the_wire() {
        assert_eq!(SessionRehearsal::default(), SessionRehearsal::Off);
        for (rehearsal, wire) in [
            (SessionRehearsal::Off, "off"),
            (SessionRehearsal::Hotkey, "hotkey"),
            (SessionRehearsal::Take, "take"),
        ] {
            assert_eq!(serde_json::to_value(rehearsal).unwrap(), json!(wire));
        }
        assert_eq!(
            serde_json::to_value(DeliveryOutcome::Shown).unwrap(),
            json!("shown")
        );
    }

    #[test]
    fn only_an_unsettled_take_with_that_id_is_live() {
        let id = TranscriptId::generate();
        let view = |status| SessionView {
            status,
            transcript_id: Some(id),
            ..SessionView::IDLE
        };
        for status in [
            SessionStatus::Arming,
            SessionStatus::Recording,
            SessionStatus::CancelPending,
            SessionStatus::Finalizing,
            SessionStatus::Delivering,
        ] {
            assert!(view(status).is_live(id), "{status:?}");
            assert!(
                !view(status).is_live(TranscriptId::generate()),
                "{status:?}"
            );
        }
        for status in [
            SessionStatus::Idle,
            SessionStatus::Done,
            SessionStatus::Discarded,
            SessionStatus::Failed,
        ] {
            assert!(!view(status).is_live(id), "{status:?}");
        }
        assert!(!SessionView::IDLE.is_live(id));
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
