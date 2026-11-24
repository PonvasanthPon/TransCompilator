# MANIFEST.md — `tc` Architecture & Long-Term Roadmap

**Status:** `0.1.0-alpha-preview`  
**Owner:** Ponvasanth  
**Scope:** This document describes the intended long-term architecture of
`tc_r1` and the future multilingual `tc` compiler. Items marked **planned** are
not implemented in `0.1.0-alpha-preview`.

---

## 1. Vision — Universal Natural-Language Compilation

```text
Human Languages (Frontends)          Shared Middle-End          Targets (Backends)
───────────────────────────          ─────────────────          ──────────────────
English  (en)  [implemented] ──┐                            ┌──► x86_64 Machine Code / Static ELF64 [implemented]
Spanish  (es)  [planned]     ──┤                            ├──► ARM64 (aarch64) Machine Code / ELF64 [planned]
Japanese (ja)  [planned]     ──┼──► tc-typeck ──► Core-7 IR ┼──► Intel/AT&T Assembly (--emit=asm)     [planned]
Tamil    (ta)  [planned]     ──┤                            ├──► Portable C17 (tc-codegen-c)          [planned]
French   (fr)  [planned]     ──┘                            └──► Python 3 (tc-codegen-py)             [planned]
```

The invariant is **`N` natural-language frontends → one Core-7 IR → `M`
targets**. A language frontend must not grow its own private backend, and a
backend must not learn English-specific concepts.

---

## 2. What Already Exists

| Layer | Current implementation |
|---|---|
| English syntax | Vendored CMU Link Grammar 5.13.2, deterministic fallback |
| English semantics | Three-way intent gate, discourse stack, clause graph, compositional lowering |
| Shared typed middle-end | `tc-typeck`: modules, visibility, overloads, generics, monomorphization |
| Shared IR | `tc-ir`: Core-7 nodes, verifier, formatter, reference interpreter |
| Native backend | `tc-codegen-x86`: Intel syntax, GNU `as`, static `ld`, zero-libc ELF64 |
| Tooling | `tc` CLI, `r1-editor` TUI, `tc-lsp`, Zed extension |
| Standard library | Eight pure Version 2 English `std/*.eng` modules |
| Lexical evidence | OEWN-2025 WordNet, VerbNet-3.4, closed-class and morphology tables |
| Verification | 1,054 tests, interpreter/ELF64 parity, std purity gates, orthogonality matrix |

The shared middle-end is already natural-language-agnostic: it consumes a
typed AST, not English words. That is why a new language frontend can reuse
`tc-typeck`, `tc-ir`, and every backend unchanged.

---

## 3. Multilingual Frontend Architecture

A language pack is a set of crates that produce the existing typed AST/Core-7
IR contract. A pack must be deterministic, offline, and inspectable.

```text
tc-syntax-<lang>  ──► tc-semantics-<lang>  ──► shared tc-typeck ──► tc-ir
       │                      │
       └── tc-lexicon-<lang> ─┘
```

### 3.1 `tc-syntax-<lang>`

Responsibilities:

- sentence and paragraph segmentation for the language's punctuation/script,
- dependency or constituency parsing,
- morphological analysis where required,
- normalization into a clause representation comparable to `ClauseTree`.

Candidate engines:

| Language | Parsing / morphology strategy |
|---|---|
| Spanish (`es`) | Link Grammar or Universal Dependencies; Romance morphology |
| Japanese (`ja`) | morphological analyzer + dependency parser; no whitespace assumption |
| Tamil (`ta`) | agglutinative morphological analyzer + dependency parser |
| French (`fr`) | Link Grammar / UD; elision and contraction handling |

No ML models. A language pack may use a deterministic parser, a finite
grammar, or a vendored lexicon, but the result must be explainable and
reproducible.

### 3.2 `tc-lexicon-<lang>`

- Harvest read-only from Open Multilingual WordNet and language-specific
  VerbNet-like resources where available.
- Preserve provenance: dataset version, commit/tag, hash, anchors, counts.
- Generate compact sorted Rust tables; never commit raw corpora.
- Provide morphology/lemmatization tables for the language.
- Keep language-specific data out of English tables and out of Rust match
  statements.

### 3.3 `tc-semantics-<lang>`

- Language-specific intent classification, discourse, anaphora, honorifics,
  measure words, numeral systems, and clause composition.
- Lower directly into the shared typed AST, not into a private IR.
- Reuse shared concepts where they are genuinely universal; declare
  language-specific concepts explicitly rather than forcing English grammar.
- Preserve 1-based source spans through all rewrites.

### 3.4 Language-pack contract

A pack is complete only when it can:

1. parse and classify the language's everyday computational prose,
2. lower algorithmic programs to runtime Core-7 loops/branches,
3. produce deterministic paraphrases or trace output,
4. pass the same interpreter/ELF64 parity harness as English,
5. report unsupported constructs with clean diagnostics, never panics,
6. keep its lexicon provenance reproducible.

---

## 4. Multi-Target Backends

### 4.1 Native targets

- **x86_64** (`tc-codegen-x86`) — implemented; static zero-libc ELF64.
- **ARM64 / aarch64** (`tc-codegen-arm64`) — planned; same Core-7 verifier and
  ABI discipline.
- **Assembly emitters** — planned `--emit=asm` (Intel and AT&T) and
  `--emit=obj` for inspection and embedding.

### 4.2 Portable transpilers

- **C17** (`tc-codegen-c`) — planned; emits portable C with explicit integer
  widths, no libc assumptions beyond freestanding primitives, and a documented
  runtime shim for syscalls.
- **Python 3** (`tc-codegen-py`) — planned; emits deterministic Python for
  teaching, differential testing, and rapid validation. It is not intended as
  a performance target.

