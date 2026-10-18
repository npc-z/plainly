//! Putting an Explanation on the clipboard.
//!
//! The main window is a focused, ordinary window, so it only writes the
//! clipboard; the selection has to outlive the call, and a window that exits
//! must not take it away.
//!
//! Two routes, in order: `wl-copy` from `wl-clipboard`, which forks and serves
//! the selection until something else claims it, and `xclip` with the same trick
//! for an X11 session. Neither is a crate dependency: both are tiny and already
//! on a desktop. Both failing is reported rather than swallowed, because a
//! "已复制" that copied nothing is worse than a sentence saying it did not take.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::error::WindowError;

/// How long a route is given to finish.
///
/// Not a bound on the work: both programs hand the selection to a process that
/// outlives them. It bounds a client stuck before that, because these run on the
/// window's own thread and a thread that waits forever is a window that freezes.
const ROUTE_TIMEOUT: Duration = Duration::from_secs(5);

const POLL: Duration = Duration::from_millis(25);

/// One route to the clipboard: a program that takes the text on stdin and keeps
/// serving the selection after this call returns.
struct Route {
    program: &'static str,
    args: &'static [&'static str],
    failure: fn(&str) -> String,
    /// How long this route is given, and how often it is looked at. Per route so
    /// the deadline test does not spend five seconds proving one.
    timeout: Duration,
    poll: Duration,
}

/// The routes, in the order they are tried.
const ROUTES: [Route; 2] = [
    Route {
        program: "wl-copy",
        args: &[],
        failure: crate::strings::clipboard_wl_copy,
        timeout: ROUTE_TIMEOUT,
        poll: POLL,
    },
    Route {
        program: "xclip",
        args: &["-selection", "clipboard"],
        failure: crate::strings::clipboard_xclip,
        timeout: ROUTE_TIMEOUT,
        poll: POLL,
    },
];


/// Put `text` on the clipboard, or say why it could not go.
pub fn copy(text: &str) -> Result<(), WindowError> {
    let mut failures: Vec<String> = Vec::new();

    for route in &ROUTES {
        match run(route, text) {
            Ok(()) => return Ok(()),
            Err(reason) => failures.push((route.failure)(&reason)),
        }
    }

    Err(WindowError::Clipboard {
        message: failures.join("；"),
    })
}

/// Run one route over `text`.
///
/// The exit status is read rather than ignored: a program that accepted the text
/// and then died leaves nothing on the clipboard, so "it started" is not "it
/// worked".
///
/// Its **stderr is not read, and deliberately not piped**. `wl-copy` forks a
/// child that keeps serving the selection after the parent returns, and that
/// child inherits the pipe, so reading to EOF would wait for the selection to be
/// claimed and 「复制 markdown」 would appear to hang. The cost is the program's
/// own diagnostic in the failure message. The exit status is still read.
fn run(route: &Route, text: &str) -> Result<(), String> {
    let mut child = Command::new(route.program)
        .args(route.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("起不来：{error}"))?;

    // A route that has already exited closed its input, and a `BrokenPipe` here
    // means there was nobody left to read the text. The exit status below is what
    // says whether the clipboard got anything, so the write is best-effort and
    // every other write error is real.
    if let Some(mut input) = child.stdin.take() {
        if let Err(error) = input.write_all(text.as_bytes())
            && error.kind() != std::io::ErrorKind::BrokenPipe
        {
            let _ = child.wait();
            return Err(format!("写不进去：{error}"));
        }
    }
    // Dropping stdin is what tells the program that the text has ended.

    // Polled rather than waited on with a deadline, because the standard library
    // has no such wait and these commands run on the window's own thread.
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(format!("退出码 {:?}", status.code())),
            Ok(None) if started.elapsed() < route.timeout => std::thread::sleep(route.poll),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "超过 {} 秒没有退出，已经把它结束了",
                    route.timeout.as_secs()
                ));
            }
            Err(error) => return Err(format!("没有正常退出：{error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A route that never exits: what a stuck clipboard client looks like, and
    /// the case the deadline exists for. It is `sleep` rather than a stub so
    /// that the real spawn, write and reap path is the one under test.
    fn hanging() -> Route {
        Route {
            program: "sleep",
            args: &["30"],
            failure: |reason| reason.to_string(),
            timeout: Duration::from_millis(150),
            poll: Duration::from_millis(10),
        }
    }

    #[test]
    fn a_route_that_never_exits_is_killed_rather_than_waited_for() {
        let started = Instant::now();

        let refused = run(&hanging(), "text");

        assert!(refused.is_err(), "a hung route is a failure, not a success");
        assert!(
            refused.unwrap_err().contains("没有退出"),
            "the failure says the deadline was the problem"
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the wait did not run to the child's own thirty seconds"
        );
    }

    #[test]
    fn a_route_that_exits_unsuccessfully_is_reported_with_its_code() {
        let route = Route {
            program: "false",
            args: &[],
            failure: |reason| reason.to_string(),
            timeout: Duration::from_secs(5),
            poll: Duration::from_millis(10),
        };

        assert!(run(&route, "text").is_err());
    }

    #[test]
    fn a_route_that_reads_its_input_and_exits_successfully_is_the_end_of_it() {
        // `cat` rather than `true`: a route has to read what it is given, and
        // `true` exits before the write, which is a race rather than a success.
        let route = Route {
            program: "cat",
            args: &[],
            failure: |reason| reason.to_string(),
            timeout: Duration::from_secs(5),
            poll: Duration::from_millis(10),
        };

        assert!(run(&route, "text").is_ok());
    }

    #[test]
    fn a_route_that_exits_before_reading_is_not_a_write_failure() {
        // The same race, as a case of its own: `true` may exit before the text
        // is written, and a closed input pipe is not the failure to report.
        let route = Route {
            program: "true",
            args: &[],
            failure: |reason| reason.to_string(),
            timeout: Duration::from_secs(5),
            poll: Duration::from_millis(10),
        };

        assert!(run(&route, "text").is_ok());
    }
}
