//! The Artifact: an Explanation with its Passage and the app's metadata.
//!
//! This is the shape `plainly explain --format json` prints and the shape a
//! Record is written from, so it stays one flat document — the model's four
//! fields sit beside the ones the app appends, and no field is nested behind a
//! wrapper the reader has to unwrap.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{Explanation, Level, Thinking};

/// The version of the Explanation data model, stamped on every Artifact.
///
/// v0 is `1`. It is deliberately not part of the Lookup Key: a change of shape
/// does not make an old answer wrong, so bumping it must not throw the cache
/// away (spec §9).
pub const ARTIFACT_VERSION: u32 = 1;

/// An instant, to the second, in UTC.
///
/// Seconds since the Unix epoch inside, RFC 3339 in every document: the wire
/// form is what a script reads and what SQLite orders by, and the conversion is
/// small enough that a date library would be the heavier choice.
///
/// The stored form writes a four-digit year, so this type carries the instants
/// from `0000-01-01T00:00:00Z` to `9999-12-31T23:59:59Z` and no others. Both ways
/// in — [`Timestamp::from_unix_seconds`] and [`Timestamp::from_rfc3339`] — refuse
/// anything outside that, which is what makes the round trip total: a `Timestamp`
/// can always be written down and read back. Every instant a clock could
/// plausibly read is well inside the range.
///
/// There is no `now()` here: the explain path takes the instant as a value, so
/// reading the system clock belongs to the caller that can do something about a
/// clock outside the range (the CLI), rather than being settled by a silent
/// default in the domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(i64);

/// `0000-01-01T00:00:00Z`: the earliest instant a four-digit year can name.
const EARLIEST_UNIX_SECONDS: i64 = -62_167_219_200;

/// `9999-12-31T23:59:59Z`: the latest.
const LATEST_UNIX_SECONDS: i64 = 253_402_300_799;

impl Timestamp {
    /// A given instant, in seconds since the Unix epoch, or `None` outside the
    /// range this type can store — which [`Timestamp::from_rfc3339`] would then
    /// refuse to read back.
    pub fn from_unix_seconds(seconds: i64) -> Option<Self> {
        (EARLIEST_UNIX_SECONDS..=LATEST_UNIX_SECONDS)
            .contains(&seconds)
            .then_some(Self(seconds))
    }

    /// Seconds since the Unix epoch.
    pub fn unix_seconds(self) -> i64 {
        self.0
    }

    /// The instant as `2026-10-05T12:34:56Z` — always exactly this shape, since
    /// [`Timestamp::from_unix_seconds`] is what keeps the year four digits wide.
    pub fn to_rfc3339(self) -> String {
        let days = self.0.div_euclid(86_400);
        let second_of_day = self.0.rem_euclid(86_400);
        let (year, month, day) = civil_from_days(days);
        let (hour, minute, second) = (
            second_of_day / 3_600,
            (second_of_day / 60) % 60,
            second_of_day % 60,
        );
        format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
    }

    /// Read the form [`Timestamp::to_rfc3339`] writes: `YYYY-MM-DDTHH:MM:SSZ`,
    /// ASCII digits, fixed field widths, always UTC.
    ///
    /// Anything else — a local offset, a fractional second, a sign, a field of
    /// the wrong width, a date that does not exist — is `None` rather than a
    /// guess, so a stored timestamp is never quietly shifted. The form is fixed
    /// rather than parsed loosely precisely because the strict version cannot
    /// turn `2025-10-09T-1:00:00Z` into the day before.
    pub fn from_rfc3339(text: &str) -> Option<Self> {
        let (date, time) = text.strip_suffix('Z')?.split_once('T')?;
        let (date, time) = (date.as_bytes(), time.as_bytes());
        if date.len() != 10 || date[4] != b'-' || date[7] != b'-' {
            return None;
        }
        if time.len() != 8 || time[2] != b':' || time[5] != b':' {
            return None;
        }

        let year = i64::from(digits(&date[0..4])?);
        let month = digits(&date[5..7])?;
        let day = digits(&date[8..10])?;
        let hour = digits(&time[0..2])?;
        let minute = digits(&time[3..5])?;
        let second = digits(&time[6..8])?;

        if !(1..=12).contains(&month) || day == 0 || hour > 23 || minute > 59 || second > 59 {
            return None;
        }
        // A day the calendar does not have — 2026-02-30, 2025-04-31 — computes
        // to some other date. Requiring the round trip to land back on the fields
        // we were given is what rejects it.
        let days = days_from_civil(year, month, day);
        if civil_from_days(days) != (year, month, day) {
            return None;
        }

        let seconds =
            days * 86_400 + i64::from(hour) * 3_600 + i64::from(minute) * 60 + i64::from(second);
        // The range is applied here too rather than assumed: both ways in go
        // through the same check, so neither can build a `Timestamp` the other
        // would refuse. If the range ever narrows, the boundary tests are the
        // alarm — a stored timestamp outside it comes back as unreadable rather
        // than as a panic on somebody's history.
        Self::from_unix_seconds(seconds)
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_rfc3339())
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Timestamp::from_rfc3339(&text).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "{text:?} is not an RFC 3339 UTC timestamp such as 2026-10-05T12:34:56Z"
            ))
        })
    }
}

/// One Passage, the Explanation it produced, and who produced it.
///
/// Everything the model is not trusted with is here: the Passage verbatim, the
/// Level and Native Language the Explanation was pitched at, the provider profile
/// that answered, and the two versions — the shape's and the prompt's — that say
/// what this document is and what produced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    /// The Passage, verbatim. Never a copy the model sent back.
    pub passage: String,
    pub level: Level,
    pub source_language: String,
    pub native_language: String,
    pub provider: String,
    pub model: String,
    pub thinking: Thinking,
    /// The data model's version, not the prompt's: [`ARTIFACT_VERSION`].
    pub artifact_version: u32,
    /// The content hash of the prompt that produced this Explanation, which is
    /// what makes two runs comparable (tickets/07).
    pub prompt_version: String,
    /// That prompt's human-readable name, for people rather than for identity.
    pub prompt_label: String,
    /// When this Explanation was first created. Equal to `generated_at` on a
    /// freshly generated Artifact; they part ways once a Record is reused or
    /// regenerated (tickets/08).
    pub created_at: Timestamp,
    /// When this Explanation was generated.
    pub generated_at: Timestamp,
    /// The model's four fields, flattened into the document beside the rest.
    #[serde(flatten)]
    pub explanation: Explanation,
}

/// A run of ASCII digits as a number. Signs, padding and empty fields are not
/// numbers here: the stored form is fixed-width, so anything else is not it.
fn digits(bytes: &[u8]) -> Option<u32> {
    if !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

/// Days since 1970-01-01 to a calendar date. Howard Hinnant's `civil_from_days`.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11], March-based
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// A calendar date to days since 1970-01-01. Hinnant's `days_from_civil`.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = (if year >= 0 { year } else { year - 399 }) / 400;
    let yoe = (year - era * 400) as u64; // [0, 399]
    let mp = (if month > 2 { month - 3 } else { month + 9 }) as u64; // [0, 11]
    let doy = (153 * mp + 2) / 5 + u64::from(day) - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe as i64 - 719_468
}
