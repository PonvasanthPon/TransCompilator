# LeetCode Massive Battle Test — Batches 1–2

**Project:** `tc_r1` `0.1.0-alpha-preview`  
**Batch 1 (`1–50`):** 50 solved, 0 skipped  
**Batch 2 (`51–100`):** 50 solved, 0 skipped  
**Global:** 100 solved, 1,002 skipped, 3,022 pending of 4,124 catalog entries.

The strict prose-only gate is unchanged. Every solved program is unindented,
colon-free, symbolic-operator-free human prose. No Core-7 opcode, external
crate, OOP construct, or compile-time evaluator was added.

---

## 1. Final patterns used

- **Sequence-backed linked lists:** linked-list algorithms run over
  `List<i64>` with runtime indexing, swapping, removal, sorting, carry
  arithmetic, and stable partitioning.
- **Array-backed binary trees:** trees use parallel `values`, `lefts`, and
  `rights` lists with `-1` for an absent child. Recursive prose functions
  perform inorder traversal, validation, recovery, and structural comparison.
- **Unindented recursion:** `To ...` functions with natural self-calls,
  early `return`, shared scalar state, shared integer lists, shared text lists,
  and recursive backtracking.
- **Text machinery:** `sort characters of <text>`, `compare text ... with ...`,
  `format whole number ... as text`, `split`/`join`, and mutable text buffers.
- **Interval DP:** Scramble String uses a flattened runtime 3D table.
- **Greedy layout:** Text Justification uses recursive line fitting and
  runtime space distribution.

No compiler change was required in `crates/tc-semantics/src/lower/` for this
final batch. The existing recursion/shared-state primitives were sufficient.
The editor parity runner’s interpreter budget was raised to 50M steps to match
the verified MBT budget for heavy programs such as Sudoku.

---

## 2. Final scorecard

### Batch 1: **50 / 50**

`0001`–`0050` all solved, including the final linked-list sequences
`0002`, `0019`, `0023`, `0024`, `0025`.

### Batch 2: **50 / 50**

`0051`–`0100` all solved, including the final structural and tree problems
`0068`, `0087`, `0094`, `0095`, `0098`, `0099`, `0100`, plus list problems
`0061`, `0082`, `0083`, `0086`, `0092`.

Skipped in `1–100`: **none**.

---

## 3. Complete final examples

### #0068 — Text Justification

```text
Let source be "This,is,an,example,of,text,justification.".
Let pieces be split source by ",".
Keep a shared words as an empty list of text.
For each piece in pieces.
Append piece to words.

Keep a shared count as 0.
Set count to 7.
Keep a shared width as 0.
Set width to 16.
Keep a shared finish as 0.
Keep a shared usedwidth as 0.
Keep a shared padding as empty text.
Append "                                                                " to padding.

To search from position (a whole number) with occupied (a whole number).
If position is count, set finish to position, return.
Let word be item position of words.
Let word last be the last position of word.
Let next usedwidth be occupied plus 2 plus word last.
Let previous used be occupied.
Set usedwidth to next usedwidth.
If next usedwidth is at most width, search from position plus 1 with next usedwidth, return.
Set usedwidth to previous used.
Set finish to position.

To build line from start (a whole number).
Set finish to 0.
Let first word be item start of words.
Let first last be the last position of first word.
Set usedwidth to first last plus 1.
Search from start plus 1 with usedwidth.
Let line words be finish minus start.
Let gaps be line words minus 1.
Let base be 0.
Let extra be 0.
If finish is less than count and gaps is more than 0, set base to (width minus usedwidth plus gaps) divided by gaps, set extra to (width minus usedwidth plus gaps) modulo gaps.
Let line be empty text.
For every index from start to finish minus 1.
Let word be item index of words.
Append word to line.
Let gap be index minus start.
Let amount be base.
If gap is less than extra, add 1 to amount.
Let piece be the slice of padding from 0 to amount.
If index is less than finish minus 1 and finish is count, append " " to line.
If index is less than finish minus 1 and finish is less than count, append piece to line.
If index is finish minus 1 and finish is count, let pad piece be the slice of padding from 0 to width minus usedwidth plus gaps, append pad piece to line, print line.
If index is finish minus 1 and finish is less than count, print line, build line from finish.

Build line from 0.
```

