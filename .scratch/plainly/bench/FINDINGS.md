# Findings — prompt calibration across a local 4B and a cloud model

Measured for ticket 14. Raw data: `results/<provider>/<prompt>.json` — one file per run, carrying
every request outcome plus the run summary.
**192 requests total**: local llama.cpp 6 variants × 12 (72), DeepSeek 5 variants × 12 (84, one of
which retried 3× on every passage), then the thinking-off reruns (3 × 12 = 36).

| | local | cloud |
|---|---|---|
| runtime | llama.cpp router, `--ctx-size 16384`, RTX 3060 Laptop | `https://api.deepseek.com/v1` |
| model | `unsloth/Qwen3-4B-Instruct-2507-GGUF:Q4_K_M` | `deepseek-flash` |
| schema enforcement | **yes** — nested `response_format.json_schema.schema` | **none** — `response_format.type` is `json_object` only |
| decode | temperature 0, max_tokens 2048 | temperature 0 — **silently ignored in thinking mode**, see below — max_tokens 8192 |
| note | `--models-max 1`, 900s idle unload → first call is a cold start | **reasoning model**: 1304 reasoning tokens to emit 115 tokens of JSON on a one-line passage |

## Metrics

| provider | prompt | invalid | verbatim paraphrase | grammar fired | mean glosses | `expression` ≠ substring | mean s |
|---|---|---|---|---|---|---|---|
| local | v1 one-shot | 0/12 | 0/12 | 0/12 | 1.92 | 2 | 1.8 |
| local | v2 grammar-explicit | 0/12 | 0/12 | 0/12 | 1.75 | 1 | 1.7 |
| local | v3 staged | 0/12 | **5/12** | 1/12 | 2.17 | 1 | 2.1 |
| local | v4 cautious | 0/12 | 0/12 | 0/12 | 1.67 | **4** | 1.7 |
| local | v5 cautious+understatement | 0/12 | 0/12 | 0/12 | 1.75 | 2 | 1.7 |
| local | **v6 synthesis** | 0/12 | 0/12 | 0/12 | 1.58 | 1 | 1.7 |
| cloud | v1 one-shot | 0/12 | 0/12 | 8/12 | 3.25 | 0 | 12.5 |
| cloud | v2 grammar-explicit | 0/12 | 0/12 | **11/12** | 3.50 | 0 | 12.3 |
| cloud | v3 staged | **12/12** | 0/12 | 6/12 | 3.67 | 0 | 18.1 |
| cloud | v5 cautious+understatement | 0/12 | 0/12 | 10/12 | 3.75 | 0 | 9.8 |
| cloud | **v6 synthesis** | 0/12 | 0/12 | 10/12 | **4.00** | 0 | 12.8 |

`verbatim paraphrase` = the "comprehensible" field is byte-identical to the input, i.e. the section
gives the learner nothing. `expression` ≠ substring = a gloss quotes something that does not appear
in the passage.

## The five findings

**1. The local model's failures are capability, not prompt.** Every defect survived all six prompts on
the 4B, and every one was handled correctly by the cloud model under the *same* prompts:

| passage | local 4B (all variants) | cloud |
|---|---|---|
| p02 `bottled it` | "kept something secret" — **wrong** (5/5) | "lost your nerve … failed to do something because you were afraid" |
| p04 `genteel` | "polite" — **wrong** (5/5) | "refined, respectable, or upper-class; dignified shabbiness" |
| p04 `repaired to` | missed, or misquoted as "repired" | "'repaired' here is a formal word meaning 'went', not 'fixed'" |
| p04 `the Continent` | lower-cased, sense lost | "mainland Europe, especially as referred to by British speakers" |
| p06 `indemnify` / lessor | confused; the lessor vanishes from the paraphrase | both parties kept; even glosses legal `shall` |
| p08 `to put it mildly` | flattened in paraphrase and translation | "used to say that you are choosing gentle words, even though the real situation is stronger" |
| p09 invented detail | "the missing part" (invented) | not invented |

**2. `grammar` is not a dead field — the 4B just never takes the structural route.** Local: 0/12 under
five of six prompts (1/12 staged). Cloud: 8–11/12, and the notes are genuinely useful (participial
clauses, the legal `indemnify X against Y` frame, parenthetical insertion). **v2's explicit wording
raises cloud firing from 8/12 to 11/12** — so the rewording is a real lever, it simply cannot reach a
4B's ceiling.

**3. The staged prompt (v3) fails on both providers, differently — and that is the lesson.**

- On the **enforced** path it cannot drift, so it fails *content-wise*: 5/12 paraphrases are the
  original, verbatim.
- On the **unenforced** path it drifts *structurally*: it renamed the field to `paraphrase`
  (the prompt says "Step 2 — paraphrase"), so the contract broke **12/12**.

