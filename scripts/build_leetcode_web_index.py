#!/usr/bin/env python3
"""Build the compact, lazy-loaded playground problem index.

Only solved entries are published. Each record contains the problem number,
slug, title, difficulty, one primary category, and our original English source.
No LeetCode problem statement is copied.
"""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BENCH = ROOT / "benchmarks" / "leetcode"
LEDGER = BENCH / "ledger.json"
OUTPUT = ROOT / "web" / "site" / "leetcode_problems.json"


def main() -> None:
    ledger = json.loads(LEDGER.read_text())
    problems = []
    for entry in ledger["problems"]:
        if entry.get("status") != "solved":
            continue
        source_path = BENCH / entry["path"]
        expected_path = BENCH / entry["expected"]
        if not source_path.is_file() or not expected_path.is_file():
            raise SystemExit(f"solved entry {entry['id']} is missing files")
        steps = int(entry.get("ir_steps") or 0)
        # Keep the normal 2M default, but let verified heavier programs run
        # with a bounded budget derived from their measured IR steps.
        step_budget = max(2_000_000, min(50_000_000, steps + steps // 4 + 100_000))
        problems.append(
            {
                "id": entry["id"],
                "slug": entry["slug"],
                "title": entry["title"],
                "difficulty": entry["difficulty"],
                "category": entry["category"],
                "code": source_path.read_text(),
                "expected": expected_path.read_text(),
                "step_budget": step_budget,
            }
        )
    problems.sort(key=lambda item: item["id"])
    OUTPUT.write_text(json.dumps(problems, separators=(",", ":"), ensure_ascii=False) + "\n")
    print(f"wrote {OUTPUT.relative_to(ROOT)} with {len(problems)} solved problems ({OUTPUT.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
