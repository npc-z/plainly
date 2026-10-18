//! The copy listener: one resident process, only while the switch is on.
//!
//! "Explain on copy" is off by default, and the default path has nothing
//! resident (spec §12). Turning it on starts this process as
//! `plainly-desktop --watch`: it holds a data-control watch on the clipboard and
//! spawns the panel the key binding would have spawned. It is the *only* thing
//! `--watch` adds.
//!
//! **The listener decides nothing.** When a copy arrives it starts
//! `plainly-panel --clipboard`, exactly as the compositor's binding does, and the
//! panel reads the clipboard again itself. The sensitive-content rule, the reuse
//! of an open panel, the Lookup Key cache and the ⟳ semantics are the panel's, so
//! the automatic trigger and the key press cannot drift apart because they are the
//! same path (spec §11, §12). The listener only answers "is this clipboard state
//! new", which is why it is a value with an [`Observer`] injected and testable
//! without a Wayland session.
//!
//! ## What ends it
//!
//! Three ways, and none of them is a signal:
//!
//! - the switch is read again on every pass, and a listener that finds it off
//!   **exits**. That makes the config the one truth about whether the feature is
//!   on, so a crash cannot resurrect it;
//! - a handover from another launch says whether that launch was a watcher, and a
//!   later `plainly-desktop` **without** `--watch` turns this one off, because
//!   "open the window" and "keep listening for copies" are different requests.
//!   (`--nowatch` is the word that request has on the wire, not a flag anybody
//!   types.)
//! - the clipboard watch itself ends: the compositor stops answering, or the
//!   session goes away. Pretending to keep listening is the one thing a listener
//!   must not do.
//!
//! ## Why a copy waits a moment
//!
//! Copying three things quickly is three selection events, and spawning three
//! panels would leave processes racing for the panel's single instance. So a copy
//! is *settled* for [`SETTLE`]: the listener waits, a new copy restarts the wait,
//! and when the clipboard has been still for one full window it starts **one**
//! panel, for the last copy.
//!
//! The window is counted in slices of [`POLL`] rather than measured on a clock,
//! which is what lets the tests reach it without spending it.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use plainly_core::ConfigFile;

use crate::clipboard::{Reading, Unreadable};

/// How long one wait lasts.
///
/// The clipboard wait blocks, and a handover that is not a watcher has to end it
/// in bounded time; this is that bound. It is not the settle window — that is
/// [`SETTLE`], and a window is as many of these as it takes.
pub const POLL: Duration = Duration::from_millis(50);

/// How long a copy is given to stop being the newest one.
///
/// Long enough to swallow the `Ctrl-C`, `Ctrl-V`, `Ctrl-C` a person does without
/// thinking; short enough that a single copy's panel is not noticeably late. The
/// panel's own start-up is 1.3–2 seconds on a cold model.
pub const SETTLE: Duration = Duration::from_millis(250);

/// The panel, and the one argument that says what to explain (spec §12).
const PANEL: &str = "plainly-panel";
const PANEL_CLIPBOARD: &str = "--clipboard";

/// A source of clipboard changes, as one question: wait a slice.
///
/// A trait rather than the Wayland watcher directly, because the decisions worth
/// testing are all *about* the stream (what counts as new, what a burst is, when
/// to stop), and none of them need a compositor.
///
/// Silence and "the watch is over" are different answers ([`Waiting::Quiet`]
/// against [`Waiting::Ended`]): one is a reason to wait again, the other a reason
/// to stop being a listener. Neither is the clipboard repeating what it already
/// held, which is no answer at all.
pub trait Observer {
    /// Wait for a change until `deadline`.
    ///
    /// The deadline is what makes a settle window measurable: with only a blocking
    /// read, a listener would wait for as long as the clipboard took.
    ///
    /// [`Waiting::Arrived`] **consumes** the change it reports. And a slice costs
    /// a slice: a reading equal to the one already in hand is the clipboard's
    /// *level* being re-reported by a fresh connection, not a change, so the wait
    /// goes on until the deadline.
    fn quiet(&mut self, deadline: Instant) -> Waiting;
}

/// What waiting until a deadline produced.
#[derive(Debug)]
pub enum Waiting {
    /// A change arrived inside the window.
    Arrived(Reading),
    /// The watch itself ended: there is nothing left to listen to.
    Ended,
    /// Nothing arrived before the deadline.
    Quiet,
}

