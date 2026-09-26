/*!
 * SOURCE OF TRUTH KEYWORDS: Shortcut, HotkeyEvent, KeyState, HotkeySpec, HotkeyScope, HotkeyAction, HotkeyBindFailure, RecordMode, accelerator string, hold-to-talk
 * WHAT:  Shortcut (a key combination in accelerator syntax, e.g. `Ctrl+Alt+Space`), HotkeyEvent (a bound
 *        hotkey was pressed or released), HotkeySpec / HotkeyScope / HotkeyAction (a registry hotkey entry, when
 *        it is registered and what pressing it does), HotkeyBindFailure (a hotkey that could not be bound, e.g.
 *        another app owns it) and RecordMode (whether the record hotkey toggles a take or holds it).
 * WHY:   The combination stays text end to end (setting value, registry default, UI input) and only the hotkey
 *        adapter parses it, so a different hotkey backend (a `WH_KEYBOARD_LL` hook, 05 W9) can accept a different
 *        syntax without touching the core; an unparsable combination is `AppError::Hotkey { reason: invalid }`.
 *        Events carry the registry HotkeyId, never the key text, so the session reacts to *which binding* fired;
 *        what a binding does is its HotkeyAction, declared in the registry entry, so the session actor matches on
 *        the action and never on an id (root CLAUDE.md §3).
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

/**
 * SOURCE OF TRUTH KEYWORDS: KeyState, Pressed, Released, Interrupted, modifier-only chord, chord interrupted, part of another shortcut
 * WHAT:  What happened to a bound combination: it went down, came back up, or turned out to be the start of a
 *        longer shortcut (Interrupted).
 * WHY:   A modifier-only combination such as Ctrl+Alt fires the moment it is held (hold-to-talk must not wait), but
 *        the same keys start other apps' shortcuts (Ctrl+Alt+T, Ctrl+Alt+Del). When another key joins while it is
 *        held, the adapter reports Interrupted instead of Released, so the session can drop a take that was never
 *        meant (05 W9). Only adapters with `HotkeyCaps.supports_modifier_only` report it; after Interrupted no
 *        Released follows for that press.
 *        It crosses IPC inside HotkeyRehearsed, so onboarding can show a press, a release or an interruption.
 * WHERE: HotkeyEvent.state, built by the hotkey adapters; mapped to session inputs by
 *        pipeline/session/hotkey_input.rs; reported as-is by a hotkey rehearsal (pipeline/session/rehearsal.rs).
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum KeyState {
    Pressed,
    Released,
    /// Another key joined a held modifier-only combination: the press was part of another shortcut.
    Interrupted,
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
 * SOURCE OF TRUTH KEYWORDS: HotkeyAction, hotkey action, record action, cancel take action, paste last action, hotkey routing
 * WHAT:  What pressing (and releasing) a registry hotkey does: start or stop a take, cancel it with the Esc
 *        countdown, or paste the last transcript.
 * WHY:   The session actor turns a HotkeyEvent into a session input by the action its registry entry declares, so
 *        a new hotkey that reuses an action is one registry entry, and a new action fails to compile until the
 *        actor says what it does. The actor binds only the Always-scoped hotkeys whose action it handles, so a
 *        combination is never taken from other apps while pressing it would do nothing.
 * WHERE: HotkeySpec.action (registry/hotkeys.rs); read by the session actor (pipeline/session/hotkey_input.rs)
 *        through registry::hotkeys::find.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyAction {
    /// Starts a take, or stops it (toggle mode); in hold mode the release stops it.
    Record,
    /// The Esc countdown: a first press pauses and counts down, a second one undoes (05 W10).
    CancelTake,
    /// Pastes the most recent delivered transcript again.
    PasteLast,
}

/**
 * SOURCE OF TRUTH KEYWORDS: RecordMode, toggle mode, hold mode, hold-to-talk, hotkeys.mode, record hotkey behaviour
 * WHAT:  How the record hotkey drives a take: Toggle (press starts, the next press stops) or Hold (recording lasts
 *        while the keys are held; the release stops it).
 * WHY:   The session state machine branches on this value, never on the stored `hotkeys.mode` text, which only the
 *        registry spells (registry/settings/values.rs). Hold needs release events, which exist only when
 *        `HotkeyCaps.supports_release` is set; the setting is hidden otherwise. Hold is the default (hold Ctrl+Alt,
 *        speak, let go: 05 decision log 2026-09-25); Toggle suits a combination with a main key.
 * WHERE: Read from settings by registry::settings::record_mode; carried in SessionPolicy (types/session_machine.rs)
 *        and acted on by pipeline/session/transition.rs.
 */
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum RecordMode {
    Toggle,
    #[default]
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
    /// What pressing it does.
    pub action: HotkeyAction,
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
            action: HotkeyAction::CancelTake,
        };
        assert_eq!(
            serde_json::to_value(&CANCEL).unwrap(),
            json!({
                "id": "cancel",
                "label": "Cancel take",
                "default_accelerator": "Escape",
                "setting_key": null,
                "scope": "during_session",
                "action": "cancel_take",
            })
        );
    }
}
