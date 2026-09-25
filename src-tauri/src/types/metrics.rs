/*!
 * SOURCE OF TRUTH KEYWORDS: MetricSpec, LogMetricSpec, MetricQuery, MetricEmphasis, MetricsSummary, MetricValue, MetricAggregate, MetricUnit, MetricsRange, ActivityDay, TranscriptTotals, MetricsSummaryInput, MetricsActivityInput
 * WHAT:  Dashboard metric shapes: the registry entry (MetricSpec: label, unit, query, emphasis), which SQL
 *        aggregate or series a metric reads (MetricQuery / MetricAggregate), how it is displayed (MetricUnit,
 *        MetricEmphasis), the time window (MetricsRange), the command inputs (MetricsSummaryInput,
 *        MetricsActivityInput), the computed results (MetricsSummary, ActivityDay) and
 *        the log-only metrics (LogMetricSpec) and the raw sums the aggregates are computed from (TranscriptTotals).
 * WHY:   Metrics are computed from `transcripts`, never counted (02 §7.4, 05 decision log). The dashboard renders
 *        registry metric entries and looks each value up by aggregate, so adding a metric is a registry entry plus
 *        one aggregate, not a new component. Emphasis places a metric in the 04 §5 dashboard layout (hero, stat
 *        card, small card) so the page never switches on a metric id. Values are f64 because averages and time saved are fractional and
 *        sums can exceed u32; None means "no data yet", not zero.
 * WHERE: registry/metrics entries; services/transcripts/aggregate; `metrics_summary` / `metrics_activity` commands.
 */

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{LocalDate, MetricId, StaticStr};

/// How a metric value is formatted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MetricUnit {
    /// Milliseconds shown as a human duration (e.g. `2 h 5 min`).
    Duration,
    Count,
    Wpm,
    /// Milliseconds shown as-is (latency).
    Ms,
    Days,
}

/// The SQL aggregate behind a metric (formulas in 02 §7.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MetricAggregate {
    Words,
    Transcriptions,
    TimeSaved,
    SpeakingWpm,
    MedianLatency,
    Streak,
}

impl MetricAggregate {
    /// Every aggregate, in declaration order.
    pub const ALL: [Self; 6] = [
        Self::Words,
        Self::Transcriptions,
        Self::TimeSaved,
        Self::SpeakingWpm,
        Self::MedianLatency,
        Self::Streak,
    ];
}

/// What a metric reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MetricQuery {
    /// One value of `metrics_summary(range)`.
    Summary { aggregate: MetricAggregate },
    /// Words per day for the last `days` days, from `metrics_activity(days)`.
    Activity { days: u32 },
}

/// Where a metric sits on the dashboard (04 §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MetricEmphasis {
    /// The one large card at the top.
    Hero,
    /// A regular stat card, or the activity chart for an `activity` query.
    Primary,
    /// A small card below the chart.
    Secondary,
}

/// A registry metric entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct MetricSpec {
    pub id: MetricId,
    pub label: StaticStr,
    /// One sentence on how the value is computed, shown as the card's tooltip.
    pub help: StaticStr,
    pub unit: MetricUnit,
    pub query: MetricQuery,
    pub emphasis: MetricEmphasis,
}

/**
 * SOURCE OF TRUTH KEYWORDS: LogMetricSpec, log-only metric, local log metric, command duration, diagnostics, timing
 * WHAT:  A registry metric that is written only to the local log (02 §12): its id, what it measures and its unit.
 * WHY:   Operational timings (command duration and outcome, 02 §4.1 step 7; later the per-take stage breakdown)
 *        are registry entries like the dashboard metrics (root CLAUDE.md §3), but they are never shown, never
 *        stored in SQLite and never leave the machine, so they carry no query or emphasis and do not cross IPC.
 *        The id is written as the `metric` field of the log line, so the log can be filtered by it.
 * WHERE: Entries in registry/metrics (`LOG_METRICS`); recorded by ipc/factory.rs and, later, pipeline stages.
 */
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogMetricSpec {
    pub id: MetricId,
    /// One sentence on what one log line of this metric measures.
    pub help: StaticStr,
    pub unit: MetricUnit,
}

