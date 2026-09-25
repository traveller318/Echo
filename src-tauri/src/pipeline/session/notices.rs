/*!
 * SOURCE OF TRUTH KEYWORDS: session toasts, DEVICE_LOST_TOAST, MAX_DURATION_TOAST, TAKE_FAILED_TOAST, START_FAILED_TOAST, HOTKEY_UNAVAILABLE_TOAST, NOTHING_TO_PASTE_TOAST, PASTE_LAST_FAILED_TOAST, paste_last_toast, stop_toast, failure toast
 * WHAT:  The native toasts a take can raise: the microphone went away (05 W12), the longest take was reached, the
 *        take failed after recording, it could not start, or the dictation hotkey could not be bound; and the ones
 *        the paste-last hotkey raises when nothing could be pasted (no finished take yet, or a failure).
 * WHY:   The pill disappears after a few seconds and may be on another monitor, so a stop the user did not ask for
 *        and every failure also leave a toast (01 "no silent failures"). Copy is calm and never contains transcript
 *        text (types/notification.rs). A failed take keeps its audio (02 §5), so its toast points to History for a
 *        retry; a take that never started has no audio, so its toast points to the microphone instead. The
 *        paste-last hotkey has no window to answer in (History's button shows its own error), so its failures toast;
 *        a copy-instead fallback is already toasted by pipeline/delivery.rs.
 * WHERE: Chosen by pipeline/session/transition.rs as SessionEffect::Toast (the hotkey one by runner.rs when it
 *        prepares); shown by the session actor's runner through the Notifier port.
 */

use crate::types::{AppError, ResourceKind, StaticStr, StopCause, Toast, ToastKind};

/// The microphone disappeared mid-take; what was said is still delivered.
pub const DEVICE_LOST_TOAST: Toast = Toast {
    kind: ToastKind::Warning,
    title: StaticStr::new("Microphone disconnected"),
    body: StaticStr::new("Echo kept what you said before it went away."),
};

/// Recording reached `session.max_duration_min` and stopped by itself.
pub const MAX_DURATION_TOAST: Toast = Toast {
    kind: ToastKind::Info,
    title: StaticStr::new("Recording limit reached"),
    body: StaticStr::new("Echo stopped recording and is transcribing what you said."),
};

/// The take failed after audio was captured; the audio is kept for a retry.
pub const TAKE_FAILED_TOAST: Toast = Toast {
    kind: ToastKind::Error,
    title: StaticStr::new("Couldn't finish that dictation"),
    body: StaticStr::new("Your recording is saved in History, where you can retry it."),
};

/// The take could not start (the row or the microphone failed).
pub const START_FAILED_TOAST: Toast = Toast {
    kind: ToastKind::Error,
    title: StaticStr::new("Couldn't start dictation"),
    body: StaticStr::new("Check your microphone in Settings, then try again."),
};

/// A hotkey the session needs (dictation or paste-last) could not be bound, or hotkeys cannot be received at all.
/// Echo keeps running; the combination can be changed in Settings.
pub const HOTKEY_UNAVAILABLE_TOAST: Toast = Toast {
    kind: ToastKind::Warning,
    title: StaticStr::new("Hotkey unavailable"),
    body: StaticStr::new(
        "Echo couldn't set up one of its hotkeys. Restart Echo, or choose a different one in Settings.",
    ),
};

/// The paste-last hotkey found no finished take to paste.
pub const NOTHING_TO_PASTE_TOAST: Toast = Toast {
    kind: ToastKind::Info,
    title: StaticStr::new("Nothing to paste yet"),
    body: StaticStr::new("Dictate something first, then this hotkey pastes it again."),
};

/// The paste-last hotkey could not deliver the last take (the clipboard stayed busy, the database failed).
pub const PASTE_LAST_FAILED_TOAST: Toast = Toast {
    kind: ToastKind::Error,
    title: StaticStr::new("Couldn't paste your last transcript"),
    body: StaticStr::new("Try again, or copy it from History."),
};

/// The toast a failed paste-last hotkey press shows.
pub fn paste_last_toast(error: &AppError) -> Toast {
    match error {
        AppError::NotFound {
            resource: ResourceKind::TranscriptText | ResourceKind::Transcript,
        } => NOTHING_TO_PASTE_TOAST,
        _ => PASTE_LAST_FAILED_TOAST,
    }
}

/// The toast a stop for `cause` shows; None for the stops the user asked for.
pub fn stop_toast(cause: StopCause) -> Option<Toast> {
    match cause {
        StopCause::DeviceLost => Some(DEVICE_LOST_TOAST),
        StopCause::MaxDuration => Some(MAX_DURATION_TOAST),
        StopCause::Hotkey | StopCause::Released | StopCause::Ui => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_unrequested_stops_toast() {
        assert_eq!(stop_toast(StopCause::DeviceLost), Some(DEVICE_LOST_TOAST));
        assert_eq!(stop_toast(StopCause::MaxDuration), Some(MAX_DURATION_TOAST));
        for cause in [StopCause::Hotkey, StopCause::Released, StopCause::Ui] {
            assert_eq!(stop_toast(cause), None);
        }
    }

    #[test]
    fn an_empty_history_is_told_apart_from_a_failed_paste() {
        let nothing = AppError::NotFound {
            resource: ResourceKind::TranscriptText,
        };
        assert_eq!(paste_last_toast(&nothing), NOTHING_TO_PASTE_TOAST);
        assert_eq!(
            paste_last_toast(&AppError::Internal),
            PASTE_LAST_FAILED_TOAST
        );
    }
}
