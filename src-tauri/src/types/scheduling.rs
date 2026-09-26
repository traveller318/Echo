/*!
 * SOURCE OF TRUTH KEYWORDS: WorkerPriority, thread priority, capture worker priority, ASR thread priority, above normal, below normal, background GPU measurement
 * WHAT:  WorkerPriority: how urgently the operating system should run one of Echo's own pipeline threads.
 * WHY:   ONNX inference threads compete with audio and UI for the CPU, so the capture worker runs above normal
 *        and the ASR thread at normal priority (05 A9); the background GPU measurement of `auto` runs below normal,
 *        because it can keep a core busy for a minute (DirectML compiling, then tearing down its session, 05 W43)
 *        while the user is already dictating on the CPU; the device callback thread is never touched. Setting a
 *        thread's priority is an operating-system call, so the pipeline names the class it wants and the
 *        WorkerScheduler adapter maps it to the Windows value. Only classes the pipeline uses exist, so the adapter's
 *        mapping stays exhaustive.
 * WHERE: ports/scheduler.rs (WorkerScheduler); pipeline/capture (AboveNormal); the ASR worker and loader (Normal);
 *        the loader measuring the GPU in the background (BelowNormal, pipeline/asr/loader.rs).
 */

/// Scheduling class of a pipeline worker thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkerPriority {
    /// Work that must never slow a take (the background GPU measurement).
    BelowNormal,
    /// Background work that should not disturb the audio path (ASR inference).
    Normal,
    /// Work that must keep up with the microphone (the capture worker).
    AboveNormal,
}
