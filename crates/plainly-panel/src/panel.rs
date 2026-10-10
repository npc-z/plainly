//! The panel window: what a trigger puts on the screen, and what its actions do.
//!
//! One window, reused: `activate` re-reads the clipboard and refreshes what is
//! inside it rather than making a second panel (spec §12). The window is a layer
//! surface that never takes focus, or — where the session has no data-control,
//! which is also where it has no layer shell — a plain window that does.
//!
//! This file owns **widgets and the run**, and nothing else. What the panel is
//! showing is [`crate::state`], a value with unit tests; where it goes is
//! [`crate::position`]; what it reads out of the configuration is
//! [`crate::prefs`]. The layout is what is left, and the two properties the spec
//! cares about are properties of it rather than of each state:
//!
//! - **the title bar is always there**, above every state, carrying provenance,
//!   复制, ⟳, ⤢ and ✕ — so a refusal or an error has the same way out as a
//!   finished Explanation (spec §12);
//! - **the body is replaceable**, so loading, a stack of Passages, a sentence and
//!   a refusal are all "what is in the body now" rather than four screens.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gdk, glib};
use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use plainly_core::clipboard::Clipboard;
use plainly_core::desktop::Presentation;
use plainly_core::render::SectionKind;

use crate::position::Anchors;
use crate::prefs::Prefs;
use crate::state::{self, Outcome, Showing, Trigger};
use crate::strings;
use crate::watchdog::{self, RenderWatchdog};
use crate::worker::{self, Event, Recall};
use crate::{clipboard, css, document, open, plan, position};

/// The panel's width (spec §12: about 470px).
const WIDTH: i32 = 470;
/// The gap between a layer surface and the screen edge.
const MARGIN: i32 = 12;
/// How tall the panel is before its first allocation: the title bar and its
/// separator. Only the fallback window's first placement uses it, and any positive
/// number does — what matters is that a bottom corner is measured from the
/// window's bottom edge rather than from a height nobody chose.
const HEADER_HEIGHT: i32 = 44;

/// How much of a screen's height the panel takes before its body scrolls.
///
/// A third, so the panel never becomes a full-screen overlay: it is a note beside
/// what a person is reading, and a note that covers everything is the
/// interruption the panel exists to avoid.
const HEIGHT_SHARE: i32 = 3;

/// How the panel reads the clipboard, which is the same question as its shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reader {
    /// A layer surface that never has focus: through data-control.
    DataControl,
    /// A focused window: through GTK, which works there.
    Gtk,
}

/// What set a trigger off.
///
/// A value, not the recall it resolves to: the recall for a ⟳ depends on the
/// configuration read at the **start** of that trigger (a panel is reused, so the
/// file may have changed since the last press), and resolving it any earlier is
/// how the cloud move ended up chosen from the previous trigger's provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Press {
    /// A key press: re-read the clipboard, reuse what the store holds.
    Refresh,
    /// ⟳: re-read the clipboard and go past the cache (spec §8).
    ///
    /// It reads the clipboard again rather than re-running what is on screen:
    /// "generate this again" means the Passage the person last copied, and
    /// regenerating the previous run's Passage would answer a question they have
    /// moved on from.
    ///
    /// On a locally hosted model it also changes provider, when there is a cloud
    /// one it can change to — the note under the content tells the person to press
    /// this button for exactly that, so a button that re-ran the same local model
    /// would make the sentence a lie (spec §12). Which provider that is is decided
    /// from the configuration read at the start of the trigger.
    Regenerate,
}

/// What an action button does.
///
/// A value rather than a closure per button: which button was pressed is then a
/// question the panel answers in one place, and the wiring cannot drift between
/// two buttons that do the same thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    /// Copy the Explanation the panel is showing.
    Copy,
    /// Generate again, past the cache — on the cloud provider when there is one.
    Regenerate,
    /// Open the main window, on the history screen.
    OpenWindow,
    /// Close the panel: the one path out that always exists.
    Quit,
}

/// The widgets one future is allowed to touch, and nothing else.
///
/// Cloned per future rather than captured as the whole `Rc<Panel>`: a future
/// holds its captures across `await`s, and a `Panel` held there would keep a
/// borrow of the body state alive for as long as the run takes.
#[derive(Clone)]
struct Surface {
    generation: Rc<Cell<u64>>,
    trigger: Rc<RefCell<Trigger>>,
    body: gtk::Box,
    provenance: gtk::Label,
    /// The status line: what an action just did — `已复制`, `复制失败`, or the
    /// sentence `open.rs` gives when ⤢ cannot start the window — because a layer
    /// surface has nowhere to put a banner.
    ///
    /// Separate from the 复制 button's own label, which is a control rather than a
    /// place to report: a button that renamed itself after a failed ⤢ would be a
    /// panel with no copy control. Not the *chip* either — the chip is the
    /// provenance label beside it.
    status: gtk::Label,
    copy: gtk::Button,
    /// The fixed note under the content, shown when a local model is answering.
    note: gtk::Label,
}

