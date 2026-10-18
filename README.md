# plainly

Make difficult English understandable in English. Press one key and the Passage
on your clipboard comes back as a leveled paraphrase, plain-English glosses of
what blocks it, an optional grammar note, and a translation — in a panel that
never takes focus, with the original still on screen to check it against. Every
Explanation lands in a local history that doubles as a cache, from a `plainly`
command line that also works over a pipe.

v0 is Linux-first: niri and the wlroots family get the whole path, GNOME is
expected to degrade in one documented way, and X11 is not supported — the command
line still is. `plainly-core` deliberately depends on no GTK, WebKit or Wayland,
so a headless machine can build and run the CLI.

## Build

Install it — the flake builds the binaries a compositor's key binding can spawn,
with their library paths written into the wrappers, and puts them on `PATH`:

```
nix build .                       # → result/bin/{plainly,plainly-panel,plainly-desktop}
nix profile install .             # the same three, into a profile
```

`nix profile install .#plainly` installs the command line alone, on a machine
with no graphics stack: that package's closure reaches no GTK, WebKit or Wayland.

Both packages are also pushed to a binary cache, and `just nix-build` passes
`--accept-flake-config` so a pushed revision substitutes instead of rebuilding.
A bare `nix build .` does not: Nix applies a flake's cache settings only when it
trusts the flake, which "Binary cache" below covers.

The development environment is the flake at the repository root, and it is for
people changing the code rather than for running it:

```
nix develop
cargo build --workspace
```

That gives the same three binaries in `target/debug/`, but they are *not* the
ones a key binding can spawn: the panel and the main window need the dev shell's
`LD_LIBRARY_PATH`, so they only run from inside `nix develop` or a shell that
sets it. Use the built ones from `result/bin` to check behaviour, and this shell
to rebuild after an edit.

The command line is built from a generated two-crate workspace, committed under
`nix/cli-workspace/`. After changing a dependency in the root `Cargo.toml` (and
letting the root `Cargo.lock` catch up), regenerate it inside the dev shell and
commit the result:

```
nix develop -c bash scripts/generate-cli-workspace.sh
bash scripts/check-cli-workspace.sh        # offline, <1 s; says "stale" if forgotten
```

`nix build` does not run the generator, and it only notices a stale copy when the
committed manifests stop satisfying the crates — a moved lock version or a changed
`[workspace.package]` builds fine and silently. `docs/manual-checks.md` §0 says
when it is needed; the check above is the reliable signal.

A machine without a graphics stack needs none of it:

```
cargo build -p plainly-cli        # the command line alone, with no graphics stack
cargo test -p plainly-core        # the domain logic the CLI is built on
cargo test -p plainly-cli         # the command line itself
```

Those two are what CI runs (`.github/workflows/ci.yml`), and deliberately not the
workspace: they are the crates that must not grow a graphics dependency. The
panel and the main window need GTK, WebKit and a compositor, so a runner is not
where they are checked — `docs/manual-checks.md` is.

## Use it from the flake

The flake is also the install path for someone who does not want to clone this
repository. With flakes enabled, the apps and packages are reachable through the
GitHub reference:

```
nix run github:npc-z/plainly -- --version            # the command line, smoke test
echo "The manager had already gone to ground." | nix run github:npc-z/plainly
nix run github:npc-z/plainly#desktop                 # the main window
nix run github:npc-z/plainly#panel -- --clipboard    # the panel, for the clipboard
nix profile install github:npc-z/plainly             # all three binaries
nix profile install github:npc-z/plainly#plainly     # the command line alone
```

`github:npc-z/plainly` follows the repository's default branch, so these commands
see whatever was last pushed there. A flake input in someone else's configuration
is pinned by their `flake.lock`; a bare command can be pinned to a revision:

```
nix run github:npc-z/plainly/<rev> -- --version
```

As an input, the packages are ordinary flake outputs. This NixOS fragment puts
the three binaries on `PATH`:

```nix
{
  nixConfig = {
    extra-substituters = [ "https://plainly-npc-z.cachix.org" ];
    extra-trusted-public-keys = [
      "plainly-npc-z.cachix.org-1:F7TqG/U7AJfUZ8McMct3VHbxchqKZSF/oSop0pv9/SE="
    ];
  };

  inputs.plainly.url = "github:npc-z/plainly";

  outputs = { self, nixpkgs, plainly, ... }:
    let
      system = "x86_64-linux";
    in
    {
      nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
        inherit system;
        modules = [
          ({ ... }: {
            environment.systemPackages = [ plainly.packages.${system}.default ];
          })
        ];
      };
    };
}
```

