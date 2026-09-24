/*!
 * SOURCE OF TRUTH KEYWORDS: win32 helpers, OwnedHandle, CloseHandle guard, hwnd conversion, WindowHandle to HWND, synthetic keyboard input, shared Win32 adapter code
 * WHAT:  Small Win32 building blocks shared by several adapters: OwnedHandle (a kernel handle closed on drop), the
 *        WindowHandle ⇄ HWND conversion and the synthetic-keyboard helpers (SYNTHETIC_INPUT_TAG, the menu mask key,
 *        one SendInput batch, the asynchronous key state).
 * WHY:   Every Win32 adapter needs the same RAII close, the same handle conversion and the same tagged SendInput;
 *        one copy keeps each `unsafe` block reviewed once (root CLAUDE.md §1). Nothing here is a port
 *        implementation, and nothing outside adapters/ may use it, since Windows API calls stay behind ports
 *        (root CLAUDE.md §3).
 * WHERE: adapters/appearance (events), adapters/foreground (process and token handles, window handles),
 *        adapters/inserter (window handles, the paste strokes), adapters/hotkey (the hook skips tagged events and
 *        taps the mask key), adapters/overlay (window handles).
 */

mod handle;
mod keyboard;
mod window;

pub use handle::OwnedHandle;
pub use keyboard::{
    KeyStroke, MASK_TAP, MENU_MASK_KEY, SYNTHETIC_INPUT_TAG, is_key_down, keyboard_input,
    send_strokes,
};
pub use window::{hwnd, window_handle};
