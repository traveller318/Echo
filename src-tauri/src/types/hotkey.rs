/*!
 * SOURCE OF TRUTH KEYWORDS: Shortcut, HotkeyEvent, KeyState, HotkeySpec, HotkeyScope, accelerator string, global shortcut, hold-to-talk
 * WHAT:  Shortcut (a key combination in accelerator syntax, e.g. `Ctrl+Alt+Space`), HotkeyEvent (a bound
 *        hotkey was pressed or released) and HotkeySpec / HotkeyScope (a registry hotkey entry and when it is
 *        registered).
 * WHY:   The combination stays text end to end (setting value, registry default, UI input) and only the hotkey
 *        adapter parses it, so a different hotkey backend (a `WH_KEYBOARD_LL` hook, 05 W9) can accept a different
 *        syntax without touching the core; an unparsable combination is `AppError::Hotkey { reason: invalid }`.
 *        Events carry the registry HotkeyId, never the key text, so the session reacts to *which binding* fired.
 *        Release events exist only when `HotkeyCaps.supports_release` is set (hold mode). The scope keeps Esc
 *        from being stolen outside a take (05 W10): `during_session` bindings are registered only while one runs.
 * WHERE: `HotkeyService::register` / `listen` (ports/hotkey.rs); registry/hotkeys entries and defaults; the
 *        session actor turns HotkeyEvent into RecordPressed / RecordReleased / Esc inputs.
 */

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{HotkeyId, SettingKey, StaticStr, ids::static_str_id};

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
