/*!
 * SOURCE OF TRUTH KEYWORDS: settings commands, registry_get, settings_get_all, settings_availability, settings_set, settings_reset, SettingsChanged, live hotkey rebind, SettingsEffects, RegistryView
 * WHAT:  The settings command group. `registry_get` returns every registry list the UI renders from
 *        (RegistryView). `settings_get_all` returns every setting's effective value; `settings_availability` what
 *        the Settings page may offer now (caps that hold, each choice setting's options); `settings_set` validates a
 *        value against the registry and the running adapters, stores it and returns the value now in effect;
 *        `settings_reset` removes the stored value so the default applies. Both writes emit SettingsChanged and make
 *        the change take effect at once.
 * WHY:   The UI builds its navigation, Settings form, Models page and dashboard layout from the registry and never
 *        hardcodes a list (02 §3.3, root CLAUDE.md §7). Writes are validated against the registry spec here, in the
 *        command layer, not in the service (02 §7.2), including what the adapters' caps allow (hold mode needs
 *        key-up). Both writes share one path: inside `SharedSettings::update` the candidate snapshot is built, a
 *        hotkey setting is bound first (a combination the port refuses fails the write and the previous binding
 *        stays, 05 W7), then the row is written and the stored rows are read back and resolved into the one live
 *        SettingsSnapshot, so concurrent writes cannot publish a snapshot that misses one of them and the cache
 *        always equals the table. A failed write puts the previous binding back. Reads come from that snapshot (no
 *        disk). SettingsChanged carries the effective value so every window updates without polling (02 §4.4);
 *        everything else a change sets in motion (appearance, retention, engine swap, dashboard refresh) is
 *        pipeline/settings_effects.rs, so this file never matches on a setting key.
 * WHERE: Registered through `ipc::commands::catalog`; called from the UI as `commands.registryGet()`,
 *        `commands.settingsGetAll()`, `commands.settingsAvailability()`, `commands.settingsSet({ key, value })`,
 *        `commands.settingsReset({ key })`.
 */

use crate::{
    ipc::{CommandCtx, factory::echo_command},
    pipeline::{hotkeys as hotkey_bindings, session, settings_effects::SettingsEffects},
    registry::{engines, hotkeys, metrics, nav, settings},
    services,
    services::Db,
    types::{
        AdapterCaps, AppError, PortError, PortResult, RegistryView, ResourceKind, SettingEntry,
        SettingKey, SettingValue, SettingsAvailability, SettingsChanged, SettingsResetInput,
        SettingsSetInput, SettingsSnapshot,
    },
};

echo_command! {
    /// Every registry list the UI renders from. Compiled in, so it never changes while the app runs.
    name: registry_get,
    output: RegistryView,
    permission: None,
    reentrancy: Shared,
    handler: get_registry,
}

echo_command! {
    /// Every setting's effective value (the stored value, or the registry default), ordered by key.
    name: settings_get_all,
    output: Vec<SettingEntry>,
    permission: None,
    reentrancy: Shared,
    handler: get_all,
}

echo_command! {
    /// What the Settings page may offer now: the caps requirements that hold and every choice setting's options.
    name: settings_availability,
    output: SettingsAvailability,
    permission: None,
    reentrancy: Shared,
    handler: get_availability,
}

echo_command! {
    /// Stores a new value for a setting after checking it against the setting's registry spec and what this PC
    /// supports; returns the value now in effect and emits SettingsChanged.
    name: settings_set,
    input: SettingsSetInput,
    output: SettingEntry,
    permission: None,
    reentrancy: Shared,
    handler: set,
}

echo_command! {
    /// Returns a setting to its registry default; returns the value now in effect and emits SettingsChanged.
    name: settings_reset,
    input: SettingsResetInput,
    output: SettingEntry,
    permission: None,
    reentrancy: Shared,
    handler: reset,
}

