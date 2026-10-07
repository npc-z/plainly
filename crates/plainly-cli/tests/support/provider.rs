//! A fake OpenAI-compatible endpoint on loopback.
//!
//! The second seam of ticket 03: the real binary, a real (temporary) set of XDG
//! directories, and this stub in place of the cloud. It answers exactly the way
//! a provider does — `POST /v1/chat/completions`, a `choices[0].message.content`
//! — so a test can assert both what the CLI sent and what it did with an answer,
//! without a key, a model or any network beyond the loopback interface.

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
    pub body: Value,
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
    requests: Arc<Mutex<Vec<Recorded>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FakeProvider {
    /// Bind a loopback port and serve `replies` in order.
    ///
    /// A request that arrives with the script exhausted gets a 500, which makes
    /// an unexpectedly repeated call fail the test instead of hanging it.
    pub fn start(replies: impl IntoIterator<Item = Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port is available");
        let addr = listener
            .local_addr()
            .expect("the bound address is readable");
        listener
            .set_nonblocking(true)
            .expect("the listener can be non-blocking");

        let queue = Arc::new(Mutex::new(replies.into_iter().collect::<VecDeque<_>>()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));

        let thread = {
            let queue = Arc::clone(&queue);
            let requests = Arc::clone(&requests);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _)) => serve(&mut stream, &queue, &requests),
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
            requests,
            stop,
            thread: Some(thread),
        }
    }

    /// The base URL to write into `[providers.<name>] endpoint`.
    pub fn base_url(&self) -> String {
        format!("http://{}/v1", self.addr)
    }

    /// Every request received so far, in order.
    pub fn requests(&self) -> Vec<Recorded> {
        self.requests
            .lock()
            .expect("the request log is never poisoned")
            .clone()
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

/// Read one request and answer it from the script.
fn serve(stream: &mut TcpStream, queue: &Mutex<VecDeque<Reply>>, requests: &Mutex<Vec<Recorded>>) {
    let Some(raw) = read_request(stream) else {
        return;
    };
    if let Some(recorded) = parse_request(&raw) {
        requests
            .lock()
            .expect("the request log is never poisoned")
            .push(recorded);
    }

    let reply = queue
        .lock()
        .expect("the script is never poisoned")
        .pop_front();

    let (code, body) = match reply {
        Some(Reply::Content(content)) => (
            200,
            json!({
                "choices": [{
                    "message": { "role": "assistant", "content": content },
                    "finish_reason": "stop"
                }]
            })
            .to_string(),
        ),
        Some(Reply::Status { code, body }) => (code, body),
        None => (
            500,
            json!({ "error": "the stub was asked more times than it was scripted for" })
                .to_string(),
        ),
    };

    let _ = stream.write_all(response(code, &body).as_bytes());
    let _ = stream.flush();
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
    let body = serde_json::from_str(body).ok()?;
    Some(Recorded { method, path, body })
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
