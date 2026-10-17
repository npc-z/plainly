# Incremental / cached Rust compilation under Nix — findings

Date: 2026-10-14. Local source: pinned nixpkgs `/nix/store/pgp7knjgswsdv1mzgdhx21v8zmdkbisy-source`
(`.version` = **26.11**, `flake.lock` rev `494ce7fd23ff6a5dff39e1fb11e9b6f2ac74bf25`, nixos-unstable).
Repo toolchain in the dev shell: `rustc 1.101.0-nightly (282215592 2026-10-04)` via fenix.
No repo files were modified by this investigation (this scratch note is the only file written).

Confidence: **H** = verified against local source or primary upstream artifact, **M** = secondary
source only, **L** = weak/inferred, **UNVERIFIED** = could not confirm.

---

## 1. Does nixpkgs `buildRustPackage` support `cargoArtifacts`? — **NO. Confidence: H**

- `pkgs/build-support/rust/build-rust-package/default.nix` has no `cargoArtifacts` argument. Its
  dependency-related args are exactly: `cargoDeps`, `cargoVendorDir`, `cargoLock`, `cargoPatches`,
  `cargoDepsName`, `cargoUpdateHook`, `cargoDepsHook`, `useFetchCargoVendor`, `depsExtraArgs`.
- `cargoDeps` is **vendored source only**. `fetch-cargo-vendor.nix` returns a `runCommand` producing a
  `cargo vendor`-style tree, and `cargo-setup-hook.sh` does `cp -Lr "$cargoDeps" "$dest"` /
  `unpackFile "$cargoDeps"` in `postUnpack`, then writes `.cargo/config.toml` with
  `[source.crates-io] replace-with = "vendored-sources"`. No compiled artifacts, no `target/`.
- `grep -rn cargoArtifacts pkgs/` over the whole tree returns **one** hit, unrelated:
  `pkgs/by-name/md/mdfried/package.nix:36` ("read-only cargoArtifacts files" — a local comment).
- `cargo-build-hook.sh` runs `cargo build ... --offline` in `$(pwd)`; `cargo-install-hook.sh` copies
  only `$out/bin` (executables) and `$out/lib` (`*.so*`, `*.a`, `*.dylib`) out of `target/<target>/<type>`.
  The `target/` directory itself is never installed. `CARGO_TARGET_DIR` appears **once** in the whole
  Rust support tree (`cargo-build-hook.sh:14`, only to pull the target dir up one level when
  `buildAndTestSubdir` is set).
- `make-rust-platform.nix` exposes: `fetchCargoVendor`, `buildRustPackage`, `importCargoLock`,
  `rustcSrc`, `rustLibSrc`, `rustVendorSrc`, `cargoBuildHook`, `cargoCheckHook`, `cargoInstallHook`,
  `cargoNextestHook`, `cargoSetupHook`, `maturinBuildHook`, `bindgenHook`, `rust.{rustc,cargo}`.
  **No `cargoArtifacts`, no `buildDepsOnly`.**

### What nixpkgs *does* expose for Q7

| Hook / var | Where | What it gives you |
|---|---|---|
| `cargoDeps` | `buildRustPackage` arg | vendored **source** tree |
| `cargoVendorDir` | `buildRustPackage` arg | alternative vendor dir path (`null` disables) |
| `cargoSetupPostUnpackHook` | `cargo-setup-hook.sh` | runs `eval "${cargoDepsHook-}"`, copies vendor tree, writes `.cargo/config.toml` |
| `cargoBuildHook` / `cargoBuildFlags` | `cargo-build-hook.sh` | default `buildPhase`; **`buildPhase = cargoBuildHook` only if `buildPhase` is unset and `dontCargoBuild` unset** |
| `cargoCheckHook` | `cargo-check-hook.sh` | default `checkPhase`, runs `cargo test --offline` |
| `cargoInstallHook` | `cargo-install-hook.sh` | default `installPhase` only if `installPhase` unset; also adds `postBuildHooks+=(cargoInstallPostBuildHook)` |
| `dontCargoBuild` / `dontCargoCheck` / `dontCargoInstall` | hooks | suppress each default phase |
| `cargoBuildType`, `cargoBuildFlags`, `cargoBuildFeatures`, `cargoBuildNoDefaultFeatures` | `buildRustPackage` | wired into `cargoBuildHook` |
| `CARGO_TARGET_DIR` | *not set by nixpkgs* (only in the `buildAndTestSubdir` branch) | you must set it yourself |

