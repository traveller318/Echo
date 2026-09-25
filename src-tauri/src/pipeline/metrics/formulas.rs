/*!
 * SOURCE OF TRUTH KEYWORDS: metric formulas, time saved formula, speaking WPM formula, median latency, streak rule, activity series fill, metric_value, SummaryInputs
 * WHAT:  The 02 §7.4 formulas as pure functions over the raw aggregates: time saved, speaking WPM, the median of
 *        the recent latencies, the streak, the zero-filled activity series, and `metric_value`, which answers one
 *        registry MetricAggregate from a SummaryInputs.
 * WHY:   Metrics are computed from `transcripts`, never counted (05 decision log), and the services return plain
 *        sums and lists, so every rule lives here where it is tested on hand-computed data without a database.
 *        None means "nothing to compute from yet" (no completed take in the window, no speech time, no latency,
 *        no active day ever) so the UI shows a dash instead of a misleading 0; a real zero (a streak that
 *        lapsed) stays Some(0). Time saved is negative when someone speaks slower than they type, and that is
 *        reported as it is. The streak counts consecutive local days with a completed take ending today, or
 *        ending yesterday while today has none yet, so it does not read 0 every morning before the first take;
 *        dates after today (a clock moved back) are ignored. `metric_value` matches every aggregate
 *        exhaustively, so a new registry aggregate fails to compile until its formula exists.
 * WHERE: pipeline/metrics/compute.rs (metrics_summary / metrics_activity); tests below.
 */

use std::num::NonZeroU32;

use crate::types::{ActivityDay, LocalDate, MetricAggregate, TranscriptTotals};

const MS_PER_MINUTE: f64 = 60_000.0;

/// What the summary formulas read, gathered once per `metrics_summary`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryInputs {
    /// Sums over the completed takes in the window.
    pub totals: TranscriptTotals,
    /// Latencies of the newest completed takes in the window (any order).
    pub latencies: Vec<u32>,
    /// Every local date with a completed take, newest first.
    pub active_days: Vec<LocalDate>,
    /// Today's local date.
    pub today: LocalDate,
    /// `metrics.typing_wpm`.
    pub typing_wpm: NonZeroU32,
}

/// One summary aggregate from the gathered inputs.
pub fn metric_value(aggregate: MetricAggregate, inputs: &SummaryInputs) -> Option<f64> {
    let totals = &inputs.totals;
    let any_take = totals.transcriptions > 0;
    match aggregate {
        MetricAggregate::Words => any_take.then_some(count(totals.words)),
        MetricAggregate::Transcriptions => any_take.then_some(count(totals.transcriptions)),
        MetricAggregate::TimeSaved => time_saved_ms(totals, inputs.typing_wpm),
        MetricAggregate::SpeakingWpm => speaking_wpm(totals),
        MetricAggregate::MedianLatency => median(&inputs.latencies),
        MetricAggregate::Streak => streak(&inputs.active_days, inputs.today).map(f64::from),
    }
}

/// Σ(words ÷ typing WPM × 60 s) − Σ(recorded time), in ms; None without a completed take.
pub fn time_saved_ms(totals: &TranscriptTotals, typing_wpm: NonZeroU32) -> Option<f64> {
    (totals.transcriptions > 0).then(|| {
        count(totals.words) * MS_PER_MINUTE / f64::from(typing_wpm.get())
            - count(totals.duration_ms)
    })
}

/// Σ words ÷ Σ speech time × 60 000; None without a completed take or any measured speech.
pub fn speaking_wpm(totals: &TranscriptTotals) -> Option<f64> {
    (totals.transcriptions > 0 && totals.speech_ms > 0)
        .then(|| count(totals.words) / count(totals.speech_ms) * MS_PER_MINUTE)
}

/// The middle value (the mean of the two middle values for an even count); None for no values.
pub fn median(values: &[u32]) -> Option<f64> {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let middle = sorted.len() / 2;
    match sorted.len() {
        0 => None,
        length if length % 2 == 1 => sorted.get(middle).copied().map(f64::from),
        _ => {
            let low = sorted.get(middle - 1).copied().map(f64::from)?;
            let high = sorted.get(middle).copied().map(f64::from)?;
            Some(f64::midpoint(low, high))
        }
    }
}

