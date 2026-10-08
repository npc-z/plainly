//! What the compositor says about itself, as data.
//!
//! The panel's shape and its way of reading the clipboard both follow from
//! whether the session publishes data-control (spec §12, tickets/12). The answer
//! is read from the compositor's registry rather than guessed at, and handed to
//! core, which owns the decision.

use plainly_core::desktop::{Desktop, Presentation};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_registry;
use wayland_client::{Connection, Dispatch, QueueHandle};

/// How this session's panel has to present itself.
pub fn presentation() -> Result<Presentation, String> {
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
