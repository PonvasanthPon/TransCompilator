# Round 3 Summary — DIRTY, advance to Round 4

**Seed block:** `16001..19000` (strictly disjoint)
**Authored programs:** 128 / 128 passed (16 per band)
**Fuzz/crash seeds:** 3,000 / 3,000 passed on the final code
- Engine 1: 1,000 / 1,000
- Engine 2: 750 / 750
- Engine 3: 750 / 750
- Engine 4: 500 / 500

## Bugs found

| Bug | Band | Root cause | Fix commit |
|---|---|---|---|
| BUG-CONV-005 | B5 | Comma-aware conditions were split at the first comma by the prose normalizer, so `If A, and B, <body>` lost its body or reported a trailing token. | `a35d01b` |
| BUG-CONV-006 | B5 | Transfer role detection scanned prepositions inside parenthesized operands, so `Remove (cash divided by 4) from cash` selected the inner `by`. | `a35d01b` |

The fix also preserves clean errors for empty `Until`/`While`/`If` conditions; the full 662-test baseline caught that regression before the round closed.

## System gates

- `make build`: PASS
- `make test`: **662 / 662 PASS**
- `make lint`: PASS, 0 warnings
- `make fmt-check`: clean
- `make audit`: PASS
- `tc --selftest`: PASS
- Static all-8-`std` ELF: no dynamic section, 0 `INTERP`, exact parity
- `r1-editor --check std/alloc.eng`: exact parity
- `r1-editor-gui std/alloc.eng`: clean native launch, zero stderr

## Verdict

`DIRTY_ADVANCE_TO_NEXT_ROUND` — Round 4 must be fully fresh and clean.
