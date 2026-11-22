# Round 4 Summary — DIRTY, advance to Round 5

**Seed block:** `19001..22000` (strictly disjoint)
**Authored programs:** 128 / 128 passed (16 per band)
**Fuzz/crash seeds:** 3,000 / 3,000 passed
- Engine 1: 1,000 / 1,000
- Engine 2: 750 / 750
- Engine 3: 750 / 750
- Engine 4: 500 / 500

## Bug found

| Bug | Band | Root cause | Fix commit |
|---|---|---|---|
| BUG-CONV-007 | B8 / crash | 150 nested parentheses exhausted the Rust stack while parsing expressions. The parser now rejects group depth 33+ with a located `expression nesting exceeds the Core-7 limit` error, verified on the smaller Rust test-thread stack. | `feead65` |

## System gates

- `make build`: PASS
- `make test`: **663 / 663 PASS**
- `make lint`: PASS, 0 warnings
- `make fmt-check`: clean
- `make audit`: PASS
- `tc --selftest`: PASS
- Static all-8-`std` ELF: no dynamic section, 0 `INTERP`, exact parity
- `r1-editor --check std/alloc.eng`: exact parity
- `r1-editor-gui std/alloc.eng`: clean native launch, zero stderr

## Verdict

`DIRTY_ADVANCE_TO_NEXT_ROUND` — Round 5 must be fully fresh and clean.
