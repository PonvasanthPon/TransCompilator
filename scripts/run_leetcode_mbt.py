#!/usr/bin/env python3
"""Verify solved LeetCode MBT programs on IR, static ELF64, and WebAssembly.

Each solved problem must:
  * compile with the native frontend and produce its recorded stdout,
  * match under `tc interp` and `tc run` (static zero-libc ELF64),
  * lower its entry function to genuine Core-7 `loop` and `if` control flow,
  * match under the exact browser `tc_wasm.wasm` bridge, and
  * report zero `tc-lsp` diagnostics.

The full run is intentionally outside `make test`; `make mbt` invokes it.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BENCH = ROOT / "benchmarks" / "leetcode"
LEDGER = BENCH / "ledger.json"
TC = ROOT / "target" / "release" / "tc"
TC_LSP = ROOT / "target" / "release" / "tc-lsp"
WASM = ROOT / "web" / "site" / "tc_wasm.wasm"
WASM_EVAL = ROOT / "scripts" / "wasm_eval.mjs"



PURE_ENGLISH_PATTERNS = [
    (re.compile(r"\b[A-Za-z][A-Za-z0-9]*_[A-Za-z0-9_]+\b"), "snake_case identifier"),
    (re.compile(r"\b[A-Za-z][A-Za-z0-9_]*\("), "C-style function call"),
    (re.compile(r"\baddress\s+of\b", re.IGNORECASE), "raw address syntax"),
    (re.compile(r"\bas\s+an\s+unsigned\s+number\b", re.IGNORECASE), "unsigned cast"),
    (re.compile(r"\bas\s+a\s+whole\s+number\b", re.IGNORECASE), "whole-number cast"),
    (re.compile(r"\bas\s+a\s+pointer\b", re.IGNORECASE), "pointer cast"),
    (re.compile(r"\bas\s+a\s+byte\b", re.IGNORECASE), "byte cast"),
    (re.compile(r"(?im)^\s*(?:use|import)\s+"), "manual std import"),
    (re.compile(r"(?im)^\s*return\s+0\s*\.?\s*$"), "manual Return 0"),
    (re.compile(r"(?m)^[ \t]+\S"), "indented block"),
    (re.compile(r"(?m)^.*:\s*$"), "colon block header"),
    (re.compile(r"(?im)^\s*(?:end|end the loop|end if)\.?\s*$"), "explicit block terminator"),
    (re.compile(r"[+*/%]|<=|>=|==|!=|<|>|\s-\s"), "symbolic operator"),
    (re.compile(r"(?im)\b(?:let|set)\s+[a-z]\b"), "single-letter variable"),
    (re.compile(r"(?im)\bfor\s+(?:every|each)\s+[a-z]\b"), "single-letter loop variable"),
    (re.compile(r"(?im)\bitem\s+[a-z]\s+of\b"), "single-letter index"),
    (re.compile(r"(?im)\b(?:let|set)\s+found\s+(?:be|to)\s+(?:true|false)\b"), "boolean flag boilerplate"),
    (re.compile(r"(?im)\bis\s+(?:true|false)\b"), "boolean literal comparison"),
]


def strip_quoted(text: str) -> str:
    """Blank quoted literals while preserving byte offsets and line numbers."""
    output = []
    index = 0
    quote = None
    while index < len(text):
        character = text[index]
        if quote is not None:
            if character == "\\" and index + 1 < len(text):
                output.extend("  ")
                index += 2
                continue
            if character == quote:
                quote = None
            output.append("\n" if character == "\n" else " ")
            index += 1
            continue
        if character in {'"', "'"}:
            quote = character
            output.append(" ")
            index += 1
            continue
        output.append(character)
        index += 1
    return "".join(output)


def check_pure_english(path: Path, text: str) -> list[str]:
    """Hard-fail disguised C, pointer plumbing, casts, and boilerplate."""
    visible = strip_quoted(text)
    violations = []
    for pattern, label in PURE_ENGLISH_PATTERNS:
        for match in pattern.finditer(visible):
            line = visible.count("\n", 0, match.start()) + 1
            violations.append(f"{path.name}:{line}: {label} `{match.group(0).strip()}`")
    return violations


def run(command: list[str], timeout: int = 120) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(
        command,
        cwd=ROOT,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
    )


def entry_control_flow(trace: str) -> tuple[int, int]:
    """Count Core-7 loops/branches in the user module's functions.

    Helpers are part of the same lowered program, so a solution that moves
    its loop into a function still has to lower that function to real runtime
    control flow. Standard-library functions are excluded so their loops can
    never mask a constant-folded user program.
    """
    marker = "--- TC-IR ---"
    if marker not in trace:
        return 0, 0
    ir = trace.split(marker, 1)[1]
    starts = []
    offset = 0
    while True:
        found = ir.find('  fn #', offset)
        if found < 0:
            break
        starts.append(found)
        offset = found + 6
    if not starts:
        return 0, 0
    blocks = []
    for index, start in enumerate(starts):
        end = starts[index + 1] if index + 1 < len(starts) else len(ir)
        block = ir[start:end]
        header_end = block.find("\n")
        header = block[:header_end]
        quote = header.find('"')
        close = header.find('"', quote + 1)
        if quote < 0 or close < 0:
            continue
        name = header[quote + 1 : close]
        blocks.append((name, block))
    if not blocks:
        return 0, 0
    stem = blocks[0][0].split("__", 1)[0]
    loops = 0
    branches = 0
    for name, block in blocks:
        if not name.startswith(stem + "__"):
            continue
        loops += sum(1 for line in block.splitlines() if line.strip() == "loop {")
        branches += sum(
            1 for line in block.splitlines() if line.strip().startswith("if ")
        )
    return loops, branches


def lsp_check(paths: list[Path]) -> dict[str, list[str]]:
    """Open every document in one tc-lsp process and collect diagnostics."""
    if not TC_LSP.exists():
        return {str(path): ["tc-lsp binary missing"] for path in paths}

    messages: list[dict] = [
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"processId": None, "rootUri": ROOT.as_uri(), "capabilities": {}},
        },
        {"jsonrpc": "2.0", "method": "initialized", "params": {}},
    ]
    for path in paths:
        messages.append(
            {
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {
                    "textDocument": {
                        "uri": path.as_uri(),
                        "languageId": "tc-eng",
                        "version": 1,
                        "text": path.read_text(),
                    }
                },
            }
        )
    messages.append({"jsonrpc": "2.0", "id": 2, "method": "shutdown", "params": None})
    messages.append({"jsonrpc": "2.0", "method": "exit", "params": None})
    wire = b"".join(
        f"Content-Length: {len(body)}\r\n\r\n{body}".encode()
        for body in (
            json.dumps(message, separators=(",", ":")).encode() for message in messages
        )
    )

    process = subprocess.run(
        [str(TC_LSP), "--stdio"],
        cwd=ROOT,
        input=wire,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=300,
    )
    payloads = []
    data = process.stdout
    while data:
        header_end = data.find(b"\r\n\r\n")
        if header_end < 0:
            break
        header = data[:header_end].decode(errors="replace")
        length = None
        for line in header.splitlines():
            if line.lower().startswith("content-length:"):
                length = int(line.split(":", 1)[1].strip())
        if length is None:
            break
        body = data[header_end + 4 : header_end + 4 + length]
        data = data[header_end + 4 + length :]
        try:
            payloads.append(json.loads(body))
        except json.JSONDecodeError:
            continue

    found: dict[str, list[str]] = {str(path): [] for path in paths}
    for payload in payloads:
        if payload.get("method") != "textDocument/publishDiagnostics":
            continue
        params = payload.get("params", {})
        uri = params.get("uri", "")
        path = Path(uri.removeprefix("file://")) if uri.startswith("file://") else Path(uri)
        key = str(path)
        if key not in found:
            # Decode percent-escaped URIs conservatively.
            from urllib.parse import unquote, urlparse

            parsed = urlparse(uri)
            key = unquote(parsed.path)
        bucket = found.get(key)
        if bucket is None:
            bucket = found.setdefault(key, [])
        for diagnostic in params.get("diagnostics", []):
            bucket.append(
                f"{diagnostic.get('range', {}).get('start', {}).get('line', 0) + 1}: "
                f"{diagnostic.get('message', '')}"
            )
    return found


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ids", help="comma-separated problem numbers")
    parser.add_argument("--limit", type=int, default=0)
    parser.add_argument("--no-wasm", action="store_true")
    parser.add_argument("--no-lsp", action="store_true")
    parser.add_argument("--summary", action="store_true")
    args = parser.parse_args()

    ledger = json.loads(LEDGER.read_text())
    solved = [entry for entry in ledger["problems"] if entry["status"] == "solved"]
    if args.ids:
        wanted = {int(value) for value in args.ids.split(",") if value.strip()}
        solved = [entry for entry in solved if entry["id"] in wanted]
    if args.limit:
        solved = solved[: args.limit]
    if not solved:
        print("mbt: no solved problems to verify")
        return 0

    if not TC.exists():
        print("mbt: target/release/tc is missing; run `make release`", file=sys.stderr)
        return 1

    failures: dict[int, list[str]] = {}
    rows = []

    wasm_results: dict[int, dict] = {}
    if not args.no_wasm:
        if not WASM.exists():
            print("mbt: web/site/tc_wasm.wasm is missing; run `make web`", file=sys.stderr)
            return 1
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as jobs_file:
            json.dump(
                [{"id": entry["id"], "path": str(BENCH / entry["path"])} for entry in solved],
                jobs_file,
            )
            jobs_path = jobs_file.name
        out_path = jobs_path + ".out"
        node = subprocess.run(
            ["node", str(WASM_EVAL), str(WASM), jobs_path, out_path],
            cwd=ROOT,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=600,
        )
        if node.returncode != 0:
            print(f"mbt: wasm evaluator failed: {node.stderr.decode(errors='replace')}", file=sys.stderr)
            return 1
        for result in json.loads(Path(out_path).read_text()):
            wasm_results[result["id"]] = result
        Path(jobs_path).unlink(missing_ok=True)
        Path(out_path).unlink(missing_ok=True)

    lsp_results: dict[str, list[str]] = {}
    if not args.no_lsp:
        lsp_results = lsp_check([BENCH / entry["path"] for entry in solved])

    for entry in solved:
        number = entry["id"]
        source = BENCH / entry["path"]
        expected_path = BENCH / entry["expected"]
        issues: list[str] = []
        expected = expected_path.read_bytes() if expected_path.exists() else b""
        if not expected_path.exists():
            issues.append(f"missing expected output {entry['expected']}")

        pure_violations = check_pure_english(source, source.read_text())
        issues.extend(pure_violations)

        interpreted = run([str(TC), "interp", str(source)])
        if interpreted.returncode != 0:
            issues.append(
                "tc interp failed: " + interpreted.stderr.decode(errors="replace").strip()
            )
        if interpreted.stdout != expected:
            issues.append(
                f"interp stdout mismatch: {interpreted.stdout!r} != {expected!r}"
            )

        native = run([str(TC), "run", str(source)])
        if native.returncode != 0:
            issues.append(
                "tc run failed: " + native.stderr.decode(errors="replace").strip()
            )
        if native.stdout != expected:
            issues.append(f"ELF stdout mismatch: {native.stdout!r} != {expected!r}")

        trace = run([str(TC), "trace", str(source)])
        loops, branches = entry_control_flow(trace.stdout.decode(errors="replace"))
        # Recursive algorithms may legitimately lower to branches without a
        # loop; either primitive proves runtime control flow. A straight-line
        # constant-answer program has neither.
        if loops + branches < 1:
            issues.append(f"IR honesty: entry has loops={loops}, branches={branches}")

        steps = None
        if not args.no_wasm:
            result = wasm_results.get(number)
            if not result or "payload" not in result:
                issues.append(
                    "wasm evaluation failed: "
                    + (result or {}).get("error", "no result")
                )
            else:
                payload = result["payload"]
                steps = payload.get("steps")
                if not payload.get("ok"):
                    issues.append(
                        "wasm rejected program: "
                        + json.dumps(payload.get("diagnostics", []), ensure_ascii=False)
                    )
                if payload.get("stdout", "").encode() != expected:
                    issues.append(
                        f"wasm stdout mismatch: {payload.get('stdout', '')!r} != {expected!r}"
                    )

        if not args.no_lsp:
            diagnostics = lsp_results.get(str(source), [])
            if diagnostics:
                issues.append("tc-lsp diagnostics: " + "; ".join(diagnostics))

        if issues:
            failures[number] = issues
        rows.append((number, steps, loops, branches, not issues))
        entry["ir_steps"] = steps
        entry["ir_runtime_honesty"] = loops + branches >= 1
        entry["elf_parity"] = native.stdout == expected and native.returncode == 0
        entry["wasm_parity"] = (
            None
            if args.no_wasm
            else bool(wasm_results.get(number, {}).get("payload", {}).get("ok"))
            and wasm_results[number]["payload"].get("stdout", "").encode() == expected
        )

    if not args.no_lsp and not args.no_wasm:
        ledger["summary"]["solved"] = sum(
            entry["status"] == "solved" for entry in ledger["problems"]
        )
        LEDGER.write_text(json.dumps(ledger, indent=2, ensure_ascii=False) + "\n")

    if args.summary:
        for number, steps, loops, branches, ok in rows:
            print(
                f"{number:04d} {'PASS' if ok else 'FAIL'} "
                f"steps={steps if steps is not None else '-'} loops={loops} branches={branches}"
            )
    for number, issues in sorted(failures.items()):
        print(f"{number:04d} FAIL", file=sys.stderr)
        for issue in issues:
            print(f"  - {issue}", file=sys.stderr)
    passed = len(solved) - len(failures)
    print(f"mbt: {passed}/{len(solved)} solved problems passed")
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