Output:

```text
This    is    an
example  of text
justification.  
```

### #0087 — Scramble String

```text
Let first text be "great".
Let second text be "rgeat".
Let size be 5.
Let width be 6.
Let table be a list of 150 zeros.
For every first from 0 to 4.
For every second from 0 to 4.
If character first of first text is character second of second text, set item ((first times 5 plus second) times 6 plus 1) of table to 1.

For every length from 2 to 5.
For every first from 0 to (5 minus length).
For every second from 0 to (5 minus length).
For every split from 1 to length minus 1.
Let no swap be 0.
If item ((first times 5 plus second) times 6 plus split) of table is 1 and item (((first plus split) times 5 plus second plus split) times 6 plus length minus split) of table is 1, set no swap to 1.
Let with swap be 0.
If item ((first times 5 plus second plus length minus split) times 6 plus split) of table is 1 and item (((first plus split) times 5 plus second) times 6 plus length minus split) of table is 1, set with swap to 1.
If no swap is 1 or with swap is 1, set item ((first times 5 plus second) times 6 plus length) of table to 1.

If item 5 of table is 1, print true.
Otherwise, print false.
```

Output: `true`.

### #0098 — Validate Binary Search Tree

```text
Keep a shared values as an empty list of whole numbers.
Append 2 to values.
Append 1 to values.
Append 3 to values.
Keep a shared lefts as an empty list of whole numbers.
Append 1 to lefts.
Append -1 to lefts.
Append -1 to lefts.
Keep a shared rights as an empty list of whole numbers.
Append 2 to rights.
Append -1 to rights.
Append -1 to rights.
Keep a shared order as an empty list of whole numbers.
Keep a shared order count as 0.
To gather from node (a whole number).
If node is less than 0, return.
Gather from item node of lefts.
Append node to order.
Add 1 to order count.
Gather from item node of rights.

Gather from 0.
Let valid be 1.
Let last be order count minus 1.
For every position from 1 to last.
Let before be item (position minus 1) of order.
Let current be item position of order.
If item before of values is at least item current of values, set valid to 0.

If valid is 1, print true.
Otherwise, print false.
```

Output: `true`.

---

## 4. Verification

| Gate | Result |
|---|---|
| `make mbt` | **100/100 PASS** |
| Strict prose-only gate | **100/100 PASS** |
| `tc interp` expected stdout | 100/100 |
| Static zero-libc ELF64 | 100/100 |
| `tc_wasm.wasm` | 100/100 |
| IR runtime-honesty | 100/100 |
| `tc-lsp` diagnostics | 0 |
| `make test` | **1,093 tests PASS** |
| `make fmt-check` | PASS |
| `make lint` | PASS, 0 warnings |
| `make audit` | PASS |
| `tc --audit` / `tc --selftest` | PASS |

The source tree still has exactly nine local `tc-*` packages, zero external
crates, zero lockfile sources, and zero checksums.

---

## 5. Playground

- Drawer title: `Solved Problems (100)`.
- `web/site/leetcode_problems.json`: 100 entries, 90,832 bytes.
- Desktop at **1440×900**: drawer **100 of 100**, `#0068`, `#0087`, and
  `#0098` all ran through wasm, zero console errors.
- Mobile at **375×812**: drawer **100 of 100**, zero console errors.
- `Page.getInstallabilityErrors` (Chromium/Brave CDP): `[]`.

---

## 6. Remaining catalog limits

Nothing in `1–100` is skipped. The remaining 1,002 catalog skips are outside
this batch: paywalled entries, OOP/class-design problems, SQL/shell tracks,
interactive judges, concurrency, and later structural formats.
