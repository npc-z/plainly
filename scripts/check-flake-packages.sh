#!/usr/bin/env bash
#
# The acceptance seam for the packaging ticket: what `nix build` produces, and
# whether the thing it produces actually runs.
#
# `cargo test` cannot answer either question — both crates' tests run against
# `target/debug`, where the Rust toolchain's `LD_LIBRARY_PATH` or the dev shell's
# is in force. The failure this script exists to catch is specifically the one
# that only appears *outside* that environment: a binary that links GTK and then
# cannot find it, which is what a compositor's key binding would hit.
#
# Usage:
#   scripts/check-flake-packages.sh            # build and check
#   scripts/check-flake-packages.sh --no-build # check already-built outputs
#
# Every check prints what it observed, so a failure is a line of evidence rather
# than a bare "FAIL".
set -euo pipefail

cd "$(dirname "$0")/.."

# A nix whose fetcher cache is writable. The default one lives in
# `~/.cache/nix`, which is read-only in some sandboxes; a caller can point this
# anywhere.
: "${NIX_CACHE_HOME:=${TMPDIR:-/tmp}/plainly-nix-cache}"
export NIX_CACHE_HOME
mkdir -p "$NIX_CACHE_HOME"

# `--no-build` skips the `nix build` calls and asks Nix for the store paths it
# already has; useful when a full build has just run and the question is only
# whether the outputs are shaped right.
build=true
if [ "${1:-}" = "--no-build" ]; then
    build=false
fi

fail() {
    echo "FAIL: $*" >&2
    exit 1
}

note() {
    echo "   $*"
}

# --- the flake declares the two packages ------------------------------------
echo "== the flake's packages"
packages=$(nix flake show --json 2>/dev/null | python3 -c '
import json, sys
show = json.load(sys.stdin)
print(" ".join(sorted(show.get("packages", {}).get("x86_64-linux", {}))))
' 2>/dev/null || echo "")
note "packages: ${packages:-（none）}"
case " $packages " in
    *" plainly "*) ;;
    *) fail "the flake exposes no \`plainly\` package (got: ${packages:-none})" ;;
esac
case " $packages " in
    *" default "*) ;;
    *) fail "the flake exposes no \`default\` package (got: ${packages:-none})" ;;
esac

# --- built, or build it -----------------------------------------------------
if [ "$build" = true ]; then
    out_cli=$(nix build --no-link --print-out-paths .#plainly)
    out_all=$(nix build --no-link --print-out-paths .)
else
    out_cli=$(nix path-info .#plainly)
    out_all=$(nix path-info .)
    # `nix path-info` resolves the output path without building it, so with
    # `--no-build` the honest thing is to say the path is not in the store yet
    # rather than to check the shape of a directory that is not there.
    for path in "$out_cli" "$out_all"; do
        [ -e "$path" ] || fail "$path is not built yet — run without --no-build"
    done
fi
note "plainly: $out_cli"
note "default: $out_all"

# --- `nix run` reaches a real program ---------------------------------------
echo "== nix run"
# `< /dev/null` matters: with no Passage named, `plainly` reads stdin, and a check
# that leaves it a terminal waits for a human instead of failing.
run_version=$(timeout 180 nix run . -- --version </dev/null 2>/dev/null | head -1)
note "nix run . -- --version → ${run_version:-（nothing）}"
case "$run_version" in
    *0.1.0*) ;;
    *) fail "\`nix run .\` does not reach a working plainly (got: ${run_version:-nothing})" ;;
esac
cli_version=$(timeout 180 nix run .#plainly -- --version </dev/null 2>/dev/null | head -1)
note "nix run .#plainly -- --version → ${cli_version:-（nothing）}"
case "$cli_version" in
    *0.1.0*) ;;
    *) fail "\`nix run .#plainly\` does not reach a working plainly (got: ${cli_version:-nothing})" ;;
esac

# --- the default package carries all three binaries and the desktop entry ----
echo "== the default package's contents"
for bin in plainly plainly-panel plainly-desktop; do
    [ -x "$out_all/bin/$bin" ] || fail "default has no bin/$bin"
    note "bin/$bin"
done
ls "$out_all/share/applications/"*.desktop >/dev/null 2>&1 \
    || fail "default carries no .desktop entry"
note "$(ls "$out_all/share/applications/"*.desktop)"
# The size is read out of the icon rather than written here: a hard-coded size is
# a second place to keep in step with the file, and it would fail the day somebody
# redraws the icon bigger for no reason.
icon_size=$(python3 - "$out_all/share/icons/hicolor" <<'PYEOF'
import pathlib, struct, sys
root = pathlib.Path(sys.argv[1])
icons = sorted(root.glob("*/apps/dev.plainly.app.png"))
if not icons:
    print("")
    raise SystemExit
png = icons[0].read_bytes()
print("{}x{}".format(*struct.unpack(">II", png[16:24])))
PYEOF
)
[ -n "$icon_size" ] || fail "default carries no icon under share/icons/hicolor/*/apps/dev.plainly.app.png"
note "icon present at ${icon_size}"

# --- the CLI package is the CLI only ----------------------------------------
echo "== the CLI package carries one binary and no GUI libraries"
[ -x "$out_cli/bin/plainly" ] || fail "plainly has no bin/plainly"
for unwanted in plainly-panel plainly-desktop; do
    [ ! -e "$out_cli/bin/$unwanted" ] \
        || fail "plainly ($out_cli) carries $unwanted: the split did not happen"
