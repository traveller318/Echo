/*!
 * SOURCE OF TRUTH KEYWORDS: HookThread, WH_KEYBOARD_LL, SetWindowsHookExW, hook_proc, LowLevelHooksTimeout, hook renewal, time-critical hook thread, AltGr fake Ctrl, reconcile timer, mask tap
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
 *        need to release is held (a key stuck down in Windows' own state is let go once stale, tracker.rs), so an
 *        idle Echo wakes only for the renewal below; it reads the WM_TIMER message time, the clock key events carry. The message queue is created
 *        before the thread reports ready, so WM_QUIT posted right after start is never lost.
 *        Windows skips a hook callback that misses `LowLevelHooksTimeout` (losing that key for Echo) and may remove
 *        the hook silently, with no notification, which left the hotkeys dead until Echo restarted. Two defences:
 *        the thread runs time-critical, so ONNX inference saturating every core can never delay a callback (the
 *        callback is microseconds of pure work, so it cannot starve anything else); and every RENEW_INTERVAL_MS the
 *        thread installs a fresh hook and removes the old one a moment later (Hooks). Renewal keeps the thread and
 *        its tracker (a chord held across it stays held) and costs two wake-ups per interval.
 * WHERE: Started and stopped by LowLevelKeyboardHotkeys (mod.rs) while at least one binding exists.
 */

use std::{
    cell::RefCell,
    sync::{Arc, mpsc},
    thread::{self, JoinHandle},
};

use windows::Win32::{
    Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM},
    System::{
        LibraryLoader::GetModuleHandleW,
        Threading::{GetCurrentThreadId, THREAD_PRIORITY_TIME_CRITICAL},
    },
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
    adapters::win32::{
        MASK_TAP, SYNTHETIC_INPUT_TAG, is_key_down, send_strokes, set_current_thread_priority,
    },
    types::{AppError, PortError, PortResult},
};

/// Thread message: tap the menu mask key (posted by the callback, run after it returns).
const WM_ECHO_MASK: u32 = WM_APP + 1;

/// How often held keys are compared with what Windows reports, in ms.
const RECONCILE_INTERVAL_MS: u32 = 100;

/// How often the hook is installed afresh, in ms: the longest a hook Windows silently removed stays gone.
const RENEW_INTERVAL_MS: u32 = 15_000;

