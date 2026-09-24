/*!
 * SOURCE OF TRUTH KEYWORDS: Chord, Modifiers, parse chord, accelerator syntax, modifier-only combination, virtual key names, key_of, vk classification
 * WHAT:  Chord (a set of modifiers plus at most one main key, as a Windows virtual-key code), Modifiers (Ctrl, Alt,
 *        Shift, Win), `parse(shortcut)` for the accelerator text Echo stores (`Ctrl+Alt+Space`, `Ctrl+Alt`,
 *        `Escape`) and `key_of(vk)`, which says whether a virtual key is a modifier (and which) or a main key.
 * WHY:   The combination stays text everywhere else (types/hotkey.rs) and only the hotkey adapter parses it. The
 *        spelling follows the accelerator syntax the Settings UI and the previous global-shortcut adapter used
 *        (case-insensitive, `+`-separated, the main key last), so stored values keep working. A modifier-only chord
 *        needs at least two modifiers: a lone Ctrl or Shift would fire on every shortcut the user types. The
 *        low-level hook reports left and right modifiers separately, so both sides count as the same modifier.
 * WHERE: `parse` by LowLevelKeyboardHotkeys::register; `key_of` by the tracker for every key event.
 */

use std::ops::{BitOr, BitOrAssign, Sub};

use crate::types::{AppError, HotkeyIssue, PortError, PortResult, Shortcut};

/// A set of modifier keys, side-agnostic.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const NONE: Self = Self(0);
    pub const CTRL: Self = Self(1);
    pub const ALT: Self = Self(1 << 1);
    pub const SHIFT: Self = Self(1 << 2);
    pub const WIN: Self = Self(1 << 3);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// How many distinct modifiers the set holds.
    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }

    /// Alt or Win is in the set: releasing them alone would open a menu bar or the Start menu.
    pub const fn opens_menus(self) -> bool {
        self.0 & (Self::ALT.0 | Self::WIN.0) != 0
    }
}

