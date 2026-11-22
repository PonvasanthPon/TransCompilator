# tc_r1

`tc_r1` is a clean-slate Rust rewrite of TransCompilator built on the
**Core-7 + English `std/`** philosophy. The target is a deterministic,
inspectable English-to-semantics compiler with a static, zero-libc x86_64
ELF64 output path.

This repository is **`tc_r1` v1.0**: the hardened Core-7 typed IR, exhaustive
verifier, byte-accurate reference interpreter, direct x86_64 static zero-libc
ELF64 backend, the high-level `tc-typeck` front end, the zero-ML `tc-lexicon`
word/verb engine, a finite English front end, the readable natural-English
`std/` standard library, a terminal editor (`r1-editor`), and a true native
`eframe`/`egui` desktop editor (`r1-editor-gui`) are implemented. English source now flows through vendored
CMU Link Grammar 5.13.2, paragraph/block segmentation, a normalized clause
tree, a strict three-way intent gate, a declarative `Phrase:` dispatcher, a
DRS-style discourse referent stack (`it`, `its <field>`, `the <type>`), a
comma-aware boolean condition parser, deterministic global/local quantifier
lowering, and one unified recursive Core-7 statement lowerer. `tc build`,
`tc run`, `tc interp`, `tc trace`, and `tc paraphrase` compile and inspect
real English programs end to end. Differential parity suites in
`crates/tc-codegen-x86/tests/`, `crates/tc-typeck/tests/`, and
`crates/tc-semantics/tests/` require exact exit-status/stdout/stderr
agreement between the reference interpreter and compiled ELF64 binaries,
including sandboxed file creation, appends, seeks, and deletion.

The standard library in `std/` is written entirely in readable `.eng`
English. It provides `Option`/`Result` and panic helpers, a free-list
allocator, exact console formatting and byte-safe input, heap strings,
generic `List of T`, streaming file I/O, `sqrt`/integer math, and
open-addressing `IntMap`/`StrMap` tables. The source uses prose-style
declarations such as `A public record ... has ...`, `Keep a shared ...`,
`Publicly, to ... given ..., returning ... . ... Done.`, `Give back ...`,
`Clear bits ... from ...`, and `Ask Linux kernel ... with ...`. Programs that
import `std/` still compile to static zero-libc ELF64.

## v1.0 convergence status

The v1.0 compiler is the fixed point of a five-round zero-bug convergence
campaign: **640 fresh `.eng` programs** across eight English-variety bands,
**15,000 disjoint differential/paraphrase/prose/crash seeds**, and **7
root-cause bugs fixed** with permanent regressions. The final suite is
**663/663 passing**, and Round 5 completed its entire fresh budget with zero
bugs. The live-test hotfix and orthogonality audit add four permanent tests,
for a current suite of **747/747 passing**. All eight `std/*.eng` modules pass
the Version 2 pure-English readability gate. Compiler diagnostics now suggest
natural spellings for unknown names and fields, explain reserved control words,
and point unresolved anaphors at concrete fixes; `std/list` and `std/string`
add everyday sum/average/minimum/maximum/contains/find/reverse and text
starts/ends/contains/trim/parse/format helpers in pure Version 2 English.

```bash
./target/debug/tc --selftest
./target/debug/tc build program.eng -o program.elf
./target/debug/r1-editor program.eng
./target/debug/r1-editor-gui program.eng
```

Documentation:

- [English language specification](docs/TC_ENGLISH_SPEC.md)
- [Convergence final report](docs/CONVERGENCE_FINAL_REPORT.md)
- [Convergence limits](docs/CONVERGENCE_LIMITS.md)
- [Vocabulary & grammar coverage audit](docs/COVERAGE_AUDIT.md)
- [Round history](docs/CONVERGENCE_ROUNDS_HISTORY.json)
- [Bug catalog](docs/CONVERGENCE_BUGS.json)

## Hard properties

- **`std`-only compiler and TUI** — the seven compiler crates and the
  `r1-editor` TUI have zero crates.io dependencies; only local workspace path
  dependencies.
- **Native GUI exception** — `tc-editor-gui` is the only crate allowed to use
  crates.io, and only `eframe`/`egui` for a real Linux desktop window.
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
    tc-editor/        # native std-only R1 Editor (`r1-editor`) for .eng programs
    tc-editor-gui/    # native eframe/egui desktop editor (`r1-editor-gui`)
  std/                # 8 Version 2 pure-English standard-library modules
  tests/              # integration-test placeholder
  docs/               # spec, convergence report, limits, and round history
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
tc-editor       -> tc-ir, tc-codegen-x86, tc-typeck, tc-lexicon, tc-syntax, tc-semantics
tc-editor-gui   -> tc-editor, compiler crates, eframe/egui (binary: r1-editor-gui)
```

The graph is strictly acyclic. The two editor crates call the compiler
in-process; neither shells out to a compiler binary. Only the native GUI
links `eframe`/`egui`; all compiler crates and the TUI remain Rust `std`-only. `crates/tc-cli` produces
the `tc` binary:

- `tc --version` prints `tc_r1 0.1.0`.
- `tc --audit` checks the workspace crate identities and reports the exact
  WordNet, VerbNet, morphology, and closed-class counts.
- `tc --selftest` verifies, interprets, compiles, executes, and
  differentially checks a small Core-7 module, then prints `SELFTEST PASS`.
- `tc build <file.eng|file.tc> [-o <out_elf>] [-I <dir>]` compiles English
  to a static zero-libc ELF64 binary. `.eng` is the primary source extension;
  `.tc` remains a compatible fallback.
- `tc run <file.eng|file.tc> [-I <dir>] [-- <args...>]` compiles and
  executes it.
- `tc interp <file.eng|file.tc> [-I <dir>] [-- <args...>]` executes the
  verified Core-7 IR in the reference interpreter.
- `tc trace <file.tc> [-I <dir>] [--paraphrase]` prints per-sentence Link
  Grammar counts, lexicon provenance, intent verdicts, and formatted TC-IR;
  `--paraphrase` appends canonical resolved English.
- `tc paraphrase <file.eng|file.tc> [-I <dir>]` type-checks the program and
  prints the deterministic paraphrase echo, including `[it -> cash]`,
  `[its x -> p.x]`, `[the point -> origin]`, condition grouping, and omitted
  prose markers.

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
make audit       # zero-ML, compiler/TUI std-only, native-GUI boundary checks
make selftest    # verify + interpret + compile + execute + differential check
make editor      # launch the terminal editor (r1-editor)
make editor-gui  # launch the offline GUI editor (r1-editor-gui)
make clean       # cargo clean
```

