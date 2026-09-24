/*!
 * SOURCE OF TRUTH KEYWORDS: plugins, plugin registration, tauri plugin wiring, notification plugin
 * WHAT:  `register`: adds every Tauri plugin Echo uses to the app builder.
 * WHY:   Only app/ wires plugins (02 §3.2), and only this file does, so the list of plugins in the binary is read
 *        in one place. Plugins are Rust-side services for their adapters (TauriToastNotifier); the webview gets
 *        none of their permissions (capabilities/), so the UI cannot raise a toast behind the pipeline's back.
 *        Hotkeys need no plugin: the keyboard hook adapter runs its own thread (adapters/hotkey). A plugin's
 *        state exists once the app is built, which is before app/bootstrap constructs the adapters that look it up.
 * WHERE: app::run, on the builder before `build`. Steps 25–26 add single-instance and autostart here.
 */

use tauri::{Builder, Runtime};

/// `builder` with Echo's plugins registered.
pub fn register<R: Runtime>(builder: Builder<R>) -> Builder<R> {
    builder.plugin(tauri_plugin_notification::init())
}