impl BitOr for Modifiers {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl BitOrAssign for Modifiers {
    fn bitor_assign(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

impl Sub for Modifiers {
    type Output = Self;

    /// The modifiers of `self` that are not in `other`.
    fn sub(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
}

/// A bound combination: modifiers plus an optional main key (a virtual-key code).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Chord {
    pub modifiers: Modifiers,
    /// None for a modifier-only chord (e.g. Ctrl+Alt).
    pub key: Option<u16>,
}

impl Chord {
    pub const fn is_modifier_only(&self) -> bool {
        self.key.is_none()
    }
}

/// What a virtual key is to the chord matcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    Modifier(Modifiers),
    Main,
}

/// Whether `vk` is a modifier (either side) or a main key.
pub const fn key_of(vk: u16) -> KeyKind {
    match vk {
        // VK_CONTROL, VK_LCONTROL, VK_RCONTROL
        0x11 | 0xA2 | 0xA3 => KeyKind::Modifier(Modifiers::CTRL),
        // VK_MENU, VK_LMENU, VK_RMENU
        0x12 | 0xA4 | 0xA5 => KeyKind::Modifier(Modifiers::ALT),
        // VK_SHIFT, VK_LSHIFT, VK_RSHIFT
        0x10 | 0xA0 | 0xA1 => KeyKind::Modifier(Modifiers::SHIFT),
        // VK_LWIN, VK_RWIN
        0x5B | 0x5C => KeyKind::Modifier(Modifiers::WIN),
        _ => KeyKind::Main,
    }
}

/// The modifier a token names (case-insensitive), if it names one.
fn modifier_named(token: &str) -> Option<Modifiers> {
    match token {
        "ctrl" | "control" | "cmdorctrl" | "cmdorcontrol" | "commandorctrl"
        | "commandorcontrol" => Some(Modifiers::CTRL),
        "alt" | "option" => Some(Modifiers::ALT),
        "shift" => Some(Modifiers::SHIFT),
        "super" | "win" | "windows" | "meta" | "cmd" | "command" => Some(Modifiers::WIN),
        _ => None,
    }
}

/// Named keys and the punctuation keys of the US layout (the OEM codes Windows reports for them).
const NAMED_KEYS: &[(&str, u16)] = &[
    ("space", 0x20),
    ("enter", 0x0D),
    ("return", 0x0D),
    ("tab", 0x09),
    ("escape", 0x1B),
    ("esc", 0x1B),
    ("backspace", 0x08),
    ("delete", 0x2E),
    ("del", 0x2E),
    ("insert", 0x2D),
    ("home", 0x24),
    ("end", 0x23),
    ("pageup", 0x21),
    ("pagedown", 0x22),
    ("up", 0x26),
    ("arrowup", 0x26),
    ("down", 0x28),
    ("arrowdown", 0x28),
    ("left", 0x25),
    ("arrowleft", 0x25),
    ("right", 0x27),
    ("arrowright", 0x27),
    ("capslock", 0x14),
    ("numlock", 0x90),
    ("scrolllock", 0x91),
    ("pause", 0x13),
    ("printscreen", 0x2C),
    ("comma", 0xBC),
    (",", 0xBC),
    ("period", 0xBE),
    (".", 0xBE),
    ("slash", 0xBF),
    ("/", 0xBF),
    ("semicolon", 0xBA),
    (";", 0xBA),
    ("quote", 0xDE),
    ("'", 0xDE),
    ("bracketleft", 0xDB),
    ("[", 0xDB),
    ("bracketright", 0xDD),
    ("]", 0xDD),
    ("backslash", 0xDC),
    ("\\", 0xDC),
    ("minus", 0xBD),
    ("-", 0xBD),
    ("equal", 0xBB),
    ("=", 0xBB),
    ("backquote", 0xC0),
    ("`", 0xC0),
    ("numpadmultiply", 0x6A),
    ("numpadadd", 0x6B),
    ("numpadsubtract", 0x6D),
    ("numpaddecimal", 0x6E),
    ("numpaddivide", 0x6F),
];

/// The virtual-key code a (lower-case) main-key token names.
fn key_named(token: &str) -> Option<u16> {
    let letter_or_digit = |text: &str| -> Option<u16> {
        let mut chars = text.chars();
        let only = chars.next()?;
        if chars.next().is_some() {
            return None;
        }
        match only {
            // VK_A..VK_Z share the upper-case ASCII codes, VK_0..VK_9 the digit codes.
            'a'..='z' => u16::try_from(u32::from(only.to_ascii_uppercase())).ok(),
            '0'..='9' => u16::try_from(u32::from(only)).ok(),
            _ => None,
        }
    };
    if let Some(vk) = letter_or_digit(token) {
        return Some(vk);
    }
    if let Some(rest) = token.strip_prefix("key") {
        return letter_or_digit(rest).filter(|vk| (0x41..=0x5A).contains(vk));
    }
    if let Some(rest) = token.strip_prefix("digit") {
        return letter_or_digit(rest).filter(|vk| (0x30..=0x39).contains(vk));
    }
    if let Some(rest) = token.strip_prefix("numpad")
        && let Some(vk) = letter_or_digit(rest).filter(|vk| (0x30..=0x39).contains(vk))
    {
        // VK_NUMPAD0..VK_NUMPAD9 are 0x60..0x69.
        return Some(vk + 0x30);
    }
    if let Some(number) = token
        .strip_prefix('f')
        .and_then(|rest| rest.parse::<u16>().ok())
        && (1..=24).contains(&number)
    {
        // VK_F1..VK_F24 are 0x70..0x87.
        return Some(0x6F + number);
    }
    NAMED_KEYS
        .iter()
        .find(|(name, _)| *name == token)
        .map(|(_, vk)| *vk)
}

fn invalid(shortcut: &Shortcut, why: &str) -> PortError {
    PortError::new(AppError::Hotkey {
        reason: HotkeyIssue::Invalid,
    })
    .with_detail(format!("{shortcut:?} cannot be bound: {why}"))
}

/// The chord `shortcut` spells, or `Hotkey { invalid }`.
pub fn parse(shortcut: &Shortcut) -> PortResult<Chord> {
    let text = shortcut.as_str().trim();
    if text.is_empty() {
        return Err(invalid(shortcut, "it is empty"));
    }
    let tokens: Vec<String> = text
        .split('+')
        .map(|token| token.trim().to_ascii_lowercase())
        .collect();
    let mut modifiers = Modifiers::NONE;
    let mut key = None;
    for (index, token) in tokens.iter().enumerate() {
        if token.is_empty() {
            return Err(invalid(shortcut, "it has an empty part"));
        }
        if let Some(modifier) = modifier_named(token) {
            if modifiers.contains(modifier) {
                return Err(invalid(shortcut, "a modifier is named twice"));
            }
            modifiers |= modifier;
            continue;
        }
        if index + 1 != tokens.len() {
            return Err(invalid(shortcut, "the main key must come last"));
        }
        key = Some(key_named(token).ok_or_else(|| invalid(shortcut, "unknown key"))?);
    }
    if key.is_none() && modifiers.count() < 2 {
        return Err(invalid(
            shortcut,
            "a combination of modifiers alone needs at least two of them",
        ));
    }
    Ok(Chord { modifiers, key })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(text: &str) -> PortResult<Chord> {
        parse(&Shortcut::from(text.to_owned()))
    }

    fn is_invalid(text: &str) -> bool {
        matches!(
            chord(text).map_err(PortError::into_app_error),
            Err(AppError::Hotkey {
                reason: HotkeyIssue::Invalid
            })
        )
    }

    #[test]
    fn combinations_parse_case_insensitively() {
        assert_eq!(
            chord("Ctrl+Alt+Space").unwrap(),
            Chord {
                modifiers: Modifiers::CTRL | Modifiers::ALT,
                key: Some(0x20)
            }
        );
        assert_eq!(
            chord("ctrl+alt+space").unwrap(),
            chord("Control + Option + SPACE").unwrap()
        );
        assert_eq!(
            chord("Escape").unwrap(),
            Chord {
                modifiers: Modifiers::NONE,
                key: Some(0x1B)
            }
        );
        assert_eq!(chord("CmdOrCtrl+Shift+D").unwrap().key, Some(0x44));
        assert_eq!(chord("Super+F24").unwrap().key, Some(0x87));
        assert_eq!(chord("Ctrl+F1").unwrap().key, Some(0x70));
        assert_eq!(chord("Alt+KeyQ").unwrap().key, Some(0x51));
        assert_eq!(chord("Alt+Digit7").unwrap().key, Some(0x37));
        assert_eq!(chord("Alt+7").unwrap().key, Some(0x37));
        assert_eq!(chord("Ctrl+Numpad3").unwrap().key, Some(0x63));
        assert_eq!(chord("Ctrl+/").unwrap().key, Some(0xBF));
        assert_eq!(
            chord("Ctrl+ArrowUp").unwrap().key,
            chord("Ctrl+Up").unwrap().key
        );
    }

    #[test]
    fn modifier_only_chords_need_two_modifiers() {
        let ctrl_alt = chord("Ctrl+Alt").unwrap();
        assert!(ctrl_alt.is_modifier_only());
        assert_eq!(ctrl_alt.modifiers, Modifiers::CTRL | Modifiers::ALT);
        assert!(chord("Ctrl+Win").unwrap().is_modifier_only());
        assert!(is_invalid("Ctrl"));
        assert!(is_invalid("Shift"));
    }

    #[test]
    fn malformed_combinations_are_invalid() {
        for text in [
            "",
            "   ",
            "Ctrl+Space+Alt",
            "Hyper+Q",
            "Ctrl++Q",
            "Ctrl+Ctrl+Q",
            "Ctrl+F25",
            "Ctrl+F0",
            "Ctrl+KeyAB",
            "Ctrl+Digit",
            "Q+W",
        ] {
            assert!(is_invalid(text), "{text:?}");
        }
    }

    #[test]
    fn both_sides_of_a_modifier_are_the_same_modifier() {
        assert_eq!(key_of(0xA2), KeyKind::Modifier(Modifiers::CTRL));
        assert_eq!(key_of(0xA3), KeyKind::Modifier(Modifiers::CTRL));
        assert_eq!(key_of(0xA4), KeyKind::Modifier(Modifiers::ALT));
        assert_eq!(key_of(0xA5), KeyKind::Modifier(Modifiers::ALT));
        assert_eq!(key_of(0xA0), KeyKind::Modifier(Modifiers::SHIFT));
        assert_eq!(key_of(0x5C), KeyKind::Modifier(Modifiers::WIN));
        assert_eq!(key_of(0x20), KeyKind::Main);
        assert_eq!(key_of(0xE8), KeyKind::Main);
    }

    #[test]
    fn modifier_sets_combine_and_subtract() {
        let chord = Modifiers::CTRL | Modifiers::ALT;
        assert!(chord.contains(Modifiers::CTRL));
        assert!(!chord.contains(Modifiers::SHIFT));
        assert_eq!(chord.count(), 2);
        assert_eq!(chord - Modifiers::ALT, Modifiers::CTRL);
        assert_eq!(chord - chord, Modifiers::NONE);
        assert!(chord.opens_menus());
        assert!(!(Modifiers::CTRL | Modifiers::SHIFT).opens_menus());
    }
}
