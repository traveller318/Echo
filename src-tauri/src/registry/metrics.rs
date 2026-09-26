/*!
 * SOURCE OF TRUTH KEYWORDS: metrics registry, METRICS, LOG_METRICS, COMMAND_DURATION, ACCELERATOR_BENCHMARK, dashboard metrics, time saved, speaking WPM, median latency, streak, activity, MEDIAN_LATENCY_TAKES
 * WHAT:  Every dashboard metric of 02 §7.4 with its label, help, unit, query (a summary aggregate or the daily
 *        activity series) and emphasis, plus lookups; and every log-only metric (LOG_METRICS, 02 §12), such as
 *        the command duration the factory records.
 * WHY:   The dashboard renders these entries and the metrics commands compute the aggregates they name, so adding
 *        a metric is an entry here plus one aggregate (02 §3.3). Values are always computed from `transcripts`,
 *        never counted (05 decision log). Order within an emphasis is the order on the page (04 §5).
 * WHERE: Sent to the UI by `registry_get`; `summary_aggregates` tells `metrics_summary` what to compute;
 *        `inputs_changed` tells a settings write whether the dashboard must read again;
 *        `ACTIVITY_DAYS` is the chart's length (the UI asks `metrics_activity` for it through the activity entry);
 *        `MEDIAN_LATENCY_TAKES` bounds the median in pipeline/metrics; `ACCELERATOR_BENCHMARK` by pipeline/asr/accelerator.rs; `COMMAND_DURATION` is recorded by
 *        ipc/factory.rs.
 */

use super::settings::keys;
use crate::types::{
    LogMetricSpec, MetricAggregate, MetricEmphasis, MetricId, MetricQuery, MetricSpec, MetricUnit,
    SettingKey, SettingsSnapshot, StaticStr,
};

/// Days the activity chart covers.
pub const ACTIVITY_DAYS: u32 = 30;

/// Newest completed takes whose latencies the median is taken over (02 §7.4 "last 100").
pub const MEDIAN_LATENCY_TAKES: u32 = 100;

/// Every metric, in dashboard order.
pub const METRICS: &[MetricSpec] = &[
    MetricSpec {
        id: MetricId::from_static("time-saved"),
        label: StaticStr::new("Time saved"),
        help: StaticStr::new(
            "Time typing these words would take at your typing speed, minus the time you spent speaking.",
        ),
        unit: MetricUnit::Duration,
        query: MetricQuery::Summary {
            aggregate: MetricAggregate::TimeSaved,
        },
        emphasis: MetricEmphasis::Hero,
    },
    MetricSpec {
        id: MetricId::from_static("words"),
        label: StaticStr::new("Words"),
        help: StaticStr::new("Words delivered by completed takes."),
        unit: MetricUnit::Count,
        query: MetricQuery::Summary {
            aggregate: MetricAggregate::Words,
        },
        emphasis: MetricEmphasis::Primary,
    },
    MetricSpec {
        id: MetricId::from_static("transcriptions"),
        label: StaticStr::new("Transcriptions"),
        help: StaticStr::new("Completed takes."),
        unit: MetricUnit::Count,
        query: MetricQuery::Summary {
            aggregate: MetricAggregate::Transcriptions,
        },
        emphasis: MetricEmphasis::Primary,
    },
    MetricSpec {
        id: MetricId::from_static("speaking-wpm"),
        label: StaticStr::new("Speaking speed"),
        help: StaticStr::new("Words per minute while you were speaking."),
        unit: MetricUnit::Wpm,
        query: MetricQuery::Summary {
            aggregate: MetricAggregate::SpeakingWpm,
        },
        emphasis: MetricEmphasis::Primary,
    },
    MetricSpec {
        id: MetricId::from_static("activity"),
        label: StaticStr::new("Activity"),
        help: StaticStr::new("Words per day over the last 30 days."),
        unit: MetricUnit::Count,
        query: MetricQuery::Activity {
            days: ACTIVITY_DAYS,
        },
        emphasis: MetricEmphasis::Primary,
    },
    MetricSpec {
        id: MetricId::from_static("median-latency"),
        label: StaticStr::new("Median latency"),
        help: StaticStr::new(
            "Typical time from stopping to pasted text, over your last 100 takes.",
        ),
        unit: MetricUnit::Ms,
        query: MetricQuery::Summary {
            aggregate: MetricAggregate::MedianLatency,
        },
        emphasis: MetricEmphasis::Secondary,
    },
    MetricSpec {
        id: MetricId::from_static("streak"),
        label: StaticStr::new("Streak"),
        help: StaticStr::new("Consecutive days with at least one completed take."),
        unit: MetricUnit::Days,
        query: MetricQuery::Summary {
            aggregate: MetricAggregate::Streak,
        },
        emphasis: MetricEmphasis::Secondary,
    },
];

/// One line per IPC command call: name, request id, outcome (`ok` or the AppError code) and duration.
pub const COMMAND_DURATION: LogMetricSpec = LogMetricSpec {
    id: MetricId::from_static("command-duration"),
    help: StaticStr::new(
        "Time from a command call reaching the factory to its result, with the outcome.",
    ),
    unit: MetricUnit::Ms,
};

