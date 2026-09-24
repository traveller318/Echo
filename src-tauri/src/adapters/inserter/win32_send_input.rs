/*!
 * SOURCE OF TRUTH KEYWORDS: Win32SendInputInserter, SendInput, Ctrl+V, GetAsyncKeyState, modifier release, synthetic key-up, menu mask key, SetForegroundWindow, UIPI
 * WHAT:  Win32SendInputInserter: TextInserter that pastes what the clipboard holds into the take's target window
 *        with a synthetic Ctrl+V. Caps: uses the clipboard, cannot reach elevated windows.
 * WHY:   The user usually still holds the hotkey's modifiers when the take stops, so a Ctrl+V sent at once would
 *        arrive as Ctrl+Alt+V (05 W1): it polls GetAsyncKeyState until every modifier is up (at most 400 ms),
 *        then sends key-ups for any still held before the paste. A synthetic Alt or Win release with nothing
 *        pressed in between would open the target's menu bar or the Start menu, so an unassigned key (vk E8, the
 *        "menu mask" key AutoHotkey uses for the same reason) is tapped first. The paste goes to the window the
 *        take started in (05 W3): if focus moved, it is brought back to the front, and when Windows refuses the
 *        call fails rather than pasting into whatever is in front now. UIPI drops input into an elevated window
 *        while SendInput still reports success (05 W2), so an elevated target is refused up front with
 *        `PermissionDenied { input_injection }`; delivery then copies instead. Every event carries
 *        SYNTHETIC_INPUT_TAG so a future low-level keyboard hook adapter (05 W9) can ignore Echo's own keys.
 *        `text` is not used: the text is already on the clipboard (`InserterCaps.uses_clipboard`).
 * WHERE: Built by app/bootstrap into pipeline/delivery.rs's Delivery; called through `dyn TextInserter`.
 */

use std::{
    thread,
    time::{Duration, Instant},
};

use windows::Win32::{
    Foundation::HWND,
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
            KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput,
            VIRTUAL_KEY, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_RCONTROL,
            VK_RMENU, VK_RSHIFT, VK_RWIN, VK_V,
        },
        WindowsAndMessaging::{GetForegroundWindow, IsWindow, SetForegroundWindow},
    },
};

use crate::{
    adapters::win32::hwnd,
    ports::TextInserter,
    types::{AppError, AppTarget, InserterCaps, Permission, PortError, PortResult},
};

/// `dwExtraInfo` of every event Echo synthesises ("ECHO" in ASCII), so Echo's own keyboard hooks can skip them.
pub const SYNTHETIC_INPUT_TAG: usize = 0x4543_484F;

/// Longest wait for the user to let go of the hotkey's modifiers (05 W1).
const MODIFIER_RELEASE_TIMEOUT: Duration = Duration::from_millis(400);

/// Longest wait for the target to come back to the front after focus moved away during the take.
const FOCUS_TIMEOUT: Duration = Duration::from_millis(150);

/// How often key state and focus are re-read while waiting.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Every modifier that would turn Ctrl+V into another shortcut, by physical side so a key-up matches the held key.
const MODIFIERS: [VIRTUAL_KEY; 8] = [
    VK_LCONTROL,
    VK_RCONTROL,
    VK_LSHIFT,
    VK_RSHIFT,
    VK_LMENU,
    VK_RMENU,
    VK_LWIN,
    VK_RWIN,
];

/// An unassigned virtual key: tapping it makes a following Alt or Win release not "lone" (no menu, no Start).
const MENU_MASK_KEY: VIRTUAL_KEY = VIRTUAL_KEY(0xE8);

/// Pastes with SendInput.
#[derive(Debug, Default)]
pub struct Win32SendInputInserter;

impl Win32SendInputInserter {
    pub fn new() -> Self {
        Self
    }
}

impl TextInserter for Win32SendInputInserter {
    fn caps(&self) -> InserterCaps {
        InserterCaps {
            can_target_elevated: false,
            uses_clipboard: true,
        }
    }

    fn insert(&self, target: &AppTarget, _text: &str) -> PortResult<()> {
        if target.elevated {
            return Err(PortError::new(AppError::PermissionDenied {
                permission: Permission::InputInjection,
            })
            .with_detail("the target runs at a higher integrity level than Echo"));
        }
        let window = hwnd(target.window);
        bring_to_front(window)?;
        let held = wait_for_modifier_release(MODIFIER_RELEASE_TIMEOUT);
        if !held.is_empty() {
            tracing::debug!(
                held = held.len(),
                "modifiers still held after the wait; releasing them"
            );
        }
        send(&paste_strokes(&held))
    }
}

