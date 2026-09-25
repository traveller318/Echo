/*!
 * SOURCE OF TRUTH KEYWORDS: metrics summary, metrics activity, compute metrics, range window, local day window, first_day, dashboard numbers
 * WHAT:  `summary(db, settings, range, now)`: every registry summary aggregate over the MetricsRange ending at
 *        `now`, in dashboard order; `activity(db, days, now)`: words and completed takes per local day for the
 *        `days` days ending today, oldest first, every day present; `window_start`: the instant a range begins.
 * WHY:   The formulas are pure (formulas.rs); this is the part that asks the services for exactly the sums the
 *        registry's aggregates need (a series nobody shows is never read). A range of N days is N local calendar
 *        days with today as the last, starting at the local midnight services/calendar reports, the same clock
 *        the aggregates group days with. The streak always reads every kept day, whatever the range: it is a
 *        run of days up to today, not a total over a window. `now` is a parameter, so tests pin the clock and the
 *        command passes the wall clock.
 * WHERE: ipc/commands/metrics.rs (on the blocking pool); tests below run the formulas on a fixed dataset.
 */

use std::num::NonZeroU32;

use super::formulas::{self, SummaryInputs};
use crate::{
    registry,
    services::{Db, calendar, transcripts::aggregate},
    types::{
        ActivityDay, LocalDate, MetricAggregate, MetricValue, MetricsRange, MetricsSummary,
        PortResult, SettingsSnapshot, UnixMs,
    },
};

/// Every summary metric of the registry over `range`, ending at `now`.
pub fn summary(
    db: &Db,
    settings: &SettingsSnapshot,
    range: MetricsRange,
    now: UnixMs,
) -> PortResult<MetricsSummary> {
    let aggregates = registry::metrics::summary_aggregates();
    let wants = |aggregate| aggregates.contains(&aggregate);
    let today = calendar::local_date(db, now)?;
    let since = window_start(db, range, today)?;
    let inputs = SummaryInputs {
        totals: aggregate::totals(db, since)?,
        latencies: if wants(MetricAggregate::MedianLatency) {
            aggregate::recent_latencies(db, since, registry::metrics::MEDIAN_LATENCY_TAKES)?
        } else {
            Vec::new()
        },
        active_days: if wants(MetricAggregate::Streak) {
            aggregate::active_days(db)?
        } else {
            Vec::new()
        },
        today,
        typing_wpm: registry::settings::typing_wpm(settings),
    };
    Ok(MetricsSummary {
        range,
        values: aggregates
            .into_iter()
            .map(|aggregate| MetricValue {
                aggregate,
                value: formulas::metric_value(aggregate, &inputs),
            })
            .collect(),
    })
}

/// Words and completed takes per local day for the `days` days ending today, oldest first.
pub fn activity(db: &Db, days: NonZeroU32, now: UnixMs) -> PortResult<Vec<ActivityDay>> {
    let today = calendar::local_date(db, now)?;
    let first = first_day(today, days);
    let taken = aggregate::words_per_day(db, calendar::day_start(db, first)?)?;
    Ok(formulas::activity_series(&taken, first, today))
}

/// The instant `range` begins when today is `today`; None for every kept take.
pub fn window_start(db: &Db, range: MetricsRange, today: LocalDate) -> PortResult<Option<UnixMs>> {
    range
        .local_days()
        .and_then(NonZeroU32::new)
        .map(|days| calendar::day_start(db, first_day(today, days)))
        .transpose()
}