/// What time it is, for the settle window.
///
/// Injected for the reason every clock in this workspace is: a test about a
/// 250-millisecond window should not take 250 milliseconds. It is used for one
/// thing only, the deadline handed to [`Observer::quiet`], because the window
/// itself is counted in slices.
pub trait Clock {
    fn now(&self) -> Instant;
}

/// The system clock.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// The listener: where the switch lives, how the panel is started, what a change
/// looks like, and what time it is.
pub struct Watch<'a> {
    config: PathBuf,
    spawn: Box<dyn FnMut() -> Result<(), String> + 'a>,
    observer: Box<dyn Observer + 'a>,
    clock: Box<dyn Clock + 'a>,
    /// The reading in hand: what a panel would be asked for right now.
    reading: Option<Reading>,
    /// Whether a copy has arrived that no panel has been opened for yet.
    ///
    /// This is the whole of the settle rule: **a panel opens for a copy, once,
    /// after the copying stops.** The flag is raised by a copy and lowered by the
    /// panel it opened, so the arm never opens one, a burst opens one for its last
    /// copy, the same words copied again after the panel was dismissed open
    /// another, and a clipboard that merely holds still opens nothing.
    armed: bool,

    /// How much quiet has passed, in slices of [`POLL`].
    quiet: Duration,
}

/// What became of the watch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    /// The switch was found off, and the listener stopped being one.
    SwitchOff,
    /// Another launch said it was not a watcher.
    Asked,
    /// The clipboard watch ended: the compositor stopped answering, or the
    /// session went away.
    WatchOver,
    /// The window this listener belonged to is closing, and it is going with it.
    ShutDown,
}

impl<'a> Watch<'a> {
    /// A listener over the real panel, clock and config path.
    pub fn new(
        config: impl Into<PathBuf>,
        spawn: impl FnMut() -> Result<(), String> + 'a,
        observer: impl Observer + 'a,
    ) -> Self {
        Self {
            config: config.into(),
            spawn: Box::new(spawn),
            observer: Box::new(observer),
            clock: Box::new(SystemClock),
            reading: None,
            armed: false,
            quiet: Duration::ZERO,
        }
    }

    /// The same, with the time supplied.
    ///
    /// The listener measures nothing against the clock, so a test's clock only
    /// has to answer `now()` honestly. It is a parameter so that the deadline the
    /// observer is handed can be inspected.
    pub fn with_clock(mut self, clock: impl Clock + 'a) -> Self {
        self.clock = Box::new(clock);
        self
    }

