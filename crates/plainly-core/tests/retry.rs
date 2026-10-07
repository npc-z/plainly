//! The retry policy, driven by a scripted provider and a pause that records
//! instead of sleeping.
//!
//! What each class is *supposed* to do is spec §7's table; these tests pin the
//! parts that are easy to get subtly wrong: how many calls a class is worth, the
//! intervals between them, and the one case where a repeated error means stop
//! rather than back off again.

mod support;

use std::cell::RefCell;
use std::time::Duration;

use plainly_core::provider::ProviderError;
use plainly_core::retry::{self, Failure, FailureKind, Stopped};
use plainly_core::{Artifact, Timestamp};
use support::provider::FakeProvider;

fn now() -> Timestamp {
    Timestamp::from_unix_seconds(1_760_000_000).expect("the fixture instant is in range")
}

/// The waits the policy asked for, in order.
#[derive(Default)]
struct Pauses(RefCell<Vec<Duration>>);

impl Pauses {
    fn pause(&self, wait: Duration) {
        self.0.borrow_mut().push(wait);
    }

    fn taken(&self) -> Vec<Duration> {
        self.0.borrow().clone()
    }
}

/// Run the policy over a script: how many calls it made, what it decided, and
/// what it waited for. A script longer than the calls made proves the policy
/// stopped where the test says it did.
fn run(
    script: impl IntoIterator<Item = Result<String, ProviderError>>,
) -> (usize, Result<Artifact, Failure>, Vec<Duration>) {
    let provider = FakeProvider::scripted(script);
    let request = support::request(support::PASSAGE);
    let pauses = Pauses::default();

    let outcome = retry::explain(&provider, &request, now(), &|wait| pauses.pause(wait));

    (provider.calls(), outcome, pauses.taken())
}

#[test]
fn a_good_answer_is_asked_for_once() {
    let (calls, outcome, pauses) = run([Ok(support::ANSWER.to_string())]);

    assert!(outcome.is_ok(), "{outcome:?}");
    assert_eq!(calls, 1);
    assert!(pauses.is_empty(), "nothing to wait for");
}

#[test]
fn a_malformed_answer_is_asked_for_again_at_once() {
    let (calls, outcome, pauses) = run([
        Ok("I'd rather not, sorry.".to_string()),
        Ok(support::ANSWER.to_string()),
    ]);

    assert!(outcome.is_ok(), "{outcome:?}");
    assert_eq!(calls, 2);
    assert_eq!(
        pauses,
        vec![Duration::ZERO],
        "sampling noise needs no delay"
    );
}

#[test]
fn two_identical_unusable_answers_stop_before_a_third_call() {
    // The measured failure mode: a renamed field reproduced 3 times out of 3, so
    // the third call is not a retry, it is a repeat.
    let (calls, outcome, pauses) = run([
        Ok("I'd rather not, sorry.".to_string()),
        Ok("I'd rather not, sorry.".to_string()),
        Ok(support::ANSWER.to_string()),
    ]);

    let failure = outcome.expect_err("the same answer twice is systematic drift");
    assert_eq!(calls, 2);
    assert_eq!(failure.kind, FailureKind::Malformed);
    assert_eq!(failure.stopped, Stopped::Repeated);
    assert_eq!(failure.attempts, 2);
    assert!(failure.retried());
    assert_eq!(pauses, vec![Duration::ZERO]);
}

