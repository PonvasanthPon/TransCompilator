# tc_r1 v1.0 Coverage & Grammar-Orthogonality Audit

## Executive verdict

**Can valid English still fail? Yes, but only in three honest categories.**

1. **Non-computational meaning.** Physical, social, emotional, weather, and
   communication events (`bake`, `walk`, `sleep`, `rain`, `admire`) are
   deliberately `COMMENT_OMITTED` or cleanly `UNSUPPORTED`; the compiler is a
   finite computational language, not an open-world reasoner.
2. **Remaining computational gaps.** Verbs such as `split`, `search`, `sort`,
   `check`, `verify`, `measure`, `find`, `select`, and `filter` do not yet have
   executable frame lowering. They fail loudly, never panic or silently drop.
3. **Documented architectural limits.** The expression-depth cap, caller-owned
   `StrMap` keys, editor interpreter budget, and other entries in
   `CONVERGENCE_LIMITS.md` remain intentional boundaries.

Inside the supported vocabulary, the full **Statement x Target x Scope**
matrix is green: **798 PASS, 35 structurally inapplicable, 0 FAIL (833 cells)**.
The two live human-test programs pass verbatim with exact ELF64 == reference
interpreter parity.

## WordNet 2025 coverage after expansion

| Measure | Value |
|---|---:|
| WordNet corpus described by provenance | 135,969 entries / 107,519 synsets |
| Computational rows | **772** |
| Verb / adjective / noun rows | **769 / 2 / 1** |
| Unique computational lemmas | **723** |
| Unique synset ids | **517** |
| Depth-0 / 1 / 2 / 3 rows | **76 / 213 / 218 / 265** |
| Morphology verb lemmas | **8,749** |
| Irregular morphology rows | **103** |
| Exact anchors | **39** |
| Reachable synsets | **630** |

### Entries by SemanticConcept

| Concept | Rows |
|---|---:|
| `Transfer` | 269 |
| `Assign` | 232 |
| `Branch` | 56 |
| `Destroy` | 47 |
| `CompareNe` | 42 |
| `Loop` | 34 |
| `Create` | 23 |
| `Swap` | 9 |
| `Input` | 8 |
| `Add` | 7 |
| `Adjust` | 6 |
| `CompareGt` | 4 |
| `Double` | 4 |
| `Mul` | 4 |
| `Negate` | 4 |
| `Reset` | 4 |
| `Div` | 3 |
| `Return` | 3 |
| `Sub` | 3 |
| `Copy` | 2 |
| `CompareEq` | 1 |
| `CompareLt` | 1 |
| `Convert` | 1 |
| `Decrement` | 1 |
| `Halve` | 1 |
| `Increment` | 1 |
| `Mod` | 1 |
| `Output` | 1 |

The sweep now executes **41** verb entries in at least one imperative form,
correctly omits **754** narratives, returns **728** unsupported imperatives,
and reports **0 silent drops** and **0 panics**. New anchors are direct-only
so they do not open broad change-of-state trees.

### Honest verb/gap status

| Verb | Status | Evidence / note |
|---|---|---|
| `adjust` | mapped via WordNet | Adjust; `adjust-26.9` disabled by `Adjust x by ...` |
| `modify` | mapped via WordNet | Adjust; `Modify x to ...` lowers as assignment |
| `alter` | mapped via WordNet | Adjust; `Alter x to ...` lowers as assignment |
| `split` | UNMAPPED GAP | no general split lowering |
| `merge` | mapped via VerbNet | Combine; additive combine |
| `copy` | mapped via WordNet | Copy; `Copy x into y` |
| `swap` | mapped via WordNet | Swap; `Swap a and b` |
| `exchange / interchange` | mapped via WordNet | Swap anchors |
| `search` | UNMAPPED GAP | imperative not in computational lexicon |
| `sort` | UNMAPPED GAP | `std/list` sort exists; verb form is a gap |
| `compare` | mapped via VerbNet | Combine; additive approximation |
| `check` | UNMAPPED GAP | imperative not in computational lexicon |
| `verify` | UNMAPPED GAP | Emit; no scalar verifier lowering |
| `measure` | UNMAPPED GAP | imperative not in computational lexicon |
| `convert` | mapped via WordNet | Convert; type-cast expression |
| `reset / zero / clear` | mapped via WordNet | Reset; scalar clear to zero |
| `increment / bump` | mapped via WordNet | Increment anchor; default +1 or `by n` |
| `decrement` | mapped via WordNet | Decrement anchor; default -1 or `by n` |
| `append` | mapped via WordNet | Add |
| `insert` | mapped via VerbNet | Assign/Add |
| `lookup` | handled as std phrase | `look up <key> in <map>` |
| `negate` | mapped via WordNet | Transform/Negate |
| `count / sum / average` | handled as expression phrases | `count of`, `sum of`; imperative verbs are gaps |
| `delete` | handled as std phrase | `Delete file <path>`; scalar verb is a gap |
| `find / select / filter` | UNMAPPED GAP | selection verbs without lowering |
| `round / scale / translate` | UNMAPPED GAP | no general verb lowering |
| `clear bits` | handled as structural keyword | preserved bitwise clear; scalar Clear uses Reset |

