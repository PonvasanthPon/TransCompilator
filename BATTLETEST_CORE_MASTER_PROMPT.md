# `tc_r1` — Phase 3.5: Exhaustive Core Foundation Battle-Test & Hardening Campaign (Master Autonomous Prompt)

> **COMPACTION & SESSION-RESUME MANDATE (READ THIS FIRST ON EVERY TURN / SESSION)**
> This prompt lives at `/home/ponvasanth/tc_r1/BATTLETEST_CORE_MASTER_PROMPT.md`.
> Whenever your context compacts, a new session starts, or you resume after any interruption, you **MUST** immediately read:
> 1. This entire file (`/home/ponvasanth/tc_r1/BATTLETEST_CORE_MASTER_PROMPT.md`) from line 1 to EOF.
> 2. `/home/ponvasanth/tc_r1/battletest_core/STATE.json`.
> 3. The last 60 lines of `/home/ponvasanth/tc_r1/battletest_core/JOURNAL.md`.
> 4. `/home/ponvasanth/tc_r1/battletest_core/BUGS.json` and `/home/ponvasanth/tc_r1/battletest_core/CORE_LIMITS.json`.
>
> **NEVER** rely on hazy memory after compaction. **NEVER** declare completion early. **NEVER** stop until **all 10 Core Battle-Test Tiers (Tiers C1–C10)**, **>= 420 hand-written adversarial Core + English programs**, **all 4,000 automated stress/fuzz seeds** (`2,000` Core-7 IR/x86 differential + `500` English paraphrase-equivalence + `500` prose-injection + `500` generic/module AST + `500` malformed/crash-fuzz inputs, with the final 500 consecutive fuzz seeds passing `500/500`), **full 786-WordNet + 10,845-VerbNet live pipeline sweep**, **0 open bugs**, **0 panics**, **0 compiler/clippy warnings**, and **`make audit` PASS** are verified on disk.

---

## 1. Mission Statement: Why We Harden Core Before Writing `std/`

`tc_r1` has completed Phases 0–3 on `main` (`92dad36`, `256/256` tests passing):
- `tc-ir` — Core-7 typed IR + verifier + byte-accurate Reference IR Interpreter
- `tc-codegen-x86` — Direct x86_64 assembly + static zero-libc ELF64 (`as --64` + `ld -static`)
- `tc-typeck` — Typed AST, multi-file modules, type-directed overloads, compile-time generics `<T>`
- `tc-lexicon` — 786 WordNet hypernym entries + 10,845 VerbNet semantic-predicate frames + 103 irregular lemmas + 279 closed-class words
- `tc-syntax` — Safe Rust FFI over vendored CMU Link Grammar 5.13.2 + segmenter + `ClauseTree` walker
- `tc-semantics` — 3-way intent gate (`EXEC` | `COMMENT_OMITTED` | `UNSUPPORTED`) + declarative `Phrase:` matcher + unified `lower_stmt`
- `tc-cli` — `tc build`, `tc run`, `tc interp`, `tc trace`, `--version`, `--audit`, `--selftest`

**Before we write a single line of `std/` (`std/alloc.tc`, `std/list.tc`, `std/string.tc`, `std/fs.tc`, `std/map.tc`, `std/math.tc`) in English `tc` itself, the Core foundation MUST be battle-hardened until it is completely bulletproof.**

Any latent crack in:
- x86_64 width/sign/float/ABI/sret/syscall codegen,
- generic monomorphization (`Outer<Inner<T>>`, `List<Point>`, cross-module struct passing),
- Link Grammar C FFI memory safety / malformed-input crash resistance,
- WordNet/VerbNet live semantic lowering (silent drops or target clobbering),
- unified control-body lowering, or
- declarative `Phrase:` dispatch
will multiply 10x once `std/` is built on top of it.

**Your mission:** Attack every single crate and boundary of `tc_r1` relentlessly—writing adversarial programs, exhaustive lexicon sweeps, crash fuzzers, and 3,500 differential fuzz seeds—fixing every single bug at its root architectural cause, adding permanent regression tests, and proving `tc_r1`'s Core foundation is 100% ready for Phase 4 (`std/`).

You are authorized to run as long as needed (many hours or multiple sessions). **Do NOT rush. Do NOT shrink any tier or fuzz budget.**

---

## 2. Hard Constitutional Guardrails (Non-Negotiable During Every Fix!)

Every single fix, refactor, test, and commit MUST obey all 12 guardrails:

1. **Core-7 Purity (Zero Library Hacks in Rust)**:
   - Do **NOT** add hardcoded `List`, `String` heap ops, `Print` syscalls, `File` syscalls, `Map`, or `Sqrt` inside `crates/tc-*`. Those belong in Phase 4 `std/`.
2. **Zero Per-Verb Hardcoding in Rust**:
   - Do **NOT** write hand-coded verb synonym lists (`"deduct" | "spend" | "withdraw"`) in `tc-syntax`, `tc-semantics`, or `tc-lexicon/src/`. All open-class verb meaning MUST flow from the generated WordNet (786) & VerbNet (10,845) tables with real provenance IDs (`oewn-*` / VerbNet class IDs).
