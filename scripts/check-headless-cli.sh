#!/usr/bin/env bash
#
# The `plainly` CLI must build and run with no graphics stack. That is the whole
# reason the workspace is split into crates, and it is easy to break by adding a
# dependency to plainly-core.
#
# Usage:
#   scripts/check-headless-cli.sh            # debug build
#   scripts/check-headless-cli.sh --release  # release build
#
# Ticket 17 wires the same check into CI, where the job must build
# `-p plainly-cli` rather than the whole workspace.
set -euo pipefail

cd "$(dirname "$0")/.."

profile_flag=""
if [ "${1:-}" = "--release" ]; then
    profile_flag="--release"
fi

graphical='gtk|webkit|gdk-|libwayland|soup|glib-2'

echo "== dependencies reachable from plainly-cli"
if cargo tree -p plainly-cli --edges normal | grep -Ei "$graphical"; then
    echo "FAIL: the CLI's dependency graph reaches a graphics library" >&2
    exit 1
fi
echo "   none"

echo "== building the CLI on its own"
# Ask cargo where it put the binary rather than assuming target/debug: a
# hardcoded path would quietly check a stale artifact after a --release build,
# and the check would pass on the wrong file. The JSON is parsed as JSON (node
# is already part of this dev shell for the desktop frontend) rather than with a
# regex: a path containing a quote, or an `"executable":null` line for the
# lib target, is not worth being clever about.
if ! command -v node >/dev/null 2>&1; then
    echo "FAIL: node is needed to read cargo's JSON output and is not on PATH" >&2
    exit 1
fi
executable=$(
    cargo build -p plainly-cli $profile_flag --message-format=json | node -e '
        let input = "";
        process.stdin.on("data", (chunk) => { input += chunk; });
        process.stdin.on("end", () => {
            let found = "";
            for (const line of input.split("\n")) {
                if (!line.trim()) continue;
                let message;
                try { message = JSON.parse(line); } catch { continue; }
                if (message.reason === "compiler-artifact" && message.executable) {
                    found = message.executable;
                }
            }
            process.stdout.write(found);
        });
    '
)
if [ -z "$executable" ]; then
    echo "FAIL: cargo did not report a built executable for plainly-cli" >&2
    exit 1
fi
if [ ! -x "$executable" ]; then
    echo "FAIL: $executable is not an executable file" >&2
    exit 1
fi
echo "   $executable"

echo "== shared libraries the CLI needs at run time"
# `ldd` is a glibc tool. The dependency-graph check above is the invariant this
# script exists for; the linkage check is a second opinion, so say when it could
# not be taken rather than pretending it passed.
if command -v ldd >/dev/null 2>&1; then
    needed=$(ldd "$executable")
    echo "$needed" | sed 's/^/   /'
    if echo "$needed" | grep -Ei "$graphical"; then
        echo "FAIL: the CLI links a graphics library" >&2
        exit 1
    fi
else
    echo "   SKIPPED, NOT VERIFIED: no ldd on this system, so the linkage"
    echo "   check did not run. The dependency-graph check above still passed."
fi

echo "== smoke"
"$executable" --version
echo "OK: plainly-cli is headless"
