//! The HTTP adapter: an OpenAI-compatible `/chat/completions` call.
//!
//! One adapter covers the cloud and the local runtimes, because all five speak
//! this endpoint. What differs between them is the request's *shape*, and that
//! is what [`ProviderSetup`] resolves into the fields this module sends: whether
//! a JSON Schema can be enforced at all (nested under
//! `response_format.json_schema.schema`, never the flat form), whether the
//! canonical thinking switch applies, and where the answer actually sits.
//!
//! It is deliberately the whole transport and no policy: one request, one
//! answer, no retry (tickets/04) and no probing (tickets/05). The retry policy
//! will sit above [`Provider::generate`], where it can see the transport
//! failure and the contract failure alike.

use std::time::Duration;

use serde_json::{Value, json};

use crate::Thinking;
use crate::explanation::wire_schema;
use crate::presets::{SchemaSupport, ThinkingSwitch};
use crate::provider::{ExplainRequest, Provider, ProviderError};
use crate::setup::ProviderSetup;

/// The completion budget every request is given.
///
/// 2048, not the 1200 floor, and deliberately paired with the 150-word splitting
/// threshold (spec §6): a 150-word chunk needs roughly 900 output tokens, and a
/// cut-off answer is malformed JSON, which looks like a model problem while the
/// root cause is a budget. Change one of the two and look at the other.
pub const MAX_TOKENS: u32 = 2048;

/// The budget when the user asked for the detailed mode.
///
/// Reasoning is spent before the answer exists, and it can be most of the
/// completion (spec §7). A 150-word chunk needs roughly 900 answer tokens, so
/// the ordinary budget leaves nothing for thinking; 8192 is what the prototype's
/// thinking runs used against DeepSeek, several times the 1304 reasoning tokens
/// measured for a single sentence, whose own default output budget is 64K. A
/// local runtime's context is the other half of that question, and tickets/06
/// probes it rather than this constant guessing.
pub const MAX_TOKENS_THINKING: u32 = 8192;

/// The floor is part of the constants rather than a comment below them, so the
/// pairing with the 150-word threshold cannot be forgotten.
const _: () = assert!(MAX_TOKENS >= 1200, "the budget must cover a 150-word chunk");
const _: () = assert!(
    MAX_TOKENS_THINKING >= MAX_TOKENS,
    "the detailed mode must not get a smaller budget"
);

/// How long one request may take. Covers a local model's cold start (measured at
/// 7.2 s) with room to spare.
pub const TIMEOUT: Duration = Duration::from_secs(60);

/// The name the schema is sent under. Providers echo it back; nothing branches
/// on it.
const SCHEMA_NAME: &str = "explanation";

/// How much of a failed response's body is quoted back. Enough to recognise
/// `This response_format type is unavailable now`, short enough not to paste a
/// page of HTML into a terminal.
const EXCERPT_LIMIT: usize = 300;

/// One OpenAI-compatible chat-completions endpoint.
pub struct ChatCompletions {
    setup: ProviderSetup,
    api_key: ApiKey,
    agent: ureq::Agent,
}

/// The API key, wrapped so that a `Debug` on anything holding one — this client
/// today, some future error type — prints that there is a key and not the key.
#[derive(Clone)]
struct ApiKey(String);

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKey(<redacted>)")
    }
}

impl ApiKey {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The key itself, for the one job that needs the value: finding it again in
    /// an error body before that body is repeated.
    fn secret(&self) -> &str {
        &self.0
    }

    /// The `Authorization` header value. Only called after [`ApiKey::is_empty`]
    /// said no, so a keyless endpoint never gets a bare `Bearer`.
    fn bearer(&self) -> String {
        format!("Bearer {}", self.0)
    }
}

impl ChatCompletions {
    /// Build a client for `setup`, authenticating with `api_key` when there is
    /// one. A loopback runtime that authenticates nothing gets an empty key, and
    /// an empty key sends no `Authorization` header at all.
    pub fn new(setup: ProviderSetup, api_key: impl Into<String>) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            // The status is not an error here: a 400 or a 500 still has a body,
            // and that body is the most useful part of the message.
            .http_status_as_error(false)
            .build();

