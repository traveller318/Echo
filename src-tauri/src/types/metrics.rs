/*!
 * SOURCE OF TRUTH KEYWORDS: MetricSpec, LogMetricSpec, MetricQuery, MetricEmphasis, MetricsSummary, MetricValue, MetricAggregate, MetricUnit, MetricsRange, ActivityDay
 * WHAT:  Dashboard metric shapes: the registry entry (MetricSpec: label, unit, query, emphasis), which SQL
 *        aggregate or series a metric reads (MetricQuery / MetricAggregate), how it is displayed (MetricUnit,
 *        MetricEmphasis), the time window (MetricsRange), the computed results (MetricsSummary, ActivityDay) and
 *        the log-only metrics (LogMetricSpec).
 * WHY:   Metrics are computed from `transcripts`, never counted (02 §7.4, 05 decision log). The dashboard renders
 *        registry metric entries and looks each value up by aggregate, so adding a metric is a registry entry plus
 *        one aggregate, not a new component. Emphasis places a metric in the 04 §5 dashboard layout (hero, stat
 *        card, small card) so the page never switches on a metric id. Values are f64 because averages and time saved are fractional and
 *        sums can exceed u32; None means "no data yet", not zero.
 * WHERE: registry/metrics entries; services/transcripts/aggregate; `metrics_summary` / `metrics_activity` commands.
 */

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{MetricId, StaticStr};

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

/// The window a summary covers, ending now.
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct MetricValue {
    pub aggregate: MetricAggregate,
    /// None when there is no completed take in the range.
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

/// Words delivered on one local calendar day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ActivityDay {
    /// Local date as `YYYY-MM-DD`.
    pub date: String,
    pub words: u32,
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
}
