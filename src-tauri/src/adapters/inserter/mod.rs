/*!
 * SOURCE OF TRUTH KEYWORDS: inserter adapters, TextInserter implementations, Win32SendInputInserter, paste adapter
 * WHAT:  Adapters behind the TextInserter port.
 * WHY:   Synthetic keyboard input is a Windows API concern and stays behind its port (root CLAUDE.md §3); a
 *        typing or UI Automation inserter would be a second adapter here, picked by its caps.
 * WHERE: Constructed by app/bootstrap; used only through `dyn TextInserter` by pipeline/delivery.rs.
 */

#[cfg(target_os = "linux")]
mod clipboard_only;
#[cfg(target_os = "linux")]
mod portal_remote_desktop;
#[cfg(target_os = "linux")]
mod wayland_virtual_keyboard;
#[cfg(windows)]
mod win32_send_input;
#[cfg(target_os = "linux")]
mod x11_xtest;

#[cfg(target_os = "linux")]
pub use clipboard_only::ClipboardOnlyInserter;
#[cfg(target_os = "linux")]
pub use portal_remote_desktop::PortalRemoteDesktopInserter;
#[cfg(target_os = "linux")]
pub use wayland_virtual_keyboard::WaylandVirtualKeyboardInserter;
#[cfg(windows)]
pub use win32_send_input::Win32SendInputInserter;
#[cfg(target_os = "linux")]
pub use x11_xtest::X11XTestInserter;
