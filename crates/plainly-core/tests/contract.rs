//! The Explanation contract: what the model is asked for, and what happens when
//! its answer is not that.

mod support;

use std::collections::BTreeSet;

use plainly_core::{ContractError, Explanation, wire_schema};
use serde_json::{Value, json};

/// The contract violations in an answer, asserting it failed the way the test
/// expects rather than some other way.
fn problems_with(answer: &str) -> Vec<String> {
    match Explanation::parse(answer) {
        Err(ContractError::Schema { problems }) => problems,
        other => panic!("expected a schema failure, got {other:?}"),
    }
}

#[test]
fn a_good_answer_becomes_an_explanation() {
    let explanation = Explanation::parse(support::ANSWER).expect("the contract holds");

    assert_eq!(
        explanation.comprehensible,
        "The committee tried hard to find out what had happened, but the manager had already gone into hiding."
    );
    assert_eq!(explanation.glosses.len(), 3);
    assert_eq!(
        explanation.glosses[0].expression,
        "conduct an investigation"
    );
    assert_eq!(
        explanation.glosses[0].gloss,
        "try to find out what happened"
    );
    assert_eq!(
        explanation.glosses[2].gloss,
        "hide so that nobody can find you"
    );
    assert_eq!(
        explanation.translation,
        "委员会对此事进行了彻底调查，但那位经理已经躲了起来。"
    );
}

#[test]
fn the_glosses_keep_the_order_the_model_gave_them() {
    // The prompt asks for the most blocking expression first. That order is part
    // of the artifact — the learner reads the section top down — so it is kept,
    // not sorted.
    let explanation = Explanation::parse(support::ANSWER).expect("the contract holds");
    let expressions: Vec<&str> = explanation
        .glosses
        .iter()
        .map(|gloss| gloss.expression.as_str())
        .collect();

    assert_eq!(
        expressions,
        ["conduct an investigation", "the matter", "go to ground"]
    );
}

#[test]
fn a_null_grammar_means_there_is_no_grammar_section() {
    let explanation = Explanation::parse(support::ANSWER).expect("the contract holds");
    assert_eq!(explanation.grammar, None);
}

#[test]
fn a_grammar_note_is_kept_whole() {
    let explanation = Explanation::parse(support::ANSWER_WITH_GRAMMAR).expect("the contract holds");

    let grammar = explanation
        .grammar
        .expect("the example is a structural one");
    assert!(
        grammar.starts_with("`Not until … did the manager admit …`"),
        "{grammar:?}"
    );
}

#[test]
fn an_answer_that_is_not_json_is_a_parse_failure() {
    let error = Explanation::parse("Sure! Here is the JSON you asked for:").unwrap_err();

    assert!(
        matches!(error, ContractError::MalformedJson { .. }),
        "got {error:?}"
    );
}

#[test]
fn an_empty_answer_is_a_parse_failure() {
    assert!(matches!(
        Explanation::parse("").unwrap_err(),
        ContractError::MalformedJson { .. }
    ));
}

#[test]
fn a_wrong_field_name_is_a_schema_failure() {
    // issues/14 measured exactly this drift on DeepSeek — `paraphrase` instead
    // of `comprehensible`, three attempts out of three — which is why the two
    // failures have to be tellable apart.
    let answer = support::ANSWER.replace("\"comprehensible\"", "\"paraphrase\"");
    let problems = problems_with(&answer);

    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("missing required field \"comprehensible\"")),
        "{problems:?}"
    );
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("unexpected field \"paraphrase\"")),
        "{problems:?}"
    );
}

#[test]
fn a_missing_field_is_a_schema_failure() {
    let problems = problems_with(r#"{ "comprehensible": "c", "glosses": [], "grammar": null }"#);

    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("missing required field \"translation\"")),
        "{problems:?}"
    );
}

#[test]
fn a_gloss_of_the_wrong_type_is_a_schema_failure() {
    let problems = problems_with(
        r#"{
            "comprehensible": "c",
            "glosses": ["the matter"],
            "grammar": null,
            "translation": "t"
        }"#,
    );

    assert_eq!(problems, ["output.glosses[0]: expected object, got string"]);
}

#[test]
fn a_number_where_a_string_belongs_is_a_schema_failure() {
    let problems = problems_with(
        r#"{
            "comprehensible": "c",
            "glosses": [{ "expression": "e", "gloss": 7 }],
            "grammar": null,
            "translation": "t"
        }"#,
    );

    assert_eq!(
        problems,
        ["output.glosses[0].gloss: expected string, got number"]
    );
}

#[test]
fn a_field_the_contract_does_not_have_is_a_schema_failure() {
    // The model does not get to add fields: the Passage and every piece of
    // metadata are the app's, and an answer that carries its own copy of them is
    // not trusted.
    let answer = support::ANSWER.replace(
        "\"translation\":",
        "\"passage\": \"a passage of its own\", \"translation\":",
    );
    let problems = problems_with(&answer);

    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("unexpected field \"passage\"")),
        "{problems:?}"
    );
}