The `nixConfig` block is the consumer-side version of what the flake itself
declares: Nix applies it to this flake once it is trusted (`--accept-flake-config`
or the one-time prompt). The system-wide `nix.settings` form under "Binary cache"
needs no per-flake trust.

A home-manager user who wants only the command line writes
`home.packages = [ plainly.packages.x86_64-linux.plainly ];` instead, and anyone
building from another flake can name the same attributes. v0 exposes
`x86_64-linux` only, so there is no package for any other system.

The cache holds only what was pushed, for the revision and `flake.lock` that
produced it: a different nixpkgs input changes the derivations, and a miss is an
ordinary source build. A `nix run` or `nix profile install` against a pushed
revision substitutes the packages, while a revision that was never pushed builds
from source. "Binary cache" below is how to make sure the substitute is used.

## Binary cache

Builds are pushed to the public Cachix cache
[`plainly-npc-z`](https://app.cachix.org/cache/plainly-npc-z), and the flake
declares it in `nixConfig`:

```
extra-substituters         = https://plainly-npc-z.cachix.org
extra-trusted-public-keys = plainly-npc-z.cachix.org-1:F7TqG/U7AJfUZ8McMct3VHbxchqKZSF/oSop0pv9/SE=
```

Nix applies a flake's `nixConfig` only when it trusts the flake. Add
`--accept-flake-config` to the `nix` command that reads it:

```
nix build --accept-flake-config github:npc-z/plainly
nix run --accept-flake-config github:npc-z/plainly
nix profile install --accept-flake-config github:npc-z/plainly
```

or accept flake configuration globally — `accept-flake-config = true` in
`~/.config/nix/nix.conf` or `/etc/nix/nix.conf`, or on NixOS
`nix.settings.accept-flake-config = true`. Without either, Nix treats the
settings as untrusted: a non-interactive run prints `warning: ignoring
untrusted flake configuration setting ...` and builds from source, while an
interactive run asks first and saves the answer. Either way it is the same
result, only slower.

A flake's `nixConfig` belongs to the flake that is the top-level reference; it
does **not** travel with the flake as an input. If plainly is one input of
someone's NixOS or home-manager flake, they enable the cache in their own
settings:

```nix
nix.settings = {
  extra-substituters = [ "https://plainly-npc-z.cachix.org" ];
  extra-trusted-public-keys = [
    "plainly-npc-z.cachix.org-1:F7TqG/U7AJfUZ8McMct3VHbxchqKZSF/oSop0pv9/SE="
  ];
};
```

or run `cachix use plainly-npc-z` once on a machine that has Cachix.

### Filling it

The cache is public to read and token-authenticated to write. Authenticate once
with the token from
<https://app.cachix.org/personal-auth-tokens> (or a per-cache token from the
cache's settings):

```
cachix authtoken <token>          # or: export CACHIX_AUTH_TOKEN=<token>
```

Then, from a committed tree, after a build that changed anything:

```
just cache-push
```

Commit before pushing: a build of a dirty tree produces store paths that no
other checkout can name, so those entries are dead weight. The useful entry is
the one built from the commit a consumer's `github:npc-z/plainly/<rev>` points
at.

That recipe is `nix build --no-link --print-out-paths .#default .#plainly |
cachix push plainly-npc-z`. It pushes both packages' runtime closures — under a
gigabyte uncompressed together — which is what an install substitutes. It is
the whole automation: CI deliberately runs no Nix job (see `AGENTS.md`), so
pushes are manual and a revision without a push is not cached.

To also save the *dependency* builds — useful to someone rebuilding this flake's
source, and several gigabytes — push the build-time closure as Cachix documents:

```
nix-store -qR --include-outputs $(nix-store -qd $(nix build --no-link --print-out-paths .#default .#plainly)) \
  | grep -v '\.drv$' \
  | cachix push plainly-npc-z
```

Cachix's free tier for open-source projects is 5 GB; the packages fit well
inside it, which is why `just cache-push` starts there rather than pushing the
toolchain too.

## First run

1. **Pick a provider.** The default is `deepseek`, which needs a key.

   ```
   plainly providers key set deepseek      # reads the key from stdin
   ```

   The key never goes into the configuration file: it is resolved from
   `PLAINLY_DEEPSEEK_API_KEY`, then the OS keyring, then this session's memory —
   in that order. Everything else about the provider lives in
   `~/.config/dev.plainly.app/config.toml`, which `plainly config set` edits in
   place and keeps your comments in.

2. **Or point it at a model on your own machine.** Local runtimes take no key:

   ```
   plainly providers discover              # asks the common ports what they serve
   plainly providers use llamacpp <model>  # writes it into [providers.llamacpp]
   plainly config set app.provider llamacpp
   ```

3. **Check it end to end.**

   ```
   echo "The manager had already gone to ground." | plainly
   plainly history
   ```

   `plainly` with no subcommand reads stdin and writes the Explanation's sections
   to stdout — the Grammar section appears only when the model returns one.
   Everything a person reads goes to stderr, so the command pipes. Exit
   codes: `0` success, `1` failure, `2` usage, `3` not configured, `4` refused
   (the clipboard was marked sensitive).

4. **Bind a key** (niri):

   ```kdl
   binds {
       Mod+Shift+E { spawn "plainly-panel" "--clipboard"; hotkey-overlay-title="Explain clipboard"; }
   }
   ```

   `spawn` does not go through a shell, so `plainly-panel` has to be reachable by
   that name on `PATH`, and `~` is not expanded — write the full path if it is not
   installed. `hotkey-overlay-title` is what names it in niri's shortcut overlay.
   Pressing the key again re-reads the clipboard and refreshes the panel rather
   than opening a second one; ✕ closes it, and `app.panel_autohide` can make it
   dismiss itself after `app.panel_autohide_seconds`.

5. **Optionally, explain on copy.** `copy_auto_popup = true` on a session with
   data-control lets `plainly-desktop --watch` (or a desktop autostart entry
   running it) pop the panel when the clipboard changes. It is off by default and
   nothing is resident unless you start it.

`crates/plainly-panel/README.md` documents what the panel reads out of
`config.toml` and what each title-bar button does.

## What to believe

Three things are structurally true and worth knowing before the prose below:

- **The history is a cache.** The same Passage with the same Level, source and
  Native Language, prompt version and Provider Profile (provider, model and
  whether thinking is on) is generated once; pressing the key again reuses the
  record. Changing the prompt appendix changes the prompt version —
  deliberately, so a stored Explanation is never silently reattributed to new
  rules. The same Passage can therefore appear twice with different prompt
  versions, and that is a feature rather than a duplicate. Within one trigger, a
  Passage that appears twice is one Lookup Key, so the second copy reuses the
  first turn's answer instead of paying for it again.
- **An Explanation is the model's response, not a dictionary.** What the app
  guarantees is the *shape* — the sections, the metadata, the Lookup Key — never
  the correctness of the paraphrase or the gloss.
- **One Passage's run has one owner.** `plainly_core::Runner` opens the
  configuration, the provider and its credential, the prompt, the history store,
  the capability cache and the instant, and decides the turn: reuse the stored
  answer, or generate it and remember it. The CLI and the panel are adapters over
  that module. The *run policy* — store failure, credential states, the turn
  itself — lives in core and is covered by the same CI tests as the CLI's. The
  question just before a run gets the same treatment: whether a provider can be
  used now — its key requirement, its credential state and where that key came
  from, and whether its endpoint is on this machine — is one core value,
  `Readiness`. The desktop settings screen and the panel's ⟳ read that value;
  the CLI's credential adapter and the run path share the classifier behind it.
  Core's tests cover the shared judgement; each surface's tests pin its own
  edges. The panel's own adapter decisions (deduplicating a repeat within one
  trigger, showing a run notice once, placing the not-stored warning) stay under
  its unit tests and the manual checklist (ADR-0003). Where a Passage comes from,
  how a long input is split, which copies repeat, and how progress and failures
  are worded stay with each surface.

