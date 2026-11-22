#!/usr/bin/env python3
"""Atomic Phase 5 state updater and journal appender."""
import datetime, json, os, pathlib, sys, tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
STATE = ROOT / "STATE.json"
JOURNAL = ROOT / "JOURNAL.md"

def now():
    return datetime.datetime.now().astimezone().isoformat(timespec="seconds")

def load():
    return json.loads(STATE.read_text())

def save(state):
    state["updated_at"] = now()
    fd, tmp = tempfile.mkstemp(dir=ROOT, prefix=".STATE.", suffix=".tmp")
    with os.fdopen(fd, "w") as handle:
        json.dump(state, handle, indent=2)
        handle.write("\n")
    os.replace(tmp, STATE)

def journal(message):
    with JOURNAL.open("a") as handle:
        handle.write(f"- {now()} — {message}\n")

def main():
    if len(sys.argv) < 2:
        print("usage: update_state.py '<json patch>' [journal message]", file=sys.stderr)
        return 2
    patch = json.loads(sys.argv[1])
    state = load()
    for key, value in patch.items():
        if key == "metrics":
            state.setdefault("metrics", {}).update(value)
        elif key == "tiers":
            by_id = {tier["id"]: tier for tier in state.get("tiers", [])}
            for tier_patch in value:
                by_id.setdefault(tier_patch["id"], {"id": tier_patch["id"]}).update(tier_patch)
            state["tiers"] = [by_id[key] for key in sorted(by_id)]
        else:
            state[key] = value
    save(state)
    if len(sys.argv) > 2:
        journal(sys.argv[2])
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
