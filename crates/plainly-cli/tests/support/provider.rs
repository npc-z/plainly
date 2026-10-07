//! A fake OpenAI-compatible endpoint on loopback, with Ollama's own route
//! beside it.
//!
//! The second seam of ticket 03, extended by ticket 05: the real binary, a real
//! (temporary) set of XDG directories, and this stub in place of the cloud. It
//! answers exactly the way a provider does — `POST /v1/chat/completions`, a
//! `choices[0].message.content`, `GET /v1/models` for the model list, and
//! `POST /api/chat` for Ollama — so a test can assert both what the CLI sent and
//! what it did with an answer, without a key, a model or any network beyond the
//! loopback interface.
//!
//! Probe requests are answered from [`Capability`] rather than from the script.
//! A probe is not the product under test in a test about explaining; it is the
//! thing that has to have happened, and scripting it per test would make every
//! explanation test say what shape the endpoint takes before it can say anything
//! about the Explanation.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::{Value, json};

/// One request the stub received, already split into its parts.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    /// The `Authorization` header, when the request carried one. Local
    /// discovery sends a key where a name has one, and a test has to be able to
    /// tell that it did (tickets/06).
    pub authorization: Option<String>,
    pub body: Value,
}

/// What the stub says its endpoint takes.
///
/// Deliberately not `plainly_core::Capability`: that type is the *conclusion* a
/// probe reaches, while this is the raw behaviour a fake endpoint exhibits —
/// the thing the conclusion is reached *from*.
#[derive(Debug, Clone)]
pub struct StubCapability {
    /// Whether a request carrying a JSON Schema is accepted.
    pub schema: bool,
    /// When set, every probe is answered with this status instead of the above:
    /// an endpoint that answers the run but not the experiment.
    pub probe_status: Option<u16>,
    /// Whether the vendor's own route exists, as opposed to only the
    /// OpenAI-compatible fallback.
    pub native: bool,
    /// When set, `GET /v1/models` is answered with this status instead of the
    /// list: an endpoint that explains but will not say what it serves.
    pub models_status: Option<u16>,
    /// The body `GET /v1/models` answers with.
    pub models: Value,
    /// When set, every request without `Authorization: Bearer <token>` is
    /// answered 401 — a loopback runtime started with `--api-key`, which is what
    /// spec §7 asks of the llama.cpp sidecar.
    pub token: Option<String>,
}

impl Default for StubCapability {
    fn default() -> Self {
        Self {
            schema: true,
            probe_status: None,
            native: true,
            models_status: None,
            models: json!({ "data": [{ "id": "stub-model" }] }),
            token: None,
        }
    }
}

impl StubCapability {
    /// An endpoint that rejects a JSON Schema, the way DeepSeek does.
    pub fn without_schema() -> Self {
        Self {
            schema: false,
            ..Self::default()
        }
    }
}

/// What the stub answers with, in the order the requests arrive.
#[derive(Debug, Clone)]
pub enum Reply {
    /// A 200 whose assistant message is `content`.
    Content(String),
    /// Any status, with a body of the test's choosing.
    Status { code: u16, body: String },
}

impl Reply {
    /// A 200 whose assistant message is `content`.
    pub fn content(content: impl Into<String>) -> Self {
        Reply::Content(content.into())
    }
}

pub struct FakeProvider {
    addr: SocketAddr,
    capability: Arc<Mutex<StubCapability>>,
    requests: Arc<Mutex<Vec<Recorded>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FakeProvider {
    /// Bind a loopback port, serve `replies` in order, and accept a JSON Schema
    /// from every probe.
    ///
    /// A request that arrives with the script exhausted gets a 500, which makes
    /// an unexpectedly repeated call fail the test instead of hanging it.
    pub fn start(replies: impl IntoIterator<Item = Reply>) -> Self {
        Self::start_with(StubCapability::default(), replies)
    }

    /// The same, with the endpoint's behaviour chosen by the test.
    pub fn start_with(
        capability: StubCapability,
        replies: impl IntoIterator<Item = Reply>,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port is available");
        let addr = listener
            .local_addr()
            .expect("the bound address is readable");
        listener
            .set_nonblocking(true)
            .expect("the listener can be non-blocking");

        let queue = Arc::new(Mutex::new(replies.into_iter().collect::<VecDeque<_>>()));
        let capability = Arc::new(Mutex::new(capability));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));

        let thread = {
            let queue = Arc::clone(&queue);
            let capability = Arc::clone(&capability);
            let requests = Arc::clone(&requests);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _)) => serve(&mut stream, &queue, &capability, &requests),
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(2));
                        }
                        Err(_) => break,
                    }
                }
            })
        };

        Self {
            addr,
            capability,
            requests,
            stop,
            thread: Some(thread),
        }
    }

    /// Change what the endpoint takes, mid-test: an endpoint that changed its
    /// mind after a cache was written is the case ticket 05's downgrade path
    /// exists for.
    pub fn set_capability(&self, capability: StubCapability) {
        *self
            .capability
            .lock()
            .expect("the capability is never poisoned") = capability;
    }

    /// The base URL to write into `[providers.<name>] endpoint`.
    pub fn base_url(&self) -> String {
        format!("http://{}/v1", self.addr)
    }

    /// The authority, for an endpoint that also serves a vendor route.
    pub fn authority(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Every request received so far, in order.
    pub fn requests(&self) -> Vec<Recorded> {
        self.requests
            .lock()
            .expect("the request log is never poisoned")
            .clone()
    }

    /// The requests that were a Passage being explained: a capability probe and
    /// a model listing are not.
    pub fn chat_requests(&self) -> Vec<Recorded> {
        self.requests()
            .into_iter()
            .filter(|request| request.method == "POST" && !is_probe(&request.body))
            .collect()
    }

    /// The capability probes the run sent, in order.
    pub fn probe_requests(&self) -> Vec<Recorded> {
        self.requests()
            .into_iter()
            .filter(|request| is_probe(&request.body))
            .collect()
    }
}

