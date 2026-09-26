/*!
 * SOURCE OF TRUTH KEYWORDS: session rehearsal, Rehearsal, rehearse hotkey, HotkeyRehearsed, practice take, keep text in Echo, Echo window focused, is_echo_window
 * WHAT:  Rehearsal: what the session rehearses (types/session.rs SessionRehearsal) and its two decisions: whether a
 *        hotkey event is only reported (`hotkey`, a HotkeyRehearsed instead of a session input) and whether a take's
 *        text stays in Echo (`keeps_in_app`, delivered as DeliveryOutcome::Shown).
 * WHY:   Onboarding tests the hotkey and runs a practice take without recording a test press or pasting into Echo
 *        (step 24). Both decisions hold only while an Echo window is in the foreground (the focused window's process
 *        is Echo's own), read when the hotkey fires and from the take's target for delivery, so a rehearsal left on
 *        by a hidden or closed window can never swallow a hotkey or keep text from the app the user dictates into.
 *        The machine never sees a rehearsal: a rehearsed hotkey is not an input, and a rehearsed take is an ordinary
 *        take whose delivery reports `Shown`, so recording state keeps its one owner (02 §5). A foreground that cannot
 *        be read counts as not Echo: the hotkey then works normally.
 * WHERE: Held by the session runner (runner.rs): the actor asks `hotkey` for every hotkey event while no take is in
 *        progress (actor.rs); the runner's delivery asks `keeps_in_app`.
 */

use crate::{
    ports::ForegroundApp,
    registry,
    types::{AppTarget, HotkeyEvent, HotkeyRehearsed, SessionRehearsal},
};

/// What the session rehearses; nothing until onboarding asks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Rehearsal(SessionRehearsal);

impl Rehearsal {
    pub fn set(&mut self, rehearsal: SessionRehearsal) {
        self.0 = rehearsal;
    }

    /// The report that replaces `event` under a hotkey rehearsal while Echo has focus; None when the event is routed
    /// as usual.
    pub fn hotkey(
        self,
        event: &HotkeyEvent,
        foreground: &dyn ForegroundApp,
    ) -> Option<HotkeyRehearsed> {
        if self.0 != SessionRehearsal::Hotkey {
            return None;
        }
        let action = registry::hotkeys::action_of(&event.id)?;
        echo_has_focus(foreground).then(|| HotkeyRehearsed {
            hotkey: event.id.clone(),
            action,
            state: event.state,
        })
    }

    /// A take started in `target` keeps its text in Echo instead of being delivered.
    pub fn keeps_in_app(self, target: Option<&AppTarget>) -> bool {
        self.0 == SessionRehearsal::Take && target.is_some_and(is_echo_window)
    }
}

/// The window belongs to Echo itself (the main window; the pill never takes focus).
pub(super) fn is_echo_window(target: &AppTarget) -> bool {
    target.process_id == std::process::id()
}

fn echo_has_focus(foreground: &dyn ForegroundApp) -> bool {
    match foreground.current() {
        Ok(target) => target.as_ref().is_some_and(is_echo_window),
        Err(error) => {
            tracing::warn!(
                detail = error.detail(),
                "the focused window could not be read; the hotkey is not rehearsed"
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ports::fakes::FakeForegroundApp,
        registry::hotkeys::{PASTE_LAST, RECORD},
        types::{AppError, HotkeyAction, HotkeyId, KeyState, PortError},
    };

    fn echo() -> AppTarget {
        AppTarget {
            process_id: std::process::id(),
            ..FakeForegroundApp::target("echo.exe", false)
        }
    }

    fn notepad() -> AppTarget {
        FakeForegroundApp::target("notepad.exe", false)
    }

    fn rehearsing(mode: SessionRehearsal) -> Rehearsal {
        let mut rehearsal = Rehearsal::default();
        rehearsal.set(mode);
        rehearsal
    }

    fn event(id: HotkeyId, state: KeyState) -> HotkeyEvent {
        HotkeyEvent { id, state }
    }

    #[test]
    fn a_hotkey_rehearsal_reports_presses_only_while_echo_has_focus() {
        let rehearsal = rehearsing(SessionRehearsal::Hotkey);
        let focused = FakeForegroundApp::focused(echo());
        assert_eq!(
            rehearsal.hotkey(&event(RECORD, KeyState::Pressed), &focused),
            Some(HotkeyRehearsed {
                hotkey: RECORD,
                action: HotkeyAction::Record,
                state: KeyState::Pressed,
            })
        );
        assert_eq!(
            rehearsal
                .hotkey(&event(PASTE_LAST, KeyState::Released), &focused)
                .map(|rehearsed| (rehearsed.action, rehearsed.state)),
            Some((HotkeyAction::PasteLast, KeyState::Released))
        );
        assert_eq!(
            rehearsal.hotkey(
                &event(HotkeyId::from_static("gone"), KeyState::Pressed),
                &focused
            ),
            None,
            "an unknown hotkey is left to the router"
        );

        let elsewhere = FakeForegroundApp::focused(notepad());
        assert_eq!(
            rehearsal.hotkey(&event(RECORD, KeyState::Pressed), &elsewhere),
            None
        );
        let nothing = FakeForegroundApp::default();
        assert_eq!(
            rehearsal.hotkey(&event(RECORD, KeyState::Pressed), &nothing),
            None
        );
        focused.fail_next(PortError::new(AppError::Internal).with_detail("no desktop"));
        assert_eq!(
            rehearsal.hotkey(&event(RECORD, KeyState::Pressed), &focused),
            None,
            "an unreadable foreground leaves the hotkey working"
        );
    }

    #[test]
    fn only_a_hotkey_rehearsal_reports_hotkeys() {
        let focused = FakeForegroundApp::focused(echo());
        for mode in [SessionRehearsal::Off, SessionRehearsal::Take] {
            assert_eq!(
                rehearsing(mode).hotkey(&event(RECORD, KeyState::Pressed), &focused),
                None,
                "{mode:?}"
            );
        }
    }

    #[test]
    fn only_a_take_rehearsal_in_an_echo_window_keeps_the_text_in_echo() {
        let take = rehearsing(SessionRehearsal::Take);
        assert!(take.keeps_in_app(Some(&echo())));
        assert!(!take.keeps_in_app(Some(&notepad())));
        assert!(!take.keeps_in_app(None));
        for mode in [SessionRehearsal::Off, SessionRehearsal::Hotkey] {
            assert!(!rehearsing(mode).keeps_in_app(Some(&echo())), "{mode:?}");
        }
    }
}
