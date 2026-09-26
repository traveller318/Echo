/*!
 * SOURCE OF TRUTH KEYWORDS: ForegroundTracker, EVENT_SYSTEM_FOREGROUND, SetWinEventHook, WINEVENT_SKIPOWNPROCESS, last external window, shell window classes, tray click target
 * WHAT:  ForegroundTracker: remembers the last top-level window that came to the front and belongs neither to
 *        Echo nor to the Windows shell (taskbar, tray overflow, Start, search, task view, the desktop), from an
 *        out-of-context WinEvent hook on its own message thread. `last` answers it while that window still exists.
 * WHY:   A click on the tray icon brings the taskbar to the front, then the tray menu (Echo's own window), so at the
 *        moment a tray action runs the window in front is never where the user was typing; the app they left is.
 *        The hook skips Echo's own process itself (WINEVENT_SKIPOWNPROCESS) and the shell is recognised by its window
 *        classes, so the answer is the user's app. The callback stores one handle and returns: it runs on the
 *        tracker's thread, never blocking another app. The tracker starts from the window in front when Echo starts,
 *        so a tray action right after launch has an answer too.
 * WHERE: Owned by Win32ForegroundApp (win32.rs), started in its constructor; read by `last_external`.
 */

use std::{cell::RefCell, sync::Arc};

use parking_lot::Mutex;
use windows::Win32::{
    Foundation::HWND,
    UI::{
        Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent},
        WindowsAndMessaging::{
            EVENT_SYSTEM_FOREGROUND, GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId,
            IsWindow, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
        },
    },
};

use crate::{
    adapters::win32::{MessageThread, hwnd, window_handle},
    types::{AppError, PortError, PortResult, WindowHandle},
};

const THREAD_NAME: &str = "echo-foreground";

/// Window classes of the Windows shell: never where dictated text should go.
const SHELL_CLASSES: &[&str] = &[
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "NotifyIconOverflowWindow",
    "TopLevelWindowForOverflowXamlIsland",
    "XamlExplorerHostIslandWindow",
    "Windows.UI.Core.CoreWindow",
    "MultitaskingViewFrame",
    "TaskSwitcherWnd",
    "ForegroundStaging",
    "Progman",
    "WorkerW",
];

/// Longest window class name Windows allows, plus the terminator.
const CLASS_CAPACITY: usize = 257;

type Slot = Arc<Mutex<Option<WindowHandle>>>;

thread_local! {
    static SLOT: RefCell<Option<Slot>> = const { RefCell::new(None) };
}

/// The last external window, kept current by a foreground hook.
pub struct ForegroundTracker {
    last: Slot,
    _thread: MessageThread,
}

impl ForegroundTracker {
    /// Starts the hook, seeded with the window in front now.
    pub fn start() -> PortResult<Self> {
        let last: Slot = Arc::default();
        // SAFETY: no arguments; a null handle when nothing is in front.
        let seed = unsafe { GetForegroundWindow() };
        if is_external(seed) {
            *last.lock() = Some(window_handle(seed));
        }
        let slot = Arc::clone(&last);
        let thread = MessageThread::spawn(THREAD_NAME, move || Hook::install(slot))?;
        Ok(Self {
            last,
            _thread: thread,
        })
    }

    /// The last external window, if it still exists.
    pub fn last(&self) -> Option<HWND> {
        let window = hwnd((*self.last.lock())?);
        // SAFETY: IsWindow accepts any handle value and only reports whether it names a live window.
        unsafe { IsWindow(Some(window)) }
            .as_bool()
            .then_some(window)
    }
}

/// The installed hook, removed on its thread when the loop ends.
struct Hook(HWINEVENTHOOK);

impl Hook {
    fn install(slot: Slot) -> PortResult<Self> {
        SLOT.with(|cell| *cell.borrow_mut() = Some(slot));
        // SAFETY: an out-of-context hook delivers to this thread's message loop; the callback is a plain fn.
        let hook = unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(on_foreground),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            )
        };
        if hook.is_invalid() {
            return Err(PortError::new(AppError::Internal)
                .with_detail("SetWinEventHook(EVENT_SYSTEM_FOREGROUND) failed"));
        }
        Ok(Self(hook))
    }
}

impl Drop for Hook {
    fn drop(&mut self) {
        // SAFETY: the hook was installed by `install` on this thread and is removed exactly once.
        let _ = unsafe { UnhookWinEvent(self.0) };
        SLOT.with(|cell| *cell.borrow_mut() = None);
    }
}

/// The hook callback: remembers `window` when it is an external top-level window.
unsafe extern "system" fn on_foreground(
    _hook: HWINEVENTHOOK,
    _event: u32,
    window: HWND,
    object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    // OBJID_WINDOW (0): the event concerns the window itself.
    if object != 0 || !is_external(window) {
        return;
    }
    SLOT.with(|cell| {
        if let Ok(slot) = cell.try_borrow()
            && let Some(slot) = slot.as_ref()
        {
            *slot.lock() = Some(window_handle(window));
        }
    });
}

/// A live window of another process that is not part of the shell.
fn is_external(window: HWND) -> bool {
    if window.is_invalid() {
        return false;
    }
    let mut process_id = 0u32;
    // SAFETY: `process_id` outlives the call.
    let thread = unsafe { GetWindowThreadProcessId(window, Some(&raw mut process_id)) };
    if thread == 0 || process_id == std::process::id() {
        return false;
    }
    let mut class = [0u16; CLASS_CAPACITY];
    // SAFETY: `class` is a writable buffer; the call writes at most its length.
    let length = unsafe { GetClassNameW(window, &mut class) };
    let name = String::from_utf16_lossy(
        class
            .get(..usize::try_from(length).unwrap_or(0))
            .unwrap_or(&[]),
    );
    !is_shell_class(&name)
}

/// The class belongs to the Windows shell.
fn is_shell_class(name: &str) -> bool {
    SHELL_CLASSES.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_windows_are_never_targets() {
        for class in [
            "Shell_TrayWnd",
            "NotifyIconOverflowWindow",
            "Progman",
            "Windows.UI.Core.CoreWindow",
        ] {
            assert!(is_shell_class(class), "{class}");
        }
        for class in [
            "Notepad",
            "Chrome_WidgetWin_1",
            "CASCADIA_HOSTING_WINDOW_CLASS",
        ] {
            assert!(!is_shell_class(class), "{class}");
        }
        assert!(!is_external(HWND::default()), "no window is no target");
    }

    #[test]
    fn the_tracker_starts_and_stops() {
        let tracker = ForegroundTracker::start().unwrap();
        // Whatever is in front on the test machine, a remembered window is always a live one.
        if let Some(window) = tracker.last() {
            // SAFETY: IsWindow only inspects the handle.
            assert!(unsafe { IsWindow(Some(window)) }.as_bool());
        }
        drop(tracker);
    }
}
