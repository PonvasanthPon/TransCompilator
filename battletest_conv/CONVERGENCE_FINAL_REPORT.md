# tc_r1 — Zero-Bug Fixed-Point Convergence Final Report

**Campaign:** Simple Basic Casual English → Advanced Compound-Complex Sentences  
**Branch:** `battletest-convergence`  
**Main base:** `883432b03ddaf2d7d27bacb191cd467e3361e606` (untouched after Phase 0)  
**Status:** **CONVERGED** — clean Round 5  
**Last code-fix commit:** `feead65`
**Final campaign artifacts commit:** `640eead`  
**Date:** 2026-11-22

## Executive verdict

Five complete, disjoint convergence rounds were executed. Round 1 found three
real defects, Round 2 found one, Round 3 found two, and Round 4 found one. Each
was minimized, fixed at the root cause, covered by a permanent regression, and
committed atomically. **Round 5 executed its entire fresh budget and found zero
bugs**, so the fixed point is reached.

| Metric | Result |
|---|---|
| Complete rounds | **5** |
| Fresh authored `.eng` programs | **640** (128 × 5; 16 per band per round) |
| Fresh fuzz/crash seeds | **15,000** (3,000 × 5, strictly disjoint blocks) |
| Bugs found / fixed / open | **7 / 7 / 0** |
| Rust panics / C segfaults / hangs | **0** |
| Open memory leaks | **0** |
| Silent drops / false `EXEC` prose | **0** |
| Regression suite | **663 / 663 PASS** (655 baseline + 8 convergence regressions) |
| Limits cataloged | **7** |
| Clean final fuzz block | **3,000 / 3,000** |
| Final 500-seed crash block | **500 / 500** |
| `make build` / `test` / `lint` / `fmt-check` / `audit` | **PASS / PASS / PASS / PASS / PASS** |
| Static zero-libc ELF64 | `ldd`: not a dynamic executable; no dynamic section; 0 `INTERP` |
| Native GUI | clean launch, zero stderr |

## Round-by-round convergence table

| Round | Seed Range | Fresh Programs | Fuzz Seeds | Bugs Found | Bugs Fixed | Tests After | Verdict |
|---|---:|---:|---:|---:|---:|---:|---|
| 1 | `10001..13000` | 128 | 3,000 | 3 | 3 | 658 | DIRTY → Round 2 |
| 2 | `13001..16000` | 128 | 3,000 | 1 | 1 | 660 | DIRTY → Round 3 |
| 3 | `16001..19000` | 128 | 3,000 | 2 | 2 | 662 | DIRTY → Round 4 |
| 4 | `19001..22000` | 128 | 3,000 | 1 | 1 | 663 | DIRTY → Round 5 |
| 5 | `22001..25000` | 128 | 3,000 | **0** | **0** | 663 | **CLEAN → CONVERGED** |

Every round's fuzz allocation was exactly 1,000 differential + 750
paraphrase-equivalence + 750 prose-injection + 500 malformed/FFI-crash seeds.
No seed block overlaps another round.

## Complete bug catalog

| Bug | Round | Band / Engine | Layer | Root cause | Fix commit | Permanent regression |
|---|---:|---|---|---|---|---|
| BUG-CONV-001 | 1 | B5 | Semantics | The prose normalizer omitted `Put` from multi-action chain splitting, so `Put …, add … it` was parsed as one unsupported sentence and the new referent never entered the discourse stack. | `ad35f4b` | `compound_put_chains_share_the_new_referent` |
| BUG-CONV-002 | 1 | B3 | Stdlib | `Show`, `Print`, and `Display` resolved as Emit verbs but `std/io`/`std/string` exposed no matching overloads across the required scalar and text types. | `a61e492` | `output_synonyms_resolve_across_standard_types` |
| BUG-CONV-003 | 1 | B1 | Semantics | A direct `Say "literal"` produced no pointer/length lowering even though the canonical bytes phrase existed. | `019bf9a` | `say_accepts_a_direct_string_literal` |
| BUG-CONV-004 | 2 | B5 / B7 | Semantics | `Set` was missing from action-chain splitting. Adding it exposed that `starts_action` treated a leading parenthesized operand as a new action, which broke parenthesized `Set` expressions in `std/map`. | `5951466` | `compound_set_chains_share_the_new_referent`; `set_expressions_with_parenthesized_operands_stay_one_action` |
| BUG-CONV-005 | 3 | B5 | Semantics | The normalizer split `If`/`While`/`Until` headers at the first comma rather than using the comma-aware splitter, so `If A, and B, <string body>` lost its body. The guard also preserves clean errors for empty conditions. | `a35d01b` | `comma_before_and_keeps_the_condition_together_for_string_bodies` |
| BUG-CONV-006 | 3 | B5 | Semantics | Transfer-role discovery scanned prepositions inside parenthesized operands, so `Remove (cash divided by 4) from cash` selected the inner `by` instead of the outer `from`. | `a35d01b` | `parenthesized_division_is_one_transfer_operand` |
| BUG-CONV-007 | 4 | B8 / crash | Semantics | Expression parsing recursed through parenthesized groups without a depth guard; 150 groups exhausted the Rust stack in both the CLI and the smaller test-thread stack. | `feead65` | `deeply_nested_parentheses_fail_cleanly` |

The 655-test Phase 5 baseline was never weakened. The final suite is
`663 = 655 + 8` new convergence regressions.

## 8-Band English Variety coverage

Each round authored 16 new programs in every band, yielding 80 programs per band
and 640 total. Round 5 alone authored 128 new programs.

