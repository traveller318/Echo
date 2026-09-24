/*!
 * SOURCE OF TRUTH KEYWORDS: launcher adapters, SystemLauncher implementations, Win32ShellLauncher, shell open
 * WHAT:  Adapters behind the SystemLauncher port.
 * WHY:   Handing folders and settings pages to the Windows shell is a Windows API concern and stays behind its port
 *        (root CLAUDE.md §3).
 * WHERE: Constructed by app/bootstrap; used only through `dyn SystemLauncher`.
 */

mod win32;

pub use win32::Win32ShellLauncher;