/// How long a replaced hook stays installed, in ms: Windows' ceiling for `LowLevelHooksTimeout`, so every call it had
/// already aimed at the old hook has been answered (or given up on) by then.
const RETIRE_DELAY_MS: u32 = 1_000;

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
        Self::start_renewing(shared, Renewal::DEFAULT)
    }

    /// `start` with its own renewal timing (tests renew far more often).
    fn start_renewing(shared: Arc<Shared>, renewal: Renewal) -> PortResult<Self> {
        let (ready, started) = mpsc::channel();
        let join = thread::Builder::new()
            .name(String::from("echo-keyboard-hook"))
            .spawn(move || run(shared, renewal, &ready))
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
fn run(shared: Arc<Shared>, renewal: Renewal, ready: &mpsc::Sender<PortResult<u32>>) {
    // SAFETY: no arguments.
    let thread_id = unsafe { GetCurrentThreadId() };
    let mut message = MSG::default();
    if let Err(error) = set_current_thread_priority(THREAD_PRIORITY_TIME_CRITICAL) {
        tracing::warn!(%error, "the keyboard hook thread runs at normal priority");
    }
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
    let mut hooks = Hooks::new(hook, renewal);
    loop {
        // SAFETY: `message` is a valid MSG; None reads every message of this thread.
        let got = unsafe { GetMessageW(&raw mut message, None, 0, 0) };
        // 0 is WM_QUIT, -1 an error (only for an invalid window filter, which None is not).
        if got.0 == 0 || got.0 == -1 {
            break;
        }
        match message.message {
            WM_TIMER => match hooks.timer(message.wParam.0) {
                Some(HookTimer::Renew) => hooks.renew(),
                Some(HookTimer::Retire) => hooks.retire(),
                None => reconcile(message.time),
            },
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
    hooks.remove();
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

/// How often the hook is replaced and how long a replaced hook stays installed, in ms.
#[derive(Debug, Clone, Copy)]
struct Renewal {
    every_ms: u32,
    retire_after_ms: u32,
}

impl Renewal {
    /// Every RENEW_INTERVAL_MS; a replaced hook goes after RETIRE_DELAY_MS.
    const DEFAULT: Self = Self {
        every_ms: RENEW_INTERVAL_MS,
        retire_after_ms: RETIRE_DELAY_MS,
    };
}

/// Which of the hooks' timers fired.
enum HookTimer {
    Renew,
    Retire,
}

/**
 * SOURCE OF TRUTH KEYWORDS: Hooks, renew keyboard hook, retire replaced hook, silently removed hook, hook watchdog, hook overlap, RETIRE_DELAY_MS
 * WHAT:  This thread's hook handles and their thread timers: the live hook, replaced every `Renewal::every_ms`, and
 *        the hook a renewal replaced, removed `Renewal::retire_after_ms` later.
 * WHY:   Windows gives no sign when it drops a hook (file header), so the hook is replaced on a schedule. Replacing
 *        is install-first, retire-later: unhooking first leaves an instant with no hook at all, and a key whose hook
 *        call Windows had already aimed at the old hook is lost if that hook goes before this thread answers the
 *        call (unhooking right after the install lost about one chord in four in the renewal test; retiring later
 *        lost none). While both are installed a key reaches both
 *        through CallNextHookEx, which is harmless: the second call carries the same timestamp, so the tracker takes
 *        a down for a repeat and finds nothing left to release on an up, and a swallowed key never reaches the old
 *        hook. The thread-local tracker is untouched, so held chords survive a renewal. A failed install keeps the
 *        live hook (the next renewal tries again); a replaced hook whose unhook fails had been removed by Windows,
 *        which is logged so a silent removal shows up in the log.
 * WHERE: `run`: built after the first install, driven by its WM_TIMER messages, removed when the thread ends.
 */
struct Hooks {
    live: HHOOK,
    retiring: Option<HHOOK>,
    renewal: Renewal,
    /// Thread timer ids; 0 while none runs.
    renew_timer: usize,
    retire_timer: usize,
}

impl Hooks {
    fn new(live: HHOOK, renewal: Renewal) -> Self {
        // SAFETY: a thread timer (no window, no callback); its WM_TIMER carries the returned id in wParam.
        let renew_timer = unsafe { SetTimer(None, 0, renewal.every_ms, None) };
        if renew_timer == 0 {
            tracing::warn!(
                "the keyboard hook will not be renewed; only a wake from sleep reinstalls it"
            );
        }
        Self {
            live,
            retiring: None,
            renewal,
            renew_timer,
            retire_timer: 0,
        }
    }

    /// Which hook timer `id` is; None for any other (the tracker's reconcile timer).
    fn timer(&self, id: usize) -> Option<HookTimer> {
        if id != 0 && id == self.renew_timer {
            Some(HookTimer::Renew)
        } else if id != 0 && id == self.retire_timer {
            Some(HookTimer::Retire)
        } else {
            None
        }
    }

    /// Installs a fresh hook in front of the live one and schedules the live one's removal.
    fn renew(&mut self) {
        let fresh = match install() {
            Ok(fresh) => fresh,
            Err(error) => {
                tracing::warn!(
                    detail = error.detail(),
                    "the keyboard hook could not be renewed; the next renewal tries again"
                );
                return;
            }
        };
        // A hook still waiting from the previous renewal has had a whole interval; it goes now.
        self.retire();
        self.retiring = Some(std::mem::replace(&mut self.live, fresh));
        // SAFETY: a one-shot use of a thread timer; `retire` kills it.
        self.retire_timer = unsafe { SetTimer(None, 0, self.renewal.retire_after_ms, None) };
        if self.retire_timer == 0 {
            self.retire();
        }
    }

    /// Removes the hook a renewal replaced, if any, and stops its timer.
    fn retire(&mut self) {
        if self.retire_timer != 0 {
            // SAFETY: stops the thread timer this thread started.
            let _ = unsafe { KillTimer(None, self.retire_timer) };
            self.retire_timer = 0;
        }
        if let Some(old) = self.retiring.take() {
            // SAFETY: `old` was installed by this thread; it is removed once and never used again.
            if let Err(error) = unsafe { UnhookWindowsHookEx(old) } {
                tracing::warn!(
                    %error,
                    "Windows had removed the keyboard hook; the renewed one replaced it"
                );
            }
        }
    }

    /// Stops the timers and removes every hook this thread installed.
    fn remove(mut self) {
        self.retire();
        if self.renew_timer != 0 {
            // SAFETY: stops the thread timer this thread started.
            let _ = unsafe { KillTimer(None, self.renew_timer) };
        }
        // SAFETY: the live hook was installed by this thread and is removed once.
        if let Err(error) = unsafe { UnhookWindowsHookEx(self.live) } {
            tracing::warn!(%error, "the keyboard hook could not be removed");
        }
    }
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

/// The reconcile timer fired at `now` (its message time, the clock key events carry): release held keys whose key-up
/// was lost. Logged, since a lost key-up is otherwise invisible and was once the whole of a dead hotkey.
fn reconcile(now: u32) {
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
            .reconcile(now, |vk| is_key_down(VIRTUAL_KEY(vk)), &bindings);
        for vk in &reaction.lost {
            tracing::info!(
                vk = format_args!("{vk:#04X}"),
                "a held key's release never arrived; it no longer counts as held"
            );
        }
        state.apply(reaction);
    });
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, SendInput, VIRTUAL_KEY, VK_F21, VK_LCONTROL, VK_LMENU, VK_LSHIFT,
    };

    use super::*;
    use crate::{
        adapters::{
            hotkey::low_level_hook::{REAL_KEYS, chord::parse, tracker::HookBinding},
            win32::{KeyStroke, keyboard_input},
        },
        ports::fakes::RecordingSink,
        types::{HotkeyEvent, HotkeyId, KeyState, Shortcut},
    };

    const RECORD: HotkeyId = HotkeyId::from_static("record");

    /// Types `keys` down then up the way a physical keyboard does (no Echo tag). Shift goes first, so the keys never
    /// pass through Ctrl+Alt alone and an Echo running on this machine (hold-to-talk on Ctrl+Alt) ignores them.
    fn type_chord(keys: &[VIRTUAL_KEY]) {
        let strokes = keys
            .iter()
            .copied()
            .map(KeyStroke::down)
            .chain(keys.iter().rev().copied().map(KeyStroke::up));
        let inputs: Vec<INPUT> = strokes
            .map(|stroke| {
                let mut input = keyboard_input(stroke);
                input.Anonymous.ki.dwExtraInfo = 0;
                input
            })
            .collect();
        // SAFETY: `inputs` holds initialised keyboard INPUTs and the size is the size of one.
        let sent = unsafe { SendInput(&inputs, i32::try_from(size_of::<INPUT>()).unwrap()) };
        assert_eq!(usize::try_from(sent).unwrap(), inputs.len());
    }

    /// A hook renewed every 25 ms reports every one of 60 chords typed across dozens of renewals, and stops cleanly.
    #[test]
    fn a_renewed_hook_keeps_reporting_every_chord() {
        const TIMES: usize = 60;
        let _keys = REAL_KEYS.lock();
        let sink = Arc::new(RecordingSink::default());
        let shared = Arc::new(Shared::default());
        *shared.sink.lock() = Some(sink.clone());
        *shared.bindings.write() = Arc::from([HookBinding {
            id: RECORD,
            chord: parse(&Shortcut::from_static("Ctrl+Alt+Shift+F21")).unwrap(),
        }]);
        let renewal = Renewal {
            every_ms: 25,
            retire_after_ms: 10,
        };
        let hook = HookThread::start_renewing(shared, renewal).unwrap();
        for _ in 0..TIMES {
            type_chord(&[VK_LSHIFT, VK_LCONTROL, VK_LMENU, VK_F21]);
            thread::sleep(Duration::from_millis(15));
        }
        let mut events = Vec::new();
        for _ in 0..100 {
            events = sink.events();
            if events.len() >= 2 * TIMES {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let chord = [
            HotkeyEvent {
                id: RECORD,
                state: KeyState::Pressed,
            },
            HotkeyEvent {
                id: RECORD,
                state: KeyState::Released,
            },
        ];
        let expected: Vec<_> = chord.iter().cycle().take(2 * TIMES).cloned().collect();
        assert_eq!(events, expected);
        drop(hook);
    }
}
