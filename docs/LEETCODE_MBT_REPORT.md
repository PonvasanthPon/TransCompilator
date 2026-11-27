# LeetCode Massive Battle Test — Batches 1–2

**Project:** `tc_r1` `0.1.0-alpha-preview`  
**Batch 1 (`1–50`):** 36 solved, 14 skipped  
**Batch 2 (`51–100`):** 30 solved, 20 skipped  
**Global:** 66 solved, 1,036 skipped, 3,022 pending of 4,124 catalog entries.

The strict prose-only gate is unchanged. Every solved program is unindented,
colon-free, symbolic-operator-free human prose. No Core-7 opcode, external
crate, OOP construct, or compile-time evaluator was added.

---

## 1. General primitives added

### Multi-action branches and `Otherwise if` chains

- Separate-line `Otherwise, if <condition>, <actions>.` attaches to the most
  recent `If` as a real nested `else { if ... }` chain.
- A final `Otherwise, <actions>.` closes the chain.
- Inline branches accept two or more imperative actions separated by commas.
- Word-prefix detection now accepts punctuation boundaries such as
  `Otherwise,` without weakening identifier matching.

### Natural map prose over `std/map.eng`

- `Let <name> be an empty map.` / `an empty number map.` → `IntMap`.
- `Set entry <key> of <map> to <value>.`
- `entry <key> of <map>` → stored value, or `0` when absent.
- `Add <n> to entry <key> of <map>.` and
  `Subtract <n> from entry <key> of <map>.`
- `<map> has entry <key>` / `<map> does not have entry <key>`, including inside
  compound `and` / `or` conditions.
- `the number of entries in <map>`.

### Text splitting, joining, and text-list stacks

- `split <text> by <delimiter>` → `List<TcString>` with multi-byte delimiter
  matching in pure Version 2 English (`std/string.eng`).
- `join <list> with <delimiter>` → `TcString`.
- `Let <stack> be an empty list of text.`, `append ... to item <pos> of ...`,
  `remove item <pos> of ...`, `the last item of ...`, and `Reverse <list>.`
  all work for text lists.
- `Let <word> be item <pos> of <text-list>.` now infers the element type, so
  character access, text length, printing, and slicing work without casts.

### Prior prose combinators retained

- Unindented paragraph loops and closers.
- `then stop` / `skip to the next one`.
- `one more than`, `one less than`, `the midpoint of`, `the larger/smaller of`,
  `the last position of`.
- `empty text`, append text/byte, text slices.
- Repeated-value lists, text/character comparisons, and list-item swaps.

---

## 2. Batch 1 scorecard

Solved: **36 / 50**

`0001`, `0003`, `0004`, `0005`, `0006`, `0007`, `0008`, `0009`, `0010`,
`0011`, `0012`, `0013`, `0014`, `0015`, `0016`, `0018`, `0020`, `0021`,
`0026`, `0027`, `0028`, `0029`, `0031`, `0032`, `0033`, `0034`, `0035`,
`0036`, `0038`, `0041`, `0042`, `0043`, `0044`, `0045`, `0048`, `0050`.

Newly recovered in this checkpoint: `0006`, `0009`, `0010`, `0014`, `0015`,
`0016`, `0018`, `0020`, `0031`, `0032`, `0036`, `0038`, `0043`, `0044`,
`0045`, `0048`.

Skipped: **14** (`2, 17, 19, 22, 23, 24, 25, 30, 37, 39, 40, 46, 47, 49`).

---

## 3. Batch 2 scorecard

Solved: **30 / 50**

`0053`, `0054`, `0055`, `0056`, `0057`, `0058`, `0059`, `0060`, `0062`,
`0063`, `0064`, `0066`, `0067`, `0069`, `0070`, `0071`, `0072`, `0073`,
`0074`, `0075`, `0076`, `0080`, `0081`, `0084`, `0085`, `0088`, `0089`,
`0091`, `0096`, `0097`.

Newly recovered in this checkpoint: `0054`, `0056`, `0057`, `0059`, `0060`,
`0071`, `0072`, `0073`, `0076`, `0084`, `0085`, `0097`.

Skipped: **20**

| Group | Count | Problems |
|---|---:|---|
| Linked-list pointers | 5 | 61, 82, 83, 86, 92 |
| Binary-tree node harnesses | 5 | 94, 95, 98, 99, 100 |
| Recursive/backtracking | 7 | 51, 52, 77, 78, 79, 90, 93 |
| Other structural limits | 3 | 65, 68, 87 |

Every entry has a specific `skip_reason` in
`benchmarks/leetcode/ledger.json`.

---

## 4. Complete newly solved examples

### #0020 — Valid Parentheses

