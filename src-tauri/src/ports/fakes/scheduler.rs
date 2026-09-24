/*!
 * SOURCE OF TRUTH KEYWORDS: FakeWorkerScheduler, fake thread priority, recorded priorities, worker thread test, full speed requests
 * WHAT:  FakeWorkerScheduler: a WorkerScheduler that records every requested priority with the name of the thread
 *        that asked and counts full-speed requests, and can fail the next priority request.
 * WHY:   Pipeline tests prove each worker asks for its class from its own thread (05 A9) and keeps running when
 *        the request fails, without changing real thread priorities.
 * WHERE: pipeline/capture tests and the ipc command harness.
 */

use std::{sync::Mutex, thread};

use super::lock;
use crate::{
    ports::WorkerScheduler,
    types::{PortError, PortResult, WorkerPriority},
};

#[derive(Default)]
struct SchedulerState {
    requests: Vec<(Option<String>, WorkerPriority)>,
    next_error: Option<PortError>,
    full_speed: usize,
}

/// A recording thread-priority setter.
#[derive(Default)]
pub struct FakeWorkerScheduler {
    state: Mutex<SchedulerState>,
}

impl FakeWorkerScheduler {
    /// Every request so far: the requesting thread's name and the class it asked for.
    pub fn requests(&self) -> Vec<(Option<String>, WorkerPriority)> {
        lock(&self.state).requests.clone()
    }

    pub fn fail_next(&self, error: PortError) {
        lock(&self.state).next_error = Some(error);
    }

    /// How many times the process was asked to run at full speed.
    pub fn full_speed_requests(&self) -> usize {
        lock(&self.state).full_speed
    }
}

impl WorkerScheduler for FakeWorkerScheduler {
    fn prioritize_current_thread(&self, priority: WorkerPriority) -> PortResult<()> {
        let mut state = lock(&self.state);
        if let Some(error) = state.next_error.take() {
            return Err(error);
        }
        let name = thread::current().name().map(str::to_owned);
        state.requests.push((name, priority));
        Ok(())
    }

    fn keep_full_speed(&self) -> PortResult<()> {
        lock(&self.state).full_speed += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AppError;

    #[test]
    fn records_requests_and_fails_once_when_told() {
        let scheduler = FakeWorkerScheduler::default();
        scheduler.fail_next(AppError::Internal.into());
        assert!(
            scheduler
                .prioritize_current_thread(WorkerPriority::AboveNormal)
                .is_err()
        );
        scheduler
            .prioritize_current_thread(WorkerPriority::Normal)
            .unwrap();
        let requests = scheduler.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].1, WorkerPriority::Normal);
    }
}
