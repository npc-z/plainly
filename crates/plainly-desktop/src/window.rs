//! The process: one window, one socket, one listener, and the two commands that
//! are not about the window's own state.
//!
//! Startup order matters here and is the whole point of this file:
//!
//! 1. the window's state is opened — the history store and the configuration —
//!    because a failure there is something the window has to *show*, not
//!    something to exit over;
//! 2. the single-instance lock is claimed, and a second launch hands over and
//!    stops here;
//! 3. the listener is started, if this launch was asked for one;
//! 4. the window is built and the socket begins to listen.
//!
//! (1) deliberately happens before (2), because between (2) and (3) this process
//! is a window that nobody can reach: a second launch while the store is being
//! opened would wait on a handover that is minutes away on a cold machine.
//! Opening the store first costs a handed-over launch a little work it discards,
//! and buys every handover a socket that is already there.
//!
//! The listener is started **before** the webview and on a thread of its own, so
//! that a clipboard the session will not give up cannot hold the window back: a
//! `--watch` launch is also a window, and a window that never opened because the
//! clipboard was busy would be the worst of both.

use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use tauri::{Manager, WebviewWindow};

use crate::Window;
use crate::app::{Desktop, Environment};
use crate::single::Activation;
use crate::single::Role;
use crate::watch;

/// What this launch was asked to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The window, and nothing else: what a desktop shortcut and the panel's ⤢
    /// start, and what every launch is when the switch has never been turned on.
    Window,
    /// The window **and** the copy listener (spec §12). The listener ends when
    /// the switch is turned off, when the window shuts down, or when a later
    /// launch that is not a watcher asks for the window.
    Watch,
}

impl Mode {
    /// Whether this launch is a watcher.
    pub fn watching(self) -> bool {
        matches!(self, Mode::Watch)
    }
}

/// What one launch of `plainly-desktop` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Run {
    /// This process held the window until it closed.
    Served,
    /// Another window was already open, and it has been asked to come forward.
    Handed,
}

/// Give GDK the DPI it needs when the session does not supply one.
///
/// WebKitGTK derives its device pixel ratio from GDK's screen resolution —
/// multiplied by the GTK scale factor, over 96 — and `gdk_screen_get_resolution`
/// answers `-1` when nothing provides one. An XSettings daemon is what normally
/// provides one and a niri or sway session usually runs none, so on those
/// sessions the ratio comes out at 1/48: the layout viewport becomes 96 times
/// the window (1180×760 logical px laid out on a 113280×72960 CSS viewport), the
/// `.win` column resolves to `height: 0`, and the interface is drawn into the
/// window's top-left corner with the rest of it blank. Nothing the frontend can
/// ask would recover from that — `100vh`, `devicePixelRatio` and every rect are
/// wrong — so the missing DPI is supplied here instead.
///
/// Only an *unset* resolution is filled in: a session that has one (a real
/// `Xft.dpi` from an XSettings daemon, text scaling and all) keeps it, because
/// that value is the user's and is what the ratio is meant to follow.
///
/// It has to happen before the webview exists. Changing the device scale factor
/// afterwards is not applied to the live page (WebKit bug 259334), which is why
/// this is called from the top of [`run`] rather than from a window callback.
/// `gtk::init` here is also what makes the later one in Tauri's own backend a
/// no-op: it returns `Ok` immediately once the main thread is initialized.
fn screen_dpi() {
    if gtk::init().is_err() {
        // No display for GTK to initialize against. The window cannot open at
        // all in that case and Tauri reports why; there is nothing to set here.
        return;
    }

    if let Some(screen) = gtk::gdk::Screen::default()
        && screen.resolution() < 0.0
    {
        // 96 is what `-1` stands in for: GTK's own fallback, and the value the
        // ratio is defined against.
        screen.set_resolution(96.0);
    }
}