impl Drop for FakeProvider {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Whether a request is a capability probe rather than a Passage to explain.
fn is_probe(body: &Value) -> bool {
    body.get("messages")
        .and_then(|messages| messages.get(1))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        == Some(plainly_core::probe::PROBE_PASSAGE)
}

/// Read one request and answer it.
fn serve(
    stream: &mut TcpStream,
    queue: &Mutex<VecDeque<Reply>>,
    capability: &Mutex<StubCapability>,
    requests: &Mutex<Vec<Recorded>>,
) {
    let Some(raw) = read_request(stream) else {
        return;
    };
    let Some(request) = parse_request(&raw) else {
        return;
    };
    requests
        .lock()
        .expect("the request log is never poisoned")
        .push(request.clone());

    let (code, body) = answer(&request, queue, capability);

    let _ = stream.write_all(response(code, &body).as_bytes());
    let _ = stream.flush();
}

/// What the endpoint says to one request.
fn answer(
    request: &Recorded,
    queue: &Mutex<VecDeque<Reply>>,
    capability: &Mutex<StubCapability>,
) -> (u16, String) {
    let capability = capability
        .lock()
        .expect("the capability is never poisoned")
        .clone();

    // A keyed runtime answers 401 without its key, whatever the request was.
    if let Some(token) = &capability.token {
        if request.authorization.as_deref() != Some(format!("Bearer {token}").as_str()) {
            return (401, json!({ "error": "unauthorized" }).to_string());
        }
    }

    // The model list is discovery, not part of the script.
    if request.method == "GET" {
        return match capability.models_status {
            Some(code) => (code, json!({ "error": "no model list" }).to_string()),
            None => (200, capability.models.to_string()),
        };
    }

    let native = request.path.ends_with("/api/chat");
    if native && !capability.native {
        return (404, json!({ "error": "no such route" }).to_string());
    }

    if is_probe(&request.body) {
        if let Some(code) = capability.probe_status {
            return (
                code,
                json!({ "error": "the probe is not answered" }).to_string(),
            );
        }
        if !capability.schema && asks_for_schema(&request.body, native) {
            return (
                400,
                json!({ "error": "This response_format type is unavailable now" }).to_string(),
            );
        }
        return (200, content_answer("{\"ok\": true}", native));
    }

    match queue
        .lock()
        .expect("the script is never poisoned")
        .pop_front()
    {
        Some(Reply::Content(content)) => (200, content_answer(&content, native)),
        Some(Reply::Status { code, body }) => (code, body),
        None => (
            500,
            json!({ "error": "the stub was asked more times than it was scripted for" })
                .to_string(),
        ),
    }
}

/// Whether a request carries a JSON Schema, under whichever route's spelling.
fn asks_for_schema(body: &Value, native: bool) -> bool {
    if native {
        body.get("format").is_some_and(Value::is_object)
    } else {
        body.get("response_format")
            .and_then(|format| format.get("type"))
            == Some(&json!("json_schema"))
    }
}

/// A 200 body carrying `content`, in the envelope the route uses.
fn content_answer(content: &str, native: bool) -> String {
    if native {
        json!({
            "model": "stub-model",
            "message": { "role": "assistant", "content": content },
            "done": true,
            "done_reason": "stop"
        })
    } else {
        json!({
            "choices": [{
                "message": { "role": "assistant", "content": content },
                "finish_reason": "stop"
            }]
        })
    }
    .to_string()
}

/// Read a whole request: the head, then as many body bytes as it declares.
fn read_request(stream: &mut TcpStream) -> Option<String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("the stream accepts a read timeout");

    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut body_length = None;

    loop {
        if let Some(head_end) = find(&buffer, b"\r\n\r\n") {
            if body_length.is_none() {
                let head = String::from_utf8_lossy(&buffer[..head_end]);
                body_length = Some(content_length(&head));
            }
            if buffer.len() >= head_end + 4 + body_length.unwrap_or(0) {
                break;
            }
        }

        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
        }

        // A test that sends something absurd is a broken test, not a slow one.
        if buffer.len() > 4 * 1024 * 1024 {
            break;
        }
    }

    if buffer.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(&buffer).into_owned())
}

fn parse_request(raw: &str) -> Option<Recorded> {
    let (head, body) = raw.split_once("\r\n\r\n")?;
    let mut parts = head.lines().next()?.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    let authorization = head
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("authorization"))
        .map(|(_, value)| value.trim().to_string());
    // A GET carries no body, and that is not a broken request.
    let body = serde_json::from_str(body).unwrap_or(Value::Null);
    Some(Recorded {
        method,
        path,
        authorization,
        body,
    })
}

fn content_length(head: &str) -> usize {
    head.lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse().ok())
        .unwrap_or(0)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn response(code: u16, body: &str) -> String {
    let reason = match code {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        _ => "Status",
    };
    format!(
        "HTTP/1.1 {code} {reason}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    )
}
