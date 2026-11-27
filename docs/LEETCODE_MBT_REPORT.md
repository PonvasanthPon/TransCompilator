# LeetCode Massive Battle Test — Report

**Project:** `tc_r1` `0.1.0-alpha-preview`  
**Campaign state:** checkpoint 1 complete — problems **1–50 / 50 solved**  
**Catalog:** 4,124 public LeetCode problems indexed from metadata only  
**Last full MBT run:** 50/50 solved programs passed IR honesty, static ELF64 parity, WebAssembly parity, and zero `tc-lsp` diagnostics.

This report is deliberately split into **completed work** and **remaining work**.
The catalog has been fully scanned and classified, but the 4,124-problem program
is not finished: 3,072 algorithmic problems remain pending beyond checkpoint 1.

---

## 1. Scorecard

### 1.1 Catalog scan

| Measure | Count |
|---|---:|
| Public catalog entries indexed | **4,124** |
| Metadata fields | number, slug, title, difficulty, paid flag, topic tags |
| Problem statements copied | **0** |
| Batch 1 range | **1–50** |
| Batch 1 solved | **50 / 50** |
| Batch 1 skipped | **0** |
| Batch 1 pending | **0** |

### 1.2 Global state

| Status | Count |
|---|---:|
| Solved and fully verified | **50** |
| Skipped by published track/structure constraints | **1,002** |
| Pending algorithmic problems | **3,072** |
| Total | **4,124** |

### 1.3 Skip breakdown

| Reason | Count |
|---|---:|
| Paywalled / exact public spec unavailable | 792 |
| SQL / database track | 94 |
| OOP / class-design problem | 101 |
| Shell track | 4 |
| Concurrency / threads | 6 |
| Interactive-judge protocol | 5 |
| **Total skipped** | **1,002** |

### 1.4 Batch 1 difficulty and category mix

| Difficulty | Solved |
|---|---:|
| Easy | 10 |
| Medium | 30 |
| Hard | 10 |
| **Total** | **50** |

Primary categories: Array 24, String 16, Math 5, Linked List 3,
Two Pointers 1, Heap / Priority Queue 1.

### 1.5 Execution profile

