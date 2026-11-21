# `tc_r1` English Language Specification

This document specifies the finite English surface language accepted by
`tc_r1` after Phase 3.6. It is descriptive, not aspirational: every rule below
is implemented by the offline Rust frontend and covered by `cargo test`.
The specification is intentionally hype-free. Unsupported directives fail
loudly; they are never silently reinterpreted.

---

## 1. Architecture and Core-7 Philosophy

`tc_r1` compiles English directly into a small typed Core-7 AST, verifies that
AST, lowers it to Core-7 IR, and emits a static zero-libc ELF64 executable for
Linux x86-64. The reference IR interpreter and the native backend are held to
exact stdout/stderr/exit-code parity.

**The Irreducibility Rule.** If a feature can be written cleanly in `tc`
itself without touching the compiler, it belongs in `std/`. If it cannot, it
stays in Core. Phase 3.6 adds no IR opcodes and no library services.

The seven irreducible Core-7 primitives are:

1. Lexical locals, assignment, and block scoping (`Let`, `Assign`, `Block`).
2. Conditional and loop control (`If`, `Loop`, `Break`, `Continue`).
3. Function calls, aggregate returns, and function pointers.
4. Address and place computation (`AddrOfVar`, `GlobalAddr`, `FieldAddr`,
   `IndexAddr`, `Load`, `Store`).
5. Fixed-size arrays and byte copies (`MemCopy`).
6. Arithmetic, bitwise, comparison, and short-circuit boolean operations.
7. Raw Linux syscalls, traps, and static ELF64 emission.

Phase 3.6 contributes pure frontend lowering only:

- a DRS-style discourse referent stack,
- a comma-aware boolean condition parser,
- global and local quantifier desugaring to existing `Loop`/`If` nodes,
- deterministic phrase specificity and paraphrase echo.

### The three anti-hardcoding bans

1. **No per-verb synonym lists** in `tc-syntax` or `tc-semantics`. Open-class
   verb meaning comes from the generated WordNet/VerbNet lexicon tables.
2. **No library hacks** in Rust. `List`, `String` heap operations, `Print`,
   `File`, `Map`, and `Sqrt` remain Phase 4 `std/` responsibilities.
3. **No prose noun blacklists.** Narrative omission is decided by grammar,
   tense, and computational evidence, not by a list of everyday nouns.

`crates/tc-syntax/` and `crates/tc-semantics/` contain no private absolute
paths, no registry dependencies, and no ML/network runtime.

---

## 2. The Four-Bucket Vocabulary Model

Every accepted word belongs to exactly one finite bucket.

### Bucket 1 — Structural control anchors

Reserved lowercase words that introduce Core-7 structure or declarations:

```text
import       struct      union       global      function
phrase       if          else        otherwise   unless
while        repeat      loop        for every   for each
break        continue    return      syscall     copy
memcopy      trap        define      public      private
create       make        end         stop
```

Structural anchors are closed. A sentence that uses a reserved anchor with
operands it does not accept is rejected as `UNSUPPORTED`, never silently
ignored. `create` and `make` are recognized through the generated VerbNet
`Create` predicate; `end` and `stop` are deliberate block closers.

### Bucket 2 — Closed-class English function words

279 generated closed-class words in `tc-lexicon`, grouped into eight
categories (determiners, pronouns, prepositions, conjunctions, auxiliaries,
discourse fillers, comparators, and quantifier words). These words drive
article stripping, prepositional roles, passive detection, negation, and
coordinator precedence.

### Bucket 3 — Open-class computational verbs

786 WordNet 2025 hypernym entries, 10,845 VerbNet 3.4 semantic-predicate
frames, and 103 irregular lemmas. There is no hand-written verb synonym table
in the Rust frontend. Verb resolution is deterministic and offline.

### Bucket 4 — User identifiers and literals

- Identifiers are case-insensitive and lowercased canonically.
- String and byte literals are case-sensitive and may contain escaped bytes.
- Numeric literals support signed and unsigned decimal integers, `0x` hex,
  `0b` binary, `0o` octal, underscores, and `f32`/`f64` decimals/exponents.
