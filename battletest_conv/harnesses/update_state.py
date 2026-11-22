#!/usr/bin/env python3
"""Atomic convergence state/history/journal updates."""
import datetime, json, os, pathlib, tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent

def now():
    return datetime.datetime.now().astimezone().isoformat(timespec="seconds")

def _atomic(path, text):
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(text)
    os.replace(tmp, path)

def read_json(path, fallback):
    try:
        return json.loads(path.read_text())
    except Exception:
        return fallback

def save_state(state):
    state["updated_at"] = now()
    _atomic(ROOT / "STATE.json", json.dumps(state, indent=2) + "\n")

def mutate(fn):
    state = read_json(ROOT / "STATE.json", {})
    fn(state)
    save_state(state)
    return state

def journal(message):
    stamp = now()
    with (ROOT / "JOURNAL.md").open("a") as handle:
        handle.write(f"- {stamp} — {message}\n")

def append_round(entry):
    path = ROOT / "ROUNDS_HISTORY.json"
    rounds = read_json(path, [])
    rounds = [row for row in rounds if row.get("round") != entry.get("round")]
    rounds.append(entry)
    _atomic(path, json.dumps(rounds, indent=2) + "\n")

if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--activity")
    parser.add_argument("--band")
    parser.add_argument("--status")
    parser.add_argument("--journal")
    args = parser.parse_args()
    if args.activity or args.band or args.status:
        def change(state):
            if args.activity: state["current_activity"] = args.activity
            if args.band: state["current_band"] = args.band
            if args.status: state["status"] = args.status
        mutate(change)
    if args.journal:
        journal(args.journal)
