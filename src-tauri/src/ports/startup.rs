/*!
 * SOURCE OF TRUTH KEYWORDS: LaunchAtLogin, start at sign-in, launch at startup, Run key, register startup, unregister startup, autostart
 * WHAT:  LaunchAtLogin registers Echo to start when the user signs in to Windows, removes that entry, and reports
 *        what Windows holds for it (LaunchAtLoginState).
 * WHY:   Startup behaviour is Windows-specific, so it sits behind a port (root CLAUDE.md §3). `state` distinguishes
 *        "no entry" from "an entry the user switched off in Task Manager", so the core can respect that choice
 *        instead of re-enabling it at every launch; `register` is only called for the user's own choice in Echo (or
 *        a first run), and then also clears a Task Manager switch-off. The entry starts Echo with its launch
 *        argument, which is how a start at sign-in is told apart (LaunchOrigin::Login). A development build declares
 *        `available: false` and registers nothing.
 * WHERE: Implemented by adapters/startup (Win32RunKey, DisabledLaunchAtLogin) and ports/fakes; driven by
 *        pipeline/launch.rs at startup and after `general.launch_at_startup` changes.
 */

use crate::types::{LaunchAtLoginCaps, LaunchAtLoginState, PortResult};

/// Echo's start at sign-in.
pub trait LaunchAtLogin: Send + Sync {
    fn caps(&self) -> LaunchAtLoginCaps;

    /// What Windows holds for Echo's entry now.
    fn state(&self) -> PortResult<LaunchAtLoginState>;

    /// Starts this executable at every sign-in, switched on even if Task Manager had switched it off.
    fn register(&self) -> PortResult<()>;

    /// Removes the entry; no entry is a no-op.
    fn unregister(&self) -> PortResult<()>;
}
