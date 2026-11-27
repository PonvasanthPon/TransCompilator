#!/usr/bin/env python3
"""Generate the resumable LeetCode MBT ledger from catalog metadata.

Only factual metadata (number, slug, title, difficulty, tags, paid flag) is
stored. Problem statements are never copied into this repository.
"""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BENCH = ROOT / "benchmarks" / "leetcode"
CATALOG = BENCH / "catalog.json"
LEDGER = BENCH / "ledger.json"
PROBLEMS = BENCH / "problems"

PRIORITY = [
    "Array", "String", "Math", "Dynamic Programming", "Sorting", "Greedy",
    "Binary Search", "Depth-First Search", "Breadth-First Search", "Tree",
    "Graph Theory", "Stack", "Queue", "Heap (Priority Queue)", "Matrix",
    "Two Pointers", "Sliding Window", "Backtracking", "Linked List",
    "Bit Manipulation", "Simulation", "Counting", "Prefix Sum", "Design",
    "Database", "Shell", "Concurrency", "Interactive",
]


def classify(tags: list[str]) -> str:
    for name in PRIORITY:
        if name in tags:
            return name
    return tags[0] if tags else "Uncategorized"


def skip_reason(row: dict) -> str | None:
    tags = set(row["tags"])
    if row["paid_only"]:
        return "paywalled: LeetCode Premium problem; exact public spec unavailable"
    if "Database" in tags:
        return "SQL/database track is outside the Core-7 English execution surface"
    if "Shell" in tags:
        return "shell track is outside the Core-7 English execution surface"
    if "Concurrency" in tags:
        return "concurrency track requires threads, which are outside Core-7"
    if "Interactive" in tags:
        return "interactive-judge protocol is outside the standalone Core-7 execution surface"
    if "Design" in tags or row["title"].startswith("Design "):
        return "OOP/class-design problem: classes, inheritance, and virtual dispatch are forbidden"
    return None


def main() -> None:
    catalog = json.loads(CATALOG.read_text())
    existing = {}
    if LEDGER.exists():
        for entry in json.loads(LEDGER.read_text()).get("problems", []):
            existing[entry["id"]] = entry

    entries = []
    for row in catalog["problems"]:
        number = row["id"]
        category = classify(row["tags"])
        reason = skip_reason(row)
        sources = sorted(PROBLEMS.glob(f"{number:04d}_*.eng"))
        old = existing.get(number, {})
        if sources:
            source = sources[0]
            status = "solved"
            path = str(source.relative_to(BENCH))
            expected = f"expected/{source.stem}.out"
        else:
            path = None
            expected = None
            if old.get("status") == "solved":
                status = "pending"
                reason = None
            elif reason is not None:
                status = "skipped"
            elif old.get("status") == "skipped":
                status = "skipped"
                reason = old.get("skip_reason")
            else:
                status = "pending"
        entry = {
            "id": number,
            "slug": row["slug"],
            "title": row["title"],
            "difficulty": row["difficulty"],
            "category": category,
            "tags": row["tags"],
            "paid_only": row["paid_only"],
            "status": status,
            "path": path,
            "expected": expected,
            "skip_reason": reason if status == "skipped" else None,
            "ir_steps": old.get("ir_steps") if status == "solved" else None,
            "ir_runtime_honesty": old.get("ir_runtime_honesty") if status == "solved" else None,
            "elf_parity": old.get("elf_parity") if status == "solved" else None,
            "wasm_parity": old.get("wasm_parity") if status == "solved" else None,
            "verified_at": old.get("verified_at") if status == "solved" else None,
        }
        entries.append(entry)

    summary = {
        "catalog_total": len(entries),
        "solved": sum(e["status"] == "solved" for e in entries),
        "skipped": sum(e["status"] == "skipped" for e in entries),
        "pending": sum(e["status"] == "pending" for e in entries),
        "skip_breakdown": {},
    }
    for entry in entries:
        if entry["status"] == "skipped":
            key = entry["skip_reason"].split(":", 1)[0]
            summary["skip_breakdown"][key] = summary["skip_breakdown"].get(key, 0) + 1

    payload = {
        "project": "tc_r1",
        "version": "0.1.0-alpha-preview",
        "catalog_source": catalog.get("source"),
        "strategy": "Checkpointed 50-problem batches; metadata only, original English solutions.",
        "summary": summary,
        "problems": entries,
    }
    LEDGER.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
