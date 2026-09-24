/*!
 * SOURCE OF TRUTH KEYWORDS: hotkey adapters, HotkeyService implementations, LowLevelKeyboardHotkeys, keyboard hook, global hotkey
 * WHAT:  Adapters behind the HotkeyService port.
 * WHY:   System-wide hotkeys are a Windows integration and stay behind their port (root CLAUDE.md §3). The
 *        low-level keyboard hook is the one adapter: it reports real key-ups (hold-to-talk) and binds modifier-only
 *        combinations such as Ctrl+Alt, which RegisterHotKey cannot (05 W9, decision log 2026-09-25). Another backend
 *        would sit next to it and be picked by its caps.
 * WHERE: Constructed by app/bootstrap; used only through `dyn HotkeyService` by pipeline/hotkeys.rs and the session
 *        actor.
 */

mod low_level_hook;

pub use low_level_hook::LowLevelKeyboardHotkeys;
