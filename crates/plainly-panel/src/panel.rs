//! The panel window: what a trigger puts on the screen.
//!
//! One window, reused: `activate` re-reads the clipboard and refreshes what is
//! inside it rather than making a second panel (spec §12). The window is a layer
//! surface that never takes focus, or — where the session has no data-control,
//! which is also where it has no layer shell — a plain window that does.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gdk, glib};
use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use plainly_core::artifact::Artifact;
use plainly_core::clipboard::Clipboard;
use plainly_core::desktop::Presentation;
use plainly_core::render::{self, SectionKind};

use crate::strings;
use crate::watchdog::{self, RenderWatchdog};
use crate::worker::{self, Event, Outcome};
use crate::{clipboard, plan};

/// The panel's width (spec §12: about 470px).
const WIDTH: i32 = 470;
/// The gap between a layer surface and the screen edge.
const MARGIN: i32 = 12;

/// How the panel reads the clipboard, which is the same question as its shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reader {
    /// A layer surface that never has focus: through data-control.
    DataControl,
    /// A focused window: through GTK, which works there.
    Gtk,
}

/// The panel: one window, and the trigger that refreshes it.
pub struct Panel {
    window: gtk::ApplicationWindow,
    body: gtk::Box,
    reader: Reader,
    /// Bumped on every trigger, so a run started earlier cannot paint over a
    /// later one.
    generation: Rc<Cell<u64>>,
    /// Whether a trigger is waiting for the focus the GTK reader needs.
    waiting: Cell<bool>,
}

impl Panel {
    /// Make the panel, show it, and show what the clipboard holds.
    pub fn new(app: &gtk::Application, presentation: Presentation) -> Rc<Self> {
        let window = gtk::ApplicationWindow::new(app);
        window.set_title(strings::TITLE);
        window.set_decorated(false);
        window.set_default_size(WIDTH, -1);
        window.set_size_request(WIDTH, -1);

        let reader = present(&window, presentation);
        let body = gtk::Box::new(gtk::Orientation::Vertical, 9);

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let title = gtk::Label::new(Some(strings::TITLE));
        title.set_xalign(0.0);
        header.pack_start(&title, true, true, 0);
        // The one way out: `keyboard_mode=NONE` means Esc does nothing, and
        // automatic dismissal is off by default (spec §12). Quitting goes
        // through the application, never `gtk_main_quit()`.
        let close = gtk::Button::with_label(strings::CLOSE);
        close.set_relief(gtk::ReliefStyle::None);
        let quit = app.clone();
        close.connect_clicked(move |_| quit.quit());
        header.pack_end(&close, false, false, 0);

        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&body)
            .build();

        let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
        root.pack_start(&header, false, false, 0);
        root.pack_start(
            &gtk::Separator::new(gtk::Orientation::Horizontal),
            false,
            false,
            0,
        );
        root.pack_start(&scroller, true, true, 0);
        window.add(&root);

