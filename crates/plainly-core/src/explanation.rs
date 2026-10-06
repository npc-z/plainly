//! The Explanation contract: the one thing the model is asked for.
//!
//! The model returns content only — `comprehensible`, `glosses`, `grammar` and
//! `translation`. The Passage and every piece of metadata are attached by the app
//! ([`crate::Artifact`]), for two reasons that outlive this module: the
//! authoritative copy of the Passage is ours, so asking the model to echo it
//! would only give it a chance to change it silently; and the smaller the output,
//! the higher the share of models that obey the contract.
//!
//! [`wire_schema`] is the sole definition of the shape. [`Explanation::parse`]
//! validates against *that* rather than restating the shape in Rust, so a field
//! cannot be added to what we ask for without being validated on the way back,
//! and the two cannot drift. `docs/adr/0001-no-bounds-in-the-explanation-schema.md`
//! forbids every length, pattern and numeric bound here — OpenAI's strict mode
//! rejects them and a contradictory bound OOM-kills `llama-server` — so one test
//! scans the schema for them and another pins it to the shape the spec records.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// One Gloss: the expression that blocked the Passage, and a plain-English line
/// that must not be harder than the expression it explains.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gloss {
    pub expression: String,
    pub gloss: String,
}

/// What the model returns: the four content fields, and nothing else.
///
/// `grammar` is `None` when the paraphrase and glosses already leave the Passage
/// readable. It is not an optional field on the wire — the schema requires it and
/// allows `null` — because "there is no structural blocker" is a decision the
/// model has to make explicitly rather than by omission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Explanation {
    /// The Passage restated in more familiar English at the learner's Level.
    pub comprehensible: String,
    /// The Glosses, in the model's order: the most blocking expression first.
    pub glosses: Vec<Gloss>,
    /// The structural Blockers, if the shape of the sentence is the obstacle.
    pub grammar: Option<String>,
    /// The Passage's meaning in the learner's Native Language. Always last.
    pub translation: String,
}

/// The JSON Schema the model's answer is asked for and checked against.
///
/// It is sent to providers that can enforce a schema and used to validate the
/// answer from those that cannot, so both paths agree on what an Explanation is.
pub fn wire_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["comprehensible", "glosses", "grammar", "translation"],
        "properties": {
            "comprehensible": { "type": "string" },
            "glosses": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["expression", "gloss"],
                    "properties": {
                        "expression": { "type": "string" },
                        "gloss": { "type": "string" }
                    }
                }
            },
            "grammar": { "type": ["string", "null"] },
            "translation": { "type": "string" }
        }
    })
}

/// Why the model's answer is not an Explanation.
///
/// The two arms are different problems and get different treatment downstream:
/// a malformed answer is a sample the model can be asked to redo, while an answer
/// of the wrong shape repeated twice is drift, and redoing it wastes the learner's
/// time (tickets/04).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContractError {
    /// The answer was not JSON at all.
    #[error("the model did not answer with JSON: {message}")]
    MalformedJson { message: String },
    /// It was JSON, but not this JSON.
    #[error("the model's answer did not match the Explanation contract: {}", problems.join("; "))]
    Schema { problems: Vec<String> },
}

impl Explanation {
    /// Read the model's answer.
    ///
    /// A `null` field means "no such section", not "empty section": the only
    /// nullable field is `grammar`, and `null` there becomes `None`.
    pub fn parse(answer: &str) -> Result<Self, ContractError> {
        let value: Value =
            serde_json::from_str(answer).map_err(|source| ContractError::MalformedJson {
                message: source.to_string(),
            })?;

        let problems = validate(&wire_schema(), &value);
        if !problems.is_empty() {
            return Err(ContractError::Schema { problems });
        }

        // Unreachable while the schema and this struct agree, which is what the
        // contract tests are for. Mapped rather than unwrapped all the same: a
        // model's answer must never be able to panic the app.
        serde_json::from_value(value).map_err(|source| ContractError::Schema {
            problems: vec![source.to_string()],
        })
    }
}

/// Check `value` against `schema` and describe every violation.
///
/// The subset of JSON Schema implemented here is exactly what the wire schema
/// uses — `type`, `properties`, `required`, `additionalProperties` and `items`,
/// and nothing else. It is a validator and not a library because the schema we
/// send and the schema we enforce must be one document, and reaching for a
/// general implementation would buy generality this contract does not want. The
/// contract tests hold the schema to that keyword list, so a keyword the
/// validator would ignore cannot get in.
fn validate(schema: &Value, value: &Value) -> Vec<String> {
    let mut problems = Vec::new();
    validate_at(schema, value, "output", &mut problems);
    problems
}

fn validate_at(schema: &Value, value: &Value, path: &str, problems: &mut Vec<String>) {
    let Some(schema) = schema.as_object() else {
        problems.push(format!(
            "{path}: the contract's schema for this field is not an object"
        ));
        return;
    };

    if let Some(allowed) = schema.get("type")
        && !allowed_types(allowed).contains(&type_name(value))
    {
        problems.push(format!(
            "{path}: expected {}, got {}",
            allowed_types(allowed).join(" or "),
            type_name(value)
        ));
        // Every remaining keyword describes the members of an object or an
        // array, so there is nothing left to check once the type is wrong.
        return;
    }

    if let Value::Object(object) = value {
        let properties = schema.get("properties").and_then(Value::as_object);

        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for name in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(name) {
                    problems.push(format!("{path}: missing required field {name:?}"));
                }
            }
        }

        for (name, member) in object {
            match properties.and_then(|properties| properties.get(name)) {
                Some(member_schema) => {
                    validate_at(member_schema, member, &format!("{path}.{name}"), problems);
                }
                None if schema.get("additionalProperties") == Some(&Value::Bool(false)) => {
                    problems.push(format!("{path}: unexpected field {name:?}"));
                }
                None => {}
            }
        }
    }

    if let Value::Array(items) = value
        && let Some(item_schema) = schema.get("items")
    {
        for (index, item) in items.iter().enumerate() {
            validate_at(item_schema, item, &format!("{path}[{index}]"), problems);
        }
    }
}

/// The JSON type a value has, using the schema's own names.
fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// The types a `type` keyword allows. The keyword is either one name or an array
/// of names, which is how a nullable field is written.
fn allowed_types(allowed: &Value) -> Vec<&str> {
    match allowed {
        Value::String(name) => vec![name.as_str()],
        Value::Array(names) => names.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    }
}
