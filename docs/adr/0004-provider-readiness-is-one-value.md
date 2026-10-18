# Provider readiness is one value

`plainly_core::readiness::Readiness` answers one question about one provider: does it resolve from
the configuration, does it need a key, is a key there (and where from) or is the store unable to
answer, and is its endpoint on this machine. `Readiness::of(name, &Config, &Secrets)` composes the
existing `ProviderSetup::resolve`, `Secrets::resolve` and `is_loopback`; `usable()` is derived from
those facts. The desktop settings screen, the panel's ⟳ choice and the CLI's credential adapter read
it instead of re-deriving it. Wording, exit codes and UI states stay with each surface (ADR-0003).

## Why

- **The seam is real, and the answers had diverged.** Three surfaces answered "can this provider be
  used right now?": the desktop settings screen (`KeyView`, `usable`, `first_run`), the panel's cloud
  alternative for ⟳, and the CLI's probe path. Two of them disagreed with the run path:
  - the panel took the requirement from the preset *name*, while the run path takes it from the
    endpoint-filtered `ProviderSetup` — a `deepseek` preset pointed at another host was `Required`
    in one place and `Optional` in the other;
  - the panel judged "cloud" by the preset's endpoint rather than the configured one, so a provider
    pointed at this machine could be offered as the cloud move;
  - the desktop treated an environment variable as a usable key without asking whether it resolves,
    so a key that is not valid UTF-8 showed as usable and failed at run time.
- **The value is the test surface.** The condition that decides usability is a domain fact, and the
  judgement that had drifted lived in the desktop and panel crates, outside `.github` CI. Composing
  the existing core values puts the rule under the core tests, next to the run path it must agree
  with.
- **The dependencies already live in core.** `ProviderSetup`, `Presets`, `Secrets`, `KeySource`,
  `SecretError` and `is_loopback` are core's; composing them adds no dependency and keeps core free
  of GTK, WebKit and Wayland (spec §2, §14).
- **Readiness is not a probe.** It is built from the configuration and the credential stores; it
  does not call an endpoint. Capability (`probe`, `Capability`, `Origin`) is a different axis and
  stays where it is: a display path must not turn into a request nobody asked for.

## Consequences

- **Credential states stay values, not silence.** Missing and unreadable are distinct in the value,
  as they already are in the run path (`CredentialState`, `CredentialError`); readiness never folds
  "the store could not answer" into "there is no key".
- **The value carries no secret.** Readiness reports that a key was found and from which tier
  (`Environment`, `Keyring`, `Session`), never the key itself. The run path keeps
  `resolve_credential`, which does return the secret; both are projections of one crate-internal
  classifier over `(KeyRequirement, Result<Option<ResolvedKey>, SecretError>)`, so they cannot drift.
- **Surfaces keep their language.** The desktop maps the facts to its `KeyView` states and Chinese
  sentences, the panel to its note and ⟳ choice, the CLI to its exit codes (ADR-0003).
- **The panel's cloud alternative is still panel policy.** Readiness decides whether one provider is
  usable and where it points; "the first shipped preset that is remote and usable" remains a button
  rule in the panel, reading those facts.
- **Config bounds are a different judgement.** The legal domain of a numeric setting is owned by
  `config` (validation on write), not by readiness; the read side keeps tolerating hand edits. That
  decision and its one behaviour change are recorded in the feature spec, not here.

## Rejected

- **Let each surface keep its own judgement.** That is the state this replaces: three computations,
  two of them already disagreeing with the run path, and the desktop's outside CI.
- **Publish only a boolean `usable`.** The surfaces need different slices of the same answer
  (desktop needs the tier and the failure reason, the panel needs locality, history rows need only
  locality), so a boolean would push them back to re-deriving the parts they need.
- **Carry the resolved key in the value.** It would save a resolution, at the cost of a value that
  can be serialised for the interface becoming able to carry secrets. Not worth it.
- **Fold endpoint probing into readiness.** It would make the settings screen and the panel's
  capability display perform network requests, and would change what "not probed yet" means.
- **Put it in `setup` or `runner`.** `setup` does not know the secret stores, and readiness must
  answer for a name that does not resolve yet. `runner` is one opened run; readiness is the
  pre-run question a surface asks without opening one.
- **Name it `provider`.** Core already has a `provider` module for the wire seam, and the CLI and
  configuration use "provider" for the thing itself; a third meaning would be one too many.

## See also

- `crates/plainly-core/src/setup.rs`, `presets.rs`, `secrets.rs`, `runner.rs` — the values this
  module composes and the run path it must agree with.
- `docs/adr/0003-the-runner-owns-one-runs-machine-state.md` — the run-time half of the same
  principle: policy that had two owners gets one.
- `.scratch/readiness/spec.md` — the feature spec, including the config-bounds slice recorded there.
