/*!
 * SOURCE OF TRUTH KEYWORDS: local calendar, local date, local midnight, day start, time zone, SQLite localtime, date of instant, LocalDate column
 * WHAT:  The user's local calendar as the database sees it: `local_date` (the local day an instant falls on) and
 *        `day_start` (the instant a local day begins); plus the LocalDate column codec (FromSql / ToSql).
 * WHY:   The aggregates group takes into days with SQLite's `localtime` (services/transcripts/aggregate.rs), so the
 *        instants that bound those days and the answer to "which day is today" must come from the same clock,
 *        or a take just after midnight would be counted in one day and windowed into another. SQLite converts
 *        with the operating system's time-zone rules, daylight saving included (a 23- or 25-hour day starts at
 *        its real midnight), which also keeps Windows time-zone calls out of the core. No table is read: this is
 *        the one service without a table, and it takes the instant as input so tests and callers own "now".
 *        A local midnight that does not exist (a zone that springs forward at 00:00) resolves to SQLite's
 *        nearest instant, which still falls on that day.
 * WHERE: pipeline/metrics (range windows, today for the streak and the activity series, the day watch);
 *        the codec is used by services/transcripts/aggregate.rs.
 */

use rusqlite::{
    ToSql,
    types::{FromSql, FromSqlError, FromSqlResult, ToSqlOutput, ValueRef},
};

use crate::{
    services::db::Db,
    types::{LocalDate, PortResult, UnixMs},
};

/// The local calendar day `at` falls on.
pub fn local_date(db: &Db, at: UnixMs) -> PortResult<LocalDate> {
    db.read(|connection| {
        connection
            .prepare_cached("SELECT date(?1 / 1000, 'unixepoch', 'localtime')")?
            .query_row([at.as_millis()], |row| row.get(0))
    })
}

/// The instant the local calendar day `date` begins (its local midnight), in unix ms.
pub fn day_start(db: &Db, date: LocalDate) -> PortResult<UnixMs> {
    db.read(|connection| {
        connection
            .prepare_cached("SELECT CAST(strftime('%s', ?1, 'utc') AS INTEGER) * 1000")?
            .query_row([date], |row| row.get(0).map(UnixMs::from_millis))
    })
}

impl FromSql for LocalDate {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        value
            .as_str()?
            .parse()
            .map_err(|error| FromSqlError::Other(Box::new(error)))
    }
}

impl ToSql for LocalDate {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR_MS: i64 = 3_600_000;
    /// Noon UTC on 2026-01-10: the same local day in every zone within ±11 h.
    const NOON: UnixMs = UnixMs::from_millis(1_768_046_400_000);

    fn date(text: &str) -> LocalDate {
        text.parse().unwrap()
    }

    #[test]
    fn day_start_is_the_first_instant_of_its_local_day() {
        let db = Db::open_in_memory().unwrap();
        let day = date("2026-01-10");
        assert_eq!(local_date(&db, NOON).unwrap(), day);
        let start = day_start(&db, day).unwrap();
        assert_eq!(local_date(&db, start).unwrap(), day);
        let just_before = UnixMs::from_millis(start.as_millis() - 1_000);
        assert_eq!(local_date(&db, just_before).unwrap(), day.previous());
        let next = day_start(&db, day.plus_days(1)).unwrap();
        // 23, 24 or 25 hours, depending on daylight saving in the machine's zone.
        let length = next.as_millis() - start.as_millis();
        assert!((23 * HOUR_MS..=25 * HOUR_MS).contains(&length), "{length}");
    }

    #[test]
    fn every_day_of_a_year_starts_where_the_previous_one_ends() {
        let db = Db::open_in_memory().unwrap();
        let first = date("2026-01-01");
        for offset in 0..366 {
            let day = first.plus_days(offset);
            let start = day_start(&db, day).unwrap();
            assert_eq!(local_date(&db, start).unwrap(), day, "{day}");
            let before = UnixMs::from_millis(start.as_millis() - 1_000);
            assert_eq!(local_date(&db, before).unwrap(), day.previous(), "{day}");
        }
    }
}
