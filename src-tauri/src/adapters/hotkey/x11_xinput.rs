/*!
 * SOURCE OF TRUTH KEYWORDS: X11XInput, XInput2 raw key events, XI_RawKeyPress, XIAllMasterDevices, XQueryKeymap, X11 global hotkeys, listen-only key capture
 * WHAT:  X11XInput: the KeyCapture for an X11 session. It selects XInput2 raw key press/release events on the root
 *        window for all master devices and runs a thread that translates each event's keycode to a virtual key
 *        (chords/keymap.rs) and feeds the Dispatcher; while keys are held it also asks the server for the real
 *        keyboard state (XQueryKeymap) to let go of a key whose release was missed.
 * WHY:   Raw events are the X11 way to watch every key without grabbing any: nothing is intercepted or swallowed, no
 *        other app loses a key, and no permission is needed, which is the same standing the Windows low-level hook
 *        has. They also report modifier-only chords and releases, so hold-to-talk on `Ctrl+Alt` works unchanged. The
 *        trade-off is that a bound key still reaches the focused app (Ctrl+Alt+V is typed as well as fired). Only
 *        keycodes are read, never characters, and nothing is logged. The thread waits on the X connection and an
 *        eventfd with poll(2), so it sleeps until a key or a stop arrives, and wakes every 250 ms only while a held key
 *        might have lost its release. X event times are the server's wrapping milliseconds, which is the clock the
 *        tracker works in; the time `reconcile` runs at is the last event's time plus the monotonic time since it.
 *        A session where XInput 2 is missing, or the server connection ends, fails `start` / ends the thread with
 *        a log line, and Echo runs without hotkeys. Under Wayland an X11 client sees no keys typed into native
 *        Wayland windows, so this capture is chosen only for an X11 session (app/platform.rs).
 * WHERE: Built by app/platform.rs on Linux inside ChordHotkeys for an X11 session.
 */

use std::{
    os::fd::AsRawFd,
    sync::Arc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use x11rb::{
    connection::Connection,
    protocol::{
        Event,
        xinput::{self, ConnectionExt as _, XIEventMask},
        xproto::ConnectionExt as _,
    },
    rust_connection::RustConnection,
};

use super::chords::{Dispatcher, KeyCapture, Shared};
use crate::{
    adapters::linux::{
        Wake, WakeFd,
        keymap::{vk_from_x11_keycode, x11_keycode_from_vk},
    },
    types::{AppError, HotkeyCaps, PortError, PortResult},
};

/// XInput2's "all master devices" selector.
const ALL_MASTER_DEVICES: xinput::DeviceId = 1;

/// How often a held key's real state is checked.
const RECONCILE_EVERY: Duration = Duration::from_millis(250);

#[derive(Debug, Default)]
pub struct X11XInput;

impl KeyCapture for X11XInput {
    type Running = Running;

    fn caps(&self) -> HotkeyCaps {
        HotkeyCaps {
            supports_release: true,
            supports_modifier_only: true,
        }
    }

    fn start(&self, shared: Arc<Shared>) -> PortResult<Running> {
        Running::start(shared)
    }
}

/// A running capture; dropping it stops and joins the thread.
pub struct Running {
    wake: Arc<WakeFd>,
    thread: Option<JoinHandle<()>>,
}

impl Running {
    fn start(shared: Arc<Shared>) -> PortResult<Self> {
        let (conn, screen) = RustConnection::connect(None)
            .map_err(|error| failure(&format!("the X server cannot be reached: {error}")))?;
        let version = conn
            .xinput_xi_query_version(2, 0)
            .map_err(|error| failure(&error.to_string()))?
            .reply()
            .map_err(|_| failure("the X server has no XInput extension"))?;
        if version.major_version < 2 {
            return Err(failure("the X server's XInput is older than 2.0"));
        }
        let root = conn.setup().roots[screen].root;
        conn.xinput_xi_select_events(
            root,
            &[xinput::EventMask {
                deviceid: ALL_MASTER_DEVICES,
                mask: vec![XIEventMask::RAW_KEY_PRESS | XIEventMask::RAW_KEY_RELEASE],
            }],
        )
        .map_err(|error| failure(&error.to_string()))?
        .check()
        .map_err(|error| failure(&format!("raw key events cannot be selected: {error}")))?;
        let wake = Arc::new(WakeFd::new().map_err(|error| failure(&error.to_string()))?);
        let thread_wake = Arc::clone(&wake);
        let thread = thread::Builder::new()
            .name("echo-x11-keys".to_owned())
            .spawn(move || run(&conn, &thread_wake, Dispatcher::new(shared)))
            .map_err(|error| failure(&error.to_string()))?;
        Ok(Self {
            wake,
            thread: Some(thread),
        })
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.wake.signal();
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::error!("the X11 key thread panicked");
        }
    }
}

