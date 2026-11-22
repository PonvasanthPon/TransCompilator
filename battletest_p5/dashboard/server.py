#!/usr/bin/env python3
"""Offline Phase 5 mission-control dashboard (Python stdlib only)."""
import json, pathlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs

ROOT = pathlib.Path(__file__).resolve().parent.parent
DASH = pathlib.Path(__file__).resolve().parent
HOST, PORT = "127.0.0.1", 8797

def read_json(path, fallback):
    try:
        return json.loads(path.read_text())
    except Exception:
        return fallback

def read_jsonl(path, limit=None):
    try:
        lines = [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
    except Exception:
        lines = []
    return lines[-limit:] if limit else lines

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass
    def send_json(self, value, code=200):
        body = json.dumps(value, ensure_ascii=False).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def do_GET(self):
        url = urlparse(self.path)
        query = parse_qs(url.query)
        if url.path in ("/", "/index.html"):
            body = (DASH / "index.html").read_bytes()
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        elif url.path == "/api/state":
            self.send_json(read_json(ROOT / "STATE.json", {"status": "MISSING"}))
        elif url.path == "/api/bugs":
            self.send_json(read_json(ROOT / "BUGS_P5.json", []))
        elif url.path == "/api/limits":
            self.send_json(read_json(ROOT / "LIMITS_P5.json", []))
        elif url.path == "/api/programs":
            limit = int(query.get("limit", ["50"])[0])
            self.send_json(read_jsonl(ROOT / "PROGRAMS_P5.jsonl", limit))
        elif url.path == "/api/fuzz":
            self.send_json(read_jsonl(ROOT / "FUZZ_P5_LOG.jsonl", 100))
        elif url.path == "/api/journal":
            tail = int(query.get("tail", ["40"])[0])
            try:
                lines = (ROOT / "JOURNAL.md").read_text().splitlines()
            except Exception:
                lines = []
            self.send_json(lines[-tail:])
        else:
            self.send_json({"error": "not found"}, 404)

if __name__ == "__main__":
    ThreadingHTTPServer((HOST, PORT), Handler).serve_forever()
