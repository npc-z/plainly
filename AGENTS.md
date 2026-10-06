## Agent skills

### Issue tracker

Issues and specs live as markdown files under `.scratch/<feature-slug>/`. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical triage roles map to labels of the same name (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.

## Environment

The development environment is the flake in this repo: `nix develop` provides the toolchain, the
libraries and the helper tools. CI runs the same shell.

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
