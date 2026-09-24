/*!
 * SOURCE OF TRUTH KEYWORDS: Shortcut, HotkeyEvent, KeyState, HotkeySpec, HotkeyScope, HotkeyBindFailure, RecordMode, accelerator string, global shortcut, hold-to-talk
 * WHAT:  Shortcut (a key combination in accelerator syntax, e.g. `Ctrl+Alt+Space`), HotkeyEvent (a bound
 *        hotkey was pressed or released), HotkeySpec / HotkeyScope (a registry hotkey entry and when it is
 *        registered), HotkeyBindFailure (a hotkey that could not be bound, e.g. another app owns it) and
 *        RecordMode (whether the record hotkey toggles a take or holds it).
 * WHY:   The combination stays text end to end (setting value, registry default, UI input) and only the hotkey
 *        adapter parses it, so a different hotkey backend (a `WH_KEYBOARD_LL` hook, 05 W9) can accept a different
 *        syntax without touching the core; an unparsable combination is `AppError::Hotkey { reason: invalid }`.
 *        Events carry the registry HotkeyId, never the key text, so the session reacts to *which binding* fired.
 *        Release events exist only when `HotkeyCaps.supports_release` is set (hold mode). The scope keeps Esc
 *        from being stolen outside a take (05 W10): `during_session` bindings are registered only while one runs.
 *        A failed startup binding is reported per hotkey instead of aborting the rest (05 W7: keep going, badge the
 *        tray, show it in Settings).
 * WHERE: `HotkeyService::register` / `listen` (ports/hotkey.rs); registry/hotkeys entries and defaults; the
 *        session actor turns HotkeyEvent into RecordPressed / RecordReleased / Esc inputs; pipeline/hotkeys.rs
 *        returns HotkeyBindFailure.
 */

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{HotkeyId, PortError, SettingKey, StaticStr, ids::static_str_id};

static_str_id! {
    /// A key combination in accelerator syntax, e.g. `Ctrl+Alt+Space`. Parsed only by the hotkey adapter.
    Shortcut
}

/// Whether a bound combination went down or came back up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyState {
    Pressed,
    Released,
}

/// A registered hotkey fired.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HotkeyEvent {
    pub id: HotkeyId,
    pub state: KeyState,
}

/// When a hotkey is registered with the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyScope {
    /// From startup until exit.
    Always,
    /// Only while a take is recording or counting down to cancel.
    DuringSession,
}

/**
 * SOURCE OF TRUTH KEYWORDS: RecordMode, toggle mode, hold mode, hold-to-talk, hotkeys.mode, record hotkey behaviour
 * WHAT:  How the record hotkey drives a take: Toggle (press starts, the next press stops) or Hold (recording lasts
 *        while the keys are held; the release stops it).
 * WHY:   The session state machine branches on this value, never on the stored `hotkeys.mode` text, which only the
 *        registry spells (registry/settings/values.rs). Hold needs release events, which exist only when
 *        `HotkeyCaps.supports_release` is set; the setting is hidden otherwise, so Toggle is the default.
 * WHERE: Read from settings by registry::settings::record_mode; carried in SessionPolicy (types/session_machine.rs)
 *        and acted on by pipeline/session/transition.rs.
 */
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum RecordMode {
    #[default]
    Toggle,
    Hold,
}

/// A registry hotkey entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct HotkeySpec {
    pub id: HotkeyId,
    pub label: StaticStr,
    pub default_accelerator: Shortcut,
    /// The Hotkey setting the user rebinds it with; None when the combination is fixed.
    pub setting_key: Option<SettingKey>,
    pub scope: HotkeyScope,
}

/// A hotkey that could not be bound to its combination; the rest of the hotkeys are bound regardless.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyBindFailure {
    pub id: HotkeyId,
    pub shortcut: Shortcut,
    /// `Hotkey { conflict | invalid }` from the adapter, with its log-only detail.
    pub error: PortError,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn specs_are_const_and_serialize_plainly() {
        const CANCEL: HotkeySpec = HotkeySpec {
            id: HotkeyId::from_static("cancel"),
            label: StaticStr::new("Cancel take"),
            default_accelerator: Shortcut::from_static("Escape"),
            setting_key: None,
            scope: HotkeyScope::DuringSession,
        };
        assert_eq!(
            serde_json::to_value(&CANCEL).unwrap(),
            json!({
                "id": "cancel",
                "label": "Cancel take",
                "default_accelerator": "Escape",
                "setting_key": null,
                "scope": "during_session",
            })
        );
    }
}
