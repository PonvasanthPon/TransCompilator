# tc_r1 Phase 5 — Full-Stack Battle-Test & Production Hardening Final Report

**Branch:** `battletest-p5` (from `main` @ `27e57cc`; `main` untouched)
**Campaign start:** 2026-11-22
**Status:** COMPLETED
**Campaign commit:** `e678e79`

## Executive metrics

| Metric | Result |
|---|---|
| Tiers S1–S12 | **12 / 12 DONE** |
| Hand-written `.eng` programs | **560** (S1–S9), plus **52** S10 editor/CLI checks |
| Full-stack differential fuzz | **2000 / 2000** |
| Version 2 paraphrase-equivalence fuzz | **1000 / 1000** |
| Prose-injection / expletive fuzz | **1000 / 1000** |
| Malformed / FFI crash fuzz | **1000 / 1000** |
| Final 500 consecutive seeds after last fix | **500 / 500** |
| Rust panics / C segfaults / hangs | **0** |
| Open memory leaks in `std/alloc` | **0** |
| Silent drops | **0** |
| Bugs found / fixed / open | **4 / 4 / 0** |
| Regression suite | **655 tests passing** (650 baseline preserved) |
| `make build` / `test` / `lint` / `fmt-check` / `audit` | **PASS / PASS / PASS / PASS / PASS** |
| Static zero-libc ELF64 | `ldd`: not a dynamic executable; no dynamic section; 0 INTERP |
| Native GUI smoke | clean launch, 0 stderr, clean timeout |
| Dashboard | `http://127.0.0.1:8797/` returned HTTP 200 and shows COMPLETED |

## Tier results

| Tier | Scope | Tested | Target | Bugs |
|---|---|---|---|---|
| S1 | Version 2 prose, auto-phrases, multi-word identifiers, `Update` | 65 | 55 | 0 |
| S2 | ACE discourse, comma precedence, quantifiers + `std/` | 65 | 55 | 0 |
| S3 | `std/alloc` alignment, coalescing, realloc, zero leaks | 55 | 45 | 0 |
| S4 | `std/io` + `std/math` numerics, floats, stdin | 60 | 50 | 0 |
| S5 | `std/string` + generic `std/list<T>` | 65 | 55 | 0 |
| S6 | `std/fs` persistent handles, seek, multi-file I/O | 55 | 45 | 0 |
| S7 | `std/map` collisions, growth, tombstones | 55 | 45 | 0 |
| S8 | Real-world multi-module applications | 65 | 50 | 0 |
| S9 | Non-native/casual prose and rejection safety | 75 | 60 | 0 |
| S10 | CLI + TUI + GUI editor checks | 52 | 40 | 0 |
| S11 | 5,000-seed fuzz/crash campaign | 5000 | 5000 | 0 |
| S12 | Production audit, limits, report | 1 | 1 | 0 |

## Bug catalog

| Bug | Layer | Root cause | Fix commit | Regression |
|---|---|---|---|---|
| BUG-P5-001 | CLI | `tc interp` always used `interpret` with no sandbox cwd, so `std/fs` file syscalls silently did nothing and CLI parity was impossible. | `71df9fd` | `crates/tc-cli/tests/battletest_p5_regressions.rs::interp_cwd_enables_sandboxed_file_syscalls` |
| BUG-P5-002 | SEMANTICS | `Set`/`Put` assignment verbs parsed values with `parse_expr`, rejecting auto-phrase values such as `last item of address of values`. | `b6b5d02` | `assignment_verbs_accept_phrase_expressions` |
| BUG-P5-003 | SEMANTICS | Indented control blocks inside paragraph functions were flattened to the function base indent; the `If`/`While` body was silently dropped. | `b6b5d02` | `paragraph_functions_preserve_indented_control_blocks` |
| BUG-P5-004 | STDLIB | `std/map.eng` used a fixed 256-slot table for `IntMap` and fixed arrays for `StrMap`; beyond 256 entries insertion probed forever or corrupted. | `b3298b1` | `int_map_grows_beyond_256_entries`, `text_map_grows_beyond_256_entries` |

## Fuzz campaign

All seeds are deterministic and reproducible from the seed number.

| Engine | Seeds | Result |
|---|---|---|
| 1 — Full-stack Core + `std/` differential | 1..=2000 | 2000/2000 |
| 2 — Version 2 paraphrase equivalence | 1..=1000 | 1000/1000 |
| 3 — Prose injection / expletive safety | 1..=1000 | 1000/1000 |
| 4 — Malformed / adversarial crash probes | 1..=1000 | 1000/1000 |

Every executable seed asserts exact ELF64 == reference-interpreter parity on stdout, stderr, exit code, and created files. Engine 3 additionally verifies every injected narrative sentence is `COMMENT_OMITTED` and that output matches the clean program byte-for-byte. Engine 4 asserts no Rust panic, signal, or timeout.

## Genuine remaining limits

See `LIMITS_P5.md` / `LIMITS_P5.json`. The catalog contains five clean-failure or documented-contract boundaries, including flat `For every` bodies, nested phrase arguments, very long fixed sentences, the editor parity runner's 10M-step budget, and StrMap's caller-owned key lifetime.

## Readiness verdict

**READY TO MERGE.** `battletest-p5` preserves the 650-test baseline, adds 5 permanent regressions, fixes four production bugs, passes all 12 tiers and all 5,000 fuzz seeds, keeps `std/*.eng` fully Version 2 English, and produces static zero-libc ELF64 binaries for the full eight-module standard library.

No writes were made to `TransCompilator/`, `rbmt-sandbox/`, or `Project FROZEN/`. `main` remains at `27e57cc`.
