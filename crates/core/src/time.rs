//! Time, as data.
//!
//! The core never asks the operating system what time it is. Every instant it works with
//! arrived inside a record it was given, and the passage of time arrives as a tick in the
//! observation stream ([ADR-0019](../../../docs/adr/0019-the-core-is-a-pure-function-of-an-observation-stream.md)).
//! That is what lets NFR-11's TC-45a run the whole suite with the clock taken away.
//!
//! One timestamp format occurs in the recordings — `2020-01-01T00:00:00.000Z`, all 176 of
//! them across every fixture — so the parser accepts exactly that and rejects everything
//! else rather than guessing. A record whose stamp will not parse still counts as activity;
//! it simply carries no instant (`docs/observation-sources.md` §2.3 notes that several
//! record types have no `timestamp` field at all).

use serde::{Deserialize, Serialize};

/// An instant, as milliseconds since the Unix epoch.
///
/// Ordering is chronological, but **it is not the ordering the state machine uses**: events
/// are ordered by their position in the file, because `api_error` records are appended
/// after the turn they belong to while carrying an earlier stamp
/// ([ADR-0006](../../../docs/adr/0006-order-events-by-append-position.md)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Timestamp(i64);

impl Timestamp {
    #[must_use]
    pub const fn from_millis(millis: i64) -> Self {
        Self(millis)
    }

    #[must_use]
    pub const fn as_millis(self) -> i64 {
        self.0
    }

    /// Milliseconds from `earlier` to `self`. Negative when the stream is out of
    /// chronological order, which is normal and is why callers must not treat this as a
    /// duration without checking.
    #[must_use]
    pub const fn millis_since(self, earlier: Self) -> i64 {
        self.0 - earlier.0
    }

    /// The calendar date and clock time this instant is, in UTC.
    ///
    /// Used for exactly one thing: naming a backup file so that a person can read it and a
    /// sort can order it (FR-44). UTC and not local time, because a local stamp goes
    /// backwards for an hour every autumn and the sort goes with it.
    #[must_use]
    pub const fn utc(self) -> Utc {
        // Floor division, so that an instant before the epoch lands in the day it belongs to
        // rather than the one after it. Rust's `/` truncates towards zero, which for a
        // negative millisecond count would be the following day.
        let days = self.0.div_euclid(86_400_000);
        let within = self.0.rem_euclid(86_400_000);
        let (year, month, day) = civil_from_days(days);
        Utc {
            year,
            month,
            day,
            hour: within / 3_600_000,
            minute: within / 60_000 % 60,
            second: within / 1000 % 60,
        }
    }

    /// Parses the one format the recordings use: `YYYY-MM-DDTHH:MM:SS.mmmZ`, UTC.
    ///
    /// Returns `None` for anything else — a different precision, an offset other than `Z`,
    /// a missing field. Guessing at a stamp is worse than having none, because the state
    /// machine's thresholds are in seconds.
    #[must_use]
    pub fn parse_iso8601(s: &str) -> Option<Self> {
        let b = s.as_bytes();
        if b.len() != 24 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' {
            return None;
        }
        if b[13] != b':' || b[16] != b':' || b[19] != b'.' || b[23] != b'Z' {
            return None;
        }
        let num = |from: usize, to: usize| -> Option<i64> {
            let mut acc: i64 = 0;
            for &c in &b[from..to] {
                if !c.is_ascii_digit() {
                    return None;
                }
                acc = acc * 10 + i64::from(c - b'0');
            }
            Some(acc)
        };
        let (year, month, day) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
        let (hour, minute, second) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
        let millis = num(20, 23)?;

        if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
            return None;
        }
        // 60 would be a leap second. Unix time has no representation for one, so the
        // arithmetic below would silently fold it into the next minute — a different instant
        // wearing the same text. The recordings have never contained one; if that changes,
        // it wants a decision rather than a rounding.
        if hour > 23 || minute > 59 || second > 59 {
            return None;
        }

        let days = days_from_civil(year, month, day);
        Some(Self(
            ((days * 24 + hour) * 60 + minute) * 60_000 + second * 1000 + millis,
        ))
    }
}

/// Length of a month, so that an impossible date is rejected rather than rolled over.
///
/// Without this, `2020-02-30` parses into the 1st of March: a **confident wrong instant**,
/// which is the one outcome [`Timestamp::parse_iso8601`] exists to avoid. The state model's
/// thresholds are durations measured against these values.
const fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// The full Gregorian rule, century exception included: 2000 is a leap year, 1900 was not.
const fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Days from the Unix epoch to a proleptic-Gregorian date, by Howard Hinnant's `days_from_civil`.
///
/// Exact for every date in the range this program can encounter, and small enough to check
/// against known values rather than trusted — which `tests/time.rs` does.
const fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400; // [0, 399]
    let month_index = if month > 2 { month - 3 } else { month + 9 }; // March = 0
    let day_of_year = (153 * month_index + 2) / 5 + day - 1; // [0, 365]
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// A broken-down UTC time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Utc {
    pub year: i64,
    pub month: i64,
    pub day: i64,
    pub hour: i64,
    pub minute: i64,
    pub second: i64,
}

/// The inverse of [`days_from_civil`], by the same author and the same paper.
///
/// Kept beside it so the two can be checked against each other: `tests/time.rs` round-trips
/// every day across a span that includes both leap-year exceptions, which is a stronger test
/// than either direction can give on its own.
const fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097; // [0, 146096]
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153; // [0, 11], March = 0
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    (if month <= 2 { year + 1 } else { year }, month, day)
}
