# tc_r1 Lexicon Provenance

This file records the exact read-only linguistic sources used to generate the
Phase 2/3 `tc-lexicon` tables. No raw corpus files are committed into `tc_r1`;
only the deterministic Rust tables and the stdlib-only harvester scripts are
tracked.

## Open English WordNet 2025

- **Dataset:** Open English WordNet 2025 (WN-LMF XML release)
- **Release tag:** `2025-edition`
- **Upstream:** <https://github.com/globalwordnet/english-wordnet/releases/tag/2025-edition>
- **Release asset:** `english-wordnet-2025.xml.gz`
- **SHA-256:** `9ca6d1dcb75f822fdd66617f7d9da48142ace38dd544d6ad5e2feca1674ad3fe`
- **License:** CC-BY 4.0
- **Local use:** read-only copy in the sandbox goldmines tree; never modified.
- **Scanned:** 135,969 lexical entries and 107,519 synsets.
- **Harvest:** 43 exact `oewn-*` anchor synsets, 633 reachable synsets at
  hypernym distance <= 3, and **777** emitted `WordNetEntry` rows.
- **Morphology validation:** 8,749 single-word WordNet verb lemmas emitted as
  `VERB_LEMMAS`, plus **103** standard irregular form -> lemma rows.

### WordNet concept anchors

| Concept | Anchor synset |
|---|---|
| Add | `oewn-00642490-v` and `oewn-01029183-v` |
| Sub | `oewn-00642914-v` |
| Mul | `oewn-00643334-v` |
| Div | `oewn-00643760-v` |
| Mod | `oewn-13755803-n` |
| Negate | `oewn-00471015-v` |
| Double | `oewn-00247103-v` |
| Halve | `oewn-00644019-v` |
| Triple | `oewn-00247632-v` |
| Square | `oewn-00145268-v` |
| Cube | `oewn-00641924-v` |
| Assign | `oewn-01496967-v` |
| Transfer | `oewn-02225243-v` |
| Swap | `oewn-02263812-v` |
| CompareGt | `oewn-02675266-v` |
| CompareLt | `oewn-01559809-a` |
| CompareEq | `oewn-00893124-a` |
| CompareNe | `oewn-02672316-v` |
| Create | `oewn-01658171-v` and `oewn-01757596-v` |
| Destroy | `oewn-01622948-v` and `oewn-01551969-v` |
| Output | `oewn-01625832-v` and `oewn-00959711-v` (echo) |
| Input | `oewn-02214901-v` |
| Loop | `oewn-00960366-v` |
| Branch | `oewn-00676526-v` |
| Return | `oewn-02314987-v` |
| Add (direct) | `oewn-00156409-v` (increase) |
| Sub (direct) | `oewn-00151509-v` (decrease) |
| Increment (direct) | `oewn-00156409-v` (increase; imperative alias `increment`) |
| Decrement (direct) | `oewn-00151509-v` (decrease; imperative alias `decrement`) |
| Reset (direct) | `oewn-00949623-v` (reset), `oewn-00298288-v` (zero), `oewn-00181809-v` (clear) |
| Swap (direct) | `oewn-02262178-v` (exchange), `oewn-02262575-v` (interchange) |
| Copy (direct) | `oewn-01751182-v` (copy; imperative alias `duplicate`) |
| Convert (direct) | `oewn-00381684-v` |
| Adjust (direct) | `oewn-00296959-v` (adjust), `oewn-00126072-v` (alter), `oewn-00169614-v` (modify) |

The `direct` anchors are depth-zero anchors: they contribute their own synset
members without expanding a broad `verb.change` hyponym tree. This now also
covers `square`, `cube`, and `echo`.

The traversal is a multi-source BFS over inverse-hypernym (hyponym) edges,
retaining the shortest distance to an anchor and discarding lexical material
from narrative, motion, consumption, emotion, weather, and cooking roots
(`walk`, `rain`, `sleep`, `drink`, `eat`, `wait`, `love`, `laugh`, `smile`,
`bake`, `cook`, `travel`, `swim`) and ambiguous light/measure lemmas
(`be`, `have`, `do`, `go`, `come`, `get`, `run`, `watch`, `very`, `last`,
`plus`, `times`, `minus`, `equals`, `exists`, `series`, `bus`, `gas`,
`class`, `diminish`, `lessen`, `fall`). WordNet POS is preserved on every
row, so unrelated noun/adjective senses cannot collide with computational
verb senses. The post-0.1.0 expansion adds direct-only anchor families
for increment/decrement/reset/swap/copy/convert/adjust. OEWN 2025 has noun
entries but no verb lemma for `increment`/`decrement`; their imperative aliases
carry the exact increase/decrease synset provenance as explicit derived rows.
This is harvest-time data curation, not a Rust verb lookup list.

## CU Boulder VerbNet 3.4

- **Dataset:** CU Boulder VerbNet 3.4
- **Upstream:** <https://github.com/cu-clear/verbnet>
- **Commit:** `ae8e9cfdc2c0d3414b748763612f1a0a34194cc1`
- **Data directory:** `verbnet3.4/` (329 XML class files)
- **Local use:** read-only copy in the sandbox goldmines tree; never modified.
- **Scanned:** 329 XML files, 609 classes/subclasses, 6,740 member
  occurrences, and 1,602 frame elements.
- **Harvest:** **10,845** deduplicated `VerbNetFrame` rows.

Frames are sorted by `(verb, prep, predicate, class_id)`. Predicates are
derived from VerbNet member `grouping`/`fn_mapping` metadata, class/subclass
identity, and semantic `<PRED>` values. Every row retains its originating
class/subclass id. The table emits the exact required preposition bindings:
`from/out_of/off -> Source`, `to/into/in/onto -> Destination`,
`by -> Extent`, `with/using -> Instrument`, `of/about -> Theme`,
`for -> Recipient`, and `at/on/inside -> Location`.

## Closed-class and morphology data

- Phase 3 `tc-syntax` links the vendored CMU Link Grammar 5.13.2 shared
  library and resolves its English dictionary beside the executable or from
  the compile-time `CARGO_MANIFEST_DIR` fallback. No corpus file is copied.
- `crates/tc-lexicon/src/closed_class.rs` is a hand-audited closed-class table
  of **279** English function words. It is not open-class semantic mapping.
- `goldmines/tools/harvest_wordnet_r1.py` emits the sorted irregular table
  (**103** rows) from standard English irregular forms and the generated
  WordNet verb-lemma vocabulary.
- Regular suffix lemmatization is deterministic and validated against the
  generated WordNet verb-lemma table; ambiguous measure/noun forms are
  explicitly protected from mangling.

## Reproducing

```bash
python3 goldmines/tools/harvest_wordnet_r1.py --data-dir <wordnet-release-dir>
python3 goldmines/tools/harvest_verbnet_r1.py --data-dir <verbnet-repo-dir>
```

Both scripts write only into `crates/tc-lexicon/src/generated/`, sort their
output deterministically, and never write to the source corpus directories.
