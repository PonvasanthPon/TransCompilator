---
name: tc-eng-authoring
description: Write, compile, debug, and extend natural-English tc_r1 .eng programs and std/*.eng modules using the tc CLI, reference interpreter, static ELF64 backend, tc-lsp, and Zed.
---

# `tc-eng-authoring` — Writing Natural English for `tc_r1`

This skill is for anyone writing `.eng` programs or extending the eight
`std/*.eng` modules. The goal is readable English that the compiler can lower
deterministically to Core-7—not English-shaped C.

## 1. Quick start

```bash
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_NET_OFFLINE=true
make build

./target/debug/tc --selftest

cat > hello.eng <<'ENG'
Let total start at 2.
Add 3 to total.
Print total.
ENG

./target/debug/tc run hello.eng         # 5
./target/debug/tc build hello.eng       # hello.elf, static zero-libc ELF64
```

For a syntax-first look:

```bash
./target/debug/tc trace hello.eng
./target/debug/tc paraphrase hello.eng
```

## 2. Natural English idioms

### Bindings and arithmetic

```text
Let total start at 0.
Let price be 12.
Add 3 to total.
Subtract 1 from price.
Double price.
Halve price.
Square price.
Negate price.
Multiply price by 2.
Divide price by 4.
```

`start at` is preferred for counters and accumulators. `be` is still valid.

### Conditions

```text
If n is zero, give back 0.
If n isn't positive, give back 0.
If n is negative, give back 0 minus n.
If n is even, ...
If n is between 1 and 10, ...
If score is at least 90, ...
If score is at most 50, ...
If text contains "err", ...
If name starts with "Ada", ...
If pointer is null, ...
```

Use `Otherwise, ...` for the alternative branch.

### Loops

```text
Repeat while i is less than 10.
Add i to total.
Add 1 to i.
End the loop.

Repeat until i is at least 10.
Add 1 to i.
End the loop.

For every n from 1 to 5, add n to total.
For every n from 1 up to 10 by 2, ...
```

### Inline flow

```text
If total is zero, print "empty". Otherwise, print total.
Add 1 to i, then print i.
Give back n if n is positive, otherwise give back 0.
```

### Records

```text
A public record Point has x (a whole number) and y (a whole number).

Let origin be Point.
Set origin's x to 0.
Set origin's y to 0.
Update origin so its x is 3 and its y is 4.
Print origin's x plus origin's y.
```

### Functions

```text
To classify n (a whole number), named classify, giving a whole number.
If n is zero, give back 0.
If n is negative, give back 0 minus n.
Give back n.
```

Public functions used by other modules use `Publicly, to ...` and must keep
their `named` ABI alias.

### Multi-part output

```text
Print "a=" with a and ", b=" with b.
Show "n=" with n.
```

### `where` bindings

```text
Return the sum of a and b where a is 12 and b is 23.
Add 10 to x where x is 5.
```

### Spoken numbers and contractions

```text
Let count start at zero.
If count isn't one, double count.
If it's ready, print "go".
```

## 3. Auto-prelude vs `std` modules

Ordinary `.eng` programs may use `Print`, `Say`, `Show`, `Display`, `trim`,
`starts with`, `contains`, and common list/math helpers. The frontend
automatically loads `std/io`, `std/list`, `std/string`, and `std/math` when
their everyday forms are used.

`std/*.eng` modules are different: they **do not receive automatic prelude
injection**. They must declare their dependencies explicitly:

```text
Use alloc, core, and io.
```

## 4. CLI workflows

| Command | Use |
|---|---|
| `tc run file.eng` | Compile and execute |
| `tc build file.eng` | Emit static zero-libc ELF64 |
| `tc interp file.eng` | Run verified Core-7 in the reference interpreter |
| `tc trace file.eng` | Per-sentence Link Grammar, intent, and provenance |
| `tc paraphrase file.eng` | Deterministic canonical English echo |
| `tc --audit` | Crate, lexicon, lockfile, and network audit |
| `tc --selftest` | Verify, interpret, compile, execute, and compare |

Useful environment variable:

```bash
TC_STD_DIR=/path/to/std tc run program.eng
```

## 5. Authoring `std/*.eng`

- Keep the module's public ABI exactly as declared: function names, `Phrase:`
  signatures, `Call this as` aliases, record fields, parameter types, and
  return types.
- Keep the standard library in pure Version 2 English:
  - no `u64`/`i64`/`f64`/`ptr<...>`,
  - no symbolic `<`, `>`, `==`, `!=`, `<=`, `>=`,
  - no colon-ended headers or indented code blocks,
  - no dot-field notation (`p.x`), use `p's x`,
  - no C-style calls,
  - no underscored internal identifiers outside public aliases.
- Keep the top epigraph and prose notes as `COMMENT_OMITTED` sentences. Use
  short declarative or noun-phrase notes; avoid leading imperative verbs in
  notes.
- Preserve runtime behavior first. If clean phrasing reveals a general compiler
  gap, fix the general frontend rule and add a regression—never special-case a
  library phrase.
- Run the two purity gates and the stdlib parity suite after edits.

```bash
cargo test -p tc-semantics --test phase4_6_version2_english std_files_are_pure_version2_english
cargo test -p tc-semantics --test phase4_7_pure_english_polish std_files_are_pure_version2_english
cargo test -p tc-semantics --test phase4_stdlib_parity
```

## 6. Debugging

1. **Start with `tc trace`.** It shows the sentence text, Link Grammar
   provenance, intent verdict, normalized voice, and lowered form.
2. **Use `tc paraphrase`.** If the paraphrase is wrong, the lowering is wrong;
   if the paraphrase is right but output is wrong, inspect the IR or runtime.
3. **Check the intent class.** `COMMENT_OMITTED` means narrative/filler;
   `UNSUPPORTED` is a real compiler gap; `EXEC` must lower.
4. **Prefer `Let`/`Set`/`Update`/`Repeat`/`If` forms** when a sentence is
   ambiguous. See `docs/TC_ENGLISH_SPEC.md` and `docs/CONVERGENCE_LIMITS.md`.
5. **For std modules**, run `tc-lsp` diagnostics over the file and check that
   every epigraph/prose note is still `COMMENT_OMITTED`.

## 7. Zed + `tc-lsp`

```bash
./editors/zed/install_zed_config.sh
```

The helper builds and links `tc`, `tc-lsp`, and `r1-editor`, merges the offline
zero-AI settings, and installs the five `tc:` tasks.

- Install the dev extension from `editors/zed/tc-eng-extension`.
- Open a `.eng` file for diagnostics, hover, completion, definition, outline,
  and semantic tokens.
- Use the task picker (`Alt+T` in the default Zed keymap) for the five `tc:`
  tasks.
- After changing the language server or extension, run
  `editor: restart language server` from the command palette
  (`Ctrl+Shift+R` in the default keymap), or reinstall the dev extension.
- Typography uses `Liberation Serif` for `.eng` prose and `.ZedMono` for the
  terminal. If the serif font is missing:

```bash
sudo apt install -y fonts-liberation
```

## 8. Common failure patterns

| Symptom | Likely cause |
|---|---|
| `Unsupported: imperative verb ...` | Open-class verb with no lowering; rephrase with a supported form or add a clean `Phrase:` signature |
| `unrecognized imperative ... did you mean ...` | Typo in a control verb; use the suggestion |
| `UNRESOLVED_ANAPHOR` | `it`/`its` has no antecedent; bind a named value first |
| `empty loop body` | Header followed by a blank line; put the body directly below |
| `type mismatch` | Check `whole number` vs `unsigned number` vs `decimal` |
| `unexpected trailing token` | Extra word after a complete directive; inspect with `tc trace` |

## 9. Before you finish

```bash
make fmt-check
make lint
make audit
make test
./target/debug/tc --selftest
```

If you ran a full debug build, finish with:

```bash
cargo clean
./editors/zed/install_zed_config.sh
strip --strip-unneeded target/release/tc target/release/tc-lsp target/release/r1-editor
```

Do not push. See `AGENTS.md` and `CONTRIBUTING.md` for the full contract.
