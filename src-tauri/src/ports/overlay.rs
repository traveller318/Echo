/*!
 * SOURCE OF TRUTH KEYWORDS: OverlayWindow, pill window, show without activating, click-through, pointer over, bottom centre placement, SW_SHOWNOACTIVATE, work area
 * WHAT:  OverlayWindow: the always-on-top surface that floats over other apps without ever taking focus (the pill).
 *        Show it at the bottom centre of a monitor's work area, hide it, let clicks pass through it or not, and ask
 *        whether the pointer is over one of its clickable areas.
 * WHY:   Showing any window normally steals focus, and the paste would then land in the wrong place (05 W3), so
 *        showing is a Windows detail behind this port (root CLAUDE.md §3). Placement takes the work area in
 *        physical pixels because the pill must sit on the monitor of the window the user is typing in, whatever its
 *        DPI (05 W15); the adapter scales its own size for that monitor. Clicks pass through everywhere except the
 *        pill's buttons (04 §4), and a click-through window receives no pointer events at all, so the pipeline
 *        asks `pointer_over` on a timer and switches click-through off while the pointer is on a button. Every
 *        method is blocking and short (a window-manager call) and safe to repeat.
 * WHERE: Implemented by adapters/window/overlay.rs (Win32OverlayWindow) and ports/fakes; driven by
 *        pipeline/pill.rs (PillPresenter).
 */

use crate::types::{OverlayRect, PortResult, ScreenRect};

/// A focus-neutral, always-on-top overlay window.
pub trait OverlayWindow: Send + Sync {
    /// Places the overlay at the bottom centre of `work_area` (physical pixels; None = the primary monitor's) and
    /// shows it above every window without activating it or any other window.
    fn show(&self, work_area: Option<ScreenRect>) -> PortResult<()>;

    /// Hides the overlay; hiding a hidden overlay is a no-op.
    fn hide(&self) -> PortResult<()>;

    /// `true`: clicks go through to the windows below; `false`: the overlay receives them.
    fn set_click_through(&self, through: bool) -> PortResult<()>;

    /// Whether the pointer is over one of `areas` (CSS pixels of the overlay's page).
    fn pointer_over(&self, areas: &[OverlayRect]) -> PortResult<bool>;
}
