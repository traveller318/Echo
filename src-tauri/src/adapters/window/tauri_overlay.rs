/*!
 * SOURCE OF TRUTH KEYWORDS: TauriOverlayWindow, pill overlay Linux, always on top, skip taskbar, ignore cursor events, bottom centre placement, cursor_position
 * WHAT:  TauriOverlayWindow: OverlayWindow over Tauri's own window API (always-on-top, skip-taskbar, click-through via
 *        `set_ignore_cursor_events`, `set_position`, `cursor_position`). Bottom-centre placement uses the work area
 *        the foreground adapter reported, else the primary monitor.
 * WHY:   GTK/X11 windows are positioned and kept on top by the window manager, so the portable Tauri calls do what
 *        Win32's SetWindowPos does on Windows; `focusable: false` in tauri.conf.json keeps the pill from taking
 *        focus. Positions are physical pixels like the Win32 adapter's. Wayland compositors ignore client-side
 *        positioning (a plain window cannot place itself), so there the pill appears where the compositor puts it;
 *        a layer-shell implementation replaces this adapter for Wayland where the compositor offers one (plan
 *        phase 2). The pointer button state has no portable API, so `primary_button_down` is false until the X11
 *        adapter reads it; the pill's drag then does nothing, and a pill that cannot be moved is still shown.
 * WHERE: Built by app/bootstrap on Linux, attached by app/windows.rs on RunEvent::Ready, driven through
 *        `dyn OverlayWindow` by pipeline/pill.rs.
 */

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager, PhysicalPosition, Runtime, WebviewWindow};

use crate::{
    ports::OverlayWindow,
    types::{
        AppError, OverlayPlacement, OverlayRect, PortError, PortResult, ScreenPoint, ScreenRect,
    },
};

/// The pill window, driven through Tauri.
pub struct TauriOverlayWindow<R: Runtime> {
    app: AppHandle<R>,
    label: &'static str,
    /// The click-through state asked for; GTK has no surface to apply it to until the window has been shown.
    click_through: AtomicBool,
    shown: AtomicBool,
}

impl<R: Runtime> TauriOverlayWindow<R> {
    pub fn new(app: AppHandle<R>, label: &'static str) -> Self {
        Self {
            app,
            label,
            click_through: AtomicBool::new(true),
            shown: AtomicBool::new(false),
        }
    }

    /// Makes the window an overlay: above the others and off the taskbar. Click-through starts on and is applied with
    /// the first `show`, because GTK cannot shape the input of a window that has no surface yet.
    pub fn attach(&self) -> PortResult<()> {
        let window = self.window()?;
        window
            .set_always_on_top(true)
            .and_then(|()| window.set_skip_taskbar(true))
            .map_err(|error| failure(format!("the pill window could not be set up: {error}")))
    }

    /// Applies the wanted click-through state to a window that has been shown.
    fn apply_click_through(&self, window: &WebviewWindow<R>) -> PortResult<()> {
        window
            .set_ignore_cursor_events(self.click_through.load(Ordering::Acquire))
            .map_err(|error| failure(format!("click-through could not change: {error}")))
    }

    fn window(&self) -> PortResult<WebviewWindow<R>> {
        self.app
            .get_webview_window(self.label)
            .ok_or_else(|| failure(format!("there is no {} window", self.label)))
    }
}

fn failure(detail: String) -> PortError {
    PortError::new(AppError::Internal).with_detail(detail)
}

impl<R: Runtime> OverlayWindow for TauriOverlayWindow<R> {
    fn show(&self, placement: OverlayPlacement) -> PortResult<()> {
        let window = self.window()?;
        let size = window
            .outer_size()
            .map_err(|e| failure(format!("pill size: {e}")))?;
        let corner = match placement {
            OverlayPlacement::At(corner) if self.on_a_monitor(corner) => corner,
            OverlayPlacement::At(_) | OverlayPlacement::BottomCentre(None) => {
                let area = self.primary_work_area()?;
                bottom_centre(&area, size.width, size.height)
            }
            OverlayPlacement::BottomCentre(Some(area)) => {
                bottom_centre(&area, size.width, size.height)
            }
        };
        window
            .set_position(PhysicalPosition::new(corner.x, corner.y))
            .map_err(|error| failure(format!("the pill could not be moved: {error}")))?;
        window
            .show()
            .map_err(|error| failure(format!("the pill could not be shown: {error}")))?;
        self.shown.store(true, Ordering::Release);
        self.apply_click_through(&window)?;
        // Some window managers drop the hint when a window is hidden.
        window
            .set_always_on_top(true)
            .map_err(|error| failure(format!("the pill could not be raised: {error}")))
    }

