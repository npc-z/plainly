## Agent skills

### Issue tracker

Issues and specs live as markdown files under `.scratch/<feature-slug>/`. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical triage roles map to labels of the same name (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.

## Environment

The development environment is the flake in this repo: `nix develop` provides the toolchain, the
libraries and the helper tools. That is the environment everything is built and checked in.

CI is the one exception, and it is a narrow one: `.github/workflows/ci.yml` runs
`cargo test -p plainly-core` and `cargo test -p plainly-cli`, plus clippy over both, on a plain Rust
toolchain. Those are the crates that must build with no graphics stack (spec §2, §14), and proving
that should not need Nix. The panel and the main window — the crates that do need the flake — have no
CI job; their coverage is the manual checklist in `docs/manual-checks.md`.

### Generated build data

`nix/cli-workspace/{Cargo.toml,Cargo.lock}` is generated, not hand-written: it is the root
workspace's CLI subset (spec §2), produced by `scripts/generate-cli-workspace.sh` from the root
`Cargo.toml`/`Cargo.lock`. It is committed, and `nix build` only reads it — changing a dependency
means regenerating it inside `nix develop` and committing the result. `nix build` does not notice
most staleness (a moved root-lock version still satisfies `--locked`), so run
`scripts/check-cli-workspace.sh` after a dependency change; it is offline and takes under a second,
and `docs/manual-checks.md` §0 carries the workflow.

### Never search `/nix/store` by hand

- **Do not walk `/nix/store` with `find`, `grep`, `ls`, `du` or similar** to locate a program, a
  library or a version. It is a content-addressed store with hundreds of thousands of entries: the
  scan is slow, the paths it prints are hashes that change on the next update, and anything that
  hard-codes one breaks the moment `flake.lock` moves.
- **Ask Nix instead.** `nix develop -c <cmd>` runs a command from the dev shell; `nix shell
  nixpkgs#<pkg> -c <cmd>` fetches a one-off tool; `nix eval .#<attr>` reads an attribute;
  `nix path-info` and `nix why-depends` inspect a store path once you have it from Nix rather than
  from a scan; `nix-locate` / `nix-index` answer "which package owns this file"; `nix search`
  answers "which package provides this program".
- **When a program seems to be missing, the fix is `flake.nix`**, not a store path: add it to the dev
  shell so everyone gets it, and so the answer survives the next lock-file update.
