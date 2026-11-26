# AGENTS.md — Engineering Contract for `tc_r1`

**Project:** `tc_r1`  
**Version:** `0.1.0-alpha-preview`  
**Audience:** coding agents, maintainers, and contributors changing this repository  
**Rule:** if a request conflicts with this contract, stop and ask before editing.

`tc_r1` is a deterministic, general-purpose English compiler written in a
`std`-only Rust workspace. English source is parsed, classified, and lowered to
Core-7 IR, then emitted as a static zero-libc x86_64 ELF64 executable. There are
no external crates, no ML models, no network calls, and no whole-program
compile-time evaluators.

---

## 1. Irreducible Core Principles

### 1.1 Zero external crates

- The root `Cargo.lock` must contain exactly the nine local `tc-*` packages,
  with **0 `source =` entries and 0 `checksum =` entries**.
- Workspace dependencies must be `std` or local `path = ...` dependencies only.
- Do not add crates.io, git, or registry dependencies to any workspace crate.
- The isolated Zed extension (`editors/zed/tc-eng-extension/`) is **not** a
  workspace member and may use `zed_extension_api`; its lockfile is ignored.
- The isolated browser bridge (`web/tc-wasm/`) is **not** a workspace member.
  It may depend only on local path crates from this checkout, must remain
  registry-free, and uses the pure `std` linear-memory C ABI for
  `wasm32-unknown-unknown`; its `target/` and lockfile are ignored.

### 1.2 Core-7 IR irreducibility

Core-7 contains seven irreducible machine primitives:

1. typed values,
2. raw memory,
3. control flow,
4. functions,
5. aggregates,
6. raw syscalls,
7. traps.

Do not add a print opcode, file opcode, list opcode, string opcode, English
node, or any other convenience opcode. **If a capability can live cleanly in
`std/*.eng`, it stays out of Core-7 and out of the backend.**

### 1.3 Three anti-hardcoding bans

1. **No per-verb lists in Rust.** Open-class verb behavior must come from
   generated OEWN/VerbNet provenance tables (`tc-lexicon`), not hand-written
   verb tables.
2. **No library phrase hacks in Rust.** `std/*.eng` behavior must follow from
   compositional syntax and declared `Phrase:` signatures, not special-cased
   phrase strings in compiler source.
3. **No prose noun blacklists.** Sentence classification uses grammar, tense,
   structural anchors, and lexicon evidence—never a hand-curated banned-noun
   list.

### 1.4 Runtime-first rule

- No whole-program compile-time evaluators.
- Algorithmic English must lower to real runtime `Stmt::Loop` and `Stmt::If`
  nodes.
- The IR runtime-honesty test must continue to pass:
  `algorithmic_battle_cases_lower_to_runtime_core7_control_flow`.
- A fast compile-time answer for a specific program shape is a regression, not
  an optimization.

### 1.5 Exact source-line mapping

- All diagnostics use **1-based** source lines and columns.
- Normalization, contraction expansion, filler stripping, and multi-action
  splitting must preserve the original line mapping.
- Any change that rewrites text must keep a span or add a focused regression
  proving the reported line still points at the user's source.

---

## 2. The Nine-Crate Pipeline

```text
tc-lexicon ──► tc-syntax ──► tc-semantics ──► tc-typeck ──► tc-ir
                                                               │
                                                               ▼
                                                       tc-codegen-x86
                                                               │
                                      ┌────────────────────────┼────────────────────────┐
                                      ▼                        ▼                        ▼
                                   tc-cli                   tc-lsp                 tc-editor
                                 (tc binary)             (tc-lsp --stdio)        (r1-editor TUI)
```

The pipeline is intentionally acyclic. Front ends call the compiler
in-process; they never shell out to `tc`.

| Crate | Role |
|---|---|
| `tc-lexicon` | Zero-ML OEWN-2025/VerbNet-3.4 evidence, morphology, closed-class words |
| `tc-syntax` | Link Grammar FFI, paragraph/block segmentation, clause normalization |
| `tc-semantics` | Intent gate, discourse/anaphora, clause graph, English→typed-AST lowering |
| `tc-typeck` | Module resolution, type checking, generics/monomorphization, Core-7 lowering |
| `tc-ir` | Core-7 IR, verifier, formatter, reference interpreter |
| `tc-codegen-x86` | Intel-syntax assembly, GNU `as`/`ld`, static zero-libc ELF64 |
| `tc-cli` | `tc` command-line driver |
| `tc-lsp` | `std`-only JSON-RPC language server |
| `tc-editor` | `r1-editor` terminal editor and in-process parity runner |

The English front-end chain is:

