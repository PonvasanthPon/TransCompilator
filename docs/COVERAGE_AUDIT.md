# tc_r1 v1.0 Coverage & Grammar-Orthogonality Audit

## Executive verdict

**Can valid English still fail? Yes, but for three distinct reasons.**

1. **Not all English verbs are computational.** A large part of VerbNet and
   WordNet describes physical, social, emotional, weather, and communication
   events (`bake`, `walk`, `sleep`, `rain`, `admire`). Those are deliberately
   classified as `COMMENT_OMITTED` or a clean `UNSUPPORTED` diagnostic, not
   silently executed.
2. **Some computational synonyms are not mapped yet.** `adjust`, `modify`,
   `alter`, `copy`, `swap`, `search`, `sort`, `measure`, `convert`, and
   similar verbs are honest gaps. They fail loudly; they do not panic or
   silently drop an instruction.
3. **Intentional architectural limits remain.** The 32-level expression
   nesting cap, caller-owned `StrMap` keys, the editor interpreter budget,
   and the other entries in `CONVERGENCE_LIMITS.md` are documented boundaries.

Within the compiler's supported vocabulary and grammar, the full
**Statement x Target x Scope** matrix is green: **553 PASS, 35 structurally
inapplicable, 0 FAIL**. The two live human-test programs pass verbatim with
exact ELF64 == reference-interpreter parity.

## WordNet 2025 coverage

The generated table is a deliberate computational slice, not a claim to cover
all natural-language meaning.

| Measure | Value |
|---|---:|
| WordNet corpus described by provenance | 135,969 entries / 107,519 synsets |
| Computational rows in `WORDNET_ENTRIES` | **786** |
| Verb rows | **783** |
| Adjective rows | **2** |
| Noun rows | **1** |
| Unique computational lemmas | **738** |
| Unique synset ids | **534** |
| Depth-0 / 1 / 2 / 3 rows | **56 / 216 / 228 / 286** |
| Morphology verb lemmas recognized | **8,749** |
| Irregular morphology rows | **103** |

### Entries by SemanticConcept

| Concept | Rows |
|---|---:|
| `Transfer` | 302 |
| `Assign` | 232 |
| `Branch` | 57 |
| `Destroy` | 47 |
| `CompareNe` | 42 |
| `Loop` | 34 |
| `Create` | 23 |
| `Input` | 8 |
| `Add` | 7 |
| `CompareGt` | 4 |
| `Double` | 4 |
| `Mul` | 4 |
| `Negate` | 4 |
| `Swap` | 4 |
| `Div` | 3 |
| `Return` | 3 |
| `Sub` | 3 |
| `CompareEq` | 1 |
| `CompareLt` | 1 |
| `Halve` | 1 |
| `Mod` | 1 |
| `Output` | 1 |

The three non-verb rows are retained for provenance/lemma reasons but are not
executable imperative entries. The sweep exercised all 783 verb rows in `to`,
`from`, and past-tense narrative shapes: **16** entries produced at least one
executable imperative form, **766** narratives were correctly
`COMMENT_OMITTED`, **767** imperative probes were `UNSUPPORTED`, and there
were **0 silent drops** and **0 panics**.

### Honest WordNet/verb-gap list

Status is for the tested imperative form, not a claim that the English word is
never usable in another supported expression.

| Verb | Status | Evidence / note |
|---|---|---|
| `adjust` | UNMAPPED GAP | Transform; `adjust-26.9`, 36 unsupported frames |
| `modify` | UNMAPPED GAP | Transform; no executable imperative form |
| `alter` | UNMAPPED GAP | Transform; no executable imperative form |
| `split` | UNMAPPED GAP | imperative not in computational lexicon |
| `merge` | mapped via VerbNet | Combine; executes additively (`mix-22.1` / `amalgamate-22.2`) |
| `copy` | UNMAPPED GAP | no Core-7 copy lowering |
| `swap` | UNMAPPED GAP | WordNet `Swap` concept exists; no imperative lowering |
| `search` | UNMAPPED GAP | imperative not in computational lexicon |
| `sort` | UNMAPPED GAP | verb form gap; `std/list` sort exists but is a library call |
| `compare` | mapped via VerbNet | Combine; additive approximation for the swept form |
| `check` | UNMAPPED GAP | imperative not in computational lexicon |
| `verify` | UNMAPPED GAP | Emit; no scalar verifier lowering |
| `measure` | UNMAPPED GAP | imperative not in computational lexicon |
| `convert` | UNMAPPED GAP | Transform; `convert-26.6.2`, 125 unsupported frames |
| `reset` | UNMAPPED GAP | no Core-7 reset lowering |
| `clear` | handled as structural keyword | only `Clear bits ... from ...`; plain `Clear x` is a gap |
| `increment` | UNMAPPED GAP | use `Increase ... by ...` |
| `decrement` | UNMAPPED GAP | use `Decrease ... by ...` |
| `append` | mapped via WordNet | Add (`append` / `oewn-01029183-v`) |
| `insert` | mapped via VerbNet | Assign/Add (`put-9.1-1`) |
| `lookup` | handled as std phrase | `look up <key> in <map>`; bare imperative is omitted |
| `negate` | mapped via WordNet | Transform/Negate (`Negate x` works) |
| `count / sum / average` | handled as expression phrases | `count of`, `sum of` and std calls are supported; imperative verb forms are not |
| `delete` | handled as std phrase | `Delete file <path>`; plain scalar `Delete x` is a gap |
| `find / select / filter` | UNMAPPED GAP | receive/selection verbs without a mapped lowering |
| `round / scale / translate` | UNMAPPED GAP | no general verb lowering |

