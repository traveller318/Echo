/*!
 * SOURCE OF TRUTH KEYWORDS: ChordHotkeys, KeyCapture, Dispatcher, Shared, register conflict, hotkey table publish, capture thread lifecycle, refresh
 * WHAT:  `ChordHotkeys<C>`: HotkeyService over any `KeyCapture`. It keeps the table of bound chords (rejecting text
 *        that does not parse as `Invalid` and a chord another Echo hotkey already owns as `Conflict`), starts the
 *        capture while at least one binding exists and stops it with the last one, and hands the capture a `Shared`
 *        (the sink and the current bindings). `Dispatcher` is what a capture's thread owns: it feeds key events into
 *        the ChordTracker under the current bindings and emits the resulting HotkeyEvents.
 * WHY:   The same rules as the Windows adapter (low_level_hook/mod.rs), written once for every Linux backend:
 *        nothing is captured until something is bound, a failed rebind keeps the previous binding, and `refresh`
 *        restarts the capture (it re-reads devices after a sleep or an unplug) before the old one is gone, so no key
 *        is ever seen twice. Bindings are swapped whole into an `Arc`, so the capture thread clones a pointer per key.
 * WHERE: Built by app/platform.rs on Linux over X11XInput or Evdev; `Dispatcher` by those captures.
 */

use std::{collections::BTreeMap, sync::Arc};

use parking_lot::{Mutex, RwLock};

use super::{
    chord::{Chord, parse},
    tracker::{ChordTracker, HookBinding, KeyEvent},
};
use crate::{
    ports::{EventSink, HotkeyService},
    types::{
        AppError, HotkeyCaps, HotkeyEvent, HotkeyId, HotkeyIssue, PortError, PortResult, Shortcut,
    },
};

/// What a capture thread reads on every key.
#[derive(Default)]
pub struct Shared {
    sink: Mutex<Option<Arc<dyn EventSink<HotkeyEvent>>>>,
    bindings: RwLock<Arc<[HookBinding]>>,
}

/// A way of watching the keyboard. `start` returns the running capture; dropping it stops and joins its thread.
pub trait KeyCapture: Send + Sync + 'static {
    type Running: Send + 'static;

    /// What this capture can report (release events, modifier-only chords).
    fn caps(&self) -> HotkeyCaps;

    /// Starts watching and feeds a `Dispatcher` over `shared`; fails when the keyboard cannot be watched.
    fn start(&self, shared: Arc<Shared>) -> PortResult<Self::Running>;
}

/// Feeds one capture's key events through the tracker and out to the sink.
pub struct Dispatcher {
    shared: Arc<Shared>,
    tracker: ChordTracker,
}

impl Dispatcher {
    pub fn new(shared: Arc<Shared>) -> Self {
        Self {
            shared,
            tracker: ChordTracker::default(),
        }
    }

    /// One physical key event (`vk` a Windows virtual-key code, `time` in wrapping milliseconds).
    pub fn key(&mut self, vk: u16, down: bool, time: u32) {
        let bindings = Arc::clone(&self.shared.bindings.read());
        let reaction = self.tracker.handle(KeyEvent { vk, down, time }, &bindings);
        self.emit(reaction.events);
    }

    /// Some held key might have lost its release, so `reconcile` should be called soon.
    pub fn needs_reconcile(&self) -> bool {
        self.tracker.needs_reconcile()
    }

    /// Lets go of held keys that `is_down` reports up (a release the capture never saw).
    pub fn reconcile(&mut self, now: u32, is_down: impl Fn(u16) -> bool) {
        let bindings = Arc::clone(&self.shared.bindings.read());
        let reaction = self.tracker.reconcile(now, is_down, &bindings);
        if !reaction.lost.is_empty() {
            tracing::debug!(
                keys = reaction.lost.len(),
                "held keys were released: their key-up never arrived"
            );
        }
        self.emit(reaction.events);
    }

    fn emit(&self, events: Vec<HotkeyEvent>) {
        if events.is_empty() {
            return;
        }
        if let Some(sink) = self.shared.sink.lock().as_ref() {
            for event in events {
                sink.emit(event);
            }
        }
    }
}

struct Bound {
    shortcut: Shortcut,
    chord: Chord,
}