/// Builds the RegistryView from the registry lists, in registry order (nav in sidebar order).
pub async fn get_registry(_: &CommandCtx, (): ()) -> Result<RegistryView, AppError> {
    Ok(RegistryView {
        settings: settings::SETTINGS.to_vec(),
        sections: settings::SECTIONS.to_vec(),
        hotkeys: hotkeys::HOTKEYS.to_vec(),
        nav: nav::items().into_iter().cloned().collect(),
        engines: engines::specs(),
        metrics: metrics::METRICS.to_vec(),
    })
}

/// The effective value of every setting, from the live snapshot.
pub async fn get_all(ctx: &CommandCtx, (): ()) -> Result<Vec<SettingEntry>, AppError> {
    Ok(ctx
        .settings()
        .iter()
        .map(|(key, value)| SettingEntry {
            key: key.clone(),
            value: value.clone(),
        })
        .collect())
}

/// The caps that hold and the options offered, for the settings in effect and the running adapters.
pub async fn get_availability(ctx: &CommandCtx, (): ()) -> Result<SettingsAvailability, AppError> {
    Ok(settings::availability(&ctx.settings(), &adapter_caps(ctx)))
}

/// A new value to store, or a return to the registry default.
enum Change {
    Set(SettingValue),
    Reset,
}

/// Validates the value against the registry (kind, bounds, runtime options, hotkey conflicts) and the running
/// adapters' caps, then stores it through the shared write path.
pub async fn set(ctx: &CommandCtx, input: SettingsSetInput) -> Result<SettingEntry, PortError> {
    write(ctx, input.key, Change::Set(input.value))
}

/// Removes the stored value of a registered key through the shared write path, so the default applies.
pub async fn reset(ctx: &CommandCtx, input: SettingsResetInput) -> Result<SettingEntry, PortError> {
    write(ctx, input.key, Change::Reset)
}

/**
 * SOURCE OF TRUTH KEYWORDS: settings write path, candidate snapshot, bind before store, restore binding, publish snapshot
 * WHAT:  Checks the change against the settings in effect, builds the snapshot it would give, binds a changed
 *        hotkey, writes or removes the row, re-resolves and publishes the snapshot, then announces it.
 * WHY:   An unknown key is `NotFound { setting }`, a value that does not fit is `Validation` on the key, a taken
 *        combination is `Hotkey { conflict }`; nothing is stored or published when any check, the binding or the
 *        write fails, and a binding made for a write that then failed is put back, so the live hotkey always
 *        matches the published snapshot.
 * WHERE: `set` and `reset`.
 */
fn write(ctx: &CommandCtx, key: SettingKey, change: Change) -> Result<SettingEntry, PortError> {
    let adapters = adapter_caps(ctx);
    let mut before = None;
    let snapshot = ctx.shared_settings().update(|current| {
        let candidate = match &change {
            Change::Set(value) => {
                let spec = settings::validate(&key, value, current)?;
                settings::check_available(spec, value, current, &adapters)?;
                with_value(current, &key, Some(value))
            }
            Change::Reset => {
                settings::validate_reset(&key, current)?;
                with_value(current, &key, None)
            }
        };
        let rebound = rebind(ctx, &key, &candidate)?;
        let stored = match &change {
            Change::Set(value) => services::settings::set::set(ctx.db(), &key, value),
            Change::Reset => services::settings::reset::reset(ctx.db(), &key),
        };
        let published = stored.and_then(|()| resolved(ctx.db()));
        if published.is_err() && rebound {
            restore_binding(ctx, &key, current);
        }
        before = Some(current.clone());
        published
    })?;
    announce(ctx, key, before.as_ref(), &snapshot)
}

/// `current` with `key` set to `value` (None: its default).
fn with_value(
    current: &SettingsSnapshot,
    key: &SettingKey,
    value: Option<&SettingValue>,
) -> SettingsSnapshot {
    settings::resolve(
        current
            .iter()
            .filter(|(existing, _)| *existing != key)
            .map(|(existing, stored)| (existing.clone(), stored.clone()))
            .chain(value.map(|value| (key.clone(), value.clone()))),
    )
}

