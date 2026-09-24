/*!
 * SOURCE OF TRUTH KEYWORDS: UnixMs, ByteCount, MonotonicMs, timestamps, byte sizes, monotonic clock, 64-bit integers over IPC, BigInt, number export
 * WHAT:  Newtypes for the two 64-bit quantities that cross IPC (wall-clock timestamps in unix ms, byte counts), and
 *        MonotonicMs, a reading of a monotonic clock that never crosses IPC.
 * WHY:   specta refuses to export i64/u64 because JS numbers lose precision above 2^53. Timestamps and byte counts
 *        never get near that, so these two newtypes tell specta they are a 32-bit integer, which exports as a
 *        plain `number` (an f64 override would export `number | null`, because JSON writes NaN as null), while
 *        any other stray 64-bit integer still fails the bindings export instead of truncating in the webview.
 *        Durations inside a take (debounce, elapsed, countdown, latency) must not jump when the wall clock is
 *        changed or synced mid-take, so they are measured on a monotonic clock; MonotonicMs keeps that reading a
 *        plain number so pure code (the session state machine) takes time as input instead of reading a clock.
 * WHERE: Transcript rows (types/transcript.rs), model progress (types/events.rs); built by services and pipeline.
 *        MonotonicMs: the session state machine (pipeline/session), stamped by the session actor.
 */

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;

/// Milliseconds since the Unix epoch (UTC).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type,
)]
#[serde(transparent)]
pub struct UnixMs(#[specta(type = u32)] i64);

impl UnixMs {
    pub const fn from_millis(millis: i64) -> Self {
        Self(millis)
    }

    pub const fn as_millis(self) -> i64 {
        self.0
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: UnixMs::now, current time, wall clock, epoch millis
     * WHAT:  The current wall-clock time in unix milliseconds.
     * WHY:   A clock set before 1970 is the only failure `SystemTime` reports; it clamps to 0 instead of panicking,
     *        and a far-future clock saturates, so callers never handle an error for reading the time.
     * WHERE: Row timestamps written by services and the session pipeline.
     */
    pub fn now() -> Self {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| {
                i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
            });
        Self(millis)
    }
}

/// A size or transfer amount in bytes.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type,
)]
#[serde(transparent)]
pub struct ByteCount(#[specta(type = u32)] u64);

impl ByteCount {
    pub const fn new(bytes: u64) -> Self {
        Self(bytes)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: MonotonicMs, monotonic time, since, saturating_since, plus_ms, take clock
 * WHAT:  Milliseconds on a monotonic clock whose zero the owner picks (the session actor: its own start).
 * WHY:   Only differences between two readings of the same clock mean anything, so there is no conversion to a
 *        date; subtraction saturates at 0 and addition at the maximum, so a reading taken out of order can never
 *        panic or wrap, it only yields a zero-length interval.
 * WHERE: Inputs and phases of the session state machine (types/session_machine.rs, pipeline/session).
 */
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MonotonicMs(u64);

impl MonotonicMs {
    pub const ZERO: Self = Self(0);

    pub const fn from_millis(millis: u64) -> Self {
        Self(millis)
    }

    pub const fn as_millis(self) -> u64 {
        self.0
    }

    /// Milliseconds from `earlier` to `self`; 0 when `earlier` is not earlier.
    pub const fn saturating_since(self, earlier: Self) -> u64 {
        self.0.saturating_sub(earlier.0)
    }

    /// The reading `millis` later.
    #[must_use]
    pub const fn plus_ms(self, millis: u64) -> Self {
        Self(self.0.saturating_add(millis))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monotonic_arithmetic_saturates() {
        let start = MonotonicMs::from_millis(1_000);
        assert_eq!(start.plus_ms(250).saturating_since(start), 250);
        assert_eq!(start.saturating_since(start.plus_ms(250)), 0);
        assert_eq!(
            MonotonicMs::from_millis(u64::MAX).plus_ms(1).as_millis(),
            u64::MAX
        );
        assert_eq!(MonotonicMs::default(), MonotonicMs::ZERO);
    }

    #[test]
    fn units_serialize_as_plain_json_numbers() {
        assert_eq!(
            serde_json::to_string(&UnixMs::from_millis(1_700_000_000_000)).unwrap(),
            "1700000000000"
        );
        assert_eq!(
            serde_json::to_string(&ByteCount::new(670_000_000)).unwrap(),
            "670000000"
        );
        assert_eq!(
            serde_json::from_str::<UnixMs>("42").unwrap(),
            UnixMs::from_millis(42)
        );
    }

    #[test]
    fn now_is_after_2025() {
        assert!(UnixMs::now().as_millis() > 1_735_689_600_000);
    }
}