3. **Zero Prose Noun Blacklists**:
   - Do **NOT** list everyday nouns (`"coffee"`, `"cats"`, `"rain"`, `"petrol"`, `"bus"`, etc.) in Rust to detect `COMMENT_OMITTED`. Prose omission MUST remain strictly grammatical (tense/voice/structure + absence of computational predicate or `Phrase:` match).
4. **Single Unified Statement Lowerer**:
   - Top-level statements, `if`/`else`/`otherwise`/`unless`, `while`/`repeat`/`loop`, and function bodies MUST stay unified through one single `lower_stmt` path. Never fork a second control-body lowerer!
5. **Zero-ML & Zero-Network**:
   - Strictly deterministic, symbolic Rust only. Zero model files, embeddings, statistical weights, or network calls.
6. **Rust `std`-Only Offline**:
   - ZERO external crates.io dependencies in any `Cargo.toml` or `Cargo.lock`.
   - Every build and test MUST pass with `CARGO_NET_OFFLINE=true`.
7. **Zero Compiler & Clippy Warnings**:
   - `make build` -> 0 warnings
   - `make lint` (`cargo clippy --workspace --all-targets -- -D warnings`) -> 0 warnings
   - `make fmt-check` -> clean
   - `make audit` -> PASS
8. **Static Zero-Libc ELF64**:
   - Every compiled binary MUST pass `ldd` (`not a dynamic executable`), `readelf -d` (`There is no dynamic section`), and `0 INTERP` segments.
9. **Exact `ELF64 == Reference IR Interpreter` Parity**:
   - Every executable test and fuzz seed MUST assert exact byte-for-byte `stdout`, `stderr`, and `exit_code` parity between the compiled static ELF64 binary and `tc_ir::interp`.
10. **Zero Rust Panics & Zero C FFI Segfaults**:
    - No valid OR malformed input may EVER panic in Rust (`unwrap()`/`expect()`/index-out-of-bounds) or segfault/leak inside Link Grammar C FFI. Every invalid input MUST return a clean `Err(...)` diagnostic.
11. **Branch & Protected-Repo Hygiene**:
    - Work exclusively on a fresh branch **`battletest-core`** created from `main` (`92dad36`):
      ```bash
      cd /home/ponvasanth/tc_r1
      git checkout -b battletest-core main
      ```
    - Leave `main` untouched at `92dad36`.
    - Do **NOT** touch `/home/ponvasanth/TransCompilator/`, `/home/ponvasanth/rbmt-sandbox/`, or `/home/ponvasanth/Project FROZEN/`.
    - Every confirmed bug fix gets an atomic commit on `battletest-core` + a permanent regression test wired into `cargo test --workspace`.
12. **Live Dashboard Always Running**:
    - Serve the offline live mission-control dashboard at **`http://127.0.0.1:8796/`** (Python 3 stdlib only) and keep `STATE.json` updated atomically throughout the campaign. Never leave stray listeners on `8780`, `18780`, `8790`, `18790`, or `8795`.

---

## 3. Phase A — Bootstrap, Harness & Live Dashboard (`http://127.0.0.1:8796/`)

Execute Phase A cleanly before starting Tier C1:

### 3.1 Branch & Workspace Directory Layout
```text
/home/ponvasanth/tc_r1/battletest_core/
  STATE.json                    # Canonical machine-readable state (atomic tmp+rename)
  JOURNAL.md                    # Append-only chronological engineering log (timestamps + SHAs)
  BUGS.json                     # All Core battle-test bugs (id, tier, crate, class, status, repro, root_cause, fix_sha, regression)
  CORE_LIMITS.json              # Verified architectural limits (id, layer, repro, diagnostic, reason)
  CORE_LIMITS.md                # Human-readable Core limits catalog
  PROGRAMS.jsonl                # Append-only NDJSON log of every hand-written program tested
  FUZZ_LOG.jsonl                # Summary record per 50-seed fuzz batch + every failure/minimized repro
  LEXICON_SWEEP.json            # Full 786 WordNet + 10,845 VerbNet live-pipeline sweep results
  CRASH_FUZZ.jsonl              # All 500+ malformed/adversarial crash-fuzz probes
  corpora/
    tier_c1/ .. tier_c10/       # Hand-written adversarial programs & specs per tier
  harnesses/
    run_core_probe.py           # Runs tc build/run/interp/trace + parity + zero-libc + silent-drop checks
    sweep_lexicon_live.py       # Sweeps all 786 WordNet + 10,845 VerbNet entries through live tc binary
    crash_fuzz.py               # 500+ malformed/unicode/nesting/FFI crash-fuzz generator & runner
    update_state.py             # Atomic STATE.json updater + JOURNAL.md appender
  dashboard/
    server.py                   # Python 3 stdlib live dashboard on 127.0.0.1:8796
    index.html                  # Live auto-refreshing UI (polls /api/state every 2s)
  CORE_BATTLETEST_FINAL_REPORT.md
```

### 3.2 Baseline Verification
Before any edits on `battletest-core`, verify baseline:
- `CARGO_NET_OFFLINE=true make build && make test && make lint && make fmt-check && make audit` -> `256/256 PASS`, `0` warnings.
- `./target/debug/tc --audit` & `./target/debug/tc --selftest` -> PASS.

