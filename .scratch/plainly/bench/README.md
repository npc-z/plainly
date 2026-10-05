# bench — ticket 03's measurement harness

**Prototype-grade.** It exists to answer one question: *can the one local model we already have hold
the Explanation contract?* Not production, not a test suite.

```sh
python3 bench.py              # all passages
python3 bench.py p03 p08      # a subset
LLAMA_BASE=http://host:port python3 bench.py
```

Stdlib only: no pip, no venv, no downloads. `LLAMA_MODEL` overrides the model id.

## Files

- `schema.json` — the first concrete cut of the Explanation contract. Shape fixed by ticket 01: the
  model returns **only** the content fields; the original text and every piece of metadata are
  attached by the app. Deliberately free of length/pattern/numeric bounds, per
  [docs/adr/0001](../../../docs/adr/0001-no-bounds-in-the-explanation-schema.md): OpenAI strict mode
  rejects them, and a contradictory bound OOM-kills `llama-server`. The spec owns the final form;
  this file exists so the harness and the spec cannot drift apart.
- `passages.json` — 12 hard passages covering the blocker taxonomy: idioms, phrasal verbs,
  collocations, structural obstacles, literary/legal/technical register.
- `results/<provider>/<prompt>.json` — one file per run: the run summary (contract pass rate,
  latency, request settings) plus every request outcome, including the untouched model output.

## Why the request looks the way it does

- **Nested** `response_format.json_schema.schema`. Ticket 10 verified in llama.cpp's source that the
  *flat* form its own README documents is silently ignored and degrades to "any JSON object".
- `temperature: 0`, `max_tokens: 2048`. The local service runs at `--ctx-size 16384`, set per-model in
  the user's NixOS preset, so no Modelfile workaround is needed (that trap is Ollama's).
- `content` falls back to `reasoning_content` when empty — the failure mode ticket 10 found on
  LM Studio, guarded against here for the same reason.

## Reading the results

`valid: true` means the response parsed **and** satisfied the contract. It says nothing about whether
the explanation is *correct* — that is a human judgement, and ticket 03 records where this model gets
it wrong.
