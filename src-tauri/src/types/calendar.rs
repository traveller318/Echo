/*!
 * SOURCE OF TRUTH KEYWORDS: LocalDate, calendar date, local day, YYYY-MM-DD, day arithmetic, days since epoch, civil date, streak days, activity dates
 * WHAT:  LocalDate: one calendar day in the user's local time zone with no time of day, written `YYYY-MM-DD`;
 *        parses and prints that form, adds and subtracts whole days and counts the days between two dates.
 * WHY:   Dashboard metrics are about calendar days as the user lives them (the 30-day activity chart, the streak,
 *        "today"), and a take near midnight must land on the day the user saw on the clock. Which instant starts
 *        which local day is the time zone's business (services/calendar asks SQLite, the same clock that groups
 *        takes by day); what is left is plain calendar arithmetic, done here on a day number (days since
 *        1970-01-01, proleptic Gregorian, H. Hinnant's civil-date algorithms) so it is exact, total and needs no
 *        date crate. Parsing is strict (four-digit year, two-digit month and day, a real date), so a malformed
 *        value from storage fails loudly instead of shifting a day. Over IPC it is the `YYYY-MM-DD` string
 *        (field-level `#[specta(type = String)]` at each use), which the UI reads as a local date.
 * WHERE: types/metrics.rs (ActivityDay); services/calendar.rs and services/transcripts/aggregate.rs (decode and
 *        bind); pipeline/metrics (streak, day windows, activity series, the day watch).
 */

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// A calendar day in the user's local time zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LocalDate {
    /// Days since 1970-01-01 (negative before it).
    days: i32,
}

/// Text that is not a real `YYYY-MM-DD` date.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("not a valid YYYY-MM-DD date")]
pub struct InvalidDate;

/// Days from 0000-03-01 to 1970-01-01 in the civil-date algorithms.
const EPOCH_SHIFT: i64 = 719_468;
/// Days in one 400-year Gregorian cycle.
const DAYS_PER_ERA: i64 = 146_097;

impl LocalDate {
    /// The date `days` after 1970-01-01.
    pub const fn from_days_since_epoch(days: i32) -> Self {
        Self { days }
    }

    /// Days since 1970-01-01.
    pub const fn days_since_epoch(self) -> i32 {
        self.days
    }

    /// The date, when `year`-`month`-`day` exists (year 0–9999).
    pub fn from_ymd(year: i32, month: u32, day: u32) -> Option<Self> {
        if !(0..=9999).contains(&year)
            || !(1..=12).contains(&month)
            || day == 0
            || day > days_in_month(year, month)
        {
            return None;
        }
        let year = i64::from(year) - i64::from(month <= 2);
        let era = year.div_euclid(400);
        let year_of_era = year - era * 400;
        let shifted_month = i64::from(if month > 2 { month - 3 } else { month + 9 });
        let day_of_year = (153 * shifted_month + 2) / 5 + i64::from(day) - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        i32::try_from(era * DAYS_PER_ERA + day_of_era - EPOCH_SHIFT)
            .ok()
            .map(Self::from_days_since_epoch)
    }

    /// Year, month (1–12) and day (1–31).
    pub fn ymd(self) -> (i32, u32, u32) {
        let days = i64::from(self.days) + EPOCH_SHIFT;
        let era = days.div_euclid(DAYS_PER_ERA);
        let day_of_era = days - era * DAYS_PER_ERA;
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let shifted_month = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
        let month = if shifted_month < 10 {
            shifted_month + 3
        } else {
            shifted_month - 9
        };
        let year = year_of_era + era * 400 + i64::from(month <= 2);
        // Every i32 day number maps to a year well inside i32, a month 1–12 and a day 1–31.
        (
            i32::try_from(year).unwrap_or(i32::MAX),
            u32::try_from(month).unwrap_or(1),
            u32::try_from(day).unwrap_or(1),
        )
    }

    /// The date `days` later (earlier when negative); saturates at the ends of the day range.
    #[must_use]
    pub const fn plus_days(self, days: i32) -> Self {
        Self::from_days_since_epoch(self.days.saturating_add(days))
    }

    /// The day before.
    #[must_use]
    pub const fn previous(self) -> Self {
        self.plus_days(-1)
    }

