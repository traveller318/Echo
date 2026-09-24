/*!
 * SOURCE OF TRUTH KEYWORDS: HotkeyService, global hotkey, register shortcut, unregister, refresh bindings, hotkey conflict, hold-to-talk, listen
 * WHAT:  HotkeyService binds registry hotkey ids to key combinations system-wide and pushes presses (and
 *        releases, when supported) to one EventSink.
 * WHY:   `register` replaces a binding atomically: if the new combination fails (another app owns it, 05 W7) the
 *        previous one stays active, so a bad edit in Settings never leaves the user without a record hotkey.
 *        `unregister` of an unknown id succeeds, so the effect runner can release Esc on every exit path without
 *        tracking state (05 W10). `refresh` re-registers every binding after sleep or an explorer restart
 *        (05 W8). Hold mode needs key-up (`HotkeyCaps.supports_release`) and Ctrl+Alt needs modifier-only chords
 *        (`supports_modifier_only`), which the low-level keyboard hook adapter reports (05 W9).
 * WHERE: Implemented by adapters/hotkey/low_level_hook (LowLevelKeyboardHotkeys) and ports/fakes; driven by
 *        pipeline/hotkeys.rs from registry/hotkeys and settings; events feed the session actor.
 */

use std::sync::Arc;

use super::EventSink;
use crate::types::{HotkeyCaps, HotkeyEvent, HotkeyId, PortResult, Shortcut};

/// System-wide hotkeys.
pub trait HotkeyService: Send + Sync {
    fn caps(&self) -> HotkeyCaps;

    /// Sends every later press and release to `sink`, replacing the previous sink.
    fn listen(&self, sink: Arc<dyn EventSink<HotkeyEvent>>) -> PortResult<()>;

    /// Binds `id` to `shortcut`, replacing its current binding only on success. Fails with
    /// `Hotkey { reason: conflict }` when another app owns the combination and `Hotkey { reason: invalid }`
    /// when the combination cannot be parsed or bound by this adapter.
    fn register(&self, id: &HotkeyId, shortcut: &Shortcut) -> PortResult<()>;

    /// Removes the binding of `id`; an id with no binding is a no-op.
    fn unregister(&self, id: &HotkeyId) -> PortResult<()>;

    /// Registers every current binding again with the system.
    fn refresh(&self) -> PortResult<()>;
}