/// The panel: one window, one trigger at a time.
pub struct Panel {
    window: gtk::ApplicationWindow,
    /// The body's scroller, held so a trigger can re-apply the placement.
    ///
    /// The placement depends on the configuration (a corner, an output) *and* on
    /// the body's height, and both change while the panel is up: a corner changed
    /// in the settings screen has to be in effect on the next key press, not at the
    /// next monitor event.
    scroller: Rc<gtk::ScrolledWindow>,
    /// The application, held rather than asked for later.
    ///
    /// Every exit path goes through `g_application_quit()`, and there are three of
    /// them — ✕, the auto-dismiss timer, the render watchdog. `window.application()`
    /// is not a substitute: it is the window's property, and a timer that read it
    /// and found `None` would do nothing at all and leave a panel that never
    /// disappears (spec §12).
    app: gtk::Application,
    reader: Reader,
    surface: Surface,
    /// The 行为 keys and the provider facts, as the file held them when the
    /// **current trigger** started.
    ///
    /// This cell is the live one — the position, the note and the auto-dismiss
    /// timer read it while a trigger is in flight, and `reread_prefs` refills it for
    /// the next trigger. The trigger's *own* copy lives on [`Trigger::prefs`], which
    /// is what the body and 复制 render from: those two must agree, so they read one
    /// value, and this cell changing under them (a monitor event, somebody editing
    /// the file) cannot make the screen and the clipboard disagree.
    ///
    /// Re-read at the start of every trigger, not once at startup: the panel
    /// outlives a key press (the second press reuses it, spec §12), so a corner or
    /// a duration changed in the main window has to reach the next press rather
    /// than the next login.
    prefs: Rc<RefCell<Prefs>>,
    /// Whether a trigger is waiting for the focus the GTK reader needs.
    waiting: Cell<bool>,
    /// The trigger that is waiting for the focus the GTK reader needs.
    ///
    /// The whole [`Recall`], not a flag: ⟳ on a local model is a move to a named
    /// cloud provider, and a `bool` here would drop the name and quietly re-run
    /// the local model when the focus finally arrives — the one lie ⟳ exists to
    /// avoid.
    pending: RefCell<Option<Recall>>,
    /// The pending auto-dismiss, if any.
    dismiss: RefCell<Option<glib::SourceId>>,
}

impl Panel {
    /// Make the panel, show it, and show what the clipboard holds.
    pub fn new(app: &gtk::Application, presentation: Presentation) -> Rc<Self> {
        css::install();

        let window = gtk::ApplicationWindow::new(app);
        window.set_title(strings::TITLE);
        window.set_decorated(false);
        window.set_default_size(WIDTH, -1);
        window.set_size_request(WIDTH, -1);

        let reader = present(&window, presentation);
        // The position, the timer, the section switches and the provider facts
        // are four questions about one document, so they travel together as one
        // value; `Panel::start` replaces it from the file at every trigger.
        let prefs = Rc::new(RefCell::new(Prefs::read()));
        let body = gtk::Box::new(gtk::Orientation::Vertical, 9);
        let scroller = Rc::new(
            gtk::ScrolledWindow::builder()
                .hscrollbar_policy(gtk::PolicyType::Never)
                .child(&body)
                .build(),
        );

        let panel = Rc::new(Self {
            window,
            scroller: Rc::clone(&scroller),
            app: app.clone(),
            reader,
            surface: Surface {
                generation: Rc::new(Cell::new(0)),
                trigger: Rc::new(RefCell::new(Trigger::loading(
                    Recall::Reuse,
                    Prefs::default(),
                ))),
                body,
                provenance: provenance_label(strings::PROVENANCE_PENDING),
                status: status_label(),
                copy: action_button(strings::COPY, strings::COPY_LABEL),
                // The text is replaced per trigger; this is only a first value.
                note: note_label(strings::LOCAL_NOTE),
            },
            prefs,
            waiting: Cell::new(false),
            pending: RefCell::new(None),
            dismiss: RefCell::new(None),
        });

        // The one fixed line under the content, for a model on this machine
        // (spec §12): below the scrolling body rather than inside it, so the
        // warning stays in front of a person while they scroll the Passage.
        panel.surface.note.set_visible(false);

        let header = panel.header(app);
        let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
        root.pack_start(&header, false, false, 0);
        root.pack_start(
            &gtk::Separator::new(gtk::Orientation::Horizontal),
            false,
            false,
            0,
        );
        root.pack_start(&*scroller, true, true, 0);
        root.pack_start(&panel.surface.note, false, false, 0);
        panel.window.add(&root);

        // The window's own shape, which both presentations share: a corner and a
        // screen, the body's height cap, and the note that hangs off the provider
        // in effect. One code path, shared with every later trigger.
        panel.apply_layout();
        panel.watch_monitors();

        // And again on every map, for both shapes.
        //
        // Because GTK3's toplevel only becomes a layer surface when it is
        // realized — which happens when it is shown — so anchors set before
        // `show_all` are set on a widget that has no layer surface yet, and the
        // panel then maps in the compositor's default corner and stays there. That
        // was measured on this machine's niri (the panel ignored all four corner
        // settings until the anchors were re-applied here). The fallback window
        // needs it for its own reason: `move_` on an unmapped window has no
        // position to keep.
        let mapped = Rc::clone(&panel);
        panel
            .window
            .connect_map(move |_| Panel::apply_layout(&mapped));
        panel.watch_first_frame(app);

        // A focused window is what makes GTK's clipboard work here, so this
        // path's read waits for the focus the window was made to take.
        if panel.reader == Reader::Gtk {
            let focused = Rc::clone(&panel);
            panel.window.connect_focus_in_event(move |_, _| {
                if focused.waiting.replace(false) {
                    let recall = focused.pending.borrow_mut().take().unwrap_or(Recall::Reuse);
                    Panel::read(&focused, recall);
                }
                glib::Propagation::Proceed
            });
        }

        Surface::message(&panel.surface, strings::READING);
        panel.window.show_all();
        Panel::refresh(&panel);
        panel
    }

