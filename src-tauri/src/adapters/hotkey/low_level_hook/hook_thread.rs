/*!
 * SOURCE OF TRUTH KEYWORDS: HookThread, WH_KEYBOARD_LL, SetWindowsHookExW, hook_proc, LowLevelHooksTimeout, message loop, AltGr fake Ctrl, SYNTHETIC_INPUT_TAG skip, reconcile timer, mask tap
 * WHAT:  HookThread: a dedicated thread that installs the low-level keyboard hook, pumps its message loop, runs the
 *        ChordTracker for every physical key event and forwards the resulting HotkeyEvents to the adapter's sink.
 *        Dropping it posts WM_QUIT, unhooks and joins.
 * WHY:   A WH_KEYBOARD_LL hook is called on the thread that installed it, through that thread's message loop, so it
 *        needs a thread of its own that never blocks (Tauri's event loop may be busy rendering). Windows removes a
 *        hook whose callback exceeds `LowLevelHooksTimeout` (05 W9), so the callback only runs the pure tracker,
 *        posts to the session inbox (an unbounded send) and returns; the mask tap and the stuck-key check run later
 *        on the same thread as thread messages. Hook callbacks get no user pointer, so the thread's state lives in
 *        a thread-local, borrowed with `try_borrow_mut` so a re-entrant call could only pass the key through, never
 *        panic across the FFI boundary. Skipped entirely: events tagged SYNTHETIC_INPUT_TAG (Echo's own paste and
 *        mask taps) and the fake left Ctrl (scan code 0x21D) that AltGr layouts send ahead of right Alt, so typing
 *        an AltGr character never looks like Ctrl+Alt. The reconcile timer runs only while a key the tracker may
 *        need to release is held, so an idle Echo wakes nothing. The message queue is created before the thread
 *        reports ready, so WM_QUIT posted right after start is never lost.
 * WHERE: Started and stopped by LowLevelKeyboardHotkeys (mod.rs) while at least one binding exists.
 */

use std::{
    cell::RefCell,
    sync::{Arc, mpsc},
    thread::{self, JoinHandle},
};

use windows::Win32::{
    Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM},
    System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
    UI::{
        Input::KeyboardAndMouse::VIRTUAL_KEY,
        WindowsAndMessaging::{
            CallNextHookEx, GetMessageW, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, KillTimer, MSG,
            PM_NOREMOVE, PeekMessageW, PostThreadMessageW, SetTimer, SetWindowsHookExW,
            UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_APP, WM_KEYDOWN, WM_QUIT, WM_SYSKEYDOWN,
            WM_TIMER, WM_USER,
        },
    },
};

use super::{
    Shared,
    tracker::{ChordTracker, KeyEvent, Reaction},
};
use crate::{
    adapters::win32::{MASK_TAP, SYNTHETIC_INPUT_TAG, is_key_down, send_strokes},
    types::{AppError, PortError, PortResult},
};

/// Thread message: tap the menu mask key (posted by the callback, run after it returns).
const WM_ECHO_MASK: u32 = WM_APP + 1;

/// How often held keys are compared with what Windows reports, in ms.
const RECONCILE_INTERVAL_MS: u32 = 100;

/// Virtual key of the left Ctrl and the scan code AltGr layouts give their fake one.
const VK_LCONTROL: u32 = 0xA2;
const ALTGR_FAKE_CTRL_SCAN: u32 = 0x21D;

/// What the hook callback works with, owned by the hook thread.
struct HookState {
    shared: Arc<Shared>,
    tracker: ChordTracker,
    thread_id: u32,
    /// The reconcile timer, while one runs.
    timer: Option<usize>,
}

