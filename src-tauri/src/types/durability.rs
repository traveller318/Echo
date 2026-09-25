/*!
 * SOURCE OF TRUTH KEYWORDS: RetentionPolicy, RetentionReport, RecoveryReport, audio retention days, history retention days, retention cut-off, startup recovery result, never lose a take
 * WHAT:  The data of 02 §7.3 steps 4–5: RetentionPolicy (how long successful takes keep their audio and their
 *        rows, and the cut-off times they give), RetentionReport (what one sweep removed) and RecoveryReport (what
 *        startup recovery found among the takes a crash left unfinished).
 * WHY:   The settings store days as Ints with two different zero meanings (audio 0 = delete right after success,
 *        history 0 = keep forever); spelling them once here means the sweep, the "delete after success" path and
 *        their tests can never read a zero the wrong way. Cut-offs saturate, so a huge day count can never wrap
 *        into the future and select every row. Reports are counts only, never ids or text, so they are safe to log
 *        (02 §10) and cheap to turn into a toast or an event.
 * WHERE: RetentionPolicy is read from settings by registry::settings::retention_policy and applied by
 *        pipeline/retention.rs (sweeps, `release_after_success` for the session runner and retry); RecoveryReport
 *        is returned by pipeline/recovery.rs to app/bootstrap, which toasts it once the windows exist.
 */

use super::UnixMs;

/// One day in milliseconds.
const MS_PER_DAY: i64 = 86_400_000;

/// How long successful takes keep their audio and their rows (`storage.*_retention_days`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RetentionPolicy {
    /// Days a successful take keeps its WAV; 0 deletes it as soon as the take succeeds.
    pub audio_days: u32,
    /// Days a take's row is kept; 0 keeps rows forever.
    pub history_days: u32,
}

impl RetentionPolicy {
    /// The registry defaults (02 §3.3), for callers that have no settings snapshot.
    pub const DEFAULT: Self = Self {
        audio_days: 7,
        history_days: 30,
    };

    /// A take that just succeeded loses its audio at once (`storage.audio_retention_days` = 0).
    pub const fn deletes_audio_on_success(self) -> bool {
        self.audio_days == 0
    }

    /// Successful takes created before this time lose their audio (with 0 days: every successful take).
    pub fn audio_cutoff(self, now: UnixMs) -> UnixMs {
        days_before(now, self.audio_days)
    }

    /// Takes created before this time lose their row; None while history is kept forever.
    pub fn history_cutoff(self, now: UnixMs) -> Option<UnixMs> {
        (self.history_days > 0).then(|| days_before(now, self.history_days))
    }
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// `now` minus `days`, saturating at the earliest representable time.
fn days_before(now: UnixMs, days: u32) -> UnixMs {
    UnixMs::from_millis(
        now.as_millis()
            .saturating_sub(i64::from(days).saturating_mul(MS_PER_DAY)),
    )
}

/// What one retention sweep removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetentionReport {
    /// Successful takes whose WAV was deleted (the row stays).
    pub audio_removed: u32,
    /// Takes whose row (and any WAV left) was deleted.
    pub rows_removed: u32,
    /// WAV files in `recordings/` that no row pointed to.
    pub orphans_removed: u32,
}

impl RetentionReport {
    /// History rows changed (a row went, or a row lost its audio), so list queries should refetch.
    pub const fn changed_history(self) -> bool {
        self.audio_removed > 0 || self.rows_removed > 0
    }

    /// Rows went, so the dashboard aggregates over them changed (02 §7.4).
    pub const fn changed_metrics(self) -> bool {
        self.rows_removed > 0
    }
}

/// What startup recovery did with the takes a crash left `recording`/`transcribing`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RecoveryReport {
    /// Repaired and marked `recoverable`: History can retry them.
    pub recovered: u32,
    /// The journal held no audio at all (the crash came before the microphone delivered): stored `empty`.
    pub empty: u32,
    /// The journal was missing or not readable: stored `failed` without audio.
    pub unreadable: u32,
}

impl RecoveryReport {
    /// Rows changed, so History should refetch.
    pub const fn changed(self) -> bool {
        self.recovered > 0 || self.empty > 0 || self.unreadable > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: UnixMs = UnixMs::from_millis(100 * MS_PER_DAY);

    #[test]
    fn zero_audio_days_deletes_every_successful_take_and_zero_history_days_keeps_rows() {
        let policy = RetentionPolicy {
            audio_days: 0,
            history_days: 0,
        };
        assert!(policy.deletes_audio_on_success());
        assert_eq!(policy.audio_cutoff(NOW), NOW);
        assert_eq!(policy.history_cutoff(NOW), None);
    }

    #[test]
    fn cut_offs_count_whole_days_back_and_saturate() {
        let policy = RetentionPolicy {
            audio_days: 7,
            history_days: 30,
        };
        assert!(!policy.deletes_audio_on_success());
        assert_eq!(
            policy.audio_cutoff(NOW),
            UnixMs::from_millis(93 * MS_PER_DAY)
        );
        assert_eq!(
            policy.history_cutoff(NOW),
            Some(UnixMs::from_millis(70 * MS_PER_DAY))
        );
        let huge = RetentionPolicy {
            audio_days: u32::MAX,
            history_days: u32::MAX,
        };
        assert_eq!(
            huge.audio_cutoff(UnixMs::from_millis(i64::MIN + 1)),
            UnixMs::from_millis(i64::MIN)
        );
        assert!(huge.history_cutoff(NOW).is_some_and(|cutoff| cutoff < NOW));
    }

    #[test]
    fn reports_say_what_changed() {
        assert!(!RetentionReport::default().changed_history());
        let orphans_only = RetentionReport {
            orphans_removed: 3,
            ..RetentionReport::default()
        };
        assert!(!orphans_only.changed_history(), "orphans have no row");
        let audio = RetentionReport {
            audio_removed: 1,
            ..RetentionReport::default()
        };
        assert!(audio.changed_history() && !audio.changed_metrics());
        let rows = RetentionReport {
            rows_removed: 1,
            ..RetentionReport::default()
        };
        assert!(rows.changed_history() && rows.changed_metrics());

        assert!(!RecoveryReport::default().changed());
        assert!(
            RecoveryReport {
                unreadable: 1,
                ..RecoveryReport::default()
            }
            .changed()
        );
    }
}
