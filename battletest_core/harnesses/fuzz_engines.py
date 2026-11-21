#!/usr/bin/env python3
"""Deterministic paraphrase, prose-injection, and generic/module fuzz engines."""

from __future__ import annotations

import argparse
import json
import os
import random
import shutil
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REPO = ROOT.parent
TC = REPO / "target" / "debug" / "tc"
LOG = ROOT / "FUZZ_LOG.jsonl"
WORK = Path("/tmp/opencode/bt_fuzz")
sys.path.insert(0, str(ROOT / "harnesses"))
import update_state  # noqa: E402


def run(command: list[str], cwd: Path, stdin: bytes = b"", timeout: int = 45):
    return subprocess.run(
        command,
        cwd=cwd,
        input=stdin,
        capture_output=True,
        timeout=timeout,
        env={key: value for key, value in os.environ.items() if key != "LD_LIBRARY_PATH"},
    )


def build_run_interp(directory: Path, expected_exit: int, expected_stdout: bytes = b""):
    entry = directory / "main.tc"
    elf = directory / "program.elf"
    build = run([str(TC), "build", str(entry), "-I", str(directory), "-o", str(elf)], directory)
    if build.returncode != 0:
        return None, f"build failed: {build.stderr.decode('utf-8', 'replace')[:400]}"
    try:
        elf_run = subprocess.run(
            [str(elf), "prog"],
            cwd=directory,
            input=b"",
            capture_output=True,
            timeout=30,
            env={},
        )
        interp = run([str(TC), "interp", str(entry), "-I", str(directory), "--"], directory)
    finally:
        try:
            elf.unlink()
        except OSError:
            pass
    if elf_run.returncode != interp.returncode:
        return None, f"ELF/interp exit mismatch {elf_run.returncode} != {interp.returncode}"
    if elf_run.stdout != interp.stdout:
        return None, "ELF/interp stdout mismatch"
    if elf_run.stderr != interp.stderr:
        return None, "ELF/interp stderr mismatch"
    if elf_run.returncode & 0xFF != expected_exit & 0xFF:
        return None, f"exit {elf_run.returncode} != {expected_exit}"
    if elf_run.stdout != expected_stdout:
        return None, f"stdout {elf_run.stdout!r} != {expected_stdout!r}"
    return elf_run, None


def write_program(directory: Path, files: dict[str, str]) -> None:
    if directory.exists():
        shutil.rmtree(directory)
    directory.mkdir(parents=True)
    for name, content in files.items():
        path = directory / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")


def paraphrase_pair(seed: int) -> tuple[dict[str, str], dict[str, str], int, bytes]:
    value = 20 + seed % 71
    amount = 1 + seed % 13
    family = seed % 10
    if family == 0:
        canonical = (
            f"cash is {value}.\nRemove {amount} from cash.\nAdd {amount + 2} to cash.\n"
            "Return cash.\n"
        )
        paraphrase = (
            f"First, cash is {value}.\nAfter that, {amount} is removed from cash.\n"
            f"Finally, cash is increased by {amount + 2}.\nReturn cash.\n"
        )
        expected = value + 2
    elif family == 1:
        canonical = f"x is {value}.\nAdd {amount} to x.\nReturn x.\n"
        paraphrase = f"x is {value}. {amount} is added to x. Return x.\n"
        expected = value + amount
    elif family == 2:
        canonical = f"x is {value}.\nRemove {amount} from x.\nReturn x.\n"
        paraphrase = f"Please, x is {value}.\n{amount} is removed from x.\nReturn x.\n"
        expected = value - amount
    elif family == 3:
        canonical = f"x is {value}.\nMultiply x by 3.\nReturn x.\n"
        paraphrase = f"x is {value}.\nx is multiplied by 3.\nReturn x.\n"
        expected = value * 3
    elif family == 4:
        canonical = (
            f"x is {value}.\nIf x more than {value // 2}:\n    Add {amount} to x.\n"
            f"Else:\n    Remove {amount} from x.\nReturn x.\n"
        )
        paraphrase = (
            f"x is {value}. If x more than {value // 2}, add {amount} to x "
            f"otherwise remove {amount} from x. Return x.\n"
        )
        expected = value + amount if value > value // 2 else value - amount
    elif family == 5:
        canonical = f"x is 0.\nRepeat 3 times:\n    Add {amount} to x.\nReturn x.\n"
        paraphrase = f"x is 0. Repeat 3 times, add {amount} to x. Return x.\n"
        expected = amount * 3
    elif family == 6:
        canonical = f"Return a + b where a is {value} and b is {amount}.\n"
        paraphrase = f"Return sum of a and b where a is {value} and b is {amount}.\n"
        expected = value + amount
    elif family == 7:
        canonical = f"a is {value}.\nb is {amount}.\nReturn a + b.\n"
        paraphrase = f"a is {value}.\nb is {amount}.\nReturn sum of a and b.\n"
        expected = value + amount
    elif family == 8:
        canonical = (
            f"1. x is {value}.\n2. Add {amount} to x.\n3. Return x.\n"
        )
        paraphrase = f"First, x is {value}.\nAfter that, add {amount} to x.\nFinally, return x.\n"
        expected = value + amount
    else:
        canonical = (
            f"a is {value}.\nb is {amount}.\nTransfer {amount} from a to b.\nReturn a + b.\n"
        )
        paraphrase = (
            f"a is {value}.\nb is {amount}.\nMove {amount} from a to b.\nReturn a + b.\n"
        )
        expected = value + amount
    return (
        {"main.tc": canonical},
        {"main.tc": paraphrase},
        expected & 0xFF,
        b"",
    )


