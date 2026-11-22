#!/usr/bin/env python3
"""Run one convergence round corpus and update state atomically."""
import argparse, concurrent.futures, datetime, json, pathlib, sys
HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from run_conv_probe import probe, probe_clean_error, probe_omitted
from update_state import read_json, mutate, journal, append_round

ROOT = HERE.parent
REPO = pathlib.Path("/home/ponvasanth/tc_r1")

def one(entry):
    path = REPO / entry["program"]
    stdin = entry.get("stdin", "").encode()
    if entry["expect"] == "clean_error":
        result = probe_clean_error(path)
    elif entry.get("prose"):
        result = probe_omitted(path, entry["prose"], stdin)
    else:
        result = probe(path, stdin)
    result.update({"round": entry["round"], "band": entry["band"],
                   "name": entry["name"], "expect": entry["expect"]})
    result["ok"] = bool(result.get("ok"))
    return result

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--round", type=int, default=1)
    parser.add_argument("--band", default="")
    parser.add_argument("--jobs", type=int, default=10)
    args = parser.parse_args()
    round_dir = ROOT / "rounds" / f"round_{args.round:02d}"
    manifest = json.loads((round_dir / "MANIFEST.json").read_text())
    if args.band:
        manifest = [m for m in manifest if m["band"].lower() == args.band.lower()]
    results = []
    with concurrent.futures.ProcessPoolExecutor(max_workers=args.jobs) as pool:
        for result in pool.map(one, manifest):
            results.append(result)
            mark = "PASS" if result["ok"] else "FAIL"
            print(f"{mark} {result['band']} {result['name']} {result.get('stage','')} {result.get('elapsed',0):.2f}s", flush=True)
    failures = [row for row in results if not row["ok"]]
    log = ROOT / "PROGRAMS_CONV.jsonl"
    now = datetime.datetime.now().astimezone().isoformat(timespec="seconds")
    with log.open("a") as handle:
        for row in results:
            compact = {k: row.get(k) for k in ("round","band","name","program","expect","ok","stage","elapsed","stdout","stderr","panic","missing","located","elf_exit","interp_exit","stdout_match","stderr_match","files_match")}
            compact["timestamp"] = now
            handle.write(json.dumps(compact, ensure_ascii=False) + "\n")
    if failures:
        faildir = ROOT / "rounds" / f"round_{args.round:02d}" / "failures"
        faildir.mkdir(exist_ok=True)
        for row in failures:
            name = pathlib.Path(row["program"]).name
            (faildir / f"{name}.result.json").write_text(json.dumps(row, indent=2) + "\n")
    counts = {}
    for row in results:
        band = row["band"]
        counts.setdefault(band, {"tested": 0, "bugs": 0})
        counts[band]["tested"] += 1
        if not row["ok"]:
            counts[band]["bugs"] += 1
    def change(state):
        for band in state["bands"]:
            if band["id"] in counts:
                band["tested"] = counts[band["id"]]["tested"]
                band["bugs"] = counts[band["id"]]["bugs"]
                band["status"] = "DONE" if counts[band["id"]]["tested"] >= band["target"] and counts[band["id"]]["bugs"] == 0 else "IN_PROGRESS"
        m = state["round_metrics"]
        m["programs_done"] = sum(b["tested"] for b in state["bands"])
        m["bugs_found_this_round"] = sum(b["bugs"] for b in state["bands"])
        state["current_round"] = args.round
    state = mutate(change)
    journal(f"Round {args.round} corpus: {len(results)-len(failures)}/{len(results)} passed; {len(failures)} failures logged.")
    print(json.dumps({"round": args.round, "total": len(results), "passed": len(results)-len(failures), "failed": len(failures)}, indent=2))
    return 0 if not failures else 1

if __name__ == "__main__":
    raise SystemExit(main())
