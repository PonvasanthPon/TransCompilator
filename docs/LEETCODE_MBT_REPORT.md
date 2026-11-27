# LeetCode Massive Battle Test — Pure-English Checkpoint

**Project:** `tc_r1` `0.1.0-alpha-preview`  
**Campaign state:** Batch 1 rewritten under the hard Pure-English Gate  
**Solved:** **30** purely natural-English programs  
**Honestly skipped in Batch 1:** **20**  
**Catalog indexed:** 4,124 problems  
**Remaining algorithmic catalog:** 3,072 pending

The previous Batch 1 contained disguised C (`list_get`, `address of`,
`as an unsigned number`, `write_index`, `Return 0.`). It has been replaced.
Every published solved program now passes an automated purity gate and the
native/wasm/IR/LSP parity gates.

---

## 1. Pure-English Scorecard

| Measure | Count |
|---|---:|
| Problems inspected in Batch 1 | **50** |
| Published pure-English solutions | **30** |
| Batch 1 honestly skipped | **20** |
| Global catalog entries | **4,124** |
| Global solved | **30** |
| Global skipped | **1,022** |
| Global pending algorithmic | **3,072** |

### 1.1 Published solutions

`0001`, `0003`, `0004`, `0007`, `0008`, `0009`, `0010`, `0011`, `0013`,
`0015`, `0016`, `0018`, `0020`, `0021`, `0026`, `0027`, `0028`, `0029`,
`0031`, `0032`, `0033`, `0034`, `0035`, `0036`, `0041`, `0042`, `0044`,
`0045`, `0048`, `0050`.

### 1.2 Batch 1 skips

| # | Problem | Reason |
|---:|---|---|
| 2 | Add Two Numbers | Linked-list node allocation and pointer traversal |
| 5 | Longest Palindromic Substring | No pure substring-construction surface |
| 6 | Zigzag Conversion | No pure string-assembly surface |
| 12 | Integer to Roman | No pure string-assembly surface |
| 14 | Longest Common Prefix | No pure prefix-string construction |
| 17 | Letter Combinations | Recursive helper calls forbidden by the gate |
| 19 | Remove Nth Node | Linked-list pointer deletion |
| 22 | Generate Parentheses | Recursive helper calls and shared buffer |
| 23 | Merge k Sorted Lists | Linked-list node/pointer merge |
| 24 | Swap Nodes in Pairs | Pointer rewiring |
| 25 | Reverse Nodes in k-Group | Pointer rewiring |
| 30 | Substring with Concatenation | String helper/pointer slicing |
| 37 | Sudoku Solver | Recursive backtracking helper calls |
| 38 | Count and Say | No pure run-length string assembly |
| 39 | Combination Sum | Recursive enumeration |
| 40 | Combination Sum II | Recursive enumeration |
| 43 | Multiply Strings | No pure digit-to-string assembly |
| 46 | Permutations | Recursive enumeration |
| 47 | Permutations II | Recursive unique enumeration |
| 49 | Group Anagrams | String helper comparison/typed-local workaround |

These are recorded in `benchmarks/leetcode/ledger.json` with explicit
`skip_reason` values. They are not silently hidden.

---

## 2. The Hard Pure-English Gate

`check_pure_english(path, text)` in `scripts/run_leetcode_mbt.py` fails a
solved program when it finds, outside quoted strings:

- any `snake_case` identifier;
- any C-style call such as `foo(` (including `list_get(`, `str_*`, `map_*`);
- `address of`;
- `as an unsigned number`, `as a whole number`, `as a pointer`, or `as a byte`;
- manual `Use` / `Import` std boilerplate;
- a manual top-level `Return 0.`.

The gate runs before compilation and is a hard part of `make mbt`.

### Before and after

**Before — #0027 Remove Element**

```text
Use io and list.
Let nums be a list of 3, 2, 2, and 3.
Let write_index start at 0 as an unsigned number.
For every read_index from 0 as an unsigned number to count of nums - 1:
    Let current be list_get(address of nums, read_index).
    If current isn't equal to 3, list_set(address of nums, write_index, current), add 1 to write_index.
Say write_index.
list_free(address of nums).
Return 0.
```

**After — #0027 Remove Element**

```text
Start with the list of numbers 3, 2, 2, and 3.
Remove every 3 from the list.
Print the number of items in the list.
For each item in the list, print item.
```

**Before — #0001 Two Sum**

```text
Use io, list, and string.
Let found be false.
For every i from 0 as an unsigned number to n - 1:
    For every j from i + 1 to n - 1:
        If list_get(address of nums, i) + list_get(address of nums, j) equals target:
            Set answer left to i as a whole number.
```

**After — #0001 Two Sum**

