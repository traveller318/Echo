/*!
 * SOURCE OF TRUTH KEYWORDS: FakeForegroundApp, fake focused window, sample AppTarget, no foreground window, fake last external app, focus Echo
 * WHAT:  FakeForegroundApp: a ForegroundApp whose focused window the test sets; like the real one, focusing another
 *        app also makes it the last external app, while `focus_echo` moves focus to an Echo window (a tray click)
 *        and keeps it; `target` builds a sample AppTarget.
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
    last_external: Option<AppTarget>,
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

    /// Focuses `target` (another app), which also becomes the last external app; None: nothing has focus.
    pub fn set(&self, target: Option<AppTarget>) {
        let mut state = lock(&self.state);
        if target.is_some() {
            state.last_external.clone_from(&target);
        }
        state.current = target;
    }

    /// Focuses one of Echo's own windows (the tray menu, the main window): the last external app stays.
    pub fn focus_echo(&self) {
        let echo = AppTarget {
            process_id: std::process::id(),
            exe_name: Some(String::from("echo.exe")),
            ..Self::target("echo.exe", false)
        };
        lock(&self.state).current = Some(echo);
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

    fn last_external(&self) -> PortResult<Option<AppTarget>> {
        let mut state = lock(&self.state);
        match state.next_error.take() {
            Some(error) => Err(error),
            None => Ok(state.last_external.clone()),
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

    #[test]
    fn a_click_into_echo_keeps_the_last_external_app() {
        let notepad = FakeForegroundApp::target("notepad.exe", false);
        let app = FakeForegroundApp::focused(notepad.clone());
        app.focus_echo();
        let current = app.current().unwrap().unwrap();
        assert_eq!(current.process_id, std::process::id());
        assert_eq!(app.last_external().unwrap(), Some(notepad.clone()));
        app.set(None);
        assert_eq!(app.last_external().unwrap(), Some(notepad));
    }
}
