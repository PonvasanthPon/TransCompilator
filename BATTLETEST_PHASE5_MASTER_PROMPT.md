# tc_r1 Phase 5 — Full-Stack Battle-Test & Production Hardening (Canonical Prompt)

Base: `main` @ `27e57cc`, 650/650 tests green. Work branch: `battletest-p5` (main untouched).
Campaign state lives in `battletest_p5/STATE.json`, `JOURNAL.md`, `BUGS_P5.json`, `LIMITS_P5.json`,
`PROGRAMS_P5.jsonl`, `FUZZ_P5_LOG.jsonl`, and `CRASH_P5_LOG.jsonl`.

## Mission
Hammer the entire integrated stack: Core-7 compiler, all 8 pure-English `std/*.eng` modules,
Version 2 prose (auto-phrases, parenthetical types, multi-word identifiers, `Update`), ACE
anaphora + comma precedence + quantifiers, generics, static zero-libc ELF64, reference
interpreter, CLI, TUI and native GUI. Fix every genuine bug at root cause. Produce a production
readiness report.

## Non-negotiable guardrails
1. Keep Core-7 pure: no stdlib implementation moved into Rust. Fix stdlib bugs in `std/*.eng`.
2. No per-verb hardcoding or prose-noun blacklists; meaning flows from `tc-lexicon`.
3. Compiler crates + `tc-editor` stay Rust std-only. Only `tc-editor-gui` uses eframe/egui; no HTTP/HTML/browser.
4. Zero ML, zero network.
5. Every ELF64 must be static zero-libc: `ldd` not dynamic, no dynamic section, 0 INTERP.
6. Every executable program/fuzz seed must match the reference interpreter exactly on stdout, stderr, exit code, and created files.
7. Preserve the Version 2 readability gate on `std/*.eng`: no `Define local`, symbolic comparators, colon headers, 4-space indentation, C type tokens, bitwise/syscall tokens, dot fields, C-style call parentheses, `, returning nothing`, `Call this as` on internal helpers, or 3+ consecutive `Set <x>'s` lines.
8. No Rust panic, C segfault, or hang on any valid or malformed input.
9. `make build`, `make test`, `make lint`, `make fmt-check`, `make audit` all pass with 0 warnings.
10. Never weaken/delete existing tests (650 baseline).
11. Work only on `battletest-p5`; do not touch TransCompilator/, rbmt-sandbox/, Project FROZEN/.
12. Keep the live dashboard on 127.0.0.1:8797 updated; no stray listeners on 8780/18780/8790/18790/8792/8795/8796.

## Per-bug protocol
Minimize repro -> log BUGS_P5.json with tier/layer/class/repro/root cause -> fix root cause ->
add permanent regression test under `crates/*/tests/battletest_p5_regressions.rs` -> run all gates ->
atomic commit `battletest-p5(BUG-P5-NNN): ...` -> update STATE/JOURNAL/BUGS.

## Tiers and minimums (do not shrink)
- S1 Version 2 prose, auto-phrases, multi-word identifiers, `Update`: 55 programs.
- S2 ACE discourse, comma precedence, quantifiers with std/: 55.
- S3 std/alloc alignment, splitting, coalescing, realloc, zero leaks: 45.
- S4 std/io + std/math numeric, float, stdin extremes: 50.
- S5 std/string + generic std/list<T>: 55.
- S6 std/fs persistent handles, seek, multi-fd, on-disk parity: 45.
- S7 std/map collisions, overwrite, remove/reinsert, zero leaks: 45.
- S8 50 real-world multi-module applications across 10 archetypes: 50.
- S9 non-native/casual prose and rejection safety: 60.
- S10 TUI + native GUI + CLI stress: 40.
- S11 5,000 deterministic seeds:
  - 1..=2000 full-stack Core+std differential,
  - 2001..=3000 Version 2 paraphrase equivalence,
  - 3001..=4000 prose injection/expletive safety,
  - 4001..=5000 malformed/adversarial crash probes.
  Final 500 consecutive seeds must pass 500/500 after the last fix.
- S12 limits catalog, full gates, static ELF, GUI smoke, final report, dashboard COMPLETED.

## Completion checklist
- Branch `battletest-p5`, clean status, main at 27e57cc.
- >=550 hand-written programs logged in PROGRAMS_P5.jsonl.
- 5,000 seeds logged; 500/500 final block; 0 open failures.
- 0 panics/segfaults/hangs/leaks/silent drops; 0 open bugs.
- All gates green; all 8 std files pass readability gate; static zero-libc audit green.
- Dashboard at http://127.0.0.1:8797/ shows COMPLETED.
- `battletest_p5/PHASE5_BATTLETEST_FINAL_REPORT.md` written with metrics, bug catalog, verdict.
