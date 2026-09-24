/*!
 * SOURCE OF TRUTH KEYWORDS: Win32ShellLauncher, ShellExecuteW, open folder, ms-settings URI, privacy-microphone, COM apartment, shell thread
 * WHAT:  Win32ShellLauncher: SystemLauncher on the Windows shell. Folders open in File Explorer; settings pages open
 *        through their `ms-settings:` URI in the Settings app.
 * WHY:   ShellExecuteW with the "open" verb is how Windows itself opens a folder or a protocol URI, so the user's
 *        default file manager and the Settings app are used. The shell may hand the request to COM-based
 *        extensions, which need a single-threaded apartment, and command handlers run on the async runtime's
 *        worker threads whose COM state Echo does not own; each call therefore runs on its own short-lived thread
 *        that initialises and releases its apartment. The call returns once the shell has taken the request
 *        (milliseconds), so the caller waits on the thread instead of queueing work it could not report on.
 *        Success is a return value above 32 (Win32 contract); anything else is returned as `Internal` with the
 *        shell code as log detail. The page URIs live here only: they are Windows details (root CLAUDE.md §3).
 * WHERE: Built by app/bootstrap into CommandCtx; called by ipc/commands/system.rs through `dyn SystemLauncher`.
 */

use std::{path::Path, thread};

use windows::{
    Win32::{
        System::Com::{
            COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
        },
        UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    },
    core::{HSTRING, PCWSTR, w},
};

use crate::{
    ports::SystemLauncher,
    types::{AppError, PortError, PortResult, SettingsPage},
};

/// ShellExecuteW returns a value greater than this on success; smaller values are error codes.
const SHELL_SUCCESS_THRESHOLD: usize = 32;

/// Opens folders and settings pages through the Windows shell.
#[derive(Debug, Default)]
pub struct Win32ShellLauncher;

impl Win32ShellLauncher {
    pub fn new() -> Self {
        Self
    }
}

impl SystemLauncher for Win32ShellLauncher {
    fn open_folder(&self, dir: &Path) -> PortResult<()> {
        shell_open(HSTRING::from(dir))
    }

    fn open_settings_page(&self, page: SettingsPage) -> PortResult<()> {
        shell_open(HSTRING::from(settings_uri(page)))
    }
}

/// The `ms-settings:` URI of a settings page.
fn settings_uri(page: SettingsPage) -> &'static str {
    match page {
        SettingsPage::MicrophonePrivacy => "ms-settings:privacy-microphone",
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: shell_open, dedicated shell thread, STA per call, join shell thread
 * WHAT:  Runs `open_in_apartment(target)` on a new named thread and returns its result.
 * WHY:   Keeps COM initialisation off the async runtime's threads (see the file header); a thread that cannot be
 *        spawned or that panics becomes `Internal` with detail instead of a panic in the command.
 * WHERE: Both SystemLauncher methods above.
 */
fn shell_open(target: HSTRING) -> PortResult<()> {
    let worker = thread::Builder::new()
        .name("echo-shell-open".to_owned())
        .spawn(move || open_in_apartment(&target))
        .map_err(|error| launch_failure(format!("shell thread could not start: {error}")))?;
    worker
        .join()
        .map_err(|_| launch_failure("shell thread panicked"))?
}

fn open_in_apartment(target: &HSTRING) -> PortResult<()> {
    // SAFETY: called once on a thread this module owns, with no reserved pointer; a failure only means the shell
    // runs without an apartment, and the apartment is released below exactly when this call succeeded.
    let apartment =
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
    // SAFETY: every string argument is a valid null-terminated wide string (a literal or `target`, which outlives
    // the call); null parameters and directory are allowed, and no owner window is given.
    let instance = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            target,
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if apartment.is_ok() {
        // SAFETY: balances the successful CoInitializeEx above on the same thread.
        unsafe { CoUninitialize() };
    }
    let code = instance.0.addr();
    if code > SHELL_SUCCESS_THRESHOLD {
        Ok(())
    } else {
        Err(launch_failure(format!(
            "ShellExecuteW failed with code {code}"
        )))
    }
}

fn launch_failure(detail: impl Into<String>) -> PortError {
    PortError::new(AppError::Internal).with_detail(detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn microphone_privacy_opens_the_documented_settings_uri() {
        assert_eq!(
            settings_uri(SettingsPage::MicrophonePrivacy),
            "ms-settings:privacy-microphone"
        );
    }
}