| Measure | Value |
|---|---:|
| Total verified IR steps | 8,464,602 |
| Smallest solved program | 497 steps (#9 Palindrome Number) |
| Largest solved program | 7,820,303 steps (#37 Sudoku Solver) |
| Programs with zero LSP diagnostics | 50 / 50 |
| Static ELF64 stdout parity | 50 / 50 |
| WebAssembly stdout parity | 50 / 50 |
| Runtime-honesty failures | 0 |

---

## 2. Solved Batch 1

| # | Problem | Difficulty | Category |
|---:|---|---|---|
| 1 | Two Sum | Easy | Array |
| 2 | Add Two Numbers | Medium | Math / Linked List |
| 3 | Longest Substring Without Repeating Characters | Medium | String |
| 4 | Median of Two Sorted Arrays | Hard | Array |
| 5 | Longest Palindromic Substring | Medium | String |
| 6 | Zigzag Conversion | Medium | String |
| 7 | Reverse Integer | Medium | Math |
| 8 | String to Integer (atoi) | Medium | String |
| 9 | Palindrome Number | Easy | Math |
| 10 | Regular Expression Matching | Hard | Dynamic Programming / String |
| 11 | Container With Most Water | Medium | Array / Two Pointers |
| 12 | Integer to Roman | Medium | Math / String |
| 13 | Roman to Integer | Easy | String / Math |
| 14 | Longest Common Prefix | Easy | String |
| 15 | 3Sum | Medium | Array / Two Pointers |
| 16 | 3Sum Closest | Medium | Array / Two Pointers |
| 17 | Letter Combinations of a Phone Number | Medium | String / Backtracking |
| 18 | 4Sum | Medium | Array / Two Pointers |
| 19 | Remove Nth Node From End of List | Medium | Linked List |
| 20 | Valid Parentheses | Easy | Stack / String |
| 21 | Merge Two Sorted Lists | Easy | Linked List |
| 22 | Generate Parentheses | Medium | String / Backtracking |
| 23 | Merge k Sorted Lists | Hard | Linked List / Heap |
| 24 | Swap Nodes in Pairs | Medium | Linked List |
| 25 | Reverse Nodes in k-Group | Hard | Linked List |
| 26 | Remove Duplicates from Sorted Array | Easy | Array / Two Pointers |
| 27 | Remove Element | Easy | Array / Two Pointers |
| 28 | Find the Index of the First Occurrence in a String | Easy | String |
| 29 | Divide Two Integers | Medium | Math / Bit Manipulation |
| 30 | Substring with Concatenation of All Words | Hard | String / Sliding Window |
| 31 | Next Permutation | Medium | Array |
| 32 | Longest Valid Parentheses | Hard | String / Stack |
| 33 | Search in Rotated Sorted Array | Medium | Array / Binary Search |
| 34 | Find First and Last Position of Element in Sorted Array | Medium | Array / Binary Search |
| 35 | Search Insert Position | Easy | Array / Binary Search |
| 36 | Valid Sudoku | Medium | Array / Matrix |
| 37 | Sudoku Solver | Hard | Matrix / Backtracking |
| 38 | Count and Say | Medium | String |
| 39 | Combination Sum | Medium | Backtracking |
| 40 | Combination Sum II | Medium | Backtracking |
| 41 | First Missing Positive | Hard | Array |
| 42 | Trapping Rain Water | Hard | Array / Two Pointers |
| 43 | Multiply Strings | Medium | String / Math |
| 44 | Wildcard Matching | Hard | Dynamic Programming / String |
| 45 | Jump Game II | Medium | Array / Greedy |
| 46 | Permutations | Medium | Backtracking |
| 47 | Permutations II | Medium | Backtracking |
| 48 | Rotate Image | Medium | Matrix |
| 49 | Group Anagrams | Medium | String / Hash Table |
| 50 | Pow(x, n) | Medium | Math / Recursion |

Each program is a self-contained, multi-line natural-English source with a
concrete input and recorded expected output. None of the programs contains a
precomputed answer; every solution lowers to runtime Core-7 control flow.

---

## 3. Compiler Bugs Found and Fixed

### 3.1 `Repeat while …:` rejected an indented body

- **Reproducer:** a colon-headed `Repeat while cond:` with an indented body
  compiled as an unsupported flat `repeat` statement.
- **Root cause:** colon-headed blocks bypassed the flat-prose normalizer that
  rewrites `Repeat while` to `While`; the semantic `repeat` branch then tried
  to parse the condition as a repeat count.
- **Fix:** `lower_core` canonicalizes `Repeat while` / `Repeat until` headers
  to `While` / `While not` before dispatch.
- **Regressions:** `b3-repeat-while-indented` and
  `b3-repeat-until-indented` in `human_english_battletest.rs`.

### 3.2 WebAssembly std augmentation ignored explicit imports

- **Reproducer:** `Use io, list, and string.` failed in wasm with
  `imported module string not found`, while the same source compiled natively.
- **Root cause:** the wasm std augmenter chose embedded modules only from
  narrow prose heuristics; explicit `Use` declarations were not authoritative.
- **Fix:** all explicit standard-module imports now enter the embedded-module
  queue before the heuristics.
- **Regression:** covered by the 50-program wasm parity gate, which includes
  explicit `Use` lines for every program.
- **Additional fix:** the MBT verifier now counts runtime control flow across
  all user-module functions, not only `main`, so helper-carried loops are
  recognized without letting std loops mask constant answers.

---

## 4. Minor Upgrades

No `std/*.eng` signature or helper was added or changed for Batch 1. Every
solution uses the existing eight pure-English modules directly. No Core-7
opcode, backend feature, external crate, class system, or compile-time
evaluator was added.

---

## 5. Honest Limits Discovered

These are parser/typing boundaries observed while writing natural English.
They were worked around compositionally; none required a puzzle-specific rule.

1. **Multi-word identifiers containing direction/count words.** Names such as
   `left count`, `best start`, or `head count` can be misread in expressions.
   The solutions use underscore names (`first_count`, `best_low`, `head_count`).
   This is a lexical ambiguity worth a future general fix, not a per-problem
   special case.
2. **Nested phrase calls as arguments.** `Say count of nums` or passing
   `list_get(...)` directly into another phrase can fail to resolve; binding to
   a local first is accepted and is already documented as `LIM-P5-002`.
3. **Record field access after a user function return.** A value produced by a
   user function that returns a record can lose field-type information at
   `value.field`; the Sudoku solver avoids this by passing record pointers and
   using buffers where needed.
4. **Overload selection for `Say` with inferred list elements.** A `TcString`
   obtained by `list_get` may select the numeric `Say` overload; declaring a
   local `TcString` or using a typed helper resolves it. Group Anagrams uses
   the typed-local form.
5. **Unsigned range underflow.** `for i from 0 to n - 1` with `n = 0`
   underflows. Longest Valid Parentheses explicitly guards the empty string.
   This is standard unsigned arithmetic, documented in the solutions.

---

## 6. Major Limits (Skipped, Not Hidden)

The catalog classifier skips only published structural boundaries:

- LeetCode Premium / paywalled problems whose exact public specification is
  unavailable.
- SQL/database, shell, concurrency, and interactive-judge tracks, which are
  outside the standalone Core-7 execution surface.
- OOP/class-design problems, because the campaign forbids classes,
  inheritance, methods, and virtual dispatch.

The remaining 3,072 algorithmic problems are **pending**, not silently marked
solved or skipped.

---

## 7. Verification Gates

| Gate | Result |
|---|---|
| `make mbt` | **50/50 PASS** |
| `tc interp` vs expected stdout | 50/50 |
| static zero-libc ELF64 (`tc run`) vs expected | 50/50 |
| `tc_wasm.wasm` vs expected | 50/50 |
| IR runtime-honesty | 50/50 |
| `tc-lsp` diagnostics | 0 |
| `make fmt-check` | PASS |
| `make lint` | PASS, 0 warnings |
| `make audit` | PASS |
| `make test` | **1,062 / 1,062 PASS** (includes manifest + 25-program stratified sample) |
| `tc --audit` | PASS |
| `tc --selftest` | PASS |

---

## 8. Playground Integration

- `web/site/leetcode_problems.json` contains all 50 solved programs with
  original English source, difficulty, category, and a bounded per-problem
  step budget. Size: ~90 KB.
- The Problems drawer is closed by default, lazy-loads the index on first open,
  and uses a virtualized row list, instant search, category filter, and
  difficulty pills.
- Selecting a problem loads its source into the serif editor, updates the
  caption (`#0001 · Two Sum — Array · Easy`), refreshes semantic tokens, and
  readies Run.
- `sw.js` cache version is `tc-playground-v2` and precaches the index for
  offline browsing.
- Verified in Chromium at 1440×900 and 375×812: no horizontal overflow, no
  console errors, installability errors `[]`, offline reload and execution
  pass. The heaviest program (Sudoku) executes in 1.53 s / 7,820,303 steps.

---

## 9. Remaining Campaign Work

1. Continue checkpointed batches across IDs 51–4124.
2. Keep every solved program on the same four-engine gate.
3. Fix general parser/type-inference limits when they block natural phrasing,
   with focused regressions.
4. Re-generate the web index and run the PWA drawer checks at each milestone.