    /// Listen until something ends it.
    ///
    /// One shape, repeated:
    ///
    /// 1. nothing a panel could be asked for (the clipboard is empty, holds an
    ///    image, or the session will not give it up), so wait for a copy;
    /// 2. a reading is in hand, so wait one slice. A reading the watch has not
    ///    seen restarts the wait; a slice that comes back empty adds to the quiet,
    ///    and so does the clipboard repeating what it already held;
    /// 3. a full window of quiet starts one panel, for the last copy of the burst.
    ///
    /// **The first reading is never acted on.** The watch reports the clipboard as
    /// it already stands when it opens, and a panel for it would mean `--watch`
    /// explained whatever happened to be on the clipboard. It is still the reading
    /// every later one is compared against, which is what makes "copy the same
    /// words again" a new copy rather than a repeat.
    pub fn run(mut self, stop: &Receiver<bool>) -> Ended {
        // The arm: the clipboard as the watch finds it. It is not acted on, but it
        // is what the first copy is compared against.
        //
        // A wait rather than a read, because the session's contents are only the
        // first thing the watch reports. If the report does not land inside one
        // slice (a slow connect), the wait may come back empty, and the first
        // reading that arrives later is then the arm, which is what `None` in hand
        // means below.
        match self.observer.quiet(self.clock.now() + POLL) {
            Waiting::Arrived(reading) => self.reading = Some(reading),
            Waiting::Ended => return Ended::WatchOver,
            Waiting::Quiet => {}
        }

        loop {
            if !self.enabled() {
                return Ended::SwitchOff;
            }

            match stop.try_recv() {
                // The reason travels with the stop: a handover asked for the
                // window, or the process is going away.
                Ok(true) => return Ended::Asked,
                Ok(false) | Err(mpsc::TryRecvError::Disconnected) => return Ended::ShutDown,
                Err(mpsc::TryRecvError::Empty) => {}
            }

            // One slice. Whether anything is on the clipboard changes only what a
            // reading *means* (the arm, or a copy), never whether it is read.
            match self.observer.quiet(self.clock.now() + POLL) {
                // The clipboard changed but could not be read. Nothing is opened
                // for it, and it is *said*: a resident process that silently drops
                // copies is the one thing this must not be.
                Waiting::Arrived(Err(Unreadable::Failed(reason))) => {
                    eprintln!("{}", crate::strings::watch_read_failed(&reason));
                    self.reading = Some(Err(Unreadable::Nothing));
                    self.quiet = Duration::ZERO;
                }
                // The arm, arriving late: it is adopted, because it *is* the
                // reading a window can close on, but it arms nothing.
                Waiting::Arrived(reading) if self.reading.is_none() => {
                    self.reading = Some(reading);
                    self.quiet = Duration::ZERO;
                }
                // A copy: what a panel would be asked for. Nothing opens here; the
                // window below decides when, and by then the reading in hand is
                // the last copy of the burst.
                Waiting::Arrived(reading) => {
                    // *Is this a copy?* A reading the watch has not seen, or the
                    // same words arriving while no panel is owed for them (somebody
                    // copied them again after the last panel was dismissed). A
                    // re-announcement with a panel already owed is neither.
                    let copy = self.reading.as_ref() != Some(&reading) || !self.armed;

                    // *Does the window start over?* Only for a reading the watch
                    // has not seen. A re-announcement must **not** restart it, or
                    // it could never close: every connection reports the selection
                    // already on the clipboard, so a clipboard merely being watched
                    // would push the end of the window away forever.
                    let moved = self.reading.as_ref() != Some(&reading);

                    self.reading = Some(reading);
                    if moved {
                        self.quiet = Duration::ZERO;
                    }
                    if copy {
                        self.armed = true;
                    }
                }
                // The watch ended. What was in hand was never vouched for by a
                // pause, so nothing is opened for it.
                Waiting::Ended => return Ended::WatchOver,
                // A slice of silence. A full window of them is what says the
                // copying has stopped.
                Waiting::Quiet => {
                    self.quiet += POLL;
                    if self.quiet < SETTLE {
                        continue;
                    }

                    self.quiet = Duration::ZERO;

                    // One panel, for the last copy of the burst. A window that
                    // closes with nothing armed closed on the arm, or on a
                    // clipboard merely holding still, and neither is a copy.
                    if self.armed {
                        self.open();
                    }
                }
            }
        }
    }

    /// Start the panel, if the switch is still on and there is something to
    /// explain.
    ///
    /// The switch is read again here, because turning it off is how a person stops
    /// the feature, and a copy already in flight must not open one panel after they
    /// did.
    fn open(&mut self) {
        // Lowered whether or not a panel is actually started: a flag left up would
        // open a panel the moment the switch came back on, for a copy nobody has
        // made since.
        self.armed = false;

        if self.enabled()
            && matches!(self.reading, Some(Ok(_)))
            && let Err(error) = (self.spawn)()
        {
            eprintln!("{error}");
        }
    }

    /// Whether the switch is on right now.
    ///
    /// A configuration that cannot be read is **off**. It is the safe direction
    /// for a feature whose default is off, and it is the true one for a config
    /// file that was deleted under a running listener.
    fn enabled(&self) -> bool {
        ConfigFile::open(&self.config)
            .and_then(|file| file.config())
            .map(|config| config.app.copy_auto_popup)
            .unwrap_or(false)
    }
}

/// Start the panel, as the compositor's own binding does.
///
/// By **name**, and from `/`, for the same reasons the panel's ⤢ makes those two
/// choices for `plainly-desktop` (spec §12): the kernel's `PATH` lookup skips a
/// candidate it cannot run, and a child that inherited a deleted working directory
/// fails in a way nobody can read.
///
/// The child is reaped on a thread of its own. A child nobody waits for stays a
/// zombie for as long as the listener does, which is hours.
pub fn spawn_panel() -> Result<(), String> {
    let child = Command::new(PANEL)
        .arg(PANEL_CLIPBOARD)
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(crate::strings::watch_panel_would_not_start)?;

    reap(child);
    Ok(())
}

