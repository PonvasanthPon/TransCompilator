# LeetCode Massive Battle Test — Batches 1–2

**Project:** `tc_r1` `0.1.0-alpha-preview`  
**Batch 1 (`1–50`):** 45 solved, 5 skipped  
**Batch 2 (`51–100`):** 38 solved, 12 skipped  
**Global:** 83 solved, 1,019 skipped, 3,022 pending of 4,124 catalog entries.

The strict prose-only gate is unchanged. Every solved program is unindented,
colon-free, symbolic-operator-free human prose. No Core-7 opcode, external
crate, OOP construct, or compile-time evaluator was added.

---

## 1. General primitives added

### Unindented prose recursion

- Paragraph-scoped `To <phrase> (<type>).` functions with natural prose
  self-calls, no parentheses and no C-style call syntax.
- Natural early `return` inside inline branches, plus existing `give back`
  for value-returning functions.
- Function bodies may contain paragraph-scoped unindented loops.
- Recursive calls lower to ordinary Core-7 `Call`/`Return`; no evaluator or
  special-case recursion recognizer was added.

### Shared prose state

- `Keep a shared <name> as 0.` → shared whole number.
- `Keep a shared <name> as empty text.` → shared `TcString`.
- `Keep a shared <name> as an empty list of whole numbers.` → shared integer
  list.
- `Keep a shared <name> as an empty list of text.` → shared text list.
- Shared lists support append, item reads/writes, containment checks, length,
  last-item removal, and full mutation from recursive functions.
- Global aggregate access now emits `GlobalAddr`, so recursive list state works
  identically in native ELF64 and `tc_wasm.wasm`.

### Text helpers used by the new solutions

- `sort characters of <text>` / `the sorted characters of <text>`.
- `compare text <a> with text <b>` → `-1`, `0`, or `1`.
- `format whole number <n> as text`.
- `remove last character from <text>` for backtracking a mutable text buffer.
- `split`/`join`, map prose, multi-action `Otherwise if`, and all prior
  combinators remain intact.

---

## 2. Batch 1 scorecard

Solved: **45 / 50**

`0001`, `0003`, `0004`, `0005`, `0006`, `0007`, `0008`, `0009`, `0010`,
`0011`, `0012`, `0013`, `0014`, `0015`, `0016`, `0017`, `0018`, `0020`,
`0021`, `0022`, `0026`, `0027`, `0028`, `0029`, `0030`, `0031`, `0032`,
`0033`, `0034`, `0035`, `0036`, `0037`, `0038`, `0039`, `0040`, `0041`,
`0042`, `0043`, `0044`, `0045`, `0046`, `0047`, `0048`, `0049`, `0050`.

Newly recovered in this checkpoint: `0017`, `0022`, `0030`, `0037`, `0039`,
`0040`, `0046`, `0047`, `0049`.

Skipped: **5** (`2, 19, 23, 24, 25`).

---

## 3. Batch 2 scorecard

Solved: **38 / 50**

`0051`, `0052`, `0053`, `0054`, `0055`, `0056`, `0057`, `0058`, `0059`,
`0060`, `0062`, `0063`, `0064`, `0065`, `0066`, `0067`, `0069`, `0070`,
`0071`, `0072`, `0073`, `0074`, `0075`, `0076`, `0077`, `0078`, `0079`,
`0080`, `0081`, `0084`, `0085`, `0088`, `0089`, `0090`, `0091`, `0093`,
`0096`, `0097`.

Newly recovered in this checkpoint: `0051`, `0052`, `0065`, `0077`, `0078`,
`0079`, `0090`, `0093`.

Skipped: **12**

| Group | Count | Problems |
|---|---:|---|
| Linked-list pointers | 5 | 61, 82, 83, 86, 92 |
| Binary-tree node harnesses | 5 | 94, 95, 98, 99, 100 |
| Other structural limits | 2 | 68, 87 |

Every entry has a specific `skip_reason` in
`benchmarks/leetcode/ledger.json`.

---

## 4. Complete newly solved examples

### #0022 — Generate Parentheses

