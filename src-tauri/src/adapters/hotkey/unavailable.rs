/*!
 * SOURCE OF TRUTH KEYWORDS: UnavailableHotkeys, no global hotkeys, hotkey stub, Hotkey invalid
 * WHAT:  UnavailableHotkeys: HotkeyService that listens to nothing and refuses every binding with
 *        `Hotkey { reason: invalid }`.
 * WHY:   The stand-in for a Linux session none of the real hotkey adapters can serve yet (plan phases 1-3 replace it
 *        per display server): Echo still starts, the tray and the window can start a take, and Settings shows the
 *        hotkey as not bindable instead of the app failing at startup.
 * WHERE: Built by app/bootstrap on Linux when no hotkey adapter fits the session.
 */

use std::sync::Arc;

use crate::{
    ports::{EventSink, HotkeyService},
    types::{
        AppError, HotkeyCaps, HotkeyEvent, HotkeyId, HotkeyIssue, PortError, PortResult, Shortcut,
    },
};

#[derive(Debug, Default)]
pub struct UnavailableHotkeys;

impl UnavailableHotkeys {
    pub const fn new() -> Self {
        Self
    }
}

impl HotkeyService for UnavailableHotkeys {
    fn caps(&self) -> HotkeyCaps {
        HotkeyCaps {
            supports_release: false,
            supports_modifier_only: false,
        }
    }

    fn listen(&self, _sink: Arc<dyn EventSink<HotkeyEvent>>) -> PortResult<()> {
        Ok(())
    }

    fn register(&self, _id: &HotkeyId, _shortcut: &Shortcut) -> PortResult<()> {
        Err(PortError::new(AppError::Hotkey {
            reason: HotkeyIssue::Invalid,
        })
        .with_detail("global hotkeys are not available in this session"))
    }

    fn unregister(&self, _id: &HotkeyId) -> PortResult<()> {
        Ok(())
    }

    fn refresh(&self) -> PortResult<()> {
        Ok(())
    }
}