        // The watchdog: armed with the window, cancelled by the first frame that
        // actually reaches the screen. A panel that cannot draw cannot be closed
        // either, so it has to close itself (spec §12).
        let drawn = Rc::new(RefCell::new(RenderWatchdog::new(watchdog::GRACE)));
        let first_frame = Rc::clone(&drawn);
        window.connect_draw(move |window, _| {
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
        glib::timeout_add_seconds_local_once(watchdog::GRACE.as_secs() as u32, move || {
            if drawn.borrow().expired() {
                quit.quit();
            }
        });

        let panel = Rc::new(Self {
            window,
            body,
            reader,
            generation: Rc::new(Cell::new(0)),
            waiting: Cell::new(false),
        });

        // A focused window is what makes GTK's clipboard work here, so this
        // path's read waits for the focus the window was made to take.
        if reader == Reader::Gtk {
            let focused = Rc::clone(&panel);
            panel.window.connect_focus_in_event(move |_, _| {
                if focused.waiting.replace(false) {
                    focused.read();
                }
                glib::Propagation::Proceed
            });
        }

        show_message(&panel.body, strings::READING);
        panel.window.show_all();
        panel.refresh();
        panel
    }

    /// A trigger: show what the clipboard holds now.
    ///
    /// The second press is this same call: the person who pressed it has almost
    /// certainly copied something new, and if they did not, the Lookup Key finds
    /// what is already stored (spec §12).
    pub fn refresh(&self) {
        match self.reader {
            // A layer surface never has focus, and data-control does not need
            // it: the read can happen straight away.
            Reader::DataControl => self.read(),
            // This path's window is the one that takes focus, and the keybinding
            // press does not give it any: ask for it, and read when it arrives.
            Reader::Gtk => {
                self.window.present();
                if self.window.is_active() {
                    self.read();
                } else {
                    self.waiting.set(true);
                }
            }
        }
    }

    /// Read the clipboard and put the outcome on the screen.
    ///
    /// The data-control read happens on its own thread: the transfer waits for
    /// the clipboard's owner to write, and an owner that stalls must not take
    /// the main loop down with it — ✕ and the watchdog both live there.
    fn read(&self) {
        let generation = self.generation.get() + 1;
        self.generation.set(generation);

        let (sender, receiver) = async_channel::unbounded::<Result<Clipboard, clipboard::Error>>();
        match self.reader {
            Reader::DataControl => {
                std::thread::spawn(move || {
                    let _ = sender.send_blocking(clipboard::data_control());
                });
            }
            // GTK's clipboard is main-thread work. The read blocks this thread
            // for as long as the owner takes, which is the one hole the watchdog
            // cannot cover; the fallback path is a focused window on a session
            // where that transfer is a local one (spec §12).
            Reader::Gtk => {
                let _ =
                    sender.send_blocking(clipboard::focused_window(&gtk::Clipboard::for_display(
                        &gdk::Display::default().expect("GTK is up when the panel is"),
                        &gdk::SELECTION_CLIPBOARD,
                    )));
            }
        }

        let body = self.body.clone();
        let current = Rc::clone(&self.generation);
        glib::spawn_future_local(async move {
            let Ok(clipboard) = receiver.recv().await else {
                return;
            };
            // A later press owns the panel; an earlier read is stale.
            if generation != current.get() {
                return;
            }

            let passages = match clipboard {
                // An empty clipboard is not an error state, and it reads in the
                // panel's own language; anything else is, and says so in it too.
                Err(clipboard::Error::NoPassage(_)) => {
                    show_message(&body, strings::NOTHING);
                    return;
                }
                Err(error) => {
                    show_message(&body, &strings::unreadable(&error.to_string()));
                    return;
                }
                Ok(clipboard) => match plan::plan(clipboard) {
                    plan::Plan::Sensitive => {
                        show_message(&body, strings::SENSITIVE);
                        return;
                    }
                    plan::Plan::Unread => {
                        show_message(&body, strings::UNREAD);
                        return;
                    }
                    plan::Plan::Nothing => {
                        show_message(&body, strings::NOTHING);
                        return;
                    }
                    plan::Plan::TooLong(too_many) => {
                        show_message(&body, &strings::too_long(&too_many));
                        return;
                    }
                    plan::Plan::Passages(passages) => passages,
                },
            };

            // The Passages go up as they are, so the original is visible while
            // the Explanation is being made (spec §12).
            let slots = loading(&body, &passages);
            let (sender, receiver) = async_channel::unbounded::<Event>();
            worker::start(passages.clone(), sender);

            while let Ok(event) = receiver.recv().await {
                if generation != current.get() {
                    continue;
                }

                match event {
                    Event::Chunk { index, outcome } => {
                        if let (Some(slot), Some(passage)) = (slots.get(index), passages.get(index))
                        {
                            fill_outcome(slot, passage, outcome);
                        }
                    }
                    // The run could not even start, so no Passage has an
                    // Explanation — and every Passage still keeps its original
                    // (spec §12), with the same reason under each.
                    Event::Failed(reason) => {
                        for (slot, passage) in slots.iter().zip(&passages) {
                            fill_outcome(slot, passage, Outcome::Failed(reason.clone()));
                        }
                    }
                }
            }
        });
    }
}

/// How the panel presents itself, and so how it reads the clipboard.
fn present(window: &gtk::ApplicationWindow, presentation: Presentation) -> Reader {
    if presentation == Presentation::Layer && gtk_layer_shell::is_supported() {
        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        // No keyboard focus, ever: the panel must not interrupt typing.
        window.set_keyboard_mode(KeyboardMode::None);
        window.set_namespace("plainly-panel");
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Left, true);
        window.set_layer_shell_margin(Edge::Top, MARGIN);
        window.set_layer_shell_margin(Edge::Left, MARGIN);
        Reader::DataControl
    } else {
        // No data-control, which on GNOME is also no layer shell: the panel is a
        // window that takes focus, and GTK's own clipboard works there.
        Reader::Gtk
    }
}