/// Binds the hotkey `key` rebinds to its combination in `candidate`, when the session binds that hotkey; true when
/// a binding changed. A refused combination is the write's error and leaves the previous binding in place.
fn rebind(ctx: &CommandCtx, key: &SettingKey, candidate: &SettingsSnapshot) -> PortResult<bool> {
    hotkey_bindings::rebind_setting(
        ctx.hotkeys().as_ref(),
        key,
        candidate,
        session::binds_hotkey,
    )
    .map_or(Ok(false), |bound| bound.map(|()| true))
}

/// Puts back the combination `previous` gives after a write that bound a new one could not be stored.
fn restore_binding(ctx: &CommandCtx, key: &SettingKey, previous: &SettingsSnapshot) {
    if let Some(Err(error)) = hotkey_bindings::rebind_setting(
        ctx.hotkeys().as_ref(),
        key,
        previous,
        session::binds_hotkey,
    ) {
        tracing::warn!(
            setting = %key,
            detail = error.detail(),
            "the previous hotkey could not be bound again after a failed settings write"
        );
    }
}

/// The caps of the running adapters that settings availability depends on.
fn adapter_caps(ctx: &CommandCtx) -> AdapterCaps {
    AdapterCaps {
        hotkeys: ctx.hotkeys().caps(),
        updater: ctx.updater().caps(),
    }
}

/// The stored rows resolved over the registry defaults.
fn resolved(db: &Db) -> PortResult<SettingsSnapshot> {
    Ok(settings::resolve(services::settings::get::all(db)?))
}

/**
 * SOURCE OF TRUTH KEYWORDS: announce setting change, SettingsChanged emit, apply settings effects
 * WHAT:  Emits SettingsChanged with the effective value of `key`, lets SettingsEffects apply whatever the change
 *        affects (appearance, retention, speech engine, dashboard), and returns the entry.
 * WHY:   Which running state a setting feeds is decided by its owner in the pipeline, never by matching a key
 *        here. `before` is the snapshot the write replaced (always present after a successful update).
 * WHERE: `write`.
 */
fn announce(
    ctx: &CommandCtx,
    key: SettingKey,
    before: Option<&SettingsSnapshot>,
    snapshot: &SettingsSnapshot,
) -> Result<SettingEntry, PortError> {
    let value = snapshot.get(&key).cloned().ok_or(AppError::NotFound {
        resource: ResourceKind::Setting,
    })?;
    ctx.emit(SettingsChanged {
        key: key.clone(),
        value: value.clone(),
    });
    if let Some(before) = before {
        SettingsEffects {
            appearance: ctx.appearance(),
            retention: ctx.retention(),
            asr: ctx.asr(),
            paths: ctx.paths(),
            events: ctx.events(),
        }
        .apply(before, snapshot);
    }
    Ok(SettingEntry { key, value })
}

#[cfg(test)]
mod tests {
    use std::{future::Future, task::Poll};

    use super::*;
    use crate::{
        ipc::{factory, testing},
        ports::fakes::{FakePrivacyConsent, poll_once},
        registry::settings::keys,
        types::{
            AppEvent, AppearanceChanged, AppearanceView, Backdrop, CapsRequirement, CommandSpec,
            HotkeyIssue, MetricsChanged, Permission, Reentrancy, SettingKind, Shortcut, StaticStr,
            ThemePreference, Transparency,
        },
    };

    const fn spec(name: &'static str) -> CommandSpec {
        CommandSpec {
            name,
            permission: None,
            reentrancy: Reentrancy::Shared,
        }
    }

    /// Runs a call whose handler never waits, so one poll finishes it.
    fn finish<O>(call: impl Future<Output = Result<O, AppError>>) -> Result<O, AppError> {
        match poll_once(call) {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("the command did not finish on its first poll"),
        }
    }

    fn harness() -> testing::Harness {
        testing::harness(settings::defaults(), FakePrivacyConsent::granted())
    }

