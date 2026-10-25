/*!
 * SOURCE OF TRUTH KEYWORDS: chord matching, ChordTracker, shortcut parsing, chord service, KeyCapture, ChordHotkeys, Dispatcher, hotkey table, shared hotkey core
 * WHAT:  The operating-system-neutral half of the hotkey adapters: `chord` (parse `Ctrl+Alt+V` into a Chord) and
 *        `tracker` (turn physical key events into press/release events for the bound chords), used by the Windows
 *        hook as well. On Linux `ChordHotkeys<C>` adds the rest of a HotkeyService: the binding table with its
 *        conflict and parse rules, the sink, and a `Dispatcher` that a capture backend (`KeyCapture`: X11 raw
 *        events, evdev) feeds one key at a time.
 * WHY:   Key identity here is the Windows virtual-key number, which the user-facing shortcut syntax and the tracker's
 *        tests already speak; a Linux backend only translates its own key codes into those numbers (adapters/linux/keymap.rs), so the
 *        press/release/interrupt semantics, the auto-repeat and stuck-key rules and the "modifier-only chord" logic
 *        are identical on every system and tested once. A backend owns only its device or connection and a thread;
 *        it never sees a binding, and it never logs or stores a key (02 §10).
 * WHERE: `chord` and `tracker` by adapters/hotkey/low_level_hook (Windows); everything here by
 *        adapters/hotkey/x11_xinput.rs and evdev.rs (Linux).
 */

pub mod chord;
#[cfg(target_os = "linux")]
mod service;
pub mod tracker;

#[cfg(target_os = "linux")]
pub use service::{ChordHotkeys, Dispatcher, KeyCapture, Shared};