/// Wait for a spawned panel, out of the way of the listener.
fn reap(mut child: Child) {
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}

/// A running listener, as the window holds it.
///
/// The window does not manage the loop, it only has to be able to say one thing to
/// it: a later `plainly-desktop` that is not `--watch` means "the window, not a
/// watcher", and a listener that kept watching after that would be a resident
/// process nobody asked for. Nothing else crosses the boundary.
#[derive(Clone)]
pub struct Handle {
    /// Sends whether the stop was a handover asking for the window, or the
    /// process shutting down.
    stop: std::sync::mpsc::Sender<bool>,
    thread: std::sync::Arc<std::sync::Mutex<Option<std::thread::JoinHandle<()>>>>,
}

impl Handle {
    /// Whether the listener is still listening.
    ///
    /// A listener ends on its own when the switch is turned off, and a handle to a
    /// finished thread is not a listener any more: a window that treated it as one
    /// would report "already listening" and never start another.
    pub fn running(&self) -> bool {
        self.thread
            .lock()
            .expect("the listener handle is not poisoned")
            .as_ref()
            .is_some_and(|thread| !thread.is_finished())
    }

    /// A handle over a thread the caller owns.
    ///
    /// For the tests that are about *when* a listener is started and stopped rather
    /// than about what it does: starting a real one needs a Wayland session and a
    /// panel binary.
    pub fn of(stop: std::sync::mpsc::Sender<bool>, thread: std::thread::JoinHandle<()>) -> Self {
        Self {
            stop,
            thread: std::sync::Arc::new(std::sync::Mutex::new(Some(thread))),
        }
    }

    /// Ask the listener to stop, and wait for it to.
    ///
    /// Waiting is the point: the caller is about to keep this process alive as a
    /// window, and a listener still holding the clipboard watch would go on
    /// spawning panels behind it.
    ///
    /// `asked` says **why**, because the two endings are different facts: a
    /// handover asked for the window, or this process is going away.
    pub fn stop(&self, asked: bool) {
        let _ = self.stop.send(asked);

        let joined = self
            .thread
            .lock()
            .expect("the listener handle is not poisoned")
            .take();
        if let Some(thread) = joined {
            let _ = thread.join();
        }
    }
}

/// Start the listener on a thread of its own.
///
/// Returns as soon as the thread is up, because the caller is the window and a
/// watcher that failed to open its connection reports that from its own thread
/// rather than blocking the window on a clipboard it may never be able to read.
///
/// The session's capabilities are checked **first**, because they are the same
/// fact the settings screen greys the switch with (spec §12).
pub fn start(config: PathBuf) -> Result<Handle, String> {
    if !crate::capability::desktop().data_control() {
        return Err(crate::strings::watch_needs_data_control().to_string());
    }

    let changes = crate::clipboard::Changes::open()?;
    let (stop, stopped) = std::sync::mpsc::channel::<bool>();
    let thread = std::thread::spawn(move || {
        let ended = Watch::new(config, spawn_panel, changes).run(&stopped);
        // Said out loud, because a resident process that disappears quietly is
        // one a person cannot tell from a crash — except when the window it
        // belonged to is closing, which the person closing it already knows.
        match ended.sentence() {
            "" => {}
            sentence => eprintln!("{sentence}"),
        }
    });

    Ok(Handle {
        stop,
        thread: std::sync::Arc::new(std::sync::Mutex::new(Some(thread))),
    })
}

