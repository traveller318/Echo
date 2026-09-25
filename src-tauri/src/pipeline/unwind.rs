/*!
 * SOURCE OF TRUTH KEYWORDS: catch_unwind future, panic-safe await, poll_fn, AssertUnwindSafe, keep running after panic, survive panic
 * WHAT:  `catch_unwind(future)`: awaits `future`, returning Some(output), or None if one of its polls panicked.
 * WHY:   Long-lived async work (a command handler, the session actor's message handling) must survive a bug in one
 *        call: a panic would otherwise end the task and leave a promise pending or the session deaf to hotkeys
 *        (02 §12 "the app keeps running where it can"). The future is dropped right after a panic and never polled
 *        again, so no half-updated state inside it is observed (hence AssertUnwindSafe); callers that keep state
 *        outside the future repair it themselves. Built on std's poll_fn, so no extra crate is needed. One helper,
 *        so every caller maps a panic the same way.
 * WHERE: ipc/factory.rs (a handler panic becomes `Internal`); pipeline/session/actor.rs (a panic while handling
 *        a message fails the live take and the actor carries on).
 */

use std::{
    future::{Future, poll_fn},
    panic::{self, AssertUnwindSafe},
    pin::pin,
    task::Poll,
};

/// Polls `future` to completion; None if a poll panicked.
pub async fn catch_unwind<F: Future>(future: F) -> Option<F::Output> {
    let mut future = pin!(future);
    poll_fn(
        |cx| match panic::catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(cx))) {
            Ok(Poll::Ready(output)) => Poll::Ready(Some(output)),
            Ok(Poll::Pending) => Poll::Pending,
            Err(_) => Poll::Ready(None),
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run<T>(future: impl Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(future)
    }

    fn explode() -> u8 {
        panic!("boom")
    }

    #[test]
    fn output_passes_through_and_a_panic_after_an_await_is_none() {
        assert_eq!(run(catch_unwind(async { 7 })), Some(7));
        let panicked = run(catch_unwind(async {
            tokio::task::yield_now().await;
            explode()
        }));
        assert_eq!(panicked, None);
    }
}