    fn hide(&self) -> PortResult<()> {
        self.shown.store(false, Ordering::Release);
        self.window()?
            .hide()
            .map_err(|error| failure(format!("the pill could not be hidden: {error}")))
    }

    fn set_click_through(&self, through: bool) -> PortResult<()> {
        self.click_through.store(through, Ordering::Release);
        if self.shown.load(Ordering::Acquire) {
            self.apply_click_through(&self.window()?)?;
        }
        Ok(())
    }

    fn pointer_over(&self, areas: &[OverlayRect]) -> PortResult<bool> {
        let window = self.window()?;
        let cursor = window
            .cursor_position()
            .map_err(|error| failure(format!("the cursor could not be read: {error}")))?;
        let origin = window
            .outer_position()
            .map_err(|error| failure(format!("the pill position could not be read: {error}")))?;
        let scale = window.scale_factor().unwrap_or(1.0);
        let x = (cursor.x - f64::from(origin.x)) / scale;
        let y = (cursor.y - f64::from(origin.y)) / scale;
        Ok(areas.iter().any(|area| {
            x >= f64::from(area.x)
                && x < f64::from(area.x) + f64::from(area.width)
                && y >= f64::from(area.y)
                && y < f64::from(area.y) + f64::from(area.height)
        }))
    }

    fn origin(&self) -> PortResult<ScreenPoint> {
        let origin = self
            .window()?
            .outer_position()
            .map_err(|error| failure(format!("the pill position could not be read: {error}")))?;
        Ok(ScreenPoint {
            x: origin.x,
            y: origin.y,
        })
    }

    fn move_to(&self, corner: ScreenPoint) -> PortResult<()> {
        self.window()?
            .set_position(PhysicalPosition::new(corner.x, corner.y))
            .map_err(|error| failure(format!("the pill could not be moved: {error}")))
    }

    fn cursor(&self) -> PortResult<ScreenPoint> {
        let cursor = self
            .window()?
            .cursor_position()
            .map_err(|error| failure(format!("the cursor could not be read: {error}")))?;
        Ok(ScreenPoint {
            x: cursor.x.round() as i32,
            y: cursor.y.round() as i32,
        })
    }

    fn primary_button_down(&self) -> PortResult<bool> {
        Ok(false)
    }
}

impl<R: Runtime> TauriOverlayWindow<R> {
    /// The work area of the primary monitor in physical pixels.
    fn primary_work_area(&self) -> PortResult<ScreenRect> {
        let monitor = self
            .app
            .primary_monitor()
            .map_err(|error| failure(format!("monitors could not be listed: {error}")))?
            .ok_or_else(|| failure(String::from("there is no primary monitor")))?;
        let area = monitor.work_area();
        Ok(ScreenRect {
            x: area.position.x,
            y: area.position.y,
            width: area.size.width,
            height: area.size.height,
        })
    }

    /// Whether `corner` lies on some monitor (a saved position of a monitor that is gone does not).
    fn on_a_monitor(&self, corner: ScreenPoint) -> bool {
        self.app.available_monitors().is_ok_and(|monitors| {
            monitors.iter().any(|monitor| {
                let (position, size) = (monitor.position(), monitor.size());
                corner.x >= position.x
                    && corner.y >= position.y
                    && i64::from(corner.x) < i64::from(position.x) + i64::from(size.width)
                    && i64::from(corner.y) < i64::from(position.y) + i64::from(size.height)
            })
        })
    }
}

/// The corner that centres a `width` × `height` window at the bottom of `area`, a margin above its edge.
fn bottom_centre(area: &ScreenRect, width: u32, height: u32) -> ScreenPoint {
    const MARGIN: i64 = 24;
    let x = i64::from(area.x) + (i64::from(area.width) - i64::from(width)) / 2;
    let y = i64::from(area.y) + i64::from(area.height) - i64::from(height) - MARGIN;
    ScreenPoint {
        x: i32::try_from(x).unwrap_or(area.x),
        y: i32::try_from(y).unwrap_or(area.y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pill_sits_centred_above_the_bottom_edge() {
        let area = ScreenRect {
            x: 100,
            y: 0,
            width: 1920,
            height: 1040,
        };
        assert_eq!(
            bottom_centre(&area, 360, 88),
            ScreenPoint { x: 880, y: 928 }
        );
    }
}
