//! Supporting cases for the timestamp parser (NFR-11).
//!
//! These exist because a mutation audit found the gap: shifting the epoch by a day and
//! replacing the month-length calculation with `month * 30` both left the whole suite green.
//! Every other case cares about the *order* of instants, and a uniform shift preserves
//! order — so nothing pinned an absolute value until this file did.
//!
//! That matters more than it sounds. Three rules of the state model are durations measured
//! in seconds against these instants, and the calendar arithmetic is hand-rolled to avoid a
//! dependency ([ADR-0019](../../../docs/adr/0019-the-core-is-a-pure-function-of-an-observation-stream.md)) —
//! which is a reasonable trade only while something checks the arithmetic.
//!
//! The anchors below are Gregorian-calendar facts rather than recorded data, so rule 1 of
//! `docs/test-plan.md` §1 does not apply to them the way it applies to a transcript: there
//! is nothing to record, and a hand-written date encodes no assumption the parser could
//! share. The corpus is still used where it can be — the first case checks the recordings
//! against *each other*.

#[path = "common/fixtures.rs"]
mod fixtures;

use mcv_core::time::Timestamp;

/// The recordings carry the same instant in two encodings, and they must agree.
///
/// `prompt-observed.jsonl` opens at `2020-01-01T00:00:00.000Z` — the anchor the whole corpus
/// was shifted onto — and `session-registry.json`'s first `startedAt` is that same instant
/// as epoch milliseconds. Neither was written by hand for this test.
#[test]
fn the_recordings_agree_with_each_other_about_one_instant() {
    let stream = fixtures::read("prompt-observed.jsonl");
    let first: serde_json::Value =
        serde_json::from_str(stream.lines().next().expect("the recording is not empty"))
            .expect("the line is JSON");
    let stamp = first["t"].as_str().expect("every line carries `t`");

    let registry: serde_json::Value =
        serde_json::from_str(&fixtures::read("session-registry.json")).expect("registry is JSON");
    let started_at = registry["entries"][0]["startedAt"]
        .as_i64()
        .expect("the first entry carries startedAt");

    assert_eq!(
        Timestamp::parse_iso8601(stamp).map(Timestamp::as_millis),
        Some(started_at),
        "{stamp} and startedAt {started_at} are the same instant in the corpus",
    );
}

/// Absolute values, against the calendar.
///
/// The leap days are the interesting ones: 2000 is a leap year *because* it is divisible by
/// 400, and 2021-03-01 is the day after a February in a common year. An approximation of
/// month lengths gets all three wrong.
#[test]
fn known_instants_parse_to_their_epoch_milliseconds() {
    let anchors = [
        ("1970-01-01T00:00:00.000Z", 0_i64),
        ("2000-02-29T00:00:00.000Z", 951_782_400_000),
        ("2020-01-01T00:00:00.000Z", 1_577_836_800_000),
        ("2020-02-29T00:00:00.000Z", 1_582_934_400_000),
        ("2021-03-01T00:00:00.000Z", 1_614_556_800_000),
        ("2020-03-14T23:04:59.771Z", 1_584_227_099_771),
        ("2020-12-31T23:59:59.999Z", 1_609_459_199_999),
        // The leap day that exists because of the 400 rule, and the one that exists because
        // of the 4 rule. Both must still parse now that impossible dates are refused.
        ("2024-02-29T12:00:00.000Z", 1_709_208_000_000),
    ];
    for (text, expected) in anchors {
        assert_eq!(
            Timestamp::parse_iso8601(text).map(Timestamp::as_millis),
            Some(expected),
            "{text} is {expected} ms after the epoch",
        );
    }
}

/// Every stamp in every recording parses, and lands in a plausible range.
///
/// The corpus was shifted onto 2020 by a single constant, so anything outside that decade
/// means the parser is reading fields out of position rather than failing outright — which
/// is the failure mode a format check alone would miss.
#[test]
fn every_recorded_stamp_parses_into_the_corpus_window() {
    let window = 1_577_836_800_000..1_640_995_200_000; // 2020-01-01 .. 2022-01-01
    let mut seen = 0;

    for name in [
        "turn-normal.jsonl",
        "compaction.jsonl",
        "api-error-late-append.jsonl",
    ] {
        for (_, line) in fixtures::lines(name) {
            let value: serde_json::Value = serde_json::from_str(&line).expect("fixture is JSON");
            let Some(stamp) = value["timestamp"].as_str() else {
                continue;
            };
            let parsed = Timestamp::parse_iso8601(stamp)
                .unwrap_or_else(|| panic!("{name}: {stamp} does not parse"));
            assert!(
                window.contains(&parsed.as_millis()),
                "{name}: {stamp} parsed to {}, outside the window the corpus was shifted into",
                parsed.as_millis(),
            );
            seen += 1;
        }
    }
    assert!(seen > 0, "the recordings carry timestamps");
}

