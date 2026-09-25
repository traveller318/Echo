/*!
 * SOURCE OF TRUTH KEYWORDS: DayWatch, day rollover, local midnight, new day, MetricsChanged at midnight, date change, dashboard today refresh
 * WHAT:  A task that notices when the local calendar day changes (midnight, a time-zone or clock change) and
 *        emits MetricsChanged, so an open dashboard reads "today", the streak and the activity chart again.
 * WHY:   The UI never polls (02 §4.4) and every other MetricsChanged follows a write, so without this a
 *        dashboard left open overnight would keep yesterday as "today" until the next take. The task sleeps
 *        until the next local midnight (plus a second, so the check lands inside the new day), capped at
 *        MAX_WAIT: Windows does not advance wait timeouts while the machine sleeps, so one long wait could fire
 *        hours late after a resume, and the cap bounds that staleness without a power listener (one tiny query
 *        an hour is the only idle cost). It announces only a real change of date, never on its first look, and
 *        a failed read is logged and retried after MAX_WAIT. Day boundaries come from services/calendar, the
 *        clock the metrics group days with.
 * WHERE: Spawned by app/bootstrap on Tauri's runtime (runs until the runtime stops at exit); `check` is driven
 *        directly by its tests.
 */

use std::{sync::Arc, time::Duration};

use crate::{
    ports::EventSink,
    services::{Db, calendar},
    types::{AppEvent, LocalDate, MetricsChanged, PortResult, UnixMs},
};

/// Longest single wait between two looks at the date.
pub const MAX_WAIT: Duration = Duration::from_secs(60 * 60);
/// How far past midnight the look after midnight lands.
const PAST_MIDNIGHT: Duration = Duration::from_secs(1);

pub struct DayWatch {
    db: Db,
    events: Arc<dyn EventSink<AppEvent>>,
    /// The local date seen by the previous look (None before the first).
    seen: Option<LocalDate>,
}

impl DayWatch {
    pub fn new(db: Db, events: Arc<dyn EventSink<AppEvent>>) -> Self {
        Self {
            db,
            events,
            seen: None,
        }
    }

    /// Looks at the date now and after every wait, forever.
    pub async fn run(mut self) {
        loop {
            let wait = self.check(UnixMs::now());
            tokio::time::sleep(wait).await;
        }
    }

    /// Looks at the local date at `now`, announces a change since the previous look, and returns how long to
    /// wait before the next one.
    pub fn check(&mut self, now: UnixMs) -> Duration {
        match self.look(now) {
            Ok(wait) => wait,
            Err(error) => {
                tracing::warn!(
                    detail = error.detail(),
                    "could not read the local date; the dashboard's day is checked again later"
                );
                MAX_WAIT
            }
        }
    }

    fn look(&mut self, now: UnixMs) -> PortResult<Duration> {
        let today = calendar::local_date(&self.db, now)?;
        if self.seen.is_some_and(|seen| seen != today) {
            tracing::info!(%today, "a new day began; the dashboard reads its metrics again");
            self.events.emit(MetricsChanged {}.into());
        }
        self.seen = Some(today);
        let next_day = calendar::day_start(&self.db, today.plus_days(1))?;
        let until =
            u64::try_from(next_day.as_millis().saturating_sub(now.as_millis())).unwrap_or(0);
        Ok((Duration::from_millis(until) + PAST_MIDNIGHT).min(MAX_WAIT))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::fakes::RecordingSink;

    fn watch() -> (DayWatch, Arc<RecordingSink<AppEvent>>) {
        let events = Arc::new(RecordingSink::default());
        let db = Db::open_in_memory().unwrap();
        (DayWatch::new(db, Arc::clone(&events) as _), events)
    }

    fn midnight(watch: &DayWatch, date: &str) -> i64 {
        calendar::day_start(&watch.db, date.parse().unwrap())
            .unwrap()
            .as_millis()
    }

    #[test]
    fn announces_each_new_day_once_and_wakes_just_after_midnight() {
        let (mut watch, events) = watch();
        let midnight = midnight(&watch, "2026-03-13");
        let at = |offset: i64| UnixMs::from_millis(midnight + offset);

        assert_eq!(
            watch.check(at(-12 * 3_600_000)),
            MAX_WAIT,
            "midnight is far off"
        );
        assert!(
            events.events().is_empty(),
            "the first look announces nothing"
        );
        assert_eq!(watch.check(at(-30_000)), Duration::from_secs(31));
        assert!(events.events().is_empty(), "still the same day");

        let after = watch.check(at(1_000));
        assert_eq!(after, MAX_WAIT);
        assert_eq!(events.events(), [AppEvent::from(MetricsChanged {})]);
        watch.check(at(2_000));
        assert_eq!(events.events().len(), 1, "one announcement per day");
    }

    #[test]
    fn a_clock_moved_back_a_day_is_a_change_too() {
        let (mut watch, events) = watch();
        let midnight = midnight(&watch, "2026-03-13");
        watch.check(UnixMs::from_millis(midnight + 3_600_000));
        watch.check(UnixMs::from_millis(midnight - 3_600_000));
        assert_eq!(events.events(), [AppEvent::from(MetricsChanged {})]);
    }
}
