/*!
 * SOURCE OF TRUTH KEYWORDS: window adapters, OverlayWindow implementation, MainWindow implementation, Win32OverlayWindow, TauriMainWindow, pill window, main window
 * WHAT:  Adapters for Echo's own windows: the pill overlay (Win32OverlayWindow, Win32 styles and placement) and the
 *        main window (TauriMainWindow, shown through Tauri).
 * WHY:   The pill needs Win32 calls Tauri does not expose (show without activation, click-through switching,
 *        per-monitor placement, 05 W3/W15), which stay behind the OverlayWindow port (root CLAUDE.md §3); the main
 *        window only needs Tauri's own show and focus, behind the MainWindow port so commands never hold a handle.
 * WHERE: Built by app/bootstrap; used through `dyn OverlayWindow` by pipeline/pill.rs and `dyn MainWindow` by
 *        ipc/commands/system.rs.
 */

mod main_window;
mod overlay;

pub use main_window::TauriMainWindow;
pub use overlay::Win32OverlayWindow;
