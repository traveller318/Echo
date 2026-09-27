/*!
 * SOURCE OF TRUTH KEYWORDS: Win32OverlayWindow, pill window adapter, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, SWP_NOACTIVATE, per-monitor DPI placement, click-through, GetCursorPos, GetAsyncKeyState, SM_SWAPBUTTON, drag move
 * WHAT:  Win32OverlayWindow: OverlayWindow over the pre-created pill window. `attach` makes it a tool window (no
 *        taskbar button, no Alt+Tab entry) that is never activated and lets clicks through; `show` places it at the
 *        bottom centre of a monitor's work area (or at a saved corner still on a monitor) in physical pixels and
 *        shows it on top without activation; `hide` hides it; `set_click_through` switches WS_EX_TRANSPARENT;
 *        `pointer_over` compares the cursor with areas given in the page's CSS pixels; `origin`, `move_to`,
 *        `cursor` and `primary_button_down` are what the presenter's drag reads and moves with.
 * WHY:   Showing a window with SW_SHOW activates it and would send the paste to the pill (05 W3); SetWindowPos with
 *        SWP_SHOWWINDOW | SWP_NOACTIVATE shows it, puts it on top and places it in one call, which is what
 *        SW_SHOWNOACTIVATE does plus the move. Tauri's own show/hide/ignore-cursor calls are never used on this
 *        window: they rewrite its styles from Tauri's cached flags, would hide it again (Tauri still believes it
 *        hidden) and would drop the styles set here, so this adapter owns the window's visibility and styles
 *        alone. The window is sized by Tauri in logical pixels; on a monitor with another DPI Windows resizes it as
 *        it moves there (WM_DPICHANGED), so the size is scaled for the target monitor up front and the position is
 *        corrected once more with the real size after the move (05 W15, mixed-DPI setups). Click-through needs
 *        WS_EX_LAYERED | WS_EX_TRANSPARENT; LAYERED is set once at attach and only TRANSPARENT is switched, so the
 *        webview's surface is never recreated while the pointer moves over the pill. The Tauri window exists only
 *        once the event loop runs, after the composition root built this adapter, so the handle arrives later
 *        through `attach`; until then every call fails with `Internal` and the pill simply does not show. A drag
 *        moves the window with SetWindowPos (SWP_NOACTIVATE) instead of the system move loop (WM_NCLBUTTONDOWN +
 *        HTCAPTION), which is modal on the UI thread and would activate the window; the button is read with
 *        GetAsyncKeyState, which reports physical buttons, so swapped buttons (SM_SWAPBUTTON) read the right one.
 * WHERE: Built by app/bootstrap, attached by app/windows.rs on RunEvent::Ready, driven through `dyn OverlayWindow`
 *        by pipeline/pill.rs.
 */

use parking_lot::Mutex;
use windows::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::{
        ClientToScreen, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTONULL,
        MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint, MonitorFromRect,
    },
    UI::{
        HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI},
        Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON},
        WindowsAndMessaging::{
            GWL_EXSTYLE, GetCursorPos, GetSystemMetrics, GetWindowLongPtrW, GetWindowRect,
            HWND_TOPMOST, SET_WINDOW_POS_FLAGS, SM_SWAPBUTTON, SW_HIDE, SWP_FRAMECHANGED,
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW,
            SetWindowLongPtrW, SetWindowPos, ShowWindow, WS_EX_APPWINDOW, WS_EX_LAYERED,
            WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
        },
    },
};

use crate::{
    adapters::win32::hwnd,
    ports::OverlayWindow,
    types::{
        AppError, OverlayPlacement, OverlayRect, PortError, PortResult, ScreenPoint, ScreenRect,
        WindowHandle,
    },
};

/// The DPI at which one CSS pixel is one physical pixel.
const BASE_DPI: u32 = 96;

/// The pill window, driven through Win32.
#[derive(Default)]
pub struct Win32OverlayWindow {
    window: Mutex<Option<WindowHandle>>,
}

