/*!
 * SOURCE OF TRUTH KEYWORDS: ui input, route UI input, SessionUiInput to SessionInput, tray toggle, pill stop, pill cancel, from_ui policy
 * WHAT:  `route(input, settings)` turns an input from the pill or the tray into the machine input it means: Stop,
 *        Esc for the pill's ✕ and "Undo", and Toggle (stamped with a fresh take id and the UI policy) for the tray's
 *        Start / Stop dictation.
 * WHY:   The pill and the tray feed the same state machine as the hotkeys, so every rule (debounce, the Esc
 *        countdown, a stop while arming) applies unchanged; minting an id and reading the settings are side effects
 *        the pure machine cannot do, so they happen here. A click toggles and targets the app the user left
 *        (SessionPolicy::from_ui), because nothing is held and the click itself moved focus.
 * WHERE: The session actor (actor.rs) for every Message::Ui.
 */

use crate::{
    registry,
    types::{SessionInput, SessionUiInput, SettingsSnapshot, TranscriptId},
};

/// The machine input `input` means under `settings`.
pub fn route(input: SessionUiInput, settings: &SettingsSnapshot) -> SessionInput {
    match input {
        SessionUiInput::Stop => SessionInput::Stop,
        // The pill's ✕ is the Esc hotkey: one input, so the machine's cancel and undo rules apply unchanged.
        SessionUiInput::Cancel => SessionInput::Esc,
        SessionUiInput::Toggle => SessionInput::Toggle {
            next_take: TranscriptId::generate(),
            policy: registry::settings::session_policy(settings).from_ui(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        registry::settings,
        types::{RecordMode, SessionPolicy, TargetRule},
    };

    #[test]
    fn a_tray_toggle_starts_a_toggle_take_for_the_app_the_user_left() {
        let defaults = settings::defaults();
        assert_eq!(route(SessionUiInput::Stop, &defaults), SessionInput::Stop);
        assert_eq!(route(SessionUiInput::Cancel, &defaults), SessionInput::Esc);
        let SessionInput::Toggle { policy, next_take } = route(SessionUiInput::Toggle, &defaults)
        else {
            panic!("the tray's toggle is a Toggle input");
        };
        assert_eq!(
            policy.record_mode,
            RecordMode::Toggle,
            "hold mode has nothing to hold"
        );
        assert_eq!(policy.target, TargetRule::LastExternal);
        assert_eq!(
            policy.cancel_countdown_ms,
            SessionPolicy::DEFAULT.cancel_countdown_ms
        );
        let SessionInput::Toggle {
            next_take: second, ..
        } = route(SessionUiInput::Toggle, &defaults)
        else {
            panic!("the tray's toggle is a Toggle input");
        };
        assert_ne!(next_take, second);
    }
}
