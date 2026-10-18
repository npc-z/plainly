//! Tauri's build step: it embeds `tauri.conf.json`, the window icon and the
//! frontend assets, so the binary carries its own interface and needs no server,
//! bundler or `node` at run time.
//!
//! The frontend is plain HTML, CSS and JavaScript on purpose: a bundler would add
//! a JavaScript toolchain to a Rust workspace for a screen that is a list and a
//! form.

fn main() {
    tauri_build::build()
}