/// The window a summary covers, ending now: whole local calendar days counted back from today, or every kept take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MetricsRange {
    Today,
    #[serde(rename = "last_7_days")]
    Last7Days,
    #[serde(rename = "last_30_days")]
    Last30Days,
    AllTime,
}

impl MetricsRange {
    /// Every range, shortest first (the order a range picker offers them).
    pub const ALL: [Self; 4] = [
        Self::Today,
        Self::Last7Days,
        Self::Last30Days,
        Self::AllTime,
    ];

    /// Local calendar days the range covers, today included; None for every kept take.
    pub const fn local_days(self) -> Option<u32> {
        match self {
            Self::Today => Some(1),
            Self::Last7Days => Some(7),
            Self::Last30Days => Some(30),
            Self::AllTime => None,
        }
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: MetricsSummaryInput, metrics_summary input, dashboard range, summary window
 * WHAT:  The input of `metrics_summary`: which MetricsRange to compute over.
 * WHY:   A struct, not a bare enum, so the command can grow options (a comparison window for trends) without
 *        changing its call sites, like every other command input. serde already refuses an unknown range (the
 *        factory maps that to `Validation`, 05 W27), so garde has nothing left to check.
 * WHERE: ipc/commands/metrics.rs; built in the UI by the dashboard summary query (src/hooks/use-metrics.ts).
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, garde::Validate)]
pub struct MetricsSummaryInput {
    #[garde(skip)]
    pub range: MetricsRange,
}

/**
 * SOURCE OF TRUTH KEYWORDS: MetricsActivityInput, metrics_activity input, activity days, chart length, garde schema
 * WHAT:  The input of `metrics_activity`: how many local calendar days, today included, the series covers.
 * WHY:   The factory enforces 1..=MAX_DAYS before the handler runs (02 §4.1), so the series is never empty and
 *        never an unbounded allocation; a year and a day covers any chart the dashboard could draw. The UI reads
 *        the days from the registry's activity metric (`MetricQuery::Activity { days }`), never its own number.
 * WHERE: ipc/commands/metrics.rs; built in the UI by the dashboard activity query (src/hooks/use-metrics.ts).
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, garde::Validate)]
pub struct MetricsActivityInput {
    #[garde(range(min = 1, max = Self::MAX_DAYS))]
    pub days: u32,
}

impl MetricsActivityInput {
    /// Longest activity series a caller may ask for, in days.
    pub const MAX_DAYS: u32 = 366;
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct MetricValue {
    pub aggregate: MetricAggregate,
    /// None when there is nothing to compute it from yet (no completed take in the range, no speech time, no
    /// latency), so the UI can tell "no data" from zero.
    pub value: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct MetricsSummary {
    pub range: MetricsRange,
    pub values: Vec<MetricValue>,
}

impl MetricsSummary {
    pub fn value_of(&self, aggregate: MetricAggregate) -> Option<f64> {
        self.values
            .iter()
            .find(|metric| metric.aggregate == aggregate)
            .and_then(|metric| metric.value)
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: TranscriptTotals, raw aggregates, metrics sums, done takes totals, time saved inputs
 * WHAT:  Raw SQL sums over completed takes in a window: how many, and their words, recorded time and speech time.
 * WHY:   Metrics are formulas over these sums (02 §7.4): time saved, speaking WPM and averages are computed by the
 *        metrics command from them, so the service stays one aggregate query with no business rule. Sums are u64
 *        because a long history can exceed u32; the type never crosses IPC (MetricsSummary does).
 * WHERE: Returned by services/transcripts/aggregate::totals; read by pipeline/metrics (the summary formulas).
 */
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TranscriptTotals {
    pub transcriptions: u64,
    pub words: u64,
    pub duration_ms: u64,
    pub speech_ms: u64,
}

