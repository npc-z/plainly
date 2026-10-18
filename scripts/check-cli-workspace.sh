#!/usr/bin/env bash
#
# Guards the property the generation in `scripts/generate-cli-workspace.sh`
# would otherwise only state in a comment: the committed CLI workspace
# (`nix/cli-workspace/Cargo.toml` and `Cargo.lock`) still agrees with the root
# `Cargo.toml` and `Cargo.lock`.
#
# Why a guard exists at all: `cargo --locked` inside `nix build` catches most
# staleness (a changed requirement fails the lock). It does not catch the quiet
# one — a root `cargo update` moving a shared dependency to a new version while
# the committed CLI lock keeps the old one, which still satisfies the
# requirement and so passes `--locked`. That is the check this script exists
# for; the manifest checks say whether the committed projection is the one the
# generator writes.
#
# The projection rule itself lives in `scripts/cli_workspace.py`, shared with
# the generator, so the two cannot drift. This script is deliberately cheap:
# pure python3, offline, no Nix and no cargo — the normalisation cargo performs
# is only needed to *regenerate* the data, not to ask whether it is stale.
#
# Usage:
#   scripts/check-cli-workspace.sh             # the guard; expects no change
#
# It reads the files on disk (the worktree), not HEAD: stage or commit the root
# manifests/lock and `nix/cli-workspace/` before trusting a green run.
set -euo pipefail

cd "$(dirname "$0")/.."

usage() {
    echo "usage: $0" >&2
    exit 2
}

fail() {
    echo "FAIL: $*" >&2
    exit 1
}

if [ "$#" -ne 0 ]; then
    usage
fi

command -v python3 >/dev/null || fail "python3 is not on PATH"
python3 -c 'import tomllib' 2>/dev/null || fail "python3 has no tomllib (needs 3.11+)"

python3 <<'PYEOF'
import os
import sys
import tomllib

# The guard imports the shared projection; do not leave `scripts/__pycache__/`
# behind in a checkout this script is supposed to leave untouched.
sys.dont_write_bytecode = True

# The projection rule is shared with the generator; `scripts/` is on the path
# because both scripts start from the repository root.
sys.path.insert(0, os.path.join(os.getcwd(), "scripts"))
import cli_workspace

manifest_path = "nix/cli-workspace/Cargo.toml"
lock_path = "nix/cli-workspace/Cargo.lock"


def report(message):
    print(f"FAIL: {message}", file=sys.stderr)


def note(message):
    print(f"   {message}", file=sys.stderr)


def load(path):
    with open(path, "rb") as handle:
        return tomllib.load(handle)


try:
    committed_manifest = load(manifest_path)
except (FileNotFoundError, tomllib.TOMLDecodeError) as error:
    report(f"cannot read {manifest_path}: {error}")
    print("run: bash scripts/generate-cli-workspace.sh", file=sys.stderr)
    sys.exit(1)
try:
    committed_lock = load(lock_path)
except (FileNotFoundError, tomllib.TOMLDecodeError) as error:
    report(f"cannot read {lock_path}: {error}")
    print("run: bash scripts/generate-cli-workspace.sh", file=sys.stderr)
    sys.exit(1)

try:
    expected = cli_workspace.manifest_projection()
    inherited = cli_workspace.inherited_dependency_names()
except KeyError as error:
    # A bug in the root manifest, not in the committed projection.
    report(error.args[0])
    sys.exit(1)

expected_workspace = expected["workspace"]
actual_workspace = committed_manifest.get("workspace", {})
committed_dependencies = actual_workspace.get("dependencies", {})
inherited_set = set(inherited)
missing = [name for name in inherited if name not in committed_dependencies]
unused = sorted(name for name in committed_dependencies if name not in inherited_set)
changed_values = [
    name
    for name in inherited
    if name in committed_dependencies
    and committed_dependencies[name] != expected_workspace["dependencies"].get(name)
]

failures = []

# 1. The manifest's shape and its dependency *declarations* are the projection;
#    presence and extra names are checks 3 and 4, so each failure names exactly
#    one fact instead of restating the same broken manifest three times.
manifest_ok = True
extra_sections = sorted(set(committed_manifest) - {"workspace"})
if extra_sections:
    manifest_ok = False
    note("extra top-level sections: " + ", ".join(extra_sections))
for key in ("resolver", "members", "package"):
    if actual_workspace.get(key) != expected_workspace[key]:
        manifest_ok = False
        note(f"[workspace.{key}] differs from the root Cargo.toml")
if changed_values:
    manifest_ok = False
    note(
        "[workspace.dependencies] declarations differ from the root Cargo.toml: "
        + ", ".join(sorted(changed_values))
    )
if manifest_ok:
    print("ok   the CLI manifest's shape and dependency declarations are the projection")
else:
    failures.append(f"{manifest_path} is not the projection of the root Cargo.toml")

# 2. The committed lock agrees with the root lock: its versions are pinned by
#    it, it holds nothing outside the root lock's CLI closure, and every
#    dependency edge resolves inside the lock. (Whether the *requirements* still
#    match the manifests is `cargo --locked`'s job at build time.)
root_packages = cli_workspace.load("Cargo.lock")["package"]
root_versions = {}
for package in root_packages:
    root_versions.setdefault(package["name"], set()).add(package["version"])
committed_versions = {}
for package in committed_lock["package"]:
    committed_versions.setdefault(package["name"], set()).add(package["version"])
try:
    closure = {
        (package["name"], package["version"])
        for package in cli_workspace.root_lock_closure(root_packages)
    }
except KeyError as error:
    failures.append(f"the root Cargo.lock cannot be walked from the CLI crates: {error.args[0]}")
    closure = set()
drifted = sorted(
    f"{package['name']} {package['version']}"
    for package in committed_lock["package"]
    if package["version"] not in root_versions.get(package["name"], set())
)
outside = sorted(
    f"{package['name']} {package['version']}"
    for package in committed_lock["package"]
    if (package["name"], package["version"]) not in closure
)
dangling = []
for package in committed_lock["package"]:
    for entry in package.get("dependencies", []):
        name, version = cli_workspace.dependency_name_version(entry)
        versions = committed_versions.get(name)
        if not versions:
            dangling.append(f"{package['name']} -> {entry!r} (no such package)")
        elif version is not None and version not in versions:
            dangling.append(f"{package['name']} -> {entry!r} (no such version)")
if drifted:
    failures.append(
        "the CLI lock pins versions the root Cargo.lock does not have: "
        + ", ".join(drifted)
    )
if outside:
    failures.append(
        f"{lock_path} holds packages outside the root lock's CLI closure: "
        + ", ".join(outside)
    )
if dangling:
    failures.append(
        f"{lock_path} has dependency edges that do not resolve: "
        + ", ".join(sorted(set(dangling)))
    )
if not (drifted or outside or dangling):
    print("ok   every CLI lock version, package and edge agrees with the root lock")

# 3. Every dependency the crates inherit is covered by the committed manifest.
if missing:
    failures.append(
        f"{manifest_path} is missing [workspace.dependencies] for: "
        + ", ".join(sorted(missing))
    )
else:
    print("ok   every workspace-inherited dependency is declared")

# 4. And the manifest carries no dependency no crate uses.
if unused:
    failures.append(
        f"{manifest_path} declares workspace dependencies no crate uses: "
        + ", ".join(unused)
    )
else:
    print("ok   the manifest declares no unused workspace dependency")

if failures:
    for failure in failures:
        report(failure)
    print("run: bash scripts/generate-cli-workspace.sh", file=sys.stderr)
    sys.exit(1)
PYEOF
