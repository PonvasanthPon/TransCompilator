#!/usr/bin/env python3
"""Run one tier corpus through the parity probe and log results."""
import argparse, concurrent.futures, datetime, json, pathlib, sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from run_p5_probe import probe

ROOT = pathlib.Path(__file__).resolve().parent.parent
BIN = pathlib.Path("/home/ponvasanth/tc_r1")

def one(args):
    tier, program, stdin, timeout, static = args
    result = probe(program, stdin.encode(), timeout, False, static)
    result["tier"] = tier
    result["name"] = pathlib.Path(program).name
    return result

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("tier")
    parser.add_argument("--jobs", type=int, default=8)
    parser.add_argument("--timeout", type=float, default=20)
    parser.add_argument("--static-every", type=int, default=0)
    args = parser.parse_args()
    directory = ROOT / "corpora" / f"tier_{args.tier}"
    programs = sorted(directory.rglob("main.eng")) if args.tier.lower() == "s8" else sorted(directory.rglob("*.eng"))
    tasks = []
    for index, program in enumerate(programs):
        sidecar = program.with_suffix(".stdin")
        stdin = sidecar.read_text() if sidecar.exists() else ""
        static = args.static_every > 0 and index % args.static_every == 0
        tasks.append((args.tier, str(program), stdin, args.timeout, static))
    results = []
    with concurrent.futures.ProcessPoolExecutor(max_workers=args.jobs) as pool:
        for result in pool.map(one, tasks):
            results.append(result)
            status = "PASS" if result.get("ok") else "FAIL"
            print(f"{status} {result.get('name')} {result.get('stage','')} {result.get('elapsed',0):.2f}s", flush=True)
    failures = [row for row in results if not row.get("ok")]
    log = ROOT / "PROGRAMS_P5.jsonl"
    with log.open("a") as handle:
        for row in results:
            handle.write(json.dumps(row, ensure_ascii=False) + "\n")
    if failures:
        faildir = ROOT / "corpora" / "failures"
        faildir.mkdir(exist_ok=True)
        for row in failures:
            src = pathlib.Path(row["program"])
            target = faildir / f"{args.tier}_{src.name}"
            target.write_text(src.read_text())
            (faildir / f"{args.tier}_{src.name}.result.json").write_text(json.dumps(row, indent=2))
    state = json.loads((ROOT / "STATE.json").read_text())
    now = datetime.datetime.now().astimezone().isoformat(timespec="seconds")
    for tier in state.get("tiers", []):
        if tier["id"].upper() == args.tier.upper():
            tier["tested"] = len(results)
            tier["bugs"] = len(failures)
            tier["status"] = "DONE" if not failures and len(results) >= tier["target"] else "IN_PROGRESS"
    state["updated_at"] = now
    state["current_tier"] = args.tier.upper()
    state["metrics"]["programs_tested"] = sum(
        tier.get("tested", 0) for tier in state.get("tiers", []) if tier["id"].upper() != "S11"
    )
    (ROOT / "STATE.json").write_text(json.dumps(state, indent=2) + "\n")
    with (ROOT / "JOURNAL.md").open("a") as handle:
        handle.write(f"- {now} — Tier {args.tier.upper()} corpus: {len(results)-len(failures)}/{len(results)} passed.\n")
    print(json.dumps({"tier": args.tier, "total": len(results), "passed": len(results)-len(failures), "failed": len(failures)}, indent=2))
    return 0 if not failures else 1

if __name__ == "__main__":
    raise SystemExit(main())