/// Consecutive days with a completed take ending today (or yesterday); None when no day ever had one.
pub fn streak(active_days_newest_first: &[LocalDate], today: LocalDate) -> Option<u32> {
    if active_days_newest_first.is_empty() {
        return None;
    }
    let mut days = active_days_newest_first
        .iter()
        .copied()
        .filter(|day| *day <= today)
        .peekable();
    let mut expected = match days.peek() {
        Some(latest) if *latest == today => today,
        Some(latest) if *latest == today.previous() => today.previous(),
        _ => return Some(0),
    };
    let mut length = 0_u32;
    for day in days {
        if day == expected {
            length = length.saturating_add(1);
            expected = expected.previous();
        } else if day < expected {
            break;
        }
        // A repeated day (day > expected) is skipped.
    }
    Some(length)
}

/// Every day from `first` to `last` inclusive, oldest first, with `days` (any order) filled in and the rest zero.
pub fn activity_series(
    days: &[ActivityDay],
    first: LocalDate,
    last: LocalDate,
) -> Vec<ActivityDay> {
    let length = usize::try_from(last.days_since(first).saturating_add(1)).unwrap_or(0);
    let mut series: Vec<ActivityDay> = (0..length)
        .map_while(|offset| i32::try_from(offset).ok())
        .map(|offset| ActivityDay::empty(first.plus_days(offset)))
        .collect();
    for day in days {
        let slot = usize::try_from(day.date.days_since(first))
            .ok()
            .and_then(|index| series.get_mut(index));
        if let Some(slot) = slot {
            slot.words = slot.words.saturating_add(day.words);
            slot.transcriptions = slot.transcriptions.saturating_add(day.transcriptions);
        }
    }
    series
}