def run_paraphrase(seed: int) -> dict:
    canonical, paraphrase, expected, stdout = paraphrase_pair(seed)
    directory = WORK / "paraphrase" / f"{seed:04d}"
    write_program(directory, canonical)
    _, error = build_run_interp(directory, expected, stdout)
    if error:
        return {"engine": "paraphrase", "seed": seed, "status": "FAIL", "error": error}
    write_program(directory, paraphrase)
    _, error = build_run_interp(directory, expected, stdout)
    result = {"engine": "paraphrase", "seed": seed, "status": "PASS"}
    if error:
        result["status"] = "FAIL"
        result["error"] = error
    shutil.rmtree(directory, ignore_errors=True)
    return result


PROSE = [
    "Yesterday {name} called me {n} times.",
    "I had {n} cups of coffee.",
    "My {relative} said petrol is {n} rupees.",
    "I waited {n} minutes for bus.",
    "Today the weather is nice.",
    "He is {age} years old.",
    "She bought {n} kilos of rice for {price} rupees.",
    "We went to the store and made a list.",
    "Let us check how much we spent today.",
    "The shop was very busy because of rain.",
    "I am {age}.",
    "My mother cooked rice.",
    "It was raining yesterday.",
]


def prose_sentence(rng: random.Random) -> str:
    template = rng.choice(PROSE)
    return template.format(
        name=rng.choice(["Ravi", "Meena", "Arun", "Kavya"]),
        relative=rng.choice(["uncle", "aunt", "brother", "sister"]),
        n=rng.randint(2, 199),
        age=rng.randint(18, 80),
        price=rng.randint(50, 900),
    )


def prose_case(seed: int) -> tuple[str, int, list[str]]:
    rng = random.Random(10_000 + seed)
    value = 10 + seed % 50
    amount = 1 + seed % 17
    family = seed % 4
    if family == 0:
        lines = [f"x is {value}.", f"Add {amount} to x.", "Return x."]
        expected = value + amount
    elif family == 1:
        lines = [f"cash is {value}.", f"Remove {amount} from cash.", "Return cash."]
        expected = value - amount
    elif family == 2:
        lines = [f"x is 0.", f"Repeat 2 times, add {amount} to x.", "Return x."]
        expected = amount * 2
    else:
        lines = [f"Return sum of a and b where a is {value} and b is {amount}."]
        expected = value + amount
    count = 3 + seed % 6
    prose = [prose_sentence(rng) for _ in range(count)]
    for sentence in prose:
        position = rng.randrange(0, len(lines) + 1)
        lines.insert(position, sentence)
    return "\n".join(lines) + "\n", expected & 0xFF, prose


def run_prose(seed: int) -> dict:
    text, expected, prose = prose_case(seed)
    directory = WORK / "prose" / f"{seed:04d}"
    write_program(directory, {"main.tc": text})
    trace = run([str(TC), "trace", str(directory / "main.tc")], directory)
    trace_text = trace.stdout.decode("utf-8", "replace") + trace.stderr.decode(
        "utf-8", "replace"
    )
    if "UNSUPPORTED" in trace_text:
        return {
            "engine": "prose",
            "seed": seed,
            "status": "FAIL",
            "error": "prose produced UNSUPPORTED",
        }
    if trace_text.count("COMMENT_OMITTED") < len(prose):
        return {
            "engine": "prose",
            "seed": seed,
            "status": "FAIL",
            "error": f"expected {len(prose)} COMMENT_OMITTED, found {trace_text.count('COMMENT_OMITTED')}",
        }
    for sentence in prose:
        if sentence.rstrip(".") not in trace_text:
            return {
                "engine": "prose",
                "seed": seed,
                "status": "FAIL",
                "error": f"missing prose sentence: {sentence}",
            }
    _, error = build_run_interp(directory, expected, b"")
    shutil.rmtree(directory, ignore_errors=True)
    result = {"engine": "prose", "seed": seed, "status": "PASS"}
    if error:
        result["status"] = "FAIL"
        result["error"] = error
    return result


