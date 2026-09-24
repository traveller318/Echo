/*!
 * SOURCE OF TRUTH KEYWORDS: TauriMainWindow, show main window, unminimize, set_focus, bring main window forward
 * WHAT:  TauriMainWindow: MainWindow over the Tauri window with the given label: restores it if minimised, shows it
 *        and focuses it.
 * WHY:   The main window is a normal Tauri window (Tauri owns its visibility, unlike the pill), so the Tauri API is
 *        the right tool; the label is handed in by the composition root, which owns the window names. Focusing is
 *        allowed here because the request always follows a click on an Echo surface (the pill, the tray), which
 *        gives Echo the foreground right.
 * WHERE: Built by app/bootstrap into CommandCtx; called through `dyn MainWindow` by ipc/commands/system.rs
 *        (`app_open_page`), later by the tray and single-instance handling (step 25).
 */

use tauri::{AppHandle, Manager, Runtime};

use crate::{
    ports::MainWindow,
    types::{AppError, PortError, PortResult},
};

/// The main window through Tauri.
pub struct TauriMainWindow<R: Runtime> {
    app: AppHandle<R>,
    label: &'static str,
}

impl<R: Runtime> TauriMainWindow<R> {
    pub fn new(app: AppHandle<R>, label: &'static str) -> Self {
        Self { app, label }
    }
}

fn failure(detail: String) -> PortError {
    PortError::new(AppError::Internal).with_detail(detail)
}

impl<R: Runtime> MainWindow for TauriMainWindow<R> {
    fn show(&self) -> PortResult<()> {
        let window = self
            .app
            .get_webview_window(self.label)
            .ok_or_else(|| failure(format!("there is no {} window", self.label)))?;
        if window.is_minimized().unwrap_or(false) {
            window.unminimize().map_err(|error| {
                failure(format!("the main window could not be restored: {error}"))
            })?;
        }
        window
            .show()
            .map_err(|error| failure(format!("the main window could not be shown: {error}")))?;
        window
            .set_focus()
            .map_err(|error| failure(format!("the main window could not be focused: {error}")))
    }
}

#[cfg(test)]
mod tests {
    use tauri::test::{mock_builder, mock_context, noop_assets};

    use super::*;

    #[test]
    fn a_missing_window_is_an_error_not_a_panic() {
        let app = mock_builder().build(mock_context(noop_assets())).unwrap();
        let main = TauriMainWindow::new(app.handle().clone(), "main");
        assert_eq!(
            main.show().map_err(PortError::into_app_error),
            Err(AppError::Internal)
        );
    }
}
