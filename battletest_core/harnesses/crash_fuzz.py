#!/usr/bin/env python3
"""Generate and execute 500+ malformed/adversarial crash-fuzz inputs."""

from __future__ import annotations

import argparse
import json
import os
import random
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REPO = ROOT.parent
TC = REPO / "target" / "debug" / "tc"
TIER = ROOT / "corpora" / "tier_c8"
LOG = ROOT / "CRASH_FUZZ.jsonl"
sys.path.insert(0, str(ROOT / "harnesses"))
import update_state  # noqa: E402

PANIC_MARKERS = (
    "panicked at",
    "overflowed its stack",
    "segmentation fault",
    "sigsegv",
    "sigabrt",
    "sigbus",
    "aborted",
    "core dumped",
)

CASES: list[tuple[str, bytes]] = []


def add(category: str, value: str | bytes) -> None:
    data = value.encode("utf-8") if isinstance(value, str) else value
    CASES.append((category, data))


def build_cases() -> None:
    for text in [
        "",
        " ",
        "\n",
        "\t\r\n",
        "\ufeff",
        "\ufeffReturn 0.\n",
        "Return 0.",
        "Return 0.\n\n\n",
        "\r\n\r\n",
        "\x00\x01\x02",
        "End.\n",
        "Phrase:\n",
        "Define\n",
        "Import\n",
        "If\n",
        "Loop:\n",
        "Return\n",
        "syscall(\n",
        "Return 0\nReturn 1\n",
        "x is\n",
    ]:
        add("empty", text)

    quote_inputs = [
        '"hello',
        'Return "hello.\n',
        'Return "\\".\n',
        'Write "unterminated\nsecond line\nthird line".\n',
        'Phrase: "emit <x>\n',
        'Return "a""b".\n',
        'Return "\\\\\\".\n',
        'x is "a. b".\n',
        '"""\n',
        '“hello”\n',
        'Return "line1\nline2\nline3".\n',
        'Return "\\x41".\n',
        'Return "\\x".\n',
        'Return "\\u{1F680}".\n',
        'Return "a: b; c. d".\n',
        'Write "a\n' * 20,
        "Return '\n",
        'Return "\n',
        'Phrase: "<x>"\n',
        'Phrase: "<>"\n',
        "Return 'unclosed.\n",
        'Return "\\0".\n',
        'Return "\\n\\r\\t".\n',
        'Return "\\\\".\n',
        'Return "\\"".\n',
        'Return "abc" + "def".\n',
        'Return bytes_ptr("").\n',
        'Return bytes_len("").\n',
        'Return bytes_ptr("x").\n',
        'Return bytes_len("x").\n',
        'Emit "x".\n',
        'Return "🔥".\n',
        'Return "👨‍👩‍👧‍👦".\n',
        'Return "a\u200bb".\n',
        'Return "a\u0301".\n',
        'Return "வணக்கம்".\n',
        'Return "नमस्ते".\n',
        'Return "a" + "b" + "c".\n',
        'Return "a".\n"b".\n',
    ]
    for text in quote_inputs:
        add("quotes", text)

    for depth in [1, 2, 5, 10, 20, 40, 60, 100, 150, 200]:
        add("parens", "Return " + "(" * depth + "1" + ")" * depth + ".\n")
    for depth in [1, 2, 5, 10, 20, 32, 33, 40, 50]:
        lines = []
        for level in range(depth):
            lines.append("    " * level + "If true:")
        lines.append("    " * depth + "Return 1.")
        add("indent", "\n".join(lines) + "\n")
    for depth in [1, 2, 5, 10, 20, 30, 40]:
        ty = "i64"
        for _ in range(depth):
            ty = f"array<{ty}, 2>"
        add("generic-depth", f"Define global x as {ty}.\nReturn 0.\n")
    for depth in [1, 2, 5, 10, 20, 30]:
        ty = "i64"
        for _ in range(depth):
            ty = f"Pair<{ty}, {ty}>"
        add(
            "generic-depth",
            f"Define struct Pair of T and U with first as T and second as U.\n"
            f"Define global x as {ty}.\nReturn 0.\n",
        )

    add("huge", "Add " + " and ".join(["1"] * 500) + " to x.\n")
    add("huge", "Return " + " + ".join(["1"] * 5000) + ".\n")
    add("huge", "x" * 5000 + ".\n")
    add("huge", "a " * 5000 + ".\n")
    add("huge", "1 " * 5000 + ".\n")
    add("huge", ("Return 1. " * 1000) + "\n")
    add("huge", ("If true:\n" * 100) + "Return 1.\n")
    add("huge", "Syscall 1 with " + ", ".join(["1"] * 1000) + ".\n")
    add("huge", "Define struct S with " + " and ".join([f"f{i} as i64" for i in range(500)]) + ".\n")
    add("huge", "Define function f taking " + " and ".join([f"a{i} as i64" for i in range(500)]) + " returning i64:\n    Return 0.\n")
    add("huge", "Copy 999999999 bytes from address of x to address of y.\n")
    add("huge", "Repeat 999999999 times:\n    Add 1 to x.\n")
    add("huge", "While true:\n    Add 1 to x.\n")
    add("huge", "Loop:\n    Add 1 to x.\n")
    add("huge", "x is " + "1 + " * 1000 + "1.\n")
    add("huge", "Return syscall(" + ", ".join(["1"] * 100) + ").\n")
    add("huge", "0" * 5000 + ".\n")
    add("huge", "9" * 5000 + ".\n")
    add("huge", "0x" + "f" * 5000 + ".\n")
    add("huge", "Return " + "not " * 200 + "true.\n")
    add("huge", "Return " + "-" * 200 + "1.\n")
    add("huge", "Return " + "+" * 200 + "1.\n")

    unicode_inputs = [
        "வணக்கம்.\n",
        "नमस्ते.\n",
        "こんにちは.\n",
        "🚀🔥.\n",
        "👨‍👩‍👧‍👦.\n",
        "a\u200bb.\n",
        "a\u0301.\n",
        "“hello”.\n",
        "‘hello’.\n",
        "Return 0. 🚀\n",
        "🚀 Return 0.\n",
        "x is 🚀.\n",
        "Return 🚀.\n",
        "Return \"🚀\".\n",
        "Return \"வணக்கம்\".\n",
        "Return \"नमस्ते\".\n",
        "Return \"こんにちは\".\n",
        "Return \"👨‍👩‍👧‍👦\".\n",
        "Return \"a\u200bb\".\n",
        "Return \"a\u0301\".\n",
        "Return “hello”.\n",
        "Define struct 名前 with x as i64.\n",
        "x is 名前.\n",
        "Define local x as 🔥.\n",
        "Return 0x🔥.\n",
        "Return ١٢٣.\n",
        "Return １２３.\n",
        "Return Ⅳ.\n",
        "Return ½.\n",
        "Return ∞.\n",
        "Return ٠.\n",
        "\u202eReturn 0.\n",
        "\u200bReturn 0.\n",
        "\u2060Return 0.\n",
        "\ufeff\ufeffReturn 0.\n",
        "Return \u0000.\n",
        "Return \u001b[31mred.\n",
        "Return \u007f.\n",
        "Return \u0080.\n",
        "Return \uffff.\n",
    ]
    for text in unicode_inputs:
        add("unicode", text)
    for index in range(10):
        add("unicode", "Return " + chr(0x1F600 + index) + ".\n")

    numeric = [
        "Return 99999999999999999999999999999999999999.\n",
        "Return -99999999999999999999999999999999999999.\n",
        "Return 1.2.3.\n",
        "Return 1e999999.\n",
        "Return -1e999999.\n",
        "Return 1e-999999.\n",
        "Return 0x.\n",
        "Return 0xGG.\n",
        "Return 0b2.\n",
        "Return 0o8.\n",
        "Return 0xFFFFFFFFFFFFFFFFFFFFFFFF.\n",
        "Return 18446744073709551616.\n",
        "Return 9223372036854775808.\n",
        "Return -9223372036854775808.\n",
        "Return 1_2_3.\n",
        "Return 1__2.\n",
        "Return 1_.\n",
        "Return .5.\n",
        "Return 5..\n",
        "Return 5.5.5.\n",
        "Return 1e.\n",
        "Return 1e+.\n",
        "Return 1e-.\n",
        "Return 0x1p2.\n",
        "Return 0x1.8p2.\n",
        "x +++ y.\n",
        "a === b.\n",
        "===\n",
        "!@#$%^&*()_+-=[]{}|;':\",./<>?.\n",
        "Return !@#$%^&*().\n",
        "Return (((((((((.\n",
        "Return ))))))))).\n",
        "Return []]][[.\n",
        "Return {}{}{}.\n",
        "Return <<<<>>>>.\n",
        "Return &&&&&&.\n",
        "Return ||||||.\n",
        "Return ======.\n",
        "Return !!!!!!.\n",
        "Return ~~~~~~.\n",
        "Return ``````.\n",
        "Return ???.\n",
        "Return ::::.\n",
        "Return ;;;;.\n",
        "Return ,,,,.\n",
        "Return ....\n",
        "Return \\\\\\.\n",
        "x += .\n",
        "x -= .\n",
        "x *= .\n",
        "x /= .\n",
        "x = = = 1.\n",
        "x is is 1.\n",
        "Let be 1.\n",
        "Define local as i64.\n",
        "Define local x as.\n",
        "Define struct with x as i64.\n",
        "Define function taking x as i64.\n",
        "Import .\n",
        "Import ..\n",
        "Import a b c.\n",
        "Phrase: \"\".\n",
        "Phrase: 42.\n",
        "If , Return 1.\n",
        "While , Return 1.\n",
        "Repeat times:\n",
        "Loop:\nElse:\n",
        "Else.\n",
        "Otherwise.\n",
        "Break.\n",
        "Continue.\n",
        "Trap.\n",
        "Return return return.\n",
        "Syscall with 1.\n",
        "Syscall 1 with .\n",
        "Copy bytes from to.\n",
        "Copy 1 bytes.\n",
        "size of.\n",
        "align of.\n",
        "address of.\n",
        "deref.\n",
        "Return *.\n",
        "Return &.\n",
        "Return function.\n",
        "Return function .\n",
        "Return null as.\n",
        "Return x as.\n",
        "Return x as 123.\n",
        "Return x as ptr<>.\n",
        "Return x as array<i64>.\n",
        "Return x as array<i64, 0, 2>.\n",
        "Return x as fn().\n",
        "Return x as fn(i64.\n",
        "Return x as fn(i64) ->.\n",
        "Return x as Pair<>.\n",
        "Return x as Pair<i64,>.\n",
        "Return x as Pair<,i64>.\n",
        "Return x as module..Type.\n",
        "Return x as .Type.\n",
        "Return x as module<Type.\n",
        "Return x as ptr<ptr<ptr<ptr<ptr<ptr<ptr<ptr<ptr<ptr<i64>>>>>>>>>>.\n",
        "Return x[.\n",
        "Return x[1.\n",
        "Return x[1][.\n",
        "Return x..field.\n",
        "Return x.field[.\n",
        "Return x's.\n",
        "Return 's field.\n",
        "Return field of.\n",
        "Return of x.\n",
        "Return sum of.\n",
        "Return product of.\n",
        "Return difference between.\n",
        "Return difference between 1.\n",
        "Return half of.\n",
        "Return square of.\n",
        "Return 10 percent of.\n",
        "Return 10 percent.\n",
        "Return 10 percent of of.\n",
        "Return between 1 and 2.\n",
        "Return x between 1 and.\n",
        "Return x is not.\n",
        "Return x at least.\n",
        "Return x at most.\n",
        "Return x more than.\n",
        "Return x less than.\n",
        "Return x equal to.\n",
        "Return x not equal to.\n",
    ]
    for text in numeric:
        add("numeric-operator", text)
    punctuation = "!@#$%^&*()_+-=[]{}|;':\",./<>?`~\\ \t\n"
    for index in range(100):
        rng = random.Random(9000 + index)
        length = rng.randint(1, 240)
        add("random", "".join(rng.choice(punctuation) for _ in range(length)))

    for index in range(40):
        add(
            "mixed-corruption",
            f"x is {index}.\nAdd 1 to x {['+++', '===', '(((', '\"', '🔥'][index % 5]}.\nReturn x.\n",
        )
    for index in range(30):
        add(
            "mixed-corruption",
            f"Define struct S{index} with x as i64 and y as {['i64', 'f64', 'ptr<i64>', 'array<i64, 2>'][index % 4]}.\n"
            f"s is S{index}.\nSet s.x to {index}.\nReturn s.{['x', 'y', 'missing'][index % 3]}.\n",
        )
    for index in range(30):
        add(
            "mixed-corruption",
            f"Import a.\nPublic function f{index} taking x as i64 returning i64:\n"
            f"    Phrase: \"emit <x>\"\n    Return {['x', 'x +', 'missing'][index % 3]}.\nReturn f{index}(1).\n",
        )
    for index in range(20):
        add(
            "mixed-corruption",
            f"Define global g{index} as array<u8, {index + 1}>.\n"
            f"Set g{index}[{index + 5}] to 1.\nReturn 0.\n",
        )


