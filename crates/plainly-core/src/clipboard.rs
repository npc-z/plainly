//! The clipboard as data, and the one place sensitive content is judged.
//!
//! The platform's job is only to observe: the Passage the clipboard holds and
//! the MIME types its owner published, with the bytes behind the ones it asked
//! for. Everything else — which of those types means "secret", and what may then
//! be done with the Passage — is decided here, once, for the panel, the CLI and
//! the copy-auto-popup alike (spec §11). Each surface writing its own check is
//! how one of them eventually forgets, and the one that forgets is where the
//! leak happens.
//!
//! The rule is deliberately narrow. Heuristics over the text — entropy, length,
//! the absence of spaces — would produce both false positives and false
//! negatives, and the worst outcome is the false sense of protection; a password
//! manager that does not follow the convention is simply not covered.

/// The MIME type a password manager publishes to say what the clipboard holds.
///
/// It is a KDE convention rather than a Wayland one, and [`wl-clipboard`] reads
/// it too. Published without [`PASSWORD_HINT_SECRET`] as its value it means
/// nothing: KeePassXC publishes it with the value `public` for ordinary copies.
///
/// [`wl-clipboard`]: https://github.com/bugaevc/wl-clipboard
pub const PASSWORD_HINT: &str = "x-kde-passwordManagerHint";

/// The value of [`PASSWORD_HINT`] that marks the content secret.
pub const PASSWORD_HINT_SECRET: &[u8] = b"secret";

/// One MIME type the clipboard published, and the bytes its owner gave for it.
///
/// `value` is `None` for a type whose value was not read. Asking for a type is a
/// separate round trip, so a platform reads only the ones it has a use for.
/// [`PASSWORD_HINT`] is the one this module reads, and a platform that cannot
/// read it hands over `None` rather than a guess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MimeType {
    pub mime: String,
    pub value: Option<Vec<u8>>,
}

/// What the clipboard offered, as data.
///
/// The Passage is what a run would explain; the types are the owner's own
/// description of it, and the sensitive-content judgement reads those rather
/// than the words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clipboard {
    /// The Passage the clipboard holds, verbatim.
    pub passage: String,
    /// The MIME types the owner published, in the order it published them.
    pub types: Vec<MimeType>,
}

/// What Plainly may do with what the clipboard held.
///
/// Returning the Passage only through [`Reading::Passage`] is what makes the
/// check impossible to skip: a surface cannot get at the Passage without having
/// asked whether it was allowed to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reading {
    /// Nothing marked this content secret: it is a Passage to explain.
    Passage(String),
    /// The owner marked this content secret. Nothing is sent and nothing is
    /// stored, and there is no "explain anyway". The cost is asymmetric.
    Sensitive,
    /// The owner published the marker and its value was not read. Half the
    /// criterion is not a reason to send: the run stops and reports that the
    /// value could not be read.
    Unread,
}

impl Clipboard {
    /// What may be done with this clipboard (spec §11).
    pub fn reading(self) -> Reading {
        match self.marker() {
            Marker::Absent | Marker::NotSecret => Reading::Passage(self.passage),
            Marker::Secret => Reading::Sensitive,
            Marker::Unread => Reading::Unread,
        }
    }

    fn marker(&self) -> Marker {
        match self
            .types
            .iter()
            .find(|offered| offered.mime == PASSWORD_HINT)
        {
            None => Marker::Absent,
            Some(hint) => match hint.value.as_deref() {
                // Matched byte for byte: a different case is a different value.
                Some(PASSWORD_HINT_SECRET) => Marker::Secret,
                Some(_) => Marker::NotSecret,
                None => Marker::Unread,
            },
        }
    }
}

/// What the owner's password hint says about one clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Marker {
    /// The hint type was not published.
    Absent,
    /// It was published with the value that marks the content secret.
    Secret,
    /// It was published with some other value, `public` being the usual one.
    NotSecret,
    /// It was published and its value was not read.
    Unread,
}