    /// The title bar: the chip, and the four controls the spec fixes (spec §12).
    ///
    /// Every control here works in every state, which is the point: a refusal or
    /// an error has the same ✕ as a finished Explanation, so "a panel that cannot
    /// be closed" is not a state this layout can produce.
    fn header(self: &Rc<Self>, app: &gtk::Application) -> gtk::Box {
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);

        let title = gtk::Label::new(Some(strings::TITLE));
        title.set_xalign(0.0);
        header.pack_start(&title, false, false, 0);
        header.pack_start(&self.surface.provenance, true, true, 0);
        header.pack_start(&self.surface.status, false, false, 0);

        // Packed from the right, so the visual order is the reverse of this one:
        // ✕ rightmost, then ⤢, then ⟳, then 复制.
        let close = action_button(strings::CLOSE, strings::CLOSE_LABEL);
        Panel::wire(self, close.clone(), Action::Quit, app);
        header.pack_end(&close, false, false, 0);

        let open = action_button(strings::OPEN_WINDOW, strings::OPEN_WINDOW_LABEL);
        Panel::wire(self, open.clone(), Action::OpenWindow, app);
        header.pack_end(&open, false, false, 0);

        let regenerate = action_button(strings::REGENERATE, strings::REGENERATE_LABEL);
        Panel::wire(self, regenerate.clone(), Action::Regenerate, app);
        header.pack_end(&regenerate, false, false, 0);

        Panel::wire(self, self.surface.copy.clone(), Action::Copy, app);
        header.pack_end(&self.surface.copy, false, false, 0);