    fn set_input(key: &SettingKey, value: SettingValue) -> SettingsSetInput {
        SettingsSetInput {
            key: key.clone(),
            value,
        }
    }

    fn run_set(ctx: &CommandCtx, input: SettingsSetInput) -> Result<SettingEntry, AppError> {
        finish(factory::run(ctx, &spec("settings_set"), input, set))
    }

    fn run_reset(ctx: &CommandCtx, key: &SettingKey) -> Result<SettingEntry, AppError> {
        finish(factory::run(
            ctx,
            &spec("settings_reset"),
            SettingsResetInput { key: key.clone() },
            reset,
        ))
    }

    fn stored(ctx: &CommandCtx) -> Vec<(SettingKey, SettingValue)> {
        services::settings::get::all(ctx.db()).unwrap()
    }

    #[test]
    fn registry_view_carries_every_registry_list_in_order() {
        let ctx = testing::ctx();
        let Poll::Ready(Ok(view)) =
            poll_once(factory::run(&ctx, &spec("registry_get"), (), get_registry))
        else {
            panic!("registry_get did not return a view");
        };
        assert_eq!(view.settings, settings::SETTINGS);
        assert_eq!(view.hotkeys, hotkeys::HOTKEYS);
        assert_eq!(view.metrics, metrics::METRICS);
        assert_eq!(view.engines, engines::specs());
        let nav: Vec<&str> = view.nav.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(nav, ["dashboard", "history", "models", "settings"]);
    }

    #[test]
    fn get_all_returns_every_registered_setting_at_its_effective_value() {
        let ctx = testing::ctx();
        let entries = finish(factory::run(&ctx, &spec("settings_get_all"), (), get_all)).unwrap();
        assert_eq!(entries.len(), settings::SETTINGS.len());
        for spec in settings::SETTINGS {
            let entry = entries.iter().find(|entry| entry.key == spec.key).unwrap();
            assert_eq!(entry.value, spec.default, "{}", spec.key);
        }
    }

    #[test]
    fn set_stores_publishes_and_announces_the_new_value() {
        let harness = harness();
        let ctx = &harness.ctx;
        let entry = run_set(
            ctx,
            set_input(&keys::CANCEL_COUNTDOWN_MS, SettingValue::Int(5000)),
        )
        .unwrap();
        assert_eq!(entry.value, SettingValue::Int(5000));
        assert_eq!(ctx.settings().int(&keys::CANCEL_COUNTDOWN_MS), Some(5000));
        assert_eq!(
            stored(ctx),
            [(keys::CANCEL_COUNTDOWN_MS, SettingValue::Int(5000))]
        );
        assert_eq!(
            harness.events.events(),
            [AppEvent::SettingsChanged(SettingsChanged {
                key: keys::CANCEL_COUNTDOWN_MS,
                value: SettingValue::Int(5000),
            })]
        );
        let all = finish(factory::run(ctx, &spec("settings_get_all"), (), get_all)).unwrap();
        assert!(all.contains(&entry));
    }

    #[test]
    fn a_theme_write_also_announces_the_appearance_and_other_writes_do_not() {
        let harness = harness();
        let ctx = &harness.ctx;
        run_set(ctx, set_input(&keys::AUTO_PASTE, SettingValue::Bool(false))).unwrap();
        assert_eq!(harness.events.events().len(), 1);
        run_set(
            ctx,
            set_input(&keys::THEME, SettingValue::Enum(StaticStr::new("dark"))),
        )
        .unwrap();
        let events = harness.events.events();
        assert_eq!(
            events.last(),
            Some(&AppEvent::AppearanceChanged(AppearanceChanged(
                AppearanceView {
                    theme: ThemePreference::Dark,
                    transparency: Transparency::Full,
                    backdrop: Backdrop::Mica,
                }
            )))
        );
        run_reset(ctx, &keys::THEME).unwrap();
        assert!(matches!(
            harness.events.events().last(),
            Some(AppEvent::AppearanceChanged(AppearanceChanged(view))) if view.theme == ThemePreference::System
        ));
        run_reset(ctx, &keys::THEME).unwrap();
        assert!(matches!(
            harness.events.events().last(),
            Some(AppEvent::SettingsChanged(_))
        ));
    }