        Self {
            setup,
            api_key: ApiKey(api_key.into()),
            agent: config.into(),
        }
    }

    /// The JSON body one request sends.
    ///
    /// Public because the shape is a contract with the endpoint rather than an
    /// implementation detail: the nesting of `response_format`, the thinking
    /// spelling and the token budget are exactly what a provider notices.
    pub fn request_body(&self, request: &ExplainRequest) -> Value {
        let mut body = json!({
            "model": self.setup.model,
            "messages": [
                { "role": "system", "content": request.system_prompt },
                { "role": "user", "content": request.passage },
            ],
            // temperature 0 because this is a rendering contract, not a
            // creative one (spec §7). It is silently ignored in thinking mode,
            // which is one more reason the default has thinking off.
            "temperature": 0,
            "max_tokens": self.max_tokens(),
            "response_format": self.response_format(),
        });

        // The canonical form, and only where the endpoint understands it:
        // `{"thinking": false}` is a 422 and `{"enable_thinking": false}` does
        // nothing at all. Thinking on needs no field: it is the default wherever
        // the switch exists.
        if self.setup.thinking == Thinking::Off
            && self.setup.thinking_switch == ThinkingSwitch::Canonical
        {
            body["thinking"] = json!({ "type": "disabled" });
        }

        body
    }

    /// The budget this request gets: the detailed mode reasons before it
    /// answers, so it is given room to.
    pub fn max_tokens(&self) -> u32 {
        match self.setup.thinking {
            Thinking::On => MAX_TOKENS_THINKING,
            Thinking::Off => MAX_TOKENS,
        }
    }

    /// The `response_format` this endpoint gets.
    fn response_format(&self) -> Value {
        match self.setup.schema {
            // The nested shape, always. The flat `response_format.schema` that
            // llama.cpp's own README documents is not read by the server and
            // silently degrades to "any JSON object".
            SchemaSupport::Enforced => json!({
                "type": "json_schema",
                "json_schema": {
                    "name": SCHEMA_NAME,
                    "strict": true,
                    "schema": wire_schema(),
                },
            }),
            // The best an endpoint without schema enforcement can be asked for.
            // DeepSeek's JSON mode also wants the word "json" in the prompt,
            // which the shipped prompt has.
            SchemaSupport::BestEffort => json!({ "type": "json_object" }),
        }
    }

    /// The assistant's content out of a decoded chat-completions body.
    ///
    /// Providers disagree about where an answer lands, and the disagreements are
    /// documented rather than hypothetical: LM Studio routes the constrained
    /// JSON into `reasoning_content` and leaves `content` empty for reasoning
    /// models, and OpenAI answers a safety refusal with a `refusal` field that
    /// does not follow the schema. Both are read here, before the decoder is
    /// handed anything.
    ///
    /// The client's own budget is what a cut-off answer is reported against:
    /// which budget was in play is the first thing to look at when an answer is
    /// truncated, and the two modes do not share one (tickets/04).
    pub fn answer_from(&self, body: &Value) -> Result<String, ProviderError> {
        let max_tokens = self.max_tokens();
        // `status: "incomplete"` is how OpenAI's Responses API reports an answer
        // that ran out of budget; the Chat Completions route this adapter uses
        // marks the same thing with `finish_reason` instead, so in practice this
        // branch is defensive — a compatible gateway may pass the field through,
        // and spec §7 asks for it by name. Either way it is checked before the
        // content, because what an incomplete answer carries is a prefix of
        // JSON, and reporting that as a contract violation would send the retry
        // policy after the model when the cause is the budget (spec §6).
        if body.get("status").and_then(Value::as_str) == Some("incomplete") {
            let reason = body
                .get("incomplete_details")
                .and_then(|details| details.get("reason"))
                .and_then(Value::as_str)
                .unwrap_or("no reason given");
            return Err(ProviderError::truncated(format!(
                "the model's answer is incomplete: {reason} (max_tokens = {max_tokens})"
            )));
        }

        let choice = body
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .ok_or_else(|| {
                // Not the model's doing — this is not a chat completion — but a
                // broken envelope can be a hiccup, and asking twice is cheap
                // because the repeat check stops a deterministic one.
                ProviderError::empty("the answer carried no choices")
            })?;

        let message = choice
            .get("message")
            .ok_or_else(|| ProviderError::empty("the answer carried no message"))?;

        if let Some(refusal) = non_blank(message.get("refusal").and_then(Value::as_str)) {
            return Err(refused(refusal));
        }

        // Truncation is checked before the content is handed over, because the
        // content of a truncated answer is a prefix of JSON that will fail the
        // contract — which would send the retry policy after the model when the
        // real cause is the budget (spec §6).
        if choice.get("finish_reason").and_then(Value::as_str) == Some("length") {
            return Err(ProviderError::truncated(format!(
                "the answer was cut off at max_tokens = {max_tokens}; \
                 the Passage may be too long for one request"
            )));
        }

        let content = match message.get("content") {
            Some(Value::String(content)) => non_blank(Some(content.as_str())).map(str::to_string),
            Some(Value::Array(parts)) => {
                if let Some(refusal) = refusal_in_parts(parts) {
                    return Err(refused(refusal));
                }
                text_in_parts(parts)
            }
            _ => None,
        };

        if let Some(content) = content {
            // A cut-off answer the endpoint did not label. Ollama's
            // OpenAI-compatible layer reports a length cut-off as a null
            // `finish_reason`, and what is left is a JSON prefix — which the
            // contract would report as a malformed answer and the policy would
            // then spend a second call on, with the same exhausted budget.
            // Naming the budget is what makes the retry decision right (spec §7).
            if stops_mid_json(&content) {
                return Err(ProviderError::truncated(format!(
                    "the answer stops in the middle of its JSON \
                     (max_tokens = {max_tokens}); the model's budget may have run out"
                )));
            }
            return Ok(content);
        }

        // The documented LM Studio routing bug: the schema-constrained answer
        // lands in `reasoning_content` and `content` is left empty. Recovering
        // it beats failing a good answer — but only when it is a JSON object.
        // A reasoning model that put its chain of thought there and then stopped
        // produced no answer at all, and handing prose to the contract check
        // would report it as a malformed answer and spend the retries on it
        // (tickets/04). Whether the object *is* an Explanation is the explain
        // path's question, asked in one place.
        if let Some(reasoning) = non_blank(message.get("reasoning_content").and_then(Value::as_str))
        {
            if serde_json::from_str::<Value>(reasoning).is_ok_and(|value| value.is_object()) {
                return Ok(reasoning.to_string());
            }
            return Err(ProviderError::empty(
                "the model spent its answer on reasoning and returned no Explanation",
            ));
        }

        Err(ProviderError::empty("the model answered with no content"))
    }

    /// The failure an HTTP status is, as the retry policy sees it.
    ///
    /// A 400 or 422 on a request whose shape *we* chose is read as a rejection
    /// of that shape: the capability we assumed for this endpoint is wrong, so
    /// asking again would fail identically and the answer is a downgrade and a
    /// fresh probe instead (tickets/05). Distinguishing "the shape" from "the
    /// model name" by searching the body would be a guess; the consequence of
    /// guessing this way is one cheap probe, and the consequence of the other
    /// way is an endpoint that can never be used.
    pub fn http_error(&self, status: u16, body: &str) -> ProviderError {
        let message = format!(
            "{} answered HTTP {status}: {}",
            self.setup.label,
            excerpt(body, self.api_key.secret())
        );

        match status {
            400 | 422 => ProviderError::unsupported_parameter(message),
            // The transient cases the backoff exists for.
            408 | 429 => ProviderError::unavailable(message),
            500..=599 => ProviderError::unavailable(message),
            // 401, 403, 404 and the rest: the configuration is what has to
            // change, not the timing.
            _ => ProviderError::misconfigured(message),
        }
    }

    /// The failure a transport error is, as the retry policy sees it.
    ///
    /// Public for the same reason [`ChatCompletions::http_error`] is: this
    /// mapping is what the policy acts on, and the repo tests that with a
    /// `tests/` module rather than in here.
    ///
    /// The variant is not enough to decide. ureq reports a name that does not
    /// resolve as an [`ureq::Error::Io`] from `to_socket_addrs` — `HostNotFound`
    /// is only the "resolved to nothing" case — so the io kind is what says
    /// whether resending the same request could work.
    pub fn transport_error(&self, error: ureq::Error) -> ProviderError {
        let message = format!("{}: {error}", self.setup.label);

        match error {
            // ureq said so itself: the connection never came up, or the request
            // ran out of time.
            ureq::Error::Timeout(_) | ureq::Error::ConnectionFailed => {
                ProviderError::unavailable(message)
            }
            // A connection that dropped, was refused by something that may not
            // be listening yet, or ended in the middle of the response: the
            // trouble is the timing, not the request.
            ureq::Error::Io(error) if retryable_io(error.kind()) => {
                ProviderError::unavailable(message)
            }
            // The server ended the exchange mid-protocol.
            ureq::Error::Protocol(_) => ProviderError::unavailable(message),
            // Everything else is a request that will fail the same way every
            // time: a name that does not resolve, a URL that cannot be parsed,
            // a redirect loop, a socket the process may not open. Classified
            // together with the 401s: the configuration is what has to change.
            _ => ProviderError::misconfigured(message),
        }
    }
}