/// Replace the body with one message: the states that are not an Explanation —
/// a refusal, an empty clipboard, a clipboard that could not be read.
fn show_message(body: &gtk::Box, message: &str) {
    clear(body);
    body.pack_start(&body_label(message), false, false, 0);
    body.show_all();
}

/// Put each Passage up as it is, and give back the boxes its Explanation will
/// fill in. The original is visible from the first frame of the wait, and every
/// Passage keeps its place whether or not its turn comes out.
fn loading(body: &gtk::Box, passages: &[String]) -> Vec<gtk::Box> {
    clear(body);

    let mut slots = Vec::with_capacity(passages.len());
    for (index, passage) in passages.iter().enumerate() {
        if index > 0 {
            body.pack_start(
                &gtk::Separator::new(gtk::Orientation::Horizontal),
                false,
                false,
                0,
            );
        }
        let slot = gtk::Box::new(gtk::Orientation::Vertical, 6);
        fill_loading(&slot, passage);
        body.pack_start(&slot, false, false, 0);
        slots.push(slot);
    }
    body.show_all();
    slots
}

/// A Passage waiting for its Explanation: the original first, then the fact that
/// work is happening. The original is never hidden behind the wait (spec §12).
fn fill_loading(slot: &gtk::Box, passage: &str) {
    slot.pack_start(
        &heading(strings::section(SectionKind::Original)),
        false,
        false,
        0,
    );
    slot.pack_start(&body_label(passage), false, false, 0);
    slot.pack_start(&body_label(strings::EXPLAINING), false, false, 0);
}

/// One Passage's turn came out: its sections, or why it has none.
fn fill_outcome(slot: &gtk::Box, passage: &str, outcome: Outcome) {
    clear(slot);

    match outcome {
        Outcome::Explained(artifact) => fill_sections(slot, &artifact),
        Outcome::Failed(reason) => {
            // The original stays up: the person can still see what failed, which
            // is the whole reason the clipboard is only ever read.
            slot.pack_start(
                &heading(strings::section(SectionKind::Original)),
                false,
                false,
                0,
            );
            slot.pack_start(&body_label(passage), false, false, 0);
            slot.pack_start(
                &body_label(&format!("{reason}\n{}", strings::NOTHING_STORED)),
                false,
                false,
                0,
            );
        }
    }

    slot.show_all();
}

/// One Explanation, as the panel shows it: the sections core rendered, under the
/// headings the panel words for itself.
fn fill_sections(slot: &gtk::Box, artifact: &Artifact) {
    for section in render::panel(
        &artifact.passage,
        &artifact.explanation,
        strings::GRAMMAR_NOT_NEEDED,
    ) {
        slot.pack_start(&heading(strings::section(section.kind)), false, false, 0);
        slot.pack_start(&body_label(&section.body), false, false, 0);
    }
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
