/*!
 * SOURCE OF TRUTH KEYWORDS: ForegroundApp, foreground window, focused app, paste target capture, current target, last external app, previous app before tray click
 * WHAT:  ForegroundApp reports the window that currently has focus as an AppTarget (`current`), and the window of
 *        the last app that had focus outside Echo and the Windows shell (`last_external`).
 * WHY:   The target is captured once when recording starts, before the pill appears, so the paste returns to where
 *        the user was typing even if focus moves during the take (05 W3). None means no window has focus (the
 *        desktop, a lock screen); delivery then copies only. A take or a paste-last started by a click (the tray, an
 *        Echo window) cannot use `current`: the click itself moved focus to the taskbar, the tray menu or Echo, so
 *        `last_external` answers where the user was before (TargetRule::LastExternal); None when no such window is
 *        known or it has closed since.
 * WHERE: Implemented by adapters/foreground/win32.rs (Win32ForegroundApp) and ports/fakes; called by the session
 *        actor's Arm effect and paste-last (pipeline/session, pipeline/history.rs) under the take's TargetRule.
 */

use crate::types::{AppTarget, PortResult};

/// The focused app.
pub trait ForegroundApp: Send + Sync {
    /// The window in front now.
    fn current(&self) -> PortResult<Option<AppTarget>>;

    /// The last window in front that belonged neither to Echo nor to the Windows shell, if it still exists.
    fn last_external(&self) -> PortResult<Option<AppTarget>>;
}
