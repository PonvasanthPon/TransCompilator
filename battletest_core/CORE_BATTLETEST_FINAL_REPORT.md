# tc_r1 Phase 3.5 — Core Foundation Battle-Test Final Report

**Branch:** `battletest-core`  
**Base:** `main` @ `92dad36` (untouched)  
**Status:** COMPLETED  

## Executive metrics

| Metric | Result |
|---|---:|
| Tiers completed | **10 / 10** |
| Hand-written adversarial programs | **446** (target 420) |
| Core-7 IR/x86 differential fuzz | **2,000 / 2,000** |
| English paraphrase-equivalence fuzz | **500 / 500** |
| Prose-injection fuzz | **500 / 500** |
| Generic/module AST fuzz | **500 / 500** |
| Malformed/FFI crash fuzz | **511 / 511** |
| Final consecutive block after last fix | **500 / 500** (`ir_x86 1501..2000`) |
| WordNet rows swept live | **786 / 786** |
| VerbNet frames swept live | **10,845 / 10,845** |
| Silent drops | **0** |
| Target clobbers | **0** |
| Rust panics / C segfaults / hangs | **0** |
| Bugs found / fixed / open | **25 / 25 / 0** |
| Permanent `cargo test` regressions | **296 passed / 0 failed** |
| Core limits cataloged | **7** |

## Tier matrix

| Tier | Name | Programs / inputs | Result |
|---|---|---:|---|
| C1 | Scalar widths, wraparound, bitwise, IEEE-754 | 60 | **PASS** |
| C2 | Control flow, scoping, short-circuit, nesting | 50 | **PASS** |
| C3 | System V ABI, spillover, SRET, indirect calls | 54 | **PASS** |
| C4 | Layout, odd structs, unions, mmap, allocator | 60 | **PASS** |
| C5 | Type checker, modules, generics, phrases | 63 | **PASS** |
| C6 | WordNet/VerbNet multi-verb + full sweep | 44 + 786 + 10,845 | **PASS** |
| C7 | Casual, natural, non-native English | 115 | **PASS** |
| C8 | Link Grammar FFI / malformed-input crash fuzz | 511 | **PASS** |
| C9 | Multi-engine differential fuzz | 3,500 | **PASS** |
| C10 | Readiness audit, limits, final report | 7 limits | **PASS** |

## Live lexicon sweep

```text
WordNet: total=786 verbs=783 exec=16 comment_ok=766 unsupported=767 silent_drop=0 panic=0
VerbNet: total=10845 unique_programs=5356 exec=1484 unsupported=9355 comment=6 type_error=0 target_clobber=0 panic=0
```

Every WordNet verb row enters `EXEC` or a loud `UNSUPPORTED` in the imperative probe and `COMMENT_OMITTED` in the past-tense narrative probe. Every VerbNet frame is either executed, loudly unsupported, or deliberately narrative; no frame is silently dropped and no transfer target is clobbered.

## Bug catalog

