/*!
 * SOURCE OF TRUTH KEYWORDS: settings store, store setting, write setting row, reset setting row, resolve stored settings, settings write core
 * WHAT:  `store(db, key, value)`: writes (Some) or removes (None) one setting row, then reads every stored row back
 *        and resolves them over the registry defaults into the snapshot that must become current.
 * WHY:   Two writers exist: the settings commands (a user's change, after validation and a hotkey rebind) and the
 *        pipeline (hidden state, such as a one-time notice's flag). Both must re-read the table after writing, so the
 *        published snapshot always equals the table even when writes race; one function keeps them identical.
 *        Callers run it inside `SharedSettings::update`, which serializes writers (types/settings.rs). No validation
 *        happens here: each caller validates against the registry before storing.
 * WHERE: ipc/commands/settings.rs (`settings_set`, `settings_reset`); pipeline/notices.rs (NoticeBoard).
 */

use crate::{
    registry, services,
    services::Db,
    types::{PortResult, SettingKey, SettingValue, SettingsSnapshot},
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::settings::keys;

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
}
