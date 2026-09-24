/*!
 * SOURCE OF TRUTH KEYWORDS: WorkerScheduler, thread priority port, prioritize current thread, capture worker priority, keep_full_speed, power throttling, EcoQoS
 * WHAT:  WorkerScheduler sets the operating-system priority of the calling thread to a WorkerPriority class, and
 *        asks the operating system to run the whole process at full speed (no power throttling).
 * WHY:   The capture worker runs above normal and the ASR thread at normal priority (05 A9); a thread priority is
 *        a Windows call, which belongs behind a port (root CLAUDE.md §3), and the pipeline must stay testable
 *        without touching real thread priorities. It acts on the *calling* thread because the pipeline spawns its
 *        own workers with std threads and each one applies its class as its first step. A failure is reported,
 *        and the worker keeps running at the default priority. `keep_full_speed` is process-wide because ONNX
 *        Runtime creates its own inference threads, which a per-thread call never reaches: Windows 11 otherwise
 *        applies EcoQoS to a process whose windows are not in front, which is exactly when Echo transcribes, and
 *        that made inference about five times slower on the dev box (05 W35). It costs nothing while Echo idles,
 *        because Echo runs no work then (02 §6.2).
 * WHERE: Implemented by adapters/scheduler (Win32WorkerScheduler) and ports/fakes; `prioritize_current_thread` is
 *        called at the start of the capture worker (pipeline/capture) and the ASR worker and loader (pipeline/asr);
 *        `keep_full_speed` once by app/bootstrap.
 */

use crate::types::{PortResult, WorkerPriority};

/// Sets the scheduling priority of pipeline threads.
pub trait WorkerScheduler: Send + Sync {
    /// Applies `priority` to the thread that calls it.
    fn prioritize_current_thread(&self, priority: WorkerPriority) -> PortResult<()>;

    /// Opts the whole process out of the operating system's power throttling, so work Echo does runs at full speed
    /// even while its windows are in the background.
    fn keep_full_speed(&self) -> PortResult<()>;
}