#[test]
fn the_number_of_glosses_is_advisory_not_a_bound() {
    // ADR-0001 keeps every cardinality bound out of the schema — OpenAI's strict
    // mode rejects them, and a contradictory one OOM-kills llama-server. "Three
    // to five glosses" is a prompt rule, enforced by validation and retry.
    for count in [0, 1, 12] {
        let glosses: Vec<String> = (0..count)
            .map(|index| format!(r#"{{"expression":"e{index}","gloss":"g{index}"}}"#))
            .collect();
        let answer = format!(
            r#"{{"comprehensible":"c","glosses":[{}],"grammar":null,"translation":"t"}}"#,
            glosses.join(",")
        );

        let explanation = Explanation::parse(&answer)
            .unwrap_or_else(|error| panic!("{count} glosses should be allowed: {error}"));
        assert_eq!(explanation.glosses.len(), count);
    }
}

#[test]
fn an_empty_string_is_a_string() {
    // The contract fixes the shape, not the quality: an empty section is a
    // prompt problem (tickets/07), not a broken contract, and pretending otherwise
    // here would silently turn a bad answer into a retry.
    let explanation = Explanation::parse(
        r#"{ "comprehensible": "", "glosses": [], "grammar": "", "translation": "" }"#,
    )
    .expect("the shapes hold");

    assert_eq!(explanation.comprehensible, "");
    assert_eq!(explanation.grammar.as_deref(), Some(""));
}

#[test]
fn the_wire_schema_is_the_contract_the_spec_records() {
    let schema = wire_schema();

    assert_eq!(schema["type"], "object");
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(
        schema["required"],
        json!(["comprehensible", "glosses", "grammar", "translation"])
    );

    let names: BTreeSet<&str> = schema["properties"]
        .as_object()
        .expect("properties is an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        names,
        BTreeSet::from(["comprehensible", "glosses", "grammar", "translation"])
    );

    // Content only: the Passage and the metadata are the app's to attach.
    assert_eq!(schema["properties"]["comprehensible"]["type"], "string");
    assert_eq!(schema["properties"]["translation"]["type"], "string");

    // Required *and* nullable: "there is no structural blocker" is a decision the
    // model has to make, not an omission.
    assert_eq!(
        schema["properties"]["grammar"]["type"],
        json!(["string", "null"])
    );

    let glosses = &schema["properties"]["glosses"];
    assert_eq!(glosses["type"], "array");
    assert_eq!(glosses["items"]["type"], "object");
    assert_eq!(glosses["items"]["required"], json!(["expression", "gloss"]));
    assert_eq!(glosses["items"]["additionalProperties"], false);
    assert_eq!(
        glosses["items"]["properties"]["expression"]["type"],
        "string"
    );
    assert_eq!(glosses["items"]["properties"]["gloss"]["type"], "string");

    // The root cannot be a union (ADR-0001).
    for union in ["anyOf", "oneOf", "allOf", "not"] {
        assert!(schema.get(union).is_none(), "the root cannot use {union}");
    }
}

#[test]
fn the_wire_schema_uses_only_keywords_the_validator_understands() {
    // The validator implements exactly these five. A keyword outside the list
    // would be a rule nobody enforces, which is the drift this contract exists
    // to make impossible.
    const UNDERSTOOD: [&str; 5] = [
        "type",
        "properties",
        "required",
        "additionalProperties",
        "items",
    ];

    let mut found = Vec::new();
    keyword_positions(&wire_schema(), "output", &mut found);
    assert!(!found.is_empty());

    for (path, keyword) in found {
        assert!(
            UNDERSTOOD.contains(&keyword.as_str()),
            "the schema uses {keyword:?} at {path}, which the validator would ignore"
        );
    }
}

#[test]
fn the_wire_schema_carries_no_length_pattern_or_numeric_bound() {
    // ADR-0001. OpenAI's strict mode rejects these keywords outright, and on
    // llama.cpp a contradictory bound is an OOM from a single request.
    const FORBIDDEN: [&str; 10] = [
        "minItems",
        "maxItems",
        "minLength",
        "maxLength",
        "pattern",
        "format",
        "minimum",
        "maximum",
        "minProperties",
        "maxProperties",
    ];

    let mut found = Vec::new();
    collect_keys(&wire_schema(), &mut found);

    for keyword in FORBIDDEN {
        assert!(
            !found.iter().any(|found| found == keyword),
            "ADR-0001 forbids {keyword} anywhere in the wire schema"
        );
    }
}

/// Every keyword that appears *as a keyword* in a schema, and where. Keys inside
/// `properties` are field names, not keywords — the walk has to tell them apart
/// or it would reject the contract for naming its own fields.
fn keyword_positions(schema: &Value, path: &str, found: &mut Vec<(String, String)>) {
    let Value::Object(object) = schema else {
        return;
    };

    for (key, member) in object {
        found.push((path.to_string(), key.clone()));
        match key.as_str() {
            "properties" => {
                if let Value::Object(properties) = member {
                    for (name, property) in properties {
                        keyword_positions(property, &format!("{path}.properties.{name}"), found);
                    }
                }
            }
            "items" => keyword_positions(member, &format!("{path}.items"), found),
            _ => {}
        }
    }
}

/// Every key that appears anywhere in a JSON document.
fn collect_keys(value: &Value, found: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            for (key, member) in object {
                found.push(key.clone());
                collect_keys(member, found);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_keys(item, found);
            }
        }
        _ => {}
    }
}
