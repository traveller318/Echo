/*!
 * SOURCE OF TRUTH KEYWORDS: settings commands, registry_get, settings_get_all, settings_set, settings_reset, SettingsChanged, AppearanceChanged, RegistryView
 * WHAT:  The settings command group. `registry_get` returns every registry list the UI renders from
 *        (RegistryView). `settings_get_all` returns every setting's effective value; `settings_set` validates a
 *        value against the registry, stores it and returns the value now in effect; `settings_reset` removes the
 *        stored value so the default applies. The two writes emit SettingsChanged.
 * WHY:   The UI builds its navigation, Settings form, Models page and dashboard layout from the registry and never
 *        hardcodes a list (02 §3.3, root CLAUDE.md §7). Writes are validated against the registry spec here, in the
 *        command layer, not in the service (02 §7.2). After a write the stored rows are read back and resolved
 *        into the one live SettingsSnapshot inside `SharedSettings::update`, so concurrent writes cannot publish a
 *        snapshot that misses one of them and the cache always equals the table. Reads come from that snapshot
 *        (no disk). SettingsChanged carries the effective value so every window updates without polling
 *        (02 §4.4); a write that changes the theme also emits AppearanceChanged. Caps requirements (e.g. hold mode needs key-up) join `settings_set` with the adapters that
 *        declare them.
 * WHERE: Registered through `ipc::commands::catalog`; called from the UI as `commands.registryGet()`,
 *        `commands.settingsGetAll()`, `commands.settingsSet({ key, value })`, `commands.settingsReset({ key })`.
 */

use crate::{
    ipc::{CommandCtx, factory::echo_command},
    pipeline::{appearance, retention},
    registry::{engines, hotkeys, metrics, nav, settings},
    services,
    services::Db,
    types::{
        AppError, AppearanceChanged, PortError, PortResult, RegistryView, ResourceKind,
        SettingEntry, SettingKey, SettingsChanged, SettingsResetInput, SettingsSetInput,
        SettingsSnapshot,
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
    /// Stores a new value for a setting after checking it against the setting's registry spec; returns the
    /// value now in effect and emits SettingsChanged.
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

/**
 * SOURCE OF TRUTH KEYWORDS: settings_set handler, registry validate, store setting, publish snapshot
 * WHAT:  Validates the value for the key against the registry (kind, bounds, runtime options) with the settings
 *        in effect, stores it, re-resolves the snapshot and announces the effective value.
 * WHY:   An unknown key is `NotFound { setting }`; a value that does not fit is `Validation` on the key. Nothing
 *        is stored or published when either fails.
 * WHERE: `settings_set`.
 */
pub async fn set(ctx: &CommandCtx, input: SettingsSetInput) -> Result<SettingEntry, PortError> {
    let SettingsSetInput { key, value } = input;
    let mut before = None;
    let snapshot = ctx.shared_settings().update(|current| {
        settings::validate(&key, &value, current)?;
        services::settings::set::set(ctx.db(), &key, &value)?;
        before = Some(current.clone());
        resolved(ctx.db())
    })?;
    announce(ctx, key, before.as_ref(), &snapshot)
}

/// Removes the stored value of a registered key, re-resolves the snapshot and announces the default.
pub async fn reset(ctx: &CommandCtx, input: SettingsResetInput) -> Result<SettingEntry, PortError> {
    let key = input.key;
    settings::find(&key).ok_or(AppError::NotFound {
        resource: ResourceKind::Setting,
    })?;
    let mut before = None;
    let snapshot = ctx.shared_settings().update(|current| {
        services::settings::reset::reset(ctx.db(), &key)?;
        before = Some(current.clone());
        resolved(ctx.db())
    })?;
    announce(ctx, key, before.as_ref(), &snapshot)
}

/// The stored rows resolved over the registry defaults.
fn resolved(db: &Db) -> PortResult<SettingsSnapshot> {
    Ok(settings::resolve(services::settings::get::all(db)?))
}

/**
 * SOURCE OF TRUTH KEYWORDS: announce setting change, SettingsChanged emit, AppearanceChanged after theme write, retention re-sweep
 * WHAT:  Emits SettingsChanged with the effective value of `key` and, when the write changed the appearance
 *        (the theme), AppearanceChanged with the full view; when it changed the retention policy, asks the
 *        retention sweeper to apply it now; returns the entry.
 * WHY:   The appearance view and the retention policy are derived state: which settings feed them is decided in
 *        pipeline/appearance and pipeline/retention, never by matching a key here. A new retention value takes
 *        effect at once (lowering it frees the disk now), not at the next daily sweep. `before` is the snapshot the write replaced (always present after a successful
 *        update).
 * WHERE: `set` and `reset`.
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
    if let Some(view) = before
        .and_then(|before| appearance::after_settings_change(before, snapshot, ctx.appearance()))
    {
        ctx.emit(AppearanceChanged(view));
    }
    if before.is_some_and(|before| retention::policy_changed(before, snapshot)) {
        ctx.retention().sweep_soon();
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
            AppEvent, AppearanceView, Backdrop, CommandSpec, Permission, Reentrancy, SettingValue,
            StaticStr, ThemePreference, Transparency,
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
        let entry = run_set(ctx, set_input(&keys::TYPING_WPM, SettingValue::Int(65))).unwrap();
        assert_eq!(entry.value, SettingValue::Int(65));
        assert_eq!(ctx.settings().int(&keys::TYPING_WPM), Some(65));
        assert_eq!(stored(ctx), [(keys::TYPING_WPM, SettingValue::Int(65))]);
        assert_eq!(
            harness.events.events(),
            [AppEvent::SettingsChanged(SettingsChanged {
                key: keys::TYPING_WPM,
                value: SettingValue::Int(65),
            })]
        );
        let all = finish(factory::run(ctx, &spec("settings_get_all"), (), get_all)).unwrap();
        assert!(all.contains(&entry));
    }

    #[test]
    fn a_theme_write_also_announces_the_appearance_and_other_writes_do_not() {
        let harness = harness();
        let ctx = &harness.ctx;
        run_set(ctx, set_input(&keys::TYPING_WPM, SettingValue::Int(50))).unwrap();
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
