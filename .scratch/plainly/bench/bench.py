#!/usr/bin/env python3
"""PROTOTYPE-GRADE benchmark harness for tickets 03 and 14. Not production code.

Question: can a given model hold the Explanation contract from ticket 01, and
which prompt variant does it hold best?

  .scratch/plainly/bench/          <- you are here
  python3 bench.py                          # local llama.cpp, default prompt
  BENCH_PROMPT=v5-... python3 bench.py      # a prompt variant
  BENCH_PROVIDER=deepseek DEEPSEEK_API_KEY=... BENCH_MODEL=... python3 bench.py
  python3 bench.py p03 p08                  # a subset

Records every request outcome plus the run summary as **one JSON file per run**,
`results/<provider>/<prompt>.json`. One file rather than one per passage: 168
per-passage files for 14 runs was clutter, and nothing was lost by merging them.
Stdlib only: no pip, no venv, no downloads.
"""

import json
import os
import sys
import time
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))

# Which prompt variant to send; the files live in prompts/<name>.txt.
PROMPT = os.environ.get("BENCH_PROMPT", "v1-one-shot")

# Which backend to talk to. They differ in whether a schema can be enforced at
# all -- itself one of the things under measurement (ticket 10).
PROVIDER = os.environ.get("BENCH_PROVIDER", "llamacpp")

if PROVIDER == "deepseek":
    BASE = os.environ.get("BENCH_BASE", "https://api.deepseek.com/v1").rstrip("/")
    MODEL = os.environ.get("BENCH_MODEL", "deepseek-chat")
    API_KEY = os.environ.get("DEEPSEEK_API_KEY", "")
else:
    BASE = os.environ.get("LLAMA_BASE", "http://127.0.0.1:3060").rstrip("/")
    MODEL = os.environ.get(
        "LLAMA_MODEL", "unsloth/Qwen3-4B-Instruct-2507-GGUF:Q4_K_M"
    )
    API_KEY = os.environ.get("LLAMA_API_KEY", "")

# The shipped prompt's two structured parameters ({{LEVEL}}, {{NATIVE}}).
# Overridable so one run can vary one of them -- that is how we find out whether
# a knob actually moves the output (ticket 09).
LEVEL = os.environ.get("BENCH_LEVEL", "B2")
NATIVE = os.environ.get("BENCH_NATIVE", "Chinese")
ATTEMPTS = int(os.environ.get("BENCH_ATTEMPTS", "3"))
# Reasoning models spend most of their completion budget before the artifact:
# deepseek-flash burned 1304 reasoning tokens to produce 115 tokens of JSON on a
# one-line passage. 2048 truncates the longer ones.
MAX_TOKENS = int(os.environ.get("BENCH_MAX_TOKENS", "2048"))

# Extra request fields as a JSON object, merged into the body last. Used to turn
# thinking off: BENCH_EXTRA='{"thinking":{"type":"disabled"}}'. Verified against
# api.deepseek.com: {"thinking":{"type":"disabled"}} and {"reasoning_effort":
# "none"} both drop reasoning_tokens to zero, while {"enable_thinking":false} is
# SILENTLY IGNORED and {"thinking":false} is a 422.
EXTRA = json.loads(os.environ.get("BENCH_EXTRA", "{}"))

# Appended to the result directory name, so a variant run under different request
# settings does not overwrite the same-named run.
SUFFIX = os.environ.get("BENCH_SUFFIX", "")

# Shape fixed by ticket 01, bounds deliberately absent per docs/adr/0001. Lives
# in schema.json so the spec and the harness cannot drift apart.
with open(os.path.join(HERE, "schema.json"), encoding="utf-8") as _fh:
    SCHEMA = json.load(_fh)

# Set in main() from prompts/<variant>.txt; see BENCH_PROMPT.
SYSTEM = ""