### 3.3 Live Mission-Control Dashboard (`http://127.0.0.1:8796/`)
Build and launch `battletest_core/dashboard/server.py` as a detached background process bound to `127.0.0.1:8796`.
The live UI (`index.html`) polls `/api/state`, `/api/programs?limit=50`, `/api/bugs`, `/api/limits`, `/api/fuzz`, and `/api/journal?tail=40` every 2 seconds and displays:
- **Status Banner**: `RUNNING` / `COMPLETED`, branch (`battletest-core`), HEAD SHA, active tier (`C1`–`C10`), live `current_activity` text, elapsed time, session count.
- **KPI Cards**:
  - Tiers Completed (`X / 10`)
  - Hand-Written Programs (`X / 420+`)
  - Core-7 IR/x86 Differential Fuzz (`X / 2,000`)
  - English Paraphrase Fuzz (`X / 500`)
  - Prose-Injection Fuzz (`X / 500`)
  - Generic/Module AST Fuzz (`X / 500`)
  - Malformed/FFI Crash Fuzz (`X / 500`, Panics/Segfaults: `0`)
  - Lexicon Live Sweep (`786` WordNet + `10,845` VerbNet, Silent Drops: `0`)
  - Bugs Found / Fixed / Open (`X / Y / Z`)
  - Permanent `cargo test` Count (`256 -> N PASS / 0 FAIL`)
- **Tier Matrix (C1–C10)**, **Live Probe Feed**, **Bug Tracker**, **Limits Catalog**, and **Live Journal Tail**.

Verify `curl -s http://127.0.0.1:8796/api/state` returns HTTP `200` and valid JSON before starting Tier C1.

---

## 4. Subagent Orchestration & Per-Bug Fix Protocol

Use parallel subagents effectively to maximize throughput without git collisions:

- **Main Agent (Compiler Fixer, Verifier & Release Owner)**:
  - Exclusively owns all edits to `crates/`, `Makefile`, `Cargo.toml`, `battletest_core/STATE.json`, `BUGS.json`, `CORE_LIMITS.json`, and all `git commit` calls.
  - Root-causes every bug, implements clean general Rust fixes (zero hacks), adds permanent regression tests in the appropriate crate's `tests/` directory, runs full offline gates, and commits atomically.
- **Parallel Subagents (Red-Teamers, Corpus Authors & Fuzz Workers)**:
  - **Subagent A (Tier Corpus Red-Teamer)**: Authors adversarial test programs for the active tier, executes them against `./target/debug/tc` (and/or Rust test harnesses), and returns minimized failing repros.
  - **Subagent B (Lexicon & Crash-Fuzz Auditor)**: Runs the 786-WordNet + 10,845-VerbNet live sweep and 500+ malformed/FFI crash probes, isolating any panic, segfault, silent drop, or target clobber.
  - **Subagent C (Differential Fuzz Shard Runner)**: Runs deterministic 250–500 seed shards of the Core-7 IR/x86, Paraphrase, Prose-Injection, and Generic-AST fuzzers, auto-minimizing every failing seed before handing it to the Main Agent.

### Strict Per-Bug Fix Loop
For EVERY bug caught in any tier, sweep, or fuzz seed:
1. **Minimize** to the smallest self-contained reproducer.
2. **Log** in `BUGS.json` (`BUG-CORE-NNN`, crate, tier, class: `CODEGEN_PARITY` | `ABI_BUG` | `TYPECK_BUG` | `MONO_BUG` | `LEXICON_BUG` | `SYNTAX_PANIC` | `FFI_SAFETY` | `SILENT_DROP` | `TARGET_CLOBBER` | `PHRASE_BUG` | `INTERP_BUG`).
3. **Root-Cause & Fix Cleanly** in Rust (obeying all 3 anti-hardcoding bans!).
4. **Add Permanent Regression Test** in `crates/<crate>/tests/battletest_core_regressions.rs` asserting exact `ELF64 == Reference IR Interpreter` parity (or clean `Err` for negative/crash tests).
5. **Run Full Offline Gate**: `CARGO_NET_OFFLINE=true make build && make test && make lint && make fmt-check && make audit`.
6. **Atomic Commit**: `git commit -m "battletest-core(BUG-CORE-NNN): <concise root-cause fix>"`, update `BUGS.json`, `STATE.json`, and `JOURNAL.md`.

---

## 5. The 10 Exhaustive Core Battle-Test Tiers (Tiers C1–C10)

Do **NOT** mark a tier `DONE` until its minimum program count is met, every bug found in it is fixed and regression-tested, and `make test` + `make lint` + `make audit` are 100% green.

---

### Tier C1 — Core-7 Scalar Widths, Signed/Unsigned Wraparound, Bitwise & IEEE-754 Torture (Min: 60 programs)

Hammer `tc-ir`, `tc-ir::interp`, and `tc-codegen-x86` on every nasty low-level CPU corner case:
1. **Extreme Integer Boundaries**:
   - `i8::MIN` (`-128`), `i8::MAX` (`127`), `i16::MIN/MAX`, `i32::MIN/MAX`, `i64::MIN` (`-9223372036854775808`), `i64::MAX` (`9223372036854775807`).
   - Two's-complement wraparound on `add`, `sub`, `mul`, `neg` (`-i64::MIN == i64::MIN`, `i8(127) + i8(1) == i8(-128)`).
   - `i64::MIN / -1` and `i64::MIN % -1` (must NOT crash with `SIGFPE` if wrapped, or must match identically between ELF64 and Interpreter!).
   - Negative dividend/divisor truncation-toward-zero semantics (`-7 / 2 == -3`, `-7 % 2 == -1`, `7 / -2 == -3`, `7 % -2 == 1`, `-7 / -2 == 3`, `-7 % -2 == -1`).