        header
    }

    /// Wire one button to one action against this panel.
    ///
    /// The button travels by value because GTK takes over its signal; the caller
    /// keeps its own clone when it needs to keep the widget — the title bar holds
    /// the 复制 button and the status line that reports what it did, and they are
    /// deliberately two different widgets.
    fn wire(self: &Rc<Self>, button: gtk::Button, action: Action, app: &gtk::Application) {
        let panel = Rc::clone(self);
        let quit = app.clone();
        button.connect_clicked(move |_| Panel::perform(&panel, &action, &quit));
    }

    /// Do what a button says.
    fn perform(self: &Rc<Self>, action: &Action, app: &gtk::Application) {
        match action {
            Action::Copy => Panel::copy_text(self),
            Action::Regenerate => Panel::start(self, Press::Regenerate),
            Action::OpenWindow => Panel::open_window(self),
            // `g_application_quit()`, never `gtk_main_quit()`: the latter does not
            // stop a `GApplication`'s main loop, which is how a panel stayed up
            // for 23 minutes once (spec §12).
            Action::Quit => app.quit(),
        }
    }

    /// A trigger: show what the clipboard holds now.
    ///
    /// The second press is this same call: the person who pressed it has almost
    /// certainly copied something new, and if they did not, the Lookup Key finds
    /// what is already stored (spec §12).
    ///
    /// Every trigger starts by re-reading the configuration, which is the one
    /// thing that makes the settings screen's promises about the panel true: a
    /// corner, a duration, a section switch or a provider changed while the panel
    /// is up is in effect on the next press.
    pub fn refresh(&self) {
        Panel::start(self, Press::Refresh);
    }

    /// Read the configuration again and put what it describes into effect.
    ///
    /// **Every trigger starts here**, and the read is the point: the panel is
    /// reused, so the next key press has to see what the settings screen has
    /// written since — the ⟳ decision, the note under the content, the corner, the
    /// output and the height cap all hang off this one document (spec §12). Reading
    /// it anywhere later is how the cloud move once ended up chosen from the
    /// *previous* trigger's provider.
    fn reread_prefs(&self) {
        *self.prefs.borrow_mut() = Prefs::read();
        // The note is the button's instructions, so it is re-applied with the
        // preferences it describes: a provider switched while the panel is up
        // changes what the line says on the next press. The placement goes the same
        // way — a corner changed in the settings screen is in effect on the next
        // press rather than at the next monitor event.
        self.apply_layout();
    }

    /// Start a run against the clipboard, on freshly read preferences.
    fn start(&self, press: Press) {
        self.reread_prefs();
        // Armed here rather than in `read`, because `read` may be waiting for the
        // focus the GTK reader needs: a window the compositor never focuses would
        // otherwise never arm auto-dismiss at all, and a panel with no timer and no
        // focus is a panel with only ✕ (spec §12).
        Panel::reset_dismiss(self);

        let recall = match press {
            Press::Refresh => Recall::Reuse,
            Press::Regenerate => self.prefs.borrow().regenerate_recall(),
        };

        match self.reader {
            // A layer surface never has focus, and data-control does not need it:
            // the read can happen straight away.
            Reader::DataControl => Panel::read(self, recall),
            // This path's window is the one that takes focus, and the keybinding
            // press does not give it any: ask for it, and read when it arrives.
            Reader::Gtk => {
                self.window.present();
                if self.window.is_active() {
                    Panel::read(self, recall);
                } else {
                    // The recall travels with the wait: a ⟳ on a window that has
                    // not been given focus yet must not become a cache hit when
                    // the focus arrives, and a cloud move must not lose the name
                    // of the provider it was going to.
                    *self.pending.borrow_mut() = Some(recall);
                    self.waiting.set(true);
                }
            }
        }
    }

    /// The current state, briefly borrowed.
    ///
    /// A copy rather than a guard: everything the caller does with it goes on to
    /// borrow a widget or another field, and a guard held across that is how two
    /// `RefCell`s end up borrowed at once.
    fn trigger(&self) -> Trigger {
        self.surface.trigger.borrow().clone()
    }

    /// Copy what the panel is showing.
    ///
    /// What happened is said in the status line: a layer surface has no room for a
    /// banner, and a "copied" that copied nothing is worse than an error
    /// (tickets/15).
    fn copy_text(&self) {
        let Some(text) = self.trigger().copy_text() else {
            return;
        };

        // The answer may arrive from another thread — the data-control path
        // prepares the selection there — and a new trigger clears the status line
        // as it starts. So the write is guarded the way every other late writer in
        // this panel is: if the generation has moved, this answer belongs to
        // content the person has already replaced.
        let generation = self.surface.generation.get();
        let surface = self.surface.clone();
        clipboard::copy(
            &text,
            self.reader == Reader::DataControl,
            move |published| {
                if !surface.is_current(generation) {
                    return;
                }
                surface.status.set_text(match published {
                    true => strings::COPIED,
                    false => strings::COPY_FAILED,
                });
            },
        );
    }

    /// Open the main window, on the history screen.
    ///
    /// A failure is said in the status line, with the reason: "找不到 plainly-desktop"
    /// is the whole of what a person can act on, and a generic "did not work"
    /// would leave them without the name of the program that is missing.
    fn open_window(&self) {
        // Synchronous: the spawn either starts a process or does not, and there is
        // no later answer to guard against — unlike the copy, whose failure can
        // arrive from the thread that prepares the selection.
        if let Err(reason) = open::main_window() {
            self.surface.status.set_text(&reason);
        }
    }

    /// Read the clipboard and put the outcome on the screen.
    ///
    /// The data-control read happens on its own thread: the transfer waits for
    /// the clipboard's owner to write, and an owner that stalls must not take the
    /// main loop down with it — ✕ and the watchdog both live there.
    fn read(&self, recall: Recall) {
        let prefs = self.prefs.borrow().clone();
        *self.surface.trigger.borrow_mut() = Trigger::loading(recall.clone(), prefs);
        Surface::message(&self.surface, strings::READING);
        // The status line is where the last trigger's 已复制 / 复制失败 / ⤢ 的报错
        // lived; a new trigger is a new answer, so it starts empty.
        self.surface.status.set_text("");
        // Nothing has come out yet, so there is nothing to copy: a button that
        // looks live and does nothing reads as broken (spec §12's states all have
        // to be honest about what they offer).
        self.surface.copy.set_sensitive(false);
        // The chip says what the panel is showing, so it is reset with the body:
        // a second trigger that left the previous provider up would attribute the
        // new Passage's Explanation to whoever answered the old one, and a refusal
        // or a read error — which never reaches a run — would leave it up for good.
        self.surface
            .provenance
            .set_text(&self.surface.trigger.borrow().showing.chip());

        let generation = self.surface.generation.get() + 1;
        self.surface.generation.set(generation);

        let (sender, receiver) = async_channel::unbounded::<Result<Clipboard, clipboard::Error>>();
        match self.reader {
            Reader::DataControl => {
                std::thread::spawn(move || {
                    let _ = sender.send_blocking(clipboard::data_control());
                });
            }
            // GTK's clipboard is main-thread work. The read blocks this thread for
            // as long as the owner takes, which is the one hole the watchdog
            // cannot cover; the fallback path is a focused window on a session
            // where that transfer is a local one (spec §12).
            Reader::Gtk => {
                // A display that has gone away is a state to show, not a panic to
                // die of: the panel is a layer surface with ✕, a watchdog and a
                // timer, and a panic on the main loop takes all three with it.
                // Every other missing-display path in this crate (`css::install`,
                // `Panel::geometry`) answers by carrying on without it; this one
                // answers in the panel's own words, because it is the path a read
                // was supposed to come back on.
                let result = match gdk::Display::default() {
                    Some(display) => clipboard::focused_window(&gtk::Clipboard::for_display(
                        &display,
                        &gdk::SELECTION_CLIPBOARD,
                    )),
                    None => Err(clipboard::Error::Failed(strings::NO_DISPLAY.to_string())),
                };
                let _ = sender.send_blocking(result);
            }
        }

        let surface = self.surface.clone();
        glib::spawn_future_local(async move {
            let Ok(clipboard) = receiver.recv().await else {
                return;
            };
            // A later press owns the panel; an earlier read is stale.
            if !surface.is_current(generation) {
                return;
            }

            let passages = match clipboard {
                // An empty clipboard is not an error state, and it reads in the
                // panel's own language; anything else is, and says so in it too.
                Err(clipboard::Error::NoPassage(_)) => {
                    Surface::show(&surface, Showing::Nothing);
                    return;
                }
                Err(error) => {
                    Surface::show(&surface, Showing::Unreadable(error.to_string()));
                    return;
                }
                Ok(clipboard) => match plan::plan(clipboard) {
                    plan::Plan::Sensitive => {
                        Surface::show(&surface, Showing::Sensitive);
                        return;
                    }
                    plan::Plan::Unread => {
                        Surface::show(&surface, Showing::Unread);
                        return;
                    }
                    plan::Plan::Nothing => {
                        Surface::show(&surface, Showing::Nothing);
                        return;
                    }
                    plan::Plan::TooLong(too_many) => {
                        Surface::show(&surface, Showing::TooLong(too_many));
                        return;
                    }
                    plan::Plan::Passages(passages) => passages,
                },
            };

            Surface::stack(&surface, &passages);
            glib::spawn_future_local(Surface::run(surface, passages, recall, generation));
        });
    }

    /// Arm the auto-dismiss timer, if the configuration asks for one.
    ///
    /// Armed on every trigger that puts content up, so a person who presses the
    /// key again gets the full duration again rather than the remainder of the
    /// previous one. The switch and the duration are two keys in the file and one
    /// value here (spec §12).
    fn reset_dismiss(&self) {
        if let Some(source) = self.dismiss.borrow_mut().take() {
            source.remove();
        }

        let Some(seconds) = self.prefs.borrow().autohide else {
            return;
        };

        let quit = self.app.clone();
        let source = glib::timeout_add_seconds_local(seconds, move || {
            // `g_application_quit()` again: automatic dismissal is a second exit
            // path, and it has to be the same kind of exit as ✕.
            quit.quit();
            glib::ControlFlow::Break
        });

        *self.dismiss.borrow_mut() = Some(source);
    }

    /// Re-place the panel whenever the session's screens change — and, on the
    /// fallback window, whenever its own size changes.
    ///
    /// A monitor unplugged, a resolution changed, an output renamed: the same
    /// answer is re-applied rather than a panel left at a coordinate that no longer
    /// exists. A corner is resolution-independent, so re-applying it is the whole
    /// algorithm, and this is why a resolution change cannot put the panel
    /// off-screen. The window's own resizes are here for the same reason read
    /// literally: a corner measured against a window that has since grown is a
    /// coordinate that no longer exists.
    fn watch_monitors(self: &Rc<Self>) {
        // The layer path's anchors are re-applied by the compositor, but a *plain*
        // window is moved by us and it grows after it is mapped: the READING
        // message is short and the stack of Passages is not, so a bottom corner
        // measured at map time is stale by the time the body fills. Sizing is what
        // changes, so sizing is what re-places it.
        if self.reader == Reader::Gtk {
            let panel = Rc::downgrade(self);
            self.window.connect_size_allocate(move |_, _| {
                if let Some(panel) = panel.upgrade() {
                    // Geometry only: setting the note's text here would queue
                    // another allocation from inside the allocation.
                    Panel::apply_geometry(&panel);
                }
            });
        }

        let Some(display) = gdk::Display::default() else {
            return;
        };

        // A screen that appears or disappears changes what "跟随当前显示器" and
        // `output-N` mean, so the whole trigger-time refresh runs: a corner that
        // was fine on two monitors may not be on one.
        let panel = Rc::downgrade(self);
        display.connect_monitor_added(move |_, _| {
            if let Some(panel) = panel.upgrade() {
                panel.reread_prefs();
            }
        });
        let panel = Rc::downgrade(self);
        display.connect_monitor_removed(move |_, _| {
            if let Some(panel) = panel.upgrade() {
                panel.reread_prefs();
            }
        });
    }

    /// Re-apply the whole layout: the geometry **and** the note.
    ///
    /// Construction, every trigger (`reread_prefs`) and every screen change mean
    /// this one. The fallback window's resizes mean [`Panel::apply_geometry`]
    /// alone, because it must not touch the label from inside an allocation.
    fn apply_layout(&self) {
        self.apply_geometry();
        self.show_note();
    }

    /// The canvas: the corner, the output and the body's height cap.
    ///
    /// Split from [`Panel::apply_layout`] because `size-allocate` must not run
    /// [`Panel::show_note`], and the reason is a real loop rather than tidiness:
    /// `gtk_label_set_text` recalculates the label and queues a resize
    /// **unconditionally** in GTK3, so a handler that sets a label on every
    /// allocation is a handler that allocates forever. The label's text changes
    /// only with the configuration, so only the configuration path sets it.
    fn apply_geometry(&self) {
        let (anchors, pin, measure) = self.geometry();

        apply_place(
            &self.window,
            self.reader,
            &anchors,
            pin.as_ref(),
            measure.as_ref(),
        );
        self.scroller
            .set_max_content_height(cap_for(measure.as_ref()));
    }

    /// The fixed note under the content, from the provider facts (spec §12).
    ///
    /// The sentence is the button's instructions, so it comes from the same
    /// preferences that ⟳ acts on: with a cloud provider to move to, the note says
    /// so; without one it says ⟳ will use the same local model.
    fn show_note(&self) {
        let prefs = self.prefs.borrow();
        match prefs.local_note() {
            Some(note) => {
                self.surface.note.set_text(note);
                self.surface.note.set_visible(true);
            }
            None => self.surface.note.set_visible(false),
        }
    }

    /// The panel's placement: where its edges are pinned, and which screen.
    ///
    /// Two screen answers, not one, because two callers want different things and
    /// conflating them produced a real bug: the **layer surface** must be told
    /// `None` for "跟随当前显示器" (naming the primary would be the opposite of
    /// following), while the **height cap** and the fallback window's corner need a
    /// screen to measure against. `None` there meant `set_max_content_height(0)` —
    /// no cap at all on the default setting.
    fn geometry(&self) -> (Anchors, Option<gdk::Monitor>, Option<gdk::Monitor>) {
        let prefs = self.prefs.borrow();
        let anchors = Anchors::of(prefs.corner, MARGIN);
        let Some(display) = gdk::Display::default() else {
            return (anchors, None, None);
        };

        let screens: Vec<String> = (0..display.n_monitors())
            .map(|index| position::screen_name(index as usize))
            .collect();

        let (pin, measure) = match prefs.display.resolve(&screens) {
            // A screen the settings screen named, and this session has.
            Some(index) => {
                let monitor = display.monitor(index as i32);
                (monitor.clone(), monitor)
            }
            // "跟随当前显示器": the compositor places the panel on the output the
            // session is working on, so the panel deliberately names none. It
            // still measures the primary — the closest thing to a screen that is
            // always there.
            //
            // Known limit, recorded rather than hidden: once a surface *has* been
            // pinned, this arm does not unpin it. `gtk_layer_set_monitor` has no
            // `None` overload in the binding, and gtk-layer-shell offers no
            // "clear the output" — so switching `panel_display` from `output-2`
            // back to `current` leaves an already-mapped surface where it is, until
            // the next process (which is a key press away, and the panel is not
            // resident). Doing better would mean tearing the surface down and
            // remapping it, which is a visible flicker for a setting the person
            // changes once.
            None => (None, display.primary_monitor()),
        };

        (anchors, pin, measure)
    }

    /// The watchdog: armed with the window, cancelled by the first frame that
    /// actually reaches the screen.
    ///
    /// A panel that cannot draw cannot be closed either, so it has to close
    /// itself (spec §12).
    fn watch_first_frame(&self, app: &gtk::Application) {
        let drawn = Rc::new(RefCell::new(RenderWatchdog::new(watchdog::GRACE)));
        let first_frame = Rc::clone(&drawn);
        self.window.connect_draw(move |window, _| {
            // A draw on a surface that is not on screen yet is not the panel
            // reaching the screen: only a mapped window has been given a buffer,
            // and the incident this guards against is a surface that never got
            // that far.
            if window.is_mapped() {
                first_frame.borrow_mut().note_drawn();
            }
            glib::Propagation::Proceed
        });

        let quit = app.clone();
        // `_local` rather than `_local_once`: the two differ in what the closure
        // returns, and returning `Break` is what stops the source after one fire.
        glib::timeout_add_seconds_local(watchdog::GRACE.as_secs() as u32, move || {
            if drawn.borrow().expired() {
                quit.quit();
            }
            glib::ControlFlow::Break
        });
    }
}

