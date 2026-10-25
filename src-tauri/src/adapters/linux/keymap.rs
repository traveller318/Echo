/*!
 * SOURCE OF TRUTH KEYWORDS: keymap, evdev key code, X11 keycode, KEY_ constants, virtual key translation, vk_from_evdev, evdev_from_vk, linux/input-event-codes.h
 * WHAT:  The translation between Linux key codes and the Windows virtual-key numbers the chord engine speaks: `vk_from_evdev`
 *        (a `KEY_*` code from /dev/input), `vk_from_x11_keycode` (an X11 keycode, which is the evdev code plus 8) and
 *        `evdev_from_vk` (the other way, for asking a device whether a key is still down).
 * WHY:   Both Linux backends see physical keys, not characters, so a shortcut names the key by its US-layout position,
 *        exactly as it does on Windows (the OEM codes in chord.rs). Left and right modifiers keep their own codes
 *        (VK_LCONTROL, VK_RCONTROL, ...), which `key_of` folds into one modifier. The table is the single list, so the
 *        forward and reverse lookups cannot drift; keys no shortcut can name (media, language keys) map to nothing and
 *        are ignored.
 * WHERE: adapters/hotkey/x11_xinput.rs, adapters/linux/x11.rs and (later) the evdev hotkey adapter.
 */

/// X11 keycodes are the evdev codes shifted up by 8 (the legacy of the first 8 reserved codes).
const X11_OFFSET: u16 = 8;

/// (evdev `KEY_*` code, Windows virtual-key code).
const KEYS: &[(u16, u16)] = &[
    (1, 0x1B), // KEY_ESC
    (2, b'1' as u16),
    (3, b'2' as u16),
    (4, b'3' as u16),
    (5, b'4' as u16),
    (6, b'5' as u16),
    (7, b'6' as u16),
    (8, b'7' as u16),
    (9, b'8' as u16),
    (10, b'9' as u16),
    (11, b'0' as u16),
    (12, 0xBD), // KEY_MINUS
    (13, 0xBB), // KEY_EQUAL
    (14, 0x08), // KEY_BACKSPACE
    (15, 0x09), // KEY_TAB
    (16, b'Q' as u16),
    (17, b'W' as u16),
    (18, b'E' as u16),
    (19, b'R' as u16),
    (20, b'T' as u16),
    (21, b'Y' as u16),
    (22, b'U' as u16),
    (23, b'I' as u16),
    (24, b'O' as u16),
    (25, b'P' as u16),
    (26, 0xDB), // KEY_LEFTBRACE
    (27, 0xDD), // KEY_RIGHTBRACE
    (28, 0x0D), // KEY_ENTER
    (29, 0xA2), // KEY_LEFTCTRL
    (30, b'A' as u16),
    (31, b'S' as u16),
    (32, b'D' as u16),
    (33, b'F' as u16),
    (34, b'G' as u16),
    (35, b'H' as u16),
    (36, b'J' as u16),
    (37, b'K' as u16),
    (38, b'L' as u16),
    (39, 0xBA), // KEY_SEMICOLON
    (40, 0xDE), // KEY_APOSTROPHE
    (41, 0xC0), // KEY_GRAVE
    (42, 0xA0), // KEY_LEFTSHIFT
    (43, 0xDC), // KEY_BACKSLASH
    (44, b'Z' as u16),
    (45, b'X' as u16),
    (46, b'C' as u16),
    (47, b'V' as u16),
    (48, b'B' as u16),
    (49, b'N' as u16),
    (50, b'M' as u16),
    (51, 0xBC), // KEY_COMMA
    (52, 0xBE), // KEY_DOT
    (53, 0xBF), // KEY_SLASH
    (54, 0xA1), // KEY_RIGHTSHIFT
    (55, 0x6A), // KEY_KPASTERISK
    (56, 0xA4), // KEY_LEFTALT
    (57, 0x20), // KEY_SPACE
    (58, 0x14), // KEY_CAPSLOCK
    (59, 0x70),
    (60, 0x71),
    (61, 0x72),
    (62, 0x73),
    (63, 0x74), // KEY_F1..F5
    (64, 0x75),
    (65, 0x76),
    (66, 0x77),
    (67, 0x78),
    (68, 0x79), // KEY_F6..F10
    (69, 0x90), // KEY_NUMLOCK
    (70, 0x91), // KEY_SCROLLLOCK
    (71, 0x67),
    (72, 0x68),
    (73, 0x69), // KEY_KP7..KP9
    (74, 0x6D), // KEY_KPMINUS
    (75, 0x64),
    (76, 0x65),
    (77, 0x66), // KEY_KP4..KP6
    (78, 0x6B), // KEY_KPPLUS
    (79, 0x61),
    (80, 0x62),
    (81, 0x63), // KEY_KP1..KP3
    (82, 0x60), // KEY_KP0
    (83, 0x6E), // KEY_KPDOT
    (87, 0x7A),
    (88, 0x7B),  // KEY_F11, KEY_F12
    (96, 0x0D),  // KEY_KPENTER
    (97, 0xA3),  // KEY_RIGHTCTRL
    (98, 0x6F),  // KEY_KPSLASH
    (99, 0x2C),  // KEY_SYSRQ (Print Screen)
    (100, 0xA5), // KEY_RIGHTALT
    (102, 0x24), // KEY_HOME
    (103, 0x26), // KEY_UP
    (104, 0x21), // KEY_PAGEUP
    (105, 0x25), // KEY_LEFT
    (106, 0x27), // KEY_RIGHT
    (107, 0x23), // KEY_END
    (108, 0x28), // KEY_DOWN
    (109, 0x22), // KEY_PAGEDOWN
    (110, 0x2D), // KEY_INSERT
    (111, 0x2E), // KEY_DELETE
    (119, 0x13), // KEY_PAUSE
    (125, 0x5B), // KEY_LEFTMETA
    (126, 0x5C), // KEY_RIGHTMETA
    (183, 0x7C),
    (184, 0x7D),
    (185, 0x7E),
    (186, 0x7F),
    (187, 0x80),
    (188, 0x81), // KEY_F13..F18
    (189, 0x82),
    (190, 0x83),
    (191, 0x84),
    (192, 0x85),
    (193, 0x86),
    (194, 0x87), // KEY_F19..F24
];

