/*!
 * SOURCE OF TRUTH KEYWORDS: TauriGlobalShortcut, tauri-plugin-global-shortcut, RegisterHotKey, hotkey conflict, ERROR_HOTKEY_ALREADY_REGISTERED, key release polling, hotkey binding table, main thread
 * WHAT:  TauriGlobalShortcut: HotkeyService on the Tauri global-shortcut plugin (RegisterHotKey underneath).
 *        Keeps a table of registry hotkey ids → combinations and turns the plugin's press and release events for
 *        the current bindings into HotkeyEvents on the listening sink.
 * WHY:   RegisterHotKey failing because another app owns the combination is silent unless checked (05 W7): the
 *        plugin's error is text, so "already registered" is recognised by the pinned global-hotkey crate's message
 *        (a test builds that exact error) and becomes `Hotkey { conflict }`; an unparsable combination or a key
 *        with no virtual-key code is `Hotkey { invalid }`. A new combination is registered before the old one is
 *        released, so a failed rebind keeps the previous binding (ports/hotkey.rs contract). Releases are real
 *        but polled: the plugin watches the main key every 50 ms after a press, so `supports_release` is declared
 *        and hold mode's reliability is verified by hand in step 19 (05 W9). Modifier-only combinations cannot be
 *        registered. The plugin runs every registration on the event-loop thread and waits for it; this adapter
 *        therefore never holds its own lock across a plugin call, so a call made on that thread (a tray action)
 *        cannot deadlock one made elsewhere. Event handlers hold only a weak reference, so events for a dropped
 *        adapter, or for a combination that was just rebound, are ignored. The event sink must not call back into
 *        the hotkey service synchronously: the plugin holds its own lock while it runs the handler.
 * WHERE: Built by app/bootstrap into CommandCtx (the plugin is registered in app/plugins.rs); driven through
 *        `dyn HotkeyService` by pipeline/hotkeys.rs; its events feed the session actor.
 */

use std::{
    collections::BTreeMap,
    str::FromStr,
    sync::{Arc, Weak},
};

use parking_lot::Mutex;
use tauri::{AppHandle, Manager, Runtime, State};
use tauri_plugin_global_shortcut::{
    Error as PluginError, GlobalShortcut, Shortcut as Combination, ShortcutEvent, ShortcutState,
};

use crate::{
    ports::{EventSink, HotkeyService},
    types::{
        AppError, HotkeyCaps, HotkeyEvent, HotkeyId, HotkeyIssue, KeyState, PortError, PortResult,
        Shortcut,
    },
};

/// How global-hotkey 0.8 words a RegisterHotKey failure with ERROR_HOTKEY_ALREADY_REGISTERED.
const ALREADY_REGISTERED: &str = "HotKey already registered";

/// One bound hotkey: the text the user chose and the combination the plugin registered.
struct Binding {
    shortcut: Shortcut,
    combination: Combination,
}

/// What the plugin's event handlers share with the adapter.
#[derive(Default)]
struct Shared {
    sink: Mutex<Option<Arc<dyn EventSink<HotkeyEvent>>>>,
    bindings: Mutex<BTreeMap<HotkeyId, Binding>>,
}

impl Shared {
    /// Forwards a plugin event for `id` when `combination` is still what `id` is bound to.
    fn dispatch(&self, id: &HotkeyId, combination: &Combination, state: ShortcutState) {
        let current = self
            .bindings
            .lock()
            .get(id)
            .is_some_and(|binding| binding.combination == *combination);
        if !current {
            return;
        }
        let sink = self.sink.lock().clone();
        if let Some(sink) = sink {
            sink.emit(HotkeyEvent {
                id: id.clone(),
                state: match state {
                    ShortcutState::Pressed => KeyState::Pressed,
                    ShortcutState::Released => KeyState::Released,
                },
            });
        }
    }
}

/// System-wide hotkeys through the Tauri global-shortcut plugin.
pub struct TauriGlobalShortcut<R: Runtime> {
    app: AppHandle<R>,
    shared: Arc<Shared>,
}

