#!/usr/bin/env python3
"""Run the hand-authored battle-test corpus through the live `tc` pipeline."""

from __future__ import annotations

import argparse
import base64
import json
import os
import re
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REPO = ROOT.parent
TC = REPO / "target" / "debug" / "tc"
SPECS = ROOT / "corpora" / "specs.jsonl"
PROGRAMS = ROOT / "PROGRAMS.jsonl"
FAILURES = ROOT / "FAILURES.jsonl"
sys.path.insert(0, str(ROOT / "harnesses"))
import update_state  # noqa: E402


def run(command: list[str], *, cwd: Path, stdin: bytes = b"", timeout: int = 60):
    return subprocess.run(
        command,
        cwd=cwd,
        input=stdin,
        capture_output=True,
        timeout=timeout,
        env={key: value for key, value in os.environ.items() if key != "LD_LIBRARY_PATH"},
    )


def panic_free(text: str) -> bool:
    lowered = text.lower()
    return "panicked at" not in lowered and "segmentation fault" not in lowered


def static_elf(path: Path) -> tuple[bool, str]:
    dynamic = subprocess.run(["readelf", "-d", str(path)], capture_output=True, text=True)
    if "NEEDED" in dynamic.stdout:
        return False, "readelf shows NEEDED entries"
    ldd = subprocess.run(["ldd", str(path)], capture_output=True, text=True)
    text = ldd.stdout + ldd.stderr
    if "not a dynamic executable" not in text and "statically linked" not in text:
        return False, f"ldd: {text.strip()}"
    return True, ""


def trace_comments(entry: Path, directory: Path) -> str:
    result = run([str(TC), "trace", str(entry), "-I", str(directory)], cwd=directory, timeout=60)
    return result.stdout.decode("utf-8", errors="replace") + result.stderr.decode(
        "utf-8", errors="replace"
    )


