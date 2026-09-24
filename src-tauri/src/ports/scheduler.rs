/*!
 * SOURCE OF TRUTH KEYWORDS: WorkerScheduler, thread priority port, prioritize current thread, capture worker priority, SetThreadPriority
 * WHAT:  WorkerScheduler sets the operating-system priority of the calling thread to a WorkerPriority class.
 * WHY:   The capture worker runs above normal and the ASR thread at normal priority (05 A9); a thread priority is
 *        a Windows call, which belongs behind a port (root CLAUDE.md §3), and the pipeline must stay testable
 *        without touching real thread priorities. It acts on the *calling* thread because the pipeline spawns its
 *        own workers with std threads and each one applies its class as its first step. A failure is reported,
 *        and the worker keeps running at the default priority.
 * WHERE: Implemented by adapters/scheduler (Win32WorkerScheduler) and ports/fakes; called at the start of the
 *        capture worker (pipeline/capture) and the ASR worker.
 */

use crate::types::{PortResult, WorkerPriority};

/// Sets the scheduling priority of pipeline threads.
pub trait WorkerScheduler: Send + Sync {
    /// Applies `priority` to the thread that calls it.
    fn prioritize_current_thread(&self, priority: WorkerPriority) -> PortResult<()>;
}
