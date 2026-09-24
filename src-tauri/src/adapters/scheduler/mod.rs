/*!
 * SOURCE OF TRUTH KEYWORDS: scheduler adapters, WorkerScheduler implementations, Win32WorkerScheduler, thread priority
 * WHAT:  Adapters behind the WorkerScheduler port.
 * WHY:   Thread priorities are an operating-system call and stay behind their port (root CLAUDE.md §3).
 * WHERE: Constructed by app/bootstrap; used only through `dyn WorkerScheduler`.
 */

mod win32;

pub use win32::Win32WorkerScheduler;