```text
English text
  → tc-syntax segmentation + Link Grammar
  → tc-semantics intent/discourse/clause lowering
  → tc-typeck typed program + Core-7 IR
  → tc-ir verifier
  → tc-codegen-x86 static ELF64
```

---

## 3. `std/*.eng` Rules

- All eight modules (`core`, `alloc`, `io`, `string`, `list`, `fs`, `map`,
  `math`) are **pure Version 2 English**.
- Preserve every exported function name, `Phrase:` signature, `Call this as`
  alias, record/union name, field name, parameter type, and return type.
- `std/*.eng` modules do not receive automatic prelude injection. They must use
  explicit `Use core.` / `Use alloc.` / `Use io.` lines allowed by the module
  DAG.
- Keep module epigraphs and prose notes classified as `COMMENT_OMITTED` with
  zero `tc-lsp` diagnostics.
- `std_files_are_pure_version2_english` is a hard gate. Do not introduce
  symbolic comparators, colon headers, indentation-based blocks, C type tokens,
  dot-field notation, C-style calls, or underscored internal names.
- If clean prose exposes a general compiler bug, fix the general rule and add a
  regression. Do not hardcode a library-specific workaround.

---

## 4. FFI and Safety

The workspace has exactly two FFI boundaries:

1. **Link Grammar** in `crates/tc-syntax/src/lg_ffi.rs` — the parser library.
2. **Linux terminal ioctl** in `crates/tc-editor/src/terminal.rs` — required for
   raw mode, terminal size, and panic-safe restoration in `r1-editor`.

Every `unsafe` block must have a nearby `// SAFETY:` comment explaining:

- which pointer/reference is valid,
- who owns the allocation,
- how the call preserves Rust aliasing and lifetime rules,
- why the FFI signature matches the C ABI.

Do not add new `unsafe` code outside these boundaries. If a platform requires
it, document the exception in this file and add a focused test.

---

## 5. Error Handling and Code Quality

- Library code should return `Result` or `Option` on reachable paths.
- `.unwrap()` / `.expect()` / `panic!()` are acceptable only for:
  - mathematically infallible conversions with an explanatory message,
  - `String` formatting where `fmt::Error` cannot occur,
  - verified IR invariants with a concise justification,
  - test-only code.
- Never use `.unwrap()` merely because a parser "should" have validated input.
- Clippy must pass with `-D warnings`; do not silence a lint unless the comment
  explains the deliberate exception.
- Public entry points and nontrivial public types need concise `///` docs.
- Keep crate-level `//!` docs accurate, including the crate's pipeline role.

---

## 6. Required Verification Gates

Run from the repository root:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_NET_OFFLINE=true

make build
make fmt-check
make lint
make audit
make test

./target/debug/tc --audit
./target/debug/tc --selftest
```

Before finishing a change, also verify:

```bash
# Every std module has zero diagnostics
# (run tc-lsp or rely on the LSP test suite)

# Every std module stays pure Version 2 English
cargo test -p tc-semantics --test phase4_6_version2_english std_files_are_pure_version2_english
cargo test -p tc-semantics --test phase4_7_pure_english_polish std_files_are_pure_version2_english

# Static ELF64 parity and std behavior
cargo test -p tc-semantics --test phase4_stdlib_parity
```

After a full debug/test run, reclaim disk space and relink the live editor
binaries:

```bash
cargo clean
./editors/zed/install_zed_config.sh
strip --strip-unneeded target/release/tc target/release/tc-lsp target/release/r1-editor
```

The final tree should be roughly 25 MB, with only stripped release binaries in
`target/`.

---

## 7. Change Discipline

- Keep commits focused and atomic.
- Do not rewrite history or force-push.
- Do not run `git push` unless the repository owner explicitly asks.
- Never write to sibling or external repositories outside this checkout.
- Do not edit generated lexicon tables without updating
  `goldmines/PROVENANCE.md` and the doc-consistency tests.
- Do not change `LICENSE`, `THIRD_PARTY_NOTICES.md`, or
  `vendor/link-grammar/LICENSE` without checking licensing implications.
- Do not add a dependency, a model, a network call, or a browser/HTML runtime.

---

## 8. Licensing

`tc_r1` is proprietary and **All Rights Reserved**:

```text
Copyright (c) 2026 Ponvasanth. All Rights Reserved.
```

Public viewing is permitted; copying, modification, redistribution, or
sublicensing requires prior written permission. Third-party components retain
their own licenses; see `THIRD_PARTY_NOTICES.md` and
`vendor/link-grammar/LICENSE`.

## 9. Escalation

Stop and ask before:

- adding a new IR opcode,
- adding a new external crate,
- adding a new `unsafe` boundary,
- changing exported `std` signatures,
- adding a compile-time whole-program evaluator,
- changing the license or version,
- rewriting git history.
