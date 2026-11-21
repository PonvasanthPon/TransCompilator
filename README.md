# tc_r1

`tc_r1` is a clean-slate Rust rewrite of TransCompilator built on the
**Core-7 + English `std/`** philosophy. The target is a deterministic,
inspectable English-to-semantics compiler with a static, zero-libc x86_64
ELF64 output path.

This repository is at **Phase 2**: the Core-7 typed IR, exhaustive verifier,
byte-accurate reference interpreter, direct x86_64 static zero-libc ELF64
backend, the high-level `tc-typeck` front end, and the zero-ML
`tc-lexicon` word/verb engine are implemented. The type checker provides
multi-file modules, visibility, scoping, strict typing, type-directed
overloads, phrase metadata, and compile-time generics. The lexicon engine
harvests provenance-carrying Open English WordNet 2025 and CU Boulder
VerbNet 3.4 tables without per-verb Rust mappings. Differential parity suites
in `crates/tc-codegen-x86/tests/` and `crates/tc-typeck/tests/` require exact
exit-status/stdout/stderr agreement between the reference interpreter and
compiled ELF64 binaries. English parsing and the `std/` library are not
implemented yet.

## Hard properties

- **`std`-only Rust** — zero crates.io dependencies; only local workspace
  path dependencies.
- **Zero ML** — no models, embeddings, neural runtimes, or statistical
  weight files.
- **Zero network** — builds and tests run fully offline.
- **Static zero-libc target** — final binaries are ELF64 x86_64 with raw
  Linux syscalls and no `libc` or dynamic loader.
- **Vendored, read-only linguistic assets** — Link Grammar under
  `vendor/link-grammar/` and goldmine tooling under `goldmines/`.

## Workspace layout

```text
tc_r1/
  Cargo.toml
  Makefile
  README.md
  .gitignore
  vendor/link-grammar/
  goldmines/
  crates/
    tc-ir/            # intermediate representation
    tc-codegen-x86/   # ELF64 x86_64 backend
    tc-typeck/        # typed AST, modules, generics, lowering
    tc-lexicon/       # WordNet/VerbNet/closed-class lexicon engine
    tc-lexicon/       # lexical model
    tc-syntax/        # syntax analysis
    tc-semantics/     # semantic analysis
    tc-cli/           # `tc` command-line driver
  std/                # English std/ sources (empty in Phase 2)
  tests/              # integration tests (empty in Phase 2)
  docs/               # design documentation (empty in Phase 2)
```

## Dependency graph

```text
tc-ir           (no dependencies)
tc-codegen-x86  -> tc-ir
tc-typeck       -> tc-ir
tc-lexicon      (no dependencies)
tc-syntax       -> tc-lexicon
tc-semantics    -> tc-syntax, tc-lexicon, tc-typeck, tc-ir
tc-cli          -> all six library crates (binary: tc)
```

The graph is strictly acyclic. `crates/tc-cli` produces the `tc` binary:

- `tc --version` prints `tc_r1 0.1.0`.
- `tc --audit` checks the workspace crate identities and reports scaffold
  status.
- `tc --selftest` verifies, interprets, compiles, executes, and
  differentially checks a small Core-7 module, then prints `SELFTEST PASS`.

## Core-7 IR and backend

`tc-ir` exposes `types`, `ir`, `verify`, `fmt`, and `interp` modules. The IR
contains only seven irreducible machine primitives: typed values, raw
memory, control flow, functions, aggregates, raw syscalls, and traps. There
are no heap `String`/`List` operations, print statements, file opcodes, or
English-specific nodes.

`tc-codegen-x86` emits Intel-syntax assembly, assembles with GNU `as --64`,
and links with `ld -static`. It implements the System V AMD64 integer and SSE
argument/return conventions, spillover stack arguments, nested-call-safe
argument evaluation, indirect function calls, width- and sign-accurate
integer operations, IEEE-754 `f32`/`f64`, aggregates in memory, and raw
Linux syscalls with the `r10` fourth-argument convention.

## High-level front end

`tc-typeck` is a language-agnostic front end over the Core-7 IR:

- `ast` — typed high-level AST, declarative phrase patterns, spans, errors.
- `resolve` — module table, import DAG validation, aliases, visibility.
- `check` — lexical scopes, local inference, strict type checking,
  type-directed overloads, and generic type-argument inference.
- `mono` — demand-driven, deterministic monomorphization with an
  infinite-instantiation depth guard.
- `lower` — verified lowering to `tc-ir`, including `.rodata`
  deduplication and an internal pointer ABI for aggregate parameters and
  returns.

`check_and_lower` is the public AST-to-IR entry point;
`collect_phrases` validates and exports declarative English phrase
signatures for Phase 3.

## Lexicon engine

`tc-lexicon` is a compact, deterministic, provenance-backed semantic layer:

- **784** WordNet rows from 25 exact OEWN-2025 anchor synsets, depth <= 3,
  sorted by `(lemma, pos, concept)`.
- **10,845** VerbNet 3.4 frames from all 329 class files and 609
  classes/subclasses, sorted by `(verb, prep, predicate, class_id)`.
- **103** irregular morphology rows plus 8,749 WordNet verb lemmas for
  deterministic lemma validation.
- **279** audited closed-class function words.
- `classify_verb`, POS-aware lookups, lemmatization, phrase-safe semantic
  concepts, and exact provenance IDs are exposed through binary-searched
  static tables.
- `goldmines/PROVENANCE.md` records dataset versions, hashes, anchors, counts,
  and the reproducible harvester commands.

## Anti-hardcoding bans

The compiler must derive behavior from general linguistic structure, never
from hand-curated English fragments. Three bans are non-negotiable:

1. **No per-verb lists in Rust.** Verb behavior belongs in general lexical
   or syntactic rules and data, not hardcoded Rust tables.
2. **No library phrase hacks in Rust.** Multi-word expressions must follow
   from compositional rules, not special-cased phrase strings in source.
3. **No prose noun blacklists.** Token classification must use principled
   lexical and distributional structure, not hand-written banned-noun lists.

## Commands

```bash
make build       # cargo build --workspace
make release     # cargo build --workspace --release
make test        # cargo test --workspace
make lint        # cargo clippy --workspace --all-targets -- -D warnings
make fmt-check   # cargo fmt --all -- --check
make audit       # zero-ML, zero-external-crate, zero-forbidden-reference checks
make selftest    # verify + interpret + compile + execute + differential check
make clean       # cargo clean
```