2. **Unsigned Widths (`u8`, `u16`, `u32`, `u64`)**:
   - `u64::MAX > 0`, `u64::MAX / 2`, `u64::MAX % 10`, `u8(255) + u8(1) == 0`, `u8(0) - u8(1) == 255`.
   - Unsigned `shr` (logical zero-fill) vs signed `sar` (arithmetic sign-fill) across all widths!
   - Shift counts `0`, `1`, `7`, `15`, `31`, `63` (verify both constant AND variable shift counts in `cl` register!).
3. **Narrow Load/Store & Sign/Zero Extension**:
   - Storing `i8(-5)`, `u8(250)`, `i16(-1000)`, `u16(60000)`, `i32(-100000)`, `u32(4000000000)` into memory (stack, struct fields, global `.data`, `mmap` heap), loading back, and casting across every width pair (`i8 -> i64`, `u8 -> i64`, `i64 -> i8`, `i64 -> u8`, `i32 -> u64`, `u32 -> i64`).
4. **IEEE-754 `f32` & `f64` Corner Cases**:
   - `-0.0 == 0.0` (must be `true`!), `-0.0` bit pattern (`0x8000000000000000` via union!), `NaN` comparisons (`NaN == NaN` is `false`, `NaN != NaN` is `true`, `NaN < 0.0` is `false`, `NaN > 0.0` is `false`, `NaN <= 0.0` is `false`, `NaN >= 0.0` is `false` — CRITICAL x86 `ucomisd` parity-flag `jp` bug check!!).
   - `u64 -> f64` with MSB set (`0x8000_0000_0000_0001_u64 as f64`, `u64::MAX as f64`!) and `f64 -> u64` for values `> i64::MAX`!
   - `f32 <-> f64` precision narrowing/widening and `f32`/`f64` negation (`-x`).
5. Assert exact **`ELF64 == Reference IR Interpreter`** parity on all 60+ tests!

---

### Tier C2 — Control Flow, Lexical Scoping, Short-Circuit & Deep Nesting Torture (Min: 50 programs)

1. **Deeply Nested Control Structures**:
   - 5–6 levels of nested `if`/`else`/`loop`/`break`/`continue`.
   - `break` and `continue` inside nested `if` inside nested `loop` (verifying `break` exits ONLY the innermost loop, never an outer loop!).
   - Early `return` from 4 levels deep inside nested loops and `if` branches (verifying clean stack/callee-saved epilogue restoration!).
   - Unreachable statements after `return`, `break`, `continue`, or `trap`.
2. **Short-Circuit Boolean Evaluation**:
   - `a && side_effect()`, `a || side_effect()` with nested combinations `(f1() && f2()) || (f3() && f4())` — verify exact evaluation order and exact call-count log!
   - Short-circuit pointer null guard: `p != null && *p == 42` (proving `*p` does NOT segfault when `p == null`!).
3. **Lexical Scope & Shadowing Torture**:
   - Shadowing the same variable name `x` across 5 nested blocks, changing its type from `i64` → `f64` → `bool` → `ptr<i64>` → `Point` and verifying every outer `x` is untouched after each inner block closes!
   - `where` clauses inside untaken `if` branches (verifying zero eager hoisting and zero side-effect evaluation when untaken!).

---

### Tier C3 — System V AMD64 ABI, Stack Spillover, SRET Aggregates & Indirect Calls (Min: 50 programs)

1. **Argument Count & Register Pressure Stress**:
   - Functions with `0, 1, 2, 6, 7, 8, 10, 12` integer/pointer args (testing stack argument alignment and positive `[rbp + 16 + ...]` offsets!).
   - Functions with `0, 1, 8, 9, 10, 12` `f64` and `f32` args (testing `xmm0..xmm7` + stack spillover!).
   - Interleaved mixed signatures: `(i8, f32, i64, f64, u16, f64, ptr<i64>, i32, f32, u64, f64, i64)` (12 mixed args!).
   - Deeply nested call expressions: `f(g(1, h(2, 3)), h(g(4, 5), f(6, 7)))` (proves nested calls never clobber live caller args or stack alignment!).
2. **Aggregate-by-Value & SRET ABI Stress**:
   - Structs of exact byte sizes **`1, 3, 7, 8, 9, 12, 16, 17, 24, 32, 48, 64, 128, 256`** passed by value AND returned by value from functions (including recursive functions and functions with 7+ other args!).
   - Verify exact byte equality of every field before and after pass/return across `ELF64 == Reference IR Interpreter`!
3. **Function Pointers & Indirect Calls**:
   - `FnPtr` stored in local variable, in a `struct` field, in a fixed array `[FnPtr; 4]` (dispatch table!), and in a static global!
   - Indirect call with 8 integer args + 4 float args + aggregate return!
4. **Recursion & Stack Depth**:
   - Deep recursion (`sum(2000)` = `2001000`), Ackermann `ack(3, 4)` = `125`, 3-way mutual recursion (`a -> b -> c -> a`).

