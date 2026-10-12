# The Gloss Example is required from the model and absent from every older Record

Every **Gloss** now carries an **Example**: one AI-written sentence that uses the explained expression in the sense the Passage gives it. The wire schema **requires** it — a Gloss without one is not a valid answer — while a Record written before this change has no Example and **nothing backfills one**. Reading such a Record is not an error: its Glosses simply have no Example, and a surface that shows Glosses shows no Example line.

The store's `glosses` table therefore gains a column with an empty default, through **the first migration this repository has**: `PRAGMA user_version` 0 → 1, and `ALTER TABLE glosses ADD COLUMN example TEXT NOT NULL DEFAULT ''` when the column is missing. `CREATE TABLE IF NOT EXISTS` cannot do that, and no reader can invent what a v1 row never had.

## Why

- **A required field is the only thing that makes the model decide.** The contract's rule is that the schema, not the prompt, is what an answer is validated against. `grammar` is nullable because "this sentence has no structural obstacle" is a real decision, while "this Gloss has no Example" is not a decision at all.
- **ADR-0001 forbids saying "not empty" in the schema.** `minLength` fails OpenAI's strict mode and a lower bound can OOM `llama-server`, so "one sentence, never empty" is a prompt rule, checked by the bench harness and by hand — the same regime the Gloss count already lives in.
- **Backfilling would spend the learner's money to change text they have already read.** A regeneration is a different Lookup (the prompt version is a component of the key), so it writes a *new* Record rather than editing the old one; the affordance a backfill needs — replace this Record in place — is exactly the one the store refuses.

## Consequences

- `ARTIFACT_VERSION` is 2. A row stamped 1 has no Example and is still readable; the version is what says so, and it is deliberately not part of the Lookup Key, so bumping it does not throw the cache away.
- An empty `example` is a legitimate value *in an old Record* and an illegitimate one *from the model*. The visible rule is therefore "no Example, no Example line": a surface never draws an empty line, and never says why.
- The store owns a migration step from now on. It is idempotent — a fresh database is created at the current shape, an old one is altered once — and there is still no migrations directory: the test that builds a v0-shaped database by hand is the migration's documentation.

## Rejected

**Making `example` nullable (`type: ["string", "null"]`).** It reads as the smaller change and it is the worse contract: a model that skips the work can always answer `null` and pass validation, so the field would be optional in practice while looking required in the schema. The one null in this contract belongs to `grammar`, where it means something.

**Backfilling old Records on read or on open.** Silent model calls at startup, paid for by the learner, to change Records they already accepted.

## See also

- `.scratch/gloss-examples/spec.md` — the change this decides.
- `docs/adr/0001-no-bounds-in-the-explanation-schema.md` — why "never empty" is not a schema rule.
- `crates/plainly-core/src/store.rs` — the migration step, kept beside the schema it amends.
