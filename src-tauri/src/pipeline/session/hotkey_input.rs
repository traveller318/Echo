/*!
 * SOURCE OF TRUTH KEYWORDS: hotkey input, route hotkey, HotkeyRoute, HotkeyEvent to SessionInput, paste-last hotkey, record press stamp, next take id, session policy stamp
 * WHAT:  `route(event, settings)` turns a hotkey event into what the session does with it: a session input for the
 *        machine (RecordPressed stamped with a fresh take id and the policy in effect, RecordReleased,
 *        RecordInterrupted, Esc), a paste-last request, or nothing; `handles(action)` says which actions the
 *        session reacts to, so only those hotkeys are bound.
 * WHY:   The actor matches on the registry's HotkeyAction, never on a hotkey id (root CLAUDE.md §3). Minting the id
 *        and reading the settings are side effects the pure machine cannot do, so they happen here, per press;
 *        a press the machine ignores (debounced, invalid) simply drops them. Paste-last never touches recording state
 *        (02 §5), so it is not a machine input: it takes the same path as `history_paste_last` (Message::PasteLast
 *        through the runner, which owns the clipboard restore). It acts on the press only, and a paste-last chord that
 *        joins a held modifier-only record chord (Ctrl+Alt, then V) also interrupts that record press, so the take it
 *        began is dropped silently while the paste goes ahead. A released Esc means nothing.
 * WHERE: The session actor (actor.rs) for every Message::Hotkey; `handles` filters `bind_always_where` when the
 *        actor prepares and `rebind_setting` when a hotkey setting changes (`binds_hotkey`).
 */

use crate::{
    registry,
    types::{
        HotkeyAction, HotkeyEvent, HotkeySpec, KeyState, SessionInput, SettingsSnapshot,
        TranscriptId,
    },
};

/// What the session does with one hotkey event.
#[derive(Debug, Clone, PartialEq)]
pub enum HotkeyRoute {
    /// Feed this input to the state machine.
    Input(SessionInput),
    /// Deliver the newest completed take again (no caller waits for the answer).
    PasteLast,
}

/// The session reacts to this action's hotkey (and binds it).
pub const fn handles(action: HotkeyAction) -> bool {
    match action {
        HotkeyAction::Record | HotkeyAction::CancelTake | HotkeyAction::PasteLast => true,
    }
}

/// The session binds this registry hotkey: its action is one the session handles. Startup binding and a live
/// rebind from Settings use the same rule, so they never disagree.
pub fn binds_hotkey(spec: &HotkeySpec) -> bool {
    handles(spec.action)
}

/// What `event` means to the session under `settings`; None when it means nothing.
pub fn route(event: &HotkeyEvent, settings: &SettingsSnapshot) -> Option<HotkeyRoute> {
    let action = registry::hotkeys::action_of(&event.id)?;
    let input = match (action, event.state) {
        (HotkeyAction::Record, KeyState::Pressed) => SessionInput::RecordPressed {
            next_take: TranscriptId::generate(),
            policy: registry::settings::session_policy(settings),
        },
        (HotkeyAction::Record, KeyState::Released) => SessionInput::RecordReleased,
        (HotkeyAction::Record, KeyState::Interrupted) => SessionInput::RecordInterrupted,
        (HotkeyAction::CancelTake, KeyState::Pressed) => SessionInput::Esc,
        (HotkeyAction::PasteLast, KeyState::Pressed) => return Some(HotkeyRoute::PasteLast),
        (
            HotkeyAction::CancelTake | HotkeyAction::PasteLast,
            KeyState::Released | KeyState::Interrupted,
        ) => {
            return None;
        }
    };
    Some(HotkeyRoute::Input(input))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        registry::{
            hotkeys::{CANCEL, HOTKEYS, PASTE_LAST, RECORD},
            settings::{self, keys, values},
        },
        types::{HotkeyId, RecordMode, SettingValue, StaticStr},
    };

    fn event(id: HotkeyId, state: KeyState) -> HotkeyEvent {
        HotkeyEvent { id, state }
    }

    fn input(event: &HotkeyEvent, settings: &SettingsSnapshot) -> Option<SessionInput> {
        match route(event, settings) {
            Some(HotkeyRoute::Input(input)) => Some(input),
            _ => None,
        }
    }

    #[test]
    fn a_record_press_starts_a_take_under_the_settings_in_effect() {
        let hold = settings::resolve([(
            keys::HOTKEY_MODE,
            SettingValue::Enum(StaticStr::new(values::HOLD)),
        )]);
        let Some(SessionInput::RecordPressed { next_take, policy }) =
            input(&event(RECORD, KeyState::Pressed), &hold)
        else {
            panic!("a record press is RecordPressed");
        };
        assert_eq!(policy.record_mode, RecordMode::Hold);
        let Some(SessionInput::RecordPressed {
            next_take: second, ..
        }) = input(&event(RECORD, KeyState::Pressed), &hold)
        else {
            panic!("a record press is RecordPressed");
        };
        assert_ne!(next_take, second, "every press offers a fresh take id");
    }

    #[test]
    fn releases_escape_and_paste_last_map_as_the_registry_says() {
        let defaults = settings::defaults();
        let routed = |id: HotkeyId, state| route(&event(id, state), &defaults);
        assert_eq!(
            routed(RECORD, KeyState::Released),
            Some(HotkeyRoute::Input(SessionInput::RecordReleased))
        );
        assert_eq!(
            routed(CANCEL, KeyState::Pressed),
            Some(HotkeyRoute::Input(SessionInput::Esc))
        );
        assert_eq!(
            routed(RECORD, KeyState::Interrupted),
            Some(HotkeyRoute::Input(SessionInput::RecordInterrupted))
        );
        assert_eq!(
            routed(PASTE_LAST, KeyState::Pressed),
            Some(HotkeyRoute::PasteLast)
        );
        for state in [KeyState::Released, KeyState::Interrupted] {
            assert_eq!(routed(CANCEL, state), None);
            assert_eq!(
                routed(PASTE_LAST, state),
                None,
                "paste-last acts on the press only"
            );
        }
        assert_eq!(
            routed(HotkeyId::from_static("gone"), KeyState::Pressed),
            None
        );
    }

    #[test]
    fn the_session_binds_every_hotkey_it_routes() {
        for spec in HOTKEYS {
            assert!(binds_hotkey(spec), "{} is left unbound", spec.id);
        }
    }
}
