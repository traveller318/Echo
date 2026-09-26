/*!
 * SOURCE OF TRUTH KEYWORDS: power adapters, PowerEvents implementation, Win32PowerEvents, sleep wake adapter
 * WHAT:  The adapter behind the PowerEvents port.
 * WHY:   Sleep, wake, session and shell notifications are Windows messages; they stay here like every other OS
 *        integration (root CLAUDE.md §3).
 * WHERE: Constructed by app/bootstrap; used only through `dyn PowerEvents`.
 */

mod win32;

pub use win32::Win32PowerEvents;
