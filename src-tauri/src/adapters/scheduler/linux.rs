/*!
 * SOURCE OF TRUTH KEYWORDS: LinuxWorkerScheduler, setpriority, thread nice, RLIMIT_NICE, best-effort priority
 * WHAT:  LinuxWorkerScheduler: WorkerScheduler over per-thread nice values (`setpriority` on the thread id):
 *        BelowNormal +5, Normal 0, AboveNormal −5. `keep_full_speed` has nothing to do.
 * WHY:   Linux schedules each thread by its own nice value, so the pipeline's priorities map directly. Raising a
 *        priority (a lower nice) needs CAP_SYS_NICE or an RLIMIT_NICE allowance most desktops do not grant, so a
 *        refused raise is logged at debug and treated as done: the thread simply runs at the default, which is what
 *        it would get anyway. Linux has no per-process power throttling of background apps to opt out of.
 * WHERE: Built by app/bootstrap on Linux; called by the ASR worker, capture and the session threads.
 */

use crate::{
    ports::WorkerScheduler,
    types::{AppError, PortError, PortResult, WorkerPriority},
};

#[derive(Debug, Default)]
pub struct LinuxWorkerScheduler;

impl LinuxWorkerScheduler {
    pub const fn new() -> Self {
        Self
    }
}

impl WorkerScheduler for LinuxWorkerScheduler {
    fn prioritize_current_thread(&self, priority: WorkerPriority) -> PortResult<()> {
        // SAFETY: gettid has no preconditions.
        let tid = unsafe { libc::gettid() };
        let id = libc::id_t::try_from(tid).unwrap_or_default();
        // SAFETY: setpriority with PRIO_PROCESS and a thread id changes only that thread's nice value.
        if unsafe { libc::setpriority(libc::PRIO_PROCESS, id, nice(priority)) } == 0 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        match error.raw_os_error() {
            Some(libc::EACCES | libc::EPERM) => {
                tracing::debug!(?priority, "thread priority not raised: not permitted");
                Ok(())
            }
            _ => Err(PortError::new(AppError::Internal)
                .with_detail(format!("setpriority({priority:?}) failed: {error}"))),
        }
    }

    fn keep_full_speed(&self) -> PortResult<()> {
        Ok(())
    }
}

const fn nice(priority: WorkerPriority) -> libc::c_int {
    match priority {
        WorkerPriority::BelowNormal => 5,
        WorkerPriority::Normal => 0,
        WorkerPriority::AboveNormal => -5,
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;

    #[test]
    fn lowering_a_thread_works_and_raising_never_fails() {
        thread::spawn(|| {
            let scheduler = LinuxWorkerScheduler::new();
            scheduler
                .prioritize_current_thread(WorkerPriority::BelowNormal)
                .unwrap();
            // SAFETY: reads this thread's nice value.
            let tid = libc::id_t::try_from(unsafe { libc::gettid() }).unwrap();
            assert_eq!(unsafe { libc::getpriority(libc::PRIO_PROCESS, tid) }, 5);
            scheduler
                .prioritize_current_thread(WorkerPriority::AboveNormal)
                .unwrap();
        })
        .join()
        .unwrap();
    }
}