    #[test]
    fn an_invalid_value_is_a_validation_error_and_changes_nothing() {
        let harness = harness();
        let ctx = &harness.ctx;
        let out_of_range = run_set(ctx, set_input(&keys::TYPING_WPM, SettingValue::Int(5)));
        let Err(AppError::Validation { field, .. }) = out_of_range else {
            panic!("expected a validation error, got {out_of_range:?}");
        };
        assert_eq!(field, keys::TYPING_WPM.as_str());
        assert!(matches!(
            run_set(ctx, set_input(&keys::AUTO_PASTE, SettingValue::Int(1))),
            Err(AppError::Validation { .. })
        ));
        assert!(matches!(
            run_set(
                ctx,
                set_input(&keys::THEME, SettingValue::Enum(StaticStr::new("sepia")))
            ),
            Err(AppError::Validation { .. })
        ));
        assert!(stored(ctx).is_empty());
        assert!(harness.events.events().is_empty());
        assert_eq!(ctx.settings().int(&keys::TYPING_WPM), Some(40));
    }

    #[test]
    fn runtime_options_are_checked_against_what_is_registered() {
        let harness = harness();
        let result = run_set(
            &harness.ctx,
            set_input(
                &keys::ASR_ENGINE,
                SettingValue::Enum(StaticStr::new("not-an-engine")),
            ),
        );
        assert!(
            matches!(result, Err(AppError::Validation { .. })),
            "{result:?}"
        );
        assert!(stored(&harness.ctx).is_empty());
    }

    #[test]
    fn unknown_and_malformed_keys_are_refused() {
        let harness = harness();
        let ctx = &harness.ctx;
        let unknown = SettingKey::from_static("general.volume");
        assert_eq!(
            run_set(ctx, set_input(&unknown, SettingValue::Bool(true))),
            Err(AppError::NotFound {
                resource: ResourceKind::Setting
            })
        );
        assert_eq!(
            run_reset(ctx, &unknown),
            Err(AppError::NotFound {
                resource: ResourceKind::Setting
            })
        );
        let malformed = SettingKey::from_static("General Theme");
        let Err(AppError::Validation { field, .. }) =
            run_set(ctx, set_input(&malformed, SettingValue::Bool(true)))
        else {
            panic!("expected a validation error");
        };
        assert_eq!(field, "key");
        assert!(harness.events.events().is_empty());
    }

    #[test]
    fn reset_restores_the_default_and_announces_it() {
        let harness = harness();
        let ctx = &harness.ctx;
        run_set(ctx, set_input(&keys::AUTO_PASTE, SettingValue::Bool(false))).unwrap();
        run_set(ctx, set_input(&keys::TYPING_WPM, SettingValue::Int(80))).unwrap();
        let entry = run_reset(ctx, &keys::AUTO_PASTE).unwrap();
        assert_eq!(entry.value, SettingValue::Bool(true));
        assert_eq!(ctx.settings().bool(&keys::AUTO_PASTE), Some(true));
        assert_eq!(stored(ctx), [(keys::TYPING_WPM, SettingValue::Int(80))]);
        assert_eq!(
            harness.events.events().last(),
            Some(&AppEvent::SettingsChanged(SettingsChanged {
                key: keys::AUTO_PASTE,
                value: SettingValue::Bool(true),
            }))
        );
        assert_eq!(run_reset(ctx, &keys::AUTO_PASTE).unwrap(), entry);
    }

