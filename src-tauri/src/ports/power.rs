/*!
 * SOURCE OF TRUTH KEYWORDS: PowerEvents, suspend resume notification, sleep wake, power listener
 * WHAT:  PowerEvents pushes suspend and resume transitions to an EventSink.
 * WHY:   Suspend finalizes an active take; resume re-registers hotkeys and reopens audio lazily (02 §9, 05 W8).
 *        A push sink keeps the Win32 notification callback trivial.
 * WHERE: Implemented by adapters/power/win32.rs (Win32PowerEvents) and ports/fakes; wired by the pipeline at
 *        startup.
 */

use std::sync::Arc;

use super::EventSink;
use crate::types::{PortResult, PowerEvent};

/// System sleep and wake notifications.
pub trait PowerEvents: Send + Sync {
    /// Sends every later transition to `sink`, replacing the previous sink.
    fn listen(&self, sink: Arc<dyn EventSink<PowerEvent>>) -> PortResult<()>;
}