impl Surface {
    /// Whether an answer that has just arrived still belongs to the trigger on
    /// screen.
    ///
    /// Every late writer in the panel asks this and nothing else — a chunk from a
    /// run a later press replaced, a read whose owner took its time, the answer
    /// from the thread that prepared a copy — so the rule that they are all
    /// guarded the same way lives here rather than as three comparisons that
    /// happen to agree. The callers keep their own `return`/`continue`, because
    /// what to do about a stale answer differs: skip a chunk, abandon a read, drop
    /// a status line.
    fn is_current(&self, generation: u64) -> bool {
        generation == self.generation.get()
    }

    /// One sentence in the body: a refusal, a wait, an error.
    ///
    /// The note under the content is **not** touched here: it hangs off the
    /// provider rather than off an Explanation, so it is decided once per trigger
    /// with the position and the timer, and a failed run keeps the warning that
    /// applied to it (spec §12).
    fn message(&self, text: &str) {
        clear(&self.body);

        let container = gtk::Box::new(gtk::Orientation::Vertical, 6);
        container.style_context().add_class("plainly-message");
        container.pack_start(&body_label(text), false, false, 0);
        self.body.pack_start(&container, false, false, 0);
        self.body.show_all();
    }

    /// The whole stack, with every Passage waiting.
    fn stack(&self, passages: &[String]) {
        clear(&self.body);

        // The state first, then the widgets: the panel is drawn from one value,
        // and a body that existed before the state did would be a frame in which
        // the two disagree.
        let (recall, prefs) = {
            let trigger = self.trigger.borrow();
            (trigger.recall.clone(), trigger.prefs.clone())
        };
        *self.trigger.borrow_mut() = Trigger::stack(passages.to_vec(), recall, prefs);

        for (index, passage) in passages.iter().enumerate() {
            if index > 0 {
                self.body.pack_start(
                    &gtk::Separator::new(gtk::Orientation::Horizontal),
                    false,
                    false,
                    0,
                );
            }
            let slot = gtk::Box::new(gtk::Orientation::Vertical, 6);
            slot.set_widget_name(&slot_name(index));
            fill_loading(&slot, passage);
            self.body.pack_start(&slot, false, false, 0);
        }

        self.body.show_all();
    }

