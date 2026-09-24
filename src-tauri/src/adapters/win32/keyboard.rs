/*!
 * SOURCE OF TRUTH KEYWORDS: SYNTHETIC_INPUT_TAG, MENU_MASK_KEY, KeyStroke, send_strokes, is_key_down, SendInput batch, GetAsyncKeyState, synthetic keyboard input
 * WHAT:  The synthetic-keyboard building blocks Echo's Win32 adapters share: the tag every synthesised event carries,
 *        the unassigned "menu mask" key, KeyStroke (one key down or up), `send_strokes` (one uninterruptible
 *        SendInput batch) and `is_key_down` (the asynchronous key state of one key).
 * WHY:   The inserter synthesises Ctrl+V and the low-level keyboard hook must recognise those events as Echo's own
 *        (05 W9) and synthesise a mask tap itself; one copy keeps the tag, the mask key and the `unsafe` SendInput
 *        call reviewed once (root CLAUDE.md §1). vk E8 is unassigned, so tapping it makes a following Alt or Win
 *        release "not lone": no menu bar and no Start menu (05 W1, the key AutoHotkey uses for the same reason).
 * WHERE: adapters/inserter/win32_send_input.rs (the paste) and adapters/hotkey/low_level_hook (skips tagged events,
 *        taps the mask key when a modifier-only chord with Alt or Win fires, reconciles held keys).
 */

use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput,
    VIRTUAL_KEY, VK_LWIN, VK_RCONTROL, VK_RMENU, VK_RWIN,
};

use crate::types::{AppError, Permission, PortError, PortResult};

/// `dwExtraInfo` of every event Echo synthesises ("ECHO" in ASCII), so Echo's own keyboard hook can skip them.
pub const SYNTHETIC_INPUT_TAG: usize = 0x4543_484F;

/// An unassigned virtual key: tapping it makes a following Alt or Win release not "lone" (no menu, no Start).
pub const MENU_MASK_KEY: VIRTUAL_KEY = VIRTUAL_KEY(0xE8);

/// One synthetic key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyStroke {
    pub key: VIRTUAL_KEY,
    pub up: bool,
}

impl KeyStroke {
    pub const fn down(key: VIRTUAL_KEY) -> Self {
        Self { key, up: false }
    }

    pub const fn up(key: VIRTUAL_KEY) -> Self {
        Self { key, up: true }
    }
}

/// A tap of the menu mask key.
pub const MASK_TAP: [KeyStroke; 2] = [KeyStroke::down(MENU_MASK_KEY), KeyStroke::up(MENU_MASK_KEY)];

/// Keys whose scan code needs the extended-key prefix (right-hand modifiers and both Win keys).
const fn is_extended(key: VIRTUAL_KEY) -> bool {
    matches!(key, VK_RCONTROL | VK_RMENU | VK_LWIN | VK_RWIN)
}

/// The SendInput record of `stroke`, tagged with SYNTHETIC_INPUT_TAG.
pub fn keyboard_input(stroke: KeyStroke) -> INPUT {
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

/// Sends `strokes` as one uninterruptible batch; `PermissionDenied { input_injection }` when Windows refused part.
pub fn send_strokes(strokes: &[KeyStroke]) -> PortResult<()> {
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

/// True while `key` is physically down (as far as the input queue has processed it).
pub fn is_key_down(key: VIRTUAL_KEY) -> bool {
    // SAFETY: reads the asynchronous key state of one virtual key; no pointers.
    let state = unsafe { GetAsyncKeyState(i32::from(key.0)) };
    // The most significant bit is set while the key is down.
    state < 0
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_V;

    use super::*;

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
    fn the_mask_tap_is_a_down_then_an_up_of_an_unassigned_key() {
        assert_eq!(
            MASK_TAP,
            [KeyStroke::down(MENU_MASK_KEY), KeyStroke::up(MENU_MASK_KEY)]
        );
        assert_eq!(MENU_MASK_KEY.0, 0xE8);
    }
}
