/*!
 * SOURCE OF TRUTH KEYWORDS: ReentrancyLocks, ReentrancyGuard, keyed lock, Exclusive, Busy, try acquire, reentrancy guard
 * WHAT:  The keyed reentrancy locks of the command factory: `acquire(Reentrancy)` returns a guard that holds an
 *        `Exclusive` key until it is dropped, or `AppError::Busy` when another call already holds the key.
 * WHY:   02 §4.1 step 4: a second caller of an exclusive operation (a model download, a retry of the same take)
 *        is refused at once instead of queueing behind the first, so the UI shows "busy" rather than a spinner
 *        that finishes twice. It is a try-lock over a set of held keys, not an async mutex: nothing ever waits,
 *        so no runtime is needed and the lock is held only for a set insert or remove. The key is released in
 *        `Drop`, so it frees on success, error, panic and cancellation (a dropped command future) alike.
 * WHERE: Owned by ipc::CommandCtx; used only by ipc/factory.rs (`run`, step 4).
 */

use std::collections::HashSet;

use parking_lot::Mutex;

use crate::types::{AppError, Reentrancy};

/// The exclusive keys currently held by running commands.
#[derive(Debug, Default)]
pub struct ReentrancyLocks {
    held: Mutex<HashSet<&'static str>>,
}

/// Holds an exclusive key until dropped; a `Shared` call holds nothing.
#[derive(Debug)]
pub struct ReentrancyGuard<'a> {
    held: Option<(&'a ReentrancyLocks, &'static str)>,
}

impl ReentrancyLocks {
    /// Takes the key `reentrancy` names, or fails with `Busy` if a running call holds it.
    pub fn acquire(&self, reentrancy: Reentrancy) -> Result<ReentrancyGuard<'_>, AppError> {
        match reentrancy {
            Reentrancy::Shared => Ok(ReentrancyGuard { held: None }),
            Reentrancy::Exclusive(key) => {
                if self.held.lock().insert(key) {
                    Ok(ReentrancyGuard {
                        held: Some((self, key)),
                    })
                } else {
                    Err(AppError::Busy)
                }
            }
        }
    }
}

impl Drop for ReentrancyGuard<'_> {
    fn drop(&mut self) {
        if let Some((locks, key)) = self.held.take() {
            locks.held.lock().remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exclusive_key_admits_one_holder_until_dropped() {
        let locks = ReentrancyLocks::default();
        let first = locks
            .acquire(Reentrancy::Exclusive("model_transfer"))
            .unwrap();
        assert!(locks.held.lock().contains("model_transfer"));
        assert_eq!(
            locks
                .acquire(Reentrancy::Exclusive("model_transfer"))
                .unwrap_err(),
            AppError::Busy
        );
        let other = locks.acquire(Reentrancy::Exclusive("retry")).unwrap();
        drop(first);
        assert!(!locks.held.lock().contains("model_transfer"));
        assert!(
            locks
                .acquire(Reentrancy::Exclusive("model_transfer"))
                .is_ok()
        );
        drop(other);
        assert!(!locks.held.lock().contains("retry"));
    }

    #[test]
    fn shared_calls_never_block_and_hold_nothing() {
        let locks = ReentrancyLocks::default();
        let _a = locks.acquire(Reentrancy::Shared).unwrap();
        let _b = locks.acquire(Reentrancy::Shared).unwrap();
        assert!(locks.held.lock().is_empty());
    }
}