impl<R: Runtime> TauriGlobalShortcut<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self {
            app,
            shared: Arc::default(),
        }
    }

    fn plugin(&self) -> PortResult<State<'_, GlobalShortcut<R>>> {
        self.app.try_state::<GlobalShortcut<R>>().ok_or_else(|| {
            PortError::new(AppError::Internal)
                .with_detail("the global-shortcut plugin is not registered (app/plugins.rs)")
        })
    }

    /// Registers `combination` with the plugin, reporting its events as `id`.
    fn bind(
        &self,
        plugin: &GlobalShortcut<R>,
        id: &HotkeyId,
        shortcut: &Shortcut,
        combination: Combination,
    ) -> PortResult<()> {
        let shared: Weak<Shared> = Arc::downgrade(&self.shared);
        let owner = id.clone();
        plugin
            .on_shortcut(combination, move |_app, pressed, event: ShortcutEvent| {
                if let Some(shared) = shared.upgrade() {
                    shared.dispatch(&owner, pressed, event.state);
                }
            })
            .map_err(|error| registration_error(&error, shortcut))
    }
}

impl<R: Runtime> HotkeyService for TauriGlobalShortcut<R> {
    fn caps(&self) -> HotkeyCaps {
        HotkeyCaps {
            supports_release: true,
            supports_modifier_only: false,
        }
    }

    fn listen(&self, sink: Arc<dyn EventSink<HotkeyEvent>>) -> PortResult<()> {
        *self.shared.sink.lock() = Some(sink);
        Ok(())
    }

    fn register(&self, id: &HotkeyId, shortcut: &Shortcut) -> PortResult<()> {
        let combination = parse(shortcut)?;
        {
            let mut bindings = self.shared.bindings.lock();
            if let Some((owner, _)) = bindings
                .iter()
                .find(|(bound, binding)| *bound != id && binding.combination == combination)
            {
                return Err(PortError::new(AppError::Hotkey {
                    reason: HotkeyIssue::Conflict,
                })
                .with_detail(format!("{shortcut} is already Echo's {owner} hotkey")));
            }
            if let Some(binding) = bindings
                .get_mut(id)
                .filter(|binding| binding.combination == combination)
            {
                // The same keys spelled differently: nothing to register again.
                binding.shortcut = shortcut.clone();
                return Ok(());
            }
        }
        let plugin = self.plugin()?;
        self.bind(&plugin, id, shortcut, combination)?;
        let previous = self.shared.bindings.lock().insert(
            id.clone(),
            Binding {
                shortcut: shortcut.clone(),
                combination,
            },
        );
        if let Some(previous) = previous {
            release(&plugin, id, &previous);
        }
        Ok(())
    }

    fn unregister(&self, id: &HotkeyId) -> PortResult<()> {
        let removed = self.shared.bindings.lock().remove(id);
        match removed {
            Some(binding) => {
                release(&*self.plugin()?, id, &binding);
                Ok(())
            }
            None => Ok(()),
        }
    }

    fn refresh(&self) -> PortResult<()> {
        let plugin = self.plugin()?;
        let current: Vec<(HotkeyId, Shortcut, Combination)> = self
            .shared
            .bindings
            .lock()
            .iter()
            .map(|(id, binding)| (id.clone(), binding.shortcut.clone(), binding.combination))
            .collect();
        let mut failures = Vec::new();
        for (id, shortcut, combination) in current {
            // Windows may have dropped the registration (sleep, session switch, 05 W8); a failed release only means
            // there was nothing left to release.
            let _ = plugin.unregister(combination);
            if let Err(error) = self.bind(&plugin, &id, &shortcut, combination) {
                // The binding is gone at the OS level, so the table must not claim it.
                self.shared.bindings.lock().remove(&id);
                failures.push((id, error));
            }
        }
        let mut failures = failures.into_iter();
        match failures.next() {
            None => Ok(()),
            Some((id, first)) => {
                let others: Vec<String> = failures.map(|(other, _)| other.to_string()).collect();
                let detail = format!(
                    "re-registering {id} failed ({}); also failed: [{}]",
                    first.detail().unwrap_or("no detail"),
                    others.join(", ")
                );
                Err(PortError::new(first.into_app_error()).with_detail(detail))
            }
        }
    }
}