| Band | Scope | Programs across campaign |
|---|---|---:|
| B1 | Simple Basic & Non-Native Everyday English: tea stalls, fares, school marks, balances, bottle counts, direct literal output | 80 |
| B2 | Casual Conversational & Storytelling: expletive `It`, narrative omission, fillers, multiple output synonyms | 80 |
| B3 | Lexical Variety: active/passive arithmetic, VerbNet transfers, decreases/increases, `Say`/`Show`/`Print`/`Display` across i64/u64/f64/bool/TcString | 80 |
| B4 | ACE Discourse: long `it` chains, `its <field>`, grouped `Update`, multi-word identifiers, prefix-overlapping names | 80 |
| B5 | Compound Sentences: `Put`/`Set` action chains, parenthesized arithmetic operands, trailing `if`, comma-aware multi-term conditions | 80 |
| B6 | Complex & Compound-Complex: nested loops, negative steps, `Break`/`Continue`, local quantifiers, `Until`, scoped `where`, functions | 80 |
| B7 | Full-Stack `std/`: >256-entry maps, 380-key text maps, 200-item lists, 24 KB file round-trips, allocation churn, zero-leak assertions | 80 |
| B8 | Adversarial: unresolved anaphora, zero-step loops, recursive records, bad updates, Unicode, malformed syntax, OOB/empty list traps, deep nesting | 80 |

In Rounds 2–5, more than 30% of every fresh corpus deliberately re-tested the
subsystems where earlier rounds found bugs: action-chain splitting, output
synonyms, literal/string output, parenthesized expressions, comma-aware
conditions, and nesting limits.

## Fuzz and crash campaign

| Round | Engine 1 differential | Engine 2 paraphrase | Engine 3 prose injection | Engine 4 malformed/FFI | Total |
|---|---:|---:|---:|---:|---:|
| 1 | 1,000 | 750 | 750 | 500 | 3,000 / 3,000 |
| 2 | 1,000 | 750 | 750 | 500 | 3,000 / 3,000 |
| 3 | 1,000 | 750 | 750 | 500 | 3,000 / 3,000 |
| 4 | 1,000 | 750 | 750 | 500 | 3,000 / 3,000 |
| 5 | 1,000 | 750 | 750 | 500 | 3,000 / 3,000 |
| **Total** | **5,000** | **3,750** | **3,750** | **2,500** | **15,000 / 15,000** |

Every executable seed asserts exact `ELF64 == reference IR interpreter` parity
on stdout, stderr, exit code, and created file bytes. Engine 3 additionally
asserts each injected prose sentence is `COMMENT_OMITTED` and that output is
byte-identical to the uninjected program. Engine 4 asserts no panic, signal, or
hang.

## Genuine limits catalog

See `LIMITS_CONV.md` / `LIMITS_CONV.json`. Seven boundaries are documented with
reproducible, clean-failure probes:

1. Flat `For every` without an inline/indented body → located Lowering error.
2. A phrase expression nested directly as another phrase argument → clean field/Unsupported error; bind with `Let`.
3. Very long fixed sentences → deterministic prose/Unsupported verdict, never a Link Grammar crash.
4. The editor parity runner's 10,000,000-step interpreter budget.
5. `StrMap` keys are caller-owned and must outlive the map; maps grow past 256 entries.
6. A bare identifier that is also an open-class VerbNet verb can be read as a verb inside an arithmetic expression; it fails loudly. Use `Let` or an unambiguous name.
7. Parenthesized expression nesting is capped at 32 groups; 33+ returns a located depth error before stack exhaustion.

## Final system verification

| Gate | Result |
|---|---|
| `make build` | PASS — no errors |
| `make test` | **663 / 663 PASS** |
| `make lint` (`clippy -D warnings`) | PASS — 0 warnings |
| `make fmt-check` | clean |
| `make audit` | PASS |
| `tc --audit` | PASS |
| `tc --selftest` | PASS |
| `std_files_are_pure_version2_english` | PASS |
| All-8-`std` static ELF | `not a dynamic executable`; no dynamic section; 0 `INTERP`; exact parity |
| `r1-editor --check std/alloc.eng` | exact ELF64 == interpreter parity |
| TUI test suite | 34 / 34 PASS |
| GUI test suite | 46 / 46 PASS |
| Native GUI smoke | clean launch; zero stderr; clean timeout |
| Stray listeners | none on `8780`, `18780`, `8790`, `18790`, `8792`, `8795`, `8796`, `8797`; dashboard only on `8798` |

## Repository and dashboard state

- `main` remains at `883432b03ddaf2d7d27bacb191cd467e3361e606`, exactly the Phase 0 archive commit.
- `battletest-convergence` contains all code fixes, regressions, corpora, fuzz logs, harnesses, dashboard, limits, and reports. Its working tree is clean at finalization.
- `/home/ponvasanth/TransCompilator/`, `/home/ponvasanth/rbmt-sandbox/`, and `/home/ponvasanth/Project FROZEN/` were never read or written by this campaign.
- Live dashboard: `http://127.0.0.1:8798/` — HTTP 200, `CONVERGED`, Round 5, 640 programs, 15,000 seeds, 7 bugs fixed, 0 open.

## Readiness

**CONVERGED AND READY TO MERGE.** An entire unseen Round 5 completed its full
budget with zero bugs, zero panics, zero leaks, zero silent drops, exact
ELF64/interpreter parity, static zero-libc binaries, and all 663 regressions
green.
