//! The explain path: one provider, one contract, one Artifact — driven by a
//! scripted provider and no network.

mod support;

use plainly_core::provider::ProviderError;
use plainly_core::{ContractError, ExplainError, Explanation, Level, Thinking, Timestamp, explain};
use support::provider::FakeProvider;

fn now() -> Timestamp {
    Timestamp::from_unix_seconds(1_760_000_000).expect("the fixture instant is in range")
}

#[test]
fn a_good_answer_becomes_an_artifact_with_our_passage_and_the_metadata_we_stamped() {
    let provider = FakeProvider::saying(support::ANSWER);
    let request = support::request(support::PASSAGE);

    let artifact = explain(&provider, &request, now()).expect("the contract holds");

    // The Passage is the one we sent: the model is never asked to echo it, so it
    // never gets the chance to change it.
    assert_eq!(artifact.passage, support::PASSAGE);
    assert_eq!(artifact.level, Level::B2);
    assert_eq!(artifact.source_language, "en");
    assert_eq!(artifact.native_language, "Chinese");
    assert_eq!(artifact.provider, "deepseek");
    assert_eq!(artifact.model, "deepseek-flash");
    assert_eq!(artifact.thinking, Thinking::Off);
    assert_eq!(artifact.artifact_version, 1);
    assert_eq!(artifact.prompt_version, support::PROMPT_VERSION);
    assert_eq!(artifact.prompt_label, "v7-descriptors");
    assert_eq!(artifact.created_at, now());
    assert_eq!(artifact.generated_at, now());
    assert_eq!(
        artifact.explanation,
        Explanation::parse(support::ANSWER).expect("the fixture holds the contract")
    );
}

#[test]
fn the_provider_is_asked_once_with_the_request_it_was_given() {
    let provider = FakeProvider::saying(support::ANSWER);
    let request = support::request(support::PASSAGE);

    explain(&provider, &request, now()).expect("the contract holds");

    assert_eq!(provider.calls(), 1);
    assert_eq!(provider.requests(), vec![request]);
}

#[test]
fn an_answer_that_is_not_json_comes_back_as_a_contract_failure() {
    let provider = FakeProvider::saying("I'm sorry, I can't help with that.");
    let request = support::request(support::PASSAGE);

    let error = explain(&provider, &request, now()).unwrap_err();

    assert!(
        matches!(
            error,
            ExplainError::Contract(ContractError::MalformedJson { .. })
        ),
        "got {error:?}"
    );
}

#[test]
fn a_wrong_field_name_comes_back_as_a_schema_failure() {
    let provider =
        FakeProvider::saying(support::ANSWER.replace("\"comprehensible\"", "\"paraphrase\""));
    let request = support::request(support::PASSAGE);

    let error = explain(&provider, &request, now()).unwrap_err();

    let ExplainError::Contract(ContractError::Schema { problems }) = error else {
        panic!("expected a schema failure, got {error:?}");
    };
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("missing required field \"comprehensible\"")),
        "{problems:?}"
    );
}

#[test]
fn a_provider_failure_is_told_apart_from_a_contract_failure() {
    let provider = FakeProvider::scripted([Err(ProviderError::unavailable("connection refused"))]);
    let request = support::request(support::PASSAGE);

    let error = explain(&provider, &request, now()).unwrap_err();

    assert!(matches!(error, ExplainError::Provider(_)), "got {error:?}");
    // What the learner is told is which side failed: the provider never answered,
    // so nothing about the model's answer is being reported.
    assert!(error.to_string().contains("connection refused"));
    assert!(!error.to_string().contains("contract"));
    assert_eq!(provider.calls(), 1);
}

#[test]
fn the_model_cannot_smuggle_a_passage_of_its_own_into_the_artifact() {
    // A provider that returns a `passage` field instead of the contract's fields
    // fails the contract; the Passage in the Artifact is still the caller's.
    let provider = FakeProvider::saying(
        r#"{
            "comprehensible": "…",
            "glosses": [],
            "grammar": null,
            "passage": "something else entirely"
        }"#,
    );
    let request = support::request(support::PASSAGE);

    let error = explain(&provider, &request, now()).unwrap_err();

    assert!(
        matches!(error, ExplainError::Contract(ContractError::Schema { .. })),
        "got {error:?}"
    );
    assert_eq!(request.passage, support::PASSAGE);
}

#[test]
fn the_two_timestamps_agree_on_a_freshly_generated_artifact() {
    // Created and generated part ways only once a Record is reused or
    // regenerated (tickets/08); one call produces one instant.
    let provider = FakeProvider::saying(support::ANSWER);
    let request = support::request(support::PASSAGE);

    let artifact = explain(&provider, &request, now()).expect("the contract holds");

    assert_eq!(artifact.created_at, artifact.generated_at);
    assert_eq!(artifact.created_at.unix_seconds(), 1_760_000_000);
}