def spec_path(index: int) -> Path:
    return TIER / f"C8-{index:04d}"


def run_one(index: int, category: str, data: bytes) -> dict:
    directory = spec_path(index)
    directory.mkdir(parents=True, exist_ok=True)
    source = directory / "main.tc"
    source.write_bytes(data)
    elf = directory / "program.elf"
    record = {
        "id": f"C8-{index:04d}",
        "category": category,
        "bytes": len(data),
        "status": "PASS",
    }
    try:
        trace = subprocess.run(
            [str(TC), "trace", str(source)],
            cwd=directory,
            capture_output=True,
            timeout=6,
            env={key: value for key, value in os.environ.items() if key != "LD_LIBRARY_PATH"},
        )
        build = subprocess.run(
            [str(TC), "build", str(source), "-o", str(elf)],
            cwd=directory,
            capture_output=True,
            timeout=6,
            env={key: value for key, value in os.environ.items() if key != "LD_LIBRARY_PATH"},
        )
        record["trace_rc"] = trace.returncode
        record["build_rc"] = build.returncode
        combined = (
            trace.stdout
            + trace.stderr
            + build.stdout
            + build.stderr
        ).decode("utf-8", "replace")
        lowered = combined.lower()
        if trace.returncode < 0 or build.returncode < 0:
            record["status"] = "FAIL"
            record["error"] = "signal termination"
        for marker in PANIC_MARKERS:
            if marker in lowered:
                record["status"] = "FAIL"
                record["error"] = f"marker: {marker}"
                break
    except subprocess.TimeoutExpired:
        record["status"] = "FAIL"
        record["error"] = "hang/timeout"
    finally:
        try:
            elf.unlink()
        except OSError:
            pass
    return record


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--jobs", type=int, default=8)
    parser.add_argument("--limit", type=int)
    args = parser.parse_args()
    build_cases()
    if args.limit:
        del CASES[args.limit:]
    TIER.mkdir(parents=True, exist_ok=True)
    LOG.touch()
    print(f"crash-fuzzing {len(CASES)} inputs with {args.jobs} workers", flush=True)
    passed = 0
    failed = 0
    started = time.time()
    with LOG.open("a", encoding="utf-8") as log:
        with ThreadPoolExecutor(max_workers=args.jobs) as pool:
            futures = [
                pool.submit(run_one, index, category, data)
                for index, (category, data) in enumerate(CASES, 1)
            ]
            for index, future in enumerate(as_completed(futures), 1):
                record = future.result()
                log.write(json.dumps(record) + "\n")
                log.flush()
                if record["status"] == "PASS":
                    passed += 1
                else:
                    failed += 1
                    print(f"FAIL {record['id']} {record.get('error')}", flush=True)
                if index % 50 == 0 or index == len(CASES):
                    print(f"  progress {index}/{len(CASES)} pass={passed} fail={failed}", flush=True)
    print(f"crash fuzz complete in {time.time() - started:.1f}s pass={passed} fail={failed}")
    state = update_state.load()
    state["metrics"]["crash_fuzz_done"] = len(CASES)
    state["metrics"]["panics_or_segfaults"] = failed
    for entry in state["tiers"]:
        if entry["id"] == "C8":
            entry["tested"] = len(CASES)
            entry["status"] = "DONE" if failed == 0 else "RUNNING"
            entry["bugs"] = failed
    if failed == 0:
        state["metrics"]["tiers_completed"] = sum(
            1 for entry in state["tiers"] if entry["status"] == "DONE"
        )
    state["current_activity"] = f"Crash fuzz: {passed} pass / {failed} fail"
    update_state.save(state)
    update_state.journal(f"Crash fuzz {len(CASES)} inputs: {passed} PASS / {failed} FAIL.")
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
