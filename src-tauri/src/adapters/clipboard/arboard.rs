/*!
 * SOURCE OF TRUTH KEYWORDS: ArboardClipboard, arboard, OpenClipboard retry, clipboard occupied, CanIncludeInClipboardHistory, CanUploadToCloudClipboard, ExcludeClipboardContentFromMonitorProcessing
 * WHAT:  ArboardClipboard: Clipboard on the `arboard` crate. Text reads, writes and clears; a write with
 *        `ClipboardHistory::Exclude` also places the three Windows exclusion formats.
 * WHY:   Another app holding the clipboard is normal (clipboard managers, RDP), so each call retries 10× with a
 *        15 ms pause before it reports `PermissionDenied { clipboard }` (05 W4); only "occupied" is retried, any
 *        other failure is reported at once. arboard already retries the open itself (5 × 5 ms), so the worst case
 *        is about 0.4 s. Excluded writes set CanIncludeInClipboardHistory=0, CanUploadToCloudClipboard=0 and
 *        ExcludeClipboardContentFromMonitorProcessing while the clipboard is still open (05 W5): the text stays
 *        pasteable but Win+V history, cloud sync and clipboard monitors skip it, because Echo's History is the
 *        archive. An arboard handle is created per call: on Windows it holds nothing between calls, and the
 *        clipboard may only be opened on the thread that uses it.
 *        On Linux the clipboard's contents live in the process that set them (X11 selections, Wayland data
 *        offers): arboard serves them from a context shared by every live handle and drops them with the last
 *        one, so the adapter keeps one handle open for its lifetime and a paste after the write still finds the
 *        text. An excluded write there carries `x-kde-passwordManagerHint`, the hint clipboard managers honour.
 * WHERE: Built by app/bootstrap into pipeline/delivery.rs's Delivery; called through `dyn Clipboard`.
 */

use std::{thread, time::Duration};

use arboard::Error as ClipboardError;
#[cfg(target_os = "linux")]
use arboard::SetExtLinux;
#[cfg(windows)]
use arboard::SetExtWindows;

use crate::{
    ports::Clipboard,
    types::{AppError, ClipboardHistory, Permission, PortError, PortResult},
};

/// Tries per call before the clipboard counts as busy (05 W4).
const ATTEMPTS: u32 = 10;

/// Pause between tries (05 W4).
const BACKOFF: Duration = Duration::from_millis(15);

/// The system clipboard through arboard.
#[derive(Default)]
pub struct ArboardClipboard {
    /// Linux: the handle that keeps the clipboard's serving context (and so the last write) alive.
    #[cfg(target_os = "linux")]
    keep_alive: parking_lot::Mutex<Option<arboard::Clipboard>>,
}

impl std::fmt::Debug for ArboardClipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArboardClipboard").finish_non_exhaustive()
    }
}

impl ArboardClipboard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens the long-lived handle once; a failure is retried on the next write.
    #[cfg(target_os = "linux")]
    fn keep_serving(&self) {
        let mut held = self.keep_alive.lock();
        if held.is_none() {
            *held = arboard::Clipboard::new().ok();
        }
    }
}

impl Clipboard for ArboardClipboard {
    fn read_text(&self) -> PortResult<Option<String>> {
        match with_retry("read", |clipboard| clipboard.get_text()) {
            Ok(text) => Ok(Some(text)),
            Err(Failure::Clipboard(ClipboardError::ContentNotAvailable)) => Ok(None),
            Err(failure) => Err(failure.into_port_error("read")),
        }
    }

    fn write_text(&self, text: &str, history: ClipboardHistory) -> PortResult<()> {
        #[cfg(target_os = "linux")]
        self.keep_serving();
        with_retry("write", |clipboard| {
            let set = clipboard.set();
            match history {
                #[cfg(windows)]
                ClipboardHistory::Exclude => set
                    .exclude_from_history()
                    .exclude_from_cloud()
                    .exclude_from_monitoring()
                    .text(text),
                #[cfg(target_os = "linux")]
                ClipboardHistory::Exclude => set.exclude_from_history().text(text),
                ClipboardHistory::Include => set.text(text),
            }
        })
        .map_err(|failure| failure.into_port_error("write"))
    }

    fn clear(&self) -> PortResult<()> {
        with_retry("clear", arboard::Clipboard::clear)
            .map_err(|failure| failure.into_port_error("clear"))
    }
}

