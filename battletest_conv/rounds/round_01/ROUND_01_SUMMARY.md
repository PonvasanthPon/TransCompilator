# Round 1 Summary — DIRTY, advance to Round 2

**Seed block:** `10001..13000` (strictly disjoint)
**Authored programs:** 128 / 128 passed (16 in each of Bands B1–B8)
**Fuzz/crash seeds:** 3,000 / 3,000 passed
- Engine 1 differential: 1,000 / 1,000
- Engine 2 paraphrase: 750 / 750
- Engine 3 prose injection: 750 / 750
- Engine 4 malformed/FFI: 500 / 500

## Bugs found and fixed

| Bug | Band | Root cause | Fix commit |
|---|---|---|---|
| BUG-CONV-001 | B5 | A `Put`-led multi-action chain was not split by the prose normalizer, so the new referent was never visible to later `it` clauses. | `ad35f4b` |
| BUG-CONV-002 | B3 | `Show`, `Print`, and `Display` resolved as Emit verbs but std/io and std/string exposed no matching overloads. | `a61e492` |
| BUG-CONV-003 | B1 | `Say "literal"` had no pointer/length lowering; the normalizer now emits the canonical bytes call. | `019bf9a` |

## System gates

- `make build`: PASS
- `make test`: **658 / 658 PASS** (655 baseline + 3 permanent regressions)
- `make lint`: PASS, 0 warnings
- `make fmt-check`: clean
- `make audit`: PASS
- `tc --audit` / `tc --selftest`: PASS
- `std_files_are_pure_version2_english`: PASS
- Static ELF importing all 8 `std/*.eng` modules: no dynamic section, 0 `INTERP`, exact parity
- `r1-editor --check std/alloc.eng`: exact ELF64 == interpreter parity
- `r1-editor-gui std/alloc.eng`: clean native launch, zero stderr

## Verdict

`DIRTY_ADVANCE_TO_NEXT_ROUND` — a single clean round is required, so Round 2 uses brand-new programs and seeds `13001..16000`.