/// System-wide hotkeys over a `KeyCapture`.
pub struct ChordHotkeys<C: KeyCapture> {
    capture: C,
    shared: Arc<Shared>,
    table: Mutex<BTreeMap<HotkeyId, Bound>>,
    running: Mutex<Option<C::Running>>,
}

impl<C: KeyCapture> ChordHotkeys<C> {
    pub fn new(capture: C) -> Self {
        Self {
            capture,
            shared: Arc::default(),
            table: Mutex::default(),
            running: Mutex::default(),
        }
    }

    fn publish(&self, table: &BTreeMap<HotkeyId, Bound>) {
        let bindings: Arc<[HookBinding]> = table
            .iter()
            .map(|(id, bound)| HookBinding {
                id: id.clone(),
                chord: bound.chord,
            })
            .collect();
        *self.shared.bindings.write() = bindings;
    }

    fn ensure_running(&self) -> PortResult<()> {
        let mut running = self.running.lock();
        if running.is_none() {
            *running = Some(self.capture.start(Arc::clone(&self.shared))?);
        }
        Ok(())
    }
}

impl<C: KeyCapture> HotkeyService for ChordHotkeys<C> {
    fn caps(&self) -> HotkeyCaps {
        self.capture.caps()
    }

    fn listen(&self, sink: Arc<dyn EventSink<HotkeyEvent>>) -> PortResult<()> {
        *self.shared.sink.lock() = Some(sink);
        Ok(())
    }

    fn register(&self, id: &HotkeyId, shortcut: &Shortcut) -> PortResult<()> {
        let chord = parse(shortcut)?;
        let caps = self.capture.caps();
        if chord.is_modifier_only() && !caps.supports_modifier_only {
            return Err(PortError::new(AppError::Hotkey {
                reason: HotkeyIssue::Invalid,
            })
            .with_detail(format!("{shortcut} needs a key besides the modifiers here")));
        }
        let mut table = self.table.lock();
        if let Some((owner, _)) = table
            .iter()
            .find(|(bound, binding)| *bound != id && binding.chord == chord)
        {
            return Err(PortError::new(AppError::Hotkey {
                reason: HotkeyIssue::Conflict,
            })
            .with_detail(format!("{shortcut} is already Echo's {owner} hotkey")));
        }
        let previous = table.insert(
            id.clone(),
            Bound {
                shortcut: shortcut.clone(),
                chord,
            },
        );
        if let Err(error) = self.ensure_running() {
            match previous {
                Some(previous) => table.insert(id.clone(), previous),
                None => table.remove(id),
            };
            return Err(error);
        }
        self.publish(&table);
        Ok(())
    }

    fn unregister(&self, id: &HotkeyId) -> PortResult<()> {
        let mut table = self.table.lock();
        if table.remove(id).is_none() {
            return Ok(());
        }
        self.publish(&table);
        if table.is_empty() {
            self.running.lock().take();
        }
        Ok(())
    }

