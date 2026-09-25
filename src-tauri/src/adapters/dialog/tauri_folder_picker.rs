/*!
 * SOURCE OF TRUTH KEYWORDS: TauriFolderPicker, tauri-plugin-dialog, pick folder dialog, IFileOpenDialog, parent window, import model folder
 * WHAT:  TauriFolderPicker: FolderPicker on the Tauri dialog plugin, shown as a modal of the main window; resolves
 *        to the chosen folder or None.
 * WHY:   The plugin wraps the Windows folder dialog on its own thread and calls back when the user answers, so the
 *        command waits on a oneshot instead of blocking a runtime thread. It is Rust-side only (app/plugins.rs
 *        registers it, the webview gets no dialog permission), so a folder path only ever comes from the user's
 *        own choice. The plugin's state is looked up without panicking (a missing plugin is `Internal` with detail),
 *        and a parent window that does not exist yet just leaves the dialog unparented. A `file://` answer that is
 *        not a local path is `Internal`: Windows always answers with a path.
 * WHERE: Built by app/bootstrap; held by pipeline/models as `dyn FolderPicker` (`models_import`).
 */

use std::path::PathBuf;

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_dialog::{Dialog, FilePath};
use tokio::sync::oneshot;

use crate::{
    ports::FolderPicker,
    types::{AppError, BoxFuture, PortError, PortResult},
};

/// The system folder picker through the Tauri dialog plugin.
pub struct TauriFolderPicker<R: Runtime> {
    app: AppHandle<R>,
    /// Label of the window the dialog is modal to.
    parent: &'static str,
}

impl<R: Runtime> TauriFolderPicker<R> {
    pub fn new(app: AppHandle<R>, parent: &'static str) -> Self {
        Self { app, parent }
    }
}

impl<R: Runtime> FolderPicker for TauriFolderPicker<R> {
    fn pick_folder<'a>(&'a self, title: &'a str) -> BoxFuture<'a, PortResult<Option<PathBuf>>> {
        Box::pin(async move {
            let dialog = self.app.try_state::<Dialog<R>>().ok_or_else(|| {
                PortError::new(AppError::Internal)
                    .with_detail("the dialog plugin is not registered (app/plugins.rs)")
            })?;
            let mut builder = dialog.file().set_title(title);
            if let Some(window) = self.app.get_webview_window(self.parent) {
                builder = builder.set_parent(&window);
            }
            let (answer, answered) = oneshot::channel();
            builder.pick_folder(move |folder| {
                // The picker's caller may have stopped waiting; nothing else needs the answer.
                let _ = answer.send(folder);
            });
            let folder = answered.await.map_err(|_| {
                PortError::new(AppError::Internal)
                    .with_detail("the folder picker closed without answering")
            })?;
            folder.map(local_path).transpose()
        })
    }
}

fn local_path(folder: FilePath) -> PortResult<PathBuf> {
    folder.into_path().map_err(|error| {
        PortError::new(AppError::Internal).with_detail(format!(
            "the folder picker answered with no local path: {error}"
        ))
    })
}