---

### Tier C4 — Memory Layout, Odd-Aligned Structs, Unions, Arrays, `mmap` & Syscalls (Min: 60 programs)

1. **Alignment & Padding Torture**:
   - `struct Odd1 { a: u8, b: i64, c: u8 }` (`size=24, align=8`)
   - `struct Odd2 { a: u8, b: u16, c: u8, d: u32, e: u8 }` (`size=16, align=4`)
   - `struct Nested { tag: u8, inner: Odd1, tail: u16 }`
   - Fixed arrays `[Odd1; 16]` and `mmap`-backed pointer arrays `ptr<Odd1>` indexed with `IndexAddr` (verifying `index * 24` stride, zero misalignment or overlapping writes!).
   - Assert `sizeof` and `alignof` match System V AMD64 C17 rules identically in `tc-ir`, `tc-typeck`, `interp`, and `tc-codegen-x86`.
2. **Union Bit-Punning & Aliasing**:
   - `union { f: f64, u: u64, bytes: [u8; 8] }`, `union { f: f32, u: u32, bytes: [u8; 4] }`.
   - Write via one union field, read via another, pass `ptr<Union>` across functions.
3. **`MemCopy` & Buffer Stress**:
   - Copy structs/arrays of sizes `0, 1, 7, 8, 15, 16, 63, 64, 257, 1024` bytes via `MemCopy` (verifying zero off-by-one byte corruption around destination boundaries!).
4. **Raw Linux Syscall Stress (`0..6` args)**:
   - Multi-page `mmap(9)` + `munmap(11)` stress: allocate 8 separate pages (4 KB and 8 KB), write distinct checksums to every page, verify all 8 pages remain intact (no overlap!), unmap in alternating order, re-allocate, verify!
   - **Mini Zero-Libc Free-List Allocator Prototype Test** (written purely in Core-7 English!): allocate 32 blocks of varying sizes, free half, re-allocate into freed slots, verify zero corruption — proving Core-7 can cleanly host Phase 4's `std/alloc.tc`!
   - `read(0)` multi-chunk `stdin` + `write(1)` + `write(2)` (`stderr`) + `argc`/`argv` multi-argument forwarding (`["prog", "alpha", "beta", "gamma"]`).

---

### Tier C5 — Type Checker, Multi-File Modules, Generics & `Phrase:` Stress (Min: 60 programs)

Stress every feature of `tc-typeck` that `std/` will depend on:
1. **Multi-Level & Recursive-Safe Generics**:
   - `Pair<i64, Pair<f64, u8>>`, `Box<Box<Box<Point>>>`, `Triple<T, U, V>` instantiated 6 different ways in one program.
   - Generic function calling another generic function calling a third generic function (`f<T> -> g<Ptr<T>> -> h<Pair<T, i64>>`) with inferred AND explicit type args!
   - Generic struct containing a `FnPtr` field (`struct Comparer<T> { cmp: fn(ptr<T>, ptr<T>) -> i64 }`) — essential for generic `List<T>.sort` in Phase 4!
   - Generic `struct Result<T, E> { is_ok: bool, ok: T, err: E }` + `struct Option<T> { has_val: bool, val: T }` tested across `i64`, `f64`, and `Point`!
2. **Multi-File Module Graph Torture**:
   - 5-module DAG (`main -> a, b`, `a -> c, d`, `b -> c, d`, `c -> d`) with shared generic structs, static globals, and `pub` vs private functions.
   - Cross-module `Phrase:` declarations with **type-directed overloading**:
     - `module_a` declares `Phrase: "emit <x>"` for `x: i64`
     - `module_b` declares `Phrase: "emit <x>"` for `x: f64`
     - `module_c` declares `Phrase: "emit <x>"` for `x: ptr<u8>`
     - `main` imports all 3 and writes `Emit 42. Emit 3.5. Emit bytes_ptr("hi").` -> resolves uniquely by type!
3. **Negative Type/Module/Generic Error Torture (15+ tests)**:
   - Every invalid case (2/3/4-node import cycles, private cross-module struct/fn/global access, ambiguous overload, ambiguous `Phrase:`, wrong generic arity, infinite recursive generic `boom<Ptr<T>>`, infinite-size recursive struct `struct Bad { next: Bad }`, invalid `Phrase:` param, mismatched return/assign/call types) MUST return clean structured `Err(...)` with accurate `Span` and **0 panics**!

---

### Tier C6 — Exhaustive Live Pipeline Sweep of ALL 786 WordNet Entries & 10,845 VerbNet Frames (Automated Sweeper + 40+ Hand-Written Programs)

Build `battletest_core/harnesses/sweep_lexicon_live.py` (or a dedicated Rust integration tool) to sweep EVERY entry in `tc-lexicon` through the **actual live `tc` compiler (`tc trace` / `tc-syntax` → `tc-semantics`)**:

1. **All 786 WordNet Entries**:
   - Probe every `Verb` entry in both imperative (`<Verb> 10 to x.` / `<Verb> 10 from x.` / `<Verb> x.`) and narrative past-tense prose (`Yesterday Sam <verb_past> 2 apples.`).
   - Record counts: `exec`, `comment_ok`, `unsupported`, `silent_drop`, `panic`.
   - Assert **`silent_drop == 0`** and **`panic == 0`**!
