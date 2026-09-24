/*!
 * SOURCE OF TRUTH KEYWORDS: AppTarget, WindowHandle, ScreenRect, foreground window, paste target, elevated window, monitor work area, UIPI
 * WHAT:  AppTarget: the app that had focus when a take started (window, process, exe name, monitor work area,
 *        elevation), plus its WindowHandle and ScreenRect building blocks.
 * WHY:   The paste must land where the user was typing (05 W3), the pill appears on that window's monitor above
 *        the taskbar (05 W15), and an elevated target cannot receive synthetic input (05 W2), so all three facts are
 *        captured once at `RecordPressed` and carried through the take. WindowHandle is opaque: only the
 *        foreground and inserter adapters turn it back into an HWND, so no Win32 type leaks into the core.
 * WHERE: Returned by `ForegroundApp::current` (ports/foreground.rs); passed to `TextInserter::insert`
 *        (ports/inserter.rs); `exe_name` becomes `transcripts.app_name`; `work_area` places the pill (app/windows.rs).
 */

/// Opaque handle of a top-level window; only Windows adapters interpret the raw value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowHandle(isize);

impl WindowHandle {
    pub const fn from_raw(raw: isize) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> isize {
        self.0
    }
}

/// A rectangle in physical screen pixels (per-monitor DPI v2 coordinates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// The window that had focus when the take started.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AppTarget {
    pub window: WindowHandle,
    pub process_id: u32,
    /// Executable file name, e.g. `notepad.exe`; None when the process cannot be queried.
    pub exe_name: Option<String>,
    /// Work area (screen minus taskbar) of the monitor showing the window.
    pub work_area: Option<ScreenRect>,
    /// Runs at a higher integrity level than Echo, so Windows blocks synthetic input into it (05 W2).
    pub elevated: bool,
}
