/*!
 * SOURCE OF TRUTH KEYWORDS: OwnedHandle, CloseHandle, kernel handle guard, process handle, token handle, event handle
 * WHAT:  OwnedHandle: a Win32 kernel handle (event, process, token) that is closed exactly once when dropped.
 * WHY:   A handle leaked on an early `?` return is a slow resource leak in a process that runs for weeks; tying the
 *        close to Drop makes every exit path release it. Kernel handles may be used and closed from any thread,
 *        and the value is never changed after creation, so the guard is Send and Sync.
 * WHERE: adapters/appearance (stop and change events), adapters/foreground (target process and token).
 */

use windows::Win32::Foundation::{CloseHandle, HANDLE};

/// A kernel handle, closed on drop.
#[derive(Debug)]
pub struct OwnedHandle(HANDLE);

// SAFETY: a kernel handle names a kernel object; waits, queries and CloseHandle may run on any thread, and the value
// is never mutated after creation.
unsafe impl Send for OwnedHandle {}
// SAFETY: see Send; shared use only passes the value to kernel calls, which synchronise themselves.
unsafe impl Sync for OwnedHandle {}

impl OwnedHandle {
    /// Takes ownership of `handle`, which must be open and closed by nobody else.
    pub const fn new(handle: HANDLE) -> Self {
        Self(handle)
    }

    /// The raw handle, valid while self lives.
    pub const fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: the handle was handed over open by `new` and is closed exactly once here.
        let _ = unsafe { CloseHandle(self.0) };
    }
}
