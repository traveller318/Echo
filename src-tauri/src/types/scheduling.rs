/*!
 * SOURCE OF TRUTH KEYWORDS: WorkerPriority, thread priority, capture worker priority, ASR thread priority, above normal
 * WHAT:  WorkerPriority: how urgently the operating system should run one of Echo's own pipeline threads.
 * WHY:   ONNX inference threads compete with audio and UI for the CPU, so the capture worker runs above normal
 *        and the ASR thread at normal priority (05 A9); the device callback thread is never touched. Setting a
 *        thread's priority is an operating-system call, so the pipeline names the class it wants and the
 *        WorkerScheduler adapter maps it to the Windows value. Only classes the pipeline uses exist, so the adapter's
 *        mapping stays exhaustive.
 * WHERE: ports/scheduler.rs (WorkerScheduler); pipeline/capture (AboveNormal); the ASR worker (Normal).
 */

/// Scheduling class of a pipeline worker thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkerPriority {
    /// Background work that should not disturb the audio path (ASR inference).
    Normal,
    /// Work that must keep up with the microphone (the capture worker).
    AboveNormal,
}