    /// Whole days from `earlier` to `self` (negative when `earlier` is later).
    pub const fn days_since(self, earlier: Self) -> i64 {
        self.days as i64 - earlier.days as i64
    }
}

fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

impl fmt::Display for LocalDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (year, month, day) = self.ymd();
        write!(f, "{year:04}-{month:02}-{day:02}")
    }
}

impl FromStr for LocalDate {
    type Err = InvalidDate;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let bytes = text.as_bytes();
        let well_formed = bytes.len() == 10
            && bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes
                .iter()
                .enumerate()
                .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit());
        if !well_formed {
            return Err(InvalidDate);
        }
        let number =
            |range: std::ops::Range<usize>| text[range].parse::<u32>().map_err(|_| InvalidDate);
        let year = i32::try_from(number(0..4)?).map_err(|_| InvalidDate)?;
        Self::from_ymd(year, number(5..7)?, number(8..10)?).ok_or(InvalidDate)
    }
}

impl Serialize for LocalDate {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for LocalDate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(text: &str) -> LocalDate {
        text.parse().unwrap()
    }

    #[test]
    fn epoch_and_known_dates_map_to_their_day_numbers() {
        assert_eq!(date("1970-01-01").days_since_epoch(), 0);
        assert_eq!(date("1969-12-31").days_since_epoch(), -1);
        assert_eq!(date("2000-03-01").days_since_epoch(), 11_017);
        // 2026-01-10T00:00:00Z is 1_768_003_200 s after the epoch.
        assert_eq!(
            date("2026-01-10").days_since_epoch(),
            1_768_003_200 / 86_400
        );
    }

    #[test]
    fn every_day_round_trips_through_its_text_and_parts() {
        let start = date("1899-12-25").days_since_epoch();
        for days in start..start + 200 * 366 {
            let day = LocalDate::from_days_since_epoch(days);
            let (year, month, dom) = day.ymd();
            assert_eq!(LocalDate::from_ymd(year, month, dom), Some(day));
            assert_eq!(day.to_string().parse::<LocalDate>(), Ok(day));
            assert_eq!(day.plus_days(1).previous(), day);
        }
    }

    #[test]
    fn arithmetic_crosses_months_years_and_leap_days() {
        assert_eq!(date("2024-02-28").plus_days(1), date("2024-02-29"));
        assert_eq!(date("2024-02-29").plus_days(1), date("2024-03-01"));
        assert_eq!(date("2023-02-28").plus_days(1), date("2023-03-01"));
        assert_eq!(date("2026-12-31").plus_days(1), date("2027-01-01"));
        assert_eq!(date("2026-01-01").plus_days(-29), date("2025-12-03"));
        assert_eq!(date("2026-03-01").days_since(date("2026-02-01")), 28);
        assert_eq!(date("2026-02-01").days_since(date("2026-03-01")), -28);
        assert_eq!(
            LocalDate::from_days_since_epoch(i32::MAX).plus_days(1),
            LocalDate::from_days_since_epoch(i32::MAX)
        );
    }

    #[test]
    fn only_real_well_formed_dates_parse() {
        for bad in [
            "",
            "2026-1-10",
            "2026-01-1",
            "2026/01/10",
            "20260110",
            "2026-13-01",
            "2026-00-10",
            "2026-02-29",
            "2100-02-29",
            "2026-04-31",
            "2026-01-00",
            "+026-01-10",
            "2026-01-10 ",
            "２０２６-01-10",
        ] {
            assert_eq!(bad.parse::<LocalDate>(), Err(InvalidDate), "{bad:?}");
        }
        assert!("2000-02-29".parse::<LocalDate>().is_ok());
        assert_eq!(LocalDate::from_ymd(10_000, 1, 1), None);
        assert_eq!(LocalDate::from_ymd(-1, 1, 1), None);
    }

    #[test]
    fn serializes_as_its_text() {
        let day = date("2026-09-25");
        assert_eq!(serde_json::to_value(day).unwrap(), "2026-09-25");
        assert_eq!(
            serde_json::from_value::<LocalDate>("2026-09-25".into()).unwrap(),
            day
        );
        assert!(serde_json::from_value::<LocalDate>("2026-02-30".into()).is_err());
        assert!(day > date("2026-09-24"));
    }
}
