//! The Artifact: the JSON document `--format json` prints, and the instants it
//! carries.

mod support;

use std::collections::BTreeSet;

use plainly_core::{ARTIFACT_VERSION, Artifact, Explanation, Timestamp};
use serde_json::{Value, json};

fn example() -> Artifact {
    support::artifact(
        support::PASSAGE,
        Explanation::parse(support::ANSWER).expect("the fixture holds the contract"),
    )
}

#[test]
fn the_epoch_is_the_epoch() {
    assert_eq!(
        Timestamp::from_unix_seconds(0)
            .expect("in range")
            .to_rfc3339(),
        "1970-01-01T00:00:00Z"
    );
}

#[test]
fn a_known_instant_reads_back_as_utc() {
    assert_eq!(
        Timestamp::from_unix_seconds(1_760_000_000)
            .expect("in range")
            .to_rfc3339(),
        "2025-10-09T08:53:20Z"
    );
}

#[test]
fn the_calendar_handles_leap_days_and_century_boundaries() {
    let leap_day = Timestamp::from_unix_seconds(1_709_208_000).expect("in range");
    assert_eq!(leap_day.to_rfc3339(), "2024-02-29T12:00:00Z");
    assert_eq!(
        Timestamp::from_rfc3339("2024-02-29T12:00:00Z"),
        Some(leap_day)
    );

    assert_eq!(
        Timestamp::from_unix_seconds(4_102_444_800)
            .expect("in range")
            .to_rfc3339(),
        "2100-01-01T00:00:00Z"
    );
    assert_eq!(
        Timestamp::from_unix_seconds(-1)
            .expect("in range")
            .to_rfc3339(),
        "1969-12-31T23:59:59Z"
    );
}

#[test]
fn every_instant_survives_the_round_trip_through_its_written_form() {
    for seconds in [
        -1,
        0,
        1,
        1_000_000_000,
        1_760_000_000,
        4_102_444_800,
        253_402_300_799,
    ] {
        let timestamp = Timestamp::from_unix_seconds(seconds).expect("in range");
        let text = timestamp.to_rfc3339();
        assert_eq!(
            Timestamp::from_rfc3339(&text),
            Some(timestamp),
            "{text} did not come back"
        );
    }
}

#[test]
fn the_representable_range_is_what_the_stored_form_can_write_and_read() {
    // The stored form names a four-digit year, so these two instants are the ends
    // of the range — and both survive the trip out and back.
    let earliest = Timestamp::from_unix_seconds(-62_167_219_200).expect("the earliest instant");
    assert_eq!(earliest.to_rfc3339(), "0000-01-01T00:00:00Z");
    assert_eq!(
        Timestamp::from_rfc3339("0000-01-01T00:00:00Z"),
        Some(earliest)
    );

    let latest = Timestamp::from_unix_seconds(253_402_300_799).expect("the latest instant");
    assert_eq!(latest.to_rfc3339(), "9999-12-31T23:59:59Z");
    assert_eq!(
        Timestamp::from_rfc3339("9999-12-31T23:59:59Z"),
        Some(latest)
    );

    let json = serde_json::to_string(&latest).expect("the instant serializes");
    assert_eq!(
        serde_json::from_str::<Timestamp>(&json).expect("and reads back"),
        latest
    );
}

#[test]
fn an_instant_the_stored_form_cannot_name_is_refused_at_construction() {
    // Without this, `to_rfc3339` could write a year the parser refuses, and a
    // Timestamp would serialize to JSON it cannot read back.
    for seconds in [-62_167_219_201, 253_402_300_800, i64::MIN, i64::MAX] {
        assert_eq!(Timestamp::from_unix_seconds(seconds), None, "{seconds}");
    }
    for text in ["10000-01-01T00:00:00Z", "-0001-12-31T23:59:59Z"] {
        assert_eq!(Timestamp::from_rfc3339(text), None, "{text}");
    }
}

#[test]
fn a_date_that_does_not_exist_is_refused_rather_than_shifted() {
    for text in [
        "2026-02-30T00:00:00Z",
        "2025-13-01T00:00:00Z",
        "2025-00-10T00:00:00Z",
        "2025-10-00T00:00:00Z",
        "2025-04-31T00:00:00Z",
    ] {
        assert_eq!(Timestamp::from_rfc3339(text), None, "{text}");
    }
}

#[test]
fn a_time_that_does_not_exist_is_refused_rather_than_shifted() {
    for text in [
        "2025-10-09T24:00:00Z",
        "2025-10-09T08:60:00Z",
        "2025-10-09T08:53:60Z",
    ] {
        assert_eq!(Timestamp::from_rfc3339(text), None, "{text}");
    }
}

