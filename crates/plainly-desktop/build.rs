//! Tauri's build step: it embeds `tauri.conf.json`, the window icon and the
//! frontend assets, so that the binary carries its own interface and needs no
//! server, no bundler and no `node` at run time.
//!
//! The frontend is plain HTML, CSS and JavaScript on purpose. A bundler would
//! add a JavaScript toolchain to a Rust workspace for a screen that is a list and
//! a form, and would put a build artifact between the source and the binary that
//! ships.

fn main() {
    tauri_build::build()
}
