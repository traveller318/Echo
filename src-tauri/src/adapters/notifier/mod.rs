/*!
 * SOURCE OF TRUTH KEYWORDS: notifier adapters, Notifier implementations, TauriToastNotifier, native toast
 * WHAT:  Adapters behind the Notifier port.
 * WHY:   Native notifications are an OS integration and stay behind their port (root CLAUDE.md §3).
 * WHERE: Constructed by app/bootstrap; used only through `dyn Notifier` by the pipeline.
 */

mod tauri_toast;

pub use tauri_toast::TauriToastNotifier;
