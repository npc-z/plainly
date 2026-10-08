//! The application: one `GtkApplication`, one window, one trigger at a time.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gio, glib};

use crate::capability;
use crate::panel::Panel;

/// The GApplication id, and so the uniqueness key: a second `plainly-panel`
/// hands its activation to the first rather than opening a second panel
/// (spec §12). It is deliberately not the main window's `dev.plainly.app`.
pub const APPLICATION_ID: &str = "dev.plainly.panel";

/// Run the panel: probe the session, then hand every trigger to one window.
pub fn run() -> glib::ExitCode {
    // The desktop's own answer about itself, as data, before any surface is
    // made: whether the clipboard can be read without taking focus decides
    // whether the panel is a layer surface or a window (spec §12, tickets/12).
    let presentation = match capability::presentation() {
        Ok(presentation) => presentation,
        Err(error) => {
            eprintln!("plainly-panel: {error}");
            return glib::ExitCode::FAILURE;
        }
    };

    let app = gtk::Application::new(Some(APPLICATION_ID), gio::ApplicationFlags::empty());
    let panel: Rc<RefCell<Option<Rc<Panel>>>> = Rc::new(RefCell::new(None));

    app.connect_activate(move |app| {
        // `activate` is the trigger and the re-trigger: a second key press
        // reuses the window and reads the clipboard again (spec §12). The
        // single-instance handover is `GtkApplication`'s, not ours.
        let mut slot = panel.borrow_mut();
        match slot.as_ref() {
            Some(panel) => panel.refresh(),
            None => *slot = Some(Panel::new(app, presentation)),
        }
    });

    app.run_with_args::<&str>(&[])
}