---

## 2. `crane` (github.com/ipetkov/crane) — **maintained. Confidence: H (facts), M (limitation impact)**

- **Status:** latest release **v0.24.0, 2026-08-21** (verified via `releases.atom`); releases also
  v0.23.4 (2026-05-17), v0.23.3, v0.23.2, v0.23.1, v0.23.0. No archive/deprecation banner; README
  documents a compatibility policy ("breaking changes can land on `master` at any time").
- **Core mechanism:** `buildDepsOnly` substitutes `src = mkDummySrc args.src` so the derivation hash
  covers only `Cargo.lock` + cleaned manifests + `.cargo` config — the crate's real code is replaced
  by empty stubs. It runs `cargo check --all-targets`, `cargo build`, and (check phase)
  `cargo test --no-run`, then `installCargoArtifactsHook.sh` writes the whole `target/` to `$out` as
  `target.tar.zst` (`installCargoArtifactsMode = "use-zstd"`, deterministic `--mtime=@1`).
  Downstream derivations take `cargoArtifacts = <that drv>`; `inheritCargoArtifactsHook.sh`
  (a `postPatchHook`) restores it into `CARGO_TARGET_DIR`/`target`, and `buildPackage` then compiles
  only the real crate. `buildPackage` defaults `cargoArtifacts` to an inline `buildDepsOnly`.
