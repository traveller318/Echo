/*!
 * SOURCE OF TRUTH KEYWORDS: set_current_thread_priority, SetThreadPriority, GetCurrentThread, THREAD_PRIORITY, thread priority helper, time-critical hook thread
 * WHAT:  `set_current_thread_priority(priority)`: sets the Win32 scheduling priority of the calling thread.
 * WHY:   More than one adapter raises a thread it owns (the worker scheduler for pipeline workers, the keyboard hook
 *        thread, which must answer every key within `LowLevelHooksTimeout` while ONNX inference saturates the cores),
 *        so the one `unsafe` call is reviewed once. The pseudo-handle from GetCurrentThread is always valid on the
 *        calling thread and is never closed.
 * WHERE: adapters/scheduler/win32.rs (Win32WorkerScheduler) and adapters/hotkey/low_level_hook/hook_thread.rs.
 */

use windows::{
    Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY},
    core::Result,
};

/// Sets the calling thread's scheduling priority.
pub fn set_current_thread_priority(priority: THREAD_PRIORITY) -> Result<()> {
    // SAFETY: GetCurrentThread returns a pseudo-handle that is always valid on the calling thread and is never
    // closed; SetThreadPriority reads only its two arguments.
    unsafe { SetThreadPriority(GetCurrentThread(), priority) }
}
