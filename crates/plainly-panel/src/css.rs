//! The panel's one style sheet, in code.
//!
//! A layer surface gets no desktop theme to speak of, and the two things the spec
//! fixes about the panel's look are its width (about 470px) and the fact that the
//! local-model note under the content is a note rather than more content
//! (spec §12). Everything else is GTK's default, which is the right default for a
//! surface whose job is to be read and dismissed.
use gtk::prelude::*;

/// The panel's styles, as CSS.
pub const CSS: &str = "
/* The provenance chip and the status line: quieter than the body, because both
   are about the Explanation rather than part of it. */
.plainly-provenance,
.plainly-status {
    font-size: 0.85em;
    opacity: 0.65;
}
.plainly-note {
    font-size: 0.9em;
    opacity: 0.8;
    padding: 6px 10px;
}
/* A warning about the answer rather than part of it: the not-stored line under a
   Passage. Italic and dimmer than the body, so it reads as a note on what is
   above it instead of more Explanation. */
.plainly-warning {
    font-style: italic;
    opacity: 0.8;
}
/* The one-sentence states carry no padding of their own: the body they sit in
   has the panel's margin, and a second inset inside it would put a refusal at a
   different distance from the edge than the Explanation it replaces. */
/* The skeleton bar while an Explanation is being made: a trough, not a progress
   readout — GTK pulses the filled part, and nothing here claims to know how far
   along a request is. */
.plainly-skeleton trough {
    min-height: 4px;
}
.plainly-skeleton progress {
    min-height: 4px;
}

/* ── the Key Help ──────────────────────────────────────────────────────
   A Gloss has three parts with three different jobs, and the panel gives each
   its own voice rather than printing the document's markup: the expression in a
   chip (it is quoted from the Passage), its meaning as ordinary body text, and
   its Example beside a rule in a serif italic — a sample of prose, not more
   definition. The panel showed the literal `-`, backticks and `_` until the
   renderer started handing it a structure instead of a document. */
.plainly-expression-chip {
    background-color: rgba(0, 0, 0, 0.06);
    border-radius: 4px;
    padding: 1px 6px;
}
.plainly-expression {
    font-family: monospace;
    font-size: 0.95em;
}
.plainly-example-rule {
    background-color: rgba(0, 0, 0, 0.18);
}
.plainly-example {
    font-family: serif;
    font-style: italic;
    opacity: 0.75;
}
";

/// Install the styles on the display the panel is drawn on.
///
/// A screen with no display is not an error: the panel would have nothing to draw
/// on at all, and it is the window that fails loudly in that case, not the
/// stylesheet.
pub fn install() {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    let Some(screen) = gtk::gdk::Screen::default() else {
        return;
    };

    let provider = gtk::CssProvider::new();
    // A parse failure is not fatal either: the panel would draw with GTK's own
    // defaults, which is what a missing stylesheet looks like anyway.
    let _ = provider.load_from_data(CSS.as_bytes());

    gtk::StyleContext::add_provider_for_screen(
        &screen,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // Named rather than returned: the provider has to outlive this call, and the
    // screen now holds a reference to it. Leaking one small stylesheet per
    // process is what "the display owns it" means here.
    let _ = display;
}
