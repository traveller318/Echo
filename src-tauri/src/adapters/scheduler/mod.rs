/*!
 * SOURCE OF TRUTH KEYWORDS: scheduler adapters, WorkerScheduler implementations, Win32WorkerScheduler, thread priority
 * WHAT:  Adapters behind the WorkerScheduler port.
 * WHY:   Thread priorities are an operating-system call and stay behind their port (root CLAUDE.md §3).
 * WHERE: Constructed by app/bootstrap; used only through `dyn WorkerScheduler`.
 */

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod win32;

#[cfg(target_os = "linux")]
pub use linux::LinuxWorkerScheduler;
#[cfg(windows)]
pub use win32::Win32WorkerScheduler;
