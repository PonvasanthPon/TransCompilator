# Changelog

All notable changes to `tc_r1` are recorded here. The project follows a
human-readable version history; release tags are SemVer-compatible.

## [Unreleased]

No unreleased changes.

## [0.1.0-alpha-preview] — 2026-11-24

First public preview of the `tc_r1` compiler and toolchain.

### Added

- **Nine-crate `std`-only Rust workspace** with zero external crates in the
  root `Cargo.lock`.
- **Core-7 IR** with typed values, memory, control flow, functions, aggregates,
  raw syscalls, traps, a verifier, formatter, and byte-accurate reference
  interpreter.
- **Static zero-libc x86_64 ELF64 backend** using Intel-syntax emission, GNU
  `as`, and static `ld`.
- **English frontend** built on vendored CMU Link Grammar 5.13.2:
  segmentation, clause normalization, three-way intent classification,
  discourse/anaphora tracking, compositional `where`/`Given`/`then` graphs,
  quantifiers, and natural expression parsing.
- **Provenance-backed lexicon**: OEWN-2025 WordNet rows, VerbNet 3.4 frames,
  irregular morphology, and closed-class words.
- **Eight pure Version 2 English `std/*.eng` modules**: `core`, `alloc`, `io`,
  `string`, `list`, `fs`, `map`, and `math`.
- **`r1-editor`** native `std`-only terminal editor with in-process compiler
  bridge, presets, trace/paraphrase/IR panes, and interpreter/ELF64 parity.
- **`tc-lsp`** native `std`-only JSON-RPC language server with diagnostics,
  hover, completion, definition, document symbols, and semantic tokens.
- **Zed integration** with an offline zero-AI dev extension, `Liberation Serif`
  novel typography, semantic-token highlighting, and five `tc:` tasks.
- **Verification suite**: 1,054 tests covering CLI, semantics, stdlib, LSP,
  editor, orthogonality, human-English battle tests, interpreter/ELF64 parity,
  and pure-English gates.
- **Project documents**: `AGENTS.md`, `SKILL.md`, `MANIFEST.md`,
  `CONTRIBUTING.md`, and this changelog.

### Changed

- Rewrote all eight `std/*.eng` modules in natural, casual human English while
  preserving every public function name, `Phrase:` signature, `Call this as`
  alias, record field, parameter type, and return type.
- Replaced the default MIT license with **All Rights Reserved**:
  `Copyright (c) 2026 Ponvasanth. All Rights Reserved.`
- Set the release version to `0.1.0-alpha-preview` across Cargo, CLI, LSP,
  editor, Zed manifests, and documentation.
- Consolidated duplicated top-level parsing helpers into `textutil`, removed
  the duplicated std-directory locators, and deleted unused frame variants.
- Replaced reachable production `unwrap`/`expect` calls with explicit error
  paths in the type checker, semantics frame lowering, CLI, editor runner, and
  syntax segmenter.
- Documented all FFI boundaries with `// SAFETY:` comments.
- Finalized portable paths, `.gitignore`, third-party notices, and lean
  release/relink tooling.

### Fixed

- Runtime-first semantic fixes for contractions, casual openers, comparison
  synonyms, ranges, fractions, list paraphrases, string output, state names,
  narrative omission, and transfer direction.
- Parser and lowering edge cases around empty loop bodies, unresolved anaphora,
  trailing prose, block segmentation, and exact source spans.
- Stdlib formatting, parsing, allocator alignment/coalescing, generic list
  sorting, maps, and file descriptor lifecycles.
- LSP diagnostics and Zed semantic-token behavior for `.eng` prose.

### Security

- Offline build and test policy enforced with `CARGO_NET_OFFLINE=true`.
- No network, browser, HTML, ML, or model runtime in workspace crates.
- Static output binaries use raw Linux syscalls and require no libc or dynamic
  loader.
- Vendored Link Grammar and generated linguistic tables retain their original
  licenses and provenance.

### Known limitations

- English frontend only; multilingual packs are planned in `MANIFEST.md`.
- x86_64 Linux only; ARM64, C17, Python, and assembly/object targets are
  planned.
- `0.1.0-alpha-preview` is an evolving grammar surface, not a stable language
  specification.
- The standard library is intentionally small; runtime `Set`, `Stack`, `Queue`,
  word splitting, and indexed removal are planned for `0.2.0-alpha`.
- Some prose constructs are intentionally rejected with structured diagnostics
  rather than guessed; see `docs/CONVERGENCE_LIMITS.md`.
- The repository is All Rights Reserved, not open source.

---

## Versioning policy

- `0.1.x-alpha-preview` — preview fixes and documentation.
- `0.2.0-alpha` — runtime collections and broader std algorithms.
- `0.3.0-beta` — multi-target backends and the first non-English frontend.
- `1.0.0` — stabilized multilingual Core-7 contract and module ecosystem.

See `MANIFEST.md` for the architectural roadmap.
