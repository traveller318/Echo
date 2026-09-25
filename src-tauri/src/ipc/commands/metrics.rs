/*!
 * SOURCE OF TRUTH KEYWORDS: metrics commands, metrics_summary, metrics_activity, dashboard reads, time saved, activity chart, MetricsSummary
 * WHAT:  The metrics command group (02 §4.3): `metrics_summary` (every registry summary metric over a
 *        MetricsRange) and `metrics_activity` (words and completed takes per local day for the last `days` days,
 *        today included, oldest first, every day present).
 * WHY:   The dashboard reads these once and refreshes on MetricsChanged, never by polling (02 §4.4). Handlers are
 *        thin: the factory already enforced the input schema (a known range, 1..=MAX_DAYS days), pipeline/metrics
 *        owns the formulas and windows, the services own the SQL. The reads (several aggregates, one scanning
 *        every kept day for the streak) run on the blocking pool so a long history never stalls the async
 *        runtime. The typing speed is read from the live settings at every call, so a changed speed is in the
 *        next read that MetricsChanged triggers.
 * WHERE: Registered through `ipc::commands::catalog`; called from the UI as `commands.metricsSummary(…)` and
 *        `commands.metricsActivity(…)` (src/hooks/use-metrics.ts, the Dashboard route).
 */

use std::num::NonZeroU32;

use crate::{
    ipc::{CommandCtx, factory::echo_command},
    pipeline::{blocking::run_blocking, metrics},
    types::{
        ActivityDay, AppError, MetricsActivityInput, MetricsSummary, MetricsSummaryInput,
        PortError, UnixMs,
    },
};

echo_command! {
    /// Every dashboard summary metric over `range`, in dashboard order; a value is null when there is nothing
    /// to compute it from yet.
    name: metrics_summary,
    input: MetricsSummaryInput,
    output: MetricsSummary,
    permission: None,
    reentrancy: Shared,
    handler: summary,
}

echo_command! {
    /// Words and completed takes per local day for the last `days` days, today included, oldest first.
    name: metrics_activity,
    input: MetricsActivityInput,
    output: Vec<ActivityDay>,
    permission: None,
    reentrancy: Shared,
    handler: activity,
}

/// Computes the summary metrics.
pub async fn summary(
    ctx: &CommandCtx,
    input: MetricsSummaryInput,
) -> Result<MetricsSummary, PortError> {
    let db = ctx.db().clone();
    let settings = ctx.settings();
    run_blocking("reading the dashboard metrics", move || {
        metrics::summary(&db, &settings, input.range, UnixMs::now())
    })
    .await
}

/// Computes the activity series.
pub async fn activity(
    ctx: &CommandCtx,
    input: MetricsActivityInput,
) -> Result<Vec<ActivityDay>, PortError> {
    let days = NonZeroU32::new(input.days)
        .ok_or_else(|| PortError::new(AppError::validation("days", "Ask for at least one day.")))?;
    let db = ctx.db().clone();
    run_blocking("reading the dashboard activity", move || {
        metrics::activity(&db, days, UnixMs::now())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ipc::{factory, testing},
        ports::fakes::FakePrivacyConsent,
        registry::{self, settings::keys},
        services,
        types::{
            AppPaths, CommandSpec, EngineId, MetricAggregate, MetricsRange, NewTranscript,
            Reentrancy, SettingValue, TranscriptChange, TranscriptId, TranscriptStatus,
        },
    };

    const SUMMARY: CommandSpec = CommandSpec {
        name: "metrics_summary",
        permission: None,
        reentrancy: Reentrancy::Shared,
    };
    const ACTIVITY: CommandSpec = CommandSpec {
        name: "metrics_activity",
        permission: None,
        reentrancy: Reentrancy::Shared,
    };

    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(future)
    }

    /// A completed take of `words` words and `seconds` s (all speech), made just now.
    fn done(ctx: &CommandCtx, words: u32, seconds: u32) {
        let id = TranscriptId::generate();
        services::transcripts::insert::insert(
            ctx.db(),
            &NewTranscript {
                id,
                created_at: UnixMs::now(),
                status: TranscriptStatus::Recording,
                audio_path: Some(AppPaths::recording_name(id)),
                engine_id: Some(EngineId::from_static("parakeet-tdt-0.6b-v3")),
                app_name: None,
            },
        )
        .unwrap();
        services::transcripts::update::update(
            ctx.db(),
            id,
            &[
                TranscriptChange::Status(TranscriptStatus::Done),
                TranscriptChange::WordCount(words),
                TranscriptChange::DurationMs(seconds * 1_000),
                TranscriptChange::SpeechMs(seconds * 1_000),
                TranscriptChange::LatencyMs(180),
            ],
        )
        .unwrap();
    }

    #[test]
    fn summary_reads_through_the_factory_with_the_live_typing_speed() {
        let harness = testing::harness(
            registry::settings::resolve([(keys::TYPING_WPM, SettingValue::Int(60))]),
            FakePrivacyConsent::granted(),
        );
        let ctx = &harness.ctx;
        let empty = block_on(factory::run(
            ctx,
            &SUMMARY,
            MetricsSummaryInput {
                range: MetricsRange::Today,
            },
            summary,
        ))
        .unwrap();
        assert!(empty.values.iter().all(|value| value.value.is_none()));

        done(ctx, 120, 30);
        let today = block_on(factory::run(
            ctx,
            &SUMMARY,
            MetricsSummaryInput {
                range: MetricsRange::Today,
            },
            summary,
        ))
        .unwrap();
        // 120 words at 60 wpm = 120 s of typing, minus 30 s spoken.
        assert_eq!(today.value_of(MetricAggregate::TimeSaved), Some(90_000.0));
        assert_eq!(today.value_of(MetricAggregate::SpeakingWpm), Some(240.0));
        assert_eq!(today.value_of(MetricAggregate::Streak), Some(1.0));
        let order: Vec<MetricAggregate> =
            today.values.iter().map(|value| value.aggregate).collect();
        assert_eq!(order, registry::metrics::summary_aggregates());
    }

    #[test]
    fn activity_is_validated_and_ends_today() {
        let harness = testing::harness(
            registry::settings::defaults(),
            FakePrivacyConsent::granted(),
        );
        let ctx = &harness.ctx;
        for days in [0, MetricsActivityInput::MAX_DAYS + 1] {
            let error = block_on(factory::run(
                ctx,
                &ACTIVITY,
                MetricsActivityInput { days },
                activity,
            ))
            .unwrap_err();
            assert!(
                matches!(error, AppError::Validation { ref field, .. } if field == "days"),
                "{days}: {error:?}"
            );
        }
        done(ctx, 42, 10);
        let series = block_on(factory::run(
            ctx,
            &ACTIVITY,
            MetricsActivityInput {
                days: registry::metrics::ACTIVITY_DAYS,
            },
            activity,
        ))
        .unwrap();
        assert_eq!(series.len(), 30);
        let today = services::calendar::local_date(ctx.db(), UnixMs::now()).unwrap();
        assert_eq!(series.last().map(|day| day.date), Some(today));
        assert_eq!(series.iter().map(|day| day.words).sum::<u32>(), 42);
    }
}