/// Build and run the window.
pub fn run(mode: Mode) -> Result<Run, String> {
    // First, before the store, the lock and the webview: it is the one thing the
    // interface's whole layout depends on.
    screen_dpi();

    let (activate, activations) = mpsc::channel::<Activation>();

    // Opened first, and a failure is carried rather than fatal: a store that
    // will not open is exactly the thing a person needs the window to tell them
    // about, and a process that exits with a line on stderr tells nobody who
    // started it from a desktop launcher. `commands::Window` holds the `Result`
    // and every screen draws it.
    //
    // Before the lock, so that the lock is only held by a process that is ready
    // to answer a handover.
    let desktop = match Desktop::open(Environment::observe()) {
        Ok(desktop) => Ok(desktop),
        Err(error) => Err(crate::strings::cannot_open(&error.to_string())),
    };

    // A second launch hands over from here and pays for the state it will not
    // use — a store read and possibly a keyring round trip, no webview.
    let role = crate::single::claim(activate, mode.watching())?;
    if role == Role::Handed {
        return Ok(Run::Handed);
    }

    // Started before the webview: the listener has nothing to do with the window
    // and everything to do with the switch, and a session that will not give up
    // the clipboard must not become a window that never opens.
    let watching = Arc::new(Mutex::new(None));
    let reported = Arc::new(Mutex::new(None));
    let listening = match (&desktop, mode) {
        (Ok(desktop), Mode::Watch) => match watch::start(desktop.config_file()) {
            Ok(handle) => {
                *watching.lock().expect("the listener handle is not poisoned") = Some(handle);
                true
            }
            Err(reason) => {
                // The same dedupe the handover path uses: this launch addressed
                // an activation to itself, so without it the sentence would be
                // printed again a moment later by `serve`.
                eprintln!("{reason}");
                *reported.lock().expect("the report is not poisoned") = Some(reason);
                false
            }
        },
        _ => false,
    };

    // The listener's config path, so that a handover can start a listener this
    // process does not have: the handover is the same request `--watch` makes, and
    // it arrives at a window that is already open.
    let setup_watching = Watching {
        config: match &desktop {
            Ok(desktop) => Some(desktop.config_file()),
            Err(_) => None,
        },
        running: Arc::clone(&watching),
        reported: Arc::clone(&reported),
    };
    let application = tauri::Builder::default()
        .manage(Window(std::sync::Mutex::new(desktop)))
        .invoke_handler(crate::handlers!())
        .setup(move |app| {
            // The window comes forward when this is where the activation is
            // acted on, so that "the panel opened it" and "a second launch asked
            // for it" cannot drift apart.
            let window = app
                .get_webview_window("main")
                .ok_or_else(|| "the window is missing from the configuration".to_string())?;
            forward(activations, window, setup_watching);

            // Said on the launch that asked for it, and only there: a watcher
            // that started silently is one nobody can tell from a watcher that
            // failed to start.
            if listening {
                eprintln!("{}", crate::strings::WATCH_STARTED);
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .map_err(|error| crate::strings::cannot_start(&error.to_string()))?;

    application.run(|_, _| {});

    // The window is gone, so the listener has nothing to report to and no reason
    // to outlive it: it is one process, and `--watch` made it resident, not
    // immortal. Joined rather than detached so that the exit is one event.
    Watching {
        config: None,
        running: Arc::clone(&watching),
        reported: Arc::clone(&reported),
    }
    .shut();

    Ok(Run::Served)
}

/// The listener as the window holds it, and the one decision a handover makes
/// about it.
///
/// A value rather than two fields inside the closure, because the decision is
/// the part worth testing: whether a handover starts, keeps or stops the
/// listener has nothing to do with webviews, and the window it is reached
/// through is exactly what the tests in this crate cannot build.
#[derive(Clone, Default)]
pub struct Watching {
    /// Where the configuration is, or `None` when the window could not open its
    /// state at all — in which case there is no switch to read and no listener to
    /// start.
    config: Option<PathBuf>,
    running: Arc<Mutex<Option<watch::Handle>>>,
    /// The last refusal, if the one in hand is a repeat of it.
    ///
    /// It is what keeps a refusal from being *reported* twice — the launch that
    /// asked for the listener starts one, and then the handover it addressed to
    /// itself arrives and would ask again — without stopping a later attempt from
    /// being made: a session that could not give the clipboard up a moment ago
    /// may be able to now, and a latch would hide that it works.
    reported: Arc<Mutex<Option<String>>>,
}

/// What a handover asked of the listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wanted {
    /// A listener is running now, started or already there.
    Listening,
    /// A listener was running and has been stopped, and this call waited for it.
    Stopped,
    /// There is nothing to listen with, or nothing to stop.
    Nothing,
}

impl Watching {
    /// The listener this window would start when a handover asks for one.
    pub fn new(config: Option<PathBuf>) -> Self {
        Self {
            config,
            running: Arc::new(Mutex::new(None)),
            reported: Arc::new(Mutex::new(None)),
        }
    }

    /// Do what `activation` asks, synchronously.
    ///
    /// `start` runs only when a listener is actually needed, so that the common
    /// case — a handover that asks for the window — never pays for a connection
    /// to the compositor. It is a parameter rather than a call to
    /// [`watch::start`] so that this decision can be tested without a Wayland
    /// session; [`Self::serve`] is the production caller.
    pub fn ask(
        &self,
        activation: Activation,
        start: impl FnOnce(PathBuf) -> Result<watch::Handle, String>,
    ) -> Result<Wanted, String> {
        let mut running = self
            .running
            .lock()
            .expect("the listener handle is not poisoned");

        if activation.watch {
            if running.as_ref().is_some_and(watch::Handle::running) {
                return Ok(Wanted::Listening);
            }
            // Not running any more — it ended on its own when the switch was
            // turned off, most likely. The handle is a corpse from here, and the
            // request is for a listener, so one is started below.
            let Some(config) = self.config.clone() else {
                return Ok(Wanted::Nothing);
            };

            return match start(config) {
                Ok(handle) => {
                    *running = Some(handle);
                    Ok(Wanted::Listening)
                }
                Err(reason) => Err(reason),
            };
        }

        match running.take() {
            Some(listener) => {
                // The lock goes before the join: stopping waits for the listener's
                // thread, and this is the only other thread that takes the lock.
                drop(running);
                listener.stop(true);
                Ok(Wanted::Stopped)
            }
            None => Ok(Wanted::Nothing),
        }
    }

    /// The same, over the real listener.
    ///
    /// The failure is reported rather than returned: a handover is answered on a
    /// thread with nobody to hand a `Result` to, and the sentence belongs on
    /// stderr either way. It is reported **once**, which is why the attempts are
    /// remembered: the launch that starts a listener sends itself an activation
    /// as well, and a start that failed would otherwise be tried and printed
    /// twice in a row.
    pub fn serve(&self, activation: Activation) {
        let outcome = self.ask(activation, |config| {
            let handle = watch::start(config)?;
            eprintln!("{}", crate::strings::WATCH_STARTED);
            Ok(handle)
        });

        match outcome {
            Ok(_) => {
                *self.reported.lock().expect("the report is not poisoned") = None;
            }
            Err(reason) => {
                let mut reported = self.reported.lock().expect("the report is not poisoned");
                // Reported once per distinct reason: asking twice for the same
                // listener in the same session is one fact, and the sentence is
                // already on stderr.
                if reported.as_deref() != Some(reason.as_str()) {
                    eprintln!("{reason}");
                    *reported = Some(reason);
                }
            }
        }
    }

    /// Stop the listener, if one is running, and wait for it.
    ///
    /// Called when the window closes: the listener is one process, and `--watch`
    /// made it resident, not immortal.
    pub fn shut(&self) {
        let running = self
            .running
            .lock()
            .expect("the listener handle is not poisoned")
            .take();
        if let Some(listener) = running {
            // The window is closing: the listener goes quietly, with it.
            listener.stop(false);
        }
    }
}

/// Hand every activation to the window, and act on the one part of it that is
/// not about the window.
///
/// The socket's listener runs on a thread of its own, and Tauri's window
/// operations are safe to call from there: each one is posted to the event loop
/// and performed on the main thread, so this thread is a messenger rather than a
/// second owner of the window.
///
/// The order inside the loop is deliberate: the window comes forward first, so
/// that a person pressing the shortcut sees the window even if stopping the
/// listener takes a moment.
fn forward(activations: mpsc::Receiver<Activation>, window: WebviewWindow, watching: Watching) {
    std::thread::spawn(move || {
        while let Ok(activation) = activations.recv() {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();

            // A launch that is a watcher means "keep listening", and a launch
            // that is not means "the window, not a listener". The first is what
            // starts one here at all: the window a handover reaches may have been
            // opened without one — the panel's ⤢, a desktop shortcut, a first run
            // — and `--watch` is the request for a listener whether or not this
            // process is the one that was started with it. The second is what
            // stops it, because otherwise one `--watch` would keep a process
            // watching for copies forever and nothing in the interface could stop
            // it.
            watching.serve(activation);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the tests asked of the listener, and the handle they hand back.
    ///
    /// Per test rather than global: these run in parallel inside one binary, and
    /// a shared counter made two of them disagree about which call they were
    /// looking at.
    #[derive(Default)]
    struct Asked {
        started: usize,
    }

    /// A listener handle a test owns: a thread that ends when it is told to.
    ///
    /// The production handle owns a real listener, and starting one needs a
    /// Wayland session and a panel binary. What these tests are about is the
    /// *decision* — start, keep, stop — which is why [`Watching::ask`] takes the
    /// starting as a parameter at all.
    fn fake(asked: &mut Asked) -> Result<watch::Handle, String> {
        asked.started += 1;
        let (stop, stopped) = mpsc::channel::<bool>();
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = counter.clone();
        let thread = std::thread::spawn(move || {
            let _ = stopped.recv();
            seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });

        Ok(watch::Handle::of(stop, thread))
    }

    #[test]
    fn a_handover_that_asks_for_a_watcher_starts_one() {
        let mut asked = Asked::default();
        let watching = Watching::new(Some(PathBuf::from("/config.toml")));

        // The window this reaches was opened without `--watch`; the handover is
        // what asks for a listener, and it is the request that counts.
        let wanted = watching
            .ask(Activation::of(true), |_| fake(&mut asked))
            .expect("a listener starts");

        assert_eq!(wanted, Wanted::Listening);
        assert_eq!(asked.started, 1);
    }

    #[test]
    fn a_second_handover_that_asks_for_a_watcher_keeps_the_first() {
        let mut asked = Asked::default();
        let watching = Watching::new(Some(PathBuf::from("/config.toml")));

        watching
            .ask(Activation::of(true), |_| fake(&mut asked))
            .expect("starts");
        watching
            .ask(Activation::of(true), |_| fake(&mut asked))
            .expect("keeps");

        assert_eq!(
            asked.started, 1,
            "a second --watch is the same request, not a second listener"
        );
    }

    #[test]
    fn a_handover_that_is_not_a_watcher_stops_the_listener() {
        let mut asked = Asked::default();
        let watching = Watching::new(Some(PathBuf::from("/config.toml")));

        watching
            .ask(Activation::of(true), |_| fake(&mut asked))
            .expect("starts");
        let wanted = watching
            .ask(Activation::of(false), |_| fake(&mut asked))
            .expect("stops");

        assert_eq!(wanted, Wanted::Stopped);
        assert_eq!(
            asked.started, 1,
            "stopping is not a reason to start a second listener"
        );
    }

    #[test]
    fn a_handover_that_is_not_a_watcher_with_nothing_running_does_nothing() {
        let mut asked = Asked::default();
        let watching = Watching::new(Some(PathBuf::from("/config.toml")));

        let wanted = watching
            .ask(Activation::of(false), |_| fake(&mut asked))
            .expect("nothing to stop");

        assert_eq!(wanted, Wanted::Nothing);
        assert_eq!(asked.started, 0);
    }

    #[test]
    fn a_window_with_no_configuration_cannot_start_a_listener() {
        let mut asked = Asked::default();
        // A window whose state would not open: there is no switch to read, so
        // there is nothing to watch with.
        let watching = Watching::new(None);

        let wanted = watching
            .ask(Activation::of(true), |_| fake(&mut asked))
            .expect("nothing to start with");

        assert_eq!(wanted, Wanted::Nothing);
        assert_eq!(
            asked.started, 0,
            "there is no configuration to watch with, so nothing was started"
        );
    }

    #[test]
    fn a_listener_that_ended_on_its_own_does_not_block_the_next_one() {
        let mut asked = Asked::default();
        let watching = Watching::new(Some(PathBuf::from("/config.toml")));

        // A listener that stopped by itself, which is what turning the switch off
        // makes it do. Its handle is still in the slot, and a `--watch` after that
        // must start a listener rather than answer "already listening".
        let (stop, _stopped) = mpsc::channel::<bool>();
        let finished = std::sync::Arc::new(std::sync::Barrier::new(2));
        let reached = finished.clone();
        let thread = std::thread::spawn(move || {
            reached.wait();
        });
        finished.wait();
        *watching
            .running
            .lock()
            .expect("the listener handle is not poisoned") =
            Some(watch::Handle::of(stop, thread));

        let wanted = watching
            .ask(Activation::of(true), |_| fake(&mut asked))
            .expect("a listener starts");

        assert_eq!(wanted, Wanted::Listening);
        assert_eq!(asked.started, 1, "the dead handle was not counted as one");
    }

    #[test]
    fn closing_the_window_stops_the_listener() {
        let mut asked = Asked::default();
        let watching = Watching::new(Some(PathBuf::from("/config.toml")));

        watching
            .ask(Activation::of(true), |_| fake(&mut asked))
            .expect("starts");
        watching.shut();

        // `shut` waits for the listener's thread, so a second `shut` has nothing
        // to do — which is the observable half of "it was stopped".
        assert_eq!(watching.ask(Activation::of(false), |_| fake(&mut asked)), Ok(Wanted::Nothing));
        assert_eq!(asked.started, 1);
    }
}