impl Ended {
    /// What the listener says on its way out.
    pub fn sentence(self) -> &'static str {
        match self {
            Ended::SwitchOff => crate::strings::WATCH_STOPPED_SWITCH_OFF,
            Ended::Asked => crate::strings::WATCH_STOPPED_WINDOW,
            Ended::WatchOver => crate::strings::WATCH_ENDED,
            // Silent on purpose: the window it belonged to is closing, and the
            // person who closed it does not need to be told.
            Ended::ShutDown => "",
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::collections::VecDeque;
    use std::rc::Rc;
    use std::sync::mpsc;

    use plainly_core::clipboard::PASSWORD_HINT;
    use plainly_core::{Config, DEFAULT_CONFIG};

    use crate::clipboard::{Observation, Unreadable};

    /// How many quiet slices make one settle window.
    const WINDOW: usize = (SETTLE.as_millis() / POLL.as_millis()) as usize;

    /// A configuration file with the switch in a stated position.
    ///
    /// Written through [`Config`] rather than as a TOML literal, so that a test
    /// that flips the switch cannot pass by spelling a key the application does
    /// not read.
    struct Home {
        path: PathBuf,
    }

    impl Home {
        fn new(label: &str, switch: bool) -> Self {
            let directory = std::env::temp_dir().join(format!(
                "plainly-watch-{label}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&directory);
            std::fs::create_dir_all(&directory).expect("the temporary home is creatable");

            let home = Self {
                path: directory.join("config.toml"),
            };
            home.write(switch);
            home
        }

        /// The whole file, rewritten: what `plainly config set` and the settings
        /// screen both do, minus the comment-keeping `ConfigFile` handle.
        fn write(&self, switch: bool) {
            let mut config = Config::parse(DEFAULT_CONFIG).expect("the shipped default parses");
            config.app.copy_auto_popup = switch;
            std::fs::write(
                &self.path,
                config.to_toml().expect("the configuration renders as TOML"),
            )
            .expect("the configuration file is writable");
        }

        fn remove(&self) {
            std::fs::remove_file(&self.path).expect("the configuration file is there to remove");
        }

        fn path(&self) -> PathBuf {
            self.path.clone()
        }
    }

    impl Drop for Home {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.path.parent().expect("it has a parent"));
        }
    }

    /// A reading of `text`, published with `types`.
    fn reading(text: &str, types: &[&str]) -> Reading {
        Ok(Observation {
            passage: text.to_string(),
            types: types.iter().map(|mime| mime.to_string()).collect(),
        })
    }

    /// What a clipboard looks like with nothing to explain.
    fn nothing() -> Reading {
        Err(Unreadable::Nothing)
    }

    /// A clipboard as a timeline, one step at a time.
    ///
    /// Three steps, and they are the three things a real watch can do:
    ///
    /// - [`Step::Copy`]: a selection event, so the listener hears about it the
    ///   moment it asks;
    /// - [`Step::Still`]: one slice the clipboard went through without changing;
    ///   the watch produces *nothing* here, which is what the listener measures
    ///   the settle window with;
    /// - the end of the timeline: the watch is over, which is how a test reaches
    ///   [`Ended::WatchOver`] on purpose.
    ///
    /// A literal list rather than a queue plus counters, because "holding still" is
    /// neither a queued empty reading (an empty clipboard is a selection event that
    /// clears what was there) nor a queue that has run out (that is the watch
    /// ending).
    ///
    /// A step is one report, and the fixture does **not** insert the level reports
    /// a real watch makes: `what_was_already_on_the_clipboard_is_not_a_copy`
    /// writes them into its own timeline, which is where the rule that decides them
    /// (a re-report must not restart the settle window) is pinned.
    struct Script {
        current: Option<Reading>,
        steps: VecDeque<Step>,
    }

    #[derive(Clone)]
    enum Step {
        Copy(Reading),
        Still,
    }

    impl Script {
        /// A clipboard already holding `reading`, and nothing after it.
        ///
        /// The listener's first wait is what reports this, so a script **must**
        /// start here: a timeline with nothing to report first is a timeline the
        /// listener never gets an arm from, and the arm is what every later
        /// reading is compared against.
        fn holding(reading: Reading) -> Self {
            Self {
                current: Some(reading.clone()),
                steps: VecDeque::from([Step::Copy(reading)]),
            }
        }

        /// The clipboard holds the same reading for a whole window, then that
        /// window closes and a panel opens for it.
        ///
        /// This is what "the copying stopped" looks like: one full settle window
        /// with nothing happening in it.
        fn settles(mut self) -> Self {
            self.steps.extend(std::iter::repeat_n(Step::Still, WINDOW));
            self
        }

        /// A copy happens now.
        fn copies(mut self, reading: Reading) -> Self {
            self.steps.push_back(Step::Copy(reading));
            self
        }

    }

    impl Observer for Script {
        fn quiet(&mut self, _deadline: Instant) -> Waiting {
            match self.steps.pop_front() {
                // A reading is reported and consumed: consuming it is what keeps
                // the next slice a slice of the future rather than the same
                // reading counted twice.
                Some(Step::Copy(reading)) => {
                    self.current = Some(reading.clone());
                    Waiting::Arrived(reading)
                }
                Some(Step::Still) => Waiting::Quiet,
                None => Waiting::Ended,
            }
        }
    }

    // The clock the tests use is the real one ([`SystemClock`]), because the
    // listener measures nothing against it: the settle window is counted in
    // slices, and the only instant it asks for is one slice's deadline, which the
    // fixture's [`Observer::quiet`] ignores.

    /// A running total of what the listener asked for.
    #[derive(Clone, Default)]
    struct Spawns(Rc<Cell<usize>>);

    impl Spawns {
        fn counter(&self) -> impl FnMut() -> Result<(), String> + 'static {
            let count = self.0.clone();
            move || {
                count.set(count.get() + 1);
                Ok(())
            }
        }

        fn count(&self) -> usize {
            self.0.get()
        }
    }