## VerbNet 3.4 coverage

| Measure | Value |
|---|---:|
| Harvested source classes/subclasses | 609 (329 XML files) |
| Frames in `VERBNET_FRAMES` | **10,845** |
| Unique verbs | **1,492** |
| Unique referenced class ids | **185** |
| Unique prepositions | **24** |
| Executable swept frames | **1,497** |
| Correctly narrative/comment frames | **6** |
| Unsupported swept frames | **9,342** |
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

### Honest three-way triage of the 10,845 frames

The sweep is an executable-form probe, not a semantic classification of every
frame. For the non-computational side we use a conservative **43-class set**
covering sound/animal/substance emission, physical motion, body action,
social chat, complaint, and similar events.

| Category | Frames |
|---|---:|
| Mapped & executable (`EXEC`) | **1,497** |
| Conservative non-computational class set (total) | **2,898** |
| -- executable frames inside that set | 57 |
| -- unsupported frames inside that set | **2,835** |
| -- narrative/comment frames inside that set | 6 |
| Non-computational unsupported + narrative | **2,841** |
| Remaining unsupported candidate pool (computational or unclassified) | **6,507** |
| **Total** | **10,845** |

The 2,835 figure is exact for the named class set. The remaining 6,507
unsupported frames are **not** all proven computational: they are the honest
candidate pool after removing the conservative non-computational classes.
Examples of clearly computational-looking classes in that pool, with their
**unsupported** frame counts:

| Class | Unsupported frames |
|---|---:|
| `adjust-26.9` | 36 |
| `convert-26.6.2` | 125 |
| `multiply-108` | 84 |
| `separate-23.1` | 5 |
| `break_down-45.8` | 0 |
| `estimate-34.2` | 0 |
| `base-97.1` | 4 |
| `mix-22.1` | 35 |
| `amalgamate-22.2` | 88 |
| `other_cos-45.4` | 2373 |
| `calibratable_cos-45.6.1` | 241 |

`multiply` is already reachable through the structural arithmetic form
(`Multiply cash by 3`); its VerbNet frame row is a spec-shape gap, not proof
that multiplication itself is missing. The same distinction applies to
`count`, `sum`, and library-backed operations.

## Closed-class function words

`CLOSED_CLASS_COUNT` is **279**, with no duplicate surface words:

| Category | Words |
|---|---:|
| Preposition | 65 |
| Discourse filler | 64 |
| Pronoun | 46 |
| Control anchor | 35 |
| Auxiliary/modal | 27 |
| Determiner | 25 |
| Conjunction | 12 |
| Negation | 5 |
| **Total** | **279** |

All 279 are represented by `ClosedClass` and consumed through
`lookup_closed_class` by segmentation, intent classification, normalization,
and lowering. This audit found no unaccounted category.

## Statement x Target x Scope matrix

Legend: **PASS** means the program compiled, ran natively, and matched the
reference interpreter exactly on stdout, stderr, exit code, and files.
**N/A** means the combination is structurally inapplicable: `Update` has an
aggregate target, so the five scalar/anaphor rows do not form a valid cell.

**Total: 553 PASS, 35 N/A, 0 FAIL (588 cells).**

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

## Seven canonical clause patterns and English structures

This table describes the computational analogue of each pattern. Natural
narrative clauses are intentionally not executed as code.

| Pattern | Verified computational example | Status |
|---|---|---|
| SV | `It was raining outside.` | Narrative `COMMENT_OMITTED`; no executable subjectless action |
| SVO | `Add tea to cash.` / `180 is removed from cash.` | Supported imperative/passive computational forms |
| SVC | `Cash is 500.` / `Cash becomes 500.` | Supported copular assignment |
| SVA | `Ravi waited 20 minutes.` | Narrative `COMMENT_OMITTED`; location/time prose is not an instruction |
| SVOO | `Transfer 200 from bank to wallet.` | Supported ditransitive transfer |
| SVOC | `Update pondy so its distance is 150 and its toll is 220.` | Supported object + field-complement update |
| SVOA | `Put 100 in cash.` | Supported object + destination |
| Passive | `180 is removed from cash.` | Supported with the same arithmetic semantics |
| Coordination | `Put 100 in cash, add 50 to it, remove 20 from it, and say it.` | Supported action chains and comma-aware conditions |
| Subordination | `If ...`, `While ...`, `Until ...` | Supported for computational conditions/actions |
| Quantification | `For every day from 1 to 5, add it to bonus.` | Supported; local `every`/`some`/`no` also covered |
| Anaphora | `Put 100 in cash. Add 50 to it.` / `its distance` / `the waypoint's toll` | Supported through the DRS discourse stack |

**Honest partial/out-of-scope note:** the compiler does not claim general
natural-language understanding. It executes a finite computational subset.
Physical/social/weather SVO clauses cannot be used as program operations just
because they parse grammatically; they are omitted as prose or rejected with a
structured diagnostic. The matrix proves orthogonality inside the supported
subset, not universal English semantics.

## Reproduce

```bash
cargo test -p tc-semantics --test orthogonality_audit
cargo run -q -p tc-semantics --bin lexicon_sweep -- --output /tmp/lexicon_sweep.json
cargo test -p tc-semantics --test phase4_7_pure_english_polish std_files_are_pure_version2_english
cargo test -p tc-editor-gui --test native_gui_tests gui_source_has_no_unsupported_tofu_glyphs
```