## English standard library (`std/`)

`Use io.`, `Use list.`, `Use alloc, core, and string.`, and the other
`std/` imports are resolved automatically after the importing file's
directory and any `-I` directories. Classic `Import io.` remains valid.
The resolver checks `TC_STD_DIR`, walks up from `/proc/self/exe`, and falls
back to the repository `std/` directory for `cargo test`. Transitive imports
are loaded with cycle detection.

```text
std/core.eng     Option, Result, panic, assert_true, mem_zero, mem_eq
std/alloc.eng    reusable-block free-list heap over Linux pages
std/io.eng       Say/Show/Print/Display formatting and byte-safe stdin
std/string.eng   growable TcString with a final zero byte
std/list.eng     generic List of T: push, pop, get, set, sort, free
std/fs.eng       File handles plus save/display/delete/exists helpers
std/math.eng     sqrt, abs, min, max, pow, clamp
std/map.eng      IntMap of V and StrMap of V with linear probing
```

The reference interpreter implements `openat`, `read`, `write`, `close`,
`lseek`, and `unlinkat` inside a per-run working directory, so file programs
produce byte-identical output and created-file bytes in both the interpreter
and the static ELF64 binary. No libc or filesystem helper is used by the
compiled program.


## R1 Editor (`.eng`)

`r1-editor` is a native, standalone, Rust `std`-only terminal editor for
English `.eng` programs. It calls the compiler crates in-process and never
spawns a browser, Python, or a GUI toolkit.

```bash
make build
./target/debug/r1-editor                 # empty .eng buffer
./target/debug/r1-editor budget.eng      # open or create budget.eng
./target/debug/r1-editor --list-presets  # eight ready-to-run .eng programs
./target/debug/r1-editor --check budget.eng
```

Keybindings:

```text
Ctrl+S Save    Ctrl+O Open    Ctrl+N New     Ctrl+Q Quit
Ctrl+Z Undo    Ctrl+Y Redo    Ctrl+F Find    Ctrl+E Pane
F1 Help        F2 Presets     F3 Stdin
F5 / Ctrl+R    Run native ELF64 + interpreter parity
F6 / Ctrl+P    Paraphrase echo
F7 / Ctrl+T    Trace + WordNet/VerbNet provenance
F8 / Ctrl+I    Core-7 TC-IR
F9 / Ctrl+B    Build <stem>.elf
```

The editor uses raw POSIX `termios`, an alternate screen, and RAII plus a
panic hook that always restores the user's terminal. Imports resolve
`<module>.eng` first and `<module>.tc` second in the active file's directory,
then in `-I` directories, then in the discovered `std/` directory. The three
`std/` presets (`06_std_io_and_math.eng`, `07_std_generic_list.eng`, and
`08_std_file_and_map.eng`) run through the same sandboxed parity path as the
five Core presets.

Build a standalone release binary with:

```bash
CARGO_NET_OFFLINE=true cargo build --release --bin r1-editor
./target/release/r1-editor
```

## R1 Editor GUI (`r1-editor-gui`)

`r1-editor-gui` is a true native Linux desktop IDE built with
`eframe`/`egui`. It opens a real OS window, renders with OpenGL through
Mesa, and calls `tc-editor::runner` and `tc-editor::highlight` in-process.
There is no HTTP server, no `TcpListener`, no HTML/CSS/JS, and no browser
launch. The default palette is a calm GitHub-style Light theme with pure
white editor canvas, soft neutral chrome, and WCAG-friendly syntax colors;
a quiet Soft Dark theme is available from the toolbar.

```bash
make build
./target/debug/r1-editor-gui                    # open an empty native window
./target/debug/r1-editor-gui budget.eng         # open a workspace file
./target/debug/r1-editor-gui --check budget.eng # headless compile/type check
./target/debug/r1-editor-gui --version
make editor-gui
```

The window has a top toolbar with Run, Paraphrase, Trace, Core-7 IR, Build,
New, Open, Save, Presets, `std/` Reference, and Light/Dark controls; a
collapsible workspace/preset sidebar; a line-numbered central editor with
live `tc-lexicon` highlighting and a stdin strip; and a resizable output
inspector with Run & Parity, Paraphrase, Trace, Core-7 IR, and Build tabs.
Run executes both the reference interpreter and the static zero-libc ELF64
in isolated temporary directories and verifies exact parity.
