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


SKIP_REASONS = {
    2: "linked-list node allocation and pointer traversal are outside the pure-English user surface",
    5: "requires constructing and returning a substring; no pure-English string-slicing/assembly surface yet",
    6: "requires assembling zigzag rows into a new string; no pure-English string-assembly surface yet",
    12: "requires assembling Roman symbols into a new string; no pure-English string-assembly surface yet",
    14: "requires assembling a common-prefix string; no pure-English string-slicing/assembly surface yet",
    17: "recursive phone-letter enumeration needs helper calls, which the pure-English gate forbids",
    19: "linked-list node deletion and pointer rewiring are outside the pure-English user surface",
    22: "recursive parenthesis generation needs helper calls and a shared buffer",
    23: "linked-list k-way merge needs node allocation and pointer traversal",
    24: "linked-list pair rewiring needs pointer mutation",
    25: "linked-list k-group reversal needs pointer mutation",
    30: "concatenated-word matching needs string helpers or pointer-based slicing",
    37: "recursive Sudoku backtracking needs helper calls, which the pure-English gate forbids",
    38: "run-length string construction needs pure string assembly that is not implemented yet",
    39: "recursive combination enumeration needs helper calls",
    40: "recursive combination enumeration needs helper calls",
    43: "digit-array string construction needs pure string assembly that is not implemented yet",
    46: "recursive permutation enumeration needs helper calls",
    47: "recursive unique-permutation enumeration needs helper calls",
    49: "anagram grouping needs string helper comparison and typed-local workarounds",
}

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
            explicit = SKIP_REASONS.get(number)
            if explicit is not None:
                status = "skipped"
                reason = explicit
            elif old.get("status") == "solved":
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
