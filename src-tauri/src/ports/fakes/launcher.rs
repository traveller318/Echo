/*!
 * SOURCE OF TRUTH KEYWORDS: FakeSystemLauncher, LaunchCall, fake shell open, open logs folder test, mic privacy settings test
 * WHAT:  FakeSystemLauncher: a SystemLauncher that records every folder and settings page it was asked to open, and
 *        whose next call can fail.
 * WHY:   The system commands, onboarding and About need to prove what they asked the shell to open without a
 *        window appearing on the test machine.
 * WHERE: ipc::testing (default CommandCtx); ipc/commands/system.rs tests.
 */

use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use super::lock;
use crate::{
    ports::SystemLauncher,
    types::{PortError, PortResult, SettingsPage},
};

/// One request the launcher received.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchCall {
    Folder(PathBuf),
    SettingsPage(SettingsPage),
}

#[derive(Default)]
struct LauncherState {
    calls: Vec<LaunchCall>,
    next_error: Option<PortError>,
}

/// A launcher that records instead of opening.
#[derive(Default)]
pub struct FakeSystemLauncher {
    state: Mutex<LauncherState>,
}

impl FakeSystemLauncher {
    /// Every successful request so far, in order.
    pub fn calls(&self) -> Vec<LaunchCall> {
        lock(&self.state).calls.clone()
    }

    /// Makes the next request fail with `error` (and not be recorded).
    pub fn fail_next(&self, error: PortError) {
        lock(&self.state).next_error = Some(error);
    }

    fn record(&self, call: LaunchCall) -> PortResult<()> {
        let mut state = lock(&self.state);
        match state.next_error.take() {
            Some(error) => Err(error),
            None => {
                state.calls.push(call);
                Ok(())
            }
        }
    }
}

impl SystemLauncher for FakeSystemLauncher {
    fn open_folder(&self, dir: &Path) -> PortResult<()> {
        self.record(LaunchCall::Folder(dir.to_path_buf()))
    }

    fn open_settings_page(&self, page: SettingsPage) -> PortResult<()> {
        self.record(LaunchCall::SettingsPage(page))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AppError;

    #[test]
    fn records_requests_and_fails_once() {
        let launcher = FakeSystemLauncher::default();
        launcher.fail_next(AppError::Internal.into());
        assert!(
            launcher
                .open_settings_page(SettingsPage::MicrophonePrivacy)
                .is_err()
        );
        launcher.open_folder(Path::new("logs")).unwrap();
        launcher
            .open_settings_page(SettingsPage::MicrophonePrivacy)
            .unwrap();
        assert_eq!(
            launcher.calls(),
            [
                LaunchCall::Folder(PathBuf::from("logs")),
                LaunchCall::SettingsPage(SettingsPage::MicrophonePrivacy),
            ]
        );
    }
}