fn failure(detail: &str) -> PortError {
    PortError::new(AppError::Internal).with_detail(format!("X11 key capture unavailable: {detail}"))
}

/// The capture thread: sleeps in poll(2) until a key event arrives or the stop is signalled.
fn run(conn: &RustConnection, wake: &WakeFd, mut dispatcher: Dispatcher) {
    let x_fd = conn.stream().as_raw_fd();
    // The latest event time from the server and when it arrived, to date a reconcile.
    let mut clock: Option<(u32, Instant)> = None;
    loop {
        let timeout = if dispatcher.needs_reconcile() {
            i32::try_from(RECONCILE_EVERY.as_millis()).unwrap_or(250)
        } else {
            -1
        };
        match wake.wait(x_fd, timeout) {
            Wake::Stop => return,
            Wake::Failed => {
                tracing::warn!("the X11 key thread's poll failed; hotkeys stop");
                return;
            }
            Wake::Data | Wake::Timeout => {}
        }
        loop {
            match conn.poll_for_event() {
                Ok(Some(event)) => handle(event, &mut dispatcher, &mut clock),
                Ok(None) => break,
                Err(_) => {
                    tracing::warn!("the X server connection ended; hotkeys stop");
                    return;
                }
            }
        }
        if dispatcher.needs_reconcile()
            && let Some((time, at)) = clock
        {
            let now =
                time.wrapping_add(u32::try_from(at.elapsed().as_millis()).unwrap_or(u32::MAX));
            if let Some(held) = keyboard_state(conn) {
                dispatcher.reconcile(now, |vk| {
                    x11_keycode_from_vk(vk).is_some_and(|code| held.is_down(code))
                });
            }
        }
    }
}

fn handle(event: Event, dispatcher: &mut Dispatcher, clock: &mut Option<(u32, Instant)>) {
    let (raw, down) = match event {
        Event::XinputRawKeyPress(raw) => (raw, true),
        Event::XinputRawKeyRelease(raw) => (raw, false),
        _ => return,
    };
    *clock = Some((raw.time, Instant::now()));
    // Key-repeat events are downs the tracker already treats as repeats; the server's repeat flag adds nothing.
    if let Some(vk) = u16::try_from(raw.detail).ok().and_then(vk_from_x11_keycode) {
        dispatcher.key(vk, down, raw.time);
    }
}

/// The server's bit map of the keys down now.
struct KeyboardState([u8; 32]);

impl KeyboardState {
    fn is_down(&self, keycode: u16) -> bool {
        self.0
            .get(usize::from(keycode / 8))
            .is_some_and(|byte| byte & (1 << (keycode % 8)) != 0)
    }
}

fn keyboard_state(conn: &RustConnection) -> Option<KeyboardState> {
    let reply = conn.query_keymap().ok()?.reply().ok()?;
    Some(KeyboardState(reply.keys))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_bitmap_answers_by_keycode() {
        let mut keys = [0_u8; 32];
        keys[55 / 8] |= 1 << (55 % 8);
        let state = KeyboardState(keys);
        assert!(state.is_down(55));
        assert!(!state.is_down(54));
        assert!(!state.is_down(300));
    }

    /// Needs a running X server (DISPLAY): skipped in effect without one.
    #[test]
    fn starting_and_stopping_the_capture_is_clean() {
        if std::env::var_os("DISPLAY").is_none() {
            return;
        }
        let shared = Arc::new(Shared::default());
        match X11XInput.start(shared) {
            Ok(running) => drop(running),
            Err(error) => assert!(
                error
                    .detail()
                    .is_some_and(|d| d.contains("X11 key capture")),
                "{error:?}"
            ),
        }
    }
}
