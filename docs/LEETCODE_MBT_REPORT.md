# LeetCode Gold-Standard Writer Prose — 100 / 100

**Project:** `tc_r1` `0.1.0-alpha-preview`  
**Batch 1 (`1–50`):** 50 solved, 0 skipped  
**Batch 2 (`51–100`):** 50 solved, 0 skipped  
**Global:** 100 solved, 1,002 skipped, 3,022 pending of 4,124 catalog entries.

The strict gate now enforces **writer prose**, not only unindented prose. Every
one of the 100 solutions has **zero** occurrences of `Let`, `Append`,
`'s len`, `'s data`, `create text from`, `compare text`, ASCII conversion
codes, hardcoded 8+ space padding strings, or three-level nested index formulas.

---

## 1. Writer-English primitives added

- `Start with ...` / `Begin with ...` compound declarations.
- Subject-verb declarations: `The target is 9.`, `The word holds "babad".`,
  `The stack starts as an empty list of whole numbers.`
- Compound declarations with `, while` and `, and`.
- Placement verbs routed by target type:
  `Place`, `Put`, `Record`, `Collect`, `Add ... onto`.
- `a copy of <text>` for owned text copies.
- Character literals on byte variables:
  `symbol is "("`, `symbol is between "0" and "9"`.
- Lexicographic prose:
  `word comes before other`, `word comes after other`, `word matches other`.
- Dynamic text:
  `<count> spaces`, `repeat text " " <count> times`,
  `the digit value of <char>`, `the character for <number>`.
- `the length of <text>` and `the number of items in <list>` for all locals
  and shared globals.

The gate still rejects indentation, colons, `End.`, symbolic operators,
single-letter variables, boolean flags, snake_case, C calls, casts, `Use`,
top-level `Return 0`, and all former strict-prose violations.

---

## 2. Gold-gate audit

| Check | Violations across 100 files |
|---|---:|
| `Let` declarations | **0** |
| `Append` jargon | **0** |
| `'s len` / `'s data` | **0** |
| `create text from` | **0** |
| `compare text` | **0** |
| ASCII conversion codes `48` / `57` and char-code comparisons | **0** |
| 64-space / 8+ space literals | **0** |
| Three-level nested index formulas | **0** |
| Strict prose-only violations | **0** |

---

## 3. Complete gold-standard examples

### #0002 — Add Two Numbers

```text
The firstlist holds 2, 4, and 3, while the secondlist holds 5, 6, and 4. Start with an empty list for the answer, a carry of 0, and a position of 0.
The firstlast is the last position of firstlist.
The secondlast is the last position of secondlist.
Repeat while position is at most firstlast or position is at most secondlast or carry is 1.
The sum is carry.
If position is at most firstlast, set sum to sum plus item position of firstlist.
If position is at most secondlast, set sum to sum plus item position of secondlist.
Place sum modulo 10 into answer.
Set carry to sum divided by 10.
Add 1 to position.

For each value in answer, show value.
```

Output: `7`, `0`, `8`.

### #0020 — Valid Parentheses

```text
The word is "()[]{}". The last is the last position of word.
The stack is an empty list of whole numbers.
The invalid is 0.
The top is 0.
For every position from 0 to last.
The symbol is character position of word.
The size is the number of items in stack.
If symbol is "(" or symbol is "[" or symbol is "{", Place symbol into stack.
If symbol is ")" and size is 0, add 1 to invalid.
If symbol is ")" and size is more than 0, set top to item (size minus 1) of stack, remove the last item from stack, if top is not "(", add 1 to invalid.
If symbol is "]" and size is 0, add 1 to invalid.
If symbol is "]" and size is more than 0, set top to item (size minus 1) of stack, remove the last item from stack, if top is not "[", add 1 to invalid.
If symbol is "}" and size is 0, add 1 to invalid.
If symbol is "}" and size is more than 0, set top to item (size minus 1) of stack, remove the last item from stack, if top is not "{", add 1 to invalid.

The finalsize is the number of items in stack.
If invalid is 0 and finalsize is 0, show true, otherwise show false.
```

Output: `true`.

### #0022 — Generate Parentheses

```text
Keep a shared current as empty text.
Keep a shared answers as an empty list of text.
Keep a shared copy as empty text.
To build from opened (a whole number) with closed (a whole number).
If opened plus closed is 6, The copy is a copy of current, Place copy into answers, then return.
If opened is less than 3, Place "(" into current, build from opened plus 1 with closed, remove last character from current.
If closed is less than opened, Place ")" into current, build from opened with closed plus 1, remove last character from current.

Build from 0 with 0.
For each answer in answers, display answer.
```

Output: `((()))`, `(()())`, `(())()`, `()(())`, `()()()`.

### #0049 — Group Anagrams

```text
The text is "eat,tea,tan,ate,nat,bat". The words is split text by ",".
The last is the last position of words.
The signatures is an empty list of text.
For each word in words.
The signature is sort characters of word.
Place signature into signatures.

For every left from 0 to last.
For every right from left to last.
If item right of signatures comes before item left of signatures, swap item left of signatures and item right of signatures, swap item left of words and item right of words.

The group is empty text.
The groupcount is 0.
The current is empty text.
The answers is an empty list of text.
For every position from 0 to last.
The signature is item position of signatures.
The word is item position of words.
If position is 0, set current to signature, Place word into group, add 1 to groupcount.
The same is 1.
If signature comes before current, set same to 0.
If current comes before signature, set same to 0.
If position is more than 0 and same is 1 and groupcount is more than 0, Place "," into group, Place word into group, add 1 to groupcount.
If position is more than 0 and same is 0, Place group into answers, set group to empty text, set groupcount to 0, set current to signature, Place word into group, add 1 to groupcount.

Place group into answers.
For each answer in answers, display answer.
```

Output:

```text
bat
tea,ate,eat
nat,tan
```

---

## 4. Verification

| Gate | Result |
|---|---|
| `make mbt` | **100/100 PASS** |
| Gold-Standard Writer Gate | **100/100 PASS, 0 violations** |
| `tc interp` expected stdout | 100/100 |
| Static zero-libc ELF64 | 100/100 |
| `tc_wasm.wasm` | 100/100 |
| IR runtime-honesty | 100/100 |
| `tc-lsp` diagnostics | 0 |
| `make test` | **1,096 tests PASS** |
| `make fmt-check` | PASS |
| `make lint` | PASS, 0 warnings |
| `make audit` | PASS |
| `tc --audit` / `tc --selftest` | PASS |

---

## 5. Playground

- Drawer title: `Solved Problems (100)`.
- Desktop at **1440×900**: drawer **100 of 100**, zero console errors,
  `Page.getInstallabilityErrors` = `[]`.
- Wasm runs verified: `#0002`, `#0020`, `#0022`, `#0049`, `#0068`.
- Mobile at **375×812**: drawer **100 of 100**, zero console errors.

No problems in `1–100` are skipped. The remaining catalog skips are outside
this batch: paywalled entries, OOP/class design, SQL/shell, interactive
judges, and concurrency.