### 4.3 Multi-way parity

Every backend must pass the same semantic contract:

```text
reference interpreter == x86_64 ELF64 == ARM64 ELF64
                        == C17 build     == Python 3 run
```

Parity covers exit status, stdout, stderr, and created-file effects where the
target supports them. A target that cannot express a primitive must reject the
module with a clean diagnostic, not silently diverge.

---

## 5. Runtime `std/` Growth

The next standard-library milestones are chosen to unlock whole classes of
honest runtime algorithms without new Core-7 opcodes.

### 5.1 `0.2.0-alpha` runtime data structures

- `split <text> into words` — runtime text tokenization.
- `Set<T>` — insertion, membership, removal, iteration.
- `Stack<T>` — push/pop/peek/is-empty.
- `Queue<T>` — enqueue/dequeue/peek/is-empty.
- indexed `remove at` for `List<T>`.
- frequency counters over words and numbers.
- bracket-matching and simple graph/BFS workloads.

These must be implemented in `std/*.eng` over the existing Core-7 primitives
(raw memory, loops, branches, syscalls). No new opcodes.

### 5.2 Later library work

- deterministic sorting/searching for strings and records,
- stable map iteration,
- growable byte buffers and UTF-8 helpers,
- file/path utilities,
- generic graph adjacency helpers,
- error/result ergonomics,
- language-pack-aware text normalization.

### 5.3 Non-goals for the standard library

- No hidden allocation in Core-7.
- No compiler-known “magic” functions.
- No dependence on libc.
- No network, browser, or ML runtime.

---

## 6. Version Milestones

### `0.1.0-alpha-preview` — current

- Nine-crate `std`-only Rust compiler.
- Core-7 IR + verifier + reference interpreter.
- Static zero-libc x86_64 ELF64 backend.
- OEWN-2025/VerbNet-3.4 lexicon.
- English intent/discourse/clause frontend.
- Eight human-English `std/*.eng` modules.
- `r1-editor` TUI, `tc-lsp`, Zed novel typography.
- 1,054 tests, all offline.
- Proprietary All Rights Reserved license.

**Exit criteria:** clean full gate run, zero external crates, zero panics in
the fuzz sweep, exact interpreter/ELF64 parity, and a publishable repository.

### `0.2.0-alpha` — runtime collections

- `split into words`, `Set<T>`, `Stack<T>`, `Queue<T>`, indexed removal.
- Honest runtime frequency counters, bracket stacks, and BFS/graph demos.
- Expanded std parity and zero-leak tests.
- No new IR opcodes.

### `0.3.0-beta` — multi-target + first non-English pack

- Pluggable `tc-codegen-c` and `tc-codegen-py`.
- ARM64 backend or assembly/object emission.
- First non-English frontend proof-of-concept (`es` or `ta` preferred).
- Shared language-pack contract documented and tested.
- Multi-way parity harness for at least two targets.

### `1.0.0` — stabilized universal compiler

- Stabilized Core-7 semantics and versioned language-pack API.
- Multilingual frontends with reproducible lexicon provenance.
- Multi-target parity across native and portable backends.
- Stable `std/*.eng` module ecosystem.
- Clear compatibility and deprecation policy.
- Chosen open-source or dual license (if the owner decides to change the
  current All Rights Reserved status).

---

## 7. Architecture Decision Rules

1. **Language packs own language, not backends.** They may not emit machine
   code or add private IR nodes.
2. **Backends own targets, not languages.** They may not inspect English
   strings or `std` names.
3. **Core-7 stays tiny.** A new opcode requires a written justification showing
   that no `std/*.eng` implementation can express the operation.
4. **Everything is deterministic.** No model weights, nondeterministic
   schedulers, or hidden network access.
5. **Every rewrite preserves spans.** Multilingual morphology must still point
   diagnostics at the user's source line.
6. **No silent drops.** Unsupported input becomes a structured diagnostic.
7. **Parity is the contract.** A backend is not “done” until it matches the
   reference interpreter on the shared suite.

---

## 8. Repository Implications

Planned additions should follow the existing shape:

```text
crates/
  tc-syntax-es/       # planned language pack
  tc-lexicon-es/      # planned provenance tables
  tc-semantics-es/    # planned Spanish lowering
  tc-codegen-arm64/   # planned native backend
  tc-codegen-c/       # planned portable backend
  tc-codegen-py/      # planned portable backend
```

Each new crate must:

- remain `std`-only,
- declare only local path dependencies,
- add no registry source/checksum to the root lockfile,
- provide crate-level docs and focused tests,
- pass the same fmt/lint/audit/test gates.

---

## 9. Release Checklist for Future Milestones

- [ ] `Cargo.lock` = local packages only, 0 sources, 0 checksums
- [ ] `make fmt-check`, `make lint`, `make audit`, `make test`
- [ ] `tc --audit`, `tc --selftest`
- [ ] interpreter/ELF64 parity on the full suite
- [ ] std purity gates
- [ ] LSP zero-diagnostic sweep over all `std/*.eng`
- [ ] zero panics in the fuzz sweep
- [ ] docs, changelog, and manifest updated
- [ ] license and third-party notices reviewed
- [ ] lean release rebuild and Zed relink
- [ ] no push without explicit owner approval

---

## 10. Open Questions

- Which second language best validates the language-pack contract: Spanish
  (shared script, Romance morphology) or Tamil (agglutination, different
  script and word order)?
- Should portable C be the first non-native backend, or should ARM64 come
  first to keep the ABI work close to x86_64?
- How much of `std/` should be language-pack-aware versus language-neutral?
- What compatibility promise should `1.0.0` make for Core-7 and `std/`?

These are decision gates, not commitments. The invariant is the architecture:
**many languages in, one Core-7 middle, many targets out, all deterministic.**
