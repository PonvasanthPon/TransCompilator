#!/usr/bin/env python3
"""Atomic STATE.json updater and JOURNAL.md appender for the battle-test campaign."""

from __future__ import annotations

import argparse
import json
import os
import tempfile
from datetime import datetime, timezone, timedelta
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STATE = ROOT / "STATE.json"
JOURNAL = ROOT / "JOURNAL.md"
IST = timezone(timedelta(hours=5, minutes=30))


def now() -> str:
    return datetime.now(IST).isoformat(timespec="seconds")


def load() -> dict:
    with STATE.open(encoding="utf-8") as handle:
        return json.load(handle)


def save(state: dict) -> None:
    state["updated_at"] = now()
    fd, temporary = tempfile.mkstemp(prefix=".STATE.", dir=ROOT)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            json.dump(state, handle, indent=2, sort_keys=False)
            handle.write("\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, STATE)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def journal(message: str) -> None:
    with JOURNAL.open("a", encoding="utf-8") as handle:
        handle.write(f"- {now()} — {message}\n")


def update(mutator) -> dict:
    state = load()
    mutator(state)
    save(state)
    return state


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--status")
    parser.add_argument("--tier")
    parser.add_argument("--activity")
    parser.add_argument("--head")
    parser.add_argument("--metric", action="append", default=[])
    parser.add_argument("--tier-metric", action="append", default=[])
    parser.add_argument("--journal")
    args = parser.parse_args()

    def mutate(state: dict) -> None:
        if args.status:
            state["status"] = args.status
        if args.tier:
            state["current_tier"] = args.tier
        if args.activity:
            state["current_activity"] = args.activity
        if args.head:
            state["head_sha"] = args.head
        for item in args.metric:
            key, _, value = item.partition("=")
            state["metrics"][key] = json.loads(value)
        for item in args.tier_metric:
            tier, _, assignment = item.partition(":")
            key, _, value = assignment.partition("=")
            for entry in state["tiers"]:
                if entry["id"] == tier:
                    entry[key] = json.loads(value)
        if args.journal:
            journal(args.journal)

    update(mutate)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