2. **All 10,845 VerbNet Frames**:
   - Probe every `(verb, prep, predicate, role)` frame with an explicit target variable `x` (`Put 100 in x. <Verb> 10 <prep> x. Return x.`).
   - Assert **`panic == 0`**, **`silent_drop == 0`**, and **`target_clobber == 0`** (every `TransferAdd`/`TransferSub`/`TransferMove` frame that executes MUST mutate `x`, never silently dropping `x` or leaving `x == 100`!).
3. **40+ Hand-Written Multi-Verb Linguistic Programs**:
   - Mix 20+ distinct WordNet/VerbNet verbs in active voice, passive voice, `where` clauses, and nested control bodies; assert exact `ELF64 == Reference IR Interpreter` parity!

---

### Tier C7 — Real-Human Casual, Natural & Non-Native English Torture (Min: 100 programs)

Test `tc_r1`'s English frontend as ruthlessly as we tested `tc-prototype`—using **only Core-7 + declarative `Phrase:` helpers** (NO hardcoded library hacks!):

1. **Part A — 50+ Casual & Non-Native Executable Core-7 English Programs**:
   - Flowing paragraphs (no colons, no indentation, no coding symbols), numbered steps (`1. ... 2. ...`), missing periods, semicolon chains, indented colon blocks, `End.` closers.
   - Active AND passive voice (`Remove 180 from cash` == `180 is removed from cash`, `Add 50 to cash` == `50 is added to cash`, `Multiply price by 3` == `price is multiplied by 3`).
   - Two-variable transfers (`Transfer 200 from bank to wallet`, `Move 50 from a to b`).
   - Sentence-initial capitalization (`Salary is 5000. Remove 500 from salary.`), bare `i is 1`, discourse fillers (`First,`, `Next,`, `Then,`, `After that,`, `Finally,`, `Okay,`, `Please`), `percent of`, `between A and B`, dropped-`is` (`If cash more than 500`), `Unless`, `If ... otherwise ...`.
   - **Unified Body Parity Matrix**: test EVERY single top-level phrasing inside `If`, `Else`, `Unless`, `While`, `Repeat`, `Loop`, and `Function` bodies (nested 2–3 levels deep!).
   - All 50+ verified for exact **`ELF64 == Reference IR Interpreter`** parity!
2. **Part B — 50+ Everyday Prose & Non-Native Story Safety Programs**:
   - Inject diverse, realistic non-native stories with numbers, times, temperatures, prices, family members, and past-tense verbs (`"Today shop was very busy because of rain."`, `"My brother came in evening and drank 2 teas."`, `"Yesterday Ravi called me twice and said petrol is 105 rupees."`, `"I waited 20 minutes for bus."`, `"I had 2 cups of coffee."`, `"I am 25."`, `"He is 30 years old."`, `"She bought 3 kilos of rice for 180 rupees."`, `"I went to the store and made a grocery list."`, `"Let's check how much we spent today."`).
   - Assert **100% of prose sentences are `COMMENT_OMITTED`** and **0% alter program output**!
   - Also test 15+ out-of-scope computational directives (`"Start a new thread."`, `"Draw a window."`, `"Match with a regular expression."`, `"Connect to a TCP socket."`, `"Import module nonexistent."`) -> assert **100% loud `UNSUPPORTED` / structured error, 0 silent `COMMENT_OMITTED`**!

---

### Tier C8 — Link Grammar FFI & Adversarial Malformed-Input Crash Fuzzing (500+ Inputs)

Build `battletest_core/harnesses/crash_fuzz.py` to throw **500+ nasty, malformed, adversarial inputs** at `./target/debug/tc trace` and `./target/debug/tc build` (5-second timeout per input):
- Empty file, whitespace/tabs/CRLF-only, BOM (`\ufeff`).
- Unclosed quotes (`"hello`), escaped quotes (`"\"\\"`), quotes spanning 20 lines.
- Deeply nested parentheses (`(` × 200 + `1` + `)` × 200), deeply nested indented `If:` blocks (`50` levels deep — must hit the 32-level recursion guard cleanly, NEVER stack overflow!).
- Huge single sentences (500 words, 5,000 characters) fed into Link Grammar C FFI (verifying zero buffer overflows, zero C segfaults, clean timeout/cutoff!).
- Unicode & non-ASCII text (Tamil `"வணக்கம்"`, Hindi, emojis `"🚀🔥"`, zero-width joiners, smart curly quotes `“hello”`) both inside string literals AND outside!
- Overflowing integer literals (`99999999999999999999999999999999999999`), malformed floats (`1.2.3`, `1e999999`), broken operators (`x +++ y`, `a === b`, `===`), binary/punctuation soup (`!@#$%^&*()_+-=[]{}|;':",./<>?`).
- **Hard Requirement**: **0 Rust panics** (`panicked at`), **0 SIGSEGV/SIGABRT/SIGBUS**, **0 hangs** across all 500+ inputs!

---

### Tier C9 — 3,500-Seed Multi-Engine Differential Fuzzer

Build deterministic, reproducible fuzzers (Rust test binary or Python driver invoking `tc`) across **4 engines = 3,500 seeds**:

