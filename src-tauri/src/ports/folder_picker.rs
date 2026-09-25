/*!
 * SOURCE OF TRUTH KEYWORDS: FolderPicker, pick folder, folder dialog, choose directory, import from folder, system file dialog
 * WHAT:  FolderPicker shows the system's folder picker and returns the folder the user chose, or None when they
 *        closed it.
 * WHY:   The dialog is Windows UI, so it stays behind a port (root CLAUDE.md §3) and the webview never gets a dialog
 *        permission: a command asks Rust for the folder, so the page cannot hand Echo an arbitrary path it made up.
 *        Async (BoxFuture) because it waits on the user; dropping the future stops waiting (the dialog itself closes
 *        with its window). Closing the picker is a normal answer (None), not an error.
 * WHERE: Implemented by adapters/dialog (TauriFolderPicker) and ports/fakes; called by pipeline/models for
 *        `models_import`; later any "choose a folder" action (export, backups) takes the same port.
 */

use std::path::PathBuf;

use crate::types::{BoxFuture, PortResult};

/// The system folder picker.
pub trait FolderPicker: Send + Sync {
    /// Shows the picker titled `title` over Echo's main window; None when the user closes it without choosing.
    fn pick_folder<'a>(&'a self, title: &'a str) -> BoxFuture<'a, PortResult<Option<PathBuf>>>;
}
