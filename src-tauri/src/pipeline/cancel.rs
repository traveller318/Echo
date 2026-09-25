/*!
 * SOURCE OF TRUTH KEYWORDS: Cancellation, cancel token, until_cancelled, cancel running task, cancel download, stop from another command
 * WHAT:  Cancellation: a one-way "stop" flag another task can raise and a running task can await; `until_cancelled`
 *        runs a future until it finishes (Some) or the cancellation is raised (None, the future is dropped).
 * WHY:   A model download runs inside one command's future for minutes, and `models_cancel_download` is a different
 *        command: dropping the running future is how a port transfer is cancelled (02 §3.4), so the running command
 *        races its work against this signal. Built on tokio's Notify with the waiter enabled before the flag is
 *        read, so a cancel raised just before the wait is never lost; no `select!` macro (tokio `macros` is not
 *        compiled in). Raising it twice, or after the work ended, does nothing.
 * WHERE: pipeline/models (every transfer); reusable by any long operation a second command must be able to stop.
 */

use std::{
    future::{Future, poll_fn},
    pin::pin,
    sync::atomic::{AtomicBool, Ordering},
    task::Poll,
};

use tokio::sync::Notify;

/// A stop signal for one running operation.
#[derive(Debug, Default)]
pub struct Cancellation {
    raised: AtomicBool,
    notify: Notify,
}

impl Cancellation {
    /// Asks the operation to stop; it stops at its next await.
    pub fn cancel(&self) {
        self.raised.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.raised.load(Ordering::SeqCst)
    }

    /// Resolves once `cancel` has been called.
    pub async fn cancelled(&self) {
        loop {
            let mut notified = pin!(self.notify.notified());
            notified.as_mut().enable();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}

/// Runs `work` to completion (Some) unless `cancellation` is raised first (None; `work` is dropped).
pub async fn until_cancelled<F: Future>(cancellation: &Cancellation, work: F) -> Option<F::Output> {
    let mut work = pin!(work);
    let mut stop = pin!(cancellation.cancelled());
    poll_fn(|cx| {
        if stop.as_mut().poll(cx).is_ready() {
            return Poll::Ready(None);
        }
        work.as_mut().poll(cx).map(Some)
    })
    .await
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use super::*;

    fn block_on<T>(future: impl Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(future)
    }

    #[test]
    fn finished_work_is_returned_and_raised_cancellation_drops_the_work() {
        let cancellation = Cancellation::default();
        assert_eq!(
            block_on(until_cancelled(&cancellation, async { 7 })),
            Some(7)
        );
        cancellation.cancel();
        cancellation.cancel();
        assert!(cancellation.is_cancelled());
        assert_eq!(
            block_on(until_cancelled(&cancellation, std::future::pending::<()>())),
            None
        );
    }

    #[test]
    fn a_cancel_from_another_thread_stops_waiting_work() {
        let cancellation = Arc::new(Cancellation::default());
        let raiser = Arc::clone(&cancellation);
        let thread = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            raiser.cancel();
        });
        let result = block_on(async {
            until_cancelled(&cancellation, tokio::time::sleep(Duration::from_secs(30))).await
        });
        assert_eq!(result, None);
        thread.join().unwrap();
    }
}