```text
Keep a shared current as empty text.
Keep a shared answers as an empty list of text.
To build from opened (a whole number) with closed (a whole number).
If current's len is 6, let copy be create text from current's data of length current's len, append copy to answers, then return.
If opened is less than 3, append "(" to current, build from opened plus 1 with closed, remove last character from current.
If closed is less than opened, append ")" to current, build from opened with closed plus 1, remove last character from current.

Build from 0 with 0.
For each answer in answers, print answer.
```

Output:

```text
((()))
(()())
(())()
()(())
()()()
```

### #0049 — Group Anagrams

```text
Let text be "eat,tea,tan,ate,nat,bat".
Let words be split text by ",".
Let last be the last position of words.
Let signatures be an empty list of text.
For each word in words.
Let signature be sort characters of word.
Append signature to signatures.

For every left from 0 to last.
For every right from left to last.
If compare text item right of signatures with text item left of signatures is less than 0, swap item left of signatures and item right of signatures, swap item left of words and item right of words.

Let group be empty text.
Let current be empty text.
Let answers be an empty list of text.
For every position from 0 to last.
Let signature be item position of signatures.
Let word be item position of words.
If position is 0, set current to signature, append word to group.
Let same be compare text signature with text current.
If position is more than 0 and same is 0 and group's len is more than 0, append "," to group, append word to group.
If position is more than 0 and same is not 0, append group to answers, set group to empty text, set current to signature, append word to group.

Append group to answers.
For each answer in answers, print answer.
```

Output (group order is signature-sorted):

```text
bat
tea,ate,eat
nat,tan
```

### #0052 — N-Queens II

```text
Keep a shared queens as an empty list of whole numbers.
Keep a shared safe as 0.
Keep a shared total as 0.
To check square at column (a whole number) for row (a whole number).
Set safe to 1.
For every prior from 0 to row minus 1.
Let other be item prior of queens.
If other is column, set safe to 0.
Let difference be column minus other.
If difference is less than 0, set difference to 0 minus difference.
Let distance be row minus prior.
If difference is distance, set safe to 0.

To place from level (a whole number).
If level is 4, add 1 to total, then return.
For every candidate from 0 to 3.
Check square at candidate for level.
If safe is 1, append candidate to queens, place from level plus 1, remove the last item from queens.

Place from 0.
Print total.
```

Output: `2`.

---

## 5. Verification

| Gate | Result |
|---|---|
| `make mbt` | **83/83 PASS** |
| Strict prose-only gate | **83/83 PASS** |
| `tc interp` expected stdout | 83/83 |
| Static zero-libc ELF64 | 83/83 |
| `tc_wasm.wasm` | 83/83 |
| IR runtime-honesty | 83/83 |
| `tc-lsp` diagnostics | 0 |
| `make test` | **1,091 tests PASS** |
| `make fmt-check` | PASS |
| `make lint` | PASS, 0 warnings |
| `make audit` | PASS |
| `tc --audit` / `tc --selftest` | PASS |

Sudoku Solver (`#0037`) completes in **11,659,764 IR steps** under the bounded
playground budget and passes all three engines.

The source tree still has exactly nine local `tc-*` packages, zero external
crates, zero lockfile sources, and zero checksums.

---

## 6. Playground

- Drawer title: `Solved Problems (83)`.
- `web/site/leetcode_problems.json`: 83 entries, 75,600 bytes.
- Desktop at **1440×900**: drawer **83 of 83**, `#0052` ran (`2`), `#0049`
  ran (`bat`, anagram groups), zero console errors.
- Mobile at **375×812**: drawer **83 of 83**, zero console errors.
- `Page.getInstallabilityErrors` (Chromium/Brave CDP): `[]`.
- Manifest and service-worker installability remain valid.

---

## 7. Remaining limits

The remaining 17 skips are genuine structural limits:

- **10 linked-list problems** need node allocation and pointer rewiring.
- **5 binary-tree problems** need a node/traversal harness.
- **2 other problems:** `68` needs text-justification line-packing state;
  `87` needs recursive interval DP with value-returning subproblem results.

All remain marked `skipped` with a concrete one-line reason in the ledger.
