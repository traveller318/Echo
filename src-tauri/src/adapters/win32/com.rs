/*!
 * SOURCE OF TRUTH KEYWORDS: ComScope, CoInitializeEx, CoUninitialize, COM apartment guard, apartment threaded, multithreaded apartment, RPC_E_CHANGED_MODE
 * WHAT:  ComScope: COM entered on the current thread for as long as the guard lives (`shared` joins cpal's
 *        single-threaded apartment, `multithreaded` is for a thread Echo owns), balanced by exactly one
 *        CoUninitialize on drop when the entry succeeded.
 * WHY:   Core Audio (MMDevice) calls need COM on the calling thread. CoInitializeEx is reference counted per thread
 *        only while every caller asks for the same apartment: cpal enters a single-threaded apartment once per
 *        thread and keeps it for the thread's life, so on a thread cpal also uses (the blocking pool, the take's
 *        arm) Echo must ask for that same apartment. A different one would get RPC_E_CHANGED_MODE for cpal, and
 *        this guard's CoUninitialize would then pull COM out from under cpal's later calls. RPC_E_CHANGED_MODE for
 *        this guard (the thread is already in the other apartment) is not a failure: COM is usable and nothing is
 *        released. MMDevice's enumerator is free-threaded, so no message pump is needed in either apartment. A
 *        dedicated thread that receives OS callbacks uses the multithreaded apartment, so a callback is never
 *        marshalled to a thread that does not pump messages. The guard is tied to its thread (neither Send nor
 *        Sync), because an apartment belongs to the thread that entered it.
 * WHERE: adapters/audio (endpoint properties on shared threads; the device watch thread).
 */

use std::marker::PhantomData;

use windows::Win32::System::Com::{
    COINIT, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize,
};

/// COM is usable on this thread while the guard lives.
pub struct ComScope {
    /// This guard's CoInitializeEx succeeded (S_OK or S_FALSE) and must be balanced.
    owned: bool,
    /// An apartment belongs to one thread.
    _thread_bound: PhantomData<*const ()>,
}

impl ComScope {
    /// For a thread cpal may also use: the single-threaded apartment cpal enters, or whatever the thread has.
    pub fn shared() -> Self {
        Self::enter(COINIT_APARTMENTTHREADED)
    }

    /// For a thread Echo owns that receives OS callbacks: the multithreaded apartment.
    pub fn multithreaded() -> Self {
        Self::enter(COINIT_MULTITHREADED)
    }

    fn enter(apartment: COINIT) -> Self {
        // SAFETY: no reserved pointer; the result decides whether drop balances it on this same thread.
        let result = unsafe { CoInitializeEx(None, apartment) };
        Self {
            owned: result.is_ok(),
            _thread_bound: PhantomData,
        }
    }
}

impl Drop for ComScope {
    fn drop(&mut self) {
        if self.owned {
            // SAFETY: balances the successful CoInitializeEx of `enter` on the thread that made it (the guard is not
            // Send).
            unsafe { CoUninitialize() };
        }
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;

    #[test]
    fn scopes_of_one_apartment_nest_and_balance() {
        thread::spawn(|| {
            let outer = ComScope::shared();
            let inner = ComScope::shared();
            assert!(
                outer.owned && inner.owned,
                "a nested entry is S_FALSE, which is balanced too"
            );
            drop(inner);
            drop(outer);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn a_thread_in_the_other_apartment_is_left_alone() {
        thread::spawn(|| {
            let apartment = ComScope::multithreaded();
            let other = ComScope::shared();
            assert!(apartment.owned);
            assert!(!other.owned, "RPC_E_CHANGED_MODE releases nothing");
        })
        .join()
        .unwrap();
    }
}
