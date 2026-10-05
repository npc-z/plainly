# Research: local/self-hosted LLMs + incumbent landscape

Provenance: research subagent, 2026-10-05, for the Wayfinder charting of `idea.md`.
Answered: (A) can "local model" carry the product, and what does it cost; (B) who already does
"select text anywhere -> AI explanation", and where the gap actually is.
Caveats are at the bottom and matter — read them before quoting numbers.

---

## A. LOCAL / SELF-HOSTED LLM SERVING

### A1. Runtimes with OpenAI-compatible APIs (Oct 2026)

- **Ollama**: `/v1/chat/completions`, `/v1/completions`, `/v1/models`, `/v1/embeddings`,
  `/v1/responses`, `/v1/systemone`; PLUS a separate Anthropic-compat layer. Default bind
  `127.0.0.1:11434`. Current release v0.35.1 (published 2026-09-29).
  https://docs.ollama.com/api/openai-compatibility | https://github.com/ollama/ollama/releases/tag/v0.35.1
- **LM Studio**: `/v1/models`, `/v1/responses`, `/v1/chat/completions`, `/v1/embeddings`,
  `/v1/completions`; port 1234; Anthropic-compat `/v1/messages`; headless "llmster" service mode;
  REST API exposes download-a-model, download status, load/unload, tokens/sec, TTFT, and
  "Idle TTL and Auto-Evict".
  https://lmstudio.ai/docs/developer/openai-compat | https://lmstudio.ai/docs/developer/core/headless
  https://lmstudio.ai/docs/developer/core/ttl-and-auto-evict | https://lmstudio.ai/docs/developer/rest/endpoints
- **llama.cpp (llama-server)**: OpenAI-compatible chat/completions/embeddings, JSON mode, GBNF
  grammars, `--api-key`, health/slots/metrics/props, model aliases, router mode, sleeping-on-idle.
  MIT, single binary -> **BEST FOR BUNDLING**.
  https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md
- **vLLM**: OpenAI-compatible server but datacenter-GPU oriented. NOT laptop-realistic.
  https://docs.vllm.ai/en/latest/serving/openai_compatible_server.html
- **Others**: Jan, Msty, Open WebUI, GPT4All, AnythingLLM, Faraday, LocalAI, llamafile, KoboldCpp, MLX.
  https://www.youngju.dev/transcribe/culture/2026-05-16-local-ai-on-device-llms-2026-ollama-lm-studio-jan-msty-open-webui-gpt4all-anythingllm-faraday-deep-dive.en