```text
Let word be "()[]{}".
Let last be the last position of word.
Let stack be an empty list of whole numbers.
Let invalid be 0.
For every position from 0 to last.
Let symbol be character position of word.
Let size be the number of items in stack.
If symbol is 40 or symbol is 91 or symbol is 123, append symbol to stack.
If symbol is 41 and size is 0, add 1 to invalid.
If symbol is 41 and size is more than 0, let top be item (size minus 1) of stack, remove the last item from stack, if top is not 40, add 1 to invalid.
If symbol is 93 and size is 0, add 1 to invalid.
If symbol is 93 and size is more than 0, let top be item (size minus 1) of stack, remove the last item from stack, if top is not 91, add 1 to invalid.
If symbol is 125 and size is 0, add 1 to invalid.
If symbol is 125 and size is more than 0, let top be item (size minus 1) of stack, remove the last item from stack, if top is not 123, add 1 to invalid.

Let final size be the number of items in stack.
If invalid is 0 and final size is 0, print true, otherwise print false.
```

Output: `true`.

### #0071 — Simplify Path

```text
Let path be empty text.
Append "/home//foo/" to path.
Let parts be split path by "/".
Let stack be an empty list of text.
For each part in parts.
If part is "", skip to the next one.
If part is ".", skip to the next one.
Let size be the number of items in stack.
Let parent step be 0.
If part is "..", set parent step to 1.
If parent step is 1 and size is more than 0, remove the last item from stack.
If parent step is 1 and size is 0, skip to the next one.
Let keep be 1.
If part is "", set keep to 0.
If part is ".", set keep to 0.
If parent step is 1, set keep to 0.
If keep is 1, append part to stack.

Let joined be join stack with "/".
Let result be empty text.
Append "/" to result.
Append joined to result.
Print result.
```

Output: `/home/foo`.

### #0072 — Edit Distance

```text
Let first be "horse".
Let second be "ros".
Let last first be the last position of first.
Let last second be the last position of second.
Let rows be last first plus 1.
Let columns be last second plus 1.
Let width be columns plus 1.
Let height be rows plus 1.
Let size be width times height.
Let table be a list of size zeros.
For every column from 0 to last second.
Set item column of table to column.

For every row from 1 to rows.
Set item (row times width) of table to row.

For every row from 1 to rows.
For every column from 1 to columns.
Let index be row times width plus column.
If character (row minus 1) of first is character (column minus 1) of second, set item index of table to item (index minus width minus 1) of table.
Otherwise, let removal be item (index minus width) of table, let insertion be item (index minus 1) of table, let replacement be item (index minus width minus 1) of table, let best be the smaller of removal and insertion, let final best be the smaller of best and replacement, set item index of table to final best plus 1.

Let last be one less than size.
Print item last of table.
```

Output: `3`.

---

## 5. Verification

| Gate | Result |
|---|---|
| `make mbt` | **66/66 PASS** |
| Strict prose-only gate | **66/66 PASS** |
| `tc interp` expected stdout | 66/66 |
| Static zero-libc ELF64 | 66/66 |
| `tc_wasm.wasm` | 66/66 |
| IR runtime-honesty | 66/66 |
| `tc-lsp` diagnostics | 0 |
| `make test` | **1,087 tests PASS** |
| `make fmt-check` | PASS |
| `make lint` | PASS, 0 warnings |
| `make audit` | PASS |
| `tc --audit` / `tc --selftest` | PASS |

The source tree still has exactly nine local `tc-*` packages, zero external
crates, zero lockfile sources, and zero checksums.

---

## 6. Playground

- Drawer title: `Solved Problems (66)`.
- `web/site/leetcode_problems.json`: 66 entries, 53,561 bytes.
- Desktop at **1440×900**: drawer **66 of 66**, filter to **#0072**, run output
  `3`, zero console errors.
- Mobile at **375×812**: drawer **66 of 66**, filter and run **#0020** output
  `true`, zero console errors.
- `Page.getInstallabilityErrors` (Chromium/Brave CDP): `[]`.
- Manifest: standalone, `192x192` and `512x512` icons, valid start URL.
- Service worker: 13 cached assets, controller active, offline reload restored
  the full playground and the 66-problem drawer.

---

## 7. Remaining limits

The remaining 34 skips are genuine structural limits rather than disguised
Python:

- **10 linked-list problems** need node allocation and pointer rewiring.
- **5 binary-tree problems** need a node/traversal harness.
- **14 recursive/backtracking problems** need shared recursive helper state.
- **5 other problems** are a full numeric grammar (`65`), text-justification
  layout (`68`), concatenated-word indexing (`30`), anagram grouping (`49`),
  or recursive interval DP (`87`).

All remain marked `skipped` with a concrete one-line reason in the ledger.