    #[test]
    fn a_stored_setting_takes_effect_in_the_factory_preflight() {
        let harness = harness();
        let ctx = &harness.ctx;
        let network = CommandSpec {
            name: "test_download",
            permission: Some(Permission::Network),
            reentrancy: Reentrancy::Shared,
        };
        let download = |ctx: &CommandCtx| {
            finish(factory::run(ctx, &network, (), |_, ()| async {
                Ok::<_, AppError>(())
            }))
        };
        assert_eq!(download(ctx), Ok(()));
        run_set(
            ctx,
            set_input(&keys::OFFLINE_MODE, SettingValue::Bool(true)),
        )
        .unwrap();
        assert_eq!(
            download(ctx),
            Err(AppError::PermissionDenied {
                permission: Permission::Network
            })
        );
    }

    #[test]
    fn availability_reports_the_caps_that_hold_and_every_choice_settings_options() {
        let ctx = testing::ctx();
        let view = finish(factory::run(
            &ctx,
            &spec("settings_availability"),
            (),
            get_availability,
        ))
        .unwrap();
        assert!(view.caps.contains(&CapsRequirement::HotkeyRelease));
        assert!(view.caps.contains(&CapsRequirement::MultipleLanguages));
        assert!(
            !view.caps.contains(&CapsRequirement::UpdaterAvailable),
            "this build has no update source"
        );
        let offered = |key: &SettingKey| -> Vec<String> {
            view.options
                .iter()
                .find(|entry| entry.key == *key)
                .map(|entry| {
                    entry
                        .options
                        .iter()
                        .map(|option| option.value.to_string())
                        .collect()
                })
                .unwrap_or_default()
        };
        assert_eq!(offered(&keys::THEME), ["system", "light", "dark"]);
        assert_eq!(offered(&keys::HOTKEY_MODE), ["toggle", "hold"]);
        assert_eq!(
            offered(&keys::ASR_ENGINE),
            [engines::PARAKEET_TDT_V3.to_string()]
        );
        assert_eq!(
            offered(&keys::LANGUAGE).first().map(String::as_str),
            Some("auto")
        );
        let choice_settings = settings::SETTINGS
            .iter()
            .filter(|spec| matches!(spec.kind, SettingKind::Enum { .. }))
            .count();
        assert_eq!(view.options.len(), choice_settings);
    }

    #[test]
    fn a_hotkey_write_rebinds_live_and_only_hotkeys_the_session_handles() {
        let harness = harness();
        let ctx = &harness.ctx;
        run_set(
            ctx,
            set_input(
                &keys::RECORD_HOTKEY,
                SettingValue::Hotkey(StaticStr::new("Ctrl+Shift+F9")),
            ),
        )
        .unwrap();
        assert_eq!(
            harness.hotkeys.binding(&hotkeys::RECORD),
            Some(Shortcut::from_static("Ctrl+Shift+F9"))
        );
        run_set(
            ctx,
            set_input(
                &keys::PASTE_LAST_HOTKEY,
                SettingValue::Hotkey(StaticStr::new("Ctrl+Shift+F10")),
            ),
        )
        .unwrap();
        assert_eq!(
            harness.hotkeys.binding(&hotkeys::PASTE_LAST),
            None,
            "stored, but not taken from other apps while nothing handles it"
        );
        assert_eq!(
            ctx.settings().hotkey(&keys::PASTE_LAST_HOTKEY),
            Some("Ctrl+Shift+F10")
        );
        run_reset(ctx, &keys::RECORD_HOTKEY).unwrap();
        assert_eq!(
            harness.hotkeys.binding(&hotkeys::RECORD),
            Some(Shortcut::from_static(hotkeys::RECORD_DEFAULT))
        );
    }