#[test]
fn only_the_stored_form_is_accepted() {
    // A local offset, a fractional second, a missing Z: each would have to be
    // guessed at, and a guessed timestamp is worse than a refused one.
    for text in [
        "2025-10-09T08:53:20+02:00",
        "2025-10-09 08:53:20Z",
        "2025-10-09T08:53:20.500Z",
        "2025-10-09T08:53:20",
        "yesterday",
        "",
    ] {
        assert_eq!(Timestamp::from_rfc3339(text), None, "{text}");
    }
}

#[test]
fn a_field_of_the_wrong_width_or_sign_is_refused_rather_than_read_loosely() {
    // Parsing numbers loosely turns `-1` into an hour and shifts the instant a
    // day back; the promise is the fixed form, so the shape is checked first.
    for text in [
        "2025-10-09T-1:00:00Z",
        "2025-10-09T+1:00:00Z",
        "2025-10-09T8:53:20Z",
        "2025-10-09T08:5:20Z",
        "2025-9-09T08:53:20Z",
        "2025-10-9T08:53:20Z",
        "+2025-10-09T08:53:20Z",
        "2025-10-09T08:53:20z",
    ] {
        assert_eq!(Timestamp::from_rfc3339(text), None, "{text}");
    }
}

#[test]
fn the_json_document_is_the_artifact() {
    let artifact = example();
    let text = serde_json::to_string(&artifact).expect("the artifact serializes");
    let value: Value = serde_json::from_str(&text).expect("and is valid JSON");

    // The Passage and every piece of metadata the app attaches.
    assert_eq!(value["passage"], support::PASSAGE);
    assert_eq!(value["level"], "B2");
    assert_eq!(value["source_language"], "en");
    assert_eq!(value["native_language"], "Chinese");
    assert_eq!(value["provider"], "deepseek");
    assert_eq!(value["model"], "deepseek-flash");
    assert_eq!(value["thinking"], "off");
    assert_eq!(value["artifact_version"], 1);
    assert_eq!(value["prompt_version"], support::PROMPT_VERSION);
    assert_eq!(value["prompt_label"], "v7-descriptors");
    assert_eq!(value["created_at"], "2025-10-09T08:53:20Z");
    assert_eq!(value["generated_at"], "2025-10-09T08:53:20Z");

    // The model's four fields sit beside them, not behind a wrapper.
    assert_eq!(
        value["comprehensible"],
        "The committee tried hard to find out what had happened, but the manager had already gone into hiding."
    );
    assert_eq!(value["glosses"][2]["expression"], "go to ground");
    assert_eq!(
        value["glosses"][2]["gloss"],
        "hide so that nobody can find you"
    );
    assert_eq!(value["grammar"], Value::Null);
    assert_eq!(
        value["translation"],
        "委员会对此事进行了彻底调查，但那位经理已经躲了起来。"
    );

    let keys: BTreeSet<&str> = value
        .as_object()
        .expect("the document is an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        BTreeSet::from([
            "passage",
            "level",
            "source_language",
            "native_language",
            "provider",
            "model",
            "thinking",
            "artifact_version",
            "prompt_version",
            "prompt_label",
            "created_at",
            "generated_at",
            "comprehensible",
            "glosses",
            "grammar",
            "translation",
        ]),
        "the document's shape is pinned; adding a field to it is a decision"
    );
}

#[test]
fn an_artifact_survives_a_round_trip_through_json() {
    let artifact = example();
    let text = serde_json::to_string(&artifact).expect("the artifact serializes");

    let read: Artifact = serde_json::from_str(&text).expect("and reads back");
    assert_eq!(read, artifact);
}

#[test]
fn the_grammar_note_is_null_not_missing_when_there_is_none() {
    let artifact = example();
    let value = serde_json::to_value(&artifact).expect("the artifact serializes");

    assert!(value.get("grammar").is_some());
    assert_eq!(value["grammar"], Value::Null);
    let with_grammar = support::artifact(
        support::PASSAGE_WITH_GRAMMAR,
        Explanation::parse(support::ANSWER_WITH_GRAMMAR).expect("the fixture holds"),
    );
    assert_eq!(
        serde_json::to_value(&with_grammar).expect("serializes")["grammar"],
        json!(with_grammar.explanation.grammar.clone())
    );
}

#[test]
fn the_artifact_version_is_the_one_v0_records() {
    assert_eq!(ARTIFACT_VERSION, 1);
    assert_eq!(example().artifact_version, ARTIFACT_VERSION);
}