impl Win32OverlayWindow {
    pub fn new() -> Self {
        Self::default()
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: attach overlay, pill window styles, tool window, never activate, click-through at start
     * WHAT:  Takes over `window`: adds WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TRANSPARENT (and
     *        drops WS_EX_APPWINDOW), so it has no taskbar or Alt+Tab entry, is never activated and passes clicks
     *        through until a button needs them.
     * WHY:   `focusable: false` in tauri.conf.json already sets WS_EX_NOACTIVATE; it is set again here so the rule
     *        does not depend on a config flag (05 W3).
     * WHERE: app/windows.rs on RunEvent::Ready, once the pill window exists.
     */
    pub fn attach(&self, window: WindowHandle) -> PortResult<()> {
        let handle = hwnd(window);
        let styles = ex_styles(handle);
        let wanted = (styles
            | WS_EX_TOOLWINDOW.0
            | WS_EX_NOACTIVATE.0
            | WS_EX_LAYERED.0
            | WS_EX_TRANSPARENT.0)
            & !WS_EX_APPWINDOW.0;
        set_ex_styles(handle, wanted);
        // SAFETY: refreshes the frame of a live window after its styles changed; nothing moves or activates.
        unsafe {
            SetWindowPos(
                handle,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            )
        }
        .map_err(|error| {
            failure(format!(
                "the pill window styles could not be applied: {error}"
            ))
        })?;
        *self.window.lock() = Some(window);
        Ok(())
    }

    fn handle(&self) -> PortResult<HWND> {
        self.window
            .lock()
            .map(hwnd)
            .ok_or_else(|| failure(String::from("the pill window is not attached yet")))
    }
}

impl OverlayWindow for Win32OverlayWindow {
    fn show(&self, placement: OverlayPlacement) -> PortResult<()> {
        let handle = self.handle()?;
        match placement {
            OverlayPlacement::At(corner) if on_a_monitor(&window_rect(handle)?, corner) => {
                show_at(handle, corner)
            }
            OverlayPlacement::At(_) | OverlayPlacement::BottomCentre(None) => {
                show_bottom_centre(handle, &primary_work_area()?)
            }
            OverlayPlacement::BottomCentre(Some(area)) => show_bottom_centre(handle, &area),
        }
    }

    fn hide(&self) -> PortResult<()> {
        let handle = self.handle()?;
        // SAFETY: hides a live window; the return value is its previous visibility, not an error.
        let _ = unsafe { ShowWindow(handle, SW_HIDE) };
        Ok(())
    }

    fn set_click_through(&self, through: bool) -> PortResult<()> {
        let handle = self.handle()?;
        let styles = ex_styles(handle);
        let wanted = if through {
            styles | WS_EX_TRANSPARENT.0
        } else {
            styles & !WS_EX_TRANSPARENT.0
        };
        if wanted != styles {
            set_ex_styles(handle, wanted);
        }
        Ok(())
    }

    fn pointer_over(&self, areas: &[OverlayRect]) -> PortResult<bool> {
        if areas.is_empty() {
            return Ok(false);
        }
        let handle = self.handle()?;
        let mut cursor = POINT::default();
        // SAFETY: writes the cursor position into a POINT this function owns.
        unsafe { GetCursorPos(&raw mut cursor) }
            .map_err(|error| failure(format!("the cursor position could not be read: {error}")))?;
        let mut origin = POINT::default();
        // SAFETY: converts the client origin of a live window into screen coordinates, in place.
        if !unsafe { ClientToScreen(handle, &raw mut origin) }.as_bool() {
            return Err(failure(String::from(
                "the pill's client origin could not be read",
            )));
        }
        // SAFETY: reads the DPI of a live window; 0 means the handle was not valid.
        let dpi = unsafe { GetDpiForWindow(handle) };
        if dpi == 0 {
            return Err(failure(String::from("the pill's DPI could not be read")));
        }
        let scale = f64::from(dpi) / f64::from(BASE_DPI);
        let x = f64::from(cursor.x - origin.x) / scale;
        let y = f64::from(cursor.y - origin.y) / scale;
        Ok(areas.iter().any(|area| area.contains(x, y)))
    }

    fn origin(&self) -> PortResult<ScreenPoint> {
        let rect = window_rect(self.handle()?)?;
        Ok(ScreenPoint {
            x: rect.x,
            y: rect.y,
        })
    }