- **2026 note**: Ollama now ships MLX builds for Windows AND Linux, alongside ROCm. Ollama also has
  an official CLOUD mode (https://ollama.com/v1 with `OLLAMA_API_KEY`) — "Ollama" is no longer
  purely local, which affects any privacy story built on it.

### A2. Ollama's OpenAI-compat layer — exact coverage

`/v1/chat/completions` supported: chat completions, **streaming**, JSON mode, reproducible outputs
(seed), vision, tools, reasoning/thinking control. NOT supported: logprobs.

- Fields supported: `model`, `messages`, `frequency_penalty`, `presence_penalty`, `response_format`,
  `seed`, `stop`, `stream`, `stream_options.include_usage`, `temperature`, `top_p`, `max_tokens`,
  `tools`, `reasoning_effort`.
- Fields NOT supported: `tool_choice`, `logit_bias`, `user`, `n`. Image input is BASE64 ONLY.
- `/v1/completions`: supported (prompt must be a string; no `best_of`/`echo`/`logit_bias`/`user`/`n`).
- `/v1/embeddings`: supported (string or array of strings; no token arrays, no `user`).
- `/v1/responses`: added in v0.13.3, **NON-STATEFUL ONLY** — no `previous_response_id`, no
  conversation, no truncation.
- System prompts: ordinary `role:"system"` messages. Gemma 4 additionally has NATIVE system-role
  support (2026).
- Structured output: `response_format` supported; the OpenAI SDK's
  `client.beta.chat.completions.parse(..., response_format=PydanticModel)` works against
  `base_url=http://localhost:11434/v1`. Under the hood it's the native `format` field taking a JSON
  Schema. Ollama Cloud does NOT support structured outputs.
  https://docs.ollama.com/api/openai-compatibility | https://ollama.com/blog/structured-outputs
  https://docs.ollama.com/capabilities/structured-outputs

**TWO TRAPS**

1. **YOU CANNOT SET THE CONTEXT WINDOW VIA THE OPENAI-COMPAT API.** Official: "The OpenAI API does
   not have a way of setting the context size for a model." You must build a Modelfile with
   `PARAMETER num_ctx`, `ollama create`, and call that model name. This is the #1 trap for long hard
   text. https://docs.ollama.com/context-length
2. Context default is documented inconsistently: the FAQ says 4096 always; the newer Context-length
   page says VRAM-tiered (<24GiB -> 4k, 24-48GiB -> 32k, >=48GiB -> 256k). Net: an 8-16GB laptop
   gets ~4k unless you intervene. https://docs.ollama.com/faq

Alias trick: `ollama cp llama3.2 gpt-3.5-turbo` for clients that hardcode OpenAI model names.

### A3. Embedding caveats

- **Download UX**: Ollama `POST /api/pull` streams NDJSON with status/digest/total/completed -> real
  progress bars. LM Studio has `POST /download` + `GET /download-status`.
  https://docs.ollama.com/api/pull
- **Ports**: Ollama binds 127.0.0.1:11434 by default (`OLLAMA_HOST` to change); LM Studio 1234;
  llama.cpp 8080. Real conflict precedent: https://github.com/ollama/ollama/issues/9444 . Probe with
  `GET /api/version` or `/v1/models`.
- **CORS**: Ollama allows 127.0.0.1 and 0.0.0.0 origins by default; browser extensions need
  `OLLAMA_ORIGINS=chrome-extension://*,...`. A native Tauri process avoids CORS entirely —
  architectural advantage over the extension incumbents. https://docs.ollama.com/faq
- **Residency**: models unload after 5 MINUTES by default; `keep_alive` (or `OLLAMA_KEEP_ALIVE`)
  controls it; `keep_alive:-1` pins forever, `0` unloads immediately; preload by sending an EMPTY
  request. `OLLAMA_MAX_LOADED_MODELS` default 3; `OLLAMA_NUM_PARALLEL` default 1; `OLLAMA_MAX_QUEUE`
  default 512 and returns 503 when overloaded; RAM scales as NUM_PARALLEL x CONTEXT_LENGTH;
  `OLLAMA_KV_CACHE_TYPE=q8_0` halves KV memory with negligible loss. Local-only mode:
  `OLLAMA_NO_CLOUD=1` or `disable_ollama_cloud` in `~/.ollama/server.json`.
- **Cold start** (M5 Max 128GB, Q4, ctx 4096, median of 5; July 2026): 8B — llama.cpp cold load
  0.4s / cold first token 1.0s, MLX 1.8s / 2.1s; 32B — llama.cpp 1.1s / 2.4s, MLX 6.9s / 7.5s;
  70B — llama.cpp 2.3s / 4.7s, MLX 15.2s / 16.4s. Mechanism: llama.cpp mmaps the GGUF and faults
  pages lazily, so "load" looks instant and the cost lands in first prefill; MLX eagerly
  deserializes. => PIN ONE HOT MODEL and cold start basically disappears.
  Single vendor blog: https://contracollective.com/blog/local-llm-cold-start-model-load-latency-m5-max-mlx-llama-cpp-2026
- **License / bundling**: ollama/ollama is MIT (182,209 stars, pushed 2026-10-04). BUT issue #11634
  "Clarify license of the new ollama app" (2025-08-01, 47 thumbs-up) showed the GUI app was not
  obviously in the MIT repo and had no EULA; closed 2025-11-04 by the merge of PR #12933
  "app: add code for macOS and Windows apps under 'app'" (+102,956/-1,462, 212 files). UNCERTAIN: no
  published policy on bundling/redistributing the binary or on trademarks. SAFE DESIGNS:
  (a) detect-and-guide the user to install Ollama, or (b) sidecar llama.cpp (MIT, single binary).
  https://github.com/ollama/ollama/issues/11634 | https://github.com/ollama/ollama/pull/12933
- **llama.cpp gotcha to design around**: the JSON-schema/GBNF grammar builder does not bound
  repetition counts, so a contradictory/huge `min*` can allocate unboundedly and OOM-kill the
  server. https://github.com/ggml-org/llama.cpp/issues/29462

### A4. Models for this task (7-14B + small MoE)

- **Qwen3.5** (Ollama): 0.8b, 2b, 4b, 9b, 27b, 35b(MoE), 122b(MoE); all 256K context,
  vision+tools+thinking, MLX variants; 9b is `latest`. On-disk: 4b 3.3-4.0GB | 9b 6.6-7.6GB |
  27b 17-20GB | 35b 22GB. Family covers 201 languages. Flagship (397B-A17B): C-Eval 93.0,
  MMLU-ProX 84.7, WMT24++ 78.9 (XCOMET-XXL, 55-lang avg), IFEval 92.6, IFBench 76.5.
  https://ollama.com/library/qwen3.5
- **Gemma 4** (Google DeepMind, APACHE 2.0, 2026): five sizes — E2B (2.3B eff/5.1B), E4B (4.5B/8B),
  12B Unified (11.95B), 26B A4B MoE (25.2B total / 3.8B ACTIVE), 31B dense (30.7B). 128K ctx small /
  256K medium. NATIVE system role. Text+image all; audio on E2B/E4B/12B. MMLU-Pro: 12B 77.2,
  26B-A4B 82.6, 31B 85.2. https://huggingface.co/google/gemma-4-26B-A4B-it
- **INSTRUCTION FOLLOWING — THE KEY AXIS** (IFEval prompt-level strict / IFBench):
  Qwen3.5-27B 0.950 / 0.765 | Qwen3.5-35B-A3B 0.919 / 0.702 | Qwen3.5-9B 0.915 / 0.645 |
  Qwen3.5-4B —/0.592 | Gemma 3 27B 0.904 | Gemma 3 4B 0.902 | Mistral Small 4 —/0.480
  => **THE 9B DROPS 27 POINTS FROM IFEval TO IFBench.** A bespoke markdown contract lives in the
  IFBench "novel constraint" regime, not the IFEval regime. **DO NOT rely on prompting. USE
  schema-constrained decoding (Ollama `format`/`response_format`) + validate + retry, and
  temperature 0.** Ollama's own guidance says add "return as JSON" to the prompt too.
  Secondary aggregator: https://awesomeagents.ai/leaderboards/instruction-following-leaderboard/
  Primaries: https://llm-stats.com/benchmarks/ifeval | https://llm-stats.com/benchmarks/ifbench
  https://artificialanalysis.ai/evaluations/ifbench
- **HARDWARE TABLE** (memory floor = Ollama on-disk size):
  - 8GB RAM / no GPU -> `qwen3.5:4b` (3.3-4.0GB), ~4k ctx = **DEGRADED** (IFBench 0.592; truncation
    risk; shallow glosses)
  - 16GB unified or 8GB VRAM -> `qwen3.5:9b` (6.6-7.6GB) / Gemma 4 12B = **WORKABLE** with
    constrained decoding
  - 24-32GB unified or 12-16GB VRAM -> `qwen3.5:27b` (17-20GB), Gemma 4 26B-A4B, `qwen3.5:35b`
    (22GB) = **COMFORTABLE**; first tier where a single-pass strict contract is trustworthy
  - >=64GB -> `qwen3.5:122b` (81GB), diminishing returns
  - **MoE DOES NOT SAVE MEMORY**: Gemma 4 26B-A4B activates 3.8B but is 25.2B total; `qwen3.5:35b`
    is still 22GB on disk. MoE buys speed, not footprint — all experts must be resident.
- **GAP NOT CLOSED**: no rigorous current small-model EN->ZH benchmark found. Best proxy is Qwen's
  Chinese/multilingual strength (C-Eval 93.0, 201 languages) + Gemma 4's 140+ pretrain languages.
  **RECOMMEND YOUR OWN A/B** (Qwen3.5-9B vs Gemma 4 12B vs gpt-oss:20b) on ~100 held-out hard
  passages before locking the default. https://www.siliconflow.com/zh/articles/best-open-source-models-for-translation

### A5. Bottom line on local

Real useful fallback on 16GB+, genuinely good at 24GB+, **NOT a peer of cloud on 8GB**. On 8GB it is
a degraded fallback (4B + 4k default ctx truncates hard passages and drifts from format). The hard
engineering is ergonomics, not the model: port probing, download UX via `/api/pull` NDJSON, and
critically `num_ctx` (which the OpenAI-compat API cannot set). Cold start is a non-issue if you pin
one model with `keep_alive:-1`; the 15-second horror stories only happen when models are swapped
under memory pressure. **Recommended architecture: talk to a USER-INSTALLED Ollama by default
(probe `/api/version`, guide install if absent), keep an in-process llama.cpp sidecar as the
zero-setup bundling option.**

---

## B. INCUMBENT LANDSCAPE

### B1. Per-product — trigger / source-language explanation / history / local models

- **Immersive Translate**: Chrome+Edge+Firefox+Safari, iOS/Android; extension overlay + hover +
  selection + input-box; PRIMARILY TRANSLATION (bilingual), no paraphrase/gloss contract; no vocab
  notebook found; **LOCAL MODELS = YES**, official Ollama support since plugin v1.15.1 with
  OpenAI-compat custom URL `http://localhost:11434/v1/chat/completions`; 20+ engines incl.
  custom/OpenAI-compatible; huge surface (web, PDF layout-preserved "BabelDOC", PDF Pro, ePUB,
  subtitles, Zotero, Google Docs, Steam/AO3, image, manga, video, YouTube Live, meetings); free tier
  + Pro. https://immersivetranslate.com/en/ | https://immersivetranslate.com/en/docs/services/ollama/
  https://immersivetranslate.com/en/docs/services/ai/ | https://immersivetranslate.com/en/pricing/
- **Bob** (macOS only, menu bar, CLOSED SOURCE — repo is feedback-only): hotkeys 划词翻译 ⌥D,
  screenshot translate ⌥S, input translate ⌥A, silent screenshot OCR ⌥C, PopClip; translation + OCR
  only; no history; no local models. https://raw.githubusercontent.com/ripperhe/Bob/master/README.md
- **Raycast** (macOS + Windows): global hotkey with "Use Selected Text as Source"; Root Search
  inline ("hello in german"); PARTLY explains — "Continue in AI Chat ... get an explanation of an
  idiom"; Clipboard History is not a learning notebook; **LOCAL MODELS = YES**, Pro feature via
  Ollama, auto-detects models and "Raycast starts the Ollama server for you", remote Ollama host,
  Custom Providers, BYOK. https://manual.raycast.com/translate | https://manual.raycast.com/ai/local-models
- **ChatGPT desktop** (macOS, Windows): app capture; 2026 "Appshots" to capture/extract text;
  general assistant, no fixed contract; no local models. https://openai.com/chatgpt/desktop/
  https://chatgpt.com/features/desktop/
- **DeepL** (Windows/macOS desktop apps + web/extension; DeepL Voice): translation + DeepL Write +
  glossaries; no source-language pedagogy; glossary not a vocab notebook; no local models.
  https://www.deepl.com/ar/academy/deepl-desktop-app
- **Trancy** (extension + web learning center): PARTLY explains — Premium has "AI Precise Word
  Definitions", "AI sentence parsing", "AI grammar analysis"; wordbook + flashcards + collections +
  word import; Premium from ~$3.49/mo, Premium+Advanced AI ~$8.79/mo; custom translation engines
  exist, no documented Ollama page.
  https://manual.trancy.org/en/billing-and-plans/premium.md
- **Relingo** (Chrome/Edge/Firefox + iOS/Android): auto-highlights words by DIFFICULTY LEVEL while
  browsing/reading/watching; PARTLY explains — Premium has "English-english dictionary", "Advanced
  word explains", "AI grammar analyze"; STRONG history: personal word bank, flashcards, study
  reports, **ANKI EXPORT**; credits unlock DeepL/GPT/Claude/Gemini/DeepSeek/Bing.
  https://relingo.net/en | https://relingo.net/en/pricing
- **Readwise Reader** (web + desktop + mobile): Quick Lookup on MOBILE ONLY (select 1-3 words ->
  Define/Lookup/Translate, no prompt); custom Ghostreader prompts; Global Ghostreader on web +
  desktop; full searchable library with highlights/tags; BYO OpenAI key only, default GPT-5 Mini.
  https://docs.readwise.io/reader/docs/faqs/ghostreader | https://docs.readwise.io/reader/guides/ghostreader/quick-lookup
- **Language Reactor** (desktop Chrome extension; Safari/iOS variants): Netflix/YouTube dual
  subtitles + PhrasePump; translation-first; saving words outside PhrasePump needs Pro; no local
  models. https://www.languagereactor.com/
- **Eudic 欧路词典** (PC/Mac/iOS/Android/**LINUX** + Chrome/Edge/Firefox ext; v26.9.1): Mac 屏幕取词
  + 划词搜索, PDF bilingual reader; PARTLY explains — configurable LLM engines WITH YOUR OWN API KEY,
  custom style prompts, personal terminology base, AI 深度学习 section; vocab notebook + notes with
  cross-platform sync; not documented as Ollama. https://www.eudic.net/v4/en/app/eudic

### B2. 2026 entrants + open source

- **Lexirise** (2026): extension (Chrome/Firefox/Safari) + iOS/Android; tappable subtitles on
  Netflix/Crunchyroll/YouTube/Prime/Bilibili; reads speech bubbles on WEBTOON/manga; podcasts w/
  transcripts; 26 languages, deepest JA/KO/ZH; pinyin/furigana + frequency + HSK tags; FREE UNCAPPED
  lookups and saved words; Pro $14.99/mo or $79/yr w/ Anki+CSV export. Strongest 2026 challenger in
  reading/vocab. https://lexirise.app/zh-hans/blog/article/free-lingq-alternative
- **Readlang**: free plan unlimited word translations + flashcards, 10 phrase translations AND 10
  "context explanations" per day; Premium $6/mo; Premium Plus $15/mo adds "a stronger AI model".
  Closest incumbent to the planned feature.
- **Migaku** $10/mo or $96/yr, no free plan. **LingQ** free plan = 20 saved words FOR THE LIFE OF
  THE ACCOUNT, Premium $14.99/mo. **Lute v3** = MIT, self-hosted, AnkiConnect only.
  All four: https://lexirise.app/zh-hans/blog/article/free-lingq-alternative
- **Open source** (GitHub API, checked 2026-10-05):
  - **pot-desktop** — cross-platform 划词翻译+OCR, **TAURI**, GPL-3.0, 19,405 stars, **ARCHIVED**
    (last push 2026-07-04). Community Ollama plugin exists. https://github.com/pot-app/pot-desktop
  - **Easydict** — macOS/Swift/GPL-3.0, 14,841 stars, ACTIVE (pushed 2026-10-04). Offline OCR +
    many cloud engines; no Ollama documented. https://github.com/tisfeng/Easydict
  - **STranslate** — Windows/WPF/MIT, 8,168 stars. Has OFFICIAL Ollama translate plugin AND Ollama
    OCR plugin. https://github.com/STranslate/STranslate
  - KISS Translator, FluentRead — browser-extension translators.
- **STRATEGIC**: the ONLY cross-platform open-source analog (pot-desktop, Tauri, 19.4k stars) is
  archived; the three living equivalents are each locked to one OS (STranslate=Windows,
  Easydict=macOS, Bob=macOS+closed). **Verified hole as of Oct 2026.**

### B3. Wedge vs waste

**CREDIBLE WEDGES**

1. **SOURCE-LANGUAGE EXPLANATION AS THE PRODUCT.** Every major incumbent is translation-first.
   Source-language features appear only as Premium side-features (Relingo's English-English
   dictionary / Advanced word explains / AI grammar analyze; Trancy's AI word definitions + sentence
   parsing + grammar analysis; Readwise's mobile-only Quick Lookup Define; Raycast's AI Chat idiom
   explanation; Readlang's 10/day context explanations). NONE treats "hard passage -> level-matched
   paraphrase + idiom/collocation gloss + grammar note + zh translation as one contract-shaped
   artifact" as the unit of work. **No incumbent advertises CEFR/reading-LEVEL MATCHING of the
   paraphrase.**
2. **UNIT OF WORK = A PASSAGE**, not a word, page, or subtitle track. Incumbents cluster at
   page/document translation or word lookup; nobody optimizes "make this hard paragraph
   comprehensible at my level."
3. **CROSS-PLATFORM DESKTOP (Win+mac+Linux)** with global hotkey + clipboard watch —
   evidence-backed gap, see B2.
4. **LOCAL-FIRST PRIVACY + USER-OWNED PORTABLE HISTORY** (SQLite you own, export to
   markdown/Anki), with provable no-egress via `OLLAMA_NO_CLOUD=1`.

**WASTED EFFORT**

1. Translation/document breadth (Immersive Translate's free tier covers web+PDF+ePUB+subtitles+
   Zotero+image+manga+video+meetings, 20+ engines, 100+ language pairs; Eudic adds Linux + deep
   offline dictionaries + PDF bilingual layout).
2. "Another select-to-translate with a cloud LLM" — commoditized and FREE at entry tier.
3. **Making local/offline the headline differentiator.** Immersive Translate officially supports
   Ollama (v1.15.1+) and Raycast ships Local Models that even starts the server for you. Local is
   becoming a checkbox.
4. Video/subtitle mining (Language Reactor free, Trancy, Migaku, Lexirise, Relingo).
5. **SRS/flashcards as the core loop** (Anki, Relingo, Trancy, LingQ, Lexirise, Migaku, Eudic).
   **Ship EXPORT instead.**
6. A browser extension for general immersive translation — saturated.
7. "Chinese learners are underserved" — Eudic/欧路 is a full-stack Chinese-learner product (screen
   word capture, 30万+ ECE entries + 40万 professional, BYO-API-key LLM engines, custom style
   prompts, terminology base, syllable-level pronunciation scoring, cross-platform sync, Linux),
   plus Bob, Relingo, Trancy, Youdao.

**POSITIONING THAT SURVIVES THE EVIDENCE**

> "The only cross-platform desktop app that turns a hard English passage into a level-matched
> ENGLISH explanation — paraphrase, idiom glosses, grammar notes — with translation as a SECONDARY
> artifact, running fully offline and storing everything in a SQLite file you own."

---

## EVIDENCE CAVEATS

Could NOT fetch (403 / JS-only / paywalled): OpenAI desktop + help pages, Chrome Web Store listings
(Language Reactor, 欧路翻译), DeepL academy, Language Reactor's own site (empty JS shell), Immersive
Translate's pricing TIERS (page exists, free tier + Pro, exact numbers not extractable), the
newsbytes ChatGPT Appshots report. The instruction-following leaderboard is a SECONDARY aggregator
of llm-stats.com and Artificial Intelligence primaries (both given above). The cold-start benchmark
is a single vendor blog; treat absolute laptop numbers as indicative.

**THE ONE REAL GAP**: no verified small-model EN->ZH benchmark — close that before locking the
model default.