/// Why a clipboard call failed after its retries.
#[derive(Debug)]
enum Failure {
    /// Another app kept the clipboard open for every try.
    Busy,
    Clipboard(ClipboardError),
}

impl Failure {
    fn into_port_error(self, operation: &str) -> PortError {
        match self {
            Self::Busy => PortError::new(AppError::PermissionDenied {
                permission: Permission::Clipboard,
            })
            .with_detail(format!(
                "clipboard {operation}: still held by another app after {ATTEMPTS} tries"
            )),
            Self::Clipboard(error) => PortError::new(AppError::Internal)
                .with_detail(format!("clipboard {operation} failed: {error}")),
        }
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: with_retry, clipboard busy retry, ClipboardOccupied backoff
 * WHAT:  Runs `operation` on a fresh arboard handle up to ATTEMPTS times, pausing BACKOFF after each "occupied".
 * WHY:   One retry policy for every clipboard call (05 W4); a non-occupied failure would fail the same way again.
 * WHERE: Every Clipboard method above.
 */
fn with_retry<T>(
    operation: &str,
    mut call: impl FnMut(&mut arboard::Clipboard) -> Result<T, ClipboardError>,
) -> Result<T, Failure> {
    for attempt in 1..=ATTEMPTS {
        let result = arboard::Clipboard::new().and_then(|mut clipboard| call(&mut clipboard));
        match result {
            Err(ClipboardError::ClipboardOccupied) => {
                tracing::debug!(operation, attempt, "clipboard held by another app");
                if attempt < ATTEMPTS {
                    thread::sleep(BACKOFF);
                }
            }
            Err(error) => return Err(Failure::Clipboard(error)),
            Ok(value) => return Ok(value),
        }
    }
    Err(Failure::Busy)
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use windows::{
        Win32::System::DataExchange::{IsClipboardFormatAvailable, RegisterClipboardFormatW},
        core::{HSTRING, PCWSTR},
    };

    use super::*;

    /// True when the clipboard currently offers the registered format `name`.
    #[cfg(windows)]
    fn offers(name: &str) -> bool {
        let name = HSTRING::from(name);
        // SAFETY: `name` is a valid null-terminated wide string that outlives the call.
        let format = unsafe { RegisterClipboardFormatW(PCWSTR(name.as_ptr())) };
        assert_ne!(format, 0, "format could not be registered");
        // SAFETY: plain value argument; no clipboard ownership is needed to ask.
        unsafe { IsClipboardFormatAvailable(format) }.is_ok()
    }

    #[test]
    fn busy_and_other_failures_map_to_their_errors() {
        assert_eq!(
            Failure::Busy.into_port_error("write").into_app_error(),
            AppError::PermissionDenied {
                permission: Permission::Clipboard
            }
        );
        let other = Failure::Clipboard(ClipboardError::ConversionFailure).into_port_error("read");
        assert_eq!(other.error(), &AppError::Internal);
        assert!(other.detail().is_some_and(|detail| detail.contains("read")));
    }

    /// Uses the real clipboard: the developer's text is put back afterwards (excluded, so it is not re-added to
    /// Win+V history). This is the one place the exclusion formats can be proven (05 W5).
    #[cfg(windows)]
    #[test]
    fn excluded_writes_carry_the_windows_exclusion_formats() {
        let clipboard = ArboardClipboard::new();
        let before = clipboard.read_text().unwrap();

        clipboard
            .write_text("Echo clipboard test", ClipboardHistory::Exclude)
            .unwrap();
        let written = clipboard.read_text().unwrap();
        let formats = [
            offers("CanIncludeInClipboardHistory"),
            offers("CanUploadToCloudClipboard"),
            offers("ExcludeClipboardContentFromMonitorProcessing"),
        ];
        clipboard.clear().unwrap();
        let cleared = clipboard.read_text().unwrap();

        match &before {
            Some(text) => clipboard
                .write_text(text, ClipboardHistory::Exclude)
                .unwrap(),
            None => clipboard.clear().unwrap(),
        }
        assert_eq!(written.as_deref(), Some("Echo clipboard test"));
        assert_eq!(formats, [true; 3]);
        assert_eq!(cleared, None);
        assert_eq!(clipboard.read_text().unwrap(), before);
    }
}
