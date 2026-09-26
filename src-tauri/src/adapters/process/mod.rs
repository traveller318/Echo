/*!
 * SOURCE OF TRUTH KEYWORDS: process adapters, ProcessStats implementation, Win32ProcessStats, memory use
 * WHAT:  The adapter behind the ProcessStats port.
 * WHY:   Reading Echo's own memory is a Windows call; it stays here like every other OS integration.
 * WHERE: Constructed by app/bootstrap; used only through `dyn ProcessStats`.
 */

mod win32;

pub use win32::Win32ProcessStats;
