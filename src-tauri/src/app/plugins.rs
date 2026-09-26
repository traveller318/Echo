/*!
 * SOURCE OF TRUTH KEYWORDS: plugins, plugin registration, tauri plugin wiring, single instance plugin, second launch focuses window, notification plugin, dialog plugin
 * WHAT:  `register`: adds every Tauri plugin Echo uses to the app builder: single-instance first, then the toast and
 *        dialog plugins. A second launch of Echo hands its arguments to this process and exits; this process then
 *        shows its main window (unless the second launch was Windows' start at sign-in).
 * WHY:   Only app/ wires plugins (02 §3.2), and only this file does, so the list of plugins in the binary is read
 *        in one place. single-instance must be registered first so a second process exits before it builds
 *        anything, which is what keeps it from becoming a second hotkey owner (05 W20); the running Echo answers
 *        through the MainWindow port like every other "show Echo" request. A start at sign-in that finds Echo
 *        already running is not a request for the window. Plugins are Rust-side services for their adapters
 *        (TauriToastNotifier, TauriFolderPicker); the webview gets none of their permissions (capabilities/), so
 *        the UI cannot raise a toast or open a dialog behind the pipeline's back. Hotkeys need no plugin: the
 *        keyboard hook adapter runs its own thread (adapters/hotkey); start at sign-in needs none either
 *        (adapters/startup writes the Run key itself, 05 decision log step 25). A plugin's state exists once the
 *        app is built, which is before app/bootstrap constructs the adapters that look it up.
 * WHERE: app::run, on the builder before `build`.
 */

use tauri::{AppHandle, Builder, Manager, Runtime};

use crate::{ipc::CommandCtx, pipeline::launch, types::LaunchOrigin};

/// `builder` with Echo's plugins registered.
pub fn register<R: Runtime>(builder: Builder<R>) -> Builder<R> {
    builder
        .plugin(tauri_plugin_single_instance::init(on_second_launch))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
}

/// Another Echo was started: bring this one's window forward, unless Windows started it at sign-in.
fn on_second_launch<R: Runtime>(app: &AppHandle<R>, args: Vec<String>, _cwd: String) {
    if launch::origin_from_args(&args) == LaunchOrigin::Login {
        tracing::info!("a start at sign-in found Echo already running");
        return;
    }
    tracing::info!("a second launch brings the running Echo forward");
    let Some(ctx) = app.try_state::<CommandCtx>() else {
        tracing::warn!("a second launch came before startup finished");
        return;
    };
    if let Err(error) = ctx.main_window().show() {
        tracing::warn!(
            detail = error.detail(),
            "the main window could not be shown"
        );
    }
}
