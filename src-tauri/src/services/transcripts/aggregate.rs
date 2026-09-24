/*!
 * SOURCE OF TRUTH KEYWORDS: aggregate transcripts, metrics sums, totals, recent latencies, active days, words per day, activity series, streak input
 * WHAT:  The raw aggregates behind the dashboard (02 §7.4), over completed (`done`) takes only:
 *        `totals` (count, words, recorded ms, speech ms since a time), `recent_latencies` (newest first, for the
 *        median), `active_days` (distinct local dates with a take, newest first, for the streak) and
 *        `words_per_day` (the activity series).
 * WHY:   Metrics are computed from `transcripts`, never counted (05 decision log), and the formulas (time saved,
 *        speaking WPM, median, streak, filling empty days) are business rules for the metrics command, so this
 *        verb returns plain sums and lists. Calendar days use the operating system's local time zone
 *        (SQLite `localtime`), the same days the user sees. Empty windows give zero sums and empty lists.
 * WHERE: metrics_summary / metrics_activity (dashboard step); service tests.
 */

use rusqlite::{Row, params};

use crate::{
    services::db::Db,
    types::{ActivityDay, PortResult, TranscriptStatus, TranscriptTotals, UnixMs},
};

/// The local calendar date (`YYYY-MM-DD`) of `created_at`.
const LOCAL_DAY: &str = "date(created_at / 1000, 'unixepoch', 'localtime')";

/// Count and sums of completed takes created at or after `since` (all of them when None).
pub fn totals(db: &Db, since: Option<UnixMs>) -> PortResult<TranscriptTotals> {
    db.read(|connection| {
        connection
            .prepare_cached(
                "SELECT COUNT(*), COALESCE(SUM(word_count), 0), COALESCE(SUM(duration_ms), 0), \
                 COALESCE(SUM(speech_ms), 0) FROM transcripts \
                 WHERE status = ?1 AND (?2 IS NULL OR created_at >= ?2)",
            )?
            .query_row(
                params![
                    TranscriptStatus::Done.as_str(),
                    since.map(UnixMs::as_millis)
                ],
                |row| {
                    Ok(TranscriptTotals {
                        transcriptions: sum(row, 0)?,
                        words: sum(row, 1)?,
                        duration_ms: sum(row, 2)?,
                        speech_ms: sum(row, 3)?,
                    })
                },
            )
    })
}

/// Latencies of the newest `limit` completed takes created at or after `since`, newest first.
pub fn recent_latencies(db: &Db, since: Option<UnixMs>, limit: u32) -> PortResult<Vec<u32>> {
    db.read(|connection| {
        connection
            .prepare_cached(
                "SELECT latency_ms FROM transcripts \
                 WHERE status = ?1 AND latency_ms IS NOT NULL AND (?2 IS NULL OR created_at >= ?2) \
                 ORDER BY id DESC LIMIT ?3",
            )?
            .query_map(
                params![
                    TranscriptStatus::Done.as_str(),
                    since.map(UnixMs::as_millis),
                    limit
                ],
                |row| row.get(0),
            )?
            .collect()
    })
}

/// Every local date with at least one completed take, newest first.
pub fn active_days(db: &Db) -> PortResult<Vec<String>> {
    let sql = format!(
        "SELECT DISTINCT {LOCAL_DAY} AS day FROM transcripts WHERE status = ?1 ORDER BY day DESC"
    );
    db.read(|connection| {
        connection
            .prepare_cached(&sql)?
            .query_map(params![TranscriptStatus::Done.as_str()], |row| row.get(0))?
            .collect()
    })
}

/// Words of completed takes per local date since `since`, oldest first; days without takes are absent.
pub fn words_per_day(db: &Db, since: UnixMs) -> PortResult<Vec<ActivityDay>> {
    let sql = format!(
        "SELECT {LOCAL_DAY} AS day, COALESCE(SUM(word_count), 0) FROM transcripts \
         WHERE status = ?1 AND created_at >= ?2 GROUP BY day ORDER BY day"
    );
    db.read(|connection| {
        connection
            .prepare_cached(&sql)?
            .query_map(
                params![TranscriptStatus::Done.as_str(), since.as_millis()],
                |row| {
                    Ok(ActivityDay {
                        date: row.get(0)?,
                        words: row.get(1)?,
                    })
                },
            )?
            .collect()
    })
}