/// Whether resending the same request could work, by the socket error's own
/// account of what happened.
///
/// The test is "did the connection fail in a way that says *not now* rather
/// than *not like this*". A refusal counts: a local runtime that is still
/// starting up is exactly that, and it is a common case here.
fn retryable_io(kind: std::io::ErrorKind) -> bool {
    use std::io::ErrorKind;

    matches!(
        kind,
        ErrorKind::ConnectionRefused
            | ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::TimedOut
            | ErrorKind::WouldBlock
            | ErrorKind::Interrupted
            // The stream ended before the response did.
            | ErrorKind::UnexpectedEof
    )
}

impl Provider for ChatCompletions {
    fn generate(&self, request: &ExplainRequest) -> Result<String, ProviderError> {
        let url = format!("{}/chat/completions", self.setup.endpoint);
        let mut call = self
            .agent
            .post(&url)
            .header("Content-Type", "application/json");
        if !self.api_key.is_empty() {
            call = call.header("Authorization", self.api_key.bearer());
        }

        let mut response = call
            .send_json(self.request_body(request))
            .map_err(|error| self.transport_error(error))?;

        let status = response.status();
        let text = response
            .body_mut()
            .read_to_string()
            .map_err(|error| self.transport_error(error))?;

        if !status.is_success() {
            return Err(self.http_error(status.as_u16(), &text));
        }

        let body: Value = serde_json::from_str(&text).map_err(|error| {
            // A 200 that is not a chat completion: the endpoint is not speaking
            // the protocol the configuration claims it does, but a proxy page
            // or a half-written response is a hiccup as often as it is a wrong
            // endpoint. Classified with the empty answers, which are asked for
            // again and stop at the second identical one.
            ProviderError::empty(format!(
                "{} answered HTTP {status} with something that is not JSON: {error}",
                self.setup.label
            ))
        })?;

        self.answer_from(&body)
    }
}

