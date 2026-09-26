/*!
 * SOURCE OF TRUTH KEYWORDS: Win32PowerEvents, WM_POWERBROADCAST, PBT_APMSUSPEND, PBT_APMRESUMEAUTOMATIC, GUID_CONSOLE_DISPLAY_STATE, WM_WTSSESSION_CHANGE, TaskbarCreated, hidden top-level window, RegisterSuspendResumeNotification
 * WHAT:  Win32PowerEvents: PowerEvents through one hidden top-level window on its own message thread. It turns
 *        suspend into Suspend, the automatic resume into Resume, the console display going from off to on into
 *        DisplayOn, a session unlock or reconnect into SessionResumed and explorer's TaskbarCreated broadcast into
 *        ShellRestarted, and sends each to the current sink.
 * WHY:   Every one of these is delivered to a window, and TaskbarCreated only to top-level windows (a message-only
 *        window never hears a broadcast), so one hidden, never-shown top-level window collects them all; its own
 *        thread keeps delivery independent of Tauri's event loop (05 W8). Resume is taken from
 *        PBT_APMRESUMEAUTOMATIC only, which Windows always sends, so a wake is reported once. Modern Standby machines
 *        may wake with nothing but the display turning on, so the console display state is watched too; only
 *        off → on counts (a dimmed screen brightening is no wake). The callback does nothing but a non-blocking
 *        emit and returns, because Windows gives sleeping applications about two seconds in total.
 * WHERE: Built by app/bootstrap; `listen` is called once with the session actor's power sink.
 */

use std::{cell::RefCell, sync::Arc};

use parking_lot::Mutex;
use windows::{
    Win32::{
        Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM},
        System::{
            LibraryLoader::GetModuleHandleW,
            Power::{
                HPOWERNOTIFY, POWERBROADCAST_SETTING, RegisterPowerSettingNotification,
                RegisterSuspendResumeNotification, UnregisterPowerSettingNotification,
                UnregisterSuspendResumeNotification,
            },
            RemoteDesktop::{
                NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification,
                WTSUnRegisterSessionNotification,
            },
            SystemServices::GUID_CONSOLE_DISPLAY_STATE,
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DEVICE_NOTIFY_WINDOW_HANDLE, DefWindowProcW, DestroyWindow,
            PBT_APMRESUMEAUTOMATIC, PBT_APMSUSPEND, PBT_POWERSETTINGCHANGE, RegisterClassExW,
            RegisterWindowMessageW, WM_POWERBROADCAST, WM_WTSSESSION_CHANGE, WNDCLASSEXW,
            WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_OVERLAPPED, WTS_CONSOLE_CONNECT,
            WTS_REMOTE_CONNECT, WTS_SESSION_UNLOCK,
        },
    },
    core::{PCWSTR, w},
};

use crate::{
    adapters::win32::MessageThread,
    ports::{EventSink, PowerEvents},
    types::{AppError, PortError, PortResult, PowerEvent},
};

const THREAD_NAME: &str = "echo-system-events";
const WINDOW_CLASS: PCWSTR = w!("EchoSystemEvents");

/// GUID_CONSOLE_DISPLAY_STATE values.
const DISPLAY_OFF: u32 = 0;
const DISPLAY_ON: u32 = 1;

type SinkSlot = Arc<Mutex<Option<Arc<dyn EventSink<PowerEvent>>>>>;

/// Sleep, wake and session changes through a hidden window.
pub struct Win32PowerEvents {
    sink: SinkSlot,
    thread: Mutex<Option<MessageThread>>,
}

impl Win32PowerEvents {
    pub fn new() -> Self {
        Self {
            sink: Arc::default(),
            thread: Mutex::new(None),
        }
    }

    /// The hidden window's handle while it runs (tests post messages to it).
    #[cfg(test)]
    fn window(&self) -> Option<HWND> {
        WINDOWS
            .lock()
            .last()
            .map(|raw| crate::adapters::win32::hwnd(*raw))
    }
}

impl Default for Win32PowerEvents {
    fn default() -> Self {
        Self::new()
    }
}