    /// One slot, by index.
    fn slot_box(&self, index: usize) -> Option<gtk::Box> {
        let name = slot_name(index);

        self.body
            .children()
            .into_iter()
            .filter_map(|child| child.downcast::<gtk::Box>().ok())
            .find(|child| child.widget_name() == name)
    }

    /// Put one outcome into its slot.
    fn fill_slot(&self, index: usize, passage: &str, outcome: &Outcome, prefs: &Prefs) {
        let Some(slot) = self.slot_box(index) else {
            return;
        };

        clear(&slot);
        match outcome {
            Outcome::Explained { artifact, .. } => {
                for section in document::sections(artifact, prefs) {
                    let heading = heading(strings::section(section.kind));
                    let expander = gtk::Expander::new(None);
                    expander.set_label_widget(Some(&heading));
                    // Open by default: folding is for the second read, not the
                    // first, and a panel that arrived folded would hide the very
                    // thing the person pressed a key for.
                    expander.set_expanded(true);
                    expander.add(&body_label(&section.body));
                    slot.pack_start(&expander, false, false, 0);
                }
            }
            Outcome::Failed(reason) => {
                // The original stays up: the person can still see what failed,
                // which is the whole reason the clipboard is only ever read.
                slot.pack_start(
                    &heading(strings::section(SectionKind::Original)),
                    false,
                    false,
                    0,
                );
                slot.pack_start(&body_label(passage), false, false, 0);
                slot.pack_start(
                    &body_label(&state::failure_sentence(reason)),
                    false,
                    false,
                    0,
                );
            }
        }

        slot.show_all();
    }

