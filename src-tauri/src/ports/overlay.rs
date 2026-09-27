/*!
 * SOURCE OF TRUTH KEYWORDS: OverlayWindow, pill window, show without activating, click-through, pointer over, bottom centre placement, SW_SHOWNOACTIVATE, work area, drag pill, move overlay, cursor position, primary button
 * WHAT:  OverlayWindow: the always-on-top surface that floats over other apps without ever taking focus (the pill).
 *        Show it at the bottom centre of a monitor's work area or at a saved corner, hide it, let clicks pass through
 *        it or not, ask whether the pointer is over one of its clickable areas, and the pieces a drag is made of:
 *        where the window and the cursor are, whether the primary mouse button is still down, and a move.
 * WHY:   Showing any window normally steals focus, and the paste would then land in the wrong place (05 W3), so
 *        showing is a Windows detail behind this port (root CLAUDE.md §3). Placement takes the work area in
 *        physical pixels because the pill must sit on the monitor of the window the user is typing in, whatever its
 *        DPI (05 W15); the adapter scales its own size for that monitor. Clicks pass through everywhere except the
 *        pill's buttons (04 §4), and a click-through window receives no pointer events at all, so the pipeline
 *        asks `pointer_over` on a timer and switches click-through off while the pointer is on a button. A drag is
 *        run by the pipeline from these primitives (window corner + cursor offset, until the button is released)
 *        rather than by the system move loop, which activates windows and is modal; the port holds no drag logic.
 *        Every method is blocking and short (a window-manager call) and safe to repeat.
 * WHERE: Implemented by adapters/window/overlay.rs (Win32OverlayWindow) and ports/fakes; driven by
 *        pipeline/pill.rs (PillPresenter).
 */

use crate::types::{OverlayPlacement, OverlayRect, PortResult, ScreenPoint};

/// A focus-neutral, always-on-top overlay window.
pub trait OverlayWindow: Send + Sync {
    /// Places the overlay where `placement` says (physical pixels; a saved corner no monitor shows any more falls
    /// back to the primary monitor's bottom centre) and shows it above every window without activating any.
    fn show(&self, placement: OverlayPlacement) -> PortResult<()>;

    /// Hides the overlay; hiding a hidden overlay is a no-op.
    fn hide(&self) -> PortResult<()>;

    /// `true`: clicks go through to the windows below; `false`: the overlay receives them.
    fn set_click_through(&self, through: bool) -> PortResult<()>;

    /// Whether the pointer is over one of `areas` (CSS pixels of the overlay's page).
    fn pointer_over(&self, areas: &[OverlayRect]) -> PortResult<bool>;

    /// The overlay window's top-left corner (physical pixels).
    fn origin(&self) -> PortResult<ScreenPoint>;

    /// Moves the overlay's top-left corner to `corner` without resizing, activating or showing it.
    fn move_to(&self, corner: ScreenPoint) -> PortResult<()>;

    /// The cursor position (physical pixels).
    fn cursor(&self) -> PortResult<ScreenPoint>;

    /// Whether the primary mouse button is held now (the left one, or the right one when the buttons are swapped).
    fn primary_button_down(&self) -> PortResult<bool>;
}
