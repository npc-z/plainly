# The Runner owns one run's machine state

`plainly_core::runner::Runner` is the one module a surface opens to explain a Passage. It owns the
paths, the configuration snapshot, the resolved provider and its credential, the prompt, the history
store that doubles as the cache, the capability cache, and the instant the run is stamped with.
`Runner::explain(passage, recall)` drives recall → generate → remember and returns a `Turn`, or a
`TurnFailure`; the surfaces stay adapters: where a Passage comes from, how a long input is split,
which Passages are duplicates, how progress and failures are worded, and what the surface draws.

The CLI and the panel used to assemble this state themselves. The duplication had already diverged: a
store write failure aborted the CLI's batch but only failed the panel's chunk; the CLI deduplicated a
repeated Passage and the panel did not; an unreadable keyring was a warning on the CLI and silence in
the panel. The divergence is what makes this a decision rather than a tidy-up.

## Why

- **The seam is real.** Two surface adapters — the CLI and the panel — needed the same assembly and
  the same turn semantics, so a module between them pays for itself rather than inventing a slot.
- **The dependencies already live in core.** `Paths`, `ConfigFile`, `Secrets`, `Store`, `Cache` and
  `Timestamp` are core's; composing them there adds no dependency and core still depends on no GTK,
  WebKit or Wayland (spec §2, §14).
- **It is the only way the panel's run path can be covered by CI.** CI tests `plainly-core` and the
  `plainly` binary; the panel is outside the automated seam, so policy left in its worker is policy
  only the manual checklist sees.
- **The policy it centralizes is the policy that drifted.** Store failure, credential states and
  repeated-Passage cost are not surface choices; they are one answer each.

## Consequences

- **Credential states are values, not silence.** A missing key and an unreadable credential store are
  distinct results; neither is silently treated as the other. The configured provider's credential is
  resolved when the Runner opens, so the CLI can still report a missing key without draining stdin; a
  `Bypass` provider's credential is resolved per turn. A keyring that cannot answer while the
  provider may authenticate nothing yields a notice rather than a failure.
- **A store write failure keeps the Explanation.** The `Turn` carries the artifact with `record:
  None` and a `NotStored` notice; a batch continues, the CLI still exits non-zero, and the surface
  says the answer was not saved. Throwing away an answer already paid for is the one outcome that
  cannot be defended.
- **The clock stays a value.** Surfaces produce the instant with `Timestamp::now()` and pass it to
  `Runner::open`; core never calls the system clock, as everywhere else.
- **A surface's explain path must not open the configuration, store, cache or secret stores itself.**
  The maintenance check is a grep of the explain path — the CLI's
  `crates/plainly-cli/src/commands/explain.rs` and the panel's
  `crates/plainly-panel/src/worker.rs` — for `Paths::discover`, `ConfigFile::open`,
  `Store::open`, `Cache::new`, `Secrets::from_process_env` or a local `now()`.
  The panel's `prefs.rs` reads the configuration once per trigger for its own
  settings (corner, timer, switches, provider); that is not an explain path and
  is deliberately outside this check.
- **`explain::run` stays public and unchanged beneath the Runner.** Probe → retry → downgrade keeps
  its interface and its 863-line test seam; the new tests sit at the Runner's interface, and the CLI's
  binary stays the end-to-end seam.
- **`Runner` is a code name, not a domain term.** It is deliberately absent from `GLOSSARY.md`: it
  names a composition root, not a concept of the learner's domain.

## Rejected

- **Keep the assembly in each surface and extract only the shared steps.** The steps that would be
  extracted *are* the machine state — the thing the surfaces differ about. The policy would leak back
  through options and the module would be shallow.
- **A trait-based session with a fake in tests.** There is one production adapter and no second
  implementation to swap in. Tests inject `Paths` and `Secrets` values instead, which is the
  substitution the seam actually needs.
- **Read the clock inside core.** Core takes an instant as a value everywhere else; adding one
  system-clock read here would make the crate's most side-effectful path its newest module.
- **Let each surface keep its own failure policy.** Store failure, deduplication and credential
  handling are three behaviours of one question; keeping them per surface is how they diverged.
- **Call it `Session`.** The word already means three things in this crate — the tier a secret is
  held for, the display session, and the main window's `Desktop` alias — and `display.rs` already
  records that three meanings was one too many.
- **Put batch handling inside the Runner.** Batch policies are surface policies: the CLI is the long
  input's home and has no chunk limit, the panel stops at five chunks and updates per chunk. A single
  Passage per call keeps the interface small and the stop policy where it is visible.

## See also

- `crates/plainly-core/src/explain.rs` — the probe / retry / downgrade path the Runner composes.
- `crates/plainly-cli/src/commands/explain.rs` and `crates/plainly-panel/src/worker.rs` — the two
  assemblies this replaces.
- `docs/adr/0001-no-bounds-in-the-explanation-schema.md` and
  `docs/adr/0002-the-example-is-required-but-not-backfilled.md` — the decisions the Explanation's
  shape answers to.