- Boolean literals are `true` and `false`; the typed null pointer literal is
  `null`.

---

## 3. Surface Construction Rules

### 3.1 Paragraphs, sentences, and blocks

- Sentences end with `.`, `;`, `!`, or `?`, or with a newline in flowing
  prose. The final period is optional.
- A newline-separated line may contain one sentence or several.
- Numbered (`1.`, `2)`), bulleted (`-`, `*`), and plain lines are equivalent.
- Discourse fillers are stripped before analysis: `First,`, `Next,`, `Then,`,
  `Finally,`, `After that,`, `Okay,`, `Please`, and similar generated fillers.
- A colon-terminated structural header opens an indented block:

  ```text
  If x > 5:
      Add 1 to x.
      Return x.
  ```

- An explicit `End.` or `Stop.` may close a block. It is otherwise a no-op.
- The unified lowerer applies to top-level statements, `If`/`Else`/`Unless`,
  `While`/`Repeat`/`Loop`/`For every` bodies, and function bodies. The nesting
  guard is 32 levels.

### 3.2 Case rules

Identifiers are case-insensitive: `Tea`, `tea`, and `TEA` denote the same
variable. String and byte literals preserve case exactly. Keywords are
case-insensitive.

### 3.3 Declarations

```text
Import <module> [as <alias>].
Define [public|private] struct <Name> [of <T> and <U>] [with <field> as <type> ...].
Define [public|private] union  <Name> [of <T>] [with <field> as <type> ...].
Define [public|private] global <name> as <type>.
Define [public|private] function <name> [of <T>] [taking <p> as <type> ...]
    [returning <type>]:
        Phrase: "<literal words and <params>>"
        <body>
```

Aggregate fields may be separated by `and` or commas. Function and struct
generic parameters use `of T and U`; type application in expressions uses
`Pair<i64, i64>` or the `Create a Pair of i64 and i64` declaration form.
A function's `Phrase:` parameters must name its actual parameters.

Local declarations:

```text
Define local <name> as <type>.
<name> is <TypeName>.
<name> is a <TypeName>.
Let <name> be a <TypeName>.
Create a <TypeName> [called|named] <name>.
```

Local arrays use `Define local <name> as array<T, N>.` and are zero
initialized.

---

## 4. Interpretation Rules

### Rule 1 — Three-way intent gate

Every sentence receives exactly one verdict:

| Verdict | Meaning |
|---|---|
| `EXEC` | Lower to Core-7 now. |
| `COMMENT_OMITTED` | Deliberate narrative, greeting, or filler; no semantic effect. |
| `UNSUPPORTED` | Computational-looking directive outside the finite subset; loud error. |

There is no silent drop. `UNSUPPORTED` is a compile error and is also visible
in `tc trace`.

### Rule 2 — Expletive `It`

Omission is grammatical. No prose-noun list exists.

- `it` / `its <field>` in object, preposition, mutation, return, or condition
  position is always anaphoric. It resolves through the discourse stack or
  fails with `UNRESOLVED_ANAPHOR`.
- `It is false that ...` is scoped negation, not a pronoun.
- `It becomes <expr>`, `It = <expr>`, `It += <expr>`, `It goes up by <expr>`,
  and `It goes down by <expr>` mutate the most recent referent.
- `It is <pure expression>` assigns the expression when a referent is in
  scope and the entire right side parses as a Core-7 expression.
- Past-tense or modal `It was/were/had/seemed/looked/might/could/...` and
  `It is <prose with leftover words>` are `COMMENT_OMITTED` expletive prose.

Examples:

```text
Put 10 in x.
It was raining outside.      -- omitted
It is 3 pm.                  -- omitted
It is 10.                    -- x = 10
Add 5 to it.                 -- x = x + 5
```

### Rule 3 — DRS scope accessibility

The discourse stack mirrors lexical scopes exactly:

```text
ModuleTopLevel
Function
Block
IfCondition -> IfThen
IfElse
LoopBody
QuantifierBody
WhereClause
```