Honest limits, from the spec's "必须传达到实现阶段的诚实记录":

- **A history that refuses a write does not cost the answer.** The Explanation
  already paid for still goes to stdout on the CLI and stays on the panel; both
  say it was not saved, and the CLI ends with exit code 1 so a script can tell.
  A failed fresh answer asks again on the next run; a failed regeneration leaves
  the older Record in place, so the next run reuses that one until it is
  regenerated again.
- **A credential store that cannot answer is not "no key".** Both surfaces say
  so — the CLI on stderr, the panel once in its status line. A provider that may
  authenticate nothing (a local runtime) continues without a key; one that
  requires a key fails rather than pretending none was set.
- **The level is a hint, not a contract.** Measured on 4 Passages × 1 model, B2
  and C1 came out indistinguishable (average word length 4.32 vs 4.29; similarity
  to the original 0.463 vs 0.462). The measurement was taken under the v6 prompt,
  whose C1 descriptor read "keep most of the original structure". That wording
  fought the prompt's own rule, "It must be a real restatement: never return the
  passage unchanged", and the rule won; the descriptor has been reworded, and the
  numbers above are why the level stays a hint. A2 is usually the simplest, but
  not reliably: on two of the twelve samples its average word length was the
  highest of the three.
- **A local model will be confidently wrong.** The 4B this was calibrated on
  satisfies the contract 12/12 and answers in 1.87 s warm (7.2 s cold), and it got
  `bottled it` backwards, got `genteel` wrong, and produced no grammar note at all
  in 12/12. Those are the model's limits, not the prompt's: the same prompt
  answers all of it correctly in the cloud. So the panel keeps a fixed line under
  the body when the model is local — check it against the original above, or press
  ⟳ to regenerate in the cloud.
