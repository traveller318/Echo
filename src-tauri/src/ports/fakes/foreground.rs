/*!
 * SOURCE OF TRUTH KEYWORDS: FakeForegroundApp, fake focused window, sample AppTarget, no foreground window
 * WHAT:  FakeForegroundApp: a ForegroundApp whose focused window the test sets; `target` builds a sample
 *        AppTarget.
 * WHY:   Session tests start takes over normal, elevated or missing targets to reach the paste, copy-only and
 *        no-target delivery paths.
 * WHERE: session actor and delivery tests; FakeTextInserter tests.
 */

use std::sync::Mutex;

use super::lock;
use crate::{
    ports::ForegroundApp,
    types::{AppTarget, PortError, PortResult, ScreenRect, WindowHandle},
};

#[derive(Default)]
struct ForegroundState {
    current: Option<AppTarget>,
    next_error: Option<PortError>,
}

/// A settable focused window.
#[derive(Default)]
pub struct FakeForegroundApp {
    state: Mutex<ForegroundState>,
}

impl FakeForegroundApp {
    pub fn focused(target: AppTarget) -> Self {
        let app = Self::default();
        app.set(Some(target));
        app
    }

    /// A 1920×1040 work area target for `exe_name`.
    pub fn target(exe_name: &str, elevated: bool) -> AppTarget {
        AppTarget {
            window: WindowHandle::from_raw(0x1234),
            process_id: 4242,
            exe_name: Some(exe_name.to_owned()),
            work_area: Some(ScreenRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1040,
            }),
            elevated,
        }
    }

    pub fn set(&self, target: Option<AppTarget>) {
        lock(&self.state).current = target;
    }

    pub fn fail_next(&self, error: PortError) {
        lock(&self.state).next_error = Some(error);
    }
}

impl ForegroundApp for FakeForegroundApp {
    fn current(&self) -> PortResult<Option<AppTarget>> {
        let mut state = lock(&self.state);
        match state.next_error.take() {
            Some(error) => Err(error),
            None => Ok(state.current.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AppError;

    #[test]
    fn reports_the_focused_target() {
        let notepad = FakeForegroundApp::target("notepad.exe", false);
        let app = FakeForegroundApp::focused(notepad.clone());
        assert_eq!(app.current().unwrap(), Some(notepad));
        app.set(None);
        assert_eq!(app.current().unwrap(), None);
        app.fail_next(AppError::Internal.into());
        assert!(app.current().is_err());
    }
}