/// Words and completed takes on one local calendar day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ActivityDay {
    /// Local date, `YYYY-MM-DD`.
    #[specta(type = String)]
    pub date: LocalDate,
    pub words: u32,
    pub transcriptions: u32,
}

impl ActivityDay {
    /// A day with no completed take.
    pub const fn empty(date: LocalDate) -> Self {
        Self {
            date,
            words: 0,
            transcriptions: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn specs_are_const_and_queries_are_tagged() {
        const WORDS: MetricSpec = MetricSpec {
            id: MetricId::from_static("words"),
            label: StaticStr::new("Words"),
            help: StaticStr::new("Words delivered by completed takes."),
            unit: MetricUnit::Count,
            query: MetricQuery::Summary {
                aggregate: MetricAggregate::Words,
            },
            emphasis: MetricEmphasis::Primary,
        };
        let value = serde_json::to_value(&WORDS).unwrap();
        assert_eq!(
            value["query"],
            json!({ "kind": "summary", "aggregate": "words" })
        );
        assert_eq!(
            serde_json::to_value(MetricQuery::Activity { days: 30 }).unwrap(),
            json!({ "kind": "activity", "days": 30 })
        );
        assert_eq!(serde_json::from_value::<MetricSpec>(value).unwrap(), WORDS);
    }

    #[test]
    fn summary_looks_values_up_by_aggregate() {
        let summary = MetricsSummary {
            range: MetricsRange::Last7Days,
            values: vec![
                MetricValue {
                    aggregate: MetricAggregate::Words,
                    value: Some(120.0),
                },
                MetricValue {
                    aggregate: MetricAggregate::MedianLatency,
                    value: None,
                },
            ],
        };
        assert_eq!(summary.value_of(MetricAggregate::Words), Some(120.0));
        assert_eq!(summary.value_of(MetricAggregate::MedianLatency), None);
        assert_eq!(summary.value_of(MetricAggregate::Streak), None);
        assert_eq!(
            serde_json::to_value(MetricsRange::Last7Days).unwrap(),
            "last_7_days"
        );
        assert_eq!(
            serde_json::to_value(MetricsRange::Last30Days).unwrap(),
            "last_30_days"
        );
    }

    #[test]
    fn ranges_count_local_days_and_all_time_is_unbounded() {
        let days: Vec<Option<u32>> = MetricsRange::ALL
            .iter()
            .map(|range| range.local_days())
            .collect();
        assert_eq!(days, [Some(1), Some(7), Some(30), None]);
        let input: MetricsSummaryInput =
            serde_json::from_value(json!({ "range": "all_time" })).unwrap();
        assert_eq!(input.range, MetricsRange::AllTime);
        assert!(
            serde_json::from_value::<MetricsSummaryInput>(json!({ "range": "forever" })).is_err()
        );
    }

    #[test]
    fn activity_input_bounds_are_declared() {
        use garde::Validate;
        for (days, valid) in [
            (0, false),
            (1, true),
            (30, true),
            (MetricsActivityInput::MAX_DAYS, true),
            (MetricsActivityInput::MAX_DAYS + 1, false),
        ] {
            assert_eq!(
                MetricsActivityInput { days }.validate().is_ok(),
                valid,
                "{days}"
            );
        }
    }

    #[test]
    fn activity_days_carry_their_date_as_text() {
        let day = ActivityDay {
            date: "2026-09-25".parse().unwrap(),
            words: 42,
            transcriptions: 3,
        };
        assert_eq!(
            serde_json::to_value(day).unwrap(),
            json!({ "date": "2026-09-25", "words": 42, "transcriptions": 3 })
        );
        assert_eq!(ActivityDay::empty(day.date).words, 0);
    }
}
