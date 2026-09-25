/*!
 * SOURCE OF TRUTH KEYWORDS: session inbox, Message, WorkerReply, Outbox, HotkeyForwarder, TakeEvents, actor mailbox, weak sender
 * WHAT:  The session actor's mailbox: every message it can receive (Message: hotkeys, the pill, queries, paste-last,
 *        lifecycle and WorkerReply, the replies of the work its effects started), the Outbox the actor's own tasks and sinks
 *        post through, and the port sinks that forward into it (HotkeyForwarder for the hotkey port, TakeEvents for
 *        one take's capture and ASR events).
 * WHY:   All inputs go through one inbox, so the actor handles them one at a time in arrival order and is the only
 *        owner of recording state (02 §5). Ports push from their own threads (a hotkey hook, the capture worker, the
 *        ASR thread) and must never wait (ports/event_sink.rs), so the inbox is an unbounded channel: every input
 *        is small and rare (a handful per take plus one per speech segment). Everything the actor hands out holds a
 *        weak sender, so the inbox closes when the last SessionHandle is dropped and the actor ends even if a
 *        port or a stray task still holds a sink. Capture events carry no take id, so TakeEvents stamps them with
 *        the take they belong to; a late event from an ended take is then recognised as stale by the machine.
 * WHERE: Built by SessionHandle::new (actor.rs); read by the actor loop; posted to by runner.rs tasks, the hotkey
 *        adapter (through HotkeyForwarder) and a take's capture worker and ASR worker (through TakeEvents).
 */

use tokio::sync::{
    mpsc::{UnboundedReceiver, UnboundedSender, WeakUnboundedSender},
    oneshot,
};

use super::arm::ArmOutcome;
use crate::{
    pipeline::capture::CaptureOutcome,
    ports::{EventSink, VoiceActivity},
    types::{
        AsrEvent, CaptureEvent, DeliveryOutcome, DeliveryReport, HotkeyEvent, PolishOutcome,
        PortResult, SessionInput, SessionUiInput, SessionView, TranscriptId,
    },
};

/// The reply of work an effect started off the actor (a blocking call, a timer, a worker's event).
pub(super) enum WorkerReply {
    /// The Arm effect finished.
    Arm(Box<ArmOutcome>),
    /// The take's microphone reported a device loss or a failure.
    Capture {
        take: TranscriptId,
        event: CaptureEvent,
    },
    /// The ASR worker reported on a take's segments.
    Asr(AsrEvent),
    /// StopCapture closed the microphone and flushed the last segment.
    CaptureFinished {
        take: TranscriptId,
        outcome: Box<CaptureOutcome>,
    },
    /// Deliver polished the text and delivered it (or failed to).
    Delivered {
        take: TranscriptId,
        polish: PolishOutcome,
        result: PortResult<DeliveryReport>,
    },
    /// A timer fired; the input names its token.
    Timer(SessionInput),
    /// A voice activity detector built ahead of the next take.
    Detector(Box<dyn VoiceActivity>),
    /// Paste-last delivered the newest completed take (or failed to); `reply` is the caller waiting, if any.
    PastedLast {
        result: PortResult<DeliveryReport>,
        reply: Option<PasteLastReply>,
    },
}

/// Where a paste-last answers: what reached the user, or why nothing did.
pub(super) type PasteLastReply = oneshot::Sender<PortResult<DeliveryOutcome>>;

/// Everything the session actor receives.
pub(super) enum Message {
    /// A bound hotkey went down or up.
    Hotkey(HotkeyEvent),
    /// An input from the pill (`session_input`).
    Ui(SessionUiInput),
    Worker(WorkerReply),
    /// `session_get_state`: the view at this moment.
    View(oneshot::Sender<SessionView>),
    /// Deliver the newest completed take again (`history_paste_last`; the paste-last hotkey sends no reply).
    PasteLast(Option<PasteLastReply>),
    /// The windows exist: listen to hotkeys, bind them, warm the detector and the polish chain.
    Prepare,
    /// The app is exiting: finalize every open journal, then answer and stop.
    Shutdown(std::sync::mpsc::Sender<()>),
}

/// The receiving end, owned by the actor.
pub(super) type Receiver = UnboundedReceiver<Message>;

/// The sending end the SessionHandle holds; the inbox stays open while one exists.
pub(super) type Sender = UnboundedSender<Message>;

/// A weak way into the inbox for the actor's own tasks and sinks.
#[derive(Clone)]
pub(super) struct Outbox(WeakUnboundedSender<Message>);

impl Outbox {
    pub fn new(sender: &Sender) -> Self {
        Self(sender.downgrade())
    }

    /// Posts `message`; false when the actor has stopped (the message, and anything it holds, is dropped).
    pub fn post(&self, message: Message) -> bool {
        self.0
            .upgrade()
            .is_some_and(|sender| sender.send(message).is_ok())
    }

    /// Posts a worker reply.
    pub fn reply(&self, reply: WorkerReply) -> bool {
        self.post(Message::Worker(reply))
    }
}

/// The HotkeyService sink: every press and release becomes an inbox message.
pub(super) struct HotkeyForwarder(pub Outbox);

impl EventSink<HotkeyEvent> for HotkeyForwarder {
    fn emit(&self, event: HotkeyEvent) {
        self.0.post(Message::Hotkey(event));
    }
}

/// One take's capture and ASR events, stamped with the take and posted to the inbox.
pub(super) struct TakeEvents {
    pub take: TranscriptId,
    pub outbox: Outbox,
}

impl EventSink<CaptureEvent> for TakeEvents {
    fn emit(&self, event: CaptureEvent) {
        self.outbox.reply(WorkerReply::Capture {
            take: self.take,
            event,
        });
    }
}

impl EventSink<AsrEvent> for TakeEvents {
    fn emit(&self, event: AsrEvent) {
        self.outbox.reply(WorkerReply::Asr(event));
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    use super::*;
    use crate::types::{HotkeyId, KeyState};

    #[test]
    fn sinks_stamp_their_take_and_stop_posting_once_the_inbox_is_gone() {
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let outbox = Outbox::new(&sender);
        let take = TranscriptId::generate();
        let events = TakeEvents {
            take,
            outbox: outbox.clone(),
        };
        EventSink::<CaptureEvent>::emit(&events, CaptureEvent::DeviceLost);
        assert!(matches!(
            receiver.try_recv(),
            Ok(Message::Worker(WorkerReply::Capture { take: stamped, event: CaptureEvent::DeviceLost }))
                if stamped == take
        ));
        HotkeyForwarder(outbox.clone()).emit(HotkeyEvent {
            id: HotkeyId::from_static("record"),
            state: KeyState::Pressed,
        });
        assert!(matches!(receiver.try_recv(), Ok(Message::Hotkey(_))));

        drop(sender);
        assert!(
            !outbox.post(Message::Prepare),
            "a weak sender never keeps the inbox open"
        );
    }
}
