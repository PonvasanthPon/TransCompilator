# LeetCode Massive Battle Test — Prose Combinator Checkpoint

**Project:** `tc_r1` `0.1.0-alpha-preview`  
**Batch 1:** strict prose-only re-test complete  
**Surviving prose solutions:** **17 / 50**  
**Batch 1 honestly skipped:** **33 / 50**  
**Catalog indexed:** 4,124  
**Remaining algorithmic catalog:** 3,072 pending

Four general prose combinators were added:

1. unindented, period-terminated prose loops;
2. natural loop exit and skip (`then stop`, `stop immediately`, `skip to the next one`);
3. English index/min-max math (`one more than`, `the midpoint of`, `the larger of`, `the last position of`);
4. pure-English string building and slicing (`empty text`, `append ... to ...`, `the slice of ...`).

The strict gate was not weakened. No indentation, colons, block terminators,
symbolic operators, single-letter variables, or boolean flags are permitted in
a solved `.eng` file.

---

## 1. Scorecard

| Measure | Count |
|---|---:|
| Batch 1 inspected | **50** |
| Strict prose-only solved | **17** |
| Strict prose-only skipped | **33** |
| Global solved | **17** |
| Global skipped | **1,035** |
| Global pending | **3,072** |
| Total catalog | **4,124** |

### Solved Batch 1

`0001`, `0004`, `0007`, `0008`, `0011`, `0012`, `0013`, `0021`, `0026`,
`0027`, `0029`, `0033`, `0034`, `0035`, `0041`, `0042`, `0050`.

### Remaining Batch 1 skips

| Group | Count | Problems |
|---|---:|---|
| Linked-list pointer work | 5 | 2, 19, 23, 24, 25 |
| Recursive/backtracking helpers | 7 | 17, 22, 37, 39, 40, 46, 47 |
| String assembly/comparison still missing a clean prose form | 7 | 5, 6, 14, 30, 38, 43, 49 |
| Index/matrix/DP shapes still beyond prose-only lowering | 14 | 3, 9, 10, 15, 16, 18, 20, 28, 31, 32, 36, 44, 45, 48 |
| **Total** | **33** | |

Every skip has a specific `skip_reason` in
`benchmarks/leetcode/ledger.json`.

---

## 2. Newly Unlocked Solutions

### #0001 — Two Sum

```text
Start with the list of numbers 2, 7, 11, and 15.
Let target be 9.
For every first from 0 to the last position of numbers.
For every second from one more than first to the last position of numbers, if item first of numbers plus item second of numbers is target, print first, print second, then stop.
```

Output: `0 1`.

### #0012 — Integer to Roman

```text
Let value be 1994.
Let result be empty text.
Repeat while value is at least 1000, append "M" to result, set value to value minus 1000.
If value is at least 900, append "CM" to result, set value to value minus 900.
If value is at least 500, append "D" to result, set value to value minus 500.
If value is at least 400, append "CD" to result, set value to value minus 400.
Repeat while value is at least 100, append "C" to result, set value to value minus 100.
If value is at least 90, append "XC" to result, set value to value minus 90.
If value is at least 50, append "L" to result, set value to value minus 50.
If value is at least 40, append "XL" to result, set value to value minus 40.
Repeat while value is at least 10, append "X" to result, set value to value minus 10.
If value is at least 9, append "IX" to result, set value to value minus 9.
If value is at least 5, append "V" to result, set value to value minus 5.
If value is at least 4, append "IV" to result, set value to value minus 4.
Repeat while value is at least 1, append "I" to result, set value to value minus 1.
Print result.
```

Output: `MCMXCIV`.

### #0033 — Search in Rotated Sorted Array

```text
Start with the list of numbers 4, 5, 6, 7, 0, 1, and 2.
Let target be 0.
Let answer be -1.
For every position from 0 to the last position of numbers.
If item position of numbers is target, set answer to position, then stop.

Print answer.
```

Output: `4`.

---

## 3. Feature Notes

### Unindented prose loops

`For every …`, `While …`, `Repeat while …`, and `Until …` become paragraph
loop starters. Their bodies are all contiguous sentences until a blank line,
EOF, or a closer such as `At the end, …`.

### Stop and skip

- `then stop` / `stop immediately` / `stop the loop` lower to Core-7 `Break`.
- `skip to the next one` / `skip this one` lower to Core-7 `Continue`.
- Inline `If` chains can contain multiple comma-separated actions.

### English math

- `one less than <expr>`
- `one more than <expr>`
- `the midpoint of <a> and <b>`
- `the larger/greater of <a> and <b>`
- `the smaller/lesser of <a> and <b>`
- `the last position of <list>`
- `the first item of <list>`

### Strings

- `Let result be empty text.`
- `Append "IV" to result.`
- `Append word to result.`
- `Append character position of word to result.`
- `the slice of word from start to finish`
- `Print result`, `the length of result`, `character pos of result`.

---

## 4. Verification

| Gate | Result |
|---|---|
| `make mbt` | **17/17 PASS** |
| Strict prose-only gate | **17/17 PASS** |
| `tc interp` expected stdout | 17/17 |
| Static zero-libc ELF64 | 17/17 |
| `tc_wasm.wasm` | 17/17 |
| IR runtime-honesty | 17/17 |
| `tc-lsp` diagnostics | 0 |
| `make test` | **1,076 / 1,076 PASS** |
| `make fmt-check` | PASS |
| `make lint` | PASS, 0 warnings |
| `make audit` | PASS |
| `tc --audit` / `tc --selftest` | PASS |

---

## 5. Playground

- Drawer title: `Solved Problems (17)`.
- Index: `web/site/leetcode_problems.json`, about 10 KB.
- Top-left hamburger retained.
- Search, load, run, installability, and offline cache verified.

---

## 6. Remaining Limits

The 33 skips are real limits, not fallback syntax:

- pointers for linked lists;
- recursive helpers/backtracking;
- clean prose forms for string comparison/assembly beyond the current slice/append surface;
- matrix/index/DP layouts that need a higher-level prose abstraction than
  position arithmetic.

They remain skipped until a general prose feature makes them expressible
without disguised Python.