/// The plugin combination for `shortcut`, or `Hotkey { invalid }`.
fn parse(shortcut: &Shortcut) -> PortResult<Combination> {
    Combination::from_str(shortcut.as_str()).map_err(|error| {
        PortError::new(AppError::Hotkey {
            reason: HotkeyIssue::Invalid,
        })
        .with_detail(format!("{shortcut:?} cannot be parsed: {error}"))
    })
}

/// Releases a binding that was replaced or removed; a failure is logged, since the combination is no longer ours.
fn release<R: Runtime>(plugin: &GlobalShortcut<R>, id: &HotkeyId, binding: &Binding) {
    if let Err(error) = plugin.unregister(binding.combination) {
        tracing::warn!(
            hotkey = %id,
            shortcut = %binding.shortcut,
            detail = %error,
            "a released hotkey could not be unregistered"
        );
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: registration_error, hotkey conflict mapping, global-hotkey error text
 * WHAT:  Maps a plugin registration failure to `Hotkey { conflict }` (another app owns it), `Hotkey { invalid }`
 *        (the OS or crate refused the combination) or `Internal` (the event loop is gone).
 * WHY:   The plugin flattens global-hotkey's typed errors into text; the pinned crate's wording is the only signal
 *        left, and the test below builds the real error so a crate update that rewords it fails the gate.
 * WHERE: TauriGlobalShortcut::bind.
 */
fn registration_error(error: &PluginError, shortcut: &Shortcut) -> PortError {
    let reason = match error {
        PluginError::GlobalHotkey(message) if message.starts_with(ALREADY_REGISTERED) => {
            HotkeyIssue::Conflict
        }
        PluginError::GlobalHotkey(_) => HotkeyIssue::Invalid,
        _ => {
            return PortError::new(AppError::Internal)
                .with_detail(format!("registering {shortcut} failed: {error}"));
        }
    };
    PortError::new(AppError::Hotkey { reason })
        .with_detail(format!("registering {shortcut} failed: {error}"))
}

#[cfg(test)]
mod tests {
    use global_hotkey::hotkey::{Code, Modifiers};
    use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey, UnregisterHotKey, VK_F21,
    };

    use super::*;
    use crate::ports::fakes::RecordingSink;

    const RECORD: HotkeyId = HotkeyId::from_static("record");
    const PASTE_LAST: HotkeyId = HotkeyId::from_static("paste-last");
    /// Combinations no real app binds, so the live test never fights the developer's own hotkeys.
    const FREE: Shortcut = Shortcut::from_static("Ctrl+Alt+Shift+F20");
    const OCCUPIED: Shortcut = Shortcut::from_static("Ctrl+Alt+Shift+F21");
    /// The id the test's stand-in "other app" registers OCCUPIED under.
    const OTHER_APP_ID: i32 = 0x0E40;

    fn issue(result: PortResult<()>) -> Option<HotkeyIssue> {
        match result.map_err(PortError::into_app_error) {
            Err(AppError::Hotkey { reason }) => Some(reason),
            _ => None,
        }
    }

    #[test]
    fn plugin_errors_map_to_conflict_invalid_or_internal() {
        let combination = Combination::new(Some(Modifiers::CONTROL), Code::F20);
        let taken = PluginError::from(global_hotkey::Error::AlreadyRegistered(combination));
        assert_eq!(
            registration_error(&taken, &FREE).into_app_error(),
            AppError::Hotkey {
                reason: HotkeyIssue::Conflict
            }
        );
        let unknown = PluginError::from(global_hotkey::Error::FailedToRegister(String::from(
            "Unknown VKCode for F99",
        )));
        assert_eq!(
            registration_error(&unknown, &FREE).into_app_error(),
            AppError::Hotkey {
                reason: HotkeyIssue::Invalid
            }
        );
        let gone = PluginError::from(tauri::Error::FailedToReceiveMessage);
        assert_eq!(
            registration_error(&gone, &FREE).into_app_error(),
            AppError::Internal
        );
    }

    #[test]
    fn unparsable_and_modifier_only_combinations_are_invalid() {
        for text in ["", "Ctrl+Alt", "Ctrl+Space+Alt", "Hyper+Q"] {
            assert_eq!(
                parse(&Shortcut::from(text.to_owned()))
                    .map(|_| ())
                    .map_err(PortError::into_app_error),
                Err(AppError::Hotkey {
                    reason: HotkeyIssue::Invalid
                }),
                "{text}"
            );
        }
        assert!(parse(&Shortcut::from_static("Escape")).is_ok());
        assert_eq!(
            parse(&Shortcut::from_static("ctrl+alt+space")).unwrap(),
            parse(&Shortcut::from_static("Ctrl+Alt+Space")).unwrap()
        );
    }

    #[test]
    fn events_reach_the_sink_only_for_the_current_binding() {
        let shared = Shared::default();
        let sink = Arc::new(RecordingSink::default());
        *shared.sink.lock() = Some(sink.clone());
        let bound = parse(&FREE).unwrap();
        let stale = parse(&OCCUPIED).unwrap();
        shared.bindings.lock().insert(
            RECORD,
            Binding {
                shortcut: FREE,
                combination: bound,
            },
        );
        shared.dispatch(&RECORD, &bound, ShortcutState::Pressed);
        shared.dispatch(&RECORD, &stale, ShortcutState::Pressed);
        shared.dispatch(&PASTE_LAST, &bound, ShortcutState::Pressed);
        shared.dispatch(&RECORD, &bound, ShortcutState::Released);
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

    /// Registers real system hotkeys through the plugin on a mock app (whose event loop runs tasks inline), with a
    /// raw RegisterHotKey standing in for another app that owns a combination (05 W7).
    #[test]
    fn real_registration_conflicts_rebinds_and_releases() {
        let app = mock_builder()
            .plugin(tauri_plugin_global_shortcut::Builder::<MockRuntime>::new().build())
            .build(mock_context(noop_assets()))
            .unwrap();
        let hotkeys = TauriGlobalShortcut::new(app.handle().clone());
        let plugin = hotkeys.plugin().unwrap();
        // SAFETY: a thread-level registration (no window) with plain value arguments; undone at the end.
        unsafe {
            RegisterHotKey(
                None,
                OTHER_APP_ID,
                MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT,
                u32::from(VK_F21.0),
            )
        }
        .unwrap();

        hotkeys.register(&RECORD, &FREE).unwrap();
        assert!(plugin.is_registered(FREE.as_str()));
        assert_eq!(
            issue(hotkeys.register(&RECORD, &OCCUPIED)),
            Some(HotkeyIssue::Conflict)
        );
        assert!(
            plugin.is_registered(FREE.as_str()),
            "a failed rebind keeps the previous binding"
        );
        assert_eq!(
            issue(hotkeys.register(&PASTE_LAST, &FREE)),
            Some(HotkeyIssue::Conflict),
            "two Echo hotkeys cannot share a combination"
        );
        hotkeys
            .register(&RECORD, &Shortcut::from_static("ctrl+alt+shift+f20"))
            .unwrap();
        hotkeys.refresh().unwrap();
        assert!(plugin.is_registered(FREE.as_str()));

        hotkeys.unregister(&RECORD).unwrap();
        hotkeys.unregister(&RECORD).unwrap();
        assert!(!plugin.is_registered(FREE.as_str()));
        // SAFETY: undoes the registration above on the same thread.
        unsafe { UnregisterHotKey(None, OTHER_APP_ID) }.unwrap();
        hotkeys.register(&RECORD, &OCCUPIED).unwrap();
        hotkeys.unregister(&RECORD).unwrap();
        assert_eq!(
            hotkeys.caps(),
            HotkeyCaps {
                supports_release: true,
                supports_modifier_only: false,
            }
        );
    }
}
