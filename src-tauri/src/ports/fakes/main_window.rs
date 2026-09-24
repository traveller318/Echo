/*!
 * SOURCE OF TRUTH KEYWORDS: FakeMainWindow, fake main window, show main window test, open page test
 * WHAT:  FakeMainWindow: a MainWindow that counts how often it was brought to the front and whose next show can fail.
 * WHY:   `app_open_page` must show the main window before asking it to navigate; tests prove it without a window.
 * WHERE: ipc::testing (default CommandCtx); ipc/commands/system.rs tests.
 */

use std::sync::Mutex;

use super::lock;
use crate::{
    ports::MainWindow,
    types::{PortError, PortResult},
};

#[derive(Default)]
struct MainWindowState {
    shows: usize,
    next_error: Option<PortError>,
}

/// A main window that counts instead of showing.
#[derive(Default)]
pub struct FakeMainWindow {
    state: Mutex<MainWindowState>,
}

impl FakeMainWindow {
    /// How many times it was shown successfully.
    pub fn shows(&self) -> usize {
        lock(&self.state).shows
    }

    /// Makes the next show fail with `error`.
    pub fn fail_next(&self, error: PortError) {
        lock(&self.state).next_error = Some(error);
    }
}

impl MainWindow for FakeMainWindow {
    fn show(&self) -> PortResult<()> {
        let mut state = lock(&self.state);
        match state.next_error.take() {
            Some(error) => Err(error),
            None => {
                state.shows += 1;
                Ok(())
            }
        }
    }
}
