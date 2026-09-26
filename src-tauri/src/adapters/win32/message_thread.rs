/*!
 * SOURCE OF TRUTH KEYWORDS: MessageThread, Win32 message loop thread, GetMessageW loop, PostThreadMessageW WM_QUIT, thread-affine window, WinEvent hook thread, stop and join
 * WHAT:  MessageThread: a named thread that runs a setup closure, then pumps its Win32 message queue until it is
 *        stopped; what the setup returned (a window, a hook) lives on that thread and is dropped there after the
 *        loop ends. Stopping posts WM_QUIT and joins.
 * WHY:   Windows delivers window messages and out-of-context WinEvent callbacks only through the message loop of
 *        the thread that created the window or hook, and destroys them only from that thread, so each such adapter
 *        needs a thread of its own that never blocks (Tauri's event loop may be busy). The queue is created before
 *        the thread reports ready, so a stop posted right after the start is never lost; a setup failure is
 *        returned by `spawn` instead of leaving a thread that silently delivers nothing.
 * WHERE: adapters/power/win32.rs (the hidden session-events window), adapters/foreground/tracker.rs (the foreground
 *        WinEvent hook).
 */

use std::{
    sync::mpsc,
    thread::{self, JoinHandle},
};

use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    System::Threading::GetCurrentThreadId,
    UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW,
        TranslateMessage, WM_QUIT, WM_USER,
    },
};

use crate::types::{AppError, PortError, PortResult};

/// A thread pumping Win32 messages for what its setup created.
pub struct MessageThread {
    name: &'static str,
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl MessageThread {
    /**
     * SOURCE OF TRUTH KEYWORDS: MessageThread::spawn, message loop setup, thread-owned guard
     * WHAT:  Starts thread `name`, runs `setup` on it and, when it succeeds, pumps messages until stopped; the guard
     *        `setup` returned is dropped on the thread after the loop. Returns once the setup has finished.
     * WHY:   The guard type needs no Send: it is created, used and dropped on the one thread that owns it.
     * WHERE: Win32PowerEvents::listen, ForegroundTracker::start.
     */
    pub fn spawn<G, S>(name: &'static str, setup: S) -> PortResult<Self>
    where
        G: 'static,
        S: FnOnce() -> PortResult<G> + Send + 'static,
    {
        let (ready_tx, ready_rx) = mpsc::channel();
        let thread = thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let mut msg = MSG::default();
                // SAFETY: peeking with a filter that matches nothing creates this thread's message queue.
                let _ = unsafe { PeekMessageW(&raw mut msg, None, WM_USER, WM_USER, PM_NOREMOVE) };
                let guard = match setup() {
                    Ok(guard) => guard,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                };
                // SAFETY: no arguments; returns the id of the calling thread.
                let thread_id = unsafe { GetCurrentThreadId() };
                let _ = ready_tx.send(Ok(thread_id));
                pump();
                drop(guard);
            })
            .map_err(|error| failure(name, format!("the thread could not start: {error}")))?;
        match ready_rx.recv() {
            Ok(Ok(thread_id)) => Ok(Self {
                name,
                thread_id,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => {
                let _ = thread.join();
                Err(failure(
                    name,
                    String::from("the thread ended before it was ready"),
                ))
            }
        }
    }

    /// Ends the loop and waits for the thread (idempotent).
    pub fn stop(&mut self) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        // SAFETY: plain value arguments; the thread's queue exists since before it reported ready.
        if let Err(error) =
            unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) }
        {
            // The thread already ended (its loop failed); joining still collects it.
            tracing::debug!(thread = self.name, %error, "stop message not posted");
        }
        if thread.join().is_err() {
            tracing::warn!(thread = self.name, "a message thread panicked");
        }
    }
}

impl Drop for MessageThread {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Dispatches messages until WM_QUIT (or a GetMessage failure).
fn pump() {
    let mut msg = MSG::default();
    loop {
        // SAFETY: `msg` outlives the call; None reads every message of this thread.
        let got = unsafe { GetMessageW(&raw mut msg, None, 0, 0) };
        if got.0 <= 0 {
            return;
        }
        // SAFETY: `msg` was just filled by GetMessageW.
        unsafe {
            let _ = TranslateMessage(&raw const msg);
            DispatchMessageW(&raw const msg);
        }
    }
}

fn failure(name: &str, detail: String) -> PortError {
    PortError::new(AppError::Internal).with_detail(format!("{name}: {detail}"))
}

#[cfg(test)]
mod tests {
    use std::{
        cell::Cell,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };

    use super::*;

    /// Records on drop that it was dropped on the thread that made it.
    struct Guard {
        made_on: Cell<Option<thread::ThreadId>>,
        dropped_on_owner: Arc<AtomicBool>,
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            let same = self.made_on.get() == Some(thread::current().id());
            self.dropped_on_owner.store(same, Ordering::SeqCst);
        }
    }

    #[test]
    fn the_guard_lives_and_dies_on_its_thread_and_a_failed_setup_is_reported() {
        let dropped_on_owner = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&dropped_on_owner);
        let mut worker = MessageThread::spawn("echo-test-messages", move || {
            Ok(Guard {
                made_on: Cell::new(Some(thread::current().id())),
                dropped_on_owner: flag,
            })
        })
        .unwrap();
        worker.stop();
        worker.stop();
        assert!(dropped_on_owner.load(Ordering::SeqCst));

        let failed = MessageThread::spawn("echo-test-messages", || -> PortResult<()> {
            Err(AppError::Internal.into())
        });
        assert!(failed.is_err());
    }
}