**4. Retry is not a remedy for systematic drift.** On DeepSeek at temperature 0 the renamed key
reproduced **3 times out of 3** on every passage — 24 wasted calls. Validation-and-retry helps with
stochastic malformation; a deterministic contract violation needs enforcement or a prompt that names
the exact keys. (This is the input ticket 04's retry policy needs.)

**5. Two prompt rules are independently evidenced**, and neither depends on the other:

- **v4/v5's accuracy rules** — "never add detail the passage does not contain" removed p09's invented
  phrase; "keep proper nouns: 'the Continent' is not 'a continent'" restored the capital and the
  sense; the understatement rule restored the `to put it mildly` gloss.
- **v2's grammar wording** — the proven lever on grammar firing.

`v6-synthesis` is those two levers unioned, with "it must be a real restatement: never return the
passage unchanged" kept as insurance against the v3 failure mode. On 12 passages the top three
variants are **statistically indistinguishable** (grammar 10 vs 11, glosses 3.75 vs 4.00); v6 is the
rational pick because each of its parts is separately evidenced, not because it outranked the others.

## Thinking mode — the single biggest lever (measured, not assumed)

`deepseek-flash` thinks by default (effort `high`). Toggling it off —
`{"thinking": {"type": "disabled"}}` — is worth **10× latency and 15× output tokens**, and what it
costs is *thoroughness*, not *correctness*:

| | thinking ON | thinking OFF |
|---|---|---|
| mean latency | 9.8–12.8 s | **1.27–1.38 s** |
| mean output tokens | 1942–2610 | **172–178** |
| contract | 12/12 | 12/12 |
| `grammar` fired | 10–11/12 | 3–4/12 |
| mean glosses | 3.75–4.00 | 2.75–2.83 |
| `expression` ≠ substring | 0 | 0 |
| `bottled it`, `genteel`, `indemnify`, `to put it mildly`, p09 invention | all correct | **all still correct** |

Thinking off trades **how many blockers it reports** (and most of the grammar notes) for speed. It
does not reintroduce any of the failures that broke the 4B — `p03`'s inversion is still explained
correctly with thinking off.

**Traps, probed against the live API:** `{"enable_thinking": false}` is silently accepted and
**ignored** (`reasoning_tokens` stayed at 20); `{"thinking": false}` is a 422
(`expected struct ThinkingOption`). The documented form is `{"thinking": {"type": "disabled"}}`;
`{"reasoning_effort": "none"}` also works.

**Correction to the table at the top:** in thinking mode `temperature`, `presence_penalty` and
`frequency_penalty` are **silently ignored** (official docs), so the thinking-on runs were *not* at
temperature 0 as recorded there. The conclusion those runs support is unaffected, and in fact
strengthened: v3's renamed key reproduced **3/3 times without** temperature 0 — systematic drift, not
sampling noise. With thinking off, `temperature: 0` is honoured again.

## Provider profiles (for the product, not just the experiment)

With thinking off the cloud path is **1.3 s** — as fast as the local model — while being right where
the 4B is wrong.

- **Cloud, thinking off**: 1.3 s, ~175 output tokens, ~2.8 glosses, correct on every idiom and
  register trap measured, unenforced contract.
- **Local**: 1.7 s, offline, enforced contract, 1.6–1.9 glosses — and **confidently wrong on idioms
  and register**, which is exactly where a learner needs help.
- **Cloud, thinking on**: 10–13 s, 15× the tokens, 4.0 glosses and 10–11/12 grammar — the thorough
  option, not the default one.

They are no longer ranked by speed: the cloud path is the accurate one, the local path is the offline
one. The same prompt serves both.

## The two parameters have very different teeth (ticket 09)

The shipped prompt has exactly two parameter sites — `{{LEVEL}}` (twice) and `{{NATIVE}}` (twice).
Measured with `BENCH_LEVEL` / `BENCH_NATIVE` (now env-overridable), `deepseek-flash`, thinking off,
temperature 0, on four passages spanning registers (p02 idioms, p04 literary, p06 legal, p12 formal).
24 requests across six runs, no retries needed (contract 4/4 every time).

**Native language is a strong, clean knob.** With `BENCH_NATIVE=Japanese`, all four translations came
back in Japanese (43–49 kana/kanji) while `comprehensible` and **every gloss stayed English**, 4/4.
The product's promise — explain hard English in English — is not contaminated by the native-language
setting. (The risk worth checking was the glosses leaking, not the translation switching.)

**Level is a weak, coarse knob, and it needs its descriptor to do anything at all.**

| | levels collapsing to byte-identical text | mean word length A2/B2/C1 | mean similarity to the original A2/B2/C1 |
|---|---|---|---|
| label only (`A2`, `B2`, `C1`) | **3 of 8** adjacent pairs | 4.29 / 4.27 / 4.26 | 0.460 / 0.475 / 0.463 |
| label + descriptor | **1 of 8** | **4.05** / 4.32 / 4.29 | **0.395** / 0.463 / 0.462 |

The descriptors are the source contract's own: A1–A2 "very common words, short sentences, concrete";
B2 "English-first; nuance and collocations"; C1+ "keep most of the original structure; subtle
meaning, register, idiom, style".

- **A bare label is close to sending nothing**: two of the three levels produced identical bytes on
  p04, p06 and p12.
- **Inlining the descriptor separates A2** from the other two — mean word length 4.29 → 4.05 and
  similarity to the original 0.460 → 0.395, i.e. a more thorough restatement. Two of the three
  collapse pairs disappear.
- **B2 and C1 stay indistinguishable** (4.32 vs 4.29; 0.463 vs 0.462). The likely mechanism is a
  contradiction inside the contract: C1's descriptor says "keep most of the original structure",
  while the prompt's own rule says "it must be a real restatement: never return the passage
  unchanged". The rule wins.
- Even A2 is not reliably the simplest: on p06 and p12 its mean word length is the *highest* of the
  three.

Caveat: four passages, one model, one run per condition. Temperature 0 makes each row reproducible,
and "two levels produced identical bytes" is not sampling noise — but the direction is a tendency,
not a law. The takeaway is not "level does nothing"; it is **"level is a hint, not a contract, and it
has to ship with its descriptor"**.