- **The Chinese translation is not searchable.** FTS5 tokenizes per table, and
  the index covers English only: the Passage, the Comprehensible English and the
  Glosses. Searching a Chinese word you remember from a translation finds
  nothing, and the history window says so rather than letting it look like a
  failed search. Adding a trigram table is the upgrade path; v0 does not have it.
  A Gloss's Example is not indexed either — it is generated scaffolding, not a
  search target — so a word that appears only in one is not findable.
- **Under the default settings the panel's only way out is ✕.** Auto-dismiss is
  off by default, and "click outside to close" is not implementable for a Wayland
  layer surface — the client cannot see clicks that land elsewhere. That is why
  the panel also has a five-second render watchdog: the one exit path must not
  depend on the panel having drawn.
- **The sensitive-content rule has a narrow reach.** Content is refused only when
  the clipboard publishes `x-kde-passwordManagerHint` *and* its value is `secret`
  — then nothing is sent and nothing is stored. A password manager that does not
  follow that KDE convention is not covered, and there is deliberately no
  heuristic (entropy, length, spaces) guessing on its behalf: false positives and
  false negatives together, plus a false sense of protection. Files and stdin
  carry no type marker at all, so the rule only covers the clipboard.
- **X11 is not supported**, and the panel says so in those words rather than
  failing silently. The CLI is not affected: a file or a pipe works on any
  session. `--clipboard` is the one flag that needs Wayland, because that is how a
  windowless process reaches the clipboard.
- **A GNOME session is expected to degrade to a focused window.** Mutter publishes
  neither data-control nor layer-shell, so the panel is designed to become an
  ordinary window that takes focus — and there GTK's own clipboard works, which is
  why one degradation covers both. This is read off Mutter's published
  capabilities, not measured: it has not been run on a GNOME machine yet, and
  `docs/manual-checks.md` §4 carries it as an unverified checklist item. In that
  window the "four corners" setting is best-effort: Wayland compositors may ignore
  a move request for an ordinary toplevel, and the corner is re-applied when the
  window is resized rather than being a guarantee.
- **Nothing that is not written down here is promised.** The full list of what v0
  does not do is the spec's Out of Scope: no web API, no MCP server, no screenshot
  OCR, no multi-device sync, no spaced-repetition loop, no macOS or Windows
  desktop support (v1; core stays portable for it), no release pipeline, no
  browser extension, no free-form panel dragging, no heuristic sensitive-content
  detection, no history encryption, no full-text search of translations, no
  automatic tagging, no prompt-fragment registry, no deleting the factory
  prompt's own rules (an appendix can only add), no project-level config
  overrides, no streaming, and no promise about a local model's correctness.

## Verifying it

`docs/manual-checks.md` is the checklist for the parts that need a real session —
a compositor, a display and a clipboard: the panel on niri, the GNOME
degradation, the X11 refusal, and what the CLI does with its own history.
`docs/desktop-manual-checks.md` covers the main window and the copy-auto-popup
listener. Automated coverage stops at the two seams the spec names: the
`plainly-core` public API and the `plainly` binary, plus the panels' own pure
decision functions.

## Layout

| crate | what it is |
| --- | --- |
| `crates/plainly-core` | domain logic: the Explanation contract and rendering, config, secrets, providers, the history store. No GTK, WebKit or Wayland. |
| `crates/plainly-cli` | the `plainly` command line. |
| `crates/plainly-panel` | the layer-surface panel the compositor spawns. |
| `crates/plainly-desktop` | the Tauri main window: history, search, settings, and the copy listener. |

`GLOSSARY.md` is the shared vocabulary, `docs/adr/` the recorded decisions, and
`.scratch/plainly/spec.md` the v0 specification these all answer to.

## License

MIT — see `LICENSE`.
