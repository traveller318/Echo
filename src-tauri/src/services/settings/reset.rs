/*!
 * SOURCE OF TRUTH KEYWORDS: reset setting, delete stored setting, restore default, settings_reset storage
 * WHAT:  `reset`: removes the stored value of `key`, so the registry default applies again. Resetting a key with
 *        no stored value succeeds and changes nothing.
 * WHY:   A default is never written as a row: deleting the row is what "default" means (02 §7.2), so a later build
 *        with a new default reaches every user who never changed the setting.
 * WHERE: ipc/commands/settings.rs (`settings_reset`); service tests.
 */

use rusqlite::params;

use crate::{
    services::db::Db,
    types::{PortResult, SettingKey},
};

pub fn reset(db: &Db, key: &SettingKey) -> PortResult<()> {
    db.write(|connection| {
        connection
            .prepare_cached("DELETE FROM settings WHERE key = ?1")?
            .execute(params![key.as_str()])
            .map(drop)
    })
}

#[cfg(test)]
mod tests {
    use super::super::{get, set};
    use super::*;
    use crate::types::SettingValue;

    #[test]
    fn reset_removes_only_that_key_and_is_idempotent() {
        let db = Db::open_in_memory().unwrap();
        let paste = SettingKey::from_static("output.auto_paste");
        let wpm = SettingKey::from_static("metrics.typing_wpm");
        set::set(&db, &paste, &SettingValue::Bool(false)).unwrap();
        set::set(&db, &wpm, &SettingValue::Int(70)).unwrap();
        reset(&db, &paste).unwrap();
        reset(&db, &paste).unwrap();
        assert_eq!(get::all(&db).unwrap(), [(wpm, SettingValue::Int(70))]);
    }
}
