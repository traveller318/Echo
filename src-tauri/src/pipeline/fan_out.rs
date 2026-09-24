/*!
 * SOURCE OF TRUTH KEYWORDS: FanOut, fan-out event sink, tee events, observe AppEvent, multiple event sinks
 * WHAT:  FanOut<E>: an EventSink that hands every event to each of several sinks, in order.
 * WHY:   Some Rust components react to the same events the UI receives (the pill presenter follows
 *        SessionStateChanged to show and hide its window) without the emitter knowing they exist, so a new observer
 *        is one more sink in the composition root, never a change to the session actor. Each sink keeps the
 *        EventSink contract (never blocks), so the fan-out never blocks either.
 * WHERE: Built by app/bootstrap around the TauriEventSink and the PillPresenter; used as the one
 *        `EventSink<AppEvent>` of CommandCtx and the session actor.
 */

use std::sync::Arc;

use crate::ports::EventSink;

/// Sends every event to each sink, in order.
pub struct FanOut<E> {
    sinks: Vec<Arc<dyn EventSink<E>>>,
}

impl<E> FanOut<E> {
    pub fn new(sinks: Vec<Arc<dyn EventSink<E>>>) -> Self {
        Self { sinks }
    }
}

impl<E: Clone + Send> EventSink<E> for FanOut<E> {
    fn emit(&self, event: E) {
        if let Some((last, rest)) = self.sinks.split_last() {
            for sink in rest {
                sink.emit(event.clone());
            }
            last.emit(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::fakes::RecordingSink;

    #[test]
    fn every_sink_receives_every_event_in_order() {
        let first = Arc::new(RecordingSink::default());
        let second = Arc::new(RecordingSink::default());
        let fan_out = FanOut::new(vec![first.clone(), second.clone()]);
        fan_out.emit(1_u8);
        fan_out.emit(2_u8);
        assert_eq!(first.events(), [1, 2]);
        assert_eq!(second.events(), [1, 2]);
        FanOut::<u8>::new(Vec::new()).emit(3);
    }
}
