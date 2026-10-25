/*!
 * SOURCE OF TRUTH KEYWORDS: WakeFd, eventfd, stop a poll loop, wake a blocked thread, shutdown signal Linux
 * WHAT:  WakeFd: an eventfd a controlling thread signals to wake a worker blocked in poll(2) next to another file
 *        descriptor (the X server connection); the worker returns when `readable_in` reports the wake fd.
 * WHY:   Event threads wait on a socket, so a flag alone cannot stop them; the wake fd lets `Drop` end and join the
 *        thread at once without a timeout loop. It is one small type so the X11 key capture and the X11 window
 *        tracker stop the same way.
 * WHERE: adapters/hotkey/x11_xinput.rs and adapters/foreground/x11.rs.
 */

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

pub struct WakeFd(OwnedFd);

impl WakeFd {
    pub fn new() -> std::io::Result<Self> {
        // SAFETY: eventfd creates a new descriptor, owned from here on.
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: `fd` was just created and nothing else owns it.
        Ok(Self(unsafe { OwnedFd::from_raw_fd(fd) }))
    }

    pub fn fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }

    /// Wakes the thread waiting on this fd (once; the fd stays readable).
    pub fn signal(&self) {
        let one = 1_u64.to_ne_bytes();
        // SAFETY: writes 8 bytes from a live buffer to the eventfd this struct owns.
        unsafe { libc::write(self.0.as_raw_fd(), one.as_ptr().cast(), one.len()) };
    }

    /// Waits until `fd` has data, this wake fd was signalled, or `timeout_ms` passed (-1 = no limit).
    pub fn wait(&self, fd: RawFd, timeout_ms: i32) -> Wake {
        let mut fds = [
            libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: self.0.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        // SAFETY: `fds` is a live array of two pollfd structs and the length passed is 2.
        let ready = unsafe { libc::poll(fds.as_mut_ptr(), 2, timeout_ms) };
        if ready < 0 {
            return if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                Wake::Timeout
            } else {
                Wake::Failed
            };
        }
        if fds[1].revents != 0 {
            Wake::Stop
        } else if fds[0].revents != 0 {
            Wake::Data
        } else {
            Wake::Timeout
        }
    }
}

/// Why `WakeFd::wait` returned.
#[derive(Debug, PartialEq, Eq)]
pub enum Wake {
    Data,
    Stop,
    Timeout,
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_signal_wakes_the_wait_and_an_idle_fd_times_out() {
        let wake = WakeFd::new().unwrap();
        let other = WakeFd::new().unwrap();
        assert_eq!(wake.wait(other.fd(), 10), Wake::Timeout);
        other.signal();
        assert_eq!(wake.wait(other.fd(), 10), Wake::Data);
        wake.signal();
        assert_eq!(wake.wait(other.fd(), 10), Wake::Stop);
    }
}