1. **Engine 1 — Seeds `1..=2000` (Core-7 IR + x86_64 Differential Fuzzer)**:
   - Generates random valid Core-7 `tc_ir::Module`s combining: mixed signed/unsigned widths (`i8..i64`, `u8..u64`), bitwise ops, `f32`/`f64`, nested `if`/`else` & bounded `loop`/`break`/`continue`, helper functions with 0..10 args, indirect `FnPtr` calls, structs/unions/arrays, `MemCopy`, and `mmap`/`munmap` + `write(1, ...)`.
   - Verifies `tc_ir::verify_module` + asserts **exact `ELF64 == Reference IR Interpreter` parity** on all 2,000 seeds!
2. **Engine 2 — Seeds `2001..=2500` (English Paraphrase-Equivalence Fuzzer)**:
   - Generates a canonical Core-7 English program `P_canon` and a randomly paraphrased equivalent `P_casual` (mixing active/passive, WordNet/VerbNet synonyms, fillers, sentence-initial capitalization, missing periods, indented blocks vs inline clauses).
   - Asserts **exact 4-way parity**: `P_canon[ELF64] == P_canon[Interp] == P_casual[ELF64] == P_casual[Interp]`!
3. **Engine 3 — Seeds `2501..=3000` (Prose-Injection & Silent-Drop Fuzzer)**:
   - Injects 3–8 randomly generated everyday non-native prose sentences at random positions into Core-7 English programs.
   - Asserts every injected prose sentence is `COMMENT_OMITTED`, every executable sentence stays `EXEC`, and final `ELF64 == Interp` output is byte-identical!
4. **Engine 4 — Seeds `3001..=3500` (Multi-Module & Generic AST Fuzzer)**:
   - Generates random 2–4 module `AstProgram`s with generic structs (`Pair<T, U>`, `Box<T>`), generic functions, struct-by-value pass/return, and type-directed overloads.
   - Asserts `tc_typeck::check_and_lower` + **exact `ELF64 == Reference IR Interpreter` parity**!
5. **Stopping Criterion**:
   - All **3,500 seeds** complete with **0 open failures**, and the **final 500 consecutive seeds** after the last fix pass **500/500**!

---

### Tier C10 — Core Readiness Audit, Limit Catalog & Final Report

1. **Honest Core Limit Catalog (`battletest_core/CORE_LIMITS.md` & `CORE_LIMITS.json`)**:
   - Document every genuine Core architectural boundary (e.g., open-ended world knowledge, cross-sentence anaphora, non-Core library features deferred to Phase 4 `std/`), with a verified reproducer proving each fails cleanly with a structured error (never panic, never silent drop!).
2. **Full Offline Verification Gate (ALL MUST PASS)**:
   - `CARGO_NET_OFFLINE=true make build` -> **0 warnings, 0 errors**
   - `CARGO_NET_OFFLINE=true make test` -> **100% PASS / 0 FAIL** (`256` baseline preserved + all new Phase 3.5 regression tests)
   - `CARGO_NET_OFFLINE=true make lint` (`clippy -- -D warnings`) -> **0 warnings**
   - `CARGO_NET_OFFLINE=true make fmt-check` -> **clean**
   - `CARGO_NET_OFFLINE=true make audit` -> **PASS**
   - `./target/debug/tc --audit` & `./target/debug/tc --selftest` -> **PASS**
   - **Anti-Hardcoding Audit**: grep `crates/tc-syntax/src/`, `crates/tc-semantics/src/`, `crates/tc-lexicon/src/` to confirm **0 per-verb synonym lists**, **0 prose noun blacklists**, **0 library hacks**, **0 hardcoded `/home/ponvasanth` paths**, **0 external crates**.
   - **Zero-Libc ELF64 Audit**: verify representative ELFs from Tiers C1–C9 report `not a dynamic executable` (`ldd`), `There is no dynamic section` (`readelf -d`), and `0 INTERP` segments.
   - **Repository Hygiene**: `git status` clean on `battletest-core`; `main` untouched at `92dad36`; `TransCompilator/`, `rbmt-sandbox/`, `Project FROZEN/` untouched.
   - **Dashboard Final State**: `STATE.json` updated to `"status": "COMPLETED"`, live dashboard running on `http://127.0.0.1:8796/` (HTTP 200), no stray listeners on `8780`, `18780`, `8790`, `18790`, or `8795`.
3. **Final Report (`/home/ponvasanth/tc_r1/battletest_core/CORE_BATTLETEST_FINAL_REPORT.md`)**:
   - Executive metrics table (Tiers C1–C10, hand-written programs, all 4,000 fuzz/crash seeds, 786 WordNet + 10,845 VerbNet live sweep results, bugs found/fixed/open, panics/segfaults = 0, regression suite expansion `256 -> N`, commits on `battletest-core`).
   - Complete bug catalog (`BUG-CORE-001..N`) with crate, root cause, commit SHA, and regression test path.
   - Clear **Phase 4 (`std/`) Go / No-Go Verdict** confirming every prerequisite for writing `std/alloc.tc`, `std/list.tc`, `std/string.tc`, `std/io.tc`, `std/fs.tc`, `std/math.tc`, `std/map.tc` in English `tc` is verified rock-solid!

---

## 6. Concrete `STATE.json` Schema (Required by Live Dashboard)

