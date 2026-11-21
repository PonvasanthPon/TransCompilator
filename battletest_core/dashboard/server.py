#!/usr/bin/env python3
"""Offline live mission-control dashboard for the Phase 3.5 campaign."""

from __future__ import annotations

import json
import mimetypes
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

ROOT = Path(__file__).resolve().parents[1]
DASHBOARD = ROOT / "dashboard"


def tail_jsonl(path: Path, limit: int) -> list[dict]:
    if not path.is_file():
        return []
    lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    result = []
    for line in lines[-limit:]:
        try:
            result.append(json.loads(line))
        except json.JSONDecodeError:
            result.append({"raw": line})
    return result


def read_json(path: Path, default):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return default


class Handler(BaseHTTPRequestHandler):
    server_version = "tc-r1-battletest/1.0"

    def log_message(self, format, *args):  # noqa: A002
        return

    def send_json(self, value) -> None:
        payload = json.dumps(value, indent=2).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(payload)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(payload)

    def do_GET(self) -> None:  # noqa: N802
        parsed = urlparse(self.path)
        query = parse_qs(parsed.query)
        limit = int(query.get("limit", ["50"])[0])
        tail = int(query.get("tail", ["40"])[0])
        if parsed.path in ("/", "/index.html"):
            body = (DASHBOARD / "index.html").read_bytes()
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(body)
            return
        routes = {
            "/api/state": lambda: read_json(ROOT / "STATE.json", {}),
            "/api/programs": lambda: tail_jsonl(ROOT / "PROGRAMS.jsonl", limit),
            "/api/bugs": lambda: read_json(ROOT / "BUGS.json", []),
            "/api/limits": lambda: read_json(ROOT / "CORE_LIMITS.json", []),
            "/api/fuzz": lambda: tail_jsonl(ROOT / "FUZZ_LOG.jsonl", limit),
            "/api/lexicon": lambda: read_json(ROOT / "LEXICON_SWEEP.json", {}),
            "/api/crash": lambda: tail_jsonl(ROOT / "CRASH_FUZZ.jsonl", limit),
            "/api/journal": lambda: (ROOT / "JOURNAL.md").read_text(
                encoding="utf-8", errors="replace"
            ).splitlines()[-tail:],
        }
        route = routes.get(parsed.path)
        if route is None:
            self.send_error(404)
            return
        self.send_json(route())


def main() -> int:
    server = ThreadingHTTPServer(("127.0.0.1", 8796), Handler)
    print("battletest dashboard on http://127.0.0.1:8796/", flush=True)
    server.serve_forever()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