- **Known limitations (documented in crane's FAQ + tracker):**
  - Dev-deps **are** cached by default (`doCheck = true` → `--all-targets`); test *binaries* are
    deliberately excluded from installs and can't easily be exported (issues #600, #973 open).
  - Any **manifest-level** change (new bin target with autobin, workspace member version bump,
    feature/dep change) invalidates **all** deps (PR #974 open). Unfiltered `src` was itself a cache
    bug (#351).
  - Dummified sources break deps that need real files: `include_str!`/`include_bytes!`, build scripts
    reading sibling files, workspace-invoking tools. `buildPackage` deliberately does not run
    caller hooks during its implicit `buildDepsOnly`.
  - `cargoLlvmCov` ignores `cargoArtifacts` and rebuilds everything (#1075, open).
  - `use-symlink` inheritance writes into store paths and breaks if cargo spuriously invalidates a
    dep (#385, open); `remapPathPrefixHook` is path-dependent and was made opt-in in v0.23.0 partly
    because outside NixOS the build path differs and cargo recompiles all deps.
  - **Vendoring names:** crane uses `cargoVendorDir` / `vendorCargoDeps` / `outputHashes`. There is
    **no `doNotVendorCargoDeps`** and **no `vendorHash`** in crane (those are nixpkgs-isms) —
    a correction to common hearsay.

Sources: <https://github.com/ipetkov/crane/releases> · <https://crane.dev/API.html> ·
<https://github.com/ipetkov/crane/blob/master/lib/buildDepsOnly.nix> · crane FAQ `constant-rebuilds.md`,
`building-with-non-rust-includes.md`, `control-when-hooks-run.md` · issues #385, #600, #873, #973, #1075.

## 3. `naersk` — **alive but slow; NOT deprecated. Confidence: H**

- **Status:** last commit `9aa07bb0`, **2026-06-23**; 0 releases, 0 tags (tracked request in #394).
  README says only "Status: project's working!". The 2025-07-28 Discourse thread titled
  ["Naersk for Rust — deprecation notice"](https://discourse.nixos.org/t/naersk-for-rust-deprecation-notice/67322)
  is **not** a deprecation: it says support for "some legacy versions of Nix and for some old nixpkgs
  versions" was dropped. Do not report it as deprecated.
- **Mechanism:** no IFD (parses `Cargo.toml`/`Cargo.lock` with `builtins.fromTOML`). Registry crates in
  `Cargo.lock` are fetched one-by-one from `static.crates.io`, unpacked, given synthetic
  `.cargo-checksum.json`, `lndir`-joined, and exposed via a generated `CARGO_HOME/config.toml` source
  replacement. `buildPackage` makes `<name>-deps` from `dummySrc` (strips bin/example/lib/test/bench
  from every member `Cargo.toml`, stubs `src/lib.rs` and `build.rs`), tars `target/` to
  `$out/target.tar.zst`, and the real build unpacks it into local `target/`, `touch`es all sources,
  then `cargo build`. Caching unit = the whole dependency set.
- **Known issues:** all-or-nothing deps caching (#134); each mode/package gets its own deps derivation
  (#368); `mode = "test"` tests the dummy source, defeating two-step caching (#374); workspace support
  is weak (#379, #270, #336, #133); `CMakeCache.txt`-style path-dependent build scripts break stage 2
  (#285); stale docs mention `--out-dir`/`CARGO_TARGET_DIR` which it no longer uses (#202/#203).

## 4. `fenix` — **no build helper; toolchain only. Confidence: H**

The pinned fenix (`/nix/store/jkli7kpp7r040w9h6nxqhn5zb1w71bgv-source`) contains no `rustPlatform`,
`buildRustPackage`, `cargoArtifacts` or `buildDepsOnly` attribute. `grep` finds only two hits, both
*examples in prose*: `README.md:439` and `default.nix:172`, and both use
`(pkgs.makeRustPlatform { cargo = ...; rustc = ...; }).buildRustPackage` — i.e. **nixpkgs'**
`buildRustPackage` with the fenix toolchain passed in. fenix's README even shows a "building with
crane" example as the caching route. The repo's current arrangement (`pkgs.rustPlatform.buildRustPackage`
+ `pkgs.fenix.complete` toolchain, `flake.nix:2520`, `2534`) is therefore the intended one, and matches
your belief exactly: **fenix supplies the compiler, nothing else.**

## 5. `sccache` with Nix — **not viable in a pure `nix build`; works only as an impurity hole. Confidence: H (constraints), H (cc-rs correction)**

- Repeated `nix build` is normally repaved by the binary cache/store, so the reports that matter are
  failures: 2019 `discourse.nixos.org/t/3228` (needed `useSandbox = false`; second build no hits
  because `/tmp/nix-build-<name>-<ver>.drv-N` changes per attempt) and 2021
  `discourse.nixos.org/t/11138` (`buildRustPackage` cache "isn't created" under `nix-build`).
- Constraints: pure builds get private net/PID/UTS namespaces and loopback-only networking, so
  sccache's default `127.0.0.1:4226` daemon is unreachable from a *host* server; you need
  `SCCACHE_SERVER_UDS` plus `extra-sandbox-paths`. `SCCACHE_DIR` must be a bind-mounted dir outside
  the sandbox (`/homeless-shelter` by default); each build runs as a different `nixbld*` uid, so the
  cache must be group-writable (mode 0600 entries mean parallel builds under different uids miss each
  other). For **Rust**, the cache key includes the parsed arg vector and the **cwd** (sccache
  `src/compiler/rust.rs`), and `SCCACHE_BASEDIRS` is C/C++-only — so any store-hash/build-path change
  misses.
- **Correction to the premise (important):** sccache does *not* ignore C compilation, and cc-rs does
  *not* need ccache. cc-rs explicitly documents: "`RUSTC_WRAPPER` - if its file name is `sccache`,
  `cachepot`, `buildcache` or `kache` … `cc` prefixes it to the compiler command, unless `CC` already
  names a wrapper", and has a README section "Speed up compilation with sccache … set
  `RUSTC_WRAPPER=sccache`". So for the repo's `rusqlite` `bundled` SQLite the C compile *would* be
  routed through sccache automatically once `RUSTC_WRAPPER=sccache` is set.
  What genuinely is never cached: crates that invoke the system linker — **`bin`, `dylib`, `cdylib`,
  `proc-macro`** — plus incremental compilation must be **off** (sccache `docs/Rust.md`). For a repo
  like this, the *binaries are exactly the crates that can't be cached*; only lib/proc-macro and C
  objects could be.
- Sources: sccache README + `docs/Rust.md` + `docs/Local.md` +
  `src/compiler/{rust.rs,c.rs}` · cc-rs `src/lib.rs` · discourse threads 3228, 11138, 69658 ·
  nixpkgs#109033 (`-frandom-seed` keyed to the derivation hash defeats compiler caches).

## 6. Newer / notable (2025–2026) — confidence per item

| Tool | Status | Viable today? | Confidence |
|---|---|---|---|
| **crate2nix** | **Maintained** — 0.15.0 (2026-01-28), commits to 2026-09-28; one derivation per crate via `buildRustCrate` → true per-crate caching | **Yes**, the only per-crate path without experimental Nix. Cost: generated `Cargo.nix` or IFD; tests experimental; git deps in odd subdirs unsupported | H |
| **cargo2nix** | **Dormant** — last commit 2025-06-19, not archived, no notice | No for new work (generated `Cargo.nix`, stale pins) | H |
| **dream2nix** | Not archived, no notice, **de-facto maintenance-only** — last non-bot commit 2026-05-28; Rust backends listed under "Reference (experimental)" | No for Rust | M |
| **nix-cargo-integration** | Moved to `90-008/nix-cargo-integration`; flake-parts module; bot dep bumps only | No | M |
| **"cargo-nix"** | Name is essentially dead (`pandaman64/cargo-nix`, 2021). Live namesake: **`anthropics/cargo-nix-plugin`** (2026, 63★) — a Nix *plugin* providing `builtins.resolveCargoWorkspace` to feed `buildRustCrate` | Promising but needs a version-locked plugin via `plugin-files` → not usable in nixpkgs/flakes | M (existence H) |
| **nixpkgs `buildRustCrate`** | **Still present and maintained** in 26.11; no deprecation; documented as the "instead of Cargo" route | Yes as an engine (crate2nix drives it); not the idiomatic high-level path | H |
| **ca-derivations / dynamic-derivations** | Both still **experimental, off by default** in Nix 2.35.x; no stabilizing RFC | Marginal alone; `dynamic-derivations` is the real enabler | M |
| **`obsidiansystems/cargo-dyndrv`** | New (2026-08-27), headline project; uses cargo `--unit-graph` + dynamic derivations for per-crate derivations with no eval penalty | **Not yet** — needs `ca-derivations` + `dynamic-derivations` + `builder-rpc-v0` + a post-2.35 Nix daemon | M |
| **`figsoda/drowse`** | "dynamic derivations made easy", runs crate2nix *inside* a derivation; single maintainer | Young | M |

**Bottom line for production today:** whole-workspace = `crane` (best-in-class) or nixpkgs
`buildRustPackage`; per-crate = `crate2nix`. Everything else in this table is either dormant or
requires experimental Nix.

## 7. Minimal nixpkgs-only way to get `cargoArtifacts`-style reuse — **possible, with one crucial catch. Confidence: H (mechanism), M (fragility)**

**The catch first:** if your deps-only derivation uses the *same* `src`, `cargoDeps`, toolchain, env
and flags as the final build, you gain **nothing** — nix would have built the identical derivation
anyway, and you now build it twice. Reuse only pays if the deps derivation's **inputs are cheaper to
invalidate** than the real ones. That is what crane's `mkDummySrc` buys, and you must replicate it.
So the deps-only derivation needs a **dummified source** (workspace members' `src/` replaced by stub
`lib.rs`, as crane/naersk do), not the real source.

**Deps-only derivation** (real attributes are buildRustPackage args; `...` = your common args):

```nix
depsOnly = pkgs.rustPlatform.buildRustPackage (common // {
  pname = "myws-deps";
  version = "0.1.0";
  src = dummySrc;              # runCommand: copy Cargo.toml/Cargo.lock + stub src/lib.rs per member
  cargoDeps = myVendor;        # must be the SAME value the final build uses
  doCheck = false;             # or true to also cache dev-deps (cargo test --no-run)
  dontCargoInstall = true;     # suppress cargoInstallHook, which only copies bins/libs
  installPhase = ''
    runHook preInstall
    mkdir -p $out
    cp -r target $out/target        # <- the artifact handoff
    runHook postInstall
  '';
});
```

**Final derivation:**

```nix
final = pkgs.rustPlatform.buildRustPackage (common // {
  src = realSrc;               # real workspace
  cargoDeps = myVendor;        # same vendor tree
  preBuild = ''
    # unpack the cached target/ before the (default) cargoBuildHook runs
    rm -rf target
    cp -r ${depsOnly}/target target
    chmod -R u+w target                    # store paths are read-only
    find target -exec touch -h -d @1 {} +  # make cached artifacts look old
  '';
});
```

Exact names that matter: `buildPhase = cargoBuildHook` (default), `dontCargoInstall = true`,
`installPhase` override, `cargoBuildType`/`cargoBuildFlags`/`cargoBuildFeatures`,
`cargoSetupPostUnpackHook`/`cargoDepsHook`, and `CARGO_TARGET_DIR` (unset by nixpkgs — cargo's default
`target/` at `$NIX_BUILD_TOP` is what both derivations use).

**Accuracy notes / hazards:**

- nixpkgs exposes **no** `cargoArtifacts` or `buildDepsOnly`; you are hand-rolling crane's
  `inheritCargoArtifactsHook`. `cargoDeps` remains the vendored **source**, not artifacts.
- **Freshness:** cargo normally decides staleness by mtime, so the store round-trip is the main risk;
  hence the `touch`. The repo's nightly toolchain (1.101.0-nightly) also supports
  **`cargo -Z checksum-freshness`** ("Use a checksum to determine if output is fresh rather than
  filesystem mtime", verified locally) which removes the mtime hazard entirely — but it is a
  **nightly-only** flag, so a stable-toolchain build cannot use it.
- Fingerprints embed absolute paths (`CARGO_TARGET_DIR`, `--out-dir`, vendor dir), so any of those
  differing between deps and final builds costs a rebuild of the affected units. Keeping `src`
  unpacked at the same store name / using the same flags (`-p`, features) matters. `checkPhase`
  (`cargoCheckHook`) compiles test targets against the *real* source and is not cached by the deps-only
  build unless you dummify test targets too.
- Copying `target/` roughly doubles the output closure size, and the deps-only output is a fully
  compiled tree (tarball it with `zstd` if size matters).
- **Practical recommendation for this repo:** given the two derivations here already build the same
  workspace with the same vendor tree and toolchain, and the CLI/GUI differ by `--package` flags and
  `src` (separate `cliSource`), a hand-rolled deps-only split would mostly duplicate work. Adopting
  `crane` as an input is the low-risk route if real incremental caching is wanted; a
  nixpkgs-only version is real but is the kind of code crane exists to maintain.

---

## Update 2026-10-17

The decision that followed this investigation lives in [`../tickets/`](../tickets/): the toolchain
choice is **fenix pinned to 1.99.1 for both the dev shell and the package builds** (not nixpkgs'
rustc, and not the nightly this note was written against), and the caching route is crane
`buildDepsOnly`/`cargoArtifacts` rather than a hand-rolled split. The numbers this note records are
what the tickets cite as prototype evidence; the tickets are the current source of truth.