/// The first of the `days` local days that end with `today`.
fn first_day(today: LocalDate, days: NonZeroU32) -> LocalDate {
    let back = i32::try_from(days.get() - 1).unwrap_or(i32::MAX);
    today.plus_days(-back)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        registry::settings::{self, keys},
        services::transcripts::{insert, update},
        types::{
            AppPaths, EngineId, NewTranscript, SettingValue, TranscriptChange, TranscriptId,
            TranscriptStatus,
        },
    };

    const MINUTE_MS: i64 = 60_000;
    const HOUR_MS: i64 = 60 * MINUTE_MS;

    fn date(text: &str) -> LocalDate {
        text.parse().unwrap()
    }

    fn at(millis: i64) -> UnixMs {
        UnixMs::from_millis(millis)
    }

    /// Local midnight of `text` plus `offset` ms.
    fn local(db: &Db, text: &str, offset: i64) -> UnixMs {
        at(calendar::day_start(db, date(text)).unwrap().as_millis() + offset)
    }

    struct Take {
        words: u32,
        duration_ms: u32,
        speech_ms: u32,
        latency_ms: u32,
    }

    /// Stores one take created at `created` with the given final status and measurements.
    fn store(db: &Db, created: UnixMs, status: TranscriptStatus, take: &Take) {
        let millis = u64::try_from(created.as_millis()).unwrap();
        let id: TranscriptId = ulid::Ulid::from_parts(millis, ulid::Ulid::generate().random())
            .to_string()
            .parse()
            .unwrap();
        insert::insert(
            db,
            &NewTranscript {
                id,
                created_at: created,
                status: TranscriptStatus::Recording,
                audio_path: Some(AppPaths::recording_name(id)),
                engine_id: Some(EngineId::from_static("parakeet-tdt-0.6b-v3")),
                app_name: None,
            },
        )
        .unwrap();
        update::update(
            db,
            id,
            &[
                TranscriptChange::Status(status),
                TranscriptChange::WordCount(take.words),
                TranscriptChange::DurationMs(take.duration_ms),
                TranscriptChange::SpeechMs(take.speech_ms),
                TranscriptChange::LatencyMs(take.latency_ms),
            ],
        )
        .unwrap();
    }

    fn done(
        db: &Db,
        created: UnixMs,
        words: u32,
        duration_ms: u32,
        speech_ms: u32,
        latency_ms: u32,
    ) {
        let take = Take {
            words,
            duration_ms,
            speech_ms,
            latency_ms,
        };
        store(db, created, TranscriptStatus::Done, &take);
    }

    /**
     * The fixed dataset, with "now" at 2026-03-12 10:00 local time:
     *   03-12 09:00  120 words, 40 s recorded, 30 s speech, 200 ms
     *   03-11 23:59  60 words, 20 s, 18 s, 150 ms   (one minute before midnight)
     *   03-11 00:01  30 words, 10 s, 9 s, 400 ms    (one minute after midnight)
     *   03-10 12:00  90 words, 30 s, 27 s, 100 ms
     *   03-08 12:00  45 words, 15 s, 12 s, 250 ms   (breaks the run: nothing on 03-09)
     *   02-20 12:00  300 words, 100 s, 90 s, 300 ms
     *   03-12 08:00  a failed take and 03-11 12:00 an empty one, which no metric counts.
     */
    fn dataset() -> (Db, UnixMs) {
        let db = Db::open_in_memory().unwrap();
        done(
            &db,
            local(&db, "2026-03-12", 9 * HOUR_MS),
            120,
            40_000,
            30_000,
            200,
        );
        done(
            &db,
            local(&db, "2026-03-12", -MINUTE_MS),
            60,
            20_000,
            18_000,
            150,
        );
        done(
            &db,
            local(&db, "2026-03-11", MINUTE_MS),
            30,
            10_000,
            9_000,
            400,
        );
        done(
            &db,
            local(&db, "2026-03-10", 12 * HOUR_MS),
            90,
            30_000,
            27_000,
            100,
        );
        done(
            &db,
            local(&db, "2026-03-08", 12 * HOUR_MS),
            45,
            15_000,
            12_000,
            250,
        );
        done(
            &db,
            local(&db, "2026-02-20", 12 * HOUR_MS),
            300,
            100_000,
            90_000,
            300,
        );
        let ignored = Take {
            words: 999,
            duration_ms: 999_000,
            speech_ms: 999_000,
            latency_ms: 9,
        };
        store(
            &db,
            local(&db, "2026-03-12", 8 * HOUR_MS),
            TranscriptStatus::Failed,
            &ignored,
        );
        store(
            &db,
            local(&db, "2026-03-11", 12 * HOUR_MS),
            TranscriptStatus::Empty,
            &ignored,
        );
        let now = local(&db, "2026-03-12", 10 * HOUR_MS);
        (db, now)
    }

    fn values(summary: &MetricsSummary) -> Vec<(MetricAggregate, Option<f64>)> {
        summary
            .values
            .iter()
            .map(|value| (value.aggregate, value.value))
            .collect()
    }

    #[test]
    fn all_time_summary_matches_the_hand_computed_answers() {
        let (db, now) = dataset();
        let summary = summary(&db, &settings::defaults(), MetricsRange::AllTime, now).unwrap();
        assert_eq!(summary.range, MetricsRange::AllTime);
        // 645 words; typing at 40 wpm takes 645 / 40 min = 967 500 ms; 215 000 ms were recorded.
        // Speech: 186 000 ms → 645 / 186 000 × 60 000 = 208.06… wpm.
        // Latencies 100 150 200 250 300 400 → median (200 + 250) / 2 = 225.
        // Streak: 03-12, 03-11, 03-10 (03-11 counts once for both of its takes), then the gap on 03-09.
        assert_eq!(
            values(&summary),
            [
                (MetricAggregate::TimeSaved, Some(752_500.0)),
                (MetricAggregate::Words, Some(645.0)),
                (MetricAggregate::Transcriptions, Some(6.0)),
                (
                    MetricAggregate::SpeakingWpm,
                    Some(645.0 / 186_000.0 * 60_000.0)
                ),
                (MetricAggregate::MedianLatency, Some(225.0)),
                (MetricAggregate::Streak, Some(3.0)),
            ]
        );
    }

    #[test]
    fn ranges_are_whole_local_days_ending_today() {
        let (db, now) = dataset();
        let defaults = settings::defaults();
        let words = |range| {
            summary(&db, &defaults, range, now)
                .unwrap()
                .value_of(MetricAggregate::Words)
        };
        assert_eq!(words(MetricsRange::Today), Some(120.0), "only 03-12");
        // 03-06 … 03-12: everything but 02-20.
        assert_eq!(words(MetricsRange::Last7Days), Some(345.0));
        assert_eq!(
            words(MetricsRange::Last30Days),
            Some(645.0),
            "02-11 … 03-12"
        );

        let today = summary(&db, &defaults, MetricsRange::Today, now).unwrap();
        assert_eq!(today.value_of(MetricAggregate::MedianLatency), Some(200.0));
        assert_eq!(
            today.value_of(MetricAggregate::Streak),
            Some(3.0),
            "the streak does not depend on the range"
        );
        assert_eq!(
            window_start(&db, MetricsRange::Today, date("2026-03-12")).unwrap(),
            Some(local(&db, "2026-03-12", 0))
        );
        assert_eq!(
            window_start(&db, MetricsRange::AllTime, date("2026-03-12")).unwrap(),
            None
        );
    }

    #[test]
    fn the_streak_follows_local_midnight() {
        let (db, _) = dataset();
        let defaults = settings::defaults();
        let streak = |now| {
            summary(&db, &defaults, MetricsRange::AllTime, now)
                .unwrap()
                .value_of(MetricAggregate::Streak)
        };
        // One minute before midnight on 03-12 the run still ends today; just after it, 03-13 has no take yet
        // and the run through yesterday still counts; a day later it has lapsed.
        assert_eq!(streak(local(&db, "2026-03-13", -MINUTE_MS)), Some(3.0));
        assert_eq!(streak(local(&db, "2026-03-13", MINUTE_MS)), Some(3.0));
        assert_eq!(streak(local(&db, "2026-03-14", MINUTE_MS)), Some(0.0));
        // Seen from 03-11 just after midnight, the 03-12 take lies in the future and is not counted.
        assert_eq!(streak(local(&db, "2026-03-11", 2 * MINUTE_MS)), Some(2.0));
    }

    #[test]
    fn typing_speed_changes_only_time_saved() {
        let (db, now) = dataset();
        let fast = settings::resolve([(keys::TYPING_WPM, SettingValue::Int(129))]);
        let summary = summary(&db, &fast, MetricsRange::AllTime, now).unwrap();
        // 645 words at 129 wpm = 300 000 ms of typing, minus 215 000 ms recorded.
        assert_eq!(summary.value_of(MetricAggregate::TimeSaved), Some(85_000.0));
        assert_eq!(summary.value_of(MetricAggregate::Words), Some(645.0));
    }

    #[test]
    fn activity_is_one_entry_per_local_day_with_zeros() {
        let (db, now) = dataset();
        let series = activity(&db, NonZeroU32::new(5).unwrap(), now).unwrap();
        let days: Vec<(String, u32, u32)> = series
            .iter()
            .map(|day| (day.date.to_string(), day.words, day.transcriptions))
            .collect();
        assert_eq!(
            days,
            [
                ("2026-03-08".to_owned(), 45, 1),
                ("2026-03-09".to_owned(), 0, 0),
                ("2026-03-10".to_owned(), 90, 1),
                ("2026-03-11".to_owned(), 90, 2),
                ("2026-03-12".to_owned(), 120, 1),
            ]
        );
        let thirty = activity(&db, NonZeroU32::new(30).unwrap(), now).unwrap();
        assert_eq!(thirty.len(), 30);
        assert_eq!(thirty.first().map(|day| day.date), Some(date("2026-02-11")));
        assert_eq!(thirty.iter().map(|day| day.words).sum::<u32>(), 645);
        let today = activity(&db, NonZeroU32::MIN, now).unwrap();
        assert_eq!(today.len(), 1);
        assert_eq!(today[0].words, 120);
    }

    #[test]
    fn an_empty_database_has_no_values_and_a_zero_series() {
        let db = Db::open_in_memory().unwrap();
        let now = local(&db, "2026-03-12", 10 * HOUR_MS);
        let summary = summary(&db, &settings::defaults(), MetricsRange::AllTime, now).unwrap();
        assert_eq!(summary.values.len(), MetricAggregate::ALL.len());
        assert!(summary.values.iter().all(|value| value.value.is_none()));
        let series = activity(&db, NonZeroU32::new(30).unwrap(), now).unwrap();
        assert_eq!(series.len(), 30);
        assert!(
            series
                .iter()
                .all(|day| day.words == 0 && day.transcriptions == 0)
        );
        assert_eq!(series.last().map(|day| day.date), Some(date("2026-03-12")));
    }
}