## VerbNet 3.4 coverage

| Measure | Value |
|---|---:|
| Harvested classes/subclasses | 609 (329 XML files) |
| Frames | **10,845** |
| Unique verbs | **1,492** |
| Unique referenced class ids | **185** |
| Unique prepositions | **24** |
| Executable frames | **1,490** |
| Narrative/comment frames | **6** |
| Unsupported frames | **9,349** |
| Type errors / target clobbers / panics | **0 / 0 / 0** |

### Frames by VerbPredicate

| Predicate | Frames |
|---|---:|
| `Emit` | 4428 |
| `Transform` | 2857 |
| `Create` | 824 |
| `Receive` | 692 |
| `TransferSub` | 655 |
| `Combine` | 484 |
| `TransferMove` | 352 |
| `TransferAdd` | 286 |
| `Assign` | 205 |
| `Destroy` | 62 |

### Honest three-way triage

The conservative non-computational set is 43 classes covering emission,
animal sound, physical motion, body action, social chat, complaint, and similar
events.

| Category | Frames |
|---|---:|
| Mapped & executable (`EXEC`) | **1,490** |
| Conservative non-computational set (total) | **2,898** |
| -- executable inside that set | 55 |
| -- unsupported inside that set | **2,837** |
| -- narrative/comment inside that set | 6 |
| Non-computational unsupported + narrative | **2,843** |
| Remaining candidate pool (computational or unclassified) | **6,512** |
| **Total** | **10,845** |

The 2,837 figure is exact for the named class set. The remaining 6,512
unsupported frames are an honest candidate pool, not a claim that every frame
is computational. Computational-looking class examples with unsupported frame
counts:

| Class | Unsupported frames |
|---|---:|
| `adjust-26.9` | 36 |
| `convert-26.6.2` | 124 |
| `multiply-108` | 84 |
| `separate-23.1` | 5 |
| `break_down-45.8` | 0 |
| `estimate-34.2` | 0 |
| `base-97.1` | 4 |
| `mix-22.1` | 35 |
| `amalgamate-22.2` | 88 |
| `other_cos-45.4` | 2372 |
| `calibratable_cos-45.6.1` | 241 |

`multiply` is already reachable through `Multiply x by ...`; its VerbNet
`multiply-108` frame rows are a surface-shape gap, not a mathematical gap.

## Closed-class words

`CLOSED_CLASS_COUNT` is **279**, with no duplicate surface words:
65 prepositions, 64 discourse fillers, 46 pronouns, 35 control anchors,
27 auxiliaries, 25 determiners, 12 conjunctions, and 5 negations. All are
consumed through `lookup_closed_class`; no category is unaccounted for.

## Statement x Target x Scope matrix

Legend: **PASS** means compilation, native execution, and exact parity on
stdout/stderr/exit/files. **N/A** means structurally inapplicable: `Update`
has an aggregate target, so the five scalar/anaphor target rows do not apply.

**Total: 798 PASS, 35 N/A, 0 FAIL (833 cells).**

### Let

| Target \ Scope | Top level | To function | If branch | Else/Otherwise | While | Until | For every |
|---|---|---|---|---|---|---|---|
| Single-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Multi-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| it anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Value-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Pointer-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| its field anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Definite type possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |

### Put

| Target \ Scope | Top level | To function | If branch | Else/Otherwise | While | Until | For every |
|---|---|---|---|---|---|---|---|
| Single-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Multi-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| it anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Value-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Pointer-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| its field anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Definite type possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |

### Set

| Target \ Scope | Top level | To function | If branch | Else/Otherwise | While | Until | For every |
|---|---|---|---|---|---|---|---|
| Single-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Multi-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| it anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Value-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Pointer-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| its field anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Definite type possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |

### Store

| Target \ Scope | Top level | To function | If branch | Else/Otherwise | While | Until | For every |
|---|---|---|---|---|---|---|---|
| Single-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Multi-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| it anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Value-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Pointer-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| its field anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Definite type possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |

### Keep

| Target \ Scope | Top level | To function | If branch | Else/Otherwise | While | Until | For every |
|---|---|---|---|---|---|---|---|
| Single-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Multi-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| it anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Value-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Pointer-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| its field anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Definite type possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |

### Make

| Target \ Scope | Top level | To function | If branch | Else/Otherwise | While | Until | For every |
|---|---|---|---|---|---|---|---|
| Single-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Multi-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| it anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Value-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Pointer-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| its field anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Definite type possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |

### Becomes

| Target \ Scope | Top level | To function | If branch | Else/Otherwise | While | Until | For every |
|---|---|---|---|---|---|---|---|
| Single-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Multi-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| it anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Value-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Pointer-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| its field anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Definite type possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |

