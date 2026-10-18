//! What the compositor says about itself, for the one settings decision that
//! depends on it.
//!
//! The question is the panel's: does this session publish data-control, the
//! protocol that lets a process read the clipboard without taking focus? Here it
//! decides whether the copy listener can be offered. The registry is read rather
//! than guessed at, and the answer goes to core, which owns the decision. This
//! module only knows how to ask (spec §12).
//!
//! A session with no Wayland at all is an answer, not a failure: it is the same
//! "no data-control" the settings screen draws with a sentence and a greyed
//! switch.

use plainly_core::Desktop;
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_registry;
use wayland_client::{Connection, Dispatch, QueueHandle};

/// The session's capabilities, as its protocol list describes them.
///
/// A compositor that cannot be reached is an empty list rather than an error:
/// the caller's question is "can this session do it", and "no session" is a
/// good "no".
pub fn desktop() -> Desktop {
    Desktop::new(protocols().unwrap_or_default())
}

/// How many outputs the compositor publishes.
///
/// The count and not the names, deliberately. `wl_output`'s own events carry a
/// make and a model, not the connector name a person would recognise, and that
/// name comes from `xdg-output`, a protocol crate this workspace has no route to
/// fetch. A picker offering "显示器 1" and "显示器 2" is the honest version of
/// what the build can see.
pub fn outputs() -> Vec<(String, String)> {
    let count = protocols()
        .map(|protocols| {
            protocols
                .iter()
                .filter(|interface| *interface == "wl_output")
                .count()
        })
        .unwrap_or(0);

    (0..count)
        .map(|index| {
            (
                format!("output-{}", index + 1),
                crate::strings::output_label(index),
            )
        })
        .collect()
}

/// The globals the compositor published, by interface name.
fn protocols() -> Option<Vec<String>> {
    let connection = Connection::connect_to_env().ok()?;
    let (globals, _queue) = registry_queue_init::<Registry>(&connection).ok()?;

    Some(
        globals
            .contents()
            .clone_list()
            .into_iter()
            .map(|global| global.interface)
            .collect(),
    )
}

/// The registry is only listed, never bound, so it has no state to keep.
struct Registry;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Registry {
    fn event(
        _state: &mut Self,
        _registry: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _connection: &Connection,
        _queue: &QueueHandle<Self>,
    ) {
    }
}
