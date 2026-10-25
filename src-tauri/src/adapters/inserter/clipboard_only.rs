/*!
 * SOURCE OF TRUTH KEYWORDS: ClipboardOnlyInserter, no paste injection, copy and toast fallback, InputInjection denied
 * WHAT:  ClipboardOnlyInserter: TextInserter that cannot type into other apps and says so with
 *        `PermissionDenied { input_injection }`. Caps: uses the clipboard, reaches every target.
 * WHY:   The stand-in for a Linux session without a working paste path (plan phases 1-3 replace it per display
 *        server). Delivery already treats a failed paste as "copy and tell the user": the text is on the clipboard
 *        and a toast asks for Ctrl+V, so dictation stays useful.
 * WHERE: Built by app/bootstrap on Linux when no inserter fits the session.
 */

use crate::{
    ports::TextInserter,
    types::{AppError, AppTarget, InserterCaps, Permission, PortError, PortResult},
};

#[derive(Debug, Default)]
pub struct ClipboardOnlyInserter;

impl ClipboardOnlyInserter {
    pub const fn new() -> Self {
        Self
    }
}

impl TextInserter for ClipboardOnlyInserter {
    fn caps(&self) -> InserterCaps {
        InserterCaps {
            can_target_elevated: true,
            uses_clipboard: true,
        }
    }

    fn insert(&self, _target: &AppTarget, _text: &str) -> PortResult<()> {
        Err(PortError::new(AppError::PermissionDenied {
            permission: Permission::InputInjection,
        })
        .with_detail("this session offers no way to paste into other apps"))
    }
}
