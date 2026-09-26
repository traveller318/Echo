/*!
 * SOURCE OF TRUTH KEYWORDS: foreground adapters, ForegroundApp implementations, Win32ForegroundApp, ForegroundTracker, focused window, last external app
 * WHAT:  Adapters behind the ForegroundApp port: Win32ForegroundApp and the foreground hook it keeps
 *        (ForegroundTracker) for the last app outside Echo.
 * WHY:   Reading the focused window, its process and its integrity level are Windows API calls and stay behind
 *        their port (root CLAUDE.md §3).
 * WHERE: Constructed by app/bootstrap; used only through `dyn ForegroundApp`.
 */

mod tracker;
mod win32;

pub use win32::Win32ForegroundApp;
