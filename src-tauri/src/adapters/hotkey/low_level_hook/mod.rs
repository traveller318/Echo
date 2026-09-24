/*!
 * SOURCE OF TRUTH KEYWORDS: LowLevelKeyboardHotkeys, low-level keyboard hook, WH_KEYBOARD_LL adapter, hold-to-talk, modifier-only hotkey, Ctrl+Alt, hotkey binding table, key release
 * WHAT:  LowLevelKeyboardHotkeys: HotkeyService on a WH_KEYBOARD_LL hook. Keeps the table of registry hotkey ids →
 *        chords, publishes it to the hook thread, and reports Pressed, Released and Interrupted for every binding.
 *        Caps: real key-up (hold-to-talk) and modifier-only combinations (Ctrl+Alt).
 * WHY:   RegisterHotKey cannot bind a combination of modifiers alone and reports no key-up (the global-shortcut
 *        plugin polled for it), so hold-to-talk on Ctrl+Alt needs a hook (05 W9). The hook sees every key before
 *        other apps' hotkeys do, so a combination another app registered does not block Echo: Echo's binding wins
 *        and its main key is swallowed. Two Echo hotkeys still cannot share one combination (`Hotkey { conflict }`),
 *        and text that cannot be parsed is `Hotkey { invalid }` with the previous binding kept. The hook is
 *        installed only while at least one binding exists, so no keyboard hook runs before the session binds its
 *        hotkeys or after everything is unbound; `refresh` reinstalls it, because Windows can silently drop a hook
 *        after sleep or a callback timeout (05 W8). The hook cannot see keys typed into an elevated window (UIPI);
 *        a take can still be stopped there with the pill's stop button, and the paste into such a window is refused
 *        anyway (05 W2).
 * WHERE: Built by app/bootstrap into CommandCtx and the session actor; driven through `dyn HotkeyService` by
 *        pipeline/hotkeys.rs. Parsing in chord.rs, matching in tracker.rs, Win32 plumbing in hook_thread.rs.
 */

mod chord;
mod hook_thread;
mod tracker;

use std::{collections::BTreeMap, sync::Arc};

use parking_lot::{Mutex, RwLock};

use self::{
    chord::{Chord, parse},
    hook_thread::HookThread,
    tracker::HookBinding,
};
use crate::{
    ports::{EventSink, HotkeyService},
    types::{
        AppError, HotkeyCaps, HotkeyEvent, HotkeyId, HotkeyIssue, PortError, PortResult, Shortcut,
    },
};

/// What the hook thread reads on every key event.
#[derive(Default)]
struct Shared {
    sink: Mutex<Option<Arc<dyn EventSink<HotkeyEvent>>>>,
    /// Swapped whole on every change, so the callback clones an Arc instead of a table.
    bindings: RwLock<Arc<[HookBinding]>>,
}

/// One bound hotkey: the text the user chose and the chord it parses to.
struct Bound {
    shortcut: Shortcut,
    chord: Chord,
}

/// System-wide hotkeys through a low-level keyboard hook.
pub struct LowLevelKeyboardHotkeys {
    shared: Arc<Shared>,
    table: Mutex<BTreeMap<HotkeyId, Bound>>,
    /// The hook thread, while at least one binding exists.
    thread: Mutex<Option<HookThread>>,
}

impl Default for LowLevelKeyboardHotkeys {
    fn default() -> Self {
        Self::new()
    }
}

impl LowLevelKeyboardHotkeys {
    pub fn new() -> Self {
        Self {
            shared: Arc::default(),
            table: Mutex::default(),
            thread: Mutex::default(),
        }
    }

    /// Hands the current table to the hook thread.
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

    /// Starts the hook thread unless it runs.
    fn ensure_hook(&self) -> PortResult<()> {
        let mut thread = self.thread.lock();
        if thread.is_none() {
            *thread = Some(HookThread::start(Arc::clone(&self.shared))?);
        }
        Ok(())
    }
}

impl HotkeyService for LowLevelKeyboardHotkeys {
    fn caps(&self) -> HotkeyCaps {
        HotkeyCaps {
            supports_release: true,
            supports_modifier_only: true,
        }
    }

    fn listen(&self, sink: Arc<dyn EventSink<HotkeyEvent>>) -> PortResult<()> {
        *self.shared.sink.lock() = Some(sink);
        Ok(())
    }

    fn register(&self, id: &HotkeyId, shortcut: &Shortcut) -> PortResult<()> {
        let chord = parse(shortcut)?;
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
        if let Err(error) = self.ensure_hook() {
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
            // No binding left: remove the hook (dropping the thread unhooks and joins it).
            self.thread.lock().take();
        }
        Ok(())
    }