def generic_case(seed: int) -> tuple[dict[str, str], int]:
    base = 3 + seed % 40
    extra = 1 + seed % 23
    modules = 2 + seed % 3
    lib = (
        "Public struct Pair of T and U with first as T and second as U.\n"
        "Public function pair_sum taking a as i64 and b as i64 returning i64:\n"
        "    p is Pair<i64, i64>.\n"
        "    Set p.first to a.\n"
        "    Set p.second to b.\n"
        "    Return p.first + p.second.\n"
    )
    files = {"lib.tc": lib}
    if modules == 2:
        files["main.tc"] = f"Import lib.\nReturn lib.pair_sum({base}, {extra}).\n"
        expected = base + extra
    elif modules == 3:
        files["mid.tc"] = (
            "Import lib.\n"
            "Public function twice taking a as i64 and b as i64 returning i64:\n"
            "    Return lib.pair_sum(a, b) * 2.\n"
        )
        files["main.tc"] = (
            f"Import mid.\nImport lib.\nReturn mid.twice({base}, {extra}).\n"
        )
        expected = (base + extra) * 2
    else:
        files["top.tc"] = (
            "Import mid.\n"
            "Public function plus_one taking a as i64 and b as i64 returning i64:\n"
            "    Return mid.twice(a, b) + 1.\n"
        )
        files["mid.tc"] = (
            "Import lib.\n"
            "Public function twice taking a as i64 and b as i64 returning i64:\n"
            "    Return lib.pair_sum(a, b) * 2.\n"
        )
        files["main.tc"] = (
            f"Import top.\nImport mid.\nImport lib.\nReturn top.plus_one({base}, {extra}).\n"
        )
        expected = (base + extra) * 2 + 1
    return files, expected & 0xFF


def run_generic(seed: int) -> dict:
    files, expected = generic_case(seed)
    directory = WORK / "generics" / f"{seed:04d}"
    write_program(directory, files)
    _, error = build_run_interp(directory, expected, b"")
    shutil.rmtree(directory, ignore_errors=True)
    result = {"engine": "generics", "seed": seed, "status": "PASS"}
    if error:
        result["status"] = "FAIL"
        result["error"] = error
    return result


ENGINES = {
    "paraphrase": (run_paraphrase, 2001, 2500),
    "prose": (run_prose, 2501, 3000),
    "generics": (run_generic, 3001, 3500),
}


def completed(engine: str) -> set[int]:
    done = set()
    if LOG.is_file():
        for line in LOG.read_text(encoding="utf-8", errors="replace").splitlines():
            try:
                record = json.loads(line)
            except json.JSONDecodeError:
                continue
            if record.get("engine") == engine and record.get("status") == "PASS":
                done.add(record.get("seed"))
    return done


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", choices=sorted(ENGINES), required=True)
    parser.add_argument("--start", type=int)
    parser.add_argument("--end", type=int)
    parser.add_argument("--jobs", type=int, default=6)
    args = parser.parse_args()
    runner, default_start, default_end = ENGINES[args.engine]
    start = args.start or default_start
    end = args.end or default_end
    done = completed(args.engine)
    seeds = [seed for seed in range(start, end + 1) if seed not in done]
    print(f"{args.engine}: fuzzing {len(seeds)} seeds ({start}..{end})", flush=True)
    passed = 0
    failed = 0
    started = time.time()
    LOG.touch()
    with LOG.open("a", encoding="utf-8") as log:
        with ThreadPoolExecutor(max_workers=args.jobs) as pool:
            futures = {pool.submit(runner, seed): seed for seed in seeds}
            for index, future in enumerate(as_completed(futures), 1):
                record = future.result()
                log.write(json.dumps(record) + "\n")
                log.flush()
                if record["status"] == "PASS":
                    passed += 1
                else:
                    failed += 1
                    print(f"FAIL {record}", flush=True)
                if index % 50 == 0 or index == len(seeds):
                    print(
                        f"  progress {index}/{len(seeds)} pass={passed} fail={failed}",
                        flush=True,
                    )
    print(
        f"{args.engine} complete in {time.time() - started:.1f}s pass={passed} fail={failed}"
    )
    state = update_state.load()
    metric = {
        "paraphrase": "fuzz_paraphrase_done",
        "prose": "fuzz_prose_done",
        "generics": "fuzz_generics_done",
    }[args.engine]
    state["metrics"][metric] = len(completed(args.engine))
    state["current_activity"] = f"{args.engine} fuzz: {passed} pass / {failed} fail"
    update_state.save(state)
    update_state.journal(
        f"{args.engine} fuzz batch {start}..{end}: {passed} PASS / {failed} FAIL."
    )
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