    /// Replace the state with one of the sentence states and draw it.
    fn show(&self, showing: Showing) {
        let message = {
            let mut trigger = self.trigger.borrow_mut();
            trigger.showing = showing;
            trigger.showing.message().unwrap_or_default()
        };

        self.message(&message);
    }

    /// Run the passages, painting each into its slot as it lands.
    ///
    /// One run per trigger: the channel belongs to it, and the generation check
    /// is what stops a run a later trigger has replaced from painting over the
    /// panel the person is now looking at.
    async fn run(self, passages: Vec<String>, recall: Recall, generation: u64) {
        let (sender, receiver) = async_channel::unbounded::<Event>();
        worker::start(passages, recall, sender);

        // The trigger's own configuration, which is also what 复制 renders from:
        // one value, so the screen and the clipboard cannot be two opinions about
        // which sections exist. A chunk that lands after someone edited the file
        // is drawn the way the trigger that asked for it drew it.
        let prefs = self.trigger.borrow().prefs.clone();
        while let Ok(event) = receiver.recv().await {
            if !self.is_current(generation) {
                continue;
            }

            // The state first. A `Failed` event carries no index, so this is why
            // `Trigger::fail` gives every Passage the reason: none of them has an
            // Explanation, and each is still showing its original (spec §12).
            let changed = match &event {
                Event::Chunk { index, .. } => Some(*index),
                Event::Failed(_) => None,
            };
            state::apply(&mut self.trigger.borrow_mut(), event);

            // Which slots to repaint. A `Chunk` changed one; a `Failed` event
            // filled every one of them. Repainting a slot the person has already
            // read is not merely wasted work — `fill_slot` builds fresh expanders,
            // so a section they folded would pop open again on the next chunk.
            let (indexes, chip): (Vec<usize>, String) = {
                let trigger = self.trigger.borrow();
                let indexes = match changed {
                    Some(index) => vec![index],
                    None => (0..trigger.showing.slots().count()).collect(),
                };
                (indexes, trigger.showing.chip())
            };
            self.provenance.set_text(&chip);
            // Whether there is anything to copy is a question about the state, not
            // about the rendering: asking `copy_text` would build the whole
            // markdown of every answered Passage on every chunk just to test it for
            // emptiness.
            self.copy
                .set_sensitive(!self.trigger.borrow().showing.explained().is_empty());

            for index in indexes {
                let painted = {
                    let trigger = self.trigger.borrow();
                    trigger.showing.slots().nth(index).and_then(|slot| {
                        slot.outcome
                            .as_ref()
                            .map(|outcome| (slot.passage.clone(), outcome.clone()))
                    })
                };

                if let Some((passage, outcome)) = painted {
                    self.fill_slot(index, &passage, &outcome, &prefs);
                }
            }
        }
    }
}

/// How a slot's box is found again when its Explanation lands.
///
/// The widget's own name rather than a handle kept beside it: GTK hands the box
/// back by name from the body, and a name cannot go stale the way an index into a
/// vector of clones can.
fn slot_name(index: usize) -> String {
    format!("plainly-slot-{index}")
}

/// How the panel presents itself, and its shape decides how it reads the
/// clipboard.
fn present(window: &gtk::ApplicationWindow, presentation: Presentation) -> Reader {
    if presentation == Presentation::Layer && gtk_layer_shell::is_supported() {
        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        // No keyboard focus, ever: the panel must not interrupt typing.
        window.set_keyboard_mode(KeyboardMode::None);
        window.set_namespace("plainly-panel");
        Reader::DataControl
    } else {
        // No data-control, which on GNOME is also no layer shell: the panel is a
        // window that takes focus, and GTK's own clipboard works there.
        Reader::Gtk
    }
}

