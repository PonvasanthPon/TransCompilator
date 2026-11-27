# LeetCode Massive Battle Test — Strict Prose-Only Checkpoint

**Project:** `tc_r1` `0.1.0-alpha-preview`  
**Batch 1 status:** strict prose-only pruning complete  
**Surviving prose-only solutions:** **3**  
**Batch 1 honestly skipped:** **47**  
**Catalog indexed:** 4,124 problems  
**Remaining algorithmic catalog:** 3,072 pending

The previous Batch 1 still contained disguised Python: indented loops,
colon headers, symbolic operators, single-letter indices, and boolean flags.
That surface has been rejected by the strict gate. Only programs that read as
unindented, symbol-free human prose remain.

---

## 1. Surviving Prose-Only Solutions

### #0021 — Merge Two Sorted Lists

```text
Let first be a list of 1, 2, and 4.
Let second be a list of 1, 3, and 4.
Let combined be an empty list of whole numbers.
For each number in first, append number to combined.
For each number in second, append number to combined.
Sort combined.
For each number in combined, print number.
```

Output: `1 1 2 3 4 4`.

### #0026 — Remove Duplicates from Sorted Array

```text
Start with the list of numbers 0, 0, 1, 1, 1, 2, 2, 3, 3, and 4.
Let unique be an empty list of whole numbers.
For each number in the list, if unique does not contain the number, append number to unique.
Print the number of items in unique.
For each number in unique, print number.
```

Output: `5 0 1 2 3 4`.

### #0027 — Remove Element

```text
Start with the list of numbers 3, 2, 2, and 3.
Remove every 3 from the list.
Print the number of items in the list.
For each item in the list, print item.
```

Output: `2 2 2`.

---

## 2. Strict Prose-Only Gate

`check_pure_english` in `scripts/run_leetcode_mbt.py` now rejects, outside
quoted strings:

- any indented line;
- any line ending in `:`;
- explicit block terminators (`End.`, `End the loop.`, `End if.`);
- symbolic operators `+ * / % <= >= == != < >` and spaced minus;
- single-letter declarations, loop variables, and indices;
- boolean flag boilerplate (`Let found be false`, `is true`, etc.);
- `snake_case`, C-style calls, `address of`, casts, `Use`/`Import`, and
  top-level `Return 0.` from the earlier gate.

A file that compiles but reads like Python is no longer publishable.

---

## 3. Batch 1 Skip Breakdown

| Group | Count | Problems |
|---|---:|---|
| Index-loop / matrix / numeric scanning | 27 | 1, 3, 4, 7, 8, 9, 10, 11, 13, 15, 16, 18, 20, 28, 29, 31, 32, 33, 34, 35, 36, 41, 42, 44, 45, 48, 50 |
| No pure string assembly / slicing | 8 | 5, 6, 12, 14, 30, 38, 43, 49 |
| Recursive / backtracking helpers | 7 | 17, 22, 37, 39, 40, 46, 47 |
| Linked-list pointer mutation | 5 | 2, 19, 23, 24, 25 |
| **Batch 1 total** | **47** | |

Each entry carries a specific `skip_reason` in
`benchmarks/leetcode/ledger.json`.

Global state:

| Status | Count |
|---|---:|
| Solved | **3** |
| Skipped | **1,049** |
| Pending | **3,072** |
| Total catalog | **4,124** |

---

## 4. Verification

| Gate | Result |
|---|---|
| `make mbt` | **3/3 PASS** |
| Strict prose-only gate | **3/3 PASS** |
| `tc interp` expected stdout | 3/3 |
| Static zero-libc ELF64 | 3/3 |
| `tc_wasm.wasm` | 3/3 |
| IR runtime-honesty | 3/3 |
| `tc-lsp` diagnostics | 0 |
| `make test` | **1,066 / 1,066 PASS** |
| `make fmt-check` | PASS |
| `make lint` | PASS, 0 warnings |
| `make audit` | PASS |
| `tc --audit` / `tc --selftest` | PASS |

---

## 5. Playground

- Drawer title: `Solved Problems (3)`.
- Top-left hamburger remains.
- Index size: about 1.3 KB.
- Search, load, and run verified for the surviving programs.
- Offline cache and installability remain intact.

---

## 6. Remaining Work

The honest way to grow Batch 1 is to add general prose-level frames, not
Python-style index loops. Missing capabilities include:

- pure string slicing/assembly and substring wording;
- prose-safe recursive helpers and backtracking;
- pointer-free list-of-lists and matrix wording;
- natural position/membership predicates for more list algorithms.

Until those exist, the remaining 47 Batch 1 problems stay skipped rather than
being smuggled back in as disguised Python.
