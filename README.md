# tc_r1

`tc_r1` is a clean-slate Rust rewrite of TransCompilator built on the
**Core-7 + English `std/`** philosophy. The target is a deterministic,
inspectable English-to-semantics compiler with a static, zero-libc x86_64
ELF64 output path.

This repository is at **Phase 3.6**: the Core-7 typed IR, exhaustive
verifier, byte-accurate reference interpreter, direct x86_64 static zero-libc
ELF64 backend, the high-level `tc-typeck` front end, the zero-ML
`tc-lexicon` word/verb engine, and a finite English front end are implemented.
English source now flows through vendored CMU Link Grammar 5.13.2,
paragraph/block segmentation, a normalized clause tree, a strict three-way
intent gate, a declarative `Phrase:` dispatcher, a DRS-style discourse
referent stack (`it`, `its <field>`, `the <type>`), a comma-aware boolean
condition parser, deterministic global/local quantifier lowering, and one
unified recursive Core-7 statement lowerer. `tc build`, `tc run`, `tc interp`,
`tc trace`, and `tc paraphrase` compile and inspect real English programs end
to end. Differential parity suites in
`crates/tc-codegen-x86/tests/`, `crates/tc-typeck/tests/`, and
`crates/tc-semantics/tests/` require exact exit-status/stdout/stderr
agreement between the reference interpreter and compiled ELF64 binaries.
The English `std/` library is not implemented yet.

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
    tc-syntax/        # Link Grammar FFI, segmentation, clause normalization
    tc-semantics/     # intent gate, phrase dispatch, unified Core-7 lowering
    tc-cli/           # end-to-end `tc` command-line driver
  std/                # English std/ sources (Phase 4)
  tests/              # additional integration tests
  docs/               # design documentation and the English language spec
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
- `tc --audit` checks the workspace crate identities and reports the exact
  WordNet, VerbNet, morphology, and closed-class counts.
- `tc --selftest` verifies, interprets, compiles, executes, and
  differentially checks a small Core-7 module, then prints `SELFTEST PASS`.
- `tc build <file.tc> [-o <out_elf>] [-I <dir>]` compiles English to a
  static zero-libc ELF64 binary.
- `tc run <file.tc> [-I <dir>] [-- <args...>]` compiles and executes it.
- `tc interp <file.tc> [-I <dir>] [-- <args...>]` executes the verified
  Core-7 IR in the reference interpreter.
- `tc trace <file.tc> [-I <dir>] [--paraphrase]` prints per-sentence Link
  Grammar counts, lexicon provenance, intent verdicts, and formatted TC-IR;
  `--paraphrase` appends canonical resolved English.
- `tc paraphrase <file.tc> [-I <dir>]` type-checks the program and prints the
  deterministic paraphrase echo, including `[it -> cash]`, `[its x -> p.x]`,
  `[the point -> origin]`, condition grouping, and omitted prose markers.

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
signatures for the English front end.

## English front end

`tc-syntax` wraps the vendored CMU Link Grammar 5.13.2 shared library through
a minimal safe FFI layer with owned cleanup and `$ORIGIN` run-paths. It
segments flowing paragraphs, numbered steps, semicolon clauses, strings,
decimals, colon-introduced indented blocks, and explicit `End.` closers, then
normalizes dependency links into active, passive, and imperative clause trees.
When Link Grammar produces no linkage, deterministic closed-class role
extraction takes over.

`tc-semantics` classifies every sentence into exactly one of `EXEC`,
`COMMENT_OMITTED`, or `UNSUPPORTED` using structural anchors, grammatical
tense, `tc-lexicon` evidence, and in-scope declarative phrases. It matches
`Phrase:` signatures attached to English functions, binds `<param>` captures
as expressions, and lowers the whole program through a single recursive
`lower_stmt` path with a 32-level guard. Identifiers are case-insensitive while
string and byte literals preserve exact case.

## Lexicon engine

`tc-lexicon` is a compact, deterministic, provenance-backed semantic layer:

- **786** WordNet rows from 27 exact OEWN-2025 anchor synsets, depth <= 3,
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

The two extra WordNet anchors are depth-zero scalar-magnitude anchors for
`increase` and `decrease`; they add only their own synset members and do not
open a broad `verb.change` hyponym tree.

## Anti-hardcoding bans

The compiler must derive behavior from general linguistic structure, never
from hand-curated English fragments. Three bans are non-negotiable:

1. **No per-verb lists in Rust.** Verb behavior belongs in general lexical
   or syntactic rules and data, not hardcoded Rust tables. Every open-class
   verb resolves through `tc-lexicon` generated WordNet/VerbNet tables.
2. **No library phrase hacks in Rust.** Multi-word expressions must follow
   from compositional rules, not special-cased phrase strings in source.
3. **No prose noun blacklists.** Token classification uses grammar, tense,
   structural anchors, and lexicon evidence, never hand-written banned-noun
   lists.

## Commands

```bash
make build       # cargo build --workspace
make release     # cargo build --workspace --release
make test        # build + cargo test --workspace
make lint        # cargo clippy --workspace --all-targets -- -D warnings
make fmt-check   # cargo fmt --all -- --check
make audit       # zero-ML, zero-external-crate, zero-forbidden-reference checks
make selftest    # verify + interpret + compile + execute + differential check
make clean       # cargo clean
```