/// The virtual-key code of an evdev `KEY_*` code, if a shortcut can name that key.
pub fn vk_from_evdev(code: u16) -> Option<u16> {
    KEYS.iter()
        .find_map(|(evdev, vk)| (*evdev == code).then_some(*vk))
}

/// The virtual-key code of an X11 keycode.
pub fn vk_from_x11_keycode(keycode: u16) -> Option<u16> {
    vk_from_evdev(keycode.checked_sub(X11_OFFSET)?)
}

/// The evdev code that produces `vk` (the first listed, so the main Enter rather than the keypad's).
pub fn evdev_from_vk(vk: u16) -> Option<u16> {
    KEYS.iter()
        .find_map(|(evdev, mapped)| (*mapped == vk).then_some(*evdev))
}

/// The X11 keycode that produces `vk`.
pub fn x11_keycode_from_vk(vk: u16) -> Option<u16> {
    evdev_from_vk(vk).map(|code| code + X11_OFFSET)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{adapters::hotkey::chords::chord::parse, types::Shortcut};

    #[test]
    fn evdev_codes_are_unique() {
        let mut codes: Vec<u16> = KEYS.iter().map(|(code, _)| *code).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), KEYS.len());
    }

    #[test]
    fn keys_translate_both_ways() {
        assert_eq!(vk_from_evdev(47), Some(u16::from(b'V')));
        assert_eq!(vk_from_x11_keycode(55), Some(u16::from(b'V')));
        assert_eq!(x11_keycode_from_vk(u16::from(b'V')), Some(55));
        assert_eq!(vk_from_evdev(29), Some(0xA2));
        assert_eq!(vk_from_evdev(100), Some(0xA5));
        assert_eq!(
            evdev_from_vk(0x0D),
            Some(28),
            "the main Enter wins over the keypad's"
        );
        assert_eq!(vk_from_evdev(163), None, "media keys are not shortcut keys");
        assert_eq!(vk_from_x11_keycode(3), None);
    }

    /// Every key a shortcut can name has a physical key, so a binding the user types can actually be pressed.
    #[test]
    fn every_nameable_key_can_be_pressed() {
        let nameable = [
            "A",
            "Z",
            "0",
            "9",
            "F1",
            "F12",
            "F24",
            "Space",
            "Enter",
            "Tab",
            "Escape",
            "Backspace",
            "Delete",
            "Insert",
            "Home",
            "End",
            "PageUp",
            "PageDown",
            "Up",
            "Down",
            "Left",
            "Right",
            "CapsLock",
            "NumLock",
            "ScrollLock",
            "Pause",
            "PrintScreen",
            ",",
            ".",
            "/",
            ";",
            "'",
            "[",
            "]",
            "\\",
            "-",
            "=",
            "`",
            "Numpad0",
            "Numpad9",
            "NumpadAdd",
            "NumpadSubtract",
            "NumpadMultiply",
            "NumpadDivide",
            "NumpadDecimal",
        ];
        for name in nameable {
            let chord = parse(&Shortcut::from(format!("Ctrl+{name}"))).unwrap();
            let vk = chord.key.unwrap();
            assert!(
                evdev_from_vk(vk).is_some(),
                "{name} (vk {vk:#x}) has no physical key"
            );
        }
    }
}
