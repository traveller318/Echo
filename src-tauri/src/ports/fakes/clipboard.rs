/*!
 * SOURCE OF TRUTH KEYWORDS: FakeClipboard, fake clipboard, clipboard held by another app, clipboard history policy test
 * WHAT:  FakeClipboard: an in-memory Clipboard that records the history policy of the last write and can be
 *        `hold`-en by "another app" for the next N calls.
 * WHY:   Delivery tests check that transcripts are written with `ClipboardHistory::Exclude` (05 W5) and that a busy
 *        clipboard fails with `PermissionDenied { clipboard }` without losing the take (05 W4).
 * WHERE: pipeline delivery and session actor tests.
 */

use std::sync::Mutex;

use super::lock;
use crate::{
    ports::Clipboard,
    types::{AppError, ClipboardHistory, Permission, PortResult},
};

#[derive(Default)]
struct ClipboardState {
    text: Option<String>,
    last_history: Option<ClipboardHistory>,
    held_for: usize,
}

impl ClipboardState {
    /// Consumes one held call; true when this call must fail.
    fn busy(&mut self) -> bool {
        let busy = self.held_for > 0;
        self.held_for = self.held_for.saturating_sub(1);
        busy
    }
}

/// An in-memory clipboard.
#[derive(Default)]
pub struct FakeClipboard {
    state: Mutex<ClipboardState>,
}

impl FakeClipboard {
    pub fn with_text(text: &str) -> Self {
        let clipboard = Self::default();
        lock(&clipboard.state).text = Some(text.to_owned());
        clipboard
    }

    /// Another app holds the clipboard for the next `calls` calls.
    pub fn hold(&self, calls: usize) {
        lock(&self.state).held_for = calls;
    }

    pub fn text(&self) -> Option<String> {
        lock(&self.state).text.clone()
    }

    pub fn last_history(&self) -> Option<ClipboardHistory> {
        lock(&self.state).last_history
    }

    fn held() -> AppError {
        AppError::PermissionDenied {
            permission: Permission::Clipboard,
        }
    }
}

impl Clipboard for FakeClipboard {
    fn read_text(&self) -> PortResult<Option<String>> {
        let mut state = lock(&self.state);
        if state.busy() {
            return Err(Self::held().into());
        }
        Ok(state.text.clone())
    }

    fn write_text(&self, text: &str, history: ClipboardHistory) -> PortResult<()> {
        let mut state = lock(&self.state);
        if state.busy() {
            return Err(Self::held().into());
        }
        state.text = Some(text.to_owned());
        state.last_history = Some(history);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_record_the_history_policy() {
        let clipboard = FakeClipboard::with_text("before");
        assert_eq!(clipboard.read_text().unwrap().as_deref(), Some("before"));
        clipboard
            .write_text("take", ClipboardHistory::Exclude)
            .unwrap();
        assert_eq!(clipboard.text().as_deref(), Some("take"));
        assert_eq!(clipboard.last_history(), Some(ClipboardHistory::Exclude));
    }

    #[test]
    fn a_held_clipboard_fails_then_recovers() {
        let clipboard = FakeClipboard::default();
        clipboard.hold(1);
        assert!(
            clipboard
                .write_text("take", ClipboardHistory::Exclude)
                .is_err()
        );
        assert_eq!(clipboard.text(), None);
        clipboard
            .write_text("take", ClipboardHistory::Include)
            .unwrap();
        assert_eq!(clipboard.text().as_deref(), Some("take"));
    }
}