    #[test]
    fn a_combination_the_port_refuses_keeps_the_previous_binding_and_stores_nothing() {
        let harness = harness();
        let ctx = &harness.ctx;
        run_set(
            ctx,
            set_input(
                &keys::RECORD_HOTKEY,
                SettingValue::Hotkey(StaticStr::new("Ctrl+Alt+F8")),
            ),
        )
        .unwrap();
        harness.hotkeys.occupy(Shortcut::from_static("Ctrl+Alt+F7"));
        let refused = run_set(
            ctx,
            set_input(
                &keys::RECORD_HOTKEY,
                SettingValue::Hotkey(StaticStr::new("Ctrl+Alt+F7")),
            ),
        );
        assert_eq!(
            refused,
            Err(AppError::Hotkey {
                reason: HotkeyIssue::Conflict
            })
        );
        assert_eq!(
            harness.hotkeys.binding(&hotkeys::RECORD),
            Some(Shortcut::from_static("Ctrl+Alt+F8"))
        );
        assert_eq!(
            ctx.settings().hotkey(&keys::RECORD_HOTKEY),
            Some("Ctrl+Alt+F8")
        );
        assert_eq!(
            harness.events.events().len(),
            1,
            "only the first write announced"
        );
    }

    #[test]
    fn two_echo_hotkeys_on_one_chord_are_refused_on_write_and_on_reset() {
        let harness = harness();
        let ctx = &harness.ctx;
        let conflict = Err(AppError::Hotkey {
            reason: HotkeyIssue::Conflict,
        });
        let paste_last = |shortcut: &'static str| {
            run_set(
                ctx,
                set_input(
                    &keys::PASTE_LAST_HOTKEY,
                    SettingValue::Hotkey(StaticStr::new(shortcut)),
                ),
            )
        };
        assert_eq!(
            paste_last("Alt+Ctrl"),
            conflict,
            "the record hotkey's chord"
        );
        assert_eq!(paste_last("Escape"), conflict, "the cancel key");
        paste_last("Ctrl+Shift+V").unwrap();
        run_set(
            ctx,
            set_input(
                &keys::RECORD_HOTKEY,
                SettingValue::Hotkey(StaticStr::new("Ctrl+Alt+V")),
            ),
        )
        .unwrap();
        assert_eq!(
            run_reset(ctx, &keys::PASTE_LAST_HOTKEY),
            conflict,
            "its default is the record hotkey's now"
        );
        assert_eq!(
            ctx.settings().hotkey(&keys::PASTE_LAST_HOTKEY),
            Some("Ctrl+Shift+V")
        );
    }

    #[test]
    fn a_typing_speed_change_tells_the_dashboard_to_read_again() {
        let harness = harness();
        run_set(
            &harness.ctx,
            set_input(&keys::TYPING_WPM, SettingValue::Int(65)),
        )
        .unwrap();
        assert!(
            harness
                .events
                .events()
                .contains(&AppEvent::MetricsChanged(MetricsChanged {}))
        );
    }

    #[test]
    fn an_unavailable_setting_is_refused() {
        let harness = harness();
        let result = run_set(
            &harness.ctx,
            set_input(&keys::UPDATES_AUTO_CHECK, SettingValue::Bool(false)),
        );
        let Err(AppError::Validation { field, .. }) = result else {
            panic!("expected a validation error, got {result:?}");
        };
        assert_eq!(field, keys::UPDATES_AUTO_CHECK.as_str());
        assert!(stored(&harness.ctx).is_empty());
    }

    #[test]
    fn a_retention_change_asks_for_a_sweep_and_other_writes_do_not() {
        let mut harness = harness();
        run_set(
            &harness.ctx,
            set_input(&keys::TYPING_WPM, SettingValue::Int(65)),
        )
        .unwrap();
        assert!(!harness.retention_sweeper.take_requests());

        run_set(
            &harness.ctx,
            set_input(&keys::AUDIO_RETENTION_DAYS, SettingValue::Int(0)),
        )
        .unwrap();
        assert!(harness.retention_sweeper.take_requests());

        run_reset(&harness.ctx, &keys::AUDIO_RETENTION_DAYS).unwrap();
        assert!(
            harness.retention_sweeper.take_requests(),
            "going back to the default is a change too"
        );
        run_reset(&harness.ctx, &keys::HISTORY_RETENTION_DAYS).unwrap();
        assert!(
            !harness.retention_sweeper.take_requests(),
            "an unchanged policy sweeps nothing"
        );
    }
}
