/*!
 * SOURCE OF TRUTH KEYWORDS: hotkey adapters, HotkeyService implementations, TauriGlobalShortcut, global shortcut
 * WHAT:  Adapters behind the HotkeyService port.
 * WHY:   System-wide hotkeys are a Windows integration and stay behind their port (root CLAUDE.md §3); a
 *        low-level keyboard hook adapter for hold mode (05 W9) would sit next to this one, picked by its caps.
 * WHERE: Constructed by app/bootstrap; used only through `dyn HotkeyService` by pipeline/hotkeys.rs.
 */

mod tauri_global_shortcut;

pub use tauri_global_shortcut::TauriGlobalShortcut;