/// A string that is present and not just whitespace.
fn non_blank(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}

/// Whether a piece of content is JSON that ends in the middle.
///
/// `serde_json`'s `is_eof` is the difference between "the parser ran out of
/// input" and "the parser met something that is not JSON": a prefix of an
/// answer, rather than prose that was never going to be an Explanation.
fn stops_mid_json(content: &str) -> bool {
    matches!(serde_json::from_str::<Value>(content), Err(error) if error.is_eof())
}

/// What a refusal is reported as, wherever in the message it arrived.
fn refused(refusal: &str) -> ProviderError {
    ProviderError::refused(format!(
        "the model refused to explain this Passage: {refusal}"
    ))
}

/// A refusal in the array form of `content`, where OpenAI puts one.
fn refusal_in_parts(parts: &[Value]) -> Option<&str> {
    parts
        .iter()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("refusal"))
        .find_map(|part| {
            non_blank(
                part.get("refusal")
                    .or_else(|| part.get("text"))
                    .and_then(Value::as_str),
            )
        })
}

/// The text of the array form of `content`.
fn text_in_parts(parts: &[Value]) -> Option<String> {
    let joined: String = parts
        .iter()
        .filter_map(|part| {
            part.get("text")
                .or_else(|| part.get("content"))
                .and_then(Value::as_str)
                .filter(|text| !text.trim().is_empty())
        })
        .collect::<Vec<_>>()
        .join("\n");

    non_blank(Some(&joined)).map(str::to_string)
}

/// The start of a body, on one line and with credentials removed, for an error
/// message.
///
/// Providers do echo keys back — DeepSeek answers a bad key with the key in the
/// body — and stderr is where a CI log keeps it forever. Two passes, in this
/// order: the key this request actually carried is removed by value, which
/// cannot miss however the provider formatted it; then [`redact`] catches
/// credentials that are not ours, or that the body mangled into something a
/// literal search would not find.
fn excerpt(text: &str, secret: &str) -> String {
    let without_ours = hide(text, secret);
    let flat = redact(&without_ours);
    if flat.chars().count() <= EXCERPT_LIMIT {
        return flat;
    }
    let cut: String = flat.chars().take(EXCERPT_LIMIT).collect();
    format!("{cut}…")
}

/// Replace every exact occurrence of the key with `<redacted>`.
///
/// An empty key is not a value to search for: replacing it would put
/// `<redacted>` between every pair of characters.
fn hide(text: &str, secret: &str) -> String {
    if secret.is_empty() {
        return text.to_string();
    }
    text.replace(secret, "<redacted>")
}

/// Replace anything that looks like a credential with `<redacted>`.
fn redact(text: &str) -> String {
    let mut after_label = false;

    text.split_whitespace()
        .map(|word| {
            // Punctuation around a word is not part of it: `key:` and
            // `Bearer` are labels, `sk-abc,` is a key.
            let bare =
                word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_');
            let lower = bare.to_ascii_lowercase();

            let label = matches!(
                lower.as_str(),
                "bearer"
                    | "key"
                    | "apikey"
                    | "api_key"
                    | "api-key"
                    | "token"
                    | "secret"
                    | "password"
            );
            let looks_like_secret = lower.starts_with("sk-")
                || lower.starts_with("sk_")
                || bare.len() >= 24
                    && bare
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');

            let replacement = if after_label || looks_like_secret {
                "<redacted>".to_string()
            } else {
                word.to_string()
            };
            after_label = label;
            replacement
        })
        .collect::<Vec<_>>()
        .join(" ")
}
