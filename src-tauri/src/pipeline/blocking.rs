/*!
 * SOURCE OF TRUTH KEYWORDS: run_blocking, blocking pool, spawn_blocking, blocking port call, panic to Internal, async wrapper
 * WHAT:  `run_blocking(what, work)`: runs port or file work that may block (clipboard retries, disk, a WAV read)
 *        on tokio's blocking pool and awaits its result; a panic in it becomes `Internal` with `what` in the
 *        logged detail.
 * WHY:   Command handlers and pipeline tasks are async and share Tauri's runtime threads; a clipboard held by
 *        another app blocks for up to the adapter's retries (05 W4), which must not stall other commands or
 *        events. One helper keeps the panic mapping identical everywhere (the factory's promise always settles).
 * WHERE: ipc/commands/history.rs (copy, delete), pipeline/retry.rs (reading and transcribing a journal),
 *        pipeline/retention.rs (each sweep).
 */

use crate::types::{AppError, PortError, PortResult};

/// Runs `work` on the blocking pool; must be awaited inside a tokio runtime.
pub async fn run_blocking<T: Send + 'static>(
    what: &'static str,
    work: impl FnOnce() -> PortResult<T> + Send + 'static,
) -> PortResult<T> {
    tokio::task::spawn_blocking(work).await.map_err(|error| {
        PortError::new(AppError::Internal).with_detail(format!("{what} panicked: {error}"))
    })?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(future)
    }

    #[test]
    fn results_and_errors_pass_through_and_a_panic_is_internal() {
        assert_eq!(run(run_blocking("adding", || Ok(2 + 2))).unwrap(), 4);
        assert_eq!(
            run(run_blocking("failing", || Err::<(), _>(PortError::new(
                AppError::Busy
            ))))
            .unwrap_err()
            .error(),
            &AppError::Busy
        );
        let panicked = run(run_blocking("exploding", || -> PortResult<()> {
            panic!("boom")
        }))
        .unwrap_err();
        assert_eq!(panicked.error(), &AppError::Internal);
        assert!(
            panicked
                .detail()
                .is_some_and(|detail| detail.contains("exploding"))
        );
    }
}
