# Tasks for working on Plainly. `just` on its own lists them; `just <recipe>`
# runs one.
#
# The flake at this directory is the environment (AGENTS.md): every recipe that
# needs the Rust toolchain or the GUI libraries goes through `nix develop -c`
# unless it is already inside the dev shell, where `IN_NIX_SHELL` is set. The
# recipes that are Nix commands themselves (`nix build`, `nix run`) or that
# deliberately need no Nix run bare — their comments say which.

set shell := ["bash", "-euo", "pipefail", "-c"]

# `nix develop -c` from an ordinary shell; empty inside the dev shell.
dev_shell := if env_var_or_default("IN_NIX_SHELL", "") == "" { "nix develop -c" } else { "" }

# List the recipes.
default:
    @just --list

# Enter the flake's development shell: toolchain, GTK/WebKit, python3.
dev:
    nix develop

# -- build -- -----------------------------------------------------------------

# Build every crate in the workspace (debug); needs the dev shell's libraries.
build:
    {{dev_shell}} cargo build --workspace

# Build the command line alone: the crate that must need no graphics stack.
build-cli:
    {{dev_shell}} cargo build -p plainly-cli

# Build the installed binaries with Nix: result/bin/{plainly,plainly-panel,plainly-desktop}.
nix-build:
    nix build .

# -- run -- -------------------------------------------------------------------
#
# The GUI recipes come in two shapes. `desktop` and `panel` go through `nix run`,
# so they start the wrapped, release binaries an install gets (the library paths
# are in the wrapper) — the same ones a compositor's key binding and the desktop
# entry spawn. `desktop-dev` and `panel-dev` run `cargo run` inside the dev
# shell: the faster edit -> run loop, which is why the shell carries the window's
# extra libraries (gdk-pixbuf, cairo, dbus). All of them need a real Wayland
# session.

# Run the command line from source, e.g. `just cli history` (fastest edit loop).
cli *args:
    {{dev_shell}} cargo run -p plainly-cli -- {{args}}

# Open the main window; `just desktop --watch` also listens for copies.
desktop *args:
    nix run .#desktop -- {{args}}

# Open the panel for what the clipboard holds, the way a key binding asks for it.
panel:
    nix run .#panel -- --clipboard

# The window from a debug build inside the dev shell (the edit -> run loop).
desktop-dev *args:
    {{dev_shell}} cargo run -p plainly-desktop -- {{args}}

# The panel from a debug build inside the dev shell.
panel-dev:
    {{dev_shell}} cargo run -p plainly-panel -- --clipboard

# -- configure -- -------------------------------------------------------------

# Read or write config.toml (panel and window), e.g. `config set app.panel_width 520`.
config *args:
    {{dev_shell}} cargo run -p plainly-cli -- config {{args}}

# -- test -- ------------------------------------------------------------------

# Run the tests CI runs: the two crates that build with no graphics stack.
test:
    {{dev_shell}} cargo test -p plainly-core
    {{dev_shell}} cargo test -p plainly-cli

# Test every crate, including the panel and the main window (needs the dev shell).
test-all:
    {{dev_shell}} cargo test --workspace

# Test the domain logic alone.
test-core:
    {{dev_shell}} cargo test -p plainly-core

# Test the command line alone.
test-cli:
    {{dev_shell}} cargo test -p plainly-cli

# -- lint and format -- -------------------------------------------------------

# Clippy over the two crates CI lints; warnings are reported, not fatal.
clippy:
    {{dev_shell}} cargo clippy -p plainly-core -p plainly-cli --all-targets

# Format the workspace with rustfmt (wide diff; CI does not gate on formatting).
fmt:
    {{dev_shell}} cargo fmt --all

# Show what rustfmt would change, writing nothing. Advisory; not part of `check`.
fmt-check:
    {{dev_shell}} cargo fmt --all -- --check

# -- the headless promise -- --------------------------------------------------

# Prove the CLI's dependency graph and built binary reach no graphics library.
check-headless:
    {{dev_shell}} bash scripts/check-headless-cli.sh

# The same check against a release build.
check-headless-release:
    {{dev_shell}} bash scripts/check-headless-cli.sh --release

# -- the generated CLI workspace -- -------------------------------------------

# Guard nix/cli-workspace/ against drift. Offline, under a second, no Nix needed.
check-cli-workspace:
    bash scripts/check-cli-workspace.sh

# Regenerate nix/cli-workspace/ after a root dependency change (needs the dev shell).
generate-cli-workspace:
    {{dev_shell}} bash scripts/generate-cli-workspace.sh

# Compare the generated workspace with the root manifests, writing nothing.
check-generated-cli-workspace:
    {{dev_shell}} bash scripts/generate-cli-workspace.sh --check

# -- Nix packaging -- ---------------------------------------------------------

# Build with Nix and check the packaged binaries run outside the dev shell.
check-packages:
    bash scripts/check-flake-packages.sh

# The same checks against whatever is already built.
check-packages-built:
    bash scripts/check-flake-packages.sh --no-build

# After a committed flake-input bump: prove a `.rs` edit keeps the dependency cache.
check-incremental:
    bash scripts/check-incremental-build.sh

# Prove both halves of that guard are still live (`--self-test`).
check-incremental-self-test:
    bash scripts/check-incremental-build.sh --self-test

# Update flake inputs. Commit the result, then run `just check-incremental`.
update-flake:
    nix flake update

# -- manual checks -- ---------------------------------------------------------

# Run the fake provider the manual checks use, foreground, e.g. `--port 8080`.
stub-provider *args:
    {{dev_shell}} python3 scripts/stub-provider.py {{args}}

# -- everything automated -- --------------------------------------------------

# What to run before pushing: the CI set plus the generated-workspace guard.
check: check-cli-workspace clippy test check-headless