/// A sum or count as f64 (exact below 2^53, far beyond any history).
fn count(value: u64) -> f64 {
    value as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(text: &str) -> LocalDate {
        text.parse().unwrap()
    }

    fn wpm(value: u32) -> NonZeroU32 {
        NonZeroU32::new(value).unwrap()
    }

    /// Three takes: 50 + 100 + 150 words, 20 + 40 + 60 s recorded, 15 + 30 + 45 s of speech.
    const TOTALS: TranscriptTotals = TranscriptTotals {
        transcriptions: 3,
        words: 300,
        duration_ms: 120_000,
        speech_ms: 90_000,
    };

    fn inputs() -> SummaryInputs {
        SummaryInputs {
            totals: TOTALS,
            latencies: vec![300, 120, 180, 150],
            active_days: vec![date("2026-09-25"), date("2026-09-24"), date("2026-09-20")],
            today: date("2026-09-25"),
            typing_wpm: wpm(40),
        }
    }

    #[test]
    fn time_saved_is_typing_time_minus_recorded_time() {
        // 300 words at 40 wpm = 7.5 min = 450 000 ms of typing; 120 000 ms recorded.
        assert_eq!(time_saved_ms(&TOTALS, wpm(40)), Some(330_000.0));
        // At 200 wpm typing takes 90 000 ms: speaking was slower, so the saving is negative.
        assert_eq!(time_saved_ms(&TOTALS, wpm(200)), Some(-30_000.0));
        assert_eq!(time_saved_ms(&TranscriptTotals::default(), wpm(40)), None);
    }

    #[test]
    fn speaking_wpm_is_words_per_minute_of_speech() {
        // 300 words in 90 s of speech = 200 wpm.
        assert_eq!(speaking_wpm(&TOTALS), Some(200.0));
        let silent = TranscriptTotals {
            speech_ms: 0,
            ..TOTALS
        };
        assert_eq!(speaking_wpm(&silent), None, "no speech measured, no rate");
        assert_eq!(speaking_wpm(&TranscriptTotals::default()), None);
    }

    #[test]
    fn median_takes_the_middle_or_the_mean_of_the_middle_two() {
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[250]), Some(250.0));
        assert_eq!(median(&[300, 120, 180]), Some(180.0));
        assert_eq!(median(&[300, 120, 180, 150]), Some(165.0));
        assert_eq!(median(&[u32::MAX, u32::MAX]), Some(f64::from(u32::MAX)));
    }

    #[test]
    fn streak_counts_back_from_today_or_yesterday() {
        let today = date("2026-09-25");
        assert_eq!(streak(&[], today), None, "no take ever");
        assert_eq!(streak(&[today], today), Some(1));
        let run = [
            date("2026-09-25"),
            date("2026-09-24"),
            date("2026-09-23"),
            date("2026-09-21"),
        ];
        assert_eq!(streak(&run, today), Some(3), "the gap on the 22nd ends it");
        let until_yesterday = [date("2026-09-24"), date("2026-09-23")];
        assert_eq!(
            streak(&until_yesterday, today),
            Some(2),
            "today can still continue it"
        );
        assert_eq!(streak(&[date("2026-09-23")], today), Some(0), "lapsed");
        let with_future = [date("2026-10-02"), date("2026-09-25"), date("2026-09-24")];
        assert_eq!(
            streak(&with_future, today),
            Some(2),
            "a future date is ignored"
        );
        let repeated = [today, today, today.previous()];
        assert_eq!(streak(&repeated, today), Some(2));
    }

    #[test]
    fn streak_crosses_months_and_years() {
        let days = [date("2026-01-01"), date("2025-12-31"), date("2025-12-30")];
        assert_eq!(streak(&days, date("2026-01-01")), Some(3));
        let leap = [date("2024-03-01"), date("2024-02-29"), date("2024-02-28")];
        assert_eq!(streak(&leap, date("2024-03-01")), Some(3));
    }

    #[test]
    fn activity_series_fills_every_day_of_the_window() {
        let first = date("2026-09-22");
        let last = date("2026-09-25");
        let taken = [
            ActivityDay {
                date: date("2026-09-23"),
                words: 40,
                transcriptions: 2,
            },
            ActivityDay {
                date: date("2026-09-25"),
                words: 7,
                transcriptions: 1,
            },
            ActivityDay {
                date: date("2026-09-30"),
                words: 99,
                transcriptions: 9,
            },
        ];
        let series = activity_series(&taken, first, last);
        let words: Vec<(String, u32, u32)> = series
            .iter()
            .map(|day| (day.date.to_string(), day.words, day.transcriptions))
            .collect();
        assert_eq!(
            words,
            [
                ("2026-09-22".to_owned(), 0, 0),
                ("2026-09-23".to_owned(), 40, 2),
                ("2026-09-24".to_owned(), 0, 0),
                ("2026-09-25".to_owned(), 7, 1),
            ]
        );
        assert_eq!(activity_series(&taken, last, last).len(), 1);
        assert!(
            activity_series(&taken, last, first).is_empty(),
            "an inverted window is empty"
        );
    }

    #[test]
    fn every_aggregate_answers_from_the_inputs() {
        let inputs = inputs();
        let values: Vec<Option<f64>> = MetricAggregate::ALL
            .iter()
            .map(|aggregate| metric_value(*aggregate, &inputs))
            .collect();
        // Words, Transcriptions, TimeSaved, SpeakingWpm, MedianLatency, Streak.
        assert_eq!(
            values,
            [
                Some(300.0),
                Some(3.0),
                Some(330_000.0),
                Some(200.0),
                Some(165.0),
                Some(2.0)
            ]
        );
    }

    #[test]
    fn an_empty_window_has_no_values_but_the_streak_remembers_older_days() {
        let empty = SummaryInputs {
            totals: TranscriptTotals::default(),
            latencies: Vec::new(),
            active_days: vec![date("2026-09-01")],
            ..inputs()
        };
        for aggregate in MetricAggregate::ALL {
            let expected = (aggregate == MetricAggregate::Streak).then_some(0.0);
            assert_eq!(metric_value(aggregate, &empty), expected, "{aggregate:?}");
        }
        let never = SummaryInputs {
            active_days: Vec::new(),
            ..empty
        };
        assert!(
            MetricAggregate::ALL
                .iter()
                .all(|aggregate| metric_value(*aggregate, &never).is_none())
        );
    }
}