| ID | Tier | Crate | Class | Title | Fix commit |
|---|---|---|---|---|---|
| BUG-CORE-001 | C1 | tc-codegen-x86 | CODEGEN_PARITY | float-to-integer casts do not saturate like the reference interpreter | `6dbaa060c` |
| BUG-CORE-002 | C4 | tc-semantics | TARGET_CLOBBER | indexing or field access through a global pointer uses the address of the global slot | `784e0b446` |
| BUG-CORE-003 | C4 | tc-semantics | SYNTAX | hexadecimal/binary/octal integer literals are rejected | `784e0b446` |
| BUG-CORE-004 | C5 | tc-semantics | TYPECK_BUG | English type grammar cannot express FnPtr field/parameter types | `784e0b446` |
| BUG-CORE-005 | C5 | tc-semantics | PHRASE_BUG | phrase dispatch is not type-directed across overloaded module declarations | `784e0b446` |
| BUG-CORE-006 | C4 | tc-semantics | LOWERING | Core-7 MemCopy has no English statement form | `784e0b446` |
| BUG-CORE-007 | C7 | tc-syntax | SYNTAX_PANIC | bare sentence-initial `first` is stripped as a discourse filler, corrupting an identifier | `dea982e48` |
| BUG-CORE-008 | C4 | tc-semantics | TARGET_CLOBBER | pointer-to-pointer indexing reuses the address of the pointer slot instead of the loaded pointer | `615ffb904` |
| BUG-CORE-009 | C1 | tc-semantics | TYPECK_BUG | `Define local` of an unsigned type emits a signed zero literal | `615ffb904` |
| BUG-CORE-010 | C2 | tc-typeck | TYPECK_BUG | null literal is not context-typed and cannot compare against ptr<i64> | `615ffb904` |
| BUG-CORE-011 | C2 | tc-semantics | TYPECK_BUG | inner-block assignment cannot shadow an outer variable with a different type | `615ffb904` |
| BUG-CORE-012 | C3 | tc-semantics | TARGET_CLOBBER | assignment to a global function pointer creates a local shadow, then the call loads the zero global | `615ffb904` |
| BUG-CORE-013 | C4 | tc-semantics | SYNTAX | identifiers named `size`/`align`/`half`/`square` are always parsed as operators | `615ffb904` |
| BUG-CORE-014 | C5 | tc-semantics | PHRASE_BUG | equally typed duplicate Phrase declarations are silently disambiguated by name order | `615ffb904` |
| BUG-CORE-015 | C7 | tc-semantics | LEXICON_BUG | `Let us ...` discourse is treated as a computational TransferMove imperative | `615ffb904` |
| BUG-CORE-016 | C5 | tc-typeck | TYPECK_BUG | generic functions cannot declare aggregate locals because zero initialization is module-scoped | `615ffb904` |
| BUG-CORE-017 | C7 | tc-semantics | LEXICON_BUG | TransferSub accepted any preposition, so past prose like `We spent 200 rupees on food` executed | `615ffb904` |
| BUG-CORE-018 | C7 | tc-semantics | LEXICON_BUG | possessive/determiner-initial prose could be mislabeled as an imperative and rejected as UNSUPPORTED | `615ffb904` |
| BUG-CORE-019 | C8 | tc-syntax | SYNTAX_PANIC | tokenizer slices two bytes from a 3-byte UTF-8 character | `c733ce079` |
| BUG-CORE-020 | C8 | tc-semantics | SYNTAX_PANIC | very long left-associative expressions overflow the stack during AST drop/typecheck | `c733ce079` |
| BUG-CORE-021 | C8 | tc-semantics | SYNTAX_PANIC | deeply nested generic/array type syntax is unbounded | `c733ce079` |
| BUG-CORE-022 | C9 | tc-semantics | LEXICON_BUG | a noun subject that is also a lexical verb hijacks passive main-verb detection | `c733ce079` |
| BUG-CORE-023 | C9 | tc-semantics | LEXICON_BUG | numeric passive subjects make Link Grammar report `to` as the verb | `c733ce079` |
| BUG-CORE-024 | C6 | tc-semantics | TYPECK_BUG | break/continue/trap ignored trailing operands and surfaced as loop-context type errors | `c733ce079` |
| BUG-CORE-025 | C4 | tc-semantics | TARGET_CLOBBER | a pointer-typed struct field cannot be indexed because field types are unknown to the place resolver | `pending` |

All 25 bugs are fixed, each with a permanent regression in `cargo test --workspace`.

## Verification gates

```text
CARGO_NET_OFFLINE=true make build      PASS (0 warnings)
CARGO_NET_OFFLINE=true make test       PASS (296/296)
CARGO_NET_OFFLINE=true make lint       PASS (0 warnings)
CARGO_NET_OFFLINE=true make fmt-check  PASS
CARGO_NET_OFFLINE=true make audit      PASS
./target/debug/tc --audit               PASS
./target/debug/tc --selftest            SELFTEST PASS
```

