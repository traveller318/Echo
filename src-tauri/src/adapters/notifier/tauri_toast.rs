/*!
 * SOURCE OF TRUTH KEYWORDS: TauriToastNotifier, tauri-plugin-notification, WinRT toast, AppUserModelID, AUMID, notification plugin state
 * WHAT:  TauriToastNotifier: Notifier on the Tauri notification plugin, which shows a WinRT toast with the toast's
 *        title and body.
 * WHY:   The plugin already speaks WinRT toasts and stamps the installed app's AppUserModelID (the identifier in
 *        tauri.conf.json, set on the NSIS shortcut); in `tauri dev` Windows shows the toast under another name or
 *        not at all, which is expected (05 W18). The plugin is registered only in app/plugins.rs, so its state is
 *        looked up without panicking: a missing plugin is an `Internal` error with detail, never a crash. The
 *        plugin shows the toast on its own task, so a failure inside WinRT is not seen here; only failures to hand
 *        the toast over are reported. ToastKind has no WinRT counterpart in this plugin and is only logged.
 * WHERE: Built by app/bootstrap into CommandCtx and pipeline/delivery.rs's Delivery; called through
 *        `dyn Notifier`.
 */

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_notification::Notification;

use crate::{
    ports::Notifier,
    types::{AppError, PortError, PortResult, Toast},
};

/// Native toasts through the Tauri notification plugin.
pub struct TauriToastNotifier<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> TauriToastNotifier<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

impl<R: Runtime> Notifier for TauriToastNotifier<R> {
    fn toast(&self, toast: &Toast) -> PortResult<()> {
        let notification = self.app.try_state::<Notification<R>>().ok_or_else(|| {
            PortError::new(AppError::Internal)
                .with_detail("the notification plugin is not registered (app/plugins.rs)")
        })?;
        tracing::debug!(kind = ?toast.kind, title = %toast.title, "showing a toast");
        notification
            .builder()
            .title(toast.title.as_str())
            .body(toast.body.as_str())
            .show()
            .map_err(|error| {
                PortError::new(AppError::Internal)
                    .with_detail(format!("the toast could not be shown: {error}"))
            })
    }
}

#[cfg(test)]
mod tests {
    use tauri::test::{mock_builder, mock_context, noop_assets};

    use super::*;
    use crate::types::{StaticStr, ToastKind};

    #[test]
    fn a_missing_plugin_is_an_error_not_a_panic() {
        let app = mock_builder().build(mock_context(noop_assets())).unwrap();
        let notifier = TauriToastNotifier::new(app.handle().clone());
        let result = notifier.toast(&Toast {
            kind: ToastKind::Info,
            title: StaticStr::new("Title"),
            body: StaticStr::new("Body"),
        });
        assert_eq!(
            result.map_err(PortError::into_app_error),
            Err(AppError::Internal)
        );
    }
}
