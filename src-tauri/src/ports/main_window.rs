/*!
 * SOURCE OF TRUTH KEYWORDS: MainWindow, show main window, bring to front, restore minimized, open Echo, focus main window
 * WHAT:  MainWindow: Echo's own main window, brought to the front (shown, restored if minimised, focused).
 * WHY:   Surfaces outside the main window (the pill's "Set up" and "Open", the tray's "Open Echo" in step 25, a second
 *        launch in step 25) must show it without holding a Tauri handle, so commands stay testable with a fake and
 *        never import the framework (02 §3.2). Blocking and short.
 * WHERE: Implemented by adapters/window/main_window.rs (TauriMainWindow) and ports/fakes; called by
 *        ipc/commands/system.rs (`app_open_page`).
 */

use crate::types::PortResult;

/// Echo's main window.
pub trait MainWindow: Send + Sync {
    /// Shows the main window, restores it if minimised and brings it to the front.
    fn show(&self) -> PortResult<()>;
}
