/*!
 * SOURCE OF TRUTH KEYWORDS: FakePowerEvents, fake suspend, fake resume, power event test
 * WHAT:  FakePowerEvents: a PowerEvents whose transitions the test fires.
 * WHY:   Tests cover "suspend finalizes the active take" and "resume re-registers hotkeys" (02 §9, 05 W8) without
 *        sleeping the machine.
 * WHERE: session actor and hotkey wiring tests.
 */

use std::sync::{Arc, Mutex};

use super::lock;
use crate::{
    ports::{EventSink, PowerEvents},
    types::{PortResult, PowerEvent},
};

/// Power transitions fired by the test.
#[derive(Default)]
pub struct FakePowerEvents {
    sink: Mutex<Option<Arc<dyn EventSink<PowerEvent>>>>,
}

impl FakePowerEvents {
    /// Sends `event` to the listener; false when nobody listens.
    pub fn fire(&self, event: PowerEvent) -> bool {
        let sink = lock(&self.sink).clone();
        sink.is_some_and(|sink| {
            sink.emit(event);
            true
        })
    }
}

impl PowerEvents for FakePowerEvents {
    fn listen(&self, sink: Arc<dyn EventSink<PowerEvent>>) -> PortResult<()> {
        *lock(&self.sink) = Some(sink);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::fakes::RecordingSink;

    #[test]
    fn fires_to_the_latest_listener() {
        let power = FakePowerEvents::default();
        assert!(!power.fire(PowerEvent::Suspend));
        let old = Arc::new(RecordingSink::default());
        let new = Arc::new(RecordingSink::default());
        power.listen(old.clone()).unwrap();
        power.listen(new.clone()).unwrap();
        assert!(power.fire(PowerEvent::Resume));
        assert!(old.events().is_empty());
        assert_eq!(new.events(), [PowerEvent::Resume]);
    }
}
