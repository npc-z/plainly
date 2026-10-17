#!/usr/bin/env bash
#
# Guards the property the crane split exists for: editing a Rust source file
# must not invalidate the dependency-cache derivations.
#
# A `.rs` edit changes `wholeSource`, but crane keys its dependency derivations
# on the manifests and lockfiles alone, and nixpkgs' `fetchCargoVendor` folds
# the vendored sources into a fixed-output path — so the dependency *output*
# stays put even though the `.drv` paths move. The output path is what the
# cache reuses, so that is what this compares. The two packages carry different
# locks: the GUI uses the repo `Cargo.lock`, the CLI's synthetic workspace uses
# the hand-synced `cliLock` literal in flake.nix.
#
# This is a manual check, run after a flake-input bump is committed: CI has no
# Nix on purpose (AGENTS.md), and this is the one property that would silently
# rot into "every edit rebuilds every dependency".
#
# Usage:
#   scripts/check-incremental-build.sh             # the guard; expects no change
#   scripts/check-incremental-build.sh --self-test # proves both checks are live
#
# It only evaluates — nothing is built. Both trees come from `git archive HEAD`,
# so uncommitted changes do not participate: commit the bump first, or the guard
# checks the inputs you had before it.
set -euo pipefail

cd "$(dirname "$0")/.."

# A nix whose fetcher cache is writable. The default one lives in `~/.cache/nix`,
# which is read-only in some sandboxes; the same workaround as
# `scripts/check-flake-packages.sh`.
: "${NIX_CACHE_HOME:=${TMPDIR:-/tmp}/plainly-nix-cache}"
export NIX_CACHE_HOME
mkdir -p "$NIX_CACHE_HOME"

usage() {
    echo "usage: $0 [--self-test]" >&2
    exit 2
}

fail() {
    echo "FAIL: $*" >&2
    exit 1
}

self_test=false
case "${1:-}" in
    "") ;;
    --self-test) self_test=true ;;
    *) usage ;;
esac

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

mkdir -p "$work/base"
git archive HEAD | tar -x -C "$work/base"
cp -a "$work/base" "$work/edited"

if $self_test; then
    # One lockfile-level control per package, because each package's cache key
    # carries a different lock: the GUI's dependency build drops the repo
    # `Cargo.lock` into crane's dummified source, while the CLI's synthetic
    # workspace carries the hand-synced `cliLock` literal inside flake.nix.
    # A `.rs` edit is what neither must notice.
    printf '\n# incremental-build self-test\n' >>"$work/edited/Cargo.lock"
    python3 - "$work/edited/flake.nix" <<'PYEOF'
import sys

path = sys.argv[1]
text = open(path).read()
start = text.index('cliLock = pkgs.writeText "plainly-cli-Cargo.lock" \'\'')
end = text.index("\n        '';", start)
open(path, "w").write(text[:end] + "\n          # incremental-build self-test" + text[end:])
PYEOF
else
    printf '\n// incremental-build guard: this line must not reach the dependency cache\n' \
        >>"$work/edited/crates/plainly-core/src/lib.rs"
fi

# Print the name of the first input derivation of `$1` whose basename matches
# the regular expression `$2`.
drv_input() {
    nix derivation show "$1" | python3 -c '
import json, re, sys

drv = next(iter(json.load(sys.stdin)["derivations"].values()))
print(next(name for name in drv["inputs"]["drvs"] if re.search(sys.argv[1], name)))
' "$2"
}

# Print the store path of the single output of the derivation at `$1`.
drv_output() {
    nix derivation show "$1" | python3 -c '
import json, sys

drv = next(iter(json.load(sys.stdin)["derivations"].values()))
print(drv["outputs"]["out"]["path"])
'
}

# The dependency derivation's output path for one package. `default` is a
# `symlinkJoin`, so the GUI derivation is one hop down; each package's
# dependency build is then its own `-deps-` input of that derivation.
deps_output() { # <flake dir> <package> <dependency-derivation pattern>
    local flake=$1 pkg=$2 pattern=$3 drv

    drv=$(nix eval --raw "path:$flake#packages.x86_64-linux.$pkg.drvPath")
    if [ "$pkg" = default ]; then
        drv="/nix/store/$(drv_input "$drv" 'plainly-desktop-[0-9]')"
    fi
    drv="/nix/store/$(drv_input "$drv" "$pattern")"
    drv_output "$drv"
}

invalidated=0

check() { # <package> <dependency-derivation pattern>
    local pkg=$1 pattern=$2 before after

    before=$(deps_output "$work/base" "$pkg" "$pattern")
    after=$(deps_output "$work/edited" "$pkg" "$pattern")
    if [ "$before" = "$after" ]; then
        echo "ok   $pkg dependency output unchanged: $before"
    else
        echo "FAIL: $pkg dependency output changed" >&2
        echo "      before: $before" >&2
        echo "      after:  $after" >&2
        invalidated=$((invalidated + 1))
    fi
}

check plainly 'plainly-deps-[0-9]'
check default 'plainly-desktop-deps-[0-9]'

if $self_test; then
    if [ "$invalidated" -eq 2 ]; then
        echo "self-test ok: both dependency caches noticed their own lockfile change"
        exit 0
    fi
    fail "self-test: only $invalidated of 2 dependency caches noticed its lockfile change, so a dead check would go unnoticed"
fi

if [ "$invalidated" -ne 0 ]; then
    fail "the crane dependency cache is no longer independent of source edits"
fi
echo "ok: a source edit leaves both dependency outputs in place"