/// One accelerator measured by `auto` (02 §8.1): its load, warm-up and best steady-state run, in ms.
pub const ACCELERATOR_BENCHMARK: LogMetricSpec = LogMetricSpec {
    id: MetricId::from_static("accelerator-benchmark"),
    help: StaticStr::new(
        "Load, warm-up and best steady-state inference of one accelerator while choosing where speech runs.",
    ),
    unit: MetricUnit::Ms,
};

/// Every metric written only to the local log (02 §12).
pub const LOG_METRICS: &[LogMetricSpec] = &[COMMAND_DURATION, ACCELERATOR_BENCHMARK];

/// The metric `id`.
pub fn find(id: &MetricId) -> Option<&'static MetricSpec> {
    METRICS.iter().find(|spec| spec.id == *id)
}

/// The aggregates `metrics_summary` computes, in dashboard order.
pub fn summary_aggregates() -> Vec<MetricAggregate> {
    METRICS
        .iter()
        .filter_map(|spec| match spec.query {
            MetricQuery::Summary { aggregate } => Some(aggregate),
            MetricQuery::Activity { .. } => None,
        })
        .collect()
}

/**
 * SOURCE OF TRUTH KEYWORDS: metric setting inputs, SETTING_INPUTS, inputs_changed, typing speed setting, metrics refresh after settings write
 * WHAT:  SETTING_INPUTS: the settings the metric formulas read (time saved uses `metrics.typing_wpm`, 02 §7.4);
 *        `inputs_changed`: whether a settings write changed any of them.
 * WHY:   Metrics are computed from the rows at query time, never stored (05 decision log), so a new typing speed
 *        changes "Time saved" at once; the dashboard only needs to be told to read again (MetricsChanged). Which
 *        settings feed which formula is metric knowledge, so it is listed here and nothing matches on a key elsewhere.
 * WHERE: pipeline/settings_effects.rs (after `settings_set` / `settings_reset`); the metrics commands read the same
 *        settings when they compute.
 */
pub const SETTING_INPUTS: &[SettingKey] = &[keys::TYPING_WPM];

/// Whether any setting a metric formula reads differs between `before` and `after`.
pub fn inputs_changed(before: &SettingsSnapshot, after: &SettingsSnapshot) -> bool {
    SETTING_INPUTS
        .iter()
        .any(|key| before.get(key) != after.get(key))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::{
        registry::{settings, tests::is_registry_id},
        types::SettingValue,
    };

    #[test]
    fn only_a_formula_input_changes_the_metrics() {
        let before = settings::defaults();
        let faster = settings::resolve([(keys::TYPING_WPM, SettingValue::Int(80))]);
        let unrelated = settings::resolve([(keys::AUTO_PASTE, SettingValue::Bool(false))]);
        assert!(inputs_changed(&before, &faster));
        assert!(!inputs_changed(&before, &unrelated));
        for key in SETTING_INPUTS {
            assert!(settings::find(key).is_some(), "{key} is registered");
        }
    }

    #[test]
    fn ids_are_unique_and_every_aggregate_is_shown_once() {
        let mut ids = HashSet::new();
        for spec in METRICS {
            assert!(is_registry_id(spec.id.as_str()), "{}", spec.id);
            assert!(ids.insert(spec.id.as_str()), "duplicate metric {}", spec.id);
            assert!(!spec.label.trim().is_empty() && spec.help.ends_with('.'));
        }
        let aggregates = summary_aggregates();
        let unique: HashSet<MetricAggregate> = aggregates.iter().copied().collect();
        assert_eq!(
            unique.len(),
            aggregates.len(),
            "an aggregate is shown twice"
        );
        assert_eq!(unique, HashSet::from(MetricAggregate::ALL));
    }

    #[test]
    fn layout_has_one_hero_and_one_activity_series() {
        let heroes: Vec<&str> = METRICS
            .iter()
            .filter(|spec| spec.emphasis == MetricEmphasis::Hero)
            .map(|spec| spec.id.as_str())
            .collect();
        assert_eq!(heroes, ["time-saved"]);
        let series: Vec<&MetricSpec> = METRICS
            .iter()
            .filter(|spec| matches!(spec.query, MetricQuery::Activity { .. }))
            .collect();
        assert_eq!(series.len(), 1);
        assert_eq!(series[0].query, MetricQuery::Activity { days: 30 });
        assert!(
            (1..=crate::types::MetricsActivityInput::MAX_DAYS).contains(&ACTIVITY_DAYS),
            "the chart asks for a series metrics_activity accepts"
        );
        assert_eq!(
            find(&MetricId::from_static("streak")).map(|spec| spec.unit),
            Some(MetricUnit::Days)
        );
    }

    #[test]
    fn log_metrics_have_unique_ids_apart_from_dashboard_metrics() {
        let mut ids: HashSet<&str> = METRICS.iter().map(|spec| spec.id.as_str()).collect();
        for spec in LOG_METRICS {
            assert!(is_registry_id(spec.id.as_str()), "{}", spec.id);
            assert!(ids.insert(spec.id.as_str()), "duplicate metric {}", spec.id);
            assert!(spec.help.ends_with('.'), "{}", spec.id);
        }
        assert!(LOG_METRICS.contains(&COMMAND_DURATION));
        assert!(LOG_METRICS.contains(&ACCELERATOR_BENCHMARK));
    }
}
