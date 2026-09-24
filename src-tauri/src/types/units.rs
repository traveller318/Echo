/*!
 * SOURCE OF TRUTH KEYWORDS: UnixMs, ByteCount, timestamps, byte sizes, 64-bit integers over IPC, BigInt, number export
 * WHAT:  Newtypes for the two 64-bit quantities that cross IPC: wall-clock timestamps (unix ms) and byte counts.
 * WHY:   specta refuses to export i64/u64 because JS numbers lose precision above 2^53. Timestamps and byte counts
 *        never get near that, so these two newtypes tell specta they are a 32-bit integer, which exports as a
 *        plain `number` (an f64 override would export `number | null`, because JSON writes NaN as null), while
 *        any other stray 64-bit integer still fails the bindings export instead of truncating in the webview.
 * WHERE: Transcript rows (types/transcript.rs), model progress (types/events.rs); built by services and pipeline.
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

#[cfg(test)]
mod tests {
    use super::*;

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
