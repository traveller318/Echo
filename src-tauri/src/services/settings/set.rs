/*!
 * SOURCE OF TRUTH KEYWORDS: set setting, upsert setting, write settings table, settings_set storage, value_json
 * WHAT:  `set`: stores `value` for `key` as kind-tagged JSON, replacing any earlier value, stamped with now.
 * WHY:   One row per key via upsert, so a write is a single statement and never leaves two values for a key. The
 *        value is not validated here: the command layer checks it against the registry first (02 §7.2).
 * WHERE: ipc/commands/settings.rs (`settings_set`); service tests.
 */

use rusqlite::params;

use crate::{
    services::db::{Db, storage},
    types::{PortResult, SettingKey, SettingValue, UnixMs},
};

pub fn set(db: &Db, key: &SettingKey, value: &SettingValue) -> PortResult<()> {
    let json = serde_json::to_string(value).map_err(storage)?;
    db.write(|connection| {
        connection
            .prepare_cached(
                "INSERT INTO settings (key, value_json, updated_at) VALUES (?1, ?2, ?3) \
                 ON CONFLICT (key) DO UPDATE SET value_json = excluded.value_json, \
                 updated_at = excluded.updated_at",
            )?
            .execute(params![key.as_str(), json, UnixMs::now().as_millis()])
            .map(drop)
    })
}

#[cfg(test)]
mod tests {
    use super::super::get;
    use super::*;
    use crate::types::{StaticList, StaticStr, TextPair};

    #[test]
    fn a_second_write_replaces_the_first() {
        let db = Db::open_in_memory().unwrap();
        let key = SettingKey::from_static("output.auto_paste");
        set(&db, &key, &SettingValue::Bool(false)).unwrap();
        set(&db, &key, &SettingValue::Bool(true)).unwrap();
        assert_eq!(get::all(&db).unwrap(), [(key, SettingValue::Bool(true))]);
    }

    #[test]
    fn values_are_stored_as_kind_tagged_json_with_a_timestamp() {
        let db = Db::open_in_memory().unwrap();
        let key = SettingKey::from_static("polish.dictionary");
        let value = SettingValue::Pairs(StaticList::from(vec![TextPair {
            from: StaticStr::new("echo"),
            to: StaticStr::new("Echo"),
        }]));
        set(&db, &key, &value).unwrap();
        let (json, updated_at): (String, i64) = db
            .read(|connection| {
                connection.query_row(
                    "SELECT value_json, updated_at FROM settings WHERE key = 'polish.dictionary'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
            })
            .unwrap();
        assert_eq!(
            json,
            r#"{"kind":"pairs","value":[{"from":"echo","to":"Echo"}]}"#
        );
        assert!(updated_at > 0);
        assert_eq!(get::one(&db, &key).unwrap(), Some(value));
    }
}