impl PowerEvents for Win32PowerEvents {
    fn listen(&self, sink: Arc<dyn EventSink<PowerEvent>>) -> PortResult<()> {
        *self.sink.lock() = Some(sink);
        let mut thread = self.thread.lock();
        if thread.is_none() {
            let slot = Arc::clone(&self.sink);
            *thread = Some(MessageThread::spawn(THREAD_NAME, move || {
                EventWindow::create(slot)
            })?);
        }
        Ok(())
    }
}

/// Handles of the running windows, newest last (tests only look them up).
#[cfg(test)]
static WINDOWS: Mutex<Vec<crate::types::WindowHandle>> = parking_lot::const_mutex(Vec::new());

/// What the window procedure works with, owned by the message thread.
struct WindowState {
    sink: SinkSlot,
    /// The id Windows gave the "TaskbarCreated" broadcast in this session.
    taskbar_created: u32,
    /// The last console display state reported (None until the first report).
    display: Option<u32>,
}

thread_local! {
    static STATE: RefCell<Option<WindowState>> = const { RefCell::new(None) };
}

/**
 * SOURCE OF TRUTH KEYWORDS: EventWindow, hidden session window, power notification registrations, destroy on thread
 * WHAT:  The hidden window and its registrations (suspend/resume, console display state, session changes); dropping
 *        it on the message thread unregisters everything and destroys the window.
 * WHY:   A window can only be destroyed by the thread that created it, which MessageThread guarantees by dropping
 *        its guard there. A registration that fails is logged and skipped: the other notifications still work.
 * WHERE: Created by Win32PowerEvents::listen through MessageThread::spawn.
 */
struct EventWindow {
    window: HWND,
    suspend_resume: Option<HPOWERNOTIFY>,
    display: Option<HPOWERNOTIFY>,
    session: bool,
}

impl EventWindow {
    fn create(sink: SinkSlot) -> PortResult<Self> {
        // SAFETY: the string is a static wide literal; the id is valid for the whole session.
        let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
        STATE.with(|state| {
            *state.borrow_mut() = Some(WindowState {
                sink,
                taskbar_created,
                display: None,
            });
        });
        // SAFETY: None asks for the module that created this process.
        let instance = unsafe { GetModuleHandleW(None) }
            .map_err(|error| failure(format!("GetModuleHandleW failed: {error}")))?;
        let class = WNDCLASSEXW {
            cbSize: u32::try_from(size_of::<WNDCLASSEXW>()).unwrap_or(u32::MAX),
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: WINDOW_CLASS,
            ..WNDCLASSEXW::default()
        };
        // SAFETY: `class` is fully initialised; a second registration in the same process fails harmlessly
        // (ERROR_CLASS_ALREADY_EXISTS) and the existing class, with the same procedure, is used.
        let _ = unsafe { RegisterClassExW(&raw const class) };
        // SAFETY: a never-shown top-level window of the class above; no parent, so it hears broadcasts.
        let window = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                WINDOW_CLASS,
                w!("Echo system events"),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance.into()),
                None,
            )
        }
        .map_err(|error| failure(format!("CreateWindowExW failed: {error}")))?;
        let recipient = HANDLE(window.0);
        // SAFETY: `window` is a live window owned by this thread; each handle is unregistered in Drop.
        let suspend_resume =
            unsafe { RegisterSuspendResumeNotification(recipient, DEVICE_NOTIFY_WINDOW_HANDLE) }
                .inspect_err(|error| tracing::warn!(%error, "suspend notifications unavailable"))
                .ok();
        let setting = GUID_CONSOLE_DISPLAY_STATE;
        // SAFETY: as above; `setting` outlives the call, which copies the GUID.
        let display = unsafe {
            RegisterPowerSettingNotification(
                recipient,
                &raw const setting,
                DEVICE_NOTIFY_WINDOW_HANDLE,
            )
        }
        .inspect_err(|error| tracing::warn!(%error, "display state notifications unavailable"))
        .ok();
        // SAFETY: as above.
        let session = unsafe { WTSRegisterSessionNotification(window, NOTIFY_FOR_THIS_SESSION) }
            .inspect_err(|error| tracing::warn!(%error, "session change notifications unavailable"))
            .is_ok();
        #[cfg(test)]
        WINDOWS
            .lock()
            .push(crate::adapters::win32::window_handle(window));
        Ok(Self {
            window,
            suspend_resume,
            display,
            session,
        })
    }
}