    fn move_to(&self, corner: ScreenPoint) -> PortResult<()> {
        place(self.handle()?, corner.x, corner.y, SET_WINDOW_POS_FLAGS(0))
    }

    fn cursor(&self) -> PortResult<ScreenPoint> {
        let mut cursor = POINT::default();
        // SAFETY: writes the cursor position into a POINT this function owns.
        unsafe { GetCursorPos(&raw mut cursor) }
            .map_err(|error| failure(format!("the cursor position could not be read: {error}")))?;
        Ok(ScreenPoint {
            x: cursor.x,
            y: cursor.y,
        })
    }

    fn primary_button_down(&self) -> PortResult<bool> {
        // SAFETY: reads a system metric; no memory is passed.
        let swapped = unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0;
        let button = if swapped { VK_RBUTTON } else { VK_LBUTTON };
        // SAFETY: reads the async state of a virtual key; the most significant bit set (negative) is "down now".
        let state = unsafe { GetAsyncKeyState(i32::from(button.0)) };
        Ok(state < 0)
    }
}

/// Shows the window at the bottom centre of `area`, sized for that monitor's DPI.
fn show_bottom_centre(handle: HWND, area: &ScreenRect) -> PortResult<()> {
    let (width, height) = size_on(handle, area)?;
    let (x, y) = bottom_centre(area, width, height);
    place(handle, x, y, SWP_SHOWWINDOW)?;
    // Moving to a monitor with another DPI made Windows resize the window: place it again with its real size.
    let actual = window_rect(handle)?;
    let (x, y) = bottom_centre(area, actual.width, actual.height);
    if (x, y) != (actual.x, actual.y) {
        place(handle, x, y, SWP_NOZORDER)?;
    }
    Ok(())
}

/// Shows the window with its top-left corner at `corner`.
fn show_at(handle: HWND, corner: ScreenPoint) -> PortResult<()> {
    place(handle, corner.x, corner.y, SWP_SHOWWINDOW)?;
    // A DPI change on the way resizes the window around a suggested rectangle: put the corner back.
    let actual = window_rect(handle)?;
    if (actual.x, actual.y) != (corner.x, corner.y) {
        place(handle, corner.x, corner.y, SWP_NOZORDER)?;
    }
    Ok(())
}

/**
 * SOURCE OF TRUTH KEYWORDS: on_a_monitor, saved pill corner check, monitor unplugged, off-screen pill fallback
 * WHAT:  Whether a window of `current`'s size with its corner at `corner` has its centre on a connected monitor.
 * WHY:   A position saved on a monitor that was unplugged or rearranged since would show the pill off screen,
 *        where it could never be dragged back; the centre is what the user sees of the pill.
 * WHERE: Win32OverlayWindow::show for OverlayPlacement::At.
 */
fn on_a_monitor(current: &ScreenRect, corner: ScreenPoint) -> bool {
    let centre = centre_at(current, corner);
    // SAFETY: looks up the monitor under a point; the point is a value, a null handle means none.
    let monitor = unsafe { MonitorFromPoint(centre, MONITOR_DEFAULTTONULL) };
    !monitor.is_invalid()
}

/// The centre of a window of `size`'s width and height whose corner is at `corner`.
fn centre_at(size: &ScreenRect, corner: ScreenPoint) -> POINT {
    let half = |length: u32| i32::try_from(length / 2).unwrap_or(0);
    POINT {
        x: corner.x.saturating_add(half(size.width)),
        y: corner.y.saturating_add(half(size.height)),
    }
}

fn failure(detail: String) -> PortError {
    PortError::new(AppError::Internal).with_detail(detail)
}

fn ex_styles(handle: HWND) -> u32 {
    // SAFETY: reads the extended styles of a window; an invalid handle reads 0.
    let styles = unsafe { GetWindowLongPtrW(handle, GWL_EXSTYLE) };
    // Extended styles are a 32-bit mask stored in a pointer-sized slot.
    u32::try_from(styles & 0xFFFF_FFFF).unwrap_or(0)
}

fn set_ex_styles(handle: HWND, styles: u32) {
    // SAFETY: writes the extended-style mask of a live window; the previous value is not needed.
    let _ = unsafe { SetWindowLongPtrW(handle, GWL_EXSTYLE, isize::try_from(styles).unwrap_or(0)) };
}

/// Moves the window to (`x`, `y`) without resizing or activating it; `extra` adds SWP flags.
fn place(handle: HWND, x: i32, y: i32, extra: SET_WINDOW_POS_FLAGS) -> PortResult<()> {
    // SAFETY: repositions a live window above all non-topmost windows; no activation is requested.
    unsafe {
        SetWindowPos(
            handle,
            Some(HWND_TOPMOST),
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | extra,
        )
    }
    .map_err(|error| failure(format!("the pill could not be placed: {error}")))
}

fn window_rect(handle: HWND) -> PortResult<ScreenRect> {
    let mut rect = RECT::default();
    // SAFETY: writes the screen rectangle of a live window into a RECT this function owns.
    unsafe { GetWindowRect(handle, &raw mut rect) }
        .map_err(|error| failure(format!("the pill's rectangle could not be read: {error}")))?;
    Ok(screen_rect(rect))
}

fn screen_rect(rect: RECT) -> ScreenRect {
    ScreenRect {
        x: rect.left,
        y: rect.top,
        width: u32::try_from(rect.right - rect.left).unwrap_or(0),
        height: u32::try_from(rect.bottom - rect.top).unwrap_or(0),
    }
}

/// The window's size in physical pixels once it sits on the monitor holding `area`.
fn size_on(handle: HWND, area: &ScreenRect) -> PortResult<(u32, u32)> {
    let current = window_rect(handle)?;
    // SAFETY: reads the DPI of a live window; 0 means the handle was not valid.
    let window_dpi = unsafe { GetDpiForWindow(handle) };
    let target_dpi = monitor_dpi(area);
    if window_dpi == 0 || target_dpi == 0 {
        return Ok((current.width, current.height));
    }
    Ok((
        rescale(current.width, window_dpi, target_dpi),
        rescale(current.height, window_dpi, target_dpi),
    ))
}

/// The effective DPI of the monitor holding `area`; 0 when it cannot be read.
fn monitor_dpi(area: &ScreenRect) -> u32 {
    let rect = RECT {
        left: area.x,
        top: area.y,
        right: area
            .x
            .saturating_add(i32::try_from(area.width).unwrap_or(i32::MAX)),
        bottom: area
            .y
            .saturating_add(i32::try_from(area.height).unwrap_or(i32::MAX)),
    };
    // SAFETY: looks up the monitor nearest a rectangle this function owns.
    let monitor = unsafe { MonitorFromRect(&raw const rect, MONITOR_DEFAULTTONEAREST) };
    let (mut dpi_x, mut dpi_y) = (0, 0);
    // SAFETY: writes the monitor's DPI into two integers this function owns.
    match unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &raw mut dpi_x, &raw mut dpi_y) } {
        Ok(()) => dpi_x,
        Err(_) => 0,
    }
}

