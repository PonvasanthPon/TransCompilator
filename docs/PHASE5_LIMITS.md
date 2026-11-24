# Phase 5 Limits Catalog

- **LIM-P5-001** (SEMANTICS): A flat paragraph For every loop without an inline body or indented block is rejected, not silently dropped. Probe: `battletest_p5/limits/limit_flat_for.eng`. Observed: clean Lowering error.
- **LIM-P5-002** (SEMANTICS): A phrase expression used directly as the argument of another phrase (Say count of ...) is not recursively phrase-resolved; bind it to a Let first. Probe: `battletest_p5/limits/limit_nested_phrase_argument.eng`. Observed: clean Unsupported/field error.
- **LIM-P5-003** (LEXICON): Very long fixed English sentences do not panic Link Grammar; they fall back to a clean Unsupported verdict. Probe: `battletest_p5/limits/limit_long_sentence.eng`. Observed: clean Unsupported error.
- **LIM-P5-004** (EDITOR): The editor parity runner has a 10,000,000-step reference-interpreter budget; the CLI uses 50,000,000. Programs exceeding it return a structured result, not a panic. Probe: `crates/tc-editor/src/runner.rs::run_elf_with_parity`. Observed: INTERPRETER FAILED result.
- **LIM-P5-005** (STDLIB): StrMap stores TcString pointers supplied by the caller; keys must stay alive until the map is freed. IntMap and StrMap now grow beyond 256 entries. Probe: `std/map.eng`. Observed: documented ownership contract.
