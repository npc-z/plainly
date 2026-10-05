# The Explanation schema carries no length, pattern or numeric bounds

We ask the model for an `Explanation` under a JSON Schema. That schema deliberately omits every
length, pattern and numeric bound: no `maxItems` or `minItems` on `glosses`, no
`minLength`/`maxLength`/`pattern` on any string, no `minimum`/`maximum` on any number.

The intuitive tidy-up — bounding `glosses` to, say, `maxItems: 5` — **must not be made.**

## Why

Two independent constraints forbid it, and one of them is fatal rather than merely lossy.

- **OpenAI's strict structured-output mode does not support them.** `minItems`, `maxItems`,
  `minLength`, `maxLength`, `pattern`, `format`, `minimum` and `maximum` are all outside the strict
  subset. Adding one is not a no-op — it fails schema validation on that provider.
- **llama.cpp can be killed by a contradictory bound.** [ggml-org/llama.cpp#29462](https://github.com/ggml-org/llama.cpp/issues/29462)
  is open and unfixed: a grammar built from `minItems > maxItems` (or `minLength > maxLength`, or a
  large *valid* lower bound) allocates unboundedly and OOM-kills `llama-server` — from a single
  unauthenticated request. Both candidate fixes are unmerged. Upper bounds are safe; lower bounds are
  the hazard.

So the constraint the artifact actually wants — "3 to 5 glosses" — is expressed in the **prompt** and
enforced by **validation and retry**, not by the schema.

## Consequences

- Gloss count and every string length are **advisory**: the prompt asks, the validator checks, the app
  retries. A provider that cannot enforce a schema at all (DeepSeek) sits in the same regime as one
  that can — the only difference is which failures the schema catches first.
- If a cardinality bound is ever genuinely required, express it as **`maxItems` only**, and validate
  the schema before sending it. Never pair a lower bound with a lower upper bound.
- `llama-server` should run with **`--api-key`** on loopback, so the unauthenticated-request failure
  mode is not reachable in the first place.

## Rejected

**Bounding `glosses` with `maxItems` + `minItems`.** The obvious option, and the one that breaks
OpenAI strict mode while leaving an OOM path open on llama.cpp. Rejected on both counts.

## See also

- `.scratch/plainly/research/structured-output-support.md` — the per-provider support surface.
- `.scratch/plainly/issues/01-output-contract.md` — the artifact contract this schema serves.
