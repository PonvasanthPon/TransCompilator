# Contributing to `tc_r1`

## Licensing first

`tc_r1` is **proprietary and All Rights Reserved**:

```text
Copyright (c) 2026 Ponvasanth. All Rights Reserved.
```

The repository is public for viewing and evaluation. No permission is granted
to copy, modify, merge, publish, distribute, sublicense, or sell copies of the
software or its documentation without prior written permission from the
copyright holder.

- Do not open an external fork or redistribute the repository until the
  copyright holder has chosen an open-source license.
- Do not submit third-party code, datasets, or assets unless their licenses are
  compatible and documented in `THIRD_PARTY_NOTICES.md`.
- Contact the repository owner/copyright holder before submitting a pull
  request that would require a contribution license. By submitting a patch, you
  confirm that you have the right to contribute it and grant the copyright
  holder permission to use it in this project.

Third-party components keep their own licenses:

- CMU Link Grammar 5.13.2 — LGPL-2.1-or-later,
- OEWN 2025 — CC-BY-4.0,
- VerbNet 3.4 — CC-BY-4.0,
- `zed_extension_api` — Apache-2.0.

See `THIRD_PARTY_NOTICES.md` and `vendor/link-grammar/LICENSE`.

---

## 1. Prerequisites

- Linux x86_64
- Rust 1.85+ with edition 2024 support
- GNU `make`, `as`, and `ld`
- Python 3 for the optional Zed helper and goldmine harvest scripts
- No network access required; Cargo runs offline

```bash
git clone <repository-url> tc_r1
cd tc_r1
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_NET_OFFLINE=true
make build
./target/debug/tc --selftest
```

For Zed:

```bash
./editors/zed/install_zed_config.sh
```

The helper builds and links `tc`, `tc-lsp`, and `r1-editor`, backs up your Zed
settings, and merges the offline configuration.

---

## 2. Architecture rules

Read `AGENTS.md` first. The non-negotiables are:

1. **Zero external crates** in the root `Cargo.lock`.
2. **Core-7 IR irreducibility** — no print/file/list/string/English opcodes.
3. **Three anti-hardcoding bans** — no per-verb Rust lists, no library phrase
   hacks, no prose noun blacklists.
4. **Runtime-first rule** — no whole-program compile-time evaluators; all
   algorithms lower to runtime `Stmt::Loop` and `Stmt::If`.
5. **Exact 1-based source spans** through normalization and expansion.
6. **Pure Version 2 English** in every `std/*.eng` module.
7. **No silent drops** — unsupported input becomes a structured diagnostic.
8. **Interpreter/ELF64 parity** is the backend contract.

The crate pipeline is documented in `AGENTS.md` and `MANIFEST.md`.

---

## 3. Making a change

1. Pick a focused problem. Avoid mixing refactors, behavior changes, and docs
   in one commit unless they are inseparable.
2. Add or update a regression test that fails before the fix and passes after.
3. Keep compiler rules general. If a fix only works for one English sentence
   or one library function, it is probably in the wrong layer.
4. Preserve `std/*.eng` ABI: function names, `Phrase:` headers, `Call this as`
   aliases, record fields, parameter types, and return types.
5. Do not add dependencies, models, network calls, browser/HTML runtimes, or
   new `unsafe` boundaries.
6. If you touch FFI, add or update a `// SAFETY:` comment explaining pointer
   validity, ownership, aliasing, and ABI matching.
7. Update docs when behavior, commands, versions, or limits change.

### Commit messages

Use a short imperative subject and a focused body when needed:

```text
fix(semantics): preserve spans through contraction expansion
feat(std): add runtime text splitting helpers
docs: clarify language-pack provenance contract
```

Do not rewrite history or force-push. Do not run `git push` unless the owner
asks.

---

## 4. Mandatory pre-commit gates

Run all of these from the repository root:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_NET_OFFLINE=true

