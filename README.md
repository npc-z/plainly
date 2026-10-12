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

Two things are structurally true and worth knowing before the prose below:

- **The history is a cache.** The same Passage with the same Level, source and
  Native Language, prompt version and Provider Profile (provider, model and
  whether thinking is on) is generated once; pressing the key again reuses the
  record. Changing the prompt appendix changes the prompt version —
  deliberately, so a stored Explanation is never silently reattributed to new
  rules. The same Passage can therefore appear twice with different prompt
  versions, and that is a feature rather than a duplicate.
- **An Explanation is the model's response, not a dictionary.** What the app
  guarantees is the *shape* — the sections, the metadata, the Lookup Key — never
  the correctness of the paraphrase or the gloss.

Honest limits, from the spec's "必须传达到实现阶段的诚实记录":

- **The level is a hint, not a contract.** Measured on 4 Passages × 1 model, B2
  and C1 came out indistinguishable (average word length 4.32 vs 4.29; similarity
  to the original 0.463 vs 0.462). The measurement was taken under the v6 prompt,
  whose C1 descriptor still read "keep most of the original structure" — the wording
  `crates/plainly-core/src/prompt.rs` names as the one that fought the prompt's own
  rule, "It must be a real restatement: never return the passage unchanged" — and
  the rule won. That descriptor has since been reworded; the numbers above are the
  reason the hint is a hint. A2 is usually the simplest, but not reliably: on two of
  the twelve samples its average word length was the highest of the three.
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