done
# The words are the libraries a graphics stack arrives as in nixpkgs, and the
# names its *sources* have: `wayland` catches both the C library and the
# `wayland-1.26.0` package, which a `libwayland` pattern would miss.
graphical='gtk|webkit|gdk|cairo|pango|at-spi|wayland|layer-shell|soup|glib-|dbus'
cli_closure=$(nix path-info -r "$out_cli")
if echo "$cli_closure" | grep -Ei "$graphical" >/dev/null; then
    echo "$cli_closure" | grep -Ei "$graphical" >&2
    fail "the plainly closure reaches a graphics library"
fi
note "runtime closure is graphics-free"

# A buildInput that never reaches the closure still has to be substituted or built
# on the machine installing this, so the derivation's own inputs are the honest
# place to look — this is the check that caught a shared `buildInputs` handing the
# CLI the whole GTK stack while its runtime closure looked clean.
cli_drv=$(nix path-info --derivation "$out_cli")
drv_inputs=$(nix-store --query --references "$cli_drv")
if echo "$drv_inputs" | grep -Ei "$graphical" >/dev/null; then
    echo "$drv_inputs" | grep -Ei "$graphical" >&2
    fail "the plainly *build* reaches a graphics library"
fi
note "build inputs are graphics-free ($(echo "$drv_inputs" | wc -l) direct)"

# --- the split is real, not just two names ----------------------------------
echo "== the two closures differ"
all_closure=$(nix path-info -r "$out_all")
if [ "$(echo "$cli_closure" | wc -l)" -ge "$(echo "$all_closure" | wc -l)" ]; then
    fail "the plainly closure is not smaller than the default one"
fi
note "plainly $(echo "$cli_closure" | wc -l) paths, default $(echo "$all_closure" | wc -l) paths"

# --- the GUI binaries load their libraries without the dev shell ------------
# This is the ticket's whole point: a compositor's key binding spawns the panel
# with an environment of its own, and the binary has to carry its own answer to
# "where is GTK". `env -i` strips everything, including any LD_LIBRARY_PATH a
# caller happened to have, so whatever still resolves came from the build.
#
# The argument is one the binary refuses (`--help` is not a flag it has). The
# refusal happens after the dynamic linker has mapped the GTK stack, so a missing
# library shows up here while nothing ever opens a display — which is what makes
# this safe to run on someone's live session, where the panel would otherwise
# take the keyboard.
echo "== the GUI binaries start with no environment"
for bin in plainly-panel plainly-desktop; do
    out_err=$(env -i "$out_all/bin/$bin" --help 2>&1 >/dev/null || true)
    note "$bin → $(printf '%s' "$out_err" | head -1)"
    case "$out_err" in
        *"error while loading shared libraries"*)
            fail "$bin cannot find its libraries: $(printf '%s' "$out_err" | head -1)"
            ;;
    esac
    case "$out_err" in
        *"usage: $bin"*) ;;
        *) fail "$bin did not reach its own argument handling: ${out_err:-no output}" ;;
    esac
done

# The refusals above load the two libraries the binaries link against directly,
# but GTK opens the rest lazily, when a widget first needs them. `ldd` is the
# check that asks the linker for *every* library in the set the wrapper's
# `LD_LIBRARY_PATH` has to satisfy, so it is the one that would catch a
# transitive gap the running process has not reached yet. The wrapped binary is
# the ELF file; `bin/plainly-panel` is the shell script around it.
echo "== the GUI binaries resolve every library"
for bin in plainly-panel plainly-desktop; do
    missing=$(env -i ldd "$out_all/bin/.$bin-wrapped" 2>/dev/null | grep 'not found' || true)
    [ -z "$missing" ] || { printf '%s\n' "$missing" >&2; fail "$bin has unresolved libraries"; }
    note "$bin: $(env -i ldd "$out_all/bin/.$bin-wrapped" 2>/dev/null | grep -c '=>') libraries, none missing"
done

# --- the CLI reads a Passage with no dev shell ------------------------------
echo "== the CLI reads a Passage with no environment"
# `HOME` is set and everything else is not: the CLI needs a home to look for a
# store in, and pointing it at the cache directory keeps it from touching the
# real config while still exercising the no-key path.
echo "The manager had already gone to ground." \
    | env -i HOME="$NIX_CACHE_HOME" "$out_cli/bin/plainly" \
        >"$NIX_CACHE_HOME/cli.out" 2>"$NIX_CACHE_HOME/cli.err" || true
note "stderr: $(head -1 "$NIX_CACHE_HOME/cli.err")"
if grep -q "error while loading shared libraries" "$NIX_CACHE_HOME/cli.err"; then
    fail "the CLI cannot find its libraries: $(head -1 "$NIX_CACHE_HOME/cli.err")"
fi
# Without a key this is a refusal that names the fix (exit 1), which is the
# proof the binary ran: a dynamic-linker failure would never reach this sentence.
if ! grep -q "no API key" "$NIX_CACHE_HOME/cli.err"; then
    fail "the CLI did not reach its provider check: ${NIX_CACHE_HOME}/cli.err"
fi
note "refused with a usable sentence, no dynamic-linker error"

echo "OK: the flake packages are buildable, split and runnable"
