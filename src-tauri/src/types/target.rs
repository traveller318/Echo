/*!
 * SOURCE OF TRUTH KEYWORDS: AppTarget, WindowHandle, ScreenRect, ScreenPoint, foreground window, paste target, elevated window, monitor work area, UIPI
 * WHAT:  AppTarget: the app that had focus when a take started (window, process, exe name, monitor work area,
 *        elevation), plus its WindowHandle, ScreenRect and ScreenPoint building blocks.
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

/// A point in physical screen pixels (per-monitor DPI v2 coordinates), e.g. the cursor or a window's corner.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ScreenPoint {
    pub x: i32,
    pub y: i32,
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

/**
 * SOURCE OF TRUTH KEYWORDS: TargetRule, paste target rule, focused window target, last external app, tray take target, UI started take
 * WHAT:  Which window a take or a paste-last delivers to: the one in front at that moment (Focused), or the app the
 *        user was last in outside Echo and the Windows shell (LastExternal).
 * WHY:   A hotkey is pressed while the user's app is in front, so Focused is exact (05 W3). A click on the tray icon,
 *        its menu or an Echo window moves focus to the taskbar or to Echo first, so the window in front is never where
 *        the text should go; LastExternal is the app the user left to click, which is where they were typing.
 * WHERE: SessionPolicy::target (types/session_machine.rs) and the Arm effect; Message::PasteLast; resolved through
 *        `ForegroundApp::current` / `ForegroundApp::last_external` by pipeline/session (arm.rs, history.rs).
 */
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum TargetRule {
    /// The window in front when the take starts (hotkeys).
    #[default]
    Focused,
    /// The last app in front that is neither Echo nor the Windows shell (tray, menus, Echo's own windows).
    LastExternal,
}