```text
Start with the list of numbers 2, 7, 11, and 15.
Let target be 9.
Let found be false.
For every i from 0 to n - 1:
    For every j from i + 1 to n - 1:
        If found is false and item i of numbers + item j of numbers is target:
            Set answer first to i.
            Set answer second to j.
            Set found to true.
Print answer first.
Print answer second.
```

**Before — #0033 Search in Rotated Sorted Array**

```text
Define function search_rotated taking nums as a pointer to List of whole number ...
    Repeat while low is at most high:
        Let value be list_get(nums, middle as an unsigned number).
```

**After — #0033 Search in Rotated Sorted Array**

```text
Let nums be a list of 4, 5, 6, 7, 0, 1, and 2.
Let target be 0.
Repeat while low <= high:
    Let middle be low + (high - low) / 2.
    Let value be item middle of nums.
```

---

## 3. Natural-English Surface Added

### Lists

- `item <i> of <list>` and `the item at <i> in <list>` reads.
- `Set item <i> of <list> to <value>` and `Replace item <i> of <list> with <value>`.
- `Append <value> to <list>` and `Add <value> to the end of <list>`.
- `Remove the last item from <list>`.
- `Remove item <i> from <list>`.
- `Remove every <value> from <list>`.
- `Sort <list>` for integer lists.

Indices accept ordinary `i64` loop variables and `u64` values. The compiler
promotes/normalizes indices where necessary; user programs do not write casts.

### Strings

- `character <i> of <text>` and `byte <i> of <text>` return the byte/char code.
- Backed by the same runtime byte representation, with no `str_*` calls or
  `address of` in user source.

### Multi-word names and loops

- `Let write index start at 0.`, `Let max profit start at 0.`, and similar
  names are canonicalized consistently in declarations, assignments, loops,
  and conditions.
- `For every zone row from 0 to 2:` now supports multi-word bound variables.
- Range bounds unify signed and unsigned integers:
  `For every i from 0 to n - 1` works when `n` is a `u64` count.
- Comparisons unify signed/unsigned integer operands without user casts.

### Zero boilerplate

- Ordinary scripts need no `Use io`, `Use list`, or `Use string`; the compiler
  auto-adds std modules only when their surface is actually used.
- Ordinary scripts need no final `Return 0.`.
- No manual `list_free` is required in casual user programs.

---

## 4. Compiler Bugs Fixed

1. **Indented `Repeat while` bodies** were rejected; colon-headed `Repeat while`
   now lowers like `While`, with regressions.
2. **Wasm explicit std imports** were ignored by the embedded-module
   augmenter; explicit `Use` declarations are authoritative.
3. **Range type unification** failed for `For every i from 0 to n - 1` with a
   `u64` count. Mixed signed/unsigned range bounds now unify.
4. **Comparison type unification** failed for `i < count` and similar natural
   conditions; comparisons now coerce mixed integer operands.
5. **Multi-word loop binders** such as `zone row` were truncated by the
   quantifier parser; the full binder is now captured and canonicalized.
6. **Natural list operation classification** now routes `Sort`, `Append`,
   `Remove every`, and related directives through executable lowering instead
   of rejecting them as unsupported verbs.
7. **String character indexing** accepts variable indices without C-style
   calls or pointer syntax.

No Core-7 opcode was added. No external crate was added. No OOP construct or
whole-program compile-time evaluator was introduced.

---

## 5. Verification

| Gate | Result |
|---|---|
| `make mbt` | **30/30 PASS** |
| Pure-English gate | **30/30 PASS** |
| `tc interp` expected stdout | 30/30 |
| Static zero-libc ELF64 | 30/30 |
| `tc_wasm.wasm` | 30/30 |
| IR runtime-honesty | 30/30 |
| `tc-lsp` diagnostics | 0 |
| `make test` | **1,066 / 1,066 PASS** |
| `make fmt-check` | PASS |
| `make lint` | PASS, 0 warnings |
| `make audit` | PASS |
| `tc --audit` / `tc --selftest` | PASS |

---

## 6. Playground

- Top-left 3-line hamburger opens the solved-problem drawer.
- Drawer title is `Solved Problems (30)`.
- Right toolbar no longer contains a Problems text button.
- 30 original English sources, lazy-loaded from `leetcode_problems.json`
  (~27 KB), with virtualized search/filter rows.
- Service-worker cache is `tc-playground-v2`; the index is offline-cached.
- Desktop and mobile layouts have no horizontal overflow.

---

## 7. Remaining Work

1. Continue checkpointed batches beyond problem 50.
2. Add pure-English string construction/slicing and safe recursive phrase
   helpers to recover some of the 20 honest skips.
3. Keep every solved program behind the purity, IR, ELF64, wasm, and LSP gates.