def check_spec(spec: dict) -> dict:
    directory = ROOT / spec["dir"]
    entry = directory / spec["entry"]
    elf = directory / "program.elf"
    command = [
        str(TC),
        "build",
        str(entry),
        "-I",
        str(directory),
        "-o",
        str(elf),
    ]
    started = time.time()
    result = run(command, cwd=directory, timeout=90)
    record = {
        "id": spec["id"],
        "tier": spec["tier"],
        "status": "FAIL",
        "duration_ms": 0,
    }
    try:
        combined = result.stdout.decode("utf-8", "replace") + result.stderr.decode(
            "utf-8", "replace"
        )
        if not panic_free(combined):
            record["error"] = "panic/segfault text in compiler output"
            return record
        if spec["kind"] == "error":
            if result.returncode == 0:
                record["error"] = "expected structured error, build succeeded"
                return record
            if result.returncode < 0 or "overflowed its stack" in combined.lower():
                record["error"] = "compiler crashed instead of returning a structured error"
                return record
            needle = spec.get("error_contains", "")
            if needle and needle not in combined:
                record["error"] = f"diagnostic does not contain {needle!r}: {combined[:300]}"
                return record
            record["status"] = "PASS"
            return record
        if result.returncode != 0:
            record["error"] = f"build failed rc={result.returncode}: {combined[:600]}"
            return record
        static_ok, static_reason = static_elf(elf)
        if not static_ok:
            record["error"] = static_reason
            return record
        args = spec.get("args", [])
        stdin = base64.b64decode(spec.get("stdin_b64", ""))
        elf_run = subprocess.run(
            [str(elf), *args],
            cwd=directory,
            input=stdin,
            capture_output=True,
            timeout=30,
            env={},
        )
        interp = run(
            [str(TC), "interp", str(entry), "-I", str(directory), "--", *args],
            cwd=directory,
            stdin=stdin,
            timeout=60,
        )
        expected_stdout = base64.b64decode(spec.get("stdout_b64", ""))
        checks = [
            (elf_run.returncode & 0xFF, spec["exit"], "ELF exit"),
            (interp.returncode & 0xFF, spec["exit"], "interpreter exit"),
            (elf_run.stdout, expected_stdout, "ELF stdout"),
            (interp.stdout, expected_stdout, "interpreter stdout"),
        ]
        for actual, expected, label in checks:
            if actual != expected:
                record["error"] = f"{label} mismatch: {actual!r} != {expected!r}"
                return record
        if elf_run.returncode != interp.returncode:
            record["error"] = (
                f"ELF/interpreter exit mismatch: {elf_run.returncode} != {interp.returncode}"
            )
            return record
        if elf_run.stdout != interp.stdout:
            record["error"] = "ELF/interpreter stdout mismatch"
            return record
        if elf_run.stderr != interp.stderr:
            record["error"] = "ELF/interpreter stderr mismatch"
            return record
        if not panic_free(interp.stderr.decode("utf-8", "replace")):
            record["error"] = "panic text in interpreter stderr"
            return record
        comments = spec.get("comments")
        if comments:
            trace = trace_comments(entry, directory)
            for comment in comments:
                if comment not in trace:
                    record["error"] = f"missing trace sentence {comment!r}"
                    return record
            omitted = trace.count("COMMENT_OMITTED")
            if omitted < len(comments):
                record["error"] = (
                    f"expected {len(comments)} COMMENT_OMITTED verdicts, found {omitted}"
                )
                return record
        record["status"] = "PASS"
        record["exit"] = elf_run.returncode
        return record
    except subprocess.TimeoutExpired:
        record["error"] = "timeout"
        return record
    finally:
        record["duration_ms"] = int((time.time() - started) * 1000)
        try:
            elf.unlink()
        except OSError:
            pass


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tier", action="append")
    parser.add_argument("--limit", type=int)
    parser.add_argument("--jobs", type=int, default=8)
    parser.add_argument("--no-state", action="store_true")
    args = parser.parse_args()

    specs = [json.loads(line) for line in SPECS.read_text(encoding="utf-8").splitlines()]
    if args.tier:
        wanted = {value.upper() for value in args.tier}
        specs = [spec for spec in specs if spec["tier"].upper() in wanted]
    if args.limit:
        specs = specs[: args.limit]
    if not specs:
        print("no specs selected")
        return 1

    print(f"running {len(specs)} programs with {args.jobs} workers", flush=True)
    passed = 0
    failed = 0
    started = time.time()
    with PROGRAMS.open("a", encoding="utf-8") as log, FAILURES.open("a", encoding="utf-8") as fail_log:
        with ThreadPoolExecutor(max_workers=args.jobs) as pool:
            futures = {pool.submit(check_spec, spec): spec for spec in specs}
            for index, future in enumerate(as_completed(futures), 1):
                spec = futures[future]
                record = future.result()
                log.write(
                    json.dumps(
                        {
                            "id": record["id"],
                            "tier": record["tier"],
                            "status": record["status"],
                            "duration_ms": record.get("duration_ms", 0),
                            "error": record.get("error"),
                        }
                    )
                    + "\n"
                )
                log.flush()
                if record["status"] == "PASS":
                    passed += 1
                else:
                    failed += 1
                    fail_log.write(json.dumps({**record, "spec": spec}) + "\n")
                    fail_log.flush()
                    print(f"FAIL {record['id']}: {record.get('error')}", flush=True)
                if index % 25 == 0 or index == len(specs):
                    print(
                        f"  progress {index}/{len(specs)} pass={passed} fail={failed}",
                        flush=True,
                    )
    elapsed = time.time() - started
    print(f"completed in {elapsed:.1f}s: pass={passed} fail={failed}", flush=True)
    if not args.no_state:
        tiers = sorted({spec["tier"] for spec in specs})
        state = update_state.load()
        state["metrics"]["programs_tested"] = state["metrics"].get("programs_tested", 0) + len(specs)
        state["metrics"]["regression_tests"] = 277
        for tier in tiers:
            tier_specs = [spec for spec in specs if spec["tier"] == tier]
            for entry in state["tiers"]:
                if entry["id"] == tier:
                    entry["tested"] = entry.get("tested", 0) + len(tier_specs)
                    entry["status"] = "DONE" if entry["tested"] >= entry["target"] else "RUNNING"
                    if entry["status"] == "DONE":
                        state["metrics"]["tiers_completed"] = sum(
                            1 for candidate in state["tiers"] if candidate["status"] == "DONE"
                        )
        state["current_activity"] = f"Corpus run complete for {', '.join(tiers)}"
        update_state.save(state)
        update_state.journal(
            f"Corpus probe: {passed} PASS / {failed} FAIL across {len(specs)} programs ({', '.join(tiers)})."
        )
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
