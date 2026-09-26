/*!
 * SOURCE OF TRUTH KEYWORDS: tray registry, TRAY_MENU, tray menu items, Start dictation, Stop dictation, Pause hotkeys, tray tooltip, dictation_item
 * WHAT:  The tray icon's menu (TRAY_MENU, in menu order: Open Echo · Start/Stop dictation, Paste last transcript,
 *        Pause hotkeys · Quit Echo), the dictation item's label and whether it can be clicked in each session status
 *        (`dictation_item`), and the tooltip for the session and hotkey state (`tooltip`).
 * WHY:   02 §9's tray menu is registry data like the nav and the hotkeys, so a new tray action is one entry and its
 *        copy lives in one place. The dictation item reads "Stop dictation" while a take can be stopped (arming or
 *        recording) and is greyed out while one finishes or during the Esc countdown (the pill's Undo is the way
 *        back then), so the label never promises something the machine would ignore.
 * WHERE: app/tray.rs (builds the menu, updates the item and tooltip on SessionStateChanged and HotkeyStatusChanged).
 */

use crate::types::{HotkeyStatus, SessionStatus, StaticStr, TrayAction, TrayItemSpec};

const START_DICTATION: &str = "Start dictation";
const STOP_DICTATION: &str = "Stop dictation";

const fn item(action: TrayAction, label: &'static str, separated: bool) -> TrayItemSpec {
    TrayItemSpec {
        action,
        label: StaticStr::new(label),
        check: false,
        separated,
    }
}

/// The tray menu, top to bottom.
pub const TRAY_MENU: &[TrayItemSpec] = &[
    item(TrayAction::OpenMain, "Open Echo", false),
    item(TrayAction::ToggleDictation, START_DICTATION, true),
    item(TrayAction::PasteLast, "Paste last transcript", false),
    TrayItemSpec {
        action: TrayAction::PauseHotkeys,
        label: StaticStr::new("Pause hotkeys"),
        check: true,
        separated: false,
    },
    item(TrayAction::Quit, "Quit Echo", true),
];

/// The dictation item's label and whether it can be clicked in `status`.
pub const fn dictation_item(status: SessionStatus) -> (&'static str, bool) {
    match status {
        SessionStatus::Arming | SessionStatus::Recording => (STOP_DICTATION, true),
        SessionStatus::CancelPending | SessionStatus::Finalizing | SessionStatus::Delivering => {
            (STOP_DICTATION, false)
        }
        SessionStatus::Idle
        | SessionStatus::Done
        | SessionStatus::Discarded
        | SessionStatus::Failed => (START_DICTATION, true),
    }
}

/// The tray tooltip: the app name, and what Echo is doing when that matters.
pub fn tooltip(status: SessionStatus, hotkeys: HotkeyStatus) -> String {
    let doing = match status {
        SessionStatus::Arming | SessionStatus::Recording | SessionStatus::CancelPending => {
            Some("Recording")
        }
        SessionStatus::Finalizing | SessionStatus::Delivering => Some("Transcribing"),
        SessionStatus::Idle
        | SessionStatus::Done
        | SessionStatus::Discarded
        | SessionStatus::Failed => hotkeys.paused.then_some("Hotkeys paused"),
    };
    doing.map_or_else(|| String::from("Echo"), |doing| format!("Echo · {doing}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tray_action_has_one_entry() {
        for action in TrayAction::ALL {
            assert_eq!(
                TRAY_MENU
                    .iter()
                    .filter(|spec| spec.action == action)
                    .count(),
                1,
                "{action:?}"
            );
        }
        assert!(TRAY_MENU.iter().all(|spec| !spec.label.trim().is_empty()));
        assert!(
            TRAY_MENU
                .iter()
                .find(|spec| spec.action == TrayAction::PauseHotkeys)
                .is_some_and(|spec| spec.check)
        );
    }

    #[test]
    fn the_dictation_item_offers_only_what_the_session_would_do() {
        assert_eq!(dictation_item(SessionStatus::Idle), (START_DICTATION, true));
        assert_eq!(dictation_item(SessionStatus::Done), (START_DICTATION, true));
        assert_eq!(
            dictation_item(SessionStatus::Recording),
            (STOP_DICTATION, true)
        );
        assert_eq!(
            dictation_item(SessionStatus::Arming),
            (STOP_DICTATION, true)
        );
        assert_eq!(
            dictation_item(SessionStatus::CancelPending),
            (STOP_DICTATION, false)
        );
        assert_eq!(
            dictation_item(SessionStatus::Finalizing),
            (STOP_DICTATION, false)
        );
    }

    #[test]
    fn the_tooltip_says_what_echo_is_doing() {
        let running = HotkeyStatus::default();
        let paused = HotkeyStatus {
            paused: true,
            capturing: false,
        };
        assert_eq!(tooltip(SessionStatus::Idle, running), "Echo");
        assert_eq!(
            tooltip(SessionStatus::Idle, paused),
            "Echo · Hotkeys paused"
        );
        assert_eq!(
            tooltip(SessionStatus::Recording, paused),
            "Echo · Recording"
        );
        assert_eq!(
            tooltip(SessionStatus::Delivering, running),
            "Echo · Transcribing"
        );
    }
}