    /// A listener over `home`, with the script supplied.
    fn listener<'a>(home: &Home, script: Script, spawns: &Spawns) -> Watch<'a> {
        Watch::new(home.path(), spawns.counter(), script).with_clock(SystemClock)
    }

    #[test]
    fn one_copy_opens_one_panel() {
        let home = Home::new("one-copy", true);
        let spawns = Spawns::default();
        let (_stop, stopped) = mpsc::channel::<bool>();

        // The arm is on the clipboard already; then somebody presses Ctrl-C.
        let ended = listener(
            &home,
            Script::holding(reading("The report stops short.", &["text/plain"]))
                .settles()
                .copies(reading("He bottled it.", &["text/plain"]))
                .settles(),
            &spawns,
        )
        .run(&stopped);

        assert_eq!(spawns.count(), 1, "one copy is one panel");
        assert_eq!(ended, Ended::WatchOver);
    }

    #[test]
    fn what_was_already_on_the_clipboard_is_not_a_copy() {
        let home = Home::new("arm", true);
        let spawns = Spawns::default();
        let (_stop, stopped) = mpsc::channel::<bool>();
        let same = reading("The report stops short.", &["text/plain"]);

        // The watch opens with these words on the clipboard and holds still for a
        // window: that is the **arm**, nothing is armed for it, and no panel opens.
        //
        // Then something is copied and reported again and again with no window
        // closing between the reports, which is what a real watch produces: every
        // data-control connection reports the selection already on the clipboard
        // before it waits for anything.
        //
        // **One panel**, and that is a decision rather than an accident. Counting
        // the re-reports as copies would mean a window that could never close while
        // the clipboard sat still, and a panel would never open at all. A re-report
        // is not distinguishable from a real copy of the same words, so "one panel"
        // is the only rule that works. The alternative spelling, where the same
        // words after the panel was dismissed *are* a copy, is the test below.
        let ended = listener(
            &home,
            Script::holding(same.clone())
                .settles()
                .copies(reading("He bottled it.", &["text/plain"]))
                .copies(reading("He bottled it.", &["text/plain"]))
                .copies(reading("He bottled it.", &["text/plain"]))
                .copies(reading("He bottled it.", &["text/plain"]))
                .copies(reading("He bottled it.", &["text/plain"]))
                .settles(),
            &spawns,
        )
        .run(&stopped);

        assert_eq!(
            spawns.count(),
            1,
            "the arm opens nothing, and one copy opens one panel however often it is re-reported"
        );
        assert_eq!(ended, Ended::WatchOver);
    }

    #[test]
    fn content_copied_after_a_clear_is_a_new_copy() {
        let home = Home::new("after-clear", true);
        let spawns = Spawns::default();
        let (_stop, stopped) = mpsc::channel::<bool>();
        let same = reading("The report stops short.", &["text/plain"]);

        // The clipboard was emptied, then the same words were copied again: from
        // the person's side that is a copy, and the panel's cache is what makes
        // it cheap.
        let ended = listener(
            &home,
            Script::holding(same.clone())
                .settles()
                .copies(nothing())
                .copies(same)
                .settles(),
            &spawns,
        )
        .run(&stopped);

        assert_eq!(
            spawns.count(),
            1,
            "the clear opens nothing, and the words copied after it are one copy"
        );
        assert_eq!(ended, Ended::WatchOver);
    }

