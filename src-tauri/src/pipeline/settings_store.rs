/*!
 * SOURCE OF TRUTH KEYWORDS: settings store, store setting, write setting row, reset setting row, resolve stored settings, settings write core, store_internal, internal setting write
 * WHAT:  `store(db, key, value)`: writes (Some) or removes (None) one setting row, then reads every stored row back
 *        and resolves them over the registry defaults into the snapshot that must become current;
 *        `store_internal`: the whole write of a setting the pipeline keeps itself (validate, store, publish,
 *        announce).
 * WHY:   Two writers exist: the settings commands (a user's change, after validation and a hotkey rebind) and the
 *        pipeline (hidden state, such as a one-time notice's flag). Both must re-read the table after writing, so the
 *        published snapshot always equals the table even when writes race; one function keeps them identical.
 *        Callers run it inside `SharedSettings::update`, which serializes writers (types/settings.rs). No validation
 *        happens here: each caller validates against the registry before storing.
 * WHERE: ipc/commands/settings.rs (`settings_set`, `settings_reset`); pipeline/notices.rs (NoticeBoard);
 *        pipeline/onboarding.rs (`store_internal` for onboarding completion).
 */

use crate::{
    ports::EventSink,
    registry, services,
    services::Db,
    types::{
        AppEvent, PortError, PortResult, SettingKey, SettingValue, SettingsChanged,
        SettingsSnapshot, SharedSettings,
    },
};

/// Stores `value` for `key` (None removes the row, so the default applies) and returns the resolved table.
pub fn store(
    db: &Db,
    key: &SettingKey,
    value: Option<&SettingValue>,
) -> PortResult<SettingsSnapshot> {
    match value {
        Some(value) => services::settings::set::set(db, key, value)?,
        None => services::settings::reset::reset(db, key)?,
    }
    resolved(db)
}

/// The stored rows resolved over the registry defaults.
pub fn resolved(db: &Db) -> PortResult<SettingsSnapshot> {
    Ok(registry::settings::resolve(services::settings::get::all(
        db,
    )?))
}

/**
 * SOURCE OF TRUTH KEYWORDS: store_internal, internal setting write, hidden setting write, SettingsChanged for internal state
 * WHAT:  Validates `value` for `key` against the registry, stores it inside `SharedSettings::update` (one writer at
 *        a time, the published snapshot re-read from the table) and announces the effective value as
 *        SettingsChanged.
 * WHY:   A setting the pipeline keeps as internal state (onboarding completion) obeys the same registry rules and
 *        the same one-writer path as a user's change, so a hidden value is never stored unchecked and the cache
 *        always equals the table. Internal settings feed no running state, so no settings effects run.
 * WHERE: pipeline/onboarding.rs (`complete`).
 */
pub fn store_internal(
    settings: &SharedSettings,
    db: &Db,
    events: &dyn EventSink<AppEvent>,
    key: &SettingKey,
    value: SettingValue,
) -> PortResult<()> {
    settings.update(|current| {
        registry::settings::validate(key, &value, current).map_err(PortError::from)?;
        store(db, key, Some(&value))
    })?;
    events.emit(
        SettingsChanged {
            key: key.clone(),
            value,
        }
        .into(),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ports::fakes::RecordingSink, registry::settings::keys, types::AppError};

    #[test]
    fn a_stored_value_and_its_removal_are_read_back() {
        let db = Db::open_in_memory().unwrap();
        let snapshot = store(&db, &keys::SOUND_CUES, Some(&SettingValue::Bool(false))).unwrap();
        assert_eq!(snapshot.bool(&keys::SOUND_CUES), Some(false));
        assert_eq!(resolved(&db).unwrap(), snapshot);
        let reset = store(&db, &keys::SOUND_CUES, None).unwrap();
        assert_eq!(reset.bool(&keys::SOUND_CUES), Some(true));
        assert_eq!(reset, registry::settings::defaults());
    }

    #[test]
    fn an_internal_write_is_validated_published_and_announced() {
        let db = Db::open_in_memory().unwrap();
        let settings = SharedSettings::new(registry::settings::defaults());
        let events = RecordingSink::default();
        store_internal(
            &settings,
            &db,
            &events,
            &keys::ONBOARDED,
            SettingValue::Bool(true),
        )
        .unwrap();
        assert_eq!(settings.current().bool(&keys::ONBOARDED), Some(true));
        assert_eq!(resolved(&db).unwrap(), *settings.current());
        assert_eq!(
            events.events(),
            [AppEvent::SettingsChanged(SettingsChanged {
                key: keys::ONBOARDED,
                value: SettingValue::Bool(true),
            })]
        );

        let refused = store_internal(
            &settings,
            &db,
            &events,
            &keys::ONBOARDED,
            SettingValue::Int(1),
        );
        assert!(matches!(
            refused.map_err(|error| error.into_app_error()),
            Err(AppError::Validation { .. })
        ));
        assert_eq!(
            events.events().len(),
            1,
            "a refused write announces nothing"
        );
    }
}
