/*!
 * SOURCE OF TRUTH KEYWORDS: hotkey bindings, bind_always, bind_always_where, rebind_setting, unbind_always, SessionHotkeys, Esc guard, register hotkeys from registry, hotkey conflict report
 * WHAT:  Binds the registry's hotkeys through the HotkeyService port: every always-on hotkey at startup
 *        (`bind_always`, or `bind_always_where` for a subset, which report each failure and keep going), one hotkey
 *        after its setting changes (`rebind_setting`), all of them off and back on (`unbind_always`, `bind_always`),
 *        and the session-scoped ones for exactly as long as a SessionHotkeys guard lives (`abandon` at exit only).
 * WHY:   Which hotkeys exist, their defaults and scopes are registry data (registry/hotkeys); this module is the one
 *        place that turns them into registrations, so the actor, Settings, the tray and power handling cannot
 *        disagree. A conflict on one hotkey must not leave the others unbound (05 W7), so startup failures are
 *        collected, not returned early. Esc is taken from every other app while registered (05 W10): the guard
 *        registers it when a take starts recording and its Drop releases it on every exit path, errors and panics
 *        included, and a guard that fails half-way releases what it had bound. A hotkey setting is rebound with
 *        the snapshot the caller is about to store, so a conflict can refuse the write while the previous binding
 *        stays (the port keeps it); a session-scoped hotkey is never registered outside a session by a rebind.
 * WHERE: SessionHotkeys is held by the session actor from Recording until the take leaves CancelPending
 *        (pipeline/session/runner.rs); bind_always_where runs when the actor starts listening, with the actions it
 *        handles (pipeline/session/actor.rs); rebind_setting from settings_set / settings_reset; unbind_always /
 *        bind_always from the tray's Pause hotkeys and power resume (step 25).
 */

use std::sync::Arc;

use crate::{
    ports::HotkeyService,
    registry::hotkeys,
    types::{
        HotkeyBindFailure, HotkeyId, HotkeyScope, HotkeySpec, PortResult, SettingKey,
        SettingsSnapshot,
    },
};

/// Binds `spec` to the combination its setting gives (its default when it has none).
pub fn bind(
    service: &dyn HotkeyService,
    spec: &HotkeySpec,
    settings: &SettingsSnapshot,
) -> PortResult<()> {
    service.register(&spec.id, &hotkeys::effective_shortcut(spec, settings))
}

/// Binds every always-on hotkey; returns the ones that failed, while the others stay bound.
pub fn bind_always(
    service: &dyn HotkeyService,
    settings: &SettingsSnapshot,
) -> Vec<HotkeyBindFailure> {
    bind_always_where(service, settings, |_| true)
}

/// Binds the always-on hotkeys `include` accepts (the session actor: those whose action it handles); returns the
/// ones that failed, while the others stay bound.
pub fn bind_always_where(
    service: &dyn HotkeyService,
    settings: &SettingsSnapshot,
    include: impl Fn(&HotkeySpec) -> bool,
) -> Vec<HotkeyBindFailure> {
    hotkeys::in_scope(HotkeyScope::Always)
        .filter(|spec| include(spec))
        .filter_map(|spec| {
            let shortcut = hotkeys::effective_shortcut(spec, settings);
            let error = service.register(&spec.id, &shortcut).err()?;
            tracing::warn!(
                hotkey = %spec.id,
                %shortcut,
                code = error.error().code().as_str(),
                detail = error.detail(),
                "a hotkey could not be bound"
            );
            Some(HotkeyBindFailure {
                id: spec.id.clone(),
                shortcut,
                error,
            })
        })
        .collect()
}

/// Releases every always-on hotkey (the tray's Pause hotkeys).
pub fn unbind_always(service: &dyn HotkeyService) -> PortResult<()> {
    hotkeys::in_scope(HotkeyScope::Always).try_for_each(|spec| service.unregister(&spec.id))
}