- A function sees only its own parameters and accessible outer/global names.
  Referents from sibling functions or top-level scripts never leak in.
- Referents introduced in a `then` branch are not visible in `else` or after
  the `If`.
- Referents introduced in a loop body are not visible after the loop.
- `For every <var>` registers `<var>` only inside its body.
- `where` bindings are visible only to their own statement. The statement's
  declared target is hoisted to the enclosing scope so later sentences can
  address it.
- Mutating an outer variable updates its recency without leaking a new
  binding.
- `COMMENT_OMITTED` sentences never introduce or touch referents.

### Rule 4 — Anaphora resolution order

1. `it` binds to the accessible referent with the highest recency stamp.
   Recency increases on introduction and on every mutation/transfer target.
2. `its <field>` binds to the most recent accessible aggregate whose declared
   struct/union type contains `<field>`. If the newest aggregate lacks the
   field, earlier accessible aggregates are scanned. No match is a loud
   `UNRESOLVED_ANAPHOR`.
3. `the <type>` binds to a variable literally named `<type>` if one exists;
   otherwise to the most recent accessible instance of struct/union `<type>`.
   If `<type>` is not a declared aggregate type, the article is stripped
   (`the salary` denotes `salary`).
4. `the <type> <name>` binds directly to `<name>` and verifies that its type
   is `<type>`. A mismatch is `ANAPHOR_TYPE_MISMATCH`.
5. Unresolved `it`, `its <field>`, or `the <type>` is always a compile error.
   The compiler never invents a zeroed phantom variable named `it`.

Right-hand sides are lowered before a new left-hand binding is introduced, so
`y is it + 5` binds `it` to the previous referent, not to `y`.

### Rule 5 — Coordinator precedence

From loosest to tightest:

```text
, or   <   , and   <   or   <   and   <   not   <   atom
```

Therefore:

```text
a == 1 or b == 1 and c == 1      ==  a == 1 or (b == 1 and c == 1)
a == 1 or b == 1, and c == 1     ==  (a == 1 or b == 1) and c == 1
a == 1 and b == 1, or c == 0     ==  (a == 1 and b == 1) or c == 0
```

A top-level comma followed by `and` or `or` stays inside the condition. Any
other top-level comma (or `then`) ends the condition and starts the body.
Commas inside parentheses are ignored for splitting.

The `and` inside `between <lo> and <hi>` is part of the atomic comparison and
is never treated as a coordinator.

Scoped negation follows ACE 6.7:

```text
it is false that A and B       ==  (!A) and B
it is false that A and that B  ==  !(A and B)
it is false that A or that B   ==  !(A or B)
```

Condition nesting is capped at 32 levels. Deeper `not` chains or parenthesis
nests return a clean diagnostic instead of overflowing the stack.

### Rule 6 — Quantifier semantics

Global statement quantifiers lower to Core-7 `Loop`:

```text
For every i from <start> to <end> [by <step>], <body>.
For each  i from <start> to <end> [by <step>], <body>.
For every v in <array>, <body>.
For each  v in <array>, <body>.
```

- Range bounds are inclusive.
- `start`, `end`, and `step` are evaluated once into hygienic locals before
  the loop.
- A constant step of `0` is a compile error; a dynamic step of `0` traps.
- Positive and negative constant or runtime steps are supported.
- The loop counter is advanced at the top of the loop after the first
  iteration, so `Continue` always advances and can never cause an infinite
  loop.
- `Break` exits immediately.
- Array quantifiers iterate exactly `0..N` where `N` is the statically known
  array length. Element type may be scalar, float, or aggregate.
- The bound variable is visible inside the body (including `it` and
  `its <field>`) and disappears afterwards.

Local boolean quantifiers are expressions and may appear in `If`, `Unless`,
`While`, assignments, and compound conditions:

```text
every x in <array> <predicate>
some  x in <array> <predicate>
no    x in <array> <predicate>
every k from <start> to <end> <predicate>
some  k from <start> to <end> <predicate>
no    k from <start> to <end> <predicate>
```

