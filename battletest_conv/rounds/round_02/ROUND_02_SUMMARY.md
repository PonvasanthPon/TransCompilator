# Round 2 Summary — DIRTY, advance to Round 3

**Seed block:** `13001..16000` (strictly disjoint)
**Authored programs:** 128 / 128 passed (16 per band)
**Fuzz/crash seeds:** 3,000 / 3,000 passed
- Engine 1: 1,000 / 1,000
- Engine 2: 750 / 750
- Engine 3: 750 / 750
- Engine 4: 500 / 500

## Bug found

| Bug | Band | Root cause | Fix commit |
|---|---|---|---|
| BUG-CONV-004 | B5 / B7 | `Set` was missing from multi-action chain splitting. Adding it exposed a second defect: `starts_action` treated a leading parenthesized expression as a fresh action, breaking parenthesized `Set` expressions in `std/map`. Both were fixed together. | `5951466` |

## System gates

- `make build`: PASS
- `make test`: **660 / 660 PASS**
- `make lint`: PASS, 0 warnings
- `make fmt-check`: clean
- `make audit`: PASS
- `tc --selftest`: PASS
- Static all-8-`std` ELF: no dynamic section, 0 `INTERP`, exact parity
- `r1-editor --check std/alloc.eng`: exact parity
- `r1-editor-gui std/alloc.eng`: clean native launch, zero stderr

## Verdict

`DIRTY_ADVANCE_TO_NEXT_ROUND` — Round 3 must be fully fresh and clean.
