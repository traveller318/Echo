/*!
 * SOURCE OF TRUTH KEYWORDS: FakeSystemAppearance, fake transparency, fake Mica caps, appearance test
 * WHAT:  FakeSystemAppearance: a SystemAppearance whose caps and transparency the test sets, whose next read can
 *        fail, and whose transparency changes the test fires to the listener.
 * WHY:   The appearance view, the relay and the settings commands need the Mica / no-Mica, full / reduced and
 *        unreadable paths without touching Windows personalization settings.
 * WHERE: pipeline/appearance.rs, ipc/commands tests and ipc::testing (default CommandCtx).
 */

use std::sync::{Arc, Mutex};

use super::lock;
use crate::{
    ports::{EventSink, SystemAppearance},
    types::{AppearanceCaps, PortError, PortResult, Transparency},
};

struct AppearanceState {
    transparency: Transparency,
    next_error: Option<PortError>,
    sink: Option<Arc<dyn EventSink<Transparency>>>,
}

/// A settable system appearance.
pub struct FakeSystemAppearance {
    caps: AppearanceCaps,
    state: Mutex<AppearanceState>,
}

impl FakeSystemAppearance {
    pub fn new(caps: AppearanceCaps, transparency: Transparency) -> Self {
        Self {
            caps,
            state: Mutex::new(AppearanceState {
                transparency,
                next_error: None,
                sink: None,
            }),
        }
    }

    /// Windows 11 with transparency effects on.
    pub fn mica() -> Self {
        Self::new(AppearanceCaps { mica: true }, Transparency::Full)
    }

    /// Makes the next `transparency` read fail with `error`.
    pub fn fail_next(&self, error: PortError) {
        lock(&self.state).next_error = Some(error);
    }

    /// Changes the switch and tells the listener; false when nobody listens.
    pub fn switch(&self, transparency: Transparency) -> bool {
        let sink = {
            let mut state = lock(&self.state);
            state.transparency = transparency;
            state.sink.clone()
        };
        sink.is_some_and(|sink| {
            sink.emit(transparency);
            true
        })
    }
}

impl SystemAppearance for FakeSystemAppearance {
    fn caps(&self) -> AppearanceCaps {
        self.caps
    }

    fn transparency(&self) -> PortResult<Transparency> {
        let mut state = lock(&self.state);
        state.next_error.take().map_or(Ok(state.transparency), Err)
    }

    fn listen(&self, sink: Arc<dyn EventSink<Transparency>>) -> PortResult<()> {
        lock(&self.state).sink = Some(sink);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ports::fakes::RecordingSink, types::AppError};

    #[test]
    fn reads_fails_once_and_switches_for_the_latest_listener() {
        let appearance = FakeSystemAppearance::mica();
        assert!(appearance.caps().mica);
        assert_eq!(appearance.transparency(), Ok(Transparency::Full));
        appearance.fail_next(PortError::new(AppError::Internal));
        assert!(appearance.transparency().is_err());
        assert_eq!(appearance.transparency(), Ok(Transparency::Full));

        assert!(!appearance.switch(Transparency::Reduced));
        let sink = Arc::new(RecordingSink::default());
        appearance.listen(sink.clone()).unwrap();
        assert!(appearance.switch(Transparency::Full));
        assert_eq!(sink.events(), [Transparency::Full]);
    }
}
