/*!
 * SOURCE OF TRUTH KEYWORDS: DisabledLaunchAtLogin, no start at sign-in, development build startup, LaunchAtLoginCaps available false
 * WHAT:  The LaunchAtLogin of a development build: declares `available: false`, reports no entry, and registers or
 *        removes nothing.
 * WHY:   `tauri dev` runs an executable that needs the dev server, so starting it at sign-in would open a blank Echo
 *        that also holds the hotkeys. The caps hide the startup settings (CapsRequirement::LaunchAtLogin), so the
 *        calls below only happen from a stale page, and doing nothing is the correct answer.
 * WHERE: Built by app/bootstrap when `tauri::is_dev()`; held by CommandCtx.
 */

use crate::{
    ports::LaunchAtLogin,
    types::{LaunchAtLoginCaps, LaunchAtLoginState, PortResult},
};

/// Start at sign-in that is not offered.
#[derive(Debug, Default, Clone, Copy)]
pub struct DisabledLaunchAtLogin;

impl DisabledLaunchAtLogin {
    pub const fn new() -> Self {
        Self
    }
}

impl LaunchAtLogin for DisabledLaunchAtLogin {
    fn caps(&self) -> LaunchAtLoginCaps {
        LaunchAtLoginCaps { available: false }
    }

    fn state(&self) -> PortResult<LaunchAtLoginState> {
        Ok(LaunchAtLoginState::NotRegistered)
    }

    fn register(&self) -> PortResult<()> {
        Ok(())
    }

    fn unregister(&self) -> PortResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_unavailable_and_changes_nothing() {
        let disabled = DisabledLaunchAtLogin::new();
        assert!(!disabled.caps().available);
        disabled.register().unwrap();
        assert_eq!(disabled.state().unwrap(), LaunchAtLoginState::NotRegistered);
        disabled.unregister().unwrap();
    }
}