/// A stamp in any other shape is rejected rather than guessed at.
///
/// Guessing is worse than having none: the state model's thresholds are in seconds, and a
/// silently wrong instant produces a confident wrong status rather than an honest `unknown`.
#[test]
fn anything_but_the_recorded_format_is_rejected() {
    let rejected = [
        "",
        "2020-01-01",
        "2020-01-01T00:00:00Z",          // no milliseconds
        "2020-01-01T00:00:00.000+09:00", // an offset rather than UTC
        "2020-01-01 00:00:00.000Z",      // a space instead of T
        "2020-13-01T00:00:00.000Z",      // month 13
        "2020-01-32T00:00:00.000Z",      // day 32
        "2020-01-01T24:00:00.000Z",      // hour 24
        "20x0-01-01T00:00:00.000Z",      // not a digit
        // Dates that are well formed and still impossible. Left unchecked these roll over
        // into the following month, which is a confident wrong instant rather than a
        // refusal — the failure this parser is strict in order to avoid.
        "2020-02-30T00:00:00.000Z", // February never has 30 days
        "2021-02-29T00:00:00.000Z", // 2021 is not a leap year
        "1900-02-29T00:00:00.000Z", // nor was 1900: divisible by 100, not by 400
        "2020-04-31T00:00:00.000Z", // April has 30
        "2020-01-00T00:00:00.000Z", // there is no zeroth day
        // A leap second. Unix time cannot represent one, so accepting it would quietly mean
        // the first second of the next minute.
        "2016-12-31T23:59:60.000Z",
    ];
    for text in rejected {
        assert_eq!(
            Timestamp::parse_iso8601(text),
            None,
            "{text:?} is not the format the recordings use and must not be accepted",
        );
    }
}

/// Every day for two centuries, out and back again.
///
/// [`Timestamp::utc`] and the parser are inverses built from two halves of the same paper,
/// and each is only as trustworthy as the other. Checking them against each other across a
/// span that contains both leap-year exceptions — 2000 is one, 1900 and 2100 are not — is a
/// stronger statement than either could make alone, and it is the reason a backup file can
/// be named after the moment it was taken (FR-44).
#[test]
fn the_calendar_round_trips_for_every_day_of_two_centuries() {
    // 1900-01-01 to 2100-12-31, inclusive.
    let first = -25_567_i64;
    let last = 47_847_i64;

    for day in first..=last {
        // A time of day that is nobody's boundary, so an off-by-one in either direction
        // shows up as a different date rather than as the same one.
        let millis = day * 86_400_000 + (13 * 3600 + 45 * 60 + 7) * 1000;
        let t = Timestamp::from_millis(millis).utc();

        assert!(
            (1..=12).contains(&t.month),
            "day {day} gave month {}",
            t.month
        );
        assert!((1..=31).contains(&t.day), "day {day} gave day {}", t.day);
        assert_eq!((t.hour, t.minute, t.second), (13, 45, 7));

        let text = format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.000Z",
            t.year, t.month, t.day, t.hour, t.minute, t.second
        );
        assert_eq!(
            Timestamp::parse_iso8601(&text).map(Timestamp::as_millis),
            Some(millis),
            "{text} did not parse back to the instant it was written from",
        );
    }
}

/// The known dates, checked against the calendar rather than against the other direction.
#[test]
fn the_calendar_agrees_with_dates_anyone_can_check() {
    /// A date and a clock time, as the six numbers `Utc` holds.
    type Civil = (i64, i64, i64, i64, i64, i64);

    let known: [(&str, Civil); 6] = [
        ("1970-01-01T00:00:00.000Z", (1970, 1, 1, 0, 0, 0)),
        ("2000-02-29T12:00:00.000Z", (2000, 2, 29, 12, 0, 0)), // a leap day, century rule
        ("2024-12-31T23:59:59.000Z", (2024, 12, 31, 23, 59, 59)),
        ("2026-08-27T09:41:05.000Z", (2026, 8, 27, 9, 41, 5)),
        ("2100-03-01T00:00:00.000Z", (2100, 3, 1, 0, 0, 0)), // 2100 is not a leap year
        ("1969-12-31T23:59:59.000Z", (1969, 12, 31, 23, 59, 59)), // before the epoch
    ];

    for (text, expected) in known {
        let t = Timestamp::parse_iso8601(text)
            .expect("the fixture parses")
            .utc();
        assert_eq!(
            (t.year, t.month, t.day, t.hour, t.minute, t.second),
            expected,
            "{text}",
        );
    }
}