/// The primary monitor's work area (the screen minus the taskbar), in physical pixels.
fn primary_work_area() -> PortResult<ScreenRect> {
    // SAFETY: looks up the primary monitor; the point is a value.
    let monitor = unsafe { MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY) };
    let mut info = MONITORINFO {
        cbSize: u32::try_from(size_of::<MONITORINFO>()).unwrap_or(0),
        ..MONITORINFO::default()
    };
    // SAFETY: fills a MONITORINFO whose cbSize is set, for a monitor handle just returned.
    if !unsafe { GetMonitorInfoW(monitor, &raw mut info) }.as_bool() {
        return Err(failure(String::from(
            "the primary monitor could not be read",
        )));
    }
    Ok(screen_rect(info.rcWork))
}

/// `length` measured at `from` DPI, measured again at `to` DPI.
fn rescale(length: u32, from: u32, to: u32) -> u32 {
    let scaled = u64::from(length) * u64::from(to) / u64::from(from.max(1));
    u32::try_from(scaled).unwrap_or(u32::MAX)
}

/**
 * SOURCE OF TRUTH KEYWORDS: bottom_centre, pill placement math, work area bottom centre
 * WHAT:  The top-left corner that puts a `width` × `height` window at the bottom centre of `area`.
 * WHY:   The window's bottom edge sits on the work area's bottom edge (the taskbar's top); the page itself keeps the
 *        visible pill --space-6 above that edge (04 §4), so the gap scales with the page, in CSS, where the token is.
 * WHERE: Win32OverlayWindow::show.
 */
