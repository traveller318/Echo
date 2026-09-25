/*!
 * SOURCE OF TRUTH KEYWORDS: hotkey input, input_for, handles, HotkeyEvent to SessionInput, record press stamp, next take id, session policy stamp
 * WHAT:  `input_for(event, settings)` turns a hotkey event into the session input its registry action means
 *        (RecordPressed stamped with a fresh take id and the policy in effect, RecordReleased, RecordInterrupted,
 *        Esc), or None;
 *        `handles(action)` says which actions the session reacts to, so only those hotkeys are bound.
 * WHY:   The actor matches on the registry's HotkeyAction, never on a hotkey id (root CLAUDE.md §3). Minting the id
 *        and reading the settings are side effects the pure machine cannot do, so they happen here, per press;
 *        a press the machine ignores (debounced, invalid) simply drops them. Paste-last is not handled yet, so its
 *        combination is left free for other apps until its handler exists (step 19). A released Esc means nothing.
 * WHERE: The session actor (actor.rs) for every Message::Hotkey; `handles` filters `bind_always_where` when the
 *        actor prepares.
 */

use crate::{
    registry,
    types::{
        HotkeyAction, HotkeyEvent, HotkeySpec, KeyState, SessionInput, SettingsSnapshot,
        TranscriptId,
    },
};

/// The session reacts to this action's hotkey (and binds it).
pub const fn handles(action: HotkeyAction) -> bool {
    match action {
        HotkeyAction::Record | HotkeyAction::CancelTake => true,
        HotkeyAction::PasteLast => false,
    }
}

/// The session binds this registry hotkey: its action is one the session handles. Startup binding and a live
/// rebind from Settings use the same rule, so they never disagree.
pub fn binds_hotkey(spec: &HotkeySpec) -> bool {
    handles(spec.action)
}

/// The session input `event` means under `settings`; None when it means nothing to the session.
pub fn input_for(event: &HotkeyEvent, settings: &SettingsSnapshot) -> Option<SessionInput> {
    let action = registry::hotkeys::action_of(&event.id)?;
    match (action, event.state) {
        (HotkeyAction::Record, KeyState::Pressed) => Some(SessionInput::RecordPressed {
            next_take: TranscriptId::generate(),
            policy: registry::settings::session_policy(settings),
        }),
        (HotkeyAction::Record, KeyState::Released) => Some(SessionInput::RecordReleased),
        (HotkeyAction::Record, KeyState::Interrupted) => Some(SessionInput::RecordInterrupted),
        (HotkeyAction::CancelTake, KeyState::Pressed) => Some(SessionInput::Esc),
        (HotkeyAction::CancelTake, KeyState::Released | KeyState::Interrupted)
        | (HotkeyAction::PasteLast, _) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        registry::{
            hotkeys::{CANCEL, PASTE_LAST, RECORD},
            settings::{self, keys, values},
        },
        types::{HotkeyId, RecordMode, SettingValue, StaticStr},
    };

    fn event(id: HotkeyId, state: KeyState) -> HotkeyEvent {
        HotkeyEvent { id, state }
    }

    #[test]
    fn a_record_press_starts_a_take_under_the_settings_in_effect() {
        let hold = settings::resolve([(
            keys::HOTKEY_MODE,
            SettingValue::Enum(StaticStr::new(values::HOLD)),
        )]);
        let Some(SessionInput::RecordPressed { next_take, policy }) =
            input_for(&event(RECORD, KeyState::Pressed), &hold)
        else {
            panic!("a record press is RecordPressed");
        };
        assert_eq!(policy.record_mode, RecordMode::Hold);
        let Some(SessionInput::RecordPressed {
            next_take: second, ..
        }) = input_for(&event(RECORD, KeyState::Pressed), &hold)
        else {
            panic!("a record press is RecordPressed");
        };
        assert_ne!(next_take, second, "every press offers a fresh take id");
    }

    #[test]
    fn releases_escape_and_unhandled_actions_map_as_the_registry_says() {
        let defaults = settings::defaults();
        assert_eq!(
            input_for(&event(RECORD, KeyState::Released), &defaults),
            Some(SessionInput::RecordReleased)
        );
        assert_eq!(
            input_for(&event(CANCEL, KeyState::Pressed), &defaults),
            Some(SessionInput::Esc)
        );
        assert_eq!(
            input_for(&event(RECORD, KeyState::Interrupted), &defaults),
            Some(SessionInput::RecordInterrupted)
        );
        assert_eq!(
            input_for(&event(CANCEL, KeyState::Released), &defaults),
            None
        );
        assert_eq!(
            input_for(&event(CANCEL, KeyState::Interrupted), &defaults),
            None
        );
        assert_eq!(
            input_for(&event(PASTE_LAST, KeyState::Pressed), &defaults),
            None
        );
        assert_eq!(
            input_for(
                &event(HotkeyId::from_static("gone"), KeyState::Pressed),
                &defaults
            ),
            None
        );
        assert!(handles(HotkeyAction::Record) && handles(HotkeyAction::CancelTake));
        assert!(!handles(HotkeyAction::PasteLast));
    }
}