/// Apply a corner and a screen to the window, in whichever shape it has.
///
/// Two screens, because the two shapes want different ones: `pin` is the output a
/// layer surface is placed on (`None` is "跟随当前显示器" — the compositor picks),
/// and `measure` is a screen that always exists, which is what the fallback window
/// measures its corner against and what the height cap is computed from.
fn apply_place(
    window: &gtk::ApplicationWindow,
    reader: Reader,
    anchors: &Anchors,
    pin: Option<&gdk::Monitor>,
    measure: Option<&gdk::Monitor>,
) {
    match reader {
        Reader::DataControl => {
            window.set_anchor(Edge::Left, anchors.left);
            window.set_anchor(Edge::Right, anchors.right);
            window.set_anchor(Edge::Top, anchors.top);
            window.set_anchor(Edge::Bottom, anchors.bottom);
            for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
                window.set_layer_shell_margin(edge, anchors.margin);
            }
            if let Some(monitor) = pin {
                window.set_monitor(monitor);
            }
        }
        Reader::Gtk => {
            // A window has no anchors: the corner is measured from a screen's own
            // edges, which is the same setting read the only way a plain window
            // can read it. `measure` rather than `pin`, because "跟随当前显示器"
            // gives no pin and the corner still has to land somewhere.
            //
            // Honest limit: on Wayland `gtk_window_move` is a request the
            // compositor may ignore for an ordinary toplevel, and the flake pins
            // `GDK_BACKEND=wayland`. So on GNOME — the session this fallback exists
            // for — the corner is best-effort, and the *layer* path is the one
            // where 四角 is a guarantee. It is asked for anyway because the answer
            // is right wherever it is honoured, and doing nothing would leave the
            // setting silently unimplemented rather than unimplementable.
            let Some(monitor) = measure else {
                return;
            };
            let geometry = monitor.workarea();
            let (x, y) = position::window_origin(
                corner_of(anchors),
                anchors.margin,
                (
                    geometry.x(),
                    geometry.y(),
                    geometry.width(),
                    geometry.height(),
                ),
                (WIDTH, height_of(window)),
            );
            window.move_(x, y);
        }
    }
}

/// How tall the fallback window is, for measuring a bottom corner.
///
/// Its own allocation once it has one — a stack of Passages is taller than any
/// constant, and a bottom corner measured against a constant puts the window below
/// the screen's edge (spec §12: a resolution or monitor change must not lose the
/// panel). Before the first allocation it is the header's height, so the number is
/// never zero and the window never lands outside the workarea.
fn height_of(window: &gtk::ApplicationWindow) -> i32 {
    match window.allocated_height() {
        0 => HEADER_HEIGHT,
        height => height,
    }
}

/// The corner a set of anchors came from: the inverse of [`Anchors::of`], and
/// total because exactly one of each opposite pair is set.
fn corner_of(anchors: &Anchors) -> plainly_core::PanelCorner {
    use plainly_core::PanelCorner::*;

    match (anchors.left, anchors.top) {
        (true, true) => TopLeft,
        (false, true) => TopRight,
        (true, false) => BottomLeft,
        (false, false) => BottomRight,
    }
}

/// How tall the panel's body may become, from the screen it is on.
fn cap_for(monitor: Option<&gdk::Monitor>) -> i32 {
    monitor
        .map(|monitor| {
            let geometry = monitor.workarea();
            (geometry.height() - 2 * MARGIN).max(0) / HEIGHT_SHARE
        })
        .unwrap_or(0)
}

/// A Passage waiting for its Explanation: the original first, then a skeleton bar
/// and the fact that work is happening.
///
/// The original is never hidden behind the wait (spec §12), and the wait is long
/// enough to need showing — the first cold start is 1.3–2 seconds, so the loading
/// state is not an optional nicety. The bar is `gtk::ProgressBar` in its
/// indeterminate mode: GTK animates it, so there is no animation of ours to keep
/// running, and it says "work is happening" without claiming to know how much is
/// left.
fn fill_loading(slot: &gtk::Box, passage: &str) {
    slot.pack_start(
        &heading(strings::section(SectionKind::Original)),
        false,
        false,
        0,
    );
    slot.pack_start(&body_label(passage), false, false, 0);

    let bar = gtk::ProgressBar::new();
    bar.set_pulse_step(0.1);
    bar.style_context().add_class("plainly-skeleton");
    slot.pack_start(&bar, false, false, 0);
    slot.pack_start(&body_label(strings::EXPLAINING), false, false, 0);
}

/// One action button in the title bar: flat, with a name a screen reader can read
/// and a tooltip a pointer can.
fn action_button(label: &str, name: &str) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.set_relief(gtk::ReliefStyle::None);
    button.set_tooltip_text(Some(name));
    // The accessible name lives on the widget's ATK object, which is where GTK
    // keeps it; a screen reader reads `✕` as "close panel" rather than as a
    // multiplication sign.
    if let Some(accessible) = button.accessible() {
        accessible.set_accessible_name(Some(name));
    }
    button.style_context().add_class("flat");
    button
}

/// The status line: what an action just did.
///
/// Empty until an action has something to say, and never a colour or an icon: the
/// panel's own words are the interface, and a glyph would be one more thing to
/// translate (spec §12). It is not the provenance chip, which is the label beside
/// it in the header.
fn status_label() -> gtk::Label {
    let label = gtk::Label::new(None);
    label.set_xalign(0.0);
    label.style_context().add_class("plainly-status");
    label
}

/// The provenance chip.
fn provenance_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    label.style_context().add_class("plainly-provenance");
    label
}

/// The fixed note under the content.
///
/// Its text is set per trigger, because which of the two sentences applies
/// depends on the provider in effect.
fn note_label(text: &str) -> gtk::Label {
    let label = body_label(text);
    label.style_context().add_class("plainly-note");
    label
}

fn clear(container: &gtk::Box) {
    for child in container.children() {
        container.remove(&child);
    }
}

/// A section's heading, in the panel's own language.
fn heading(text: &str) -> gtk::Label {
    let label = gtk::Label::new(None);
    label.set_markup(&format!("<b>{}</b>", glib::markup_escape_text(text)));
    label.set_xalign(0.0);
    label
}

/// A section's body: wrapped, selectable, and starting at the left edge.
fn body_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_line_wrap(true);
    label.set_xalign(0.0);
    label.set_selectable(true);
    label
}
