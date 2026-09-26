/*!
 * SOURCE OF TRUTH KEYWORDS: LaunchAtLoginState, launch at startup, Run key state, StartupApproved, Task Manager startup, LaunchOrigin, MainWindowAtLaunch, start minimized, start in tray
 * WHAT:  What Windows holds for Echo's start at sign-in (LaunchAtLoginState: no entry, or an entry that Task
 *        Manager may have switched off and that may point at another executable), how this process was started
 *        (LaunchOrigin: by the user, or by Windows at sign-in), and whether the main window shows at launch
 *        (MainWindowAtLaunch).
 * WHY:   `general.launch_at_startup` follows Windows, not the other way round: an entry the user switched off in
 *        Task Manager ("approved: false") is respected at startup instead of being re-enabled behind their back,
 *        and the toggle is set to match (pipeline/launch.rs). An entry that points at an older install location is
 *        rewritten ("current: false"). Starting hidden applies only when Windows starts Echo at sign-in: a user
 *        who double-clicks Echo wants its window, and a first run always shows its setup (05 decision log, step 25).
 * WHERE: `LaunchAtLogin::state` (ports/startup.rs), adapters/startup; pipeline/launch.rs decides with them;
 *        app/mod.rs reads the origin from the command line and shows the main window from the decision.
 */

/// Echo's start-at-sign-in entry as Windows holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LaunchAtLoginState {
    /// No entry: Echo does not start at sign-in.
    NotRegistered,
    Registered {
        /// Windows will run it: the user has not switched it off in Task Manager's Startup apps.
        approved: bool,
        /// The entry starts this executable with Echo's launch arguments.
        current: bool,
    },
}

impl LaunchAtLoginState {
    /// Windows will start Echo at the next sign-in.
    pub const fn starts_at_login(self) -> bool {
        matches!(self, Self::Registered { approved: true, .. })
    }
}

/// Who started this process.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum LaunchOrigin {
    /// The user (Start menu, a shortcut, the executable).
    #[default]
    User,
    /// Windows, at sign-in (the start-at-sign-in entry passes Echo's launch argument).
    Login,
}

/// Whether the main window shows when Echo starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MainWindowAtLaunch {
    Show,
    /// Echo starts in the tray; the tray icon or a second launch shows the window.
    StayHidden,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_approved_entry_starts_echo() {
        assert!(!LaunchAtLoginState::NotRegistered.starts_at_login());
        assert!(
            !LaunchAtLoginState::Registered {
                approved: false,
                current: true
            }
            .starts_at_login()
        );
        assert!(
            LaunchAtLoginState::Registered {
                approved: true,
                current: false
            }
            .starts_at_login()
        );
    }
}