def load_prompt(variant):
    with open(os.path.join(HERE, "prompts", variant + ".txt"), encoding="utf-8") as fh:
        return fh.read().replace("{{LEVEL}}", LEVEL).replace("{{NATIVE}}", NATIVE)


def build_body(passage):
    body = {
        "model": MODEL,
        "messages": [
            {"role": "system", "content": SYSTEM},
            {"role": "user", "content": passage},
        ],
        "temperature": 0,
        "max_tokens": MAX_TOKENS,
    }
    if PROVIDER == "deepseek":
        # Ticket 10: DeepSeek enforces no schema. response_format.type accepts
        # only text/json_object, and JSON mode wants the word "json" in the
        # prompt (every variant says "a single JSON object"). So this path is
        # prompt + validate + retry, which is exactly what ATTEMPTS is for.
        body["response_format"] = {"type": "json_object"}
    else:
        # NESTED shape on purpose: ticket 10 verified in llama.cpp's source that
        # the flat {"type":"json_schema","schema":{...}} form its own README
        # documents is silently ignored and degrades to "any JSON object".
        body["response_format"] = {
            "type": "json_schema",
            "json_schema": {"name": "explanation", "schema": SCHEMA},
        }
    body.update(EXTRA)
    return body


def call(passage, timeout=180):
    headers = {"Content-Type": "application/json"}
    if API_KEY:
        headers["Authorization"] = "Bearer " + API_KEY
    req = urllib.request.Request(
        BASE + "/chat/completions",
        data=json.dumps(build_body(passage)).encode(),
        headers=headers,
    )
    t0 = time.time()
    with urllib.request.urlopen(req, timeout=timeout) as r:
        payload = json.loads(r.read())
    return payload, time.time() - t0


def validate(obj):
    """Check the artifact against the contract by hand. Returns a list of errors."""
    errs = []
    if not isinstance(obj, dict):
        return ["top level is not an object"]
    for key in ("comprehensible", "glosses", "grammar", "translation"):
        if key not in obj:
            errs.append(f"missing key: {key}")
    extra = set(obj) - {"comprehensible", "glosses", "grammar", "translation"}
    if extra:
        errs.append(f"unexpected keys: {sorted(extra)}")

    comp = obj.get("comprehensible")
    if not isinstance(comp, str) or not comp.strip():
        errs.append("comprehensible: not a non-empty string")

    glosses = obj.get("glosses")
    if not isinstance(glosses, list):
        errs.append("glosses: not an array")
    else:
        for i, item in enumerate(glosses):
            if not isinstance(item, dict):
                errs.append(f"glosses[{i}]: not an object")
            elif set(item) != {"expression", "gloss"}:
                errs.append(f"glosses[{i}]: keys are {sorted(item)}")
            elif not all(isinstance(v, str) and v.strip() for v in item.values()):
                errs.append(f"glosses[{i}]: empty value")

    grammar = obj.get("grammar")
    if not (grammar is None or isinstance(grammar, str)):
        errs.append("grammar: neither string nor null")

    trans = obj.get("translation")
    if not isinstance(trans, str) or not trans.strip():
        errs.append("translation: not a non-empty string")
    return errs


def one_passage(p):
    """Call with retry. Returns a record. Contract failures retry, because the
    DeepSeek path has no schema enforcement and needs it."""
    text = p["text"]
    last = None
    for attempt in range(1, ATTEMPTS + 1):
        try:
            payload, secs = call(text)
        except (urllib.error.URLError, urllib.error.HTTPError) as exc:
            last = {
                "errors": [f"request failed: {exc}"],
                "raw_content": "",
                "artifact": None,
                "latency_s": None,
                "finish_reason": None,
                "usage": {},
                "attempts": attempt,
            }
            continue

        choice = payload["choices"][0]
        msg = choice.get("message", {})
        content = msg.get("content") or ""
        note = ""
        # LM Studio's documented trap routes structured output into
        # reasoning_content and leaves content empty; guard against it here too.
        if not content.strip() and msg.get("reasoning_content"):
            content = msg["reasoning_content"]
            note = "took reasoning_content"
        try:
            obj = json.loads(content)
            errs = validate(obj)
        except Exception as exc:  # noqa: BLE001 - prototype
            obj, errs = None, [f"JSON parse failed: {exc}"]

        last = {
            "errors": errs,
            "raw_content": content,
            "artifact": obj,
            "latency_s": round(secs, 2),
            "finish_reason": choice.get("finish_reason"),
            "usage": payload.get("usage", {}),
            "attempts": attempt,
            "note": note,
        }
        if not errs:
            break
    return last