impl HookState {
    /// Emits the reaction's events, schedules its mask tap and keeps the reconcile timer in step; returns whether
    /// the key is swallowed.
    fn apply(&mut self, reaction: Reaction) -> bool {
        if !reaction.events.is_empty() {
            let sink = self.shared.sink.lock().clone();
            if let Some(sink) = sink {
                for event in reaction.events {
                    sink.emit(event);
                }
            }
        }
        if reaction.mask {
            // SAFETY: posts a plain message to this thread's own queue.
            let _ =
                unsafe { PostThreadMessageW(self.thread_id, WM_ECHO_MASK, WPARAM(0), LPARAM(0)) };
        }
        let watch = self.tracker.needs_reconcile();
        match (watch, self.timer) {
            (true, None) => {
                // SAFETY: a thread timer (no window, no callback); WM_TIMER arrives in this thread's queue.
                let id = unsafe { SetTimer(None, 0, RECONCILE_INTERVAL_MS, None) };
                self.timer = (id != 0).then_some(id);
            }
            (false, Some(id)) => {
                // SAFETY: stops the thread timer this state started.
                let _ = unsafe { KillTimer(None, id) };
                self.timer = None;
            }
            _ => {}
        }
        reaction.swallow
    }
}

thread_local! {
    static HOOK: RefCell<Option<HookState>> = const { RefCell::new(None) };
}

/// The running hook thread.
pub struct HookThread {
    thread_id: u32,
    join: Option<JoinHandle<()>>,
}

impl HookThread {
    /// Starts the thread and waits until its hook is installed (or failed to install).
    pub fn start(shared: Arc<Shared>) -> PortResult<Self> {
        let (ready, started) = mpsc::channel();
        let join = thread::Builder::new()
            .name(String::from("echo-keyboard-hook"))
            .spawn(move || run(shared, &ready))
            .map_err(|error| {
                internal(format!("the keyboard hook thread could not start: {error}"))
            })?;
        match started.recv() {
            Ok(Ok(thread_id)) => Ok(Self {
                thread_id,
                join: Some(join),
            }),
            Ok(Err(error)) => {
                // The thread has already returned; joining only collects it.
                let _ = join.join();
                Err(error)
            }
            Err(_) => {
                let _ = join.join();
                Err(internal(String::from(
                    "the keyboard hook thread ended before it was ready",
                )))
            }
        }
    }
}

impl Drop for HookThread {
    fn drop(&mut self) {
        // SAFETY: posts WM_QUIT to the hook thread's queue, which exists before `start` returns.
        if let Err(error) =
            unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) }
        {
            tracing::warn!(%error, "the keyboard hook thread could not be told to stop");
            return;
        }
        if let Some(join) = self.join.take()
            && join.join().is_err()
        {
            tracing::error!("the keyboard hook thread panicked");
        }
    }
}

fn internal(detail: String) -> PortError {
    PortError::new(AppError::Internal).with_detail(detail)
}

