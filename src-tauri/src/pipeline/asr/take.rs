/*!
 * SOURCE OF TRUTH KEYWORDS: AsrTake, take handle, segment sink, finish take, cancel take, release take, per-take events
 * WHAT:  AsrTake: one take's door into the ASR worker. It is the EventSink the capture worker pushes SpeechSegments
 *        into; `finish` asks for a `Drained` event after the last segment; `cancel` drops every segment not yet
 *        transcribed; dropping it releases the engine the take was pinned to.
 * WHY:   The capture worker already hands segments to an `EventSink<SpeechSegment>` (pipeline/capture), so the take
 *        plugs in without the capture knowing ASR exists, and `emit` only queues (it runs on the capture thread and
 *        must never wait). Tagging every message with the take keeps results of a discarded take out of the next
 *        one, and the pin makes a take finish on the engine it started with even if the engine is switched mid-take
 *        (02 §8.1). Cancel is a shared flag the worker checks before each segment, so an Esc discard stops inference
 *        at the next segment boundary instead of queueing behind it.
 * WHERE: Created by AsrWorker::begin_take (worker.rs) for the session actor; handed to Capture::start as the
 *        segment sink; read by the worker thread.
 */

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
};

use super::worker::{Message, TakeMessage};
use crate::{
    ports::EventSink,
    types::{AsrEvent, Language, SpeechSegment, TranscriptId},
};

/// What the worker thread needs to know about a take.
pub(super) struct TakeShared {
    pub id: TranscriptId,
    /// The user's language preference (None = auto); the worker narrows it to the engine's caps.
    pub language: Option<Language>,
    pub events: Arc<dyn EventSink<AsrEvent>>,
    cancelled: AtomicBool,
}

impl TakeShared {
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// One take's segments on their way to the ASR worker.
pub struct AsrTake {
    shared: Arc<TakeShared>,
    inbox: Sender<Message>,
}

impl AsrTake {
    pub(super) fn new(
        id: TranscriptId,
        language: Option<Language>,
        events: Arc<dyn EventSink<AsrEvent>>,
        inbox: Sender<Message>,
    ) -> Self {
        Self {
            shared: Arc::new(TakeShared {
                id,
                language,
                events,
                cancelled: AtomicBool::new(false),
            }),
            inbox,
        }
    }

    /// The take's transcript id.
    pub fn id(&self) -> TranscriptId {
        self.shared.id
    }

    /// No more segments follow: `Drained` is emitted once every segment handed over so far is reported.
    pub fn finish(&self) {
        self.send(TakeMessage::Finish {
            take: Arc::clone(&self.shared),
        });
    }

    /// Stops transcribing this take: segments not yet started are skipped and no further event is emitted.
    pub fn cancel(&self) {
        self.shared.cancelled.store(true, Ordering::Release);
    }

    fn send(&self, message: TakeMessage) {
        // A closed inbox means the worker has shut down with the app; there is nobody left to report to.
        let _ = self.inbox.send(Message::Take(message));
    }
}

impl EventSink<SpeechSegment> for AsrTake {
    fn emit(&self, segment: SpeechSegment) {
        self.send(TakeMessage::Segment {
            take: Arc::clone(&self.shared),
            segment,
        });
    }
}

impl Drop for AsrTake {
    fn drop(&mut self) {
        self.send(TakeMessage::Release {
            take: self.shared.id,
        });
    }
}
