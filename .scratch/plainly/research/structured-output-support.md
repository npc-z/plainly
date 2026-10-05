# Research: schema-constrained decoding support surface per provider

Provenance: research subagent, 2026-10-05, for issues 01 (`output-contract`) and 04
(`provider-abstraction`) via issue 10. Question: **can each provider actually force the model to
return the Plainly JSON artifact** (original text, leveled paraphrase, `glosses[]` of
expression → plain-English meaning, optional grammar note, native-language translation,
level/language metadata), or does the contract degrade to prompt + validate + retry?

Method: primary sources only — official docs (fetched as raw markdown where the site offers it),
the providers' own OpenAPI/reference pages, and llama.cpp source at `master` plus the GitHub REST
API for issue/PR state. Everything below is dated. Where a claim is second-hand (a bug report, a
blog) or could not be checked, it says so. Read the [gaps](#gaps--could-not-verify) section before
quoting anything.

Snapshot date for all "current" claims: **2026-10-05** (llama.cpp `master` pushed 2026-10-05
07:45 UTC).

---

## The table

| Provider | Can it enforce a schema? | Mechanism | Known traps | Recommended call parameters |
|---|---|---|---|---|
| **llama.cpp** (`llama-server`) | **Yes** — grammar-constrained sampling. Verified in source, not just docs. | JSON Schema → GBNF → sampling grammar (`common/json-schema-to-grammar.cpp`, `llama-grammar.cpp`). Schema is also compilable ahead of time into a `.gbnf` and passed as `--grammar-file`. | (1) **Wire shape**: only the *nested* `response_format.json_schema.schema` (or flat `response_format.schema` **with `type:"json_object"`**) is read; the flat `{"type":"json_schema","schema":{…}}` form that llama.cpp's own README documents is **not** read — it silently degrades to "any JSON object". (2) **#29462 open, unfixed**: `minItems > maxItems` (or `minLength > maxLength`, or GBNF `{10,2}`) drives an unsigned underflow in the grammar builder → unbounded allocation → **OOM-kills the server**; measured 11,447 MB peak RSS. A huge *valid* lower bound (`minItems: 100000000`) does it too. One **unauthenticated** request is enough. (3) `minimum`/`maximum` on a **float/number** type are silently ignored (integer bounds do work). (4) Unsupported `pattern` is a warning + fallback to any string, not an error. (5) `--api-key` is effectively a security control given (2). | `POST /v1/chat/completions` with the **nested** OpenAI shape; `temperature: 0`; author the schema so `min*` ≤ `max*` always, prefer `maxItems`-only, keep lower bounds small; validate the schema client-side before sending. Server: `--api-key <key>`, `-c <ctx>` sized for the passage. Prefer `-jf schema.json` (or `--grammar-file`) if the schema is fixed at launch. |
| **Ollama (local)** | **Yes**, on both surfaces, with version caveats. | Native `format`: `"json"` (any JSON) or a full JSON-Schema **object**. `/v1/chat/completions` `response_format` is a documented supported field and the docs state structured outputs work through it. | (1) **Thinking-off silently disables `format`** on qwen3.5-series (and reportedly Qwen3 from 0.30.0) — `/api/chat` returns prose, no error. Reproduced on 0.17.6, 0.17.7, 0.18.0, 0.30.7; **fixed in 0.31.2**. (2) `num_ctx` **cannot** be set through the OpenAI-compat API — Modelfile + `ollama create`, or `OLLAMA_CONTEXT_LENGTH` on the server. (3) Default context is VRAM-tiered: <24 GiB → **4k**. On an 8–16 GB laptop a hard passage may truncate. (4) Ollama Cloud does not support structured outputs at all. (5) Docs only say "it is ideal" to also paste the schema into the prompt — do it; it materially grounds the output. | Native `POST /api/chat` with `format: <schema>`, `options: {temperature: 0}`, and the schema **also echoed into the prompt**; keep thinking enabled unless you are on **≥ 0.31.2**; set context via `PARAMETER num_ctx` (Modelfile) or `OLLAMA_CONTEXT_LENGTH`. Treat `/v1` `response_format` as the fallback path, not the primary. |
| **OpenAI** | **Yes** — this is the reference implementation. | `response_format: {type:"json_schema", json_schema:{name, strict:true, schema}}` (Chat Completions) or `text.format: {type:"json_schema", name, strict, schema}` (Responses). Constrained decoding, schema cached server-side. | (1) **Strict mode accepts a *subset* of JSON Schema.** `minItems`/`maxItems`, `minLength`/`maxLength`, `pattern`, `format`, `minimum`/`maximum`, `multipleOf`, `minProperties`/`maxProperties`, `uniqueItems`, `contains` are **unsupported** — so the `glosses` array cannot be length-bounded and strings cannot be pattern/length-bounded. (2) **Every** property must be in `required` + `additionalProperties:false` — the "optional grammar note" must be `type:["string","null"]` and *still listed in `required`*. (3) Root cannot be `anyOf`. (4) Refusals **do not follow the schema**: check `message.refusal` / `content[].type == "refusal"`, and treat a refused/empty response as a first-class failure, not a parse error. (5) Structured Outputs are not supported with parallel function calls. | Responses API preferred by OpenAI's own docs; Chat Completions if you want one code path for every OpenAI-compatible target. `strict: true`; `temperature: 0`; compile the schema through a strict-mode sanitizer (see [adapter shape](#what-this-means-for-the-adapter)). Handle `refusal` and `finish_reason`. |
| **LM Studio** | **Yes on the OpenAI-compatible surface**, with an open routing bug on Qwen3.5. | GGUF models → llama.cpp grammar-based sampling; MLX models → Outlines. Documented on `POST /v1/chat/completions` with the OpenAI nested `response_format.json_schema` shape. | (1) **`{"type":"json_object"}` is rejected with HTTP 400** — `'response_format.type' must be 'json_schema' or 'text'` (LM Studio 0.4.10). Different from llama.cpp, which accepts `json_object`. (2) **Open bug: with Qwen3.5 reasoning models the JSON is constrained into `reasoning_content` and `content` is left empty**, with `finish_reason:"stop"`. Happens on Qwen3.5-35B-A3B and 9B (GGUF) and qwen3.5-4b-mlx; **not** on Gemma-4-26b-a4b. Disabling thinking did not help in the report. (3) The **native v1 REST API (`POST /api/v1/chat`) documents no structured-output parameter at all** — the schema path is the OpenAI-compatible one. (4) "Not all models are capable of structured output, particularly LLMs below 7B parameters." | `POST /v1/chat/completions` (port 1234) with the nested `response_format.json_schema`, `strict: true`, `temperature: 0`, and a schema sanitized as for OpenAI (Unsupported keywords are undefined behaviour here). **Read `content` and fall back to `reasoning_content`**; if both are empty, that is a failure → retry. Prefer Gemma-class models for schema work until the Qwen3.5 bug is closed. |
| **DeepSeek** | **No.** JSON *mode* only — no schema enforcement. | `response_format: {type:"json_object"}`. The API reference's allowed values for `response_format.type` are exactly **`text` and `json_object`** — there is no `json_schema`. | (1) Documented as **returning valid JSON only** — no schema adherence.* (2) **You must put the word "json" in the prompt and give a JSON example**, else the model "may generate an unending stream of whitespace until the generation reaches the token limit, resulting in a long-running and seemingly 'stuck' request". (3) Docs admit: "the API may occasionally return **empty content**". (4) Truncation risk if `max_tokens` is small — set it generously. (5) `tools[].function.strict` IS supported (beta) for *tool calls* — not for the response body. (6) `frequency_penalty`/`presence_penalty` are deprecated no-ops; `temperature` has no effect in thinking mode. | `response_format:{"type":"json_object"}`, `temperature` (irrelevant if thinking), a large `max_tokens`, and a prompt that (a) contains the literal word "json", (b) pastes the target schema, (c) shows a filled example. Then **validate locally and retry**. Budget for the empty-content failure mode. |

\* JSON mode is a real quality win over "please answer in JSON", but it guarantees *syntax*, not
*your field names*. For Plainly's artifact that is not a contract.

---

## Provider detail

### 1. llama.cpp (`llama-server`)

**Mechanism.** The server feature list advertises "Schema-constrained JSON response format"
([tools/server/README.md](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/tools/server/README.md)).
A JSON Schema is compiled into a GBNF grammar and applied as a *sampling* constraint, so the model
physically cannot emit a token that would break the grammar. That is a hard guarantee conditional
on the schema being satisfiable and the grammar compiling — see the OOM trap below.

**How to force a schema — the wire shape matters.** Read from the source, not the docs. In
`tools/server/server-common.cpp` (`master`, fetched 2026-10-05):

```cpp
auto json_schema = json_value(body, "json_schema", json());
auto grammar     = json_value(body, "grammar", std::string());
if (!json_schema.is_null() && !grammar.empty()) {
    throw std::runtime_error("Cannot use both json_schema and grammar");
}
if (body.contains("response_format")) {
    json response_format      = json_value(body, "response_format", json::object());
    std::string response_type = json_value(response_format, "type", std::string());
    if (response_type == "json_object") {
        if (response_format.contains("schema") || json_schema.empty()) {
            json_schema = json_value(response_format, "schema", json::object());
        }
    } else if (response_type == "json_schema") {
        auto schema_wrapper = json_value(response_format, "json_schema", json::object());
        json_schema = json_value(schema_wrapper, "schema", json::object());
    } else if (!response_type.empty() && response_type != "text") {
        throw std::invalid_argument("response_format type must be one of \"text\" or \"json_object\", but got: " + response_type);
    }
}
// an absent or empty schema means any object
if (json_schema.is_object() && json_schema.empty()) {
    json_schema["type"] = "object";
}
```
([source](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/tools/server/server-common.cpp))

Consequences, in order of how much they will cost you:

1. **Use the OpenAI nesting.** `{"type":"json_schema","json_schema":{"name":…,"strict":true,"schema":{…}}}` works — the parser descends into `json_schema.schema`. `strict` is neither read nor required.
2. **The flat form llama.cpp documents is broken.** The README says `response_format` supports
   `{"type": "json_schema", "schema": {…}}` ([README](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/tools/server/README.md)).
   With `type:"json_schema"` the code reads `response_format.json_schema.schema`; a `schema` sitting
   directly under `response_format` is never consulted, so `json_schema` stays empty and the
   fallback promotes it to `{"type":"object"}`. **You get "some JSON object", not your schema,
   and no error.** (A separate 2026-09 blog post reports the flat README form on build `b10868`
   being *byte-identical to sending no `response_format` at all* —
   [dev.to](https://dev.to/homelabpm/llama-server-ignores-the-responseformat-its-own-readme-shows-and-returns-200-41b5).
   That is *more* degraded than current `master` source implies; I could not reconcile the two and
   did not reproduce either. Treat "flat `json_schema.schema` is ignored" as the verified fact and
   the exact degraded output as version-dependent.)
3. `{"type":"json_object","schema":{…}}` **does** constrain to your schema (flat schema, `json_object` type).
4. `type:"text"` and a missing `type` are accepted and ignored (no constraint); any other non-empty
   type is a 400 — and the error message lists `"text"`/`"json_object"` but omits `"json_schema"`, which it does accept.
5. A **top-level** `json_schema` field (aliased to `grammar`) also works for the native
   `/completion` route, and `json_schema` + `grammar` together is a 400.

**Flags.** From the server README's generated help:

- `--grammar GRAMMAR` / `--grammar-file FNAME` — BNF-like grammar to constrain generations.
- `-j, --json-schema SCHEMA` / `-jf, --json-schema-file FILE` — JSON schema to constrain generations.
- `--api-key KEY` (env `LLAMA_API_KEY`), `--api-key-file FNAME` (env `LLAMA_ARG_API_KEY_FILE`) — "multiple keys can be provided as a comma-separated list".
- `--jinja, --no-jinja` — **default enabled** as of the fetched `master`. (Tool calling still requires it: `opt.use_jinja` is checked before accepting `tools`.) Structured output does **not** require `--jinja` — the grammar is applied at sampling.
- Defaults: host `127.0.0.1`, port `8080`.

**Version requirements.** UNVERIFIED. I could not establish which release first added
`response_format.json_schema` / `--json-schema` or the `strict` handling; the server changelog
lives in an issue thread ([#9291](https://github.com/ggml-org/llama.cpp/issues/9291)) and I did not
walk it. Given trap (2) and the OOM bug, pin an explicit build and test rather than trusting a
minimum version.

**Supported schema subset (from `common/json-schema-to-grammar.cpp` at `master`).** The converter
has explicit cases for:

- kinds: object, array, tuple, string, integer, number, boolean, null, any, enum, const, `$ref`, `anyOf`, `allOf`
- object: `properties`, `required`, `additionalProperties` (including a *schema* for the extra values — it builds a key rule that excludes the known property names)
- array: `minItems`, `maxItems` (via the repetition builder — see the OOM trap)
- string: `minLength`, `maxLength`, `pattern`, `format` (only `date`, `time`, `date-time`, `uuid` are recognised)
- integer: `minimum`, `maximum` (built as an int64 digit-range grammar)
- `$ref`: resolved against the bundled document (`#`, `#/$defs/...`); an **unresolved `$ref` is a hard error** ("Unresolved $ref")
- `allOf`: merges object properties (required if the child is not `anyOf`) and intersects enum values

Deliberate fallbacks and silent gaps found while reading it:

- `pattern` is parsed as an anchored regex (`^…$` required). An unsupported pattern does **not**
  fail: it reverts the rules, pushes a warning, and **accepts any string** ("pattern … is not
  supported (…), accepting any string"). An *invalid* pattern is a hard error. A `pattern` with
  lookaround, unanchored, or nesting deeper than `MAX_PATTERN_DEPTH` (100) is in the "unsupported → any string" class.
- **`minimum`/`maximum` are honoured for `integer` but ignored for `number` (float)** — the
  `KIND_NUMBER` case just emits the primitive `number` rule.
- There is **no `oneOf` kind** and no `not` / `if`-`then`-`else` / `dependentRequired` handling in
  the converter. What the JSON→schema-model layer does with them is **UNVERIFIED** — do not assume
  `oneOf` is enforced; write `anyOf` instead.
- **The repetition builder has no `min ≤ max` guard** (verified on `master` today):
  ```cpp
  static std::string build_repetition(const std::string & item_rule, int min_items, int max_items, …) {
      auto has_max = max_items != std::numeric_limits<int>::max();
      if (max_items == 0) return "";
      if (min_items == 0 && max_items == 1) return item_rule + "?";
      …
      return item_rule + "{" + std::to_string(min_items) + "," + (has_max ? std::to_string(max_items) : "") + "}";
  ```
  ([source](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/common/json-schema-to-grammar.cpp))

**Known bug #29462 — confirmed OPEN and UNFIXED.**

[ggml-org/llama.cpp#29462](https://github.com/ggml-org/llama.cpp/issues/29462),
"bug: the json-schema/GBNF grammar builder does not bound the repetition count — a contradictory or
huge `min*` makes it allocate unboundedly and OOM-kills the server".

- Filed 2026-09-26 by Yunzez. State: **open**, label **`bug-unconfirmed`**, **0 comments**,
  `updated_at` 2026-09-26 (i.e. no maintainer triage as of 2026-10-05).
- Reported against `llama-server` git `4b1a27f`, `version: 0.5.0-dev (build 1)`, CPU, Linux x86_64,
  `Qwen2.5-0.5B-Instruct-Q4_K_M.gguf`.
- Measured (fresh server per row, RSS sampled once per second):

  | schema | peak RSS | result |
  |---|---|---|
  | `{"type":"array","items":{"type":"integer"},"minItems":10,"maxItems":2}` | **11,447 MB** | process **Killed (OOM)**, server dead |
  | `minItems:3, maxItems:2` under a 3 GB `ulimit -v` | — | `std::bad_alloc` (OS limit truncates the same allocation) |
  | `minItems:2, maxItems:10` (valid) | 597 MB | 200 |
  | `minItems:5, maxItems:5` (valid) | 597 MB | 200 |

- **Triggering shapes** (all measured): array `minItems > maxItems`; string `minLength > maxLength`;
  direct GBNF `root ::= "a"{10,2}`; array with a huge *valid* lower bound (`minItems: 100000000`).
  **Upper bounds are safe**: `maxItems: 1e8`, `maxLength: 1e8` returned 200. Object
  `minProperties` "was inconsistent across runs and is left out".
- Root cause per the report: the repetition-count arithmetic is "neither guarded against `min > max`
  (which underflows in an unsigned type) nor capped for a large lower bound reached through arrays
  (`minItems`), strings (`minLength`), and direct GBNF `{M,N}`". With a memory cap the failure
  surfaces as `parse: error parsing grammar: std::bad_alloc` (HTTP 400) and the server survives;
  **an unbounded deployment is OOM-killed. A single unauthenticated request is enough.**

Fixes attempted, neither merged:

- [#29497](https://github.com/ggml-org/llama.cpp/pull/29497) "bug: fix grammar builder crash on invalid minItems/maxItems (#29462)" — **open, `draft: true`, `merged: false`, `mergeable_state: "dirty"`**, 2 comments, 1 commit but `+45988/−126359` across 832 files (a stale-base fork). Not a landed fix.
- [#25602](https://github.com/ggml-org/llama.cpp/pull/25602) "grammar : reject inverted repetition range {m,n} (m>n)" (opened 2026-07-12) — **open, `merged: false`**, 1 commit, `+3/−0`, `mergeable_state: "clean"`, reviewer requested from ggerganov. The API also returns a non-null `merge_commit_sha` alongside `merged:false` / `closed_at:null`, which is self-contradictory; I am treating the PR as **not merged** on the strength of `merged`, `state` and `closed_at`. Its description independently describes the same underflow (`max_times - min_times` on `uint64_t` → ~1.8e19), notes the existing `MAX_REPETITION_THRESHOLD` guard misses it because that guard is computed from `total_rules = max_times` (which is `1`), and calls it "a remote, unauthenticated DoS" reachable through `grammar` on `/completion` and "the `json_schema`→grammar path of `/v1/chat/completions`". Fuzzer-found, 18-byte PoC.

**Why this matters for Plainly even though *we* author the schema:** the app is the one writing
`minItems`/`maxItems`/`minLength`. A single off-by-one in our schema builder (or a schema
parameterised by user options, e.g. "max glosses") can kill the user's local server, and if the
sidecar is bound to anything but loopback without `--api-key`, anyone on the network can too.
Mitigations: never emit contradictory bounds; prefer `maxItems` over `minItems`; keep lower bounds
small; validate the compiled schema before sending; run the sidecar with `--api-key` and loopback-only binding.

### 2. Ollama

**Native `format` (the documented, supported path).**
[docs.ollama.com/capabilities/structured-outputs](https://docs.ollama.com/capabilities/structured-outputs):
`format` accepts either the string `"json"` or **a full JSON Schema object**. `POST /api/chat` with
`"format": {…schema…}`, then validate with `model_validate_json` / `Country.parse`. The same
`format` works for vision models. The docs' explicit guidance:

> "It is ideal to also pass the JSON schema as a string in the prompt to ground the model's response."

and the tips section recommends lowering temperature to `0`. Both are cheap; both should be on.

**OpenAI-compatible `response_format`.**
[docs.ollama.com/api/openai-compatibility](https://docs.ollama.com/api/openai-compatibility) lists
`/v1/chat/completions` supported features as: chat completions, streaming, **JSON mode**,
reproducible outputs, vision, tools, reasoning/thinking control — and **not** logprobs. In the
supported-request-fields list, **`response_format` is checked**. Explicitly **not** supported:
`tool_choice`, `logit_bias`, `user`, `n`; image input must be base64 (no image URLs);
`/v1/completions` takes `prompt` as a string only; `/v1/responses` exists (added v0.13.3) but is
**non-stateful only** (no `previous_response_id`, no `conversation`, no `truncation`). The
structured-outputs page closes with "Structured outputs work through the OpenAI-compatible API via
`response_format`."

So the docs assert schema enforcement through `response_format` — but note what is **not**
documented there: the page never spells out the `{"type":"json_schema","json_schema":{…}}` nesting
for the `/v1` route (only the native `format` examples are given), and it does not say which schema
features are honoured. Two data points sit on either side:

- [ollama/ollama#10001](https://github.com/ollama/ollama/issues/10001) ("this OpenAI syntax is
  ignored", Ollama 0.6.2, gemma3) was **closed as not-an-Ollama-bug**: the reporter concluded "The
  problem seems to be with Open WebUI", and a collaborator's script showed the OpenAI SDK's
  `client.beta.chat.completions.parse(...)` against `http://localhost:11434/v1` returning the
  correctly-shaped `parm1='hello'` for `gemma3:4b`. So the `/v1` json_schema path **has worked** at
  least for a flat, single-field schema.
- [pydantic/pydantic-ai#4917](https://github.com/pydantic/pydantic-ai/issues/4917) (2026-03-31 →
  closed 2026-04-16) reports the opposite for a *nested* schema via `NativeOutput(..., strict=True)`:
  `{"characters": ["Alice","Bob"]}` instead of objects, "141 Pydantic format errors", and the
  maintainer's conclusion after inspecting the trace: "looks like we are indeed sending
  `response_format` but the model/API is completely ignoring it … I'd consider that a bug in Ollama."
  The second reproduction explicitly used **`qwen3.5:397b-cloud`** — and Ollama's own docs say
  **Ollama Cloud does not support structured outputs**. That is my reading, not the thread's
  conclusion; the thread never mentions the cloud limitation. It stays a plausible reconciliation,
  not a verified one.

**Trap: thinking-off silently disables `format` (fixed in 0.31.2).**
[ollama/ollama#14645](https://github.com/ollama/ollama/issues/14645) "format is ignored when think
is disabled for qwen3.5 series" (label `bug`, 20 comments, 13 👍, closed 2026-07-07 as
`completed`), duplicated by [#14850](https://github.com/ollama/ollama/issues/14850) (closed as
duplicate).

- With `think=False` **and** `format='json'` (or a full schema), `qwen3.5:35b-a3b` returned ordinary
  prose; with `think=True` the same call returned JSON. Versions: 0.17.6, 0.17.7, 0.18.0.
- Reporter's mechanism: Ollama engages the grammar masking only after the end-of-thinking token,
  but with thinking disabled the template already closed the think tag
  (`<think>\n\n</think>\n\n`), so the transition never fires and masking is never applied.
- Still reproducing on **0.30.7** (2026-06-12) via `/api/chat` with thinking suppressed through a
  `SYSTEM /no_think` Modelfile instead of the `think` param — "format silently ignored … failure is
  **probabilistic** with compliant prompts (~1/3 in our production run)", while `POST /api/generate`
  with the same schema "enforces correctly, every run (5/5)". Same comment notes the gemma4 sibling
  issue was fixed but the qwen3.5 chat path was not.
- A comment on 2026-06-20 reports it spreading to older models (Qwen3) "starting from version 0.30.0".
- **Resolved:** "I just tested release 0.31.2, and the issue mentioned above is resolved"
  (2026-07-09). Issue closed 2026-07-07.
- **Actionable:** if Plainly disables thinking, require **Ollama ≥ 0.31.2**; otherwise the schema is
  advisory only and the failure is silent *and* intermittent — the worst combination, because a
  naive test run will pass.

**Trap: `num_ctx` is unreachable from the OpenAI-compat API.** Verbatim from the compat page:
"The OpenAI API does not have a way of setting the context size for a model. If you need to change
the context size, create a `Modelfile`" with `FROM <some model>` + `PARAMETER num_ctx <size>`, run
`ollama create mymodel`, and call that model name.
[docs.ollama.com/context-length](https://docs.ollama.com/context-length) adds the server-level
alternative `OLLAMA_CONTEXT_LENGTH=64000 ollama serve`, the App slider, and the defaults:

> Ollama defaults to the following context lengths based on VRAM: < 24 GiB VRAM: 4k context;
> 24-48 GiB VRAM: 32k context; >= 48 GiB VRAM: 256k context.

and "Cloud models are set to their maximum context length by default." This supersedes the older
FAQ claim of a flat 4096; I did not fetch the FAQ to compare. **On an 8–16 GB laptop the default is
4k**, which is a real truncation risk for a long hard passage.
Whether the native `/api/chat` accepts a per-request `options.num_ctx`: **UNVERIFIED** (the
structured-output docs only show `options={'temperature': 0}`).

**Trap: cloud.** The structured-outputs page opens with a note: **"Ollama's Cloud currently does
not support structured outputs."** Any `:cloud` model is therefore out of contract for Plainly's
artifact, regardless of which endpoint you call.

### 3. OpenAI

Source: [developers.openai.com/api/docs/guides/structured-outputs](https://developers.openai.com/api/docs/guides/structured-outputs)
(fetched as `.md`), plus Microsoft's Azure OpenAI documentation for the schema-subset table.

**API surfaces (both current).**

- Chat Completions: `response_format: {"type":"json_schema","json_schema":{"name":…,"strict":true,"schema":{…}}}`.
- Responses (the one OpenAI's docs actually lead with): `"text": {"format": {"type":"json_schema","name":…,"schema":{…},"strict":true}}` — `type`, `name`, `schema`, `strict` are **siblings under `format`**.

Docs' own framing: use a structured `text.format` "when you want to structure the model's output
when it responds to the user, rather than when the model calls a tool". Both ensure schema
adherence; JSON mode does not:

| | Structured Outputs | JSON Mode |
|---|---|---|
| Outputs valid JSON | Yes | Yes |
| Adheres to schema | Yes | **No** |
| Compatible models | `gpt-4o-mini`, `gpt-4o-2024-08-06`, and later | `gpt-3.5-turbo`, `gpt-4-*`, `gpt-4o-*`, compatible GPT-5 models |

"We recommend always using Structured Outputs instead of JSON mode when possible." Structured
Outputs is "available in our latest large language models, starting with GPT-4o"; the docs' current
recommended starting model is `gpt-6-astra`.

**Refusal handling — this is part of the contract, not an edge case.** Verbatim:

> "When using Structured Outputs with user-generated input, OpenAI models may occasionally refuse to
> fulfill the request for safety reasons. **Since a refusal does not necessarily follow the schema
> you have supplied in `response_format`, the API response will include a new field called `refusal`
> to indicate that the model refused to fulfill the request.**"

The documented response shape is a message whose content is
`[{"type":"refusal","refusal":"I'm sorry, I cannot assist with that request."}]` with
`status: "completed"`. The SDK guidance is to branch on `message.refusal` (Chat Completions) or
`content.type == "refusal"` (Responses) **before** attempting to parse, and to raise on an
incomplete response (`response.status == "incomplete"` /
`incomplete_details.reason`). Practical consequence for Plainly: a strictly-typed `serde`
deserialization of `content` will be handed `null` or a refusal string on a safety refusal — check
`refusal` first and surface it as a distinct outcome.

**The strict-mode schema subset.** I could **not** fetch OpenAI's own "Supported schemas" appendix:
the page fetch was truncated before it (it ends around the refusal section), and the `?api-mode=chat`
variant returns a JS shell. The table below is from **Microsoft's official Azure OpenAI
documentation** ([learn.microsoft.com/…/how-to/structured-outputs](https://learn.microsoft.com/en-us/azure/foundry/openai/how-to/structured-outputs),
page last updated 2026-08-24), which documents the same feature for Azure OpenAI and labels the
numeric limits "Azure-specific". It states the keyword restrictions apply to both Chat Completions
and Responses.

*Supported types:* String, Number, Boolean, Integer, Object, Array, Enum, **anyOf**. (Note: "Root
objects can't be the `anyOf` type.")

*All fields must be required.* "Include all fields or function parameters as required." Emulate an
optional field with a union with null: `"type": ["string", "null"]`, and *still* list it in
`required`.

*Always set `additionalProperties: false`* on every object.

*Key ordering:* output follows the schema's property order — useful for rendering, not a guarantee
to rely on.

*Unsupported type-specific keywords:*

| Type | Unsupported keywords |
|---|---|
| String | `minLength`, `maxLength`, `pattern`, `format` |
| Number | `minimum`, `maximum`, `multipleOf` |
| Objects | `patternProperties`, `unevaluatedProperties`, `propertyNames`, `minProperties`, `maxProperties` |
| Arrays | `unevaluatedItems`, `contains`, `minContains`, `maxContains`, `minItems`, `maxItems`, `uniqueItems` |

*Supported and worth using:* `$defs` + `$ref` (including `#/$defs/…`), and **recursive schemas**
(`items: {"$ref": "#"}`). `allOf`/`oneOf`/`not` do not appear in the supported-types list —
use `anyOf`.

*Also documented:* structured outputs are not supported with parallel function calls — set
`parallel_tool_calls: false`; and they are not supported with "bring your own data", Assistants, or
the `gpt-4o-audio-preview` / `gpt-4o-mini-audio-preview` (`2024-12-17`) snapshots.

**Numeric limits — UNVERIFIED for OpenAI proper.** The Azure page says "A schema can have up to
**100 object properties total, with up to five levels of nesting**", explicitly under a heading
that says "The following **Azure-specific** limits and unsupported keywords apply…". I could not
confirm OpenAI's own numbers from OpenAI's docs and I am **not** asserting 100/5 as OpenAI's limit.
Design to a shallow schema (the Plainly artifact is 2 levels: root → `glosses[]` → gloss object)
and this question disappears — but do not quote a number.

### 4. LM Studio

Sources: [lmstudio-ai/docs → `1_developer/3_openai-compat/structured-output.md`](https://raw.githubusercontent.com/lmstudio-ai/docs/main/1_developer/3_openai-compat/structured-output.md),
[`3_openai-compat/index.mdx`](https://raw.githubusercontent.com/lmstudio-ai/docs/main/1_developer/3_openai-compat/index.mdx),
[`2_rest/chat.md`](https://raw.githubusercontent.com/lmstudio-ai/docs/main/1_developer/2_rest/chat.md),
[`api-changelog.md`](https://raw.githubusercontent.com/lmstudio-ai/docs/main/1_developer/api-changelog.md).

**OpenAI-compatible server.** Supported endpoints: `GET /v1/models`, `POST /v1/responses`,
`POST /v1/chat/completions`, `POST /v1/embeddings`, `POST /v1/completions`; base URL
`http://localhost:1234/v1`. Structured output is documented on `/v1/chat/completions`:

> "The API supports structured JSON outputs through the `/v1/chat/completions` endpoint when given a
> JSON schema. Doing this will cause the LLM to respond in valid JSON conforming to the schema
> provided. It follows the same format as OpenAI's recently announced Structured Output API and is
> expected to work via the OpenAI client SDKs."

The documented request shape is the nested OpenAI one — `response_format.json_schema.{name, strict, schema}`
— and the JSON arrives as a **string** in `choices[0].message.content`, to be parsed by you.
Engines: "For `GGUF` models: utilize `llama.cpp`'s grammar-based sampling APIs. For `MLX` models:
using Outlines." Quality caveat, verbatim: **"Not all models are capable of structured output,
particularly LLMs below 7B parameters."**

Doc defects worth knowing before copying the example: the curl sample sets `"strict": "true"` as a
**string** rather than a boolean, and the Python sample provides `name` + `schema` but omits both
`strict` and `additionalProperties`. Neither sample demonstrates an unsupported-keyword case.

**`json_object` is rejected — use `json_schema` only.**
[lmstudio-bug-tracker#1773](https://github.com/lmstudio-ai/lmstudio-bug-tracker/issues/1773)
(LM Studio 0.4.10) reports: "`response_format: { type: "json_object" }` returns HTTP 400
(`'response_format.type' must be 'json_schema' or 'text'`)." That differs from llama.cpp, which
accepts `json_object`, and from Ollama's docs, which advertise JSON mode. So a shared
"JSON-mode fallback" in the adapter **will 400 against LM Studio** — special-case it.

**Open bug: schema output trapped in `reasoning_content` on Qwen3.5.**

- [#1773](https://github.com/lmstudio-ai/lmstudio-bug-tracker/issues/1773) — **open**, created
  2026-04-09, updated 2026-05-28, 3 comments, 6 👍. LM Studio 0.4.10, macOS. With
  `response_format: {type:"json_schema"}` on **Qwen3.5-35B-A3B** and **Qwen3.5-9B** (llama.cpp
  backend), the constraint appears to match the *thinking* stream: the model emits valid JSON inside
  `reasoning_content`, generation stops, and **`content` is `""`** with `finish_reason: "stop"` and
  `completion_tokens_details.reasoning_tokens: 13`. Without `response_format` the same model puts
  JSON in `content` after a normal 1082-token thinking phase. **Gemma-4-26b-a4b with identical
  parameters works correctly.** The report also states `/no_think` in the system prompt and
  `chat_template_kwargs: {"enable_thinking": false}` both had no effect.
- [#1971](https://github.com/lmstudio-ai/lmstudio-bug-tracker/issues/1971) — **open**, created
  2026-05-27, 0 comments. Same symptom on `qwen3.5-4b-mlx`, LM Studio 0.4.8+1, with
  `extra_body.chat_template_kwargs.enable_thinking = false`, `temperature: 0`,
  `strict: true`, `additionalProperties: false`. Control: `qwen3.5-0.8b` returns the JSON in
  `content` correctly.

This is directly load-bearing for Plainly: Qwen3.5 is the recommended local family in
[`local-models-and-incumbents.md`](local-models-and-incumbents.md). Until these close, the adapter
must treat `content == ""` with a populated `reasoning_content` as a recognised failure mode —
either recover the JSON from `reasoning_content` (it is valid and schema-shaped) or retry — and the
model-default decision should prefer a Gemma-class model for schema work on LM Studio, or gate
Qwen3.5 behind a test.

**Native REST API.** LM Studio 0.4.0 released the native v1 REST API at `/api/v1/*` (changelog).
The documented `POST /api/v1/chat` request body is: `model`, `input`, `system_prompt`,
`integrations`, `stream`, `temperature`, `top_p`, `top_k`, `min_p`, `repeat_penalty`,
`max_output_tokens`, `reasoning`, `context_length`, `store`, `previous_response_id`. **There is no
`response_format` / structured-output parameter in that documented body** — so on LM Studio the
schema path is the OpenAI-compatible one, not the native v1 chat endpoint. (Whether an undocumented
field exists: UNVERIFIED.) Note `context_length` *is* a first-class native parameter here, unlike
Ollama's OpenAI-compat surface.

**Version requirements.** The published API changelog begins at 0.3.5 and does **not** mention the
introduction of JSON-schema structured output, so that feature predates it — the minimum version is
**UNVERIFIED**. Two changelog entries do bear on the surface:
`0.3.18 (2025-07-10)` — "The `response_format.type` field now accepts `"text"` in chat-completion
requests"; `0.3.16 (2025-05-23)` — the OpenAI-compatible REST API (`/api/v0`) returns a
`capabilities` array in `GET /models` (example capability: `"tool_use"`). I found **no documented
capability flag for structured output**, so per-model capability discovery is UNVERIFIED.

### 5. DeepSeek

Sources: [DeepSeek API docs → JSON Output](https://api-docs.deepseek.com/guides/json_mode/) and the
[Chat Completions API reference](https://api-docs.deepseek.com/api/create-chat-completion) (both
current, © 2026 DeepSeek; models `deepseek-flash`, `deepseek-v4-pro`).

**No strict JSON Schema.** The API reference defines `response_format` as "An object specifying the
format that the model must output", with `type` `Possible values: [text, json_object]` and the note
"Must be one of `text` or `json_object`." There is **no `json_schema` value and no schema field**.
So DeepSeek cannot enforce Plainly's contract; it can only be asked, then checked.

**The documented JSON-mode requirements** (the JSON Output guide's "Notice" section — all four are
verbatim requirements, not advice):

1. Set `response_format` to `{'type': 'json_object'}`.
2. "Include the word "json" in the system or user prompt, **and provide an example of the desired
   JSON format** to guide the model in outputting valid JSON."
3. "Set the `max_tokens` parameter reasonably to prevent the JSON string from being truncated midway."
4. **"When using the JSON Output feature, the API may occasionally return empty content. We are
   actively working on optimizing this issue. You can try modifying the prompt to mitigate such
   problems."**

The API reference reinforces (2) with a failure mode worth quoting in full:

> "**Important:** When using JSON Output, you must also instruct the model to produce JSON yourself
> via a system or user message. Without this, the model may generate **an unending stream of
> whitespace** until the generation reaches the token limit, resulting in a **long-running and
> seemingly 'stuck' request**. Also note that the message content may be partially cut off if
> `finish_reason="length"`."

**The one schema-ish lever.** `tools[].function.strict` ("Default value: `false`") *is* supported:
"If set to true, the API will use strict-mode for the tool calls to ensure the output always
complies with the function's JSON schema. **This is a Beta feature**". Since tool-call arguments are
validated against a supplied schema, "make the artifact a single function's parameters" is a
plausible way to get real enforcement out of DeepSeek — but it is explicitly beta, it is a
tool-calling contract rather than a response contract, and I found no evidence anyone ships it that
way. Treat as an experiment, not a plan.

**Other field-level facts that will bite.** `frequency_penalty` and `presence_penalty` are marked
**deprecated — "no longer supported. It will not take effect if you pass it to the API."**
`temperature` "Has no effect in thinking mode"; `top_p` "only takes effect in thinking mode"
(clamped to ≥ 0.95) and is ignored in non-thinking mode. `thinking.type` defaults to `enabled`, and
`reasoning_effort` defaults to `high` — i.e. **the default is thinking on**, with a 64K default
output budget (128K at `max`). `finish_reason` can be `length`, `content_filter`,
`insufficient_system_resource`, or `aborted` — not just `stop`/`length`.
Whether sending `type: "json_schema"` produces a 400 or is silently ignored: **UNVERIFIED**.

---

## What this means for the adapter

A single internal schema cannot be sent verbatim to all five; the "one OpenAI-compatible HTTP
adapter" needs a small per-provider compiler in front of it. Concretely:

1. **Canonical schema in Rust** (`serde` types → JSON Schema once), then per-provider lowering:
   - **OpenAI / Azure** — strict lowering: root is an object; every property in `required`; add
     `additionalProperties: false` to every object; encode optional fields as `["<type>","null"]`;
     strip `minLength`/`maxLength`/`pattern`/`format`/`minimum`/`maximum`/`multipleOf`/`minItems`/
     `maxItems`/`minProperties`/`maxProperties`/`uniqueItems`; express unions as `anyOf`; no root `anyOf`.
   - **llama.cpp / LM Studio (GGUF)** — send the same shape but **nested** under
     `response_format.json_schema.schema`; bounds are allowed here, so clamp them and assert
     `min ≤ max` before sending (the OOM bug). Prefer `maxItems`/`maxLength`-only.
   - **Ollama** — prefer native `/api/chat` with `format: <schema>` (plus the schema echoed into the
     prompt, temperature 0), and require ≥ 0.31.2 if thinking is disabled. Note `/v1` can't set
     `num_ctx`.
   - **DeepSeek** — no schema on the wire; send `{"type":"json_object"}`, ensure the prompt contains
     "json", paste the schema and a filled example, allow a large `max_tokens`, and treat empty
     content as a known retryable failure.
2. **Never assume the response is in `content`.** Read `content`; on LM Studio also read
   `reasoning_content`; on OpenAI check `message.refusal` / `content[].type == "refusal"` *before*
   parsing; treat empty content + `finish_reason: "stop"` as failure. On OpenAI, an incomplete
   response is a distinct state (`status == "incomplete"`).
3. **Always validate locally and retry with the error text.** For OpenAI/llama.cpp/LM Studio a
   schema violation means the constraint was not actually applied (wrong nesting, a
   silently-ignored keyword, a routing bug) — log it as a *bug signal*, not as model noise.
4. `temperature: 0` everywhere for a rendering contract.
5. **Bind local servers to loopback and use `--api-key`** on `llama-server`; #29462 is an
   unauthenticated remote kill.

## Conclusion: who actually guarantees the contract

**Genuinely schema-enforced (constrained decoding, artifact-shape guaranteed if the schema is
satisfiable and the request shape is right):**

1. **OpenAI** — the reference. Enforces the subset, detects refusals programmatically. Constraint:
   the subset is narrow (no array-length or string-length bounds), so "3–5 glosses" is not
   expressible as a hard bound.
2. **llama.cpp `llama-server`** — real grammar enforcement, the broadest schema support of the
   five, and the only one where I verified the wire format in source. Two conditions: use the
   **nested** OpenAI shape, and keep `min*`/`max*` consistent and small or you OOM-kill the server
   ([#29462](https://github.com/ggml-org/llama.cpp/issues/29462), still open).
3. **LM Studio** — real enforcement on `/v1/chat/completions` (llama.cpp grammars for GGUF,
   Outlines for MLX), *but* with an open routing bug that empties `content` for Qwen3.5 reasoning
   models ([#1773](https://github.com/lmstudio-ai/lmstudio-bug-tracker/issues/1773),
   [#1971](https://github.com/lmstudio-ai/lmstudio-bug-tracker/issues/1971)) and a hard rejection of
   `json_object`. Enforced, but you must vet the model and read the right field.

**Enforced on paper, with a version floor and a silent-failure history:**

4. **Ollama (local, ≥ 0.31.2, thinking on unless ≥ 0.31.2, non-cloud)** — the native `format` field
   is a documented schema enforcement and works; but below 0.31.2 disabling thinking silently *and
   intermittently* disables it ([#14645](https://github.com/ollama/ollama/issues/14645)), and cloud
   models have no structured output at all. On the strength of the docs plus the fixed bug, I'd call
   this enforcement with a hard version/flag contract — but it is the weakest of the four "yes"es and
   is the one I would insist on an integration test for.

**Only prompt + validate + retry:**

5. **DeepSeek** — `json_object` guarantees JSON *syntax*, never Plainly's field names. The docs
   themselves require prompt-side scaffolding ("json" in the prompt + a JSON example), admit
   occasional **empty content**, and warn about whitespace-only runaway generations when the prompt
   doesn't ask for JSON. Round-tripping the artifact through `validate → retry` is not a fallback
   here; it is the mechanism.

## Gaps / could not verify

- **OpenAI's own schema-subset appendix.** OpenAI's structured-outputs page fetched truncated before
  its "Supported schemas" section and the `.md` variant ended at refusals; the `?api-mode=chat` URL
  returns a JS shell. The keyword table and the 100-properties / 5-levels figures above are from
  Microsoft's Azure OpenAI docs (updated 2026-08-24), which labels the numeric limits
  "Azure-specific". **I am not asserting OpenAI's numeric limits.** Everything else about OpenAI
  (surfaces, model floor, refusal semantics, JSON-mode comparison) is from OpenAI's own docs.
- **llama.cpp version history.** Which release introduced `response_format.json_schema`, `--json-schema`
  and the `strict` handling: not established. Also unverified: what the JSON→schema model layer does
  with `oneOf`, `not`, `if`/`then`/`else` and `dependentRequired` (the converter has no cases for
  them); whether `strict: true` is validated at all (the source never reads it); and the exact
  build-to-build behaviour of the flat `{"type":"json_schema","schema":…}` form (source on `master`
  says "any object", a 2026-09 blog reports fully unconstrained output on build `b10868`).
- **llama.cpp #29462 blame.** The issue carries no first-bad-commit and no maintainer comment; the
  two candidate fixes (#29497 draft/dirty, #25602 clean-but-unmerged) are both open. The apparent
  contradiction in #25602 between `merge_commit_sha` being set and `merged:false`/`closed_at:null`
  is unresolved; I treated it as unmerged.
- **Ollama's exact `/v1` → `format` mapping.** The docs assert structured outputs work via
  `response_format`, but never document the nesting or the honoured schema features for that route,
  and I did not read Ollama's server source. The `qwen3.5:397b-cloud` silent-failure report
  (pydantic-ai#4917) is *consistent with* the documented cloud limitation, but neither the thread
  nor I confirmed that link. Also unverified: whether native `/api/chat` accepts a per-request
  `options.num_ctx`; and whether the older FAQ still claims a flat 4096 default (the newer
  context-length page gives the VRAM tiers quoted above).
- **Ollama issue #15260 / PRs #15392, #15678** (the gemma4 sibling of #14645, mentioned inside a
  comment) were not fetched, so the claim "gemma4 fixed, qwen3.5 not" is second-hand.
- **LM Studio minimum version** for JSON-schema structured output (predates the published changelog),
  and whether `/v1/models` advertises a structured-output capability (only `tool_use` is shown as an
  example). Whether an undocumented structured-output field exists on the native `/api/v1/chat`
  endpoint. Whether `strict: false` changes anything (undocumented).
- **DeepSeek behaviour on an out-of-contract `type`.** The reference lists only `text`/`json_object`;
  whether `json_schema` is rejected with an error or ignored is untested here. The
  `tools[].function.strict` escape hatch is labeled beta and I found no production usage.
- **Not attempted:** running any of these servers (no local builds available in this session), so
  every "it works" above is documentary/source-level, not a reproduced end-to-end result. The
  Plainly artifact schema itself has not been compiled through any of these paths yet — that is the
  obvious next step for issue 01.
