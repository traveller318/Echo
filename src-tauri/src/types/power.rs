/*!
 * SOURCE OF TRUTH KEYWORDS: PowerEvent, suspend, resume, sleep, wake, display on, session unlock, explorer restart, TaskbarCreated, power notification
 * WHAT:  PowerEvent: a change in the machine or the Windows session around Echo: about to sleep, woken up, the
 *        display back on, the session back at this console (unlock, fast user switching, remote desktop), or the
 *        shell restarted (TaskbarCreated). `ends_take` and `refreshes_hotkeys` say what each one means to Echo.
 * WHY:   Suspend must finalize an active take before the machine sleeps (02 §9); every other transition is a moment
 *        Windows may have dropped the keyboard hook (05 W8: sleep, fast user switching, explorer restarts; Modern
 *        Standby machines wake with only the display turning on), so each one re-registers the hotkeys. The consumer
 *        asks the two questions instead of matching variants, so a new transition is one variant and one answer.
 * WHERE: `PowerEvents::listen` (ports/power.rs), produced by adapters/power/win32.rs; the session actor finalizes
 *        on `ends_take` and refreshes the hotkey gate (pipeline/hotkeys.rs) on `refreshes_hotkeys`.
 */

/// A system power or session transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PowerEvent {
    /// The machine is about to sleep or hibernate.
    Suspend,
    /// The machine woke up from sleep or hibernation.
    Resume,
    /// The display turned on again (the only wake signal some Modern Standby machines give).
    DisplayOn,
    /// The session is back at this console: unlocked, switched back to, or reconnected.
    SessionResumed,
    /// Explorer restarted and recreated the taskbar (TaskbarCreated).
    ShellRestarted,
}

impl PowerEvent {
    /// Every transition, so tests can prove each one has an answer.
    pub const ALL: [Self; 5] = [
        Self::Suspend,
        Self::Resume,
        Self::DisplayOn,
        Self::SessionResumed,
        Self::ShellRestarted,
    ];

    /// The take in progress must end now: the machine is going to sleep.
    pub const fn ends_take(self) -> bool {
        matches!(self, Self::Suspend)
    }

    /// Windows may have dropped Echo's keyboard hook, so the hotkeys are registered again.
    pub const fn refreshes_hotkeys(self) -> bool {
        match self {
            Self::Suspend => false,
            Self::Resume | Self::DisplayOn | Self::SessionResumed | Self::ShellRestarted => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_suspend_ends_a_take_and_every_return_refreshes_the_hotkeys() {
        for event in PowerEvent::ALL {
            assert_ne!(
                event.ends_take(),
                event.refreshes_hotkeys(),
                "{event:?} must either end the take or refresh the hotkeys"
            );
        }
        assert!(PowerEvent::Suspend.ends_take());
    }
}
