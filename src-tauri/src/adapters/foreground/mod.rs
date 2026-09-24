/*!
 * SOURCE OF TRUTH KEYWORDS: foreground adapters, ForegroundApp implementations, Win32ForegroundApp, focused window
 * WHAT:  Adapters behind the ForegroundApp port.
 * WHY:   Reading the focused window, its process and its integrity level are Windows API calls and stay behind
 *        their port (root CLAUDE.md §3).
 * WHERE: Constructed by app/bootstrap; used only through `dyn ForegroundApp`.
 */

mod win32;

pub use win32::Win32ForegroundApp;