Predicates may be implicit (`is more than 0`, `equals 42`, `> 0`) or explicit
(`satisfies x * x < 100`, `where x % 2 == 0`). They short-circuit: `every`
stops at the first false element, `some` at the first true element, and `no`
at the first true element. Empty domains give `every = true`, `some = false`,
`no = true`.

When a local quantifier appears on the right of `and` or `or`, its prelude is
wrapped in the corresponding short-circuit guard, so it does not execute when
the left operand already decides the result. A quantifier inside a `While`
condition is re-evaluated on every iteration.

### Rule 7 — Specificity and ditransitive precedence

- Three-role ditransitive transfer frames (`TransferMove`: theme + source +
  destination) are preferred over two-role frames.
- A trailing prepositional phrase is never silently discarded. If no frame
  accepts it, the sentence is a loud `UNSUPPORTED` diagnostic.
- `Phrase:` candidates are scored by `(literal_word_count, parameter_count)`.
  `"write <val> to file <path>"` beats `"write <val> to <dst>"`.
- Ties are resolved by type-directed overload matching. Exact ties on both
  specificity and argument types fail loudly as `AmbiguousPhrase` and never
  depend on module declaration order.
- Anaphors inside `Phrase:` arguments resolve before type-directed matching.

---

## 5. Paraphrase Verification Workflow

Two commands expose exactly what the compiler decided:

```bash
tc paraphrase program.tc [-I include_dir]
tc trace program.tc --paraphrase [-I include_dir]
```

`tc paraphrase` prints canonical resolved English:

```text
=== Paraphrase (module: main) ===
[L1] [OMITTED: past-tense or third-person narrative prose] Yesterday Ravi called me twice
[L2] [OMITTED: expletive prose without computational directive] It was raining outside
[L4] Set cash to 500.
[L5] Add 100 to cash. [it -> cash]
[L6] If {(cash > 400 or cash == 0) and cash < 1000}, then:
[L8] Set sum to 0.
[L9] For every i from 1 to 5 (inclusive, step 1):
[L10] Return cash + sum.
```

Rules:

- `[OMITTED: <reason>]` means the sentence had no semantic effect.
- `[UNSUPPORTED: <reason>]` means compilation failed loudly.
- `[it -> cash]`, `[its x -> p.x]`, `[the point -> origin]`, and
  `[the point p -> p]` are the exact discourse bindings used during lowering.
- `{...}` shows condition grouping; nested grouping uses parentheses.
- `For every ... (inclusive, step N)` shows the exact evaluated bounds shape.
- `[phrase -> module::function]` shows the selected declarative phrase target.
- The paraphraser never re-parses source text. It records decisions made
  during lowering, so it cannot disagree with the emitted program.

`tc trace` preserves its existing provenance and IR output. With
`--paraphrase`, it appends a `=== Paraphrase ===` section.

---

## 6. Known Boundaries

The authoritative catalog is [`CORE_LIMITS.md`](CORE_LIMITS.md).

As of Phase 3.6:

- **Supported:** intra-scope and cross-sentence anaphora for variables,
  struct/union fields, definite aggregate types, and appositive names
  (`it`, `its <field>`, `the <type>`, `the <type> <name>`).
- **Out of scope by design:** human gendered anaphora (`he`, `she`, `they`),
  cataphora (forward references), and arbitrary world-knowledge bridging.
- **Out of scope by design:** threads, windows, files, printing, allocation,
  collections, and math libraries. These either fail loudly as `UNSUPPORTED`
  or become declarative `Phrase:` calls whose bodies are written in Phase 4
  `std/` English.
- Reserved control keywords cannot be redefined as VerbNet verbs.
- Recursive aggregate values have no finite System V layout and are rejected.
- Expression complexity is capped at 1024 parser charges.
- Type nesting is capped at 64 levels.
- Condition nesting is capped at 32 levels.
- Upstream Link Grammar has practical sentence-length limits; sentences
  without a linkage are omitted deterministically as prose, never crashed on.
