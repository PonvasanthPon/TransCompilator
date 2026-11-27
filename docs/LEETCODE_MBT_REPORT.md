# LeetCode Massive Battle Test — Batches 1–2

**Project:** `tc_r1` `0.1.0-alpha-preview`  
**Batch 1 (`1–50`):** 20 solved, 30 skipped  
**Batch 2 (`51–100`):** 18 solved, 32 skipped  
**Global:** 38 solved, 1,064 skipped, 3,022 pending of 4,124 catalog entries.

The strict prose-only gate is unchanged. Every solved program is unindented,
colon-free, symbolic-operator-free human prose.

---

## 1. General primitives added

### Text and character comparisons

- Text equality/inequality on `TcString`, `Ptr(U8)`, slices, and literals.
- One-character literals promote to byte codes when compared with
  `character position of text`.
- Supports `is`, `isn't equal to`, and `is not equal to`.

### Repeated-value lists

- `a list of <count> zeros`
- `a list of <count> copies of <value>`
- Runtime-initialized `List<i64>` with a real lowering loop.

### List element swap

- `Swap item <pos> of <list> and item <pos> of <list>.`
- Synthesized runtime `list_get`/`list_set` sequence; no pointer syntax.

### Prior prose combinators retained

- Unindented paragraph loops and closers.
- `then stop` / `skip to the next one`.
- `one more than`, `one less than`, `the midpoint of`, `the larger/smaller of`,
  `the last position of`.
- `empty text`, append text/byte, text slices.

---

## 2. Batch 1 scorecard

Solved: **20 / 50**

`0001`, `0003`, `0004`, `0005`, `0007`, `0008`, `0011`, `0012`, `0013`,
`0021`, `0026`, `0027`, `0028`, `0029`, `0033`, `0034`, `0035`, `0041`,
`0042`, `0050`.

Newly recovered from the previous 17: `0003`, `0005`, `0028`.

Skipped: **30** (`2, 6, 9, 10, 14, 15, 16, 17, 18, 19, 20, 22, 23, 24, 25,
30, 31, 32, 36, 37, 38, 39, 40, 43, 44, 45, 46, 47, 48, 49`).

---

## 3. Batch 2 scorecard

Solved: **18 / 50**

`0053`, `0055`, `0058`, `0062`, `0063`, `0064`, `0066`, `0067`, `0069`,
`0070`, `0074`, `0075`, `0080`, `0081`, `0088`, `0089`, `0091`, `0096`.

Skipped: **32**

| Group | Count | Problems |
|---|---:|---|
| Recursive/backtracking | 7 | 51, 52, 77, 78, 79, 90, 93 |
| Linked-list pointers | 5 | 61, 82, 83, 86, 92 |
| Binary-tree node harnesses | 5 | 94, 95, 98, 99, 100 |
| Complex matrix/interval/string/DP patterns beyond current prose frames | 15 | 54, 56, 57, 59, 60, 65, 68, 71, 72, 73, 76, 84, 85, 87, 97 |

Every entry has a specific `skip_reason` in
`benchmarks/leetcode/ledger.json`.

---

## 4. Complete newly solved examples

### #0005 — Longest Palindromic Substring

```text
Let word be "babad".
Let last be the last position of word.
Let best be empty text.
For every start from 0 to last.
For every finish from start to last.
Let piece be the slice of word from start to one more than finish.
Let reversed be empty text.
For every step from 0 to finish minus start, let position be finish minus step, append character position of word to reversed.
Let piece length be the length of piece.
Let best length be the length of best.
If piece is reversed, if piece length is greater than best length, set best to piece.

Print best.
```

Output: `bab`.

### #0053 — Maximum Subarray

```text
Let nums be a list of -2, 1, -3, 4, -1, 2, 1, -5, and 4.
Let best be item 0 of nums.
Let current be 0.
For every position from 0 to the last position of nums.
Set current to the larger of current plus item position of nums and item position of nums.
Set best to the larger of best and current.

Print best.
```

Output: `6`.

### #0070 — Climbing Stairs

```text
Let target be 5.
Let size be target plus 1.
Let ways be a list of size zeros.
Set item 0 of ways to 1.
For every position from 1 to target.
If position is 1, set item position of ways to 1.
If position is more than 1, set item position of ways to item (position minus 1) of ways plus item (position minus 2) of ways.

Print item target of ways.
```

Output: `8`.

---

## 5. Verification

| Gate | Result |
|---|---|
| `make mbt` | **38/38 PASS** |
| Strict prose-only gate | **38/38 PASS** |
| `tc interp` expected stdout | 38/38 |
| Static zero-libc ELF64 | 38/38 |
| `tc_wasm.wasm` | 38/38 |
| IR runtime-honesty | 38/38 |
| `tc-lsp` diagnostics | 0 |
| `make test` | **1,080 / 1,080 PASS** |
| `make fmt-check` | PASS |
| `make lint` | PASS, 0 warnings |
| `make audit` | PASS |
| `tc --audit` / `tc --selftest` | PASS |

---

## 6. Playground

- Drawer title: `Solved Problems (38)`.
- `web/site/leetcode_problems.json`: about 23 KB.
- Top-left hamburger retained.
- Search/load/run, installability, offline cache verified.

---

## 7. Remaining limits

The 62 skips across Batches 1–2 are dominated by genuine structural limits:
pointer-linked list/tree nodes, recursive backtracking, and complex
matrix/interval/2D-DP layouts that lack a clean prose-level frame. These remain
skipped rather than disguised as Python.
