# tc_r1

`tc_r1` is a clean-slate Rust rewrite of TransCompilator built on the
**Core-7 + English `std/`** philosophy. The target is a deterministic,
inspectable English-to-semantics compiler with a static, zero-libc x86_64
ELF64 output path.

This repository is currently at **Phase 0**: toolchain verification, a
seven-crate `std`-only workspace, vendored linguistic assets, and smoke
tests. No IR, lexer, parser, semantics, or code generation is implemented
yet.

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
    tc-typeck/        # type checking
    tc-lexicon/       # lexical model
    tc-syntax/        # syntax analysis
    tc-semantics/     # semantic analysis
    tc-cli/           # `tc` command-line driver
  std/                # English std/ sources (empty in Phase 0)
  tests/              # integration tests (empty in Phase 0)
  docs/               # design documentation (empty in Phase 0)
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
- `tc --audit` checks the Phase 0 crate identities and reports scaffold
  status.

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
make clean       # cargo clean
```