Write `battletest_core/STATE.json` atomically (`tmp` + `os.replace`):

```json
{
  "campaign": "tc_r1 Phase 3.5 — Core Foundation Battle-Test & Hardening",
  "status": "RUNNING",
  "branch": "battletest-core",
  "base_sha": "92dad36",
  "head_sha": "92dad36",
  "started_at": "2026-11-21T17:20:00+05:30",
  "updated_at": "2026-11-21T17:20:00+05:30",
  "sessions_count": 1,
  "current_tier": "C1",
  "current_activity": "Phase A: Bootstrapping battletest-core branch, harnesses, and live dashboard",
  "metrics": {
    "tiers_completed": 0,
    "tiers_total": 10,
    "programs_tested": 0,
    "programs_target": 420,
    "fuzz_ir_x86_done": 0,
    "fuzz_ir_x86_target": 2000,
    "fuzz_paraphrase_done": 0,
    "fuzz_paraphrase_target": 500,
    "fuzz_prose_done": 0,
    "fuzz_prose_target": 500,
    "fuzz_generics_done": 0,
    "fuzz_generics_target": 500,
    "crash_fuzz_done": 0,
    "crash_fuzz_target": 500,
    "panics_or_segfaults": 0,
    "wordnet_swept": 0,
    "verbnet_swept": 0,
    "silent_drops_caught": 0,
    "silent_drops_fixed": 0,
    "bugs_found": 0,
    "bugs_fixed": 0,
    "bugs_open": 0,
    "limits_cataloged": 0,
    "regression_tests": 256,
    "commits_on_branch": 0
  },
  "tiers": [
    {"id": "C1", "name": "Scalar Widths, Wraparound, Bitwise & IEEE-754", "status": "PENDING", "tested": 0, "target": 60, "bugs": 0},
    {"id": "C2", "name": "Control Flow, Scoping, Short-Circuit & Nesting", "status": "PENDING", "tested": 0, "target": 50, "bugs": 0},
    {"id": "C3", "name": "System V ABI, Spillover, SRET & Indirect Calls", "status": "PENDING", "tested": 0, "target": 50, "bugs": 0},
    {"id": "C4", "name": "Memory Layout, Odd Structs, Unions & mmap", "status": "PENDING", "tested": 0, "target": 60, "bugs": 0},
    {"id": "C5", "name": "Type Checker, Modules, Generics & Phrase Dispatch", "status": "PENDING", "tested": 0, "target": 60, "bugs": 0},
    {"id": "C6", "name": "Exhaustive 786-WordNet & 10,845-VerbNet Sweep", "status": "PENDING", "tested": 0, "target": 40, "bugs": 0},
    {"id": "C7", "name": "Casual, Natural & Non-Native English Torture", "status": "PENDING", "tested": 0, "target": 100, "bugs": 0},
    {"id": "C8", "name": "Link Grammar FFI & Malformed-Input Crash Fuzz", "status": "PENDING", "tested": 0, "target": 500, "bugs": 0},
    {"id": "C9", "name": "3,500-Seed Multi-Engine Differential Fuzzer", "status": "PENDING", "tested": 0, "target": 3500, "bugs": 0},
    {"id": "C10", "name": "Core Readiness Audit, Limits & Final Report", "status": "PENDING", "tested": 0, "target": 1, "bugs": 0}
  ]
}
```

---

## 7. Anti-Premature-Stop Checklist (Verify EVERY Box Before Reporting Done)

- [ ] Branch is `battletest-core`; `main` untouched at `92dad36`; `git status` clean.
- [ ] All 10 Tiers (**C1–C10**) marked `DONE` in `STATE.json`.
- [ ] `>= 420` hand-written adversarial Core + English programs executed and logged.
- [ ] All **786 WordNet entries** and **10,845 VerbNet frames** swept through live `tc` with **0 panics** and **0 silent drops**.
- [ ] All **500+ malformed/FFI crash-fuzz inputs** executed with **0 Rust panics**, **0 C segfaults**, and **0 hangs**.
- [ ] All **3,500 differential fuzz seeds** (`2,000` Core-7 IR/x86 + `500` Paraphrase + `500` Prose-Injection + `500` Generic/Module AST) completed with **0 open failures** and **500/500 PASS** on the final consecutive block.
- [ ] **0 open bugs** in `BUGS.json`; every fixed bug has an atomic commit SHA and permanent regression test in `cargo test --workspace`.
- [ ] `CARGO_NET_OFFLINE=true make build && make test && make lint && make fmt-check && make audit` ALL PASS with **0 warnings** and **0 external crates**.
- [ ] Zero per-verb hardcoding, zero prose noun blacklists, zero library hacks in Rust.
- [ ] Live dashboard running at `http://127.0.0.1:8796/` showing `COMPLETED`; no stray listeners on `8780`, `18780`, `8790`, `18790`, `8795`.
- [ ] `/home/ponvasanth/tc_r1/battletest_core/CORE_BATTLETEST_FINAL_REPORT.md` written with clear Phase 4 (`std/`) Go/No-Go readiness verdict.

**BEGIN NOW with Phase A: create `battletest-core` from `main`, verify baseline (`256/256`), launch the live dashboard on `http://127.0.0.1:8796/`, and dive straight into Tier C1.**
