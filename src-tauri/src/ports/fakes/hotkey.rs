/*!
 * SOURCE OF TRUTH KEYWORDS: FakeHotkeyService, fake hotkeys, press hotkey, release hotkey, hotkey conflict test, occupied shortcut
 * WHAT:  FakeHotkeyService: a HotkeyService with an in-memory binding table; tests `press` / `release` /
 *        `interrupt` bound ids and `occupy` combinations as if another app owned them.
 * WHY:   Session tests drive the record, cancel and paste-last hotkeys directly, and settings tests need the
 *        conflict path (05 W7): a failed `register` must keep the previous binding. Releases are only emitted when
 *        the caps declare `supports_release` and interruptions only with `supports_modifier_only`, mirroring what
 *        the session may rely on.
 * WHERE: pipeline hotkey wiring, session actor and settings tests.
 */

use std::{
    collections::{BTreeMap, HashSet},
    sync::{Arc, Mutex},
};

use super::lock;
use crate::{
    ports::{EventSink, HotkeyService},
    types::{
        AppError, HotkeyCaps, HotkeyEvent, HotkeyId, HotkeyIssue, KeyState, PortResult, Shortcut,
    },
};

#[derive(Default)]
struct HotkeyState {
    sink: Option<Arc<dyn EventSink<HotkeyEvent>>>,
    bindings: BTreeMap<HotkeyId, Shortcut>,
    occupied: HashSet<Shortcut>,
    refreshes: usize,
}

/// An in-memory hotkey table.
pub struct FakeHotkeyService {
    caps: HotkeyCaps,
    state: Mutex<HotkeyState>,
}

impl Default for FakeHotkeyService {
    /// Reports releases, so hold mode is available.
    fn default() -> Self {
        Self::new(HotkeyCaps {
            supports_release: true,
            supports_modifier_only: false,
        })
    }
}

impl FakeHotkeyService {
    pub fn new(caps: HotkeyCaps) -> Self {
        Self {
            caps,
            state: Mutex::default(),
        }
    }

    /// Another app now owns `shortcut`.
    pub fn occupy(&self, shortcut: Shortcut) {
        lock(&self.state).occupied.insert(shortcut);
    }

    pub fn binding(&self, id: &HotkeyId) -> Option<Shortcut> {
        lock(&self.state).bindings.get(id).cloned()
    }

    pub fn refreshes(&self) -> usize {
        lock(&self.state).refreshes
    }

    /// The user pressed the combination bound to `id`; false when nothing was emitted.
    pub fn press(&self, id: &HotkeyId) -> bool {
        self.fire(id, KeyState::Pressed)
    }

    /// The user released it; emitted only when the caps report releases.
    pub fn release(&self, id: &HotkeyId) -> bool {
        self.caps.supports_release && self.fire(id, KeyState::Released)
    }

    /// Another key joined the held combination; emitted only when the caps allow modifier-only combinations.
    pub fn interrupt(&self, id: &HotkeyId) -> bool {
        self.caps.supports_modifier_only && self.fire(id, KeyState::Interrupted)
    }

    fn fire(&self, id: &HotkeyId, state: KeyState) -> bool {
        let sink = {
            let guard = lock(&self.state);
            if !guard.bindings.contains_key(id) {
                return false;
            }
            guard.sink.clone()
        };
        sink.is_some_and(|sink| {
            sink.emit(HotkeyEvent {
                id: id.clone(),
                state,
            });
            true
        })
    }

    fn issue(reason: HotkeyIssue) -> AppError {
        AppError::Hotkey { reason }
    }
}

impl HotkeyService for FakeHotkeyService {
    fn caps(&self) -> HotkeyCaps {
        self.caps
    }

    fn listen(&self, sink: Arc<dyn EventSink<HotkeyEvent>>) -> PortResult<()> {
        lock(&self.state).sink = Some(sink);
        Ok(())
    }

