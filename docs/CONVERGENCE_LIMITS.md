# tc_r1 v1.0 Convergence Limits Catalog

These are verified architectural boundaries from the five-round convergence
campaign. Each fails loudly with a structured diagnostic rather than panicking,
hanging, or silently dropping the instruction.

## LIM-CONV-001 — Flat `For every` body

A paragraph `For every` loop needs an inline body or an indented block.

```eng
Use io.
Let total be 0.
For every i from 1 to 3.
Add i to total.
End the loop.
Say total.
Give back 0.
```

Observed: located `For every requires an inline body or an indented block` error.

## LIM-CONV-002 — Nested phrase argument

A phrase expression used directly as another phrase argument is not recursively
phrase-resolved. Bind it to a `Let` first.

```eng
Use alloc, core, io, list.
Let nums be List of whole number.
Push 1 to address of nums.
Say count of address of nums.
Free list address of nums.
Give back 0.
```

Observed: clean TypeCheck `struct list has no field count` error.

## LIM-CONV-003 — Very long fixed sentences

Very long fixed English sentences fall back to a deterministic prose or
Unsupported verdict; they never crash Link Grammar.

```eng
Use io.
Say word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word.
Give back 0.
```

Observed: clean Unsupported diagnostic.

## LIM-CONV-004 — Editor interpreter budget

The editor parity runner uses a 10,000,000-step reference-interpreter budget.
Programs exceeding it return a structured `INTERPRETER FAILED` result rather
than a panic. Probe: `crates/tc-editor/src/runner.rs::run_elf_with_parity`.

## LIM-CONV-005 — Caller-owned `StrMap` keys

`StrMap` stores caller-owned `TcString` pointers. Keys must stay alive until
the map is freed. `IntMap` and `StrMap` now grow beyond 256 entries. Probe:
`std/map.eng`.

## LIM-CONV-006 — Verb/noun identifier collision

A bare identifier that is also an open-class VerbNet verb can be read as a
verb inside an arithmetic expression. It fails loudly; use `Let` or an
unambiguous identifier.

```eng
Use io.
Seats is 5.
Price is 43.
Total is seats times price.
Say total.
Give back 0.
```

Observed: clean Unsupported computational-verb diagnostic.

## LIM-CONV-007 — Expression nesting cap

Parenthesized expression nesting is capped at 32 groups. Depth 33 or greater
returns a located error before recursive parsing can exhaust the Rust stack,
including on the smaller Rust test-thread stack.

```eng
Use io.
Let x be (((((((((((((((((((((((((((((((((1))))))))))))))))))))))))))))))))).
Say x.
Give back 0.
```

Observed: `expression nesting exceeds the Core-7 limit`.

## Reproducing the permanent regressions

```bash
cargo test -p tc-semantics --test battletest_conv_regressions
```
