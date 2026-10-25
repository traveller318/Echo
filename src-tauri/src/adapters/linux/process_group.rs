/*!
 * SOURCE OF TRUTH KEYWORDS: KillOnCloseGroup, process group kill, PR_SET_PDEATHSIG, sidecar dies with Echo, killpg
 * WHAT:  KillOnCloseGroup: the Linux counterpart of the Windows kill-on-close Job Object. `prepare` makes a command
 *        start in its own process group and ask the kernel to SIGKILL it when Echo's thread that started it ends;
 *        `assign` records the child; `terminate` (and dropping the group) SIGKILLs the whole group.
 * WHY:   The grammar server must never outlive Echo however Echo ends (a crash, SIGKILL, a power-off logout). A
 *        process group lets `terminate` take the server's own children with it, and PR_SET_PDEATHSIG covers a dead
 *        Echo, which no Drop can: the kernel signals the child when its parent thread exits. The signal follows
 *        the *thread* that spawned the child, so `prepare` is used from the supervisor's long-lived thread; a
 *        spawn from a short-lived thread would kill the server early. `terminate` signals the group id captured at
 *        `assign`; the supervisor is the only reaper and calls it for a run it still owns, so the pid is not yet
 *        recycled. An empty group (already exited) is not an error.
 * WHERE: adapters/polish/llama_server/sidecar.rs on Linux.
 */

use std::{
    os::unix::process::CommandExt,
    process::{Child, Command},
    sync::atomic::{AtomicI32, Ordering},
};

use crate::types::{AppError, PortError, PortResult};

/// No process assigned yet.
const NONE: i32 = 0;

#[derive(Debug, Default)]
pub struct KillOnCloseGroup {
    group: AtomicI32,
}

impl KillOnCloseGroup {
    pub fn new() -> PortResult<Self> {
        Ok(Self::default())
    }

    /// Makes `command` start in a new process group that dies with the thread that spawns it.
    pub fn prepare(command: &mut Command) {
        command.process_group(0);
        // SAFETY: the closure runs between fork and exec and calls only prctl, which is async-signal-safe and
        // touches no memory of the parent.
        unsafe {
            command.pre_exec(|| {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    /// Remembers `child` (the leader of its own group, see `prepare`) as the group to kill.
    pub fn assign(&self, child: &Child) -> PortResult<()> {
        let pid = i32::try_from(child.id()).map_err(|_| {
            PortError::new(AppError::Internal)
                .with_detail("the sidecar's process id is out of range")
        })?;
        self.group.store(pid, Ordering::Release);
        Ok(())
    }

    /// SIGKILLs every process in the group now; a group that is already gone is fine.
    pub fn terminate(&self) -> PortResult<()> {
        let group = self.group.swap(NONE, Ordering::AcqRel);
        if group == NONE {
            return Ok(());
        }
        // SAFETY: killpg only sends a signal to the group id recorded for our own child.
        if unsafe { libc::killpg(group, libc::SIGKILL) } == -1 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(PortError::new(AppError::Internal)
                    .with_detail(format!("could not stop the sidecar: {error}")));
            }
        }
        Ok(())
    }
}

impl Drop for KillOnCloseGroup {
    fn drop(&mut self) {
        drop(self.terminate());
    }
}

#[cfg(test)]
mod tests {
    use std::{
        process::Stdio,
        time::{Duration, Instant},
    };

    use super::*;

    #[test]
    fn terminate_kills_the_child_and_its_children() {
        let group = KillOnCloseGroup::new().unwrap();
        let mut command = Command::new("sh");
        command
            .args(["-c", "sleep 60 & wait"])
            .stdin(Stdio::null())
            .stdout(Stdio::null());
        KillOnCloseGroup::prepare(&mut command);
        let mut child = command.spawn().unwrap();
        group.assign(&child).unwrap();
        group.terminate().unwrap();
        let started = Instant::now();
        while child.try_wait().unwrap().is_none() {
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "child still alive"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        // A second terminate and a drop are no-ops.
        group.terminate().unwrap();
    }
}