/**
 * SOURCE OF TRUTH KEYWORDS: rebind_setting, live hotkey rebind, hotkey setting change, keep previous binding
 * WHAT:  Rebinds the always-on hotkey `key` controls to its value in `settings`, if `include` accepts it (the
 *        session: only hotkeys whose action it handles); None when nothing was rebound. On failure the previous
 *        binding stays (the port's contract).
 * WHY:   A new combination takes effect the moment it is saved, and the settings write is refused when the port
 *        cannot bind it (05 W7), so the stored value and the live binding never disagree. A hotkey nobody handles
 *        yet is not registered, so it never takes a combination from other apps while pressing it does nothing; its
 *        stored value is bound when a handler binds it.
 * WHERE: ipc/commands/settings.rs (`settings_set` / `settings_reset`, with pipeline::session::binds_hotkey), which
 *        also calls it with the previous snapshot to put the old binding back when the row cannot be written.
 */
pub fn rebind_setting(
    service: &dyn HotkeyService,
    key: &SettingKey,
    settings: &SettingsSnapshot,
    include: impl Fn(&HotkeySpec) -> bool,
) -> Option<PortResult<()>> {
    let spec = hotkeys::for_setting(key)
        .filter(|spec| spec.scope == HotkeyScope::Always && include(spec))?;
    Some(bind(service, spec, settings))
}

/// The session-scoped hotkeys (Esc), registered while this guard lives.
pub struct SessionHotkeys {
    service: Arc<dyn HotkeyService>,
    bound: Vec<HotkeyId>,
}

impl SessionHotkeys {
    /// Registers every session-scoped hotkey; on failure nothing stays registered.
    pub fn bind(service: Arc<dyn HotkeyService>, settings: &SettingsSnapshot) -> PortResult<Self> {
        let mut guard = Self {
            service,
            bound: Vec::new(),
        };
        for spec in hotkeys::in_scope(HotkeyScope::DuringSession) {
            bind(guard.service.as_ref(), spec, settings)?;
            guard.bound.push(spec.id.clone());
        }
        Ok(guard)
    }

    /// Drops the guard without releasing: only at process exit, when Windows removes the process's keyboard hook anyway
    /// and nothing should wait on a hotkey backend that is shutting down.
    pub fn abandon(mut self) {
        self.bound.clear();
    }
}