/// One synthetic key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KeyStroke {
    key: VIRTUAL_KEY,
    up: bool,
}

impl KeyStroke {
    const fn down(key: VIRTUAL_KEY) -> Self {
        Self { key, up: false }
    }

    const fn up(key: VIRTUAL_KEY) -> Self {
        Self { key, up: true }
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: paste_strokes, Ctrl+V sequence, release held modifiers, menu mask tap
 * WHAT:  The key events of one paste: key-ups for `held` modifiers (after a mask-key tap when Alt or Win is among
 *        them), then Ctrl down, V down, V up, Ctrl up.
 * WHY:   Pure, so the order that avoids Ctrl+Alt+V (05 W1) and the menu/Start side effects is unit-tested.
 * WHERE: Win32SendInputInserter::insert.
 */
fn paste_strokes(held: &[VIRTUAL_KEY]) -> Vec<KeyStroke> {
    let mut strokes = Vec::with_capacity(held.len() + 6);
    if held
        .iter()
        .any(|key| matches!(*key, VK_LMENU | VK_RMENU | VK_LWIN | VK_RWIN))
    {
        strokes.extend([KeyStroke::down(MENU_MASK_KEY), KeyStroke::up(MENU_MASK_KEY)]);
    }
    strokes.extend(held.iter().copied().map(KeyStroke::up));
    strokes.extend([
        KeyStroke::down(VK_CONTROL),
        KeyStroke::down(VK_V),
        KeyStroke::up(VK_V),
        KeyStroke::up(VK_CONTROL),
    ]);
    strokes
}

/// Keys whose scan code needs the extended-key prefix (right-hand modifiers and both Win keys).
const fn is_extended(key: VIRTUAL_KEY) -> bool {
    matches!(key, VK_RCONTROL | VK_RMENU | VK_LWIN | VK_RWIN)
}

fn keyboard_input(stroke: KeyStroke) -> INPUT {
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if stroke.up {
        flags |= KEYEVENTF_KEYUP;
    }
    if is_extended(stroke.key) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    // SAFETY: MapVirtualKeyW reads only its value arguments; 0 (no scan code) is a valid answer for unassigned keys.
    let scan = unsafe { MapVirtualKeyW(u32::from(stroke.key.0), MAPVK_VK_TO_VSC) };
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: stroke.key,
                wScan: u16::try_from(scan).unwrap_or(0),
                dwFlags: flags,
                time: 0,
                dwExtraInfo: SYNTHETIC_INPUT_TAG,
            },
        },
    }
}

/// Sends `strokes` as one uninterruptible batch.
fn send(strokes: &[KeyStroke]) -> PortResult<()> {
    let inputs: Vec<INPUT> = strokes.iter().copied().map(keyboard_input).collect();
    let size = i32::try_from(size_of::<INPUT>()).unwrap_or(i32::MAX);
    // SAFETY: `inputs` is a slice of initialised keyboard INPUTs and `size` is the size of one INPUT.
    let sent = unsafe { SendInput(&inputs, size) };
    if usize::try_from(sent).is_ok_and(|sent| sent == inputs.len()) {
        return Ok(());
    }
    Err(PortError::new(AppError::PermissionDenied {
        permission: Permission::InputInjection,
    })
    .with_detail(format!(
        "SendInput sent {sent} of {} events: {}",
        inputs.len(),
        windows::core::Error::from_win32()
    )))
}

/// True while `key` is physically down.
fn is_down(key: VIRTUAL_KEY) -> bool {
    // SAFETY: reads the asynchronous key state of one virtual key; no pointers.
    let state = unsafe { GetAsyncKeyState(i32::from(key.0)) };
    // The most significant bit is set while the key is down.
    state < 0
}

/// Waits until no modifier is held or `timeout` passes; returns the ones still held.
fn wait_for_modifier_release(timeout: Duration) -> Vec<VIRTUAL_KEY> {
    let deadline = Instant::now() + timeout;
    loop {
        let held: Vec<VIRTUAL_KEY> = MODIFIERS.into_iter().filter(|key| is_down(*key)).collect();
        if held.is_empty() || Instant::now() >= deadline {
            return held;
        }
        thread::sleep(POLL_INTERVAL);
    }
}

