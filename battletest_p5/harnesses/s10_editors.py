#!/usr/bin/env python3
"""Tier S10: CLI, TUI, GUI, and readability checks."""
import json, pathlib, subprocess, tempfile, datetime

REPO = pathlib.Path("/home/ponvasanth/tc_r1")
TC = REPO / "target/debug/tc"
ROOT = REPO / "battletest_p5"
PROGRAMS = [
    "tier_s1/s1_060.eng", "tier_s2/s2_060.eng", "tier_s3/s3_050.eng",
    "tier_s4/s4_050.eng", "tier_s5/s5_060.eng", "tier_s6/s6_050.eng",
    "tier_s7/s7_050.eng", "tier_s8/app_060/main.eng",
    "tier_s9/s9_070.eng", "tier_s6/s6_002.eng", "tier_s7/s7_002.eng",
    "tier_s8/app_050/main.eng",
]

def run(cmd, cwd, timeout=60):
    return subprocess.run(cmd, cwd=str(cwd), stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout)

def main():
    entries = []
    with tempfile.TemporaryDirectory(prefix="p5s10_", dir="/tmp/opencode") as tmp:
        tmp = pathlib.Path(tmp)
        for relative in PROGRAMS:
            program = ROOT/"corpora"/relative
            name = f"{relative.replace('/','_')}"
            app = tmp/name
            app.mkdir()
            elf = app/"program.elf"
            for check, cmd in [
                ("build", [str(TC),"build",str(program),"-o",str(elf)]),
                ("interp", [str(TC),"interp","--cwd",str(app),str(program)]),
                ("trace", [str(TC),"trace",str(program)]),
                ("paraphrase", [str(TC),"paraphrase",str(program)]),
            ]:
                try:
                    result = run(cmd, REPO)
                    err = result.stderr.decode(errors="replace")
                    if check == "interp":
                        ok = result.returncode >= 0 and not err.startswith("tc interp:")
                    else:
                        ok = result.returncode == 0
                except Exception as exc:
                    ok = False; err = repr(exc)
                entries.append({"tier":"s10","name":name,"check":check,"ok":ok,"stderr":err[:400]})
    # Suites already exercise all 8 presets in TUI and GUI, plus the strict std gate.
    suites = [
        ("tc-editor","r1_editor_tests"),
        ("tc-editor-gui","native_gui_tests"),
        ("tc-semantics","phase4_7_pure_english_polish"),
    ]
    for package, test in suites:
        try:
            result = run(["cargo","test","-p",package,"--test",test], REPO, timeout=1200)
            ok = result.returncode == 0
            err = result.stderr.decode(errors="replace")[-800:]
        except Exception as exc:
            ok = False; err = repr(exc)
        entries.append({"tier":"s10","name":f"{package}::{test}","check":"cargo-test","ok":ok,"stderr":err})
    # Native GUI launch smoke; 124 is the expected clean timeout.
    try:
        result = subprocess.run(
            ["timeout","4",str(REPO/"target/debug/r1-editor-gui"),"std/alloc.eng"],
            cwd=str(REPO), stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=8,
        )
        ok = result.returncode == 124 and not result.stderr.strip()
        err = result.stderr.decode(errors="replace")[:400]
    except Exception as exc:
        ok = False; err = repr(exc)
    entries.append({"tier":"s10","name":"r1-editor-gui::launch","check":"smoke","ok":ok,"stderr":err})
    log = ROOT/"PROGRAMS_P5.jsonl"
    with log.open("a") as handle:
        for entry in entries:
            handle.write(json.dumps(entry, ensure_ascii=False)+"\n")
    failed = [entry for entry in entries if not entry["ok"]]
    state = json.loads((ROOT/"STATE.json").read_text())
    now = datetime.datetime.now().astimezone().isoformat(timespec="seconds")
    for tier in state["tiers"]:
        if tier["id"] == "S10":
            tier["tested"] = len(entries); tier["bugs"] = len(failed)
            tier["status"] = "DONE" if not failed else "IN_PROGRESS"
    state["updated_at"] = now; state["current_tier"]="S10"
    state["metrics"]["programs_tested"] = sum(t.get("tested",0) for t in state["tiers"] if t["id"] not in ("S11","S12"))
    (ROOT/"STATE.json").write_text(json.dumps(state,indent=2)+"\n")
    with (ROOT/"JOURNAL.md").open("a") as handle:
        handle.write(f"- {now} — Tier S10: {len(entries)-len(failed)}/{len(entries)} CLI/TUI/GUI checks passed.\n")
    print(json.dumps({"total":len(entries),"passed":len(entries)-len(failed),"failed":len(failed)},indent=2))
    return 0 if not failed else 1

if __name__ == "__main__":
    raise SystemExit(main())