impl Drop for SessionHotkeys {
    fn drop(&mut self) {
        for id in self.bound.drain(..) {
            if let Err(error) = self.service.unregister(&id) {
                tracing::warn!(
                    hotkey = %id,
                    detail = error.detail(),
                    "a session hotkey could not be released"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ports::fakes::FakeHotkeyService,
        registry::{
            hotkeys::{CANCEL, PASTE_LAST, PASTE_LAST_DEFAULT, RECORD, RECORD_DEFAULT},
            settings::{self, keys},
        },
        types::{AppError, HotkeyAction, HotkeyIssue, SettingValue, Shortcut, StaticStr},
    };

    fn record_bound_to(combination: &'static str) -> SettingsSnapshot {
        settings::resolve([(
            keys::RECORD_HOTKEY,
            SettingValue::Hotkey(StaticStr::new(combination)),
        )])
    }

    fn conflict() -> AppError {
        AppError::Hotkey {
            reason: HotkeyIssue::Conflict,
        }
    }

    #[test]
    fn startup_binds_the_always_on_hotkeys_at_their_settings() {
        let service = FakeHotkeyService::default();
        let failures = bind_always(&service, &record_bound_to("Ctrl+Shift+F9"));
        assert!(failures.is_empty(), "{failures:?}");
        assert_eq!(
            service.binding(&RECORD),
            Some(Shortcut::from_static("Ctrl+Shift+F9"))
        );
        assert_eq!(
            service.binding(&PASTE_LAST),
            Some(Shortcut::from_static(PASTE_LAST_DEFAULT))
        );
        assert_eq!(
            service.binding(&CANCEL),
            None,
            "Esc waits for a take (05 W10)"
        );
    }

    #[test]
    fn a_conflict_is_reported_and_the_other_hotkeys_still_bind() {
        let service = FakeHotkeyService::default();
        service.occupy(Shortcut::from_static(RECORD_DEFAULT));
        let failures = bind_always(&service, &settings::defaults());
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].id, RECORD);
        assert_eq!(failures[0].shortcut, Shortcut::from_static(RECORD_DEFAULT));
        assert_eq!(failures[0].error.error(), &conflict());
        assert_eq!(
            service.binding(&PASTE_LAST),
            Some(Shortcut::from_static(PASTE_LAST_DEFAULT))
        );
    }

    #[test]
    fn a_hotkey_setting_rebinds_and_a_conflict_keeps_the_old_binding() {
        let service = FakeHotkeyService::default();
        assert!(bind_always(&service, &settings::defaults()).is_empty());
        let moved = record_bound_to("Ctrl+Alt+F8");
        assert_eq!(
            rebind_setting(&service, &keys::RECORD_HOTKEY, &moved, |_| true)
                .map(|result| result.is_ok()),
            Some(true)
        );
        assert_eq!(
            service.binding(&RECORD),
            Some(Shortcut::from_static("Ctrl+Alt+F8"))
        );

        service.occupy(Shortcut::from_static("Ctrl+Alt+F7"));
        let taken = record_bound_to("Ctrl+Alt+F7");
        let refused = rebind_setting(&service, &keys::RECORD_HOTKEY, &taken, |_| true)
            .map(|result| result.map_err(|error| error.into_app_error()));
        assert_eq!(refused, Some(Err(conflict())));
        assert_eq!(
            service.binding(&RECORD),
            Some(Shortcut::from_static("Ctrl+Alt+F8"))
        );
        assert!(rebind_setting(&service, &keys::THEME, &taken, |_| true).is_none());
    }

    #[test]
    fn a_hotkey_the_filter_excludes_is_not_rebound() {
        let service = FakeHotkeyService::default();
        let moved = settings::resolve([(
            keys::PASTE_LAST_HOTKEY,
            SettingValue::Hotkey(StaticStr::new("Ctrl+Shift+F8")),
        )]);
        let rebound = rebind_setting(&service, &keys::PASTE_LAST_HOTKEY, &moved, |spec| {
            spec.action == HotkeyAction::Record
        });
        assert!(rebound.is_none());
        assert_eq!(service.binding(&PASTE_LAST), None);
    }

    #[test]
    fn pausing_releases_and_rebinding_restores_the_always_on_hotkeys() {
        let service = FakeHotkeyService::default();
        assert!(bind_always(&service, &settings::defaults()).is_empty());
        unbind_always(&service).unwrap();
        assert_eq!(service.binding(&RECORD), None);
        assert_eq!(service.binding(&PASTE_LAST), None);
        assert!(bind_always(&service, &settings::defaults()).is_empty());
        assert!(service.binding(&RECORD).is_some());
    }

    #[test]
    fn the_session_guard_holds_esc_only_while_it_lives() {
        let service = Arc::new(FakeHotkeyService::default());
        let guard = SessionHotkeys::bind(service.clone(), &settings::defaults()).unwrap();
        assert_eq!(
            service.binding(&CANCEL),
            Some(Shortcut::from_static("Escape"))
        );
        drop(guard);
        assert_eq!(service.binding(&CANCEL), None);
    }

    #[test]
    fn a_subset_binds_only_the_hotkeys_it_includes() {
        let service = FakeHotkeyService::default();
        let failures = bind_always_where(&service, &settings::defaults(), |spec| {
            spec.action == HotkeyAction::Record
        });
        assert!(failures.is_empty(), "{failures:?}");
        assert!(service.binding(&RECORD).is_some());
        assert_eq!(
            service.binding(&PASTE_LAST),
            None,
            "a hotkey nobody handles stays free for other apps"
        );
    }

    #[test]
    fn an_abandoned_session_guard_leaves_its_binding_to_the_process_exit() {
        let service = Arc::new(FakeHotkeyService::default());
        let guard = SessionHotkeys::bind(service.clone(), &settings::defaults()).unwrap();
        guard.abandon();
        assert!(service.binding(&CANCEL).is_some());
    }

    #[test]
    fn a_session_guard_that_cannot_bind_leaves_nothing_bound() {
        let service = Arc::new(FakeHotkeyService::default());
        service.occupy(Shortcut::from_static("Escape"));
        let result = SessionHotkeys::bind(service.clone(), &settings::defaults());
        assert_eq!(
            result.map(|_| ()).map_err(|error| error.into_app_error()),
            Err(conflict())
        );
        assert_eq!(service.binding(&CANCEL), None);
    }
}
