/*!
 * SOURCE OF TRUTH KEYWORDS: ForegroundApp, foreground window, focused app, paste target capture, current target
 * WHAT:  ForegroundApp reports the window that currently has focus as an AppTarget.
 * WHY:   The target is captured once when recording starts, before the pill appears, so the paste returns to where
 *        the user was typing even if focus moves during the take (05 W3). None means no window has focus (the
 *        desktop, a lock screen); delivery then copies only.
 * WHERE: Implemented by adapters/foreground/win32.rs (Win32ForegroundApp) and ports/fakes; called by the session
 *        actor's `RecordPressed` effect.
 */

use crate::types::{AppTarget, PortResult};

/// The focused app.
pub trait ForegroundApp: Send + Sync {
    fn current(&self) -> PortResult<Option<AppTarget>>;
}