    #[test]
    fn the_same_words_published_with_other_types_are_another_copy() {
        let home = Home::new("same-words", true);
        let spawns = Spawns::default();
        let (_stop, stopped) = mpsc::channel::<bool>();

        let ended = listener(
            &home,
            Script::holding(reading("He bottled it.", &["text/plain"]))
                .settles()
                .copies(reading("He bottled it.", &["UTF8_STRING", "text/html"]))
                .settles(),
            &spawns,
        )
        .run(&stopped);

        assert_eq!(
            spawns.count(),
            1,
            "the arm opens nothing, and the other offer of the same words is one copy"
        );
        assert_eq!(ended, Ended::WatchOver);
    }

    #[test]
    fn a_cleared_clipboard_and_an_image_open_nothing() {
        let home = Home::new("nothing", true);
        let spawns = Spawns::default();
        let (_stop, stopped) = mpsc::channel::<bool>();

        let ended = listener(
            &home,
            // The first copy settles into a panel for the words before it. Then
            // a clear, then content whose owner published the secret marker — the
            // marker is not the listener's business: what it says about marked
            // content is the panel's own refusal, and the criterion lives in core
            // (spec §11) — then an image, which has no text to explain at all.
            Script::holding(reading("before", &["text/plain"]))
                .copies(reading("first", &["text/plain"]))
                .settles()
                .copies(nothing())
                .copies(reading("hunter2", &[PASSWORD_HINT, "text/plain"]))
                .settles()
                .copies(nothing())
                .settles(),
            &spawns,
        )
        .run(&stopped);

        assert_eq!(
            spawns.count(),
            2,
            "the clear and the image open nothing; the two readable copies open one each"
        );
        assert_eq!(ended, Ended::WatchOver);
    }

    #[test]
    fn a_window_that_was_one_slice_short_opens_nothing() {
        let home = Home::new("short-window", true);
        let spawns = Spawns::default();
        let (_stop, stopped) = mpsc::channel::<bool>();

        // The same shape as `one_copy_opens_one_panel`, with one slice less of
        // silence before the watch ends. A window is what opens a panel, and a
        // window is counted in slices: one short is not a window.
        let mut short = Script::holding(reading("before", &["text/plain"]))
            .copies(reading("He bottled it.", &["text/plain"]));
        for _ in 0..WINDOW - 1 {
            short.steps.push_back(Step::Still);
        }

        let ended = listener(&home, short, &spawns).run(&stopped);

        assert_eq!(
            spawns.count(),
            0,
            "{WINDOW} slices make a window and {} do not",
            WINDOW - 1
        );
        assert_eq!(ended, Ended::WatchOver);
    }

    #[test]
    fn a_burst_of_copies_opens_one_panel_for_the_last_of_them() {
        let home = Home::new("burst", true);
        let spawns = Spawns::default();
        let (_stop, stopped) = mpsc::channel::<bool>();

        // One reading to arm on, then three copies with no silence between them:
        // one settle window for all three. A panel is the answer to the last, and
        // the two before it do not each open one — waiting for the burst to end
        // is what makes three copies one panel, and the last copy is the one the
        // person is looking at.
        let ended = listener(
            &home,
            Script::holding(reading("before", &["text/plain"]))
                .settles()
                .copies(reading("one", &["text/plain"]))
                .copies(reading("two", &["text/plain"]))
                .copies(reading("three", &["text/plain"]))
                .settles(),
            &spawns,
        )
        .run(&stopped);

        assert_eq!(
            spawns.count(),
            1,
            "a burst is one panel, not one per selection change"
        );
        assert_eq!(ended, Ended::WatchOver);
    }

    #[test]
    fn a_copy_after_a_settled_panel_is_another_copy() {
        let home = Home::new("two-copies", true);
        let spawns = Spawns::default();
        let (_stop, stopped) = mpsc::channel::<bool>();

        // Two copies with a whole window of silence between them: the person
        // read the panel, then copied something else. Two panels — the second
        // `activate` reuses the first window, which is the panel's own behaviour
        // and not this listener's.
        let ended = listener(
            &home,
            Script::holding(reading("before", &["text/plain"]))
                .settles()
                .copies(reading("first", &["text/plain"]))
                .settles()
                .copies(reading("second", &["text/plain"]))
                .settles(),
            &spawns,
        )
        .run(&stopped);

        assert_eq!(
            spawns.count(),
            2,
            "two copies with a quiet window between them are two panels"
        );
        assert_eq!(ended, Ended::WatchOver);
    }

