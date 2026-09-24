/*!
 * SOURCE OF TRUTH KEYWORDS: get settings, stored setting rows, read settings table, unreadable setting, settings overlay input
 * WHAT:  `all`: every stored (key, value) pair, ordered by key. `one`: the stored value of one key, if any.
 * WHY:   Callers pass `all` straight to `registry::settings::resolve`, which overlays the rows on the defaults and
 *        drops values that no longer fit their spec. A row whose JSON this build cannot read (written by a newer
 *        build, or damaged) is skipped with a warning naming only the key, so its default applies instead of the
 *        whole settings read failing; the row itself stays until the next write of that key replaces it.
 * WHERE: app/bootstrap and ipc/commands/settings.rs (after every write, to re-resolve); service tests.
 */

use rusqlite::{OptionalExtension, params};

use crate::{
    services::db::Db,
    types::{PortResult, SettingKey, SettingValue},
};

pub fn all(db: &Db) -> PortResult<Vec<(SettingKey, SettingValue)>> {
    let rows: Vec<(String, String)> = db.read(|connection| {
        connection
            .prepare_cached("SELECT key, value_json FROM settings ORDER BY key")?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect()
    })?;
    Ok(rows
        .into_iter()
        .filter_map(|(key, json)| decode(SettingKey::from(key), &json))
        .collect())
}

pub fn one(db: &Db, key: &SettingKey) -> PortResult<Option<SettingValue>> {
    let json: Option<String> = db.read(|connection| {
        connection
            .prepare_cached("SELECT value_json FROM settings WHERE key = ?1")?
            .query_row(params![key.as_str()], |row| row.get(0))
            .optional()
    })?;
    Ok(json.and_then(|json| decode(key.clone(), &json).map(|(_, value)| value)))
}

fn decode(key: SettingKey, json: &str) -> Option<(SettingKey, SettingValue)> {
    match serde_json::from_str(json) {
        Ok(value) => Some((key, value)),
        Err(error) => {
            tracing::warn!(key = key.as_str(), %error, "stored setting is unreadable; its default applies");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::set;
    use super::*;
    use crate::types::StaticStr;

    #[test]
    fn an_empty_table_has_no_stored_values() {
        let db = Db::open_in_memory().unwrap();
        assert!(all(&db).unwrap().is_empty());
        assert_eq!(
            one(&db, &SettingKey::from_static("general.theme")).unwrap(),
            None
        );
    }

    #[test]
    fn stored_values_come_back_ordered_by_key() {
        let db = Db::open_in_memory().unwrap();
        let theme = SettingKey::from_static("general.theme");
        let wpm = SettingKey::from_static("metrics.typing_wpm");
        set::set(&db, &wpm, &SettingValue::Int(55)).unwrap();
        set::set(&db, &theme, &SettingValue::Enum(StaticStr::new("dark"))).unwrap();
        assert_eq!(
            all(&db).unwrap(),
            [
                (theme.clone(), SettingValue::Enum(StaticStr::new("dark"))),
                (wpm.clone(), SettingValue::Int(55)),
            ]
        );
        assert_eq!(one(&db, &wpm).unwrap(), Some(SettingValue::Int(55)));
    }

    #[test]
    fn an_unreadable_row_is_skipped_not_fatal() {
        let db = Db::open_in_memory().unwrap();
        let wpm = SettingKey::from_static("metrics.typing_wpm");
        set::set(&db, &wpm, &SettingValue::Int(55)).unwrap();
        db.write(|connection| {
            connection.execute(
                "INSERT INTO settings (key, value_json, updated_at) VALUES \
                 ('general.theme', '{\"kind\":\"colour\"}', 1)",
                [],
            )
        })
        .unwrap();
        assert_eq!(all(&db).unwrap(), [(wpm, SettingValue::Int(55))]);
        assert_eq!(
            one(&db, &SettingKey::from_static("general.theme")).unwrap(),
            None
        );
    }
}
