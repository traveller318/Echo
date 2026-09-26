/*!
 * SOURCE OF TRUTH KEYWORDS: win32 helpers, OwnedHandle, KillOnCloseJob, ComScope, CloseHandle guard, hwnd conversion, WindowHandle to HWND, synthetic keyboard input, shared Win32 adapter code
 * WHAT:  Small Win32 building blocks shared by several adapters: OwnedHandle (a kernel handle closed on drop),
 *        KillOnCloseJob (a Job Object that kills its child processes when Echo ends),
 *        ComScope (COM entered and left on the calling thread), the
 *        WindowHandle ⇄ HWND conversion and the synthetic-keyboard helpers (SYNTHETIC_INPUT_TAG, the menu mask key,
 *        one SendInput batch, the asynchronous key state).
 * WHY:   Every Win32 adapter needs the same RAII close, the same handle conversion and the same tagged SendInput;
 *        one copy keeps each `unsafe` block reviewed once (root CLAUDE.md §1). Nothing here is a port
 *        implementation, and nothing outside adapters/ may use it, since Windows API calls stay behind ports
 *        (root CLAUDE.md §3).
 * WHERE: adapters/appearance (events), adapters/foreground (process and token handles, window handles),
 *        adapters/audio (COM for Core Audio), adapters/inserter (window handles, the paste strokes), adapters/hotkey (the hook skips tagged events and
 *        taps the mask key), adapters/overlay (window handles), adapters/polish/llama_server (the sidecar's job).
 */

mod com;
mod handle;
mod job;
mod keyboard;
mod window;

pub use com::ComScope;
pub use handle::OwnedHandle;
pub use job::KillOnCloseJob;
pub use keyboard::{
    KeyStroke, MASK_TAP, MENU_MASK_KEY, SYNTHETIC_INPUT_TAG, is_key_down, keyboard_input,
    send_strokes,
};
pub use window::{hwnd, window_handle};