make fmt-check
make lint
make audit
make test
./target/debug/tc --audit
./target/debug/tc --selftest
```

For `std/*.eng` changes, also run:

```bash
cargo test -p tc-semantics --test phase4_6_version2_english std_files_are_pure_version2_english
cargo test -p tc-semantics --test phase4_7_pure_english_polish std_files_are_pure_version2_english
cargo test -p tc-semantics --test phase4_stdlib_parity
cargo test -p tc-cli --test human_english_battletest
```

For `tc-lsp` or Zed changes:

```bash
cargo test -p tc-lsp --test lsp_protocol_tests
bash -n editors/zed/install_zed_config.sh
python3 -c 'import json; json.load(open("editors/zed/settings.json"))'
python3 -c 'import json; json.load(open("editors/zed/tasks.json"))'
```

A successful PR must report the exact test count and any known limitations.

---

## 5. Coding standards

- Rust code: `cargo fmt`, clippy with `-D warnings`, no new warnings.
- Prefer `Result`/`Option` over panics on reachable paths.
- Keep `.unwrap()`/`.expect()` limited to documented invariants or test code.
- Public APIs: concise `///` docs; crate roots: accurate `//!` pipeline docs.
- Comments explain *why*, not *what*.
- No dead code. Remove unused variants, helpers, imports, and stale phase notes.
- Generated lexicon tables are deterministic and sorted; update
  `goldmines/PROVENANCE.md` and doc-consistency tests together.
- No hardcoded English phrase special cases in compiler Rust.
- No new compiler-side knowledge of `std/` function names unless it is part of
  the documented auto-prelude contract.

---

## 6. `std/*.eng` contributor rules

The standard library is readable English, not a second DSL.

- Module epigraph: one clear sentence ending with a period.
- Prose notes must classify as `COMMENT_OMITTED`; keep them short and
  declarative or noun-phrase based.
- No symbolic comparators, colon headers, indentation, C type tokens, dot-field
  notation, C-style calls, or underscored internal identifiers.
- Use explicit `Use` lines; `std` modules do not receive auto-prelude injection.
- Preserve runtime behavior and existing exported signatures.
- Add zero-leak and parity tests for allocator/list/map/string changes.
- Verify zero `tc-lsp` diagnostics for every edited module.

If clean prose exposes a general compiler limitation, fix the general
frontend/lowering rule and add a regression; do not add a library-specific
workaround.

---

## 7. Tests and verification map

| Area | Tests |
|---|---|
| Core-7 IR/verifier/interpreter | `crates/tc-ir`, `crates/tc-codegen-x86/tests/phase0_core7_parity.rs` |
| Type checker and generics | `crates/tc-typeck/tests/phase1_typeck_parity.rs` |
| Lexicon provenance | `crates/tc-lexicon/tests/doc_consistency.rs`, `phase2_lexicon_audit.rs` |
| English semantics | `crates/tc-semantics/tests/phase3_*`, `battletest_*`, `human_english_battletest` |
| Stdlib behavior | `phase4_stdlib_parity.rs`, `phase4_5_natural_english_stdlib.rs`, `diagnostics_and_std_helpers.rs` |
| Std purity | `phase4_6_version2_english.rs`, `phase4_7_pure_english_polish.rs` |
| Orthogonality | `crates/tc-semantics/tests/orthogonality_audit.rs` |
| LSP | `crates/tc-lsp/tests/lsp_protocol_tests.rs` |
| Editor | `crates/tc-editor/tests/` |

When in doubt, run the full `make test`; it is the authoritative gate.

---

## 8. Repository hygiene

After a full debug/test run:

```bash
cargo clean
./editors/zed/install_zed_config.sh
strip --strip-unneeded target/release/tc target/release/tc-lsp target/release/r1-editor
```

Keep the final tree around 25 MB. Do not commit `target/`, generated ELF files,
logs, temporary harnesses, or editor caches.

---

## 9. Reporting issues

Include:

- exact `.eng` source or a minimal reduction,
- `tc --version` and commit SHA,
- `tc trace` output,
- expected vs actual behavior,
- whether `tc interp` and the static ELF agree,
- platform and toolchain details.

Do not include secrets, personal paths, or third-party data that cannot be
redistributed.

## 10. Questions

Read, in order:

1. `AGENTS.md`
2. `SKILL.md`
3. `MANIFEST.md`
4. `docs/TC_ENGLISH_SPEC.md`
5. `docs/CONVERGENCE_LIMITS.md`
6. `README.md`

If the answer is still unclear, open a focused issue with a reproducer.
