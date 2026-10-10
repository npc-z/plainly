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
# and the check would pass on the wrong file. The JSON is parsed as JSON rather
# than with a regex: a path containing a quote, or an `"executable":null` line
# for the lib target, is not worth being clever about.
#
# `python3` rather than `node`, and because of CI rather than because of this
# shell: the CI job runs on a plain Ubuntu runner, where `python3` is part of
# the base image but a `node` is not guaranteed to be. Reading a line of JSON is
# not worth a toolchain dependency in either place.
if ! command -v python3 >/dev/null 2>&1; then
    echo "FAIL: python3 is needed to read cargo's JSON output and is not on PATH" >&2
    exit 1
fi
executable=$(
    cargo build -p plainly-cli $profile_flag --message-format=json | python3 -c '
import json, sys

found = ""
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        message = json.loads(line)
    except json.JSONDecodeError:
        continue
    if message.get("reason") == "compiler-artifact" and message.get("executable"):
        found = message["executable"]
sys.stdout.write(found)
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
