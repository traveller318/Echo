/*!
 * SOURCE OF TRUTH KEYWORDS: foreground adapters, ForegroundApp implementations, Win32ForegroundApp, ForegroundTracker, focused window, last external app
 * WHAT:  Adapters behind the ForegroundApp port: Win32ForegroundApp and the foreground hook it keeps
 *        (ForegroundTracker) for the last app outside Echo.
 * WHY:   Reading the focused window, its process and its integrity level are Windows API calls and stay behind
 *        their port (root CLAUDE.md §3).
 * WHERE: Constructed by app/bootstrap; used only through `dyn ForegroundApp`.
 */

#[cfg(windows)]
mod tracker;
#[cfg(target_os = "linux")]
mod unknown_focus;
#[cfg(windows)]
mod win32;
#[cfg(target_os = "linux")]
mod x11;

#[cfg(target_os = "linux")]
pub use unknown_focus::UnknownFocusForeground;
#[cfg(windows)]
pub use win32::Win32ForegroundApp;
#[cfg(target_os = "linux")]
pub use x11::X11ForegroundApp;
