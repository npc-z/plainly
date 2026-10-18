//! What the compositor says about itself, as data.
//!
//! The panel's shape and its way of reading the clipboard both follow from
//! whether the session publishes data-control. The answer is read from the
//! compositor's registry rather than guessed at, and handed to core, which owns
//! the decision.
//!
//! The session itself is settled before connecting: a Wayland client on X11 can
//! only fail, and its failure would name a socket rather than the exclusion.

use plainly_core::DisplaySession;
use plainly_core::desktop::{Desktop, Presentation};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_registry;
use wayland_client::{Connection, Dispatch, QueueHandle};

/// How this session's panel has to present itself.
///
/// X11 is refused in words rather than attempted: the panel is a layer surface,
/// and X11 has no such surface.
pub fn presentation() -> Result<Presentation, String> {
    if let Some(refusal) = DisplaySession::from_process_env().refusal("the panel") {
        return Err(refusal.to_string());
    }

    Ok(Desktop::new(protocols()?).presentation())
}

/// The globals the compositor published, by interface name.
fn protocols() -> Result<Vec<String>, String> {
    let connection = Connection::connect_to_env()
        .map_err(|error| format!("cannot connect to the Wayland session: {error}"))?;
    let (globals, _queue) = registry_queue_init::<Registry>(&connection)
        .map_err(|error| format!("cannot read the compositor's protocols: {error}"))?;

    Ok(globals
        .contents()
        .clone_list()
        .into_iter()
        .map(|global| global.interface)
        .collect())
}

/// The registry's only job is to be listed: nothing here binds a global, so
/// there is no state to keep.
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
