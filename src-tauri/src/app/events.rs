/*!
 * SOURCE OF TRUTH KEYWORDS: TauriEventSink, event sink adapter, emit to webview, AppEvent emit, Rust to UI events
 * WHAT:  TauriEventSink: the `EventSink<AppEvent>` the running app hands to commands (and later the pipeline); it
 *        sends each event to every window through the registry event catalog.
 * WHY:   Handlers and the pipeline emit through a port and never hold a Tauri handle, so they stay testable with a
 *        RecordingSink (02 §4.4). It lives in app/ because it holds the running app's AppHandle and only app/
 *        wires Tauri runtime state (02 §3.2); the typed names come from the registry catalog. A failed emit (a window closing mid-send) is logged
 *        and never surfaces as a command error: the UI re-reads through a command anyway.
 * WHERE: Built by app/bootstrap into CommandDeps; `emit` calls `registry::events::emit`.
 */

use tauri::{AppHandle, Runtime};

use crate::{ports::EventSink, registry, types::AppEvent};

/// Sends AppEvents to the app's windows.
pub struct TauriEventSink<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> TauriEventSink<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

impl<R: Runtime> EventSink<AppEvent> for TauriEventSink<R> {
    fn emit(&self, event: AppEvent) {
        if let Err(error) = registry::events::emit(&self.app, event) {
            tracing::warn!(%error, "event could not be delivered to the windows");
        }
    }
}