    #[test]
    fn the_same_words_after_the_panel_is_dismissed_are_another_copy() {
        let home = Home::new("again", true);
        let spawns = Spawns::default();
        let (_stop, stopped) = mpsc::channel::<bool>();
        let same = reading("The report stops short.", &["text/plain"]);

        // A copy settles into a panel; it is dismissed; the same words are copied
        // again. From the person's side that is a copy, and the honest answer is a
        // panel — the panel's cache is what makes it cost nothing.
        let ended = listener(
            &home,
            Script::holding(reading("arm", &["text/plain"]))
                .copies(same.clone())
                .settles()
                .copies(same)
                .settles(),
            &spawns,
        )
        .run(&stopped);

        assert_eq!(
            spawns.count(),
            2,
            "copying the same words again after the panel was dismissed is another copy"
        );
        assert_eq!(ended, Ended::WatchOver);
    }

    #[test]
    fn the_switch_being_turned_off_ends_the_listener_before_any_panel() {
        let home = Home::new("turned-off", true);
        let spawns = Spawns::default();
        let (_stop, stopped) = mpsc::channel::<bool>();
        home.write(false);

        let ended = listener(
            &home,
            Script::holding(reading("The report stops short.", &["text/plain"]))
                .copies(reading("Something else entirely.", &["text/plain"]))
                .settles(),
            &spawns,
        )
        .run(&stopped);

        assert_eq!(
            spawns.count(),
            0,
            "a listener that finds the switch off is not a listener"
        );
        assert_eq!(ended, Ended::SwitchOff);
    }

    #[test]
    fn a_configuration_that_disappears_ends_the_listener() {
        let home = Home::new("gone", true);
        let spawns = Spawns::default();
        let (_stop, stopped) = mpsc::channel::<bool>();
        home.remove();

        let ended = listener(
            &home,
            Script::holding(reading("one", &["text/plain"]))
                .copies(reading("two", &["text/plain"]))
                .settles(),
            &spawns,
        )
        .run(&stopped);

        assert_eq!(
            spawns.count(),
            0,
            "a switch nobody can read is not a switch that is on"
        );
        assert_eq!(ended, Ended::SwitchOff);
    }

    #[test]
    fn a_window_asked_for_without_a_watcher_ends_the_listener() {
        let home = Home::new("asked", true);
        let spawns = Spawns::default();
        let (stop, stopped) = mpsc::channel::<bool>();

        // What the activation thread does when a later launch that is not a
        // watcher arrives: one message, while the listener is inside a settle
        // window. The listener only looks at that channel between slices, so the
        // message waits for one — which is the bound this is about.
        let tell = stop.clone();
        let asking = std::thread::spawn(move || {
            let _ = tell.send(true);
        });

        // A clipboard that holds still for window after window, so the listener
        // is inside a window when the ask arrives rather than racing to the end
        // of the script.
        let ended = listener(
            &home,
            Script::holding(reading("before", &["text/plain"]))
                .copies(reading("The report stops short.", &["text/plain"]))
                .settles(),
            &spawns,
        )
        .run(&stopped);
        asking.join().expect("the asking thread does not panic");

        assert_eq!(
            spawns.count(),
            0,
            "the panel the wait was about is not opened: the window was asked for instead"
        );
        assert_eq!(ended, Ended::Asked);
    }

    #[test]
    fn a_watch_that_ends_is_not_an_unreadable_clipboard() {
        let home = Home::new("over", true);
        let spawns = Spawns::default();
        let (_stop, stopped) = mpsc::channel::<bool>();

        // Nothing on the clipboard and nothing coming: the watch is over before
        // it ever reports a reading, and that is not the same thing as a
        // clipboard with nothing to explain.
        let ended = listener(
            &home,
            Script::holding(reading("arm", &["text/plain"])),
            &spawns,
        )
        .run(&stopped);

        assert_eq!(spawns.count(), 0);
        assert_eq!(ended, Ended::WatchOver);
    }
}
