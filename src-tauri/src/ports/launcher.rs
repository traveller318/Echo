/*!
 * SOURCE OF TRUTH KEYWORDS: SystemLauncher, open folder, open settings page, shell open, logs folder, microphone privacy settings
 * WHAT:  SystemLauncher hands a folder or an operating-system settings page to the OS shell, which shows it in its
 *        own window (File Explorer, the Settings app).
 * WHY:   Opening things "the Windows way" is a shell call (ShellExecute), and Windows API calls stay behind a port
 *        (root CLAUDE.md §3), so commands ask for "the logs folder" or "the microphone privacy page" without
 *        knowing a URI scheme. Blocking: the shell returns as soon as it has handed the request over. A failure is
 *        reported with the shell's code as log detail; nothing is retried, because the user can simply click again.
 * WHERE: Implemented by adapters/launcher (Win32ShellLauncher) and ports/fakes; called by ipc/commands/system.rs
 *        (`app_open_logs_dir`, `app_open_mic_privacy_settings`), later by onboarding and About.
 */

use std::path::Path;

use crate::types::{PortResult, SettingsPage};

/// Opens folders and settings pages in the operating system's own UI.
pub trait SystemLauncher: Send + Sync {
    /// Shows `dir` in the system file manager.
    fn open_folder(&self, dir: &Path) -> PortResult<()>;

    /// Opens an operating-system settings page.
    fn open_settings_page(&self, page: SettingsPage) -> PortResult<()>;
}