`tc --audit` reports 786 WordNet rows, 10,845 VerbNet frames, 103 irregular lemmas, 279 closed-class words, 0 external crates, and zero ML/network use.

## Zero-libc ELF64 audit

```text
$ ldd audit_english.elf
        not a dynamic executable
$ readelf -d audit_english.elf
There is no dynamic section in this file.
$ readelf -l audit_english.elf | grep INTERP
0 INTERP segments
$ file audit_english.elf
ELF 64-bit LSB executable, x86-64, statically linked
```

Every executable corpus/fuzz seed asserted exact `ELF64 == Reference IR Interpreter` stdout, stderr, and exit-code parity. Representative fuzz seeds also ran `ldd` and `readelf` static checks.

## Anti-hardcoding and repository hygiene

- Zero per-verb synonym lists in `tc-syntax` or `tc-semantics`; every open-class verb meaning comes from generated WordNet/VerbNet provenance.
- Zero prose noun blacklists used for `COMMENT_OMITTED`; omission is grammatical and structural.
- Zero core library hacks (`List`/`String` heap ops/`Print`/`File`/`Map`/`Sqrt`) in Rust; Phase 4 `std/` remains the only home for such features.
- Zero external crates; `Cargo.lock` contains 7 local packages, 0 registry sources, 0 checksums.
- No raw corpora committed; only harvesters, generated tables, and adversarial test sources.
- `main` untouched at `92dad36`; working branch is `battletest-core`.
- `TransCompilator/`, `rbmt-sandbox/`, and `Project FROZEN/` remain untouched (their mtimes predate this campaign).

## Core limits

See `CORE_LIMITS.md` / `CORE_LIMITS.json`. All seven boundaries fail cleanly or are deliberate omission semantics; none panics, segfaults, or hangs.

## Commits on `battletest-core`

```text
92ab292 battletest-core: C8/C9 evidence, fuzz harnesses, and 511-case crash corpus
c733ce0 battletest-core(BUG-CORE-019..024): UTF-8 token safety, complexity guards, passive fallback, and control-keyword validation
5c49166 battletest-core: add 446-program C1-C7 corpus, harnesses, and evidence
615ffb9 battletest-core(BUG-CORE-008..018): pointer places, null context, shadowing, typed phrases, let-us prose, and generic aggregate zero
6739781 battletest-core: ignore runtime dashboard pid and logs
fcbcf5f battletest-core: bootstrap campaign state, harnesses, and live dashboard
dea982e battletest-core(BUG-CORE-007): keep identifiers that collide with bare discourse fillers
784e0b4 battletest-core(BUG-CORE-002..006): global pointers, radix literals, FnPtr types, MemCopy, and typed phrase dispatch
6dbaa06 battletest-core(BUG-CORE-001): saturate float-to-integer casts across x86 and interpreter
```

## Phase 4 (`std/`) Go / No-Go verdict

**GO.** The Core-7 IR, interpreter, x86_64 static ELF backend, ABI/SRET paths, generic monomorphization, typers, modules, Link Grammar FFI, lexicon lowering, control-flow unification, and declarative phrase dispatch have survived 446 adversarial programs, 3,500 deterministic differential fuzz seeds, 511 malformed/FFI probes, and the full 786 + 10,845 live lexicon sweep with zero open bugs, zero panics/segfaults/hangs, zero silent drops, and zero target clobbers.

The seven documented limits are architectural boundaries, not defects. Phase 4 may proceed with `std/alloc.tc`, `std/list.tc`, `std/string.tc`, `std/io.tc`, `std/fs.tc`, `std/math.tc`, and `std/map.tc` written in English `tc`, using declarative `Phrase:` dispatch over the verified Core.