/// Makes `window` the foreground window again if focus moved away during the take.
fn bring_to_front(window: HWND) -> PortResult<()> {
    // SAFETY: IsWindow accepts any value and only reports whether it names a live window.
    if !unsafe { IsWindow(Some(window)) }.as_bool() {
        return Err(focus_failure("the target window was closed"));
    }
    // SAFETY: no arguments.
    if unsafe { GetForegroundWindow() } == window {
        return Ok(());
    }
    // SAFETY: `window` is a live window (checked above); a refusal is reported by the foreground check below.
    let _ = unsafe { SetForegroundWindow(window) };
    let deadline = Instant::now() + FOCUS_TIMEOUT;
    loop {
        // SAFETY: no arguments.
        if unsafe { GetForegroundWindow() } == window {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(focus_failure(
                "Windows did not bring the target window back to the front",
            ));
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn focus_failure(detail: &str) -> PortError {
    PortError::new(AppError::Internal).with_detail(detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ports::fakes::FakeForegroundApp,
        types::{AppTarget, WindowHandle},
    };

    const PASTE: [KeyStroke; 4] = [
        KeyStroke::down(VK_CONTROL),
        KeyStroke::down(VK_V),
        KeyStroke::up(VK_V),
        KeyStroke::up(VK_CONTROL),
    ];

    #[test]
    fn nothing_held_is_a_plain_ctrl_v() {
        assert_eq!(paste_strokes(&[]), PASTE);
    }

    #[test]
    fn held_ctrl_and_shift_are_released_before_the_paste() {
        let strokes = paste_strokes(&[VK_LCONTROL, VK_RSHIFT]);
        assert_eq!(
            strokes[..2],
            [KeyStroke::up(VK_LCONTROL), KeyStroke::up(VK_RSHIFT)]
        );
        assert_eq!(strokes[2..], PASTE);
    }

    #[test]
    fn a_held_alt_or_win_gets_the_mask_key_first() {
        for key in [VK_LMENU, VK_RMENU, VK_LWIN, VK_RWIN] {
            let strokes = paste_strokes(&[VK_LCONTROL, key]);
            assert_eq!(
                strokes[..4],
                [
                    KeyStroke::down(MENU_MASK_KEY),
                    KeyStroke::up(MENU_MASK_KEY),
                    KeyStroke::up(VK_LCONTROL),
                    KeyStroke::up(key),
                ]
            );
            assert_eq!(strokes[4..], PASTE);
        }
    }

    #[test]
    fn inputs_carry_the_tag_and_the_right_flags() {
        let up = keyboard_input(KeyStroke::up(VK_RCONTROL));
        // SAFETY: built as a keyboard input just above.
        let key = unsafe { up.Anonymous.ki };
        assert_eq!(up.r#type, INPUT_KEYBOARD);
        assert_eq!(key.dwExtraInfo, SYNTHETIC_INPUT_TAG);
        assert!(key.dwFlags.contains(KEYEVENTF_KEYUP));
        assert!(key.dwFlags.contains(KEYEVENTF_EXTENDEDKEY));
        assert_ne!(key.wScan, 0);
        let down = keyboard_input(KeyStroke::down(VK_V));
        // SAFETY: as above.
        let key = unsafe { down.Anonymous.ki };
        assert_eq!(key.dwFlags, KEYBD_EVENT_FLAGS(0));
    }

    #[test]
    fn caps_are_honest() {
        assert_eq!(
            Win32SendInputInserter::new().caps(),
            InserterCaps {
                can_target_elevated: false,
                uses_clipboard: true,
            }
        );
    }

    /// Both refusals happen before any key is sent, so the test never types into the developer's desktop.
    #[test]
    fn elevated_and_closed_targets_are_refused_without_sending_keys() {
        let inserter = Win32SendInputInserter::new();
        let elevated = FakeForegroundApp::target("taskmgr.exe", true);
        assert_eq!(
            inserter
                .insert(&elevated, "text")
                .map_err(PortError::into_app_error),
            Err(AppError::PermissionDenied {
                permission: Permission::InputInjection
            })
        );
        let closed = AppTarget {
            window: WindowHandle::from_raw(0),
            ..FakeForegroundApp::target("notepad.exe", false)
        };
        assert_eq!(
            inserter
                .insert(&closed, "text")
                .map_err(PortError::into_app_error),
            Err(AppError::Internal)
        );
    }
}