    fn register(&self, id: &HotkeyId, shortcut: &Shortcut) -> PortResult<()> {
        if shortcut.as_str().trim().is_empty() {
            return Err(Self::issue(HotkeyIssue::Invalid).into());
        }
        let mut state = lock(&self.state);
        let taken_by_other_binding = state
            .bindings
            .iter()
            .any(|(bound, combo)| bound != id && combo == shortcut);
        if state.occupied.contains(shortcut) || taken_by_other_binding {
            return Err(Self::issue(HotkeyIssue::Conflict).into());
        }
        state.bindings.insert(id.clone(), shortcut.clone());
        Ok(())
    }

    fn unregister(&self, id: &HotkeyId) -> PortResult<()> {
        lock(&self.state).bindings.remove(id);
        Ok(())
    }

    fn refresh(&self) -> PortResult<()> {
        lock(&self.state).refreshes += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ports::fakes::RecordingSink, types::PortError};

    const RECORD: HotkeyId = HotkeyId::from_static("record");
    const CANCEL: HotkeyId = HotkeyId::from_static("cancel");
    const DEFAULT: Shortcut = Shortcut::from_static("Ctrl+Alt+Space");

    #[test]
    fn bound_ids_emit_presses_and_releases() {
        let hotkeys = FakeHotkeyService::default();
        let sink = Arc::new(RecordingSink::default());
        hotkeys.listen(sink.clone()).unwrap();
        assert!(!hotkeys.press(&RECORD));
        hotkeys.register(&RECORD, &DEFAULT).unwrap();
        assert!(hotkeys.press(&RECORD));
        assert!(hotkeys.release(&RECORD));
        assert_eq!(
            sink.events(),
            [
                HotkeyEvent {
                    id: RECORD,
                    state: KeyState::Pressed
                },
                HotkeyEvent {
                    id: RECORD,
                    state: KeyState::Released
                },
            ]
        );
    }

    #[test]
    fn interruptions_need_modifier_only_caps() {
        let hotkeys = FakeHotkeyService::new(HotkeyCaps {
            supports_release: true,
            supports_modifier_only: true,
        });
        let sink = Arc::new(RecordingSink::default());
        hotkeys.listen(sink.clone()).unwrap();
        hotkeys
            .register(&RECORD, &Shortcut::from_static("Ctrl+Alt"))
            .unwrap();
        assert!(hotkeys.interrupt(&RECORD));
        assert_eq!(
            sink.events(),
            [HotkeyEvent {
                id: RECORD,
                state: KeyState::Interrupted
            }]
        );
        assert!(!FakeHotkeyService::default().interrupt(&RECORD));
    }

    #[test]
    fn a_conflict_keeps_the_previous_binding() {
        let hotkeys = FakeHotkeyService::default();
        hotkeys.register(&RECORD, &DEFAULT).unwrap();
        let taken = Shortcut::from_static("Ctrl+Shift+Space");
        hotkeys.occupy(taken.clone());
        let conflict = hotkeys
            .register(&RECORD, &taken)
            .map_err(PortError::into_app_error);
        assert_eq!(
            conflict,
            Err(AppError::Hotkey {
                reason: HotkeyIssue::Conflict
            })
        );
        assert_eq!(hotkeys.binding(&RECORD), Some(DEFAULT));
        assert!(hotkeys.register(&CANCEL, &DEFAULT).is_err());
        assert!(
            hotkeys
                .register(&CANCEL, &Shortcut::from_static(" "))
                .is_err()
        );
    }

    #[test]
    fn unregister_is_idempotent_and_releases_need_caps() {
        let hotkeys = FakeHotkeyService::new(HotkeyCaps {
            supports_release: false,
            supports_modifier_only: false,
        });
        hotkeys.listen(Arc::new(RecordingSink::default())).unwrap();
        hotkeys
            .register(&CANCEL, &Shortcut::from_static("Esc"))
            .unwrap();
        assert!(!hotkeys.release(&CANCEL));
        assert!(!hotkeys.interrupt(&CANCEL));
        hotkeys.unregister(&CANCEL).unwrap();
        hotkeys.unregister(&CANCEL).unwrap();
        assert!(!hotkeys.press(&CANCEL));
        hotkeys.refresh().unwrap();
        assert_eq!(hotkeys.refreshes(), 1);
    }
}
