/*!
 * SOURCE OF TRUTH KEYWORDS: hwnd, window_handle, WindowHandle HWND conversion, opaque window handle, pointer provenance
 * WHAT:  `window_handle(HWND) -> WindowHandle` and `hwnd(WindowHandle) -> HWND`.
 * WHY:   The core carries windows as the opaque WindowHandle (types/target.rs) so no Win32 type leaks out of
 *        adapters; only adapters turn it back. An HWND is an index into the window manager's table, not memory, so
 *        the round trip keeps its address bits and gives the pointer no provenance.
 * WHERE: adapters/foreground (reports the focused window), adapters/inserter (pastes into it).
 */

use std::ptr;

use windows::Win32::Foundation::HWND;

use crate::types::WindowHandle;

/// The opaque form of `window`.
pub fn window_handle(window: HWND) -> WindowHandle {
    WindowHandle::from_raw(window.0.addr().cast_signed())
}

/// The HWND `handle` was made from.
pub fn hwnd(handle: WindowHandle) -> HWND {
    HWND(ptr::without_provenance_mut(handle.raw().cast_unsigned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_round_trip() {
        let handle = WindowHandle::from_raw(0x0001_0A2C);
        assert_eq!(window_handle(hwnd(handle)), handle);
        assert!(hwnd(WindowHandle::from_raw(0)).is_invalid());
    }
}
