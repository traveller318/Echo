/*!
 * SOURCE OF TRUTH KEYWORDS: metrics pipeline, dashboard metrics, metrics formulas, metrics summary, activity series, day watch, streak
 * WHAT:  The dashboard's business rules (02 §7.4): `formulas` (pure: time saved, speaking WPM, median, streak,
 *        zero-filled activity), `compute` (reads the services for a MetricsRange or an activity window and applies
 *        the formulas) and `day_watch` (MetricsChanged when the local day changes).
 * WHY:   Metrics are computed from `transcripts` at query time, never counted (05 decision log): the services
 *        return raw sums, the registry says which metrics exist, and every formula and window rule lives here once.
 * WHERE: ipc/commands/metrics.rs (metrics_summary, metrics_activity); app/bootstrap spawns the DayWatch.
 */

mod compute;
mod day_watch;
mod formulas;

pub use compute::{activity, summary, window_start};
pub use day_watch::DayWatch;