def main():
    global SYSTEM
    SYSTEM = load_prompt(PROMPT)

    with open(os.path.join(HERE, "passages.json"), encoding="utf-8") as fh:
        passages = json.load(fh)["passages"]
    wanted = sys.argv[1:]
    if wanted:
        passages = [p for p in passages if p["id"] in wanted]

    out_dir = os.path.join(HERE, "results", PROVIDER)
    os.makedirs(out_dir, exist_ok=True)

    if not API_KEY and PROVIDER == "deepseek":
        print("!! BENCH_PROVIDER=deepseek but DEEPSEEK_API_KEY is empty; expect 401")

    rows = []
    print(f"provider: {PROVIDER}")
    print(f"prompt  : {PROMPT}")
    print(f"model   : {MODEL}")
    print(f"base    : {BASE}")
    print(f"running : {len(passages)} passage(s)\n")
    for i, p in enumerate(passages, 1):
        pid = p["id"]
        out = one_passage(p)
        record = {
            "id": pid,
            "kind": p.get("kind"),
            "text": p["text"],
            "provider": PROVIDER,
            "model": MODEL,
            "prompt": PROMPT,
            "valid": not out["errors"],
            **out,
        }
        rows.append(record)
        n_gloss = len((record.get("artifact") or {}).get("glosses") or [])
        print(
            f"[{i}/{len(passages)}] {pid} {'OK  ' if record['valid'] else 'FAIL'} "
            f"{record['latency_s']}s glosses={n_gloss} "
            f"{record.get('finish_reason') or ''} "
            f"attempts={record.get('attempts')} {record.get('note') or ''}"
        )
        for e in record["errors"]:
            print(f"        ! {e}")

    valid = sum(1 for r in rows if r["valid"])
    lat = [r["latency_s"] for r in rows if r["latency_s"]]
    summary = {
        "provider": PROVIDER,
        "model": MODEL,
        "base": BASE,
        "prompt": PROMPT,
        "level": LEVEL,
        "native_language": NATIVE,
        "temperature": 0,
        "n": len(rows),
        "valid": valid,
        "contract_pass_rate": round(valid / len(rows), 3) if rows else None,
        "latency_s": {
            "first": lat[0] if lat else None,
            "min": min(lat) if lat else None,
            "max": max(lat) if lat else None,
            "mean": round(sum(lat) / len(lat), 2) if lat else None,
        },
        "attempts_total": sum(r.get("attempts") or 0 for r in rows),
    }
    summary["records"] = rows
    out = os.path.join(HERE, "results", PROVIDER, PROMPT + SUFFIX + ".json")
    with open(out, "w", encoding="utf-8") as fh:
        json.dump(summary, fh, ensure_ascii=False, indent=2)

    print(f"\ncontract pass rate: {valid}/{len(rows)} = {summary['contract_pass_rate']}")
    print(
        f"latency first={summary['latency_s']['first']}s "
        f"min={summary['latency_s']['min']}s "
        f"max={summary['latency_s']['max']}s "
        f"mean={summary['latency_s']['mean']}s"
    )
    print(f"run file: {os.path.join('results', PROVIDER, PROMPT + SUFFIX + '.json')}")


if __name__ == "__main__":
    main()
