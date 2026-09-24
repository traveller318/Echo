/*!
 * SOURCE OF TRUTH KEYWORDS: Notifier, toast, native notification, Windows toast, notify user
 * WHAT:  Notifier shows a native notification.
 * WHY:   Toasts are the one feedback channel when the pill is gone or the main window is hidden (copy-only
 *        fallback, device lost, recovered takes). A failed toast must never fail a take, so callers log the error
 *        and continue; the port still reports it so the log explains a missing toast (05 W18).
 * WHERE: Implemented by adapters/notifier/tauri_toast.rs (TauriToastNotifier) and ports/fakes; used by the
 *        pipeline (delivery, recovery, power).
 */

use crate::types::{PortResult, Toast};

/// Native notifications.
pub trait Notifier: Send + Sync {
    fn toast(&self, toast: &Toast) -> PortResult<()>;
}
