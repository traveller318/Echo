/*!
 * SOURCE OF TRUTH KEYWORDS: Win32WorkerScheduler, SetThreadPriority, THREAD_PRIORITY_ABOVE_NORMAL, capture worker priority, GetCurrentThread, SetProcessInformation, ProcessPowerThrottling, EcoQoS
 * WHAT:  Win32WorkerScheduler: WorkerScheduler on `SetThreadPriority` for the calling thread, and on
 *        `SetProcessInformation(ProcessPowerThrottling)` with the execution-speed bit controlled and cleared for the
 *        whole process.
 * WHY:   The capture worker must keep up with the microphone while ONNX inference saturates other cores, so it runs
 *        one step above normal; the ASR thread stays normal (05 A9). Above normal (not time-critical) is enough for
 *        a thread that drains a 2 s ring every 10 ms, and it cannot starve the UI or the device callback thread
 *        (which Windows audio already runs at its own elevated priority); the call itself is the shared
 *        `set_current_thread_priority` (adapters/win32). Clearing PROCESS_POWER_THROTTLING_EXECUTION_SPEED in
 *        StateMask while setting it in ControlMask is Microsoft's documented way to turn EcoQoS off for a process
 *        (05 W35); the pseudo-handle from GetCurrentProcess needs no closing.
 * WHERE: Built by app/bootstrap into CommandCtx (and the session actor later); used through `dyn WorkerScheduler`
 *        by pipeline/capture and the ASR worker.
 */

use windows::Win32::System::Threading::{
    GetCurrentProcess, PROCESS_POWER_THROTTLING_CURRENT_VERSION,
    PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_STATE,
    ProcessPowerThrottling, SetProcessInformation, THREAD_PRIORITY, THREAD_PRIORITY_ABOVE_NORMAL,
    THREAD_PRIORITY_BELOW_NORMAL, THREAD_PRIORITY_NORMAL,
};

use crate::{
    adapters::win32::set_current_thread_priority,
    ports::WorkerScheduler,
    types::{AppError, PortError, PortResult, WorkerPriority},
};

/// Thread priorities through the Win32 scheduler.
#[derive(Debug, Default)]
pub struct Win32WorkerScheduler;

impl Win32WorkerScheduler {
    pub fn new() -> Self {
        Self
    }
}

impl WorkerScheduler for Win32WorkerScheduler {
    fn prioritize_current_thread(&self, priority: WorkerPriority) -> PortResult<()> {
        set_current_thread_priority(win32_priority(priority)).map_err(|error| {
            PortError::new(AppError::Internal)
                .with_detail(format!("SetThreadPriority({priority:?}) failed: {error}"))
        })
    }

    fn keep_full_speed(&self) -> PortResult<()> {
        // The struct is three u32s, so its size always fits the u32 the API takes.
        let size = u32::try_from(size_of::<PROCESS_POWER_THROTTLING_STATE>()).unwrap_or(u32::MAX);
        // SAFETY: GetCurrentProcess returns a pseudo-handle that is always valid and never closed; the pointer and
        // size describe FULL_SPEED, a constant that outlives the call and is only read.
        unsafe {
            SetProcessInformation(
                GetCurrentProcess(),
                ProcessPowerThrottling,
                std::ptr::from_ref(&FULL_SPEED).cast(),
                size,
            )
        }
        .map_err(|error| {
            PortError::new(AppError::Internal).with_detail(format!(
                "SetProcessInformation(ProcessPowerThrottling) failed: {error}"
            ))
        })
    }
}

/// The power-throttling state that turns EcoQoS off for the process: execution speed controlled, not throttled.
const FULL_SPEED: PROCESS_POWER_THROTTLING_STATE = PROCESS_POWER_THROTTLING_STATE {
    Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
    ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
    StateMask: 0,
};

fn win32_priority(priority: WorkerPriority) -> THREAD_PRIORITY {
    match priority {
        WorkerPriority::BelowNormal => THREAD_PRIORITY_BELOW_NORMAL,
        WorkerPriority::Normal => THREAD_PRIORITY_NORMAL,
        WorkerPriority::AboveNormal => THREAD_PRIORITY_ABOVE_NORMAL,
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use windows::Win32::System::Threading::{GetCurrentThread, GetThreadPriority};

    use super::*;

    fn priority_after(priority: WorkerPriority) -> i32 {
        thread::spawn(move || {
            Win32WorkerScheduler::new()
                .prioritize_current_thread(priority)
                .unwrap();
            // SAFETY: GetCurrentThread's pseudo-handle is always valid on the calling thread; GetThreadPriority only
            // reads that thread's priority.
            unsafe { GetThreadPriority(GetCurrentThread()) }
        })
        .join()
        .unwrap()
    }

    #[test]
    fn the_process_can_opt_out_of_power_throttling() {
        Win32WorkerScheduler::new().keep_full_speed().unwrap();
    }

    #[test]
    fn each_class_sets_its_windows_priority_on_the_calling_thread() {
        assert_eq!(
            priority_after(WorkerPriority::AboveNormal),
            THREAD_PRIORITY_ABOVE_NORMAL.0
        );
        assert_eq!(
            priority_after(WorkerPriority::Normal),
            THREAD_PRIORITY_NORMAL.0
        );
        assert_eq!(
            priority_after(WorkerPriority::BelowNormal),
            THREAD_PRIORITY_BELOW_NORMAL.0
        );
    }
}