    fn refresh(&self) -> PortResult<()> {
        let table = self.table.lock();
        if table.is_empty() {
            return Ok(());
        }
        let mut running = self.running.lock();
        // The old capture goes first, so no key is seen twice.
        running.take();
        *running = Some(self.capture.start(Arc::clone(&self.shared))?);
        tracing::debug!(
            bindings = table.len(),
            shortcuts = ?table.values().map(|bound| bound.shortcut.as_str()).collect::<Vec<_>>(),
            "the keyboard capture was restarted"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::ports::fakes::RecordingSink;
    use crate::types::KeyState;

    const RECORD: HotkeyId = HotkeyId::from_static("record");
    const PASTE_LAST: HotkeyId = HotkeyId::from_static("paste-last");

    /// The Shared a started Fake handed out, for the test to drive.
    type SharedSlot = Arc<Mutex<Option<Arc<Shared>>>>;

    /// A capture that records how often it started and hands the test its Dispatcher.
    struct Fake {
        caps: HotkeyCaps,
        starts: Arc<AtomicUsize>,
        shared: SharedSlot,
        fail: bool,
    }

    struct Guard(Arc<AtomicUsize>);

    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }

    impl KeyCapture for Fake {
        type Running = Guard;

        fn caps(&self) -> HotkeyCaps {
            self.caps
        }

        fn start(&self, shared: Arc<Shared>) -> PortResult<Guard> {
            if self.fail {
                return Err(PortError::new(AppError::Internal));
            }
            *self.shared.lock() = Some(shared);
            self.starts.fetch_add(1, Ordering::SeqCst);
            Ok(Guard(Arc::clone(&self.starts)))
        }
    }

    fn fake(modifier_only: bool, fail: bool) -> (ChordHotkeys<Fake>, Arc<AtomicUsize>, SharedSlot) {
        let starts = Arc::new(AtomicUsize::new(0));
        let shared = Arc::new(Mutex::new(None));
        let hotkeys = ChordHotkeys::new(Fake {
            caps: HotkeyCaps {
                supports_release: true,
                supports_modifier_only: modifier_only,
            },
            starts: Arc::clone(&starts),
            shared: Arc::clone(&shared),
            fail,
        });
        (hotkeys, starts, shared)
    }

    fn issue(result: PortResult<()>) -> Option<HotkeyIssue> {
        match result.map_err(PortError::into_app_error) {
            Err(AppError::Hotkey { reason }) => Some(reason),
            _ => None,
        }
    }

    #[test]
    fn capture_runs_only_while_something_is_bound() {
        let (hotkeys, running, _) = fake(true, false);
        assert_eq!(running.load(Ordering::SeqCst), 0);
        hotkeys
            .register(&RECORD, &Shortcut::from_static("Ctrl+Alt"))
            .unwrap();
        hotkeys
            .register(&PASTE_LAST, &Shortcut::from_static("Ctrl+Alt+V"))
            .unwrap();
        assert_eq!(running.load(Ordering::SeqCst), 1);
        hotkeys.unregister(&RECORD).unwrap();
        assert_eq!(running.load(Ordering::SeqCst), 1);
        hotkeys.unregister(&PASTE_LAST).unwrap();
        hotkeys.unregister(&PASTE_LAST).unwrap();
        assert_eq!(running.load(Ordering::SeqCst), 0);
        hotkeys.refresh().unwrap();
        assert_eq!(
            running.load(Ordering::SeqCst),
            0,
            "nothing bound, nothing captured"
        );
    }

    #[test]
    fn bad_text_conflicts_and_unsupported_chords_are_refused() {
        let (hotkeys, _, _) = fake(false, false);
        assert_eq!(
            issue(hotkeys.register(&RECORD, &Shortcut::from_static("Hyper+Q"))),
            Some(HotkeyIssue::Invalid)
        );
        assert_eq!(
            issue(hotkeys.register(&RECORD, &Shortcut::from_static("Ctrl+Alt"))),
            Some(HotkeyIssue::Invalid),
            "a capture without modifier-only support refuses a bare modifier chord"
        );
        hotkeys
            .register(&RECORD, &Shortcut::from_static("Ctrl+Alt+Space"))
            .unwrap();
        assert_eq!(
            issue(hotkeys.register(&PASTE_LAST, &Shortcut::from_static("alt+ctrl+space"))),
            Some(HotkeyIssue::Conflict)
        );
    }

    #[test]
    fn a_capture_that_cannot_start_keeps_the_table_unchanged() {
        let (hotkeys, _, _) = fake(true, true);
        assert!(
            hotkeys
                .register(&RECORD, &Shortcut::from_static("Ctrl+Alt"))
                .is_err()
        );
        assert!(hotkeys.table.lock().is_empty());
    }

    #[test]
    fn dispatched_keys_become_press_and_release_events() {
        let (hotkeys, _, shared) = fake(true, false);
        let sink = Arc::new(RecordingSink::default());
        hotkeys.listen(sink.clone()).unwrap();
        hotkeys
            .register(&RECORD, &Shortcut::from_static("Ctrl+Alt"))
            .unwrap();
        let mut dispatcher = Dispatcher::new(shared.lock().clone().unwrap());
        // VK_LCONTROL 0xA2, VK_LMENU 0xA4
        dispatcher.key(0xA2, true, 100);
        dispatcher.key(0xA4, true, 110);
        dispatcher.key(0xA4, false, 900);
        dispatcher.key(0xA2, false, 910);
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
}