#[test]
fn a_malformed_answer_is_given_two_extra_attempts_and_no_more() {
    // Three *different* contract violations, so nothing here is drift: the
    // class is simply allowed two extra attempts (spec §7).
    let (calls, outcome, pauses) = run([
        Ok(r#"{"glosses": [], "grammar": null, "translation": "x"}"#.to_string()),
        Ok(r#"{"comprehensible": 1, "glosses": [], "grammar": null, "translation": "x"}"#
            .to_string()),
        Ok(
            r#"{"comprehensible": "a", "glosses": [{"expression": "e"}], "grammar": null, "translation": "x"}"#
                .to_string(),
        ),
        Ok(support::ANSWER.to_string()),
    ]);

    let failure = outcome.expect_err("three misses is the budget");
    assert_eq!(calls, 3);
    assert_eq!(failure.attempts, 3);
    assert_eq!(failure.kind, FailureKind::Malformed);
    assert_eq!(failure.stopped, Stopped::Exhausted);
    assert_eq!(pauses, vec![Duration::ZERO, Duration::ZERO]);
}

#[test]
fn the_same_failure_worded_differently_is_still_the_same_failure() {
    // A provider's message can carry a request id, a body excerpt, a timestamp.
    // The drift rule compares what the failure *is*, not how it was worded, or a
    // systematic failure would be retried to the end of the budget.
    let (calls, outcome, _) = run([
        Err(ProviderError::empty(
            "the model answered with no content (request 41f2)",
        )),
        Err(ProviderError::empty(
            "the model answered with no content (request 9c07)",
        )),
        Ok(support::ANSWER.to_string()),
    ]);

    let failure = outcome.expect_err("the same class twice is drift");
    assert_eq!(calls, 2);
    assert_eq!(failure.kind, FailureKind::Empty);
    assert_eq!(failure.stopped, Stopped::Repeated);
}

#[test]
fn a_rate_limit_backs_off_on_the_documented_schedule() {
    // Identical errors on purpose: a rate limit that repeats is what backing off
    // is for, so repeat detection must not cut the schedule short.
    let (calls, outcome, pauses) = run([
        Err(ProviderError::unavailable(
            "stub answered HTTP 429: slow down",
        )),
        Err(ProviderError::unavailable(
            "stub answered HTTP 429: slow down",
        )),
        Err(ProviderError::unavailable(
            "stub answered HTTP 429: slow down",
        )),
        Ok(support::ANSWER.to_string()),
    ]);

    let failure = outcome.expect_err("the budget runs out with the third attempt");
    assert_eq!(calls, 3, "two extra attempts, and no more");
    assert_eq!(pauses, vec![Duration::from_secs(1), Duration::from_secs(4)]);
    assert_eq!(failure.kind, FailureKind::Unavailable);
    assert_eq!(failure.stopped, Stopped::Exhausted);
    assert_eq!(failure.attempts, 3);
}

#[test]
fn a_server_error_backs_off_too() {
    let (calls, outcome, pauses) = run([
        Err(ProviderError::unavailable(
            "stub answered HTTP 500: upstream exploded",
        )),
        Ok(support::ANSWER.to_string()),
    ]);

    assert!(outcome.is_ok(), "{outcome:?}");
    assert_eq!(calls, 2);
    assert_eq!(pauses, vec![Duration::from_secs(1)]);
}

#[test]
fn an_immediate_retry_does_not_consume_a_backoff_slot() {
    // The schedule follows the network's failures, not the attempt count: a
    // malformed answer should not push the first 429 out to four seconds.
    let (calls, outcome, pauses) = run([
        Ok("not json".to_string()),
        Err(ProviderError::unavailable("stub answered HTTP 503")),
        Err(ProviderError::unavailable("stub answered HTTP 503")),
    ]);

    assert_eq!(calls, 3);
    assert_eq!(
        pauses,
        vec![Duration::ZERO, Duration::from_secs(1)],
        "the second failure is the first one that backs off"
    );
    assert_eq!(outcome.unwrap_err().stopped, Stopped::Exhausted);
}

#[test]
fn an_empty_answer_is_asked_for_again_until_it_repeats() {
    let (calls, outcome, pauses) = run([
        Err(ProviderError::empty("the model answered with no content")),
        Err(ProviderError::empty("the model answered with no content")),
        Ok(support::ANSWER.to_string()),
    ]);

    let failure = outcome.expect_err("an answer that never comes is not noise");
    assert_eq!(calls, 2);
    assert_eq!(failure.kind, FailureKind::Empty);
    assert_eq!(failure.stopped, Stopped::Repeated);
    assert_eq!(pauses, vec![Duration::ZERO]);
}

#[test]
fn an_unsupported_parameter_is_not_retried() {
    // Asking again would be asking the same question the endpoint just refused.
    let (calls, outcome, pauses) = run([
        Err(ProviderError::unsupported_parameter(
            "stub answered HTTP 400: This response_format type is unavailable now",
        )),
        Ok(support::ANSWER.to_string()),
    ]);

    let failure = outcome.expect_err("the shape, not the timing, is wrong");
    assert_eq!(calls, 1);
    assert!(pauses.is_empty());
    assert_eq!(failure.kind, FailureKind::UnsupportedParameter);
    assert_eq!(failure.stopped, Stopped::NotRetryable);
    assert!(!failure.retried());
    assert!(
        failure.reason.contains("response_format"),
        "the reason is what the endpoint said: {}",
        failure.reason
    );
}

#[test]
fn a_refusal_is_not_retried() {
    let (calls, outcome, pauses) = run([
        Err(ProviderError::refused(
            "the model refused to explain this Passage: no",
        )),
        Ok(support::ANSWER.to_string()),
    ]);

    let failure = outcome.expect_err("a refusal is an answer");
    assert_eq!(calls, 1);
    assert!(pauses.is_empty());
    assert_eq!(failure.kind, FailureKind::Refused);
    assert_eq!(failure.stopped, Stopped::NotRetryable);
}

#[test]
fn a_truncated_answer_is_not_retried_as_malformed() {
    let (calls, outcome, pauses) = run([
        Err(ProviderError::truncated(
            "the answer was cut off at max_tokens = 2048; \
             the Passage may be too long for one request",
        )),
        Ok(support::ANSWER.to_string()),
    ]);

    let failure = outcome.expect_err("the same budget cuts it off again");
    assert_eq!(calls, 1);
    assert!(pauses.is_empty());
    assert_eq!(failure.kind, FailureKind::Truncated);
    assert_eq!(failure.stopped, Stopped::NotRetryable);
    assert!(
        failure.reason.contains("max_tokens = 2048"),
        "the budget is named: {}",
        failure.reason
    );
}

#[test]
fn a_misconfigured_provider_is_not_retried() {
    let (calls, outcome, pauses) = run([
        Err(ProviderError::misconfigured(
            "stub answered HTTP 401: invalid key",
        )),
        Ok(support::ANSWER.to_string()),
    ]);

    let failure = outcome.expect_err("the configuration is what has to change");
    assert_eq!(calls, 1);
    assert!(pauses.is_empty());
    assert_eq!(failure.kind, FailureKind::Misconfigured);
    assert_eq!(failure.stopped, Stopped::NotRetryable);
}

#[test]
fn the_failure_reports_the_attempt_that_actually_failed() {
    let (calls, outcome, _) = run([
        Ok("the first answer was not JSON".to_string()),
        Err(ProviderError::refused(
            "the model refused on the second try",
        )),
        Ok(support::ANSWER.to_string()),
    ]);

    let failure = outcome.expect_err("a refusal is not retried");
    assert_eq!(calls, 2);
    assert_eq!(failure.attempts, 2);
    assert_eq!(failure.kind, FailureKind::Refused);
    assert!(
        failure.reason.contains("second try"),
        "the reason belongs to the last query, not the first: {}",
        failure.reason
    );
}