/// A non-negative SQL count or sum as u64 (SQLite integers are signed).
fn sum(row: &Row<'_>, index: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(index)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
}

#[cfg(test)]
mod tests {
    use super::super::{
        fixtures::{done, new_take},
        insert, update,
    };
    use super::*;
    use crate::types::{TranscriptChange, TranscriptStatus};

    const DAY_MS: u64 = 86_400_000;
    /// Noon UTC on 2026-01-10, so every fixture take falls well inside one local day in any time zone ±11 h.
    const NOON: u64 = 1_768_046_400_000;

    fn local_day(db: &Db, millis: u64) -> String {
        db.read(|connection| {
            connection.query_row(
                "SELECT date(?1 / 1000, 'unixepoch', 'localtime')",
                params![i64::try_from(millis).unwrap()],
                |row| row.get(0),
            )
        })
        .unwrap()
    }

    #[test]
    fn totals_sum_only_completed_takes_in_the_window() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(totals(&db, None).unwrap(), TranscriptTotals::default());
        done(&db, 1_000, "a b", 2, 100);
        done(&db, 5_000, "c d e", 3, 200);
        insert::insert(&db, &new_take(6_000)).unwrap();
        let failed = done(&db, 7_000, "f", 1, 300);
        update::update(
            &db,
            failed,
            &[TranscriptChange::Status(TranscriptStatus::Failed)],
        )
        .unwrap();

        assert_eq!(
            totals(&db, None).unwrap(),
            TranscriptTotals {
                transcriptions: 2,
                words: 5,
                duration_ms: 2_500,
                speech_ms: 2_000,
            }
        );
        let recent = totals(&db, Some(UnixMs::from_millis(5_000))).unwrap();
        assert_eq!((recent.transcriptions, recent.words), (1, 3));
    }

    #[test]
    fn latencies_are_newest_first_and_limited() {
        let db = Db::open_in_memory().unwrap();
        for (index, latency) in [120, 90, 300, 150].into_iter().enumerate() {
            done(&db, (index as u64 + 1) * 1_000, "x", 1, latency);
        }
        assert_eq!(recent_latencies(&db, None, 3).unwrap(), [150, 300, 90]);
        assert_eq!(
            recent_latencies(&db, Some(UnixMs::from_millis(3_000)), 100).unwrap(),
            [150, 300]
        );
    }

    #[test]
    fn days_and_activity_follow_local_dates() {
        let db = Db::open_in_memory().unwrap();
        assert!(active_days(&db).unwrap().is_empty());
        done(&db, NOON, "a b", 2, 100);
        done(&db, NOON + 60_000, "c", 1, 100);
        done(&db, NOON + 2 * DAY_MS, "d e f", 3, 100);
        let failed = done(&db, NOON + 3 * DAY_MS, "g", 1, 100);
        update::update(
            &db,
            failed,
            &[TranscriptChange::Status(TranscriptStatus::Failed)],
        )
        .unwrap();

        let first = local_day(&db, NOON);
        let third = local_day(&db, NOON + 2 * DAY_MS);
        assert_eq!(active_days(&db).unwrap(), [third.clone(), first.clone()]);
        assert_eq!(
            words_per_day(&db, UnixMs::from_millis(0)).unwrap(),
            [
                ActivityDay {
                    date: first,
                    words: 3
                },
                ActivityDay {
                    date: third.clone(),
                    words: 3
                },
            ]
        );
        let since_second_day = UnixMs::from_millis(i64::try_from(NOON + DAY_MS).unwrap());
        assert_eq!(
            words_per_day(&db, since_second_day).unwrap(),
            [ActivityDay {
                date: third,
                words: 3
            }]
        );
    }
}