    fn refresh(&self) -> PortResult<()> {
        let table = self.table.lock();
        let mut thread = self.thread.lock();
        if table.is_empty() {
            return Ok(());
        }
        // Stop the old hook before installing the new one, so no key is ever seen twice.
        thread.take();
        *thread = Some(HookThread::start(Arc::clone(&self.shared))?);
        tracing::debug!(
            bindings = table.len(),
            shortcuts = ?table.values().map(|bound| bound.shortcut.as_str()).collect::<Vec<_>>(),
            "the keyboard hook was reinstalled"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, SendInput, VIRTUAL_KEY, VK_F20, VK_LCONTROL, VK_LMENU, VK_LSHIFT,
    };

    use super::*;
    use crate::{
        adapters::win32::{KeyStroke, keyboard_input},
        ports::fakes::RecordingSink,
        types::KeyState,
    };

    const RECORD: HotkeyId = HotkeyId::from_static("record");
    const PASTE_LAST: HotkeyId = HotkeyId::from_static("paste-last");
    /// A combination no real app binds, so the live test never fights the developer's own hotkeys.
    const FREE: Shortcut = Shortcut::from_static("Ctrl+Alt+Shift+F20");

    fn issue(result: PortResult<()>) -> Option<HotkeyIssue> {
        match result.map_err(PortError::into_app_error) {
            Err(AppError::Hotkey { reason }) => Some(reason),
            _ => None,
        }
    }

    #[test]
    fn caps_declare_release_and_modifier_only() {
        assert_eq!(
            LowLevelKeyboardHotkeys::new().caps(),
            HotkeyCaps {
                supports_release: true,
                supports_modifier_only: true,
            }
        );
    }

    #[test]
    fn invalid_text_and_shared_combinations_are_refused_and_the_old_binding_stays() {
        let hotkeys = LowLevelKeyboardHotkeys::new();
        assert_eq!(
            issue(hotkeys.register(&RECORD, &Shortcut::from_static("Ctrl"))),
            Some(HotkeyIssue::Invalid)
        );
        assert!(hotkeys.thread.lock().is_none(), "nothing bound, no hook");
        hotkeys.register(&RECORD, &FREE).unwrap();
        assert_eq!(
            issue(hotkeys.register(&RECORD, &Shortcut::from_static("Hyper+Q"))),
            Some(HotkeyIssue::Invalid)
        );
        assert_eq!(
            issue(hotkeys.register(&PASTE_LAST, &Shortcut::from_static("ctrl+shift+alt+f20"))),
            Some(HotkeyIssue::Conflict),
            "two Echo hotkeys cannot share a combination, however it is spelled"
        );
        assert_eq!(
            hotkeys
                .table
                .lock()
                .get(&RECORD)
                .map(|bound| bound.shortcut.clone()),
            Some(FREE)
        );
        hotkeys.unregister(&RECORD).unwrap();
        hotkeys.unregister(&RECORD).unwrap();
        assert!(
            hotkeys.thread.lock().is_none(),
            "the last unbind removes the hook"
        );
        hotkeys.refresh().unwrap();
        assert!(
            hotkeys.thread.lock().is_none(),
            "refresh with nothing bound installs nothing"
        );
    }

    fn send_untagged(strokes: &[KeyStroke]) {
        let inputs: Vec<INPUT> = strokes
            .iter()
            .map(|stroke| {
                let mut input = keyboard_input(*stroke);
                // Only the tag is cleared, as a real keyboard sends it.
                input.Anonymous.ki.dwExtraInfo = 0;
                input
            })
            .collect();
        let size = i32::try_from(size_of::<INPUT>()).unwrap();
        // SAFETY: `inputs` holds initialised keyboard INPUTs and `size` is the size of one.
        let sent = unsafe { SendInput(&inputs, size) };
        assert_eq!(usize::try_from(sent).unwrap(), inputs.len());
    }

    fn wait_for(sink: &RecordingSink<HotkeyEvent>, count: usize) -> Vec<HotkeyEvent> {
        for _ in 0..100 {
            let events = sink.events();
            if events.len() >= count {
                return events;
            }
            thread::sleep(Duration::from_millis(10));
        }
        sink.events()
    }

    /// Installs the real hook and types Ctrl+Alt+Shift+F20 with SendInput: untagged keys fire the binding (F20
    /// is swallowed, the modifiers reach whatever window is in front and do nothing there), Echo-tagged ones
    /// are ignored.
    #[test]
    fn the_real_hook_reports_press_and_release_and_skips_echo_keys() {
        let hotkeys = LowLevelKeyboardHotkeys::new();
        let sink = Arc::new(RecordingSink::default());
        hotkeys.listen(sink.clone()).unwrap();
        hotkeys.register(&RECORD, &FREE).unwrap();
        let chord: [VIRTUAL_KEY; 4] = [VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_F20];
        let press: Vec<KeyStroke> = chord.iter().copied().map(KeyStroke::down).collect();
        let release: Vec<KeyStroke> = chord.iter().rev().copied().map(KeyStroke::up).collect();

        // Echo's own synthetic keys are never matched.
        let tagged: Vec<INPUT> = press
            .iter()
            .chain(&release)
            .copied()
            .map(keyboard_input)
            .collect();
        // SAFETY: tagged keyboard INPUTs, as the inserter sends them.
        unsafe { SendInput(&tagged, i32::try_from(size_of::<INPUT>()).unwrap()) };
        thread::sleep(Duration::from_millis(100));
        assert!(sink.events().is_empty(), "{:?}", sink.events());

        send_untagged(&press);
        send_untagged(&release);
        let events = wait_for(&sink, 2);
        assert_eq!(
            events,
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
        hotkeys.refresh().unwrap();
        hotkeys.unregister(&RECORD).unwrap();
    }
}