fn bottom_centre(area: &ScreenRect, width: u32, height: u32) -> (i32, i32) {
    let spare = i64::from(area.width) - i64::from(width);
    let x = i64::from(area.x) + spare / 2;
    let y = i64::from(area.y) + i64::from(area.height) - i64::from(height);
    (
        i32::try_from(x).unwrap_or(area.x),
        i32::try_from(y).unwrap_or(area.y),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAPTOP: ScreenRect = ScreenRect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1032,
    };

    #[test]
    fn the_pill_sits_centred_on_the_taskbar_edge() {
        assert_eq!(bottom_centre(&LAPTOP, 360, 88), (780, 944));
        // A monitor left of the primary one has negative coordinates.
        let left = ScreenRect {
            x: -2560,
            y: -200,
            width: 2560,
            height: 1400,
        };
        assert_eq!(bottom_centre(&left, 540, 132), (-1550, 1068));
        // A window wider than the area still centres (it overhangs both sides equally).
        assert_eq!(
            bottom_centre(
                &ScreenRect {
                    width: 300,
                    ..LAPTOP
                },
                360,
                88
            ),
            (-30, 944)
        );
    }

    #[test]
    fn a_saved_corner_is_kept_only_while_a_monitor_shows_the_window() {
        let pill = ScreenRect {
            x: 0,
            y: 0,
            width: 360,
            height: 88,
        };
        assert_eq!(
            centre_at(&pill, ScreenPoint { x: 10, y: 20 }),
            POINT { x: 190, y: 64 }
        );
        let primary = primary_work_area().unwrap();
        let inside = ScreenPoint {
            x: primary.x,
            y: primary.y,
        };
        assert!(on_a_monitor(&pill, inside));
        let far_away = ScreenPoint {
            x: i32::MAX - 100,
            y: i32::MAX - 100,
        };
        assert!(!on_a_monitor(&pill, far_away));
    }

    #[test]
    fn the_cursor_and_the_button_read_without_a_window() {
        let overlay = Win32OverlayWindow::new();
        assert!(overlay.cursor().is_ok());
        assert!(overlay.primary_button_down().is_ok());
    }

    #[test]
    fn sizes_rescale_between_dpis() {
        assert_eq!(rescale(360, 96, 144), 540);
        assert_eq!(rescale(540, 144, 96), 360);
        assert_eq!(rescale(88, 120, 120), 88);
        assert_eq!(
            rescale(88, 0, 96),
            8_448,
            "a zero DPI never divides by zero"
        );
    }

    #[test]
    fn an_unattached_overlay_fails_instead_of_touching_a_window() {
        let overlay = Win32OverlayWindow::new();
        for result in [
            overlay.show(OverlayPlacement::BottomCentre(None)),
            overlay.show(OverlayPlacement::At(ScreenPoint { x: 0, y: 0 })),
            overlay.hide(),
            overlay.set_click_through(true),
            overlay.move_to(ScreenPoint { x: 0, y: 0 }),
            overlay.origin().map(|_| ()),
        ] {
            assert_eq!(
                result.map_err(PortError::into_app_error),
                Err(AppError::Internal)
            );
        }
        let stop = OverlayRect {
            x: 0,
            y: 0,
            width: 28,
            height: 28,
        };
        assert!(overlay.pointer_over(&[stop]).is_err());
        assert!(
            !overlay.pointer_over(&[]).unwrap(),
            "no area needs no window"
        );
    }

    #[test]
    fn the_primary_work_area_is_on_screen() {
        let area = primary_work_area().unwrap();
        assert!(area.width > 0 && area.height > 0);
    }
}
