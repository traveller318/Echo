/*!
 * SOURCE OF TRUTH KEYWORDS: FakeLaunchAtLogin, fake Run key, fake Task Manager startup, launch at startup test
 * WHAT:  FakeLaunchAtLogin: a LaunchAtLogin over an in-memory entry, with the moves a user or an installer makes
 *        outside Echo (switched off in Task Manager, an entry left by an older install location) and a record of
 *        every register and unregister.
 * WHY:   pipeline/launch.rs must respect a Task Manager switch-off, repair a stale entry and follow the setting,
 *        which needs each Windows state without touching the real HKCU Run key.
 * WHERE: pipeline/launch.rs and settings command tests; the command harness.
 */

use std::sync::Mutex;

use super::lock;
use crate::{
    ports::LaunchAtLogin,
    types::{LaunchAtLoginCaps, LaunchAtLoginState, PortError, PortResult},
};

/// What the pipeline asked the fake to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchAtLoginCall {
    Register,
    Unregister,
}

struct Entry {
    state: LaunchAtLoginState,
    calls: Vec<LaunchAtLoginCall>,
    next_error: Option<PortError>,
}

/// An in-memory start-at-sign-in entry.
pub struct FakeLaunchAtLogin {
    caps: LaunchAtLoginCaps,
    entry: Mutex<Entry>,
}

impl FakeLaunchAtLogin {
    /// An installed build with no entry yet.
    pub fn new() -> Self {
        Self::with(LaunchAtLoginState::NotRegistered, true)
    }

    /// A development build: unavailable, nothing registered.
    pub fn unavailable() -> Self {
        Self::with(LaunchAtLoginState::NotRegistered, false)
    }

    /// An installed build whose entry starts as `state`.
    pub fn in_state(state: LaunchAtLoginState) -> Self {
        Self::with(state, true)
    }

    fn with(state: LaunchAtLoginState, available: bool) -> Self {
        Self {
            caps: LaunchAtLoginCaps { available },
            entry: Mutex::new(Entry {
                state,
                calls: Vec::new(),
                next_error: None,
            }),
        }
    }

    /// Every register and unregister so far, in order.
    pub fn calls(&self) -> Vec<LaunchAtLoginCall> {
        lock(&self.entry).calls.clone()
    }

    /// The entry as Windows holds it now.
    pub fn current(&self) -> LaunchAtLoginState {
        lock(&self.entry).state
    }

    /// The next call fails with `error`.
    pub fn fail_next(&self, error: PortError) {
        lock(&self.entry).next_error = Some(error);
    }
}

impl Default for FakeLaunchAtLogin {
    fn default() -> Self {
        Self::new()
    }
}

impl LaunchAtLogin for FakeLaunchAtLogin {
    fn caps(&self) -> LaunchAtLoginCaps {
        self.caps
    }

    fn state(&self) -> PortResult<LaunchAtLoginState> {
        let mut entry = lock(&self.entry);
        match entry.next_error.take() {
            Some(error) => Err(error),
            None => Ok(entry.state),
        }
    }

    fn register(&self) -> PortResult<()> {
        let mut entry = lock(&self.entry);
        entry.calls.push(LaunchAtLoginCall::Register);
        if let Some(error) = entry.next_error.take() {
            return Err(error);
        }
        entry.state = LaunchAtLoginState::Registered {
            approved: true,
            current: true,
        };
        Ok(())
    }

    fn unregister(&self) -> PortResult<()> {
        let mut entry = lock(&self.entry);
        entry.calls.push(LaunchAtLoginCall::Unregister);
        if let Some(error) = entry.next_error.take() {
            return Err(error);
        }
        entry.state = LaunchAtLoginState::NotRegistered;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AppError;

    #[test]
    fn register_switches_the_entry_on_and_unregister_removes_it() {
        let fake = FakeLaunchAtLogin::in_state(LaunchAtLoginState::Registered {
            approved: false,
            current: false,
        });
        fake.register().unwrap();
        assert!(fake.state().unwrap().starts_at_login());
        fake.unregister().unwrap();
        assert_eq!(fake.state().unwrap(), LaunchAtLoginState::NotRegistered);
        fake.fail_next(AppError::Internal.into());
        assert!(fake.register().is_err());
        assert_eq!(
            fake.calls(),
            [
                LaunchAtLoginCall::Register,
                LaunchAtLoginCall::Unregister,
                LaunchAtLoginCall::Register
            ]
        );
        assert!(!FakeLaunchAtLogin::unavailable().caps().available);
    }
}