impl Drop for EventWindow {
    fn drop(&mut self) {
        // SAFETY: every handle below was registered by `create` on this thread and is released exactly once.
        unsafe {
            if self.session {
                let _ = WTSUnRegisterSessionNotification(self.window);
            }
            if let Some(handle) = self.display.take() {
                let _ = UnregisterPowerSettingNotification(handle);
            }
            if let Some(handle) = self.suspend_resume.take() {
                let _ = UnregisterSuspendResumeNotification(handle);
            }
            let _ = DestroyWindow(self.window);
        }
        #[cfg(test)]
        WINDOWS
            .lock()
            .retain(|raw| *raw != crate::adapters::win32::window_handle(self.window));
        STATE.with(|state| *state.borrow_mut() = None);
    }
}

/// The window procedure: maps each notification to a PowerEvent and emits it; everything else is default.
unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let event = STATE.with(|state| {
        let mut state = state.try_borrow_mut().ok()?;
        let state = state.as_mut()?;
        let event = match message {
            WM_POWERBROADCAST => power_broadcast(state, wparam, lparam),
            WM_WTSSESSION_CHANGE => session_change(wparam),
            _ if message == state.taskbar_created => Some(PowerEvent::ShellRestarted),
            _ => return None,
        };
        Some((event, Arc::clone(&state.sink)))
    });
    match event {
        Some((event, sink)) => {
            if let Some(event) = event {
                let current = sink.lock().clone();
                if let Some(current) = current {
                    current.emit(event);
                }
            }
            // WM_POWERBROADCAST wants TRUE; the other two ignore the result.
            LRESULT(1)
        }
        // SAFETY: the arguments are exactly those Windows passed in.
        None => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

/// What a WM_POWERBROADCAST means, updating the display state it tracks.
fn power_broadcast(state: &mut WindowState, wparam: WPARAM, lparam: LPARAM) -> Option<PowerEvent> {
    let kind = u32::try_from(wparam.0).ok()?;
    match kind {
        PBT_APMSUSPEND => Some(PowerEvent::Suspend),
        PBT_APMRESUMEAUTOMATIC => Some(PowerEvent::Resume),
        PBT_POWERSETTINGCHANGE => {
            // SAFETY: for PBT_POWERSETTINGCHANGE Windows passes a POWERBROADCAST_SETTING valid during the call.
            let now = unsafe { display_state(lparam) }?;
            let previous = state.display.replace(now);
            display_turned_on(previous, now).then_some(PowerEvent::DisplayOn)
        }
        _ => None,
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: display_state, POWERBROADCAST_SETTING, console display state value
 * WHAT:  The console display state carried by a power-setting notification; None for any other setting.
 * WHY:   The data follows the header as a byte array of `DataLength` bytes, so it is read unaligned.
 * WHERE: `power_broadcast`.
 */
unsafe fn display_state(lparam: LPARAM) -> Option<u32> {
    let setting =
        std::ptr::with_exposed_provenance::<POWERBROADCAST_SETTING>(lparam.0.cast_unsigned());
    if setting.is_null() {
        return None;
    }
    // SAFETY: the caller guarantees `setting` points at a live POWERBROADCAST_SETTING header.
    let (guid, length) = unsafe {
        (
            (&raw const (*setting).PowerSetting).read_unaligned(),
            (&raw const (*setting).DataLength).read_unaligned(),
        )
    };
    if guid != GUID_CONSOLE_DISPLAY_STATE || usize::try_from(length).ok()? < size_of::<u32>() {
        return None;
    }
    // SAFETY: DataLength says at least four bytes start at the Data field of the same allocation.
    Some(unsafe {
        setting
            .cast::<u8>()
            .add(std::mem::offset_of!(POWERBROADCAST_SETTING, Data))
            .cast::<u32>()
            .read_unaligned()
    })
}

/// Only off → on is a wake; a dimmed screen brightening, or the first report, is not.
fn display_turned_on(previous: Option<u32>, now: u32) -> bool {
    previous == Some(DISPLAY_OFF) && now == DISPLAY_ON
}

/// A session back at this console: unlocked, switched back to, or reconnected.
fn session_change(wparam: WPARAM) -> Option<PowerEvent> {
    let kind = u32::try_from(wparam.0).ok()?;
    matches!(
        kind,
        WTS_SESSION_UNLOCK | WTS_CONSOLE_CONNECT | WTS_REMOTE_CONNECT
    )
    .then_some(PowerEvent::SessionResumed)
}

fn failure(detail: String) -> PortError {
    PortError::new(AppError::Internal).with_detail(format!("{THREAD_NAME}: {detail}"))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use windows::Win32::UI::WindowsAndMessaging::SendMessageW;

    use super::*;
    use crate::ports::fakes::ChannelSink;

    #[test]
    fn only_a_display_going_from_off_to_on_is_a_wake() {
        assert!(display_turned_on(Some(DISPLAY_OFF), DISPLAY_ON));
        assert!(!display_turned_on(None, DISPLAY_ON));
        assert!(!display_turned_on(Some(2), DISPLAY_ON), "dimmed → on");
        assert!(!display_turned_on(Some(DISPLAY_ON), DISPLAY_OFF));
    }

    #[test]
    fn the_hidden_window_turns_notifications_into_events() {
        let power = Win32PowerEvents::new();
        let sink = Arc::new(ChannelSink::default());
        power.listen(Arc::clone(&sink) as _).unwrap();
        let window = power.window().unwrap();
        let send = |message: u32, wparam: usize, lparam: isize| {
            // SAFETY: a synchronous send to the live test window; lparam points at memory that outlives the call.
            unsafe { SendMessageW(window, message, Some(WPARAM(wparam)), Some(LPARAM(lparam))) }
        };
        let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };

        send(WM_POWERBROADCAST, PBT_APMSUSPEND as usize, 0);
        send(WM_POWERBROADCAST, PBT_APMRESUMEAUTOMATIC as usize, 0);
        send(WM_WTSSESSION_CHANGE, WTS_SESSION_UNLOCK as usize, 0);
        send(taskbar_created, 0, 0);
        for state in [DISPLAY_ON, DISPLAY_OFF, DISPLAY_ON] {
            let mut setting = POWERBROADCAST_SETTING {
                PowerSetting: GUID_CONSOLE_DISPLAY_STATE,
                DataLength: 4,
                Data: [0; 1],
            };
            // The value spans Data and the struct's trailing padding, as Windows lays it out.
            let mut buffer = [0u8; size_of::<POWERBROADCAST_SETTING>() + 4];
            // SAFETY: the buffer is large enough for the header plus four data bytes.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    (&raw mut setting).cast::<u8>(),
                    buffer.as_mut_ptr(),
                    size_of::<POWERBROADCAST_SETTING>(),
                );
                let data = buffer
                    .as_mut_ptr()
                    .add(std::mem::offset_of!(POWERBROADCAST_SETTING, Data));
                data.cast::<u32>().write_unaligned(state);
            }
            send(
                WM_POWERBROADCAST,
                PBT_POWERSETTINGCHANGE as usize,
                buffer.as_ptr().expose_provenance().cast_signed(),
            );
        }

        let mut received = Vec::new();
        while let Some(event) = sink.next() {
            received.push(event);
            if received.len() == 5 {
                break;
            }
        }
        assert_eq!(
            received,
            [
                PowerEvent::Suspend,
                PowerEvent::Resume,
                PowerEvent::SessionResumed,
                PowerEvent::ShellRestarted,
                PowerEvent::DisplayOn,
            ]
        );
        assert!(sink.nothing_within(Duration::from_millis(50)));
        drop(power);
    }
}
