#!/usr/bin/env python3
"""A fake OpenAI-compatible provider on loopback, for manual end-to-end checks.

The automated tests have their own stub (`crates/plainly-cli/tests/support/provider.rs`),
but the panel and the main window cannot be tested automatically, and the checks
in `docs/manual-checks.md` want a real Explanation to come back so that the panel
reaches its result state, 复制 has something to copy and ⟳ has something to
regenerate. This is that provider: it answers exactly the way a runtime does —
`GET /v1/models` for the model list, `POST /v1/chat/completions` with a
contract-shaped answer — with no network beyond loopback and no API key.

    plainly providers discover                  # stdout: the runtimes that answered
    python3 scripts/stub-provider.py --port 8080 &   # a port that did not
    plainly config set providers.llamacpp.endpoint http://127.0.0.1:8080/v1
    plainly config set providers.llamacpp.model stub-model
    plainly config set app.provider llamacpp

**8080 is a default, not a workspace convention — check what your machine already
serves.** A real llama.cpp started with `--port 3060` (the port this project's own
bench used) owns 3060, and a runtime that owns a port must keep it: two things
cannot listen there. Run `plainly providers discover` and read the addresses it
prints before picking one; if your runtime is on 8080 instead, start this stub
with `--port 8081` and point `providers.llamacpp.endpoint` at it. Binding a port
somebody else owns fails loudly here rather than quietly answering for them.

Every request is logged to `stub-provider.requests.log` beside this script, which
is what makes the cache checks checkable: explaining the same Passage twice must
leave the log one line longer, not two.
"""

import argparse
import http.server
import json
import pathlib
import sys

# A contract-shaped answer: the fields the Explanation schema names, and no
# `grammar`, which is also the shape a local model most often returns. The
# Passage it pretends to have read is "…the manager had already gone to ground."
ANSWER = json.dumps(
    {
        "comprehensible": "The committee tried hard to find out what had happened, "
        "but the manager had already gone into hiding.",
        "glosses": [
            {
                "expression": "go to ground",
                "gloss": "hide so that nobody can find you",
            }
        ],
        "grammar": None,
        "translation": "委员会对此事进行了彻底调查，但那位经理已经躲了起来。",
    }
)

LOG = pathlib.Path(__file__).with_suffix(".requests.log")


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def _json(self, body, code=200):
        raw = json.dumps(body).encode()
        self.send_response(code)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def do_GET(self):
        self._record("GET")
        self._json({"data": [{"id": "stub-model"}]})

    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("content-length", 0)))
        self._record("POST", body)
        self._json({"choices": [{"message": {"content": ANSWER}}]})

    def _record(self, method, body=None):
        # One line per request, flushed: the whole point is to be readable while
        # it runs. The body is truncated — a check reads the paths, not the
        # prompt.
        line = method + " " + self.path
        if body is not None:
            line += " " + body.decode(errors="replace")[:160]
        with LOG.open("a") as log:
            log.write(line + "\n")

    def log_message(self, *args):
        pass


def main():
    # `RawDescriptionHelpFormatter`: the docstring is a setup sequence, and the
    # default reflow turns it into one paragraph nobody can follow.
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    # llama.cpp's own upstream default, which is also the port `providers
    # discover` attributes to the llamacpp preset. Deliberately not 3060: that is
    # where this project's bench ran a *real* llama.cpp, and a machine that still
    # has one running there must keep it. See the module docstring.
    parser.add_argument("--port", type=int, default=8080)
    parser.add_argument("--host", default="127.0.0.1")
    args = parser.parse_args()

    try:
        server = http.server.HTTPServer((args.host, args.port), Handler)
    except OSError as error:
        print(
            f"cannot listen on {args.host}:{args.port} ({error}).\n"
            "Something else already owns that port — very likely a real local "
            "runtime.\nRun `plainly providers discover` to see which addresses "
            "answer, then start this stub on a free one with `--port` and point "
            "the provider's endpoint at it.",
            file=sys.stderr,
        )
        return 1

    print(
        f"stub provider on http://{args.host}:{args.port}/v1, "
        f"logging to {LOG}",
        file=sys.stderr,
        flush=True,
    )
    server.serve_forever()


if __name__ == "__main__":
    sys.exit(main())