/// The hook thread: install, pump until WM_QUIT, uninstall.
fn run(shared: Arc<Shared>, ready: &mpsc::Sender<PortResult<u32>>) {
    // SAFETY: no arguments.
    let thread_id = unsafe { GetCurrentThreadId() };
    let mut message = MSG::default();
    // Creates this thread's message queue now, so a WM_QUIT posted right after start is kept.
    // SAFETY: `message` is a valid MSG to write into; nothing is removed from the queue.
    let _ = unsafe { PeekMessageW(&raw mut message, None, WM_USER, WM_USER, PM_NOREMOVE) };
    HOOK.with(|slot| {
        *slot.borrow_mut() = Some(HookState {
            shared,
            tracker: ChordTracker::default(),
            thread_id,
            timer: None,
        });
    });
    let hook = match install() {
        Ok(hook) => hook,
        Err(error) => {
            HOOK.with(|slot| slot.borrow_mut().take());
            let _ = ready.send(Err(error));
            return;
        }
    };
    if ready.send(Ok(thread_id)).is_err() {
        // Nobody waits for this hook any more.
        // SAFETY: `hook` was installed by this thread just above.
        let _ = unsafe { UnhookWindowsHookEx(hook) };
        HOOK.with(|slot| slot.borrow_mut().take());
        return;
    }
    tracing::info!("the keyboard hook is installed");
    loop {
        // SAFETY: `message` is a valid MSG; None reads every message of this thread.
        let got = unsafe { GetMessageW(&raw mut message, None, 0, 0) };
        // 0 is WM_QUIT, -1 an error (only for an invalid window filter, which None is not).
        if got.0 == 0 || got.0 == -1 {
            break;
        }
        match message.message {
            WM_TIMER => reconcile(),
            WM_ECHO_MASK => {
                if let Err(error) = send_strokes(&MASK_TAP) {
                    tracing::debug!(
                        detail = error.detail(),
                        "the menu mask key could not be tapped"
                    );
                }
            }
            _ => {}
        }
    }
    // SAFETY: `hook` was installed by this thread and is removed once.
    if let Err(error) = unsafe { UnhookWindowsHookEx(hook) } {
        tracing::warn!(%error, "the keyboard hook could not be removed");
    }
    HOOK.with(|slot| {
        if let Some(state) = slot.borrow_mut().take()
            && let Some(id) = state.timer
        {
            // SAFETY: stops the thread timer this thread started.
            let _ = unsafe { KillTimer(None, id) };
        }
    });
    tracing::info!("the keyboard hook is removed");
}

/// Installs the hook for every desktop thread on this thread.
fn install() -> PortResult<HHOOK> {
    // SAFETY: None asks for the module of the running executable; the handle is not freed.
    let module = unsafe { GetModuleHandleW(None) }
        .map_err(|error| internal(format!("the module handle could not be read: {error}")))?;
    // SAFETY: `hook_proc` has the HOOKPROC signature and lives for the whole program; 0 hooks every thread.
    unsafe {
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(hook_proc),
            Some(HINSTANCE(module.0)),
            0,
        )
    }
    .map_err(|error| internal(format!("the keyboard hook could not be installed: {error}")))
}

/// The low-level keyboard hook callback: decide, forward, return at once.
unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if u32::try_from(code).is_ok_and(|code| code == HC_ACTION) {
        // SAFETY: for HC_ACTION, `lparam` points to a KBDLLHOOKSTRUCT that is valid for the duration of this call.
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let message = u32::try_from(wparam.0).unwrap_or(0);
        if on_key(info, matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN)) {
            return LRESULT(1);
        }
    }
    // SAFETY: hands the event to the next hook with the arguments this call received.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Runs the tracker for one key event; true when the key must be swallowed.
fn on_key(info: &KBDLLHOOKSTRUCT, down: bool) -> bool {
    if info.dwExtraInfo == SYNTHETIC_INPUT_TAG {
        return false;
    }
    if info.vkCode == VK_LCONTROL && info.scanCode == ALTGR_FAKE_CTRL_SCAN {
        return false;
    }
    let Ok(vk) = u16::try_from(info.vkCode) else {
        return false;
    };
    HOOK.with(|slot| {
        let Ok(mut slot) = slot.try_borrow_mut() else {
            return false;
        };
        let Some(state) = slot.as_mut() else {
            return false;
        };
        let bindings = Arc::clone(&*state.shared.bindings.read());
        let reaction = state.tracker.handle(
            KeyEvent {
                vk,
                down,
                time: info.time,
            },
            &bindings,
        );
        state.apply(reaction)
    })
}

/// The reconcile timer fired: release held keys Windows no longer reports down.
fn reconcile() {
    HOOK.with(|slot| {
        let Ok(mut slot) = slot.try_borrow_mut() else {
            return;
        };
        let Some(state) = slot.as_mut() else {
            return;
        };
        let bindings = Arc::clone(&*state.shared.bindings.read());
        let reaction = state
            .tracker
            .reconcile(|vk| is_key_down(VIRTUAL_KEY(vk)), &bindings);
        state.apply(reaction);
    });
}
