/*!
 * SOURCE OF TRUTH KEYWORDS: port fakes, test doubles, fake adapters, RecordingSink, poll_once, pipeline tests, cfg(test)
 * WHAT:  One scriptable in-memory fake per port (test builds only), plus RecordingSink (an EventSink that keeps
 *        every event) and `poll_once` (drives a fake's future without an async runtime).
 * WHY:   Pipeline, factory and registry tests (02 §13) need every port without a microphone, a model, a GPU or a
 *        desktop. The fakes live in ports/ because pipeline and ipc may import ports but not adapters (02 §3.2),
 *        and they are compiled only under `#[cfg(test)]`, so nothing here ships. Each fake enforces its port's
 *        documented contract (e.g. the capture fake refuses a second open stream, the inserter fake refuses an
 *        elevated target), so a pipeline test that breaks a contract fails here instead of on a user's machine.
 *        Fakes resolve their futures on the first poll (or never, to exercise timeouts), which `poll_once`
 *        observes directly; async pipeline tests can simply `.await` them.
 * WHERE: `use crate::ports::fakes::…` from any `#[cfg(test)]` module in pipeline/, ipc/, registry/ or app/.
 */

mod asr;
mod audio;
mod clipboard;
mod consent;
mod foreground;
mod hotkey;
mod inserter;
mod model_store;
mod notifier;
mod polish;
mod power;
mod updater;
mod vad;

use std::{
    future::Future,
    pin::pin,
    sync::{Mutex, MutexGuard, PoisonError},
    task::{Context, Poll, Waker},
};

pub use asr::{AsrCall, FakeAsrEngine};
pub use audio::{FakeAudioCapture, RecordingAudioSink};
pub use clipboard::FakeClipboard;
pub use consent::FakePrivacyConsent;
pub use foreground::FakeForegroundApp;
pub use hotkey::FakeHotkeyService;
pub use inserter::FakeTextInserter;
pub use model_store::FakeModelStore;
pub use notifier::FakeNotifier;
pub use polish::{FakePolish, FakeTextPolisher};
pub use power::FakePowerEvents;
pub use updater::FakeUpdater;
pub use vad::FakeVoiceActivity;

use super::EventSink;

/// Locks a fake's state; a test that panicked while holding it must not hide the original failure.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Polls `future` once with a no-op waker: `Ready` for a fake that completed, `Pending` for one that hangs.
pub fn poll_once<F: Future>(future: F) -> Poll<F::Output> {
    let mut future = pin!(future);
    future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
}

/// An EventSink that records every event in order.
pub struct RecordingSink<E> {
    events: Mutex<Vec<E>>,
}

impl<E> Default for RecordingSink<E> {
    fn default() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
        }
    }
}

impl<E: Clone> RecordingSink<E> {
    /// Every event received so far.
    pub fn events(&self) -> Vec<E> {
        lock(&self.events).clone()
    }
}

impl<E: Send> EventSink<E> for RecordingSink<E> {
    fn emit(&self, event: E) {
        lock(&self.events).push(event);
    }
}

#[cfg(test)]
mod tests {
    use std::future;

    use super::*;

    #[test]
    fn poll_once_reports_ready_and_pending() {
        assert_eq!(poll_once(future::ready(7)), Poll::Ready(7));
        assert_eq!(poll_once(future::pending::<u8>()), Poll::Pending);
    }

    #[test]
    fn recording_sink_keeps_events_in_order() {
        let sink = RecordingSink::default();
        sink.emit(1);
        sink.emit(2);
        assert_eq!(sink.events(), [1, 2]);
    }
}