### Add

| Target \ Scope | Top level | To function | If branch | Else/Otherwise | While | Until | For every |
|---|---|---|---|---|---|---|---|
| Single-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Multi-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| it anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Value-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Pointer-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| its field anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Definite type possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |

### Remove

| Target \ Scope | Top level | To function | If branch | Else/Otherwise | While | Until | For every |
|---|---|---|---|---|---|---|---|
| Single-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Multi-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| it anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Value-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Pointer-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| its field anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Definite type possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |

### Transfer

| Target \ Scope | Top level | To function | If branch | Else/Otherwise | While | Until | For every |
|---|---|---|---|---|---|---|---|
| Single-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Multi-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| it anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Value-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Pointer-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| its field anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Definite type possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |

### Update

| Target \ Scope | Top level | To function | If branch | Else/Otherwise | While | Until | For every |
|---|---|---|---|---|---|---|---|
| Single-word scalar | N/A | N/A | N/A | N/A | N/A | N/A | N/A |
| Multi-word scalar | N/A | N/A | N/A | N/A | N/A | N/A | N/A |
| it anaphor | N/A | N/A | N/A | N/A | N/A | N/A | N/A |
| Value-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Pointer-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| its field anaphor | N/A | N/A | N/A | N/A | N/A | N/A | N/A |
| Definite type possessive | N/A | N/A | N/A | N/A | N/A | N/A | N/A |

### Output

| Target \ Scope | Top level | To function | If branch | Else/Otherwise | While | Until | For every |
|---|---|---|---|---|---|---|---|
| Single-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Multi-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| it anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Value-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Pointer-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| its field anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Definite type possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |

### Increment

| Target \ Scope | Top level | To function | If branch | Else/Otherwise | While | Until | For every |
|---|---|---|---|---|---|---|---|
| Single-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Multi-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| it anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Value-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Pointer-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| its field anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Definite type possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |

### Decrement

| Target \ Scope | Top level | To function | If branch | Else/Otherwise | While | Until | For every |
|---|---|---|---|---|---|---|---|
| Single-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Multi-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| it anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Value-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Pointer-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| its field anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Definite type possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |

### Reset

| Target \ Scope | Top level | To function | If branch | Else/Otherwise | While | Until | For every |
|---|---|---|---|---|---|---|---|
| Single-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Multi-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| it anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Value-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Pointer-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| its field anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Definite type possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |

### Copy

| Target \ Scope | Top level | To function | If branch | Else/Otherwise | While | Until | For every |
|---|---|---|---|---|---|---|---|
| Single-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Multi-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| it anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Value-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Pointer-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| its field anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Definite type possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |

### Swap

| Target \ Scope | Top level | To function | If branch | Else/Otherwise | While | Until | For every |
|---|---|---|---|---|---|---|---|
| Single-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Multi-word scalar | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| it anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Value-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Pointer-struct possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| its field anaphor | PASS | PASS | PASS | PASS | PASS | PASS | PASS |
| Definite type possessive | PASS | PASS | PASS | PASS | PASS | PASS | PASS |

## Canonical clause patterns

| Pattern | Verified computational example | Status |
|---|---|---|
| SV | `It was raining outside.` | Narrative `COMMENT_OMITTED` |
| SVO | `Add tea to cash.` / `180 is removed from cash.` | Imperative/passive supported |
| SVC | `Cash is 500.` / `Cash becomes 500.` | Supported assignment |
| SVA | `Ravi waited 20 minutes.` | Narrative omitted |
| SVOO | `Transfer 200 from bank to wallet.` | Supported transfer |
| SVOC | `Update pondy so its distance is 150 and its toll is 220.` | Supported update |
| SVOA | `Put 100 in cash.` | Supported placement |
| Passive | `180 is removed from cash.` | Supported |
| Coordination | `Put 100 in cash, add 50 to it, remove 20 from it, and say it.` | Supported |
| Subordination | `If ...`, `While ...`, `Until ...` | Supported |
| Quantification | `For every day from 1 to 5, add it to bonus.` | Supported |
| Anaphora | `Put 100 in cash. Add 50 to it.` / `its distance` / `the waypoint's toll` | Supported |

**Honest scope note:** grammatical narrative is not executable by default.
Only the finite computational subset lowers to Core-7; all other prose is
omitted or gets a structured diagnostic. The matrix proves orthogonality in
the supported subset, not universal English semantics.

## Reproduce

```bash
python3 goldmines/tools/harvest_wordnet_r1.py --data-dir <oewn-release>
python3 goldmines/tools/harvest_verbnet_r1.py --data-dir <verbnet-release>
cargo test -p tc-semantics --test orthogonality_audit
cargo test -p tc-semantics --test semantic_frame_and_lexicon_expansion
cargo run -q -p tc-semantics --bin lexicon_sweep -- --output /tmp/lexicon_sweep.json
```
