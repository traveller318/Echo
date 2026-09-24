/*!
 * SOURCE OF TRUTH KEYWORDS: FakeTextInserter, fake paste, inserted text, elevated target test, UIPI block test
 * WHAT:  FakeTextInserter: a TextInserter that records every (target, text) insertion and refuses elevated
 *        targets unless its caps say it can reach them.
 * WHY:   Delivery tests assert what landed where, and that the copy-only fallback (05 W2) is taken instead of
 *        calling the inserter for an elevated window; if the pipeline calls anyway, the fake fails the same way
 *        UIPI would.
 * WHERE: pipeline delivery and session actor tests.
 */

use std::sync::Mutex;

use super::lock;
use crate::{
    ports::TextInserter,
    types::{AppError, AppTarget, InserterCaps, Permission, PortError, PortResult},
};

#[derive(Default)]
struct InserterLog {
    insertions: Vec<(AppTarget, String)>,
    next_error: Option<PortError>,
}

/// A recording text inserter.
pub struct FakeTextInserter {
    caps: InserterCaps,
    log: Mutex<InserterLog>,
}

impl Default for FakeTextInserter {
    /// Pastes through the clipboard and cannot reach elevated windows, like the Win32 SendInput adapter.
    fn default() -> Self {
        Self::new(InserterCaps {
            can_target_elevated: false,
            uses_clipboard: true,
        })
    }
}

impl FakeTextInserter {
    pub fn new(caps: InserterCaps) -> Self {
        Self {
            caps,
            log: Mutex::default(),
        }
    }

    pub fn fail_next(&self, error: PortError) {
        lock(&self.log).next_error = Some(error);
    }

    pub fn insertions(&self) -> Vec<(AppTarget, String)> {
        lock(&self.log).insertions.clone()
    }
}

impl TextInserter for FakeTextInserter {
    fn caps(&self) -> InserterCaps {
        self.caps
    }

    fn insert(&self, target: &AppTarget, text: &str) -> PortResult<()> {
        let mut log = lock(&self.log);
        if let Some(error) = log.next_error.take() {
            return Err(error);
        }
        if target.elevated && !self.caps.can_target_elevated {
            return Err(AppError::PermissionDenied {
                permission: Permission::InputInjection,
            }
            .into());
        }
        log.insertions.push((target.clone(), text.to_owned()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::fakes::FakeForegroundApp;

    #[test]
    fn records_insertions_and_refuses_elevated_targets() {
        let inserter = FakeTextInserter::default();
        let notepad = FakeForegroundApp::target("notepad.exe", false);
        inserter.insert(&notepad, "Hello.").unwrap();
        let admin = FakeForegroundApp::target("wt.exe", true);
        assert!(inserter.insert(&admin, "Hello.").is_err());
        inserter.fail_next(AppError::Internal.into());
        assert!(inserter.insert(&notepad, "again").is_err());
        assert_eq!(inserter.insertions(), [(notepad, String::from("Hello."))]);
    }
}
