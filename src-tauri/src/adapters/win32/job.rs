/*!
 * SOURCE OF TRUTH KEYWORDS: KillOnCloseJob, Job Object, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, AssignProcessToJobObject, TerminateJobObject, sidecar lifetime, orphan child process
 * WHAT:  KillOnCloseJob: a Windows Job Object whose processes are killed when its last handle closes; `assign` puts
 *        a child process in it, `terminate` kills every process in it now.
 * WHY:   05 A13: a sidecar (llama-server) must never outlive Echo. Windows closes every handle of a process that
 *        exits, crashes or is killed from Task Manager, so a child in a kill-on-close job dies with Echo however
 *        Echo ends; no shutdown hook is needed or trusted. `terminate` stops a child from any thread without owning
 *        its `Child` (the supervisor's waiter thread holds that). The child runs for a few microseconds before it
 *        is assigned (std cannot create it suspended); an Echo crash inside that window is the only way to orphan
 *        it.
 * WHERE: adapters/polish/llama_server (the sidecar supervisor), one job per sidecar.
 */

use std::{ffi::c_void, os::windows::io::AsRawHandle, process::Child};

use windows::{
    Win32::{
        Foundation::HANDLE,
        System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject, TerminateJobObject,
        },
    },
    core::PCWSTR,
};

use super::OwnedHandle;
use crate::types::{AppError, PortError, PortResult};

/// Exit code of processes the job kills (shows up in the supervisor's log).
const KILLED_EXIT_CODE: u32 = 1;

/// A job that kills its processes when it is closed.
#[derive(Debug)]
pub struct KillOnCloseJob {
    handle: OwnedHandle,
}

impl KillOnCloseJob {
    pub fn new() -> PortResult<Self> {
        // SAFETY: no security attributes and no name; the returned handle is owned below.
        let job = unsafe { CreateJobObjectW(None, PCWSTR::null()) }
            .map_err(|error| failure("create a job object", &error))?;
        let handle = OwnedHandle::new(job);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let size = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
            .map_err(|_| PortError::new(AppError::Internal).with_detail("job limits too large"))?;
        // SAFETY: the pointer and size describe `limits`, which outlives the call; the job handle is open.
        unsafe {
            SetInformationJobObject(
                handle.raw(),
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast::<c_void>(),
                size,
            )
        }
        .map_err(|error| failure("make the job kill its processes on close", &error))?;
        Ok(Self { handle })
    }

    /// Puts `child` in the job, so it dies when the job closes.
    pub fn assign(&self, child: &Child) -> PortResult<()> {
        let process = HANDLE(child.as_raw_handle());
        // SAFETY: both handles are open for the call: the job by self, the process by `child`.
        unsafe { AssignProcessToJobObject(self.handle.raw(), process) }
            .map_err(|error| failure("put the sidecar in its job", &error))
    }

    /// Kills every process in the job now; an empty job is fine.
    pub fn terminate(&self) -> PortResult<()> {
        // SAFETY: the job handle is open while self lives.
        unsafe { TerminateJobObject(self.handle.raw(), KILLED_EXIT_CODE) }
            .map_err(|error| failure("stop the sidecar", &error))
    }
}

fn failure(action: &str, error: &windows::core::Error) -> PortError {
    PortError::new(AppError::Internal).with_detail(format!("Windows could not {action}: {error}"))
}

#[cfg(test)]
mod tests {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };

    use super::*;

    /// A child that would run for a minute on its own (ping waits one second per echo request).
    fn long_child() -> Child {
        Command::new("ping")
            .args(["-n", "60", "127.0.0.1"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    }

    fn exits_within(child: &mut Child, limit: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < limit {
            if child.try_wait().unwrap().is_some() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn closing_the_job_kills_its_child() {
        let job = KillOnCloseJob::new().unwrap();
        let mut child = long_child();
        job.assign(&child).unwrap();
        drop(job);
        assert!(exits_within(&mut child, Duration::from_secs(5)));
    }

    #[test]
    fn terminate_kills_the_child_while_the_job_stays_usable() {
        let job = KillOnCloseJob::new().unwrap();
        let mut first = long_child();
        job.assign(&first).unwrap();
        job.terminate().unwrap();
        assert!(exits_within(&mut first, Duration::from_secs(5)));
        assert_eq!(
            first.wait().unwrap().code(),
            Some(i32::try_from(KILLED_EXIT_CODE).unwrap())
        );
        let mut second = long_child();
        job.assign(&second).unwrap();
        drop(job);
        assert!(exits_within(&mut second, Duration::from_secs(5)));
    }
}
