/*!
 * SOURCE OF TRUTH KEYWORDS: UnknownFocusForeground, focused window unknown, Wayland foreground, generic paste target, AppTarget without window
 * WHAT:  UnknownFocusForeground: ForegroundApp that reports the same anonymous target for "the window in front" and
 *        "the last app outside Echo": no window handle, no process, no work area, not elevated.
 * WHY:   Wayland deliberately hides other apps' windows from clients, so Echo cannot name the focused app; but Delivery
 *        only pastes when it has a target (without one it copies and toasts), and a paste on Wayland goes to whatever
 *        has focus anyway. The anonymous target lets the inserter's own caps decide between pasting and copying.
 *        `exe_name` is None, so History shows no app for these takes. An X11 session replaces this with a real
 *        window lookup (plan phase 1).
 * WHERE: Built by app/platform.rs on Linux until a display-server-specific foreground adapter exists.
 */

use crate::{
    ports::ForegroundApp,
    types::{AppTarget, PortResult, WindowHandle},
};

#[derive(Debug, Default)]
pub struct UnknownFocusForeground;

impl UnknownFocusForeground {
    pub const fn new() -> Self {
        Self
    }

    fn target() -> AppTarget {
        AppTarget {
            window: WindowHandle::from_raw(0),
            process_id: 0,
            exe_name: None,
            work_area: None,
            elevated: false,
        }
    }
}

impl ForegroundApp for UnknownFocusForeground {
    fn current(&self) -> PortResult<Option<AppTarget>> {
        Ok(Some(Self::target()))
    }

    fn last_external(&self) -> PortResult<Option<AppTarget>> {
        Ok(Some(Self::target()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_questions_get_the_anonymous_target() {
        let foreground = UnknownFocusForeground::new();
        let current = foreground.current().unwrap().unwrap();
        assert_eq!(Some(current.clone()), foreground.last_external().unwrap());
        assert!(!current.elevated && current.exe_name.is_none());
    }
}
