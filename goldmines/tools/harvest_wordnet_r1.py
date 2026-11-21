#!/usr/bin/env python3
"""Harvest Open English WordNet 2025 into deterministic Rust tables.

The harvester reads the WN-LMF release read-only, expands a compact set of
exact `oewn-*` concept anchor synsets through hyponym edges up to depth 3, and
emits provenance-carrying Rust tables for `tc-lexicon`.

No open-class English vocabulary is hard-coded in Rust by this process: every
table row records the exact WordNet synset that justified it.
"""
from __future__ import annotations

import argparse
import gzip
import hashlib
import re
import sys
import xml.etree.ElementTree as ET
from collections import defaultdict, deque
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
DEFAULT_OUT = REPO / "crates" / "tc-lexicon" / "src" / "generated"

# Scalar-magnitude verbs required by the finite English statement grammar.
# These are direct depth-zero anchors: they contribute their own synset
# members only, without opening the broad `verb.change` hyponym tree.
DIRECT_ANCHORS = [
    ("Add", "oewn-00156409-v"),       # increase
    ("Sub", "oewn-00151509-v"),       # decrease
]

# One anchor per SemanticConcept.  The two extra anchors for Create and
# Destroy keep the closely-related `build`/`create` and
# `destroy`/`delete` senses precise without opening broad hypernym trees.
# Every id was verified in the 2025 OEWN release.
ANCHORS = [
    ("Add", "oewn-00642490-v"),
    ("Add", "oewn-01029183-v"),       # append / add further
    ("Sub", "oewn-00642914-v"),
    ("Mul", "oewn-00643334-v"),
    ("Div", "oewn-00643760-v"),
    ("Mod", "oewn-13755803-n"),
    ("Negate", "oewn-00471015-v"),
    ("Double", "oewn-00247103-v"),
    ("Halve", "oewn-00644019-v"),
    ("Assign", "oewn-01496967-v"),
    ("Transfer", "oewn-02225243-v"),
    ("Swap", "oewn-02263812-v"),
    ("CompareGt", "oewn-02675266-v"),
    ("CompareLt", "oewn-01559809-a"),
    ("CompareEq", "oewn-00893124-a"),
    ("CompareNe", "oewn-02672316-v"),
    ("Create", "oewn-01658171-v"),
    ("Create", "oewn-01757596-v"),
    ("Destroy", "oewn-01622948-v"),
    ("Destroy", "oewn-01551969-v"),
    ("Output", "oewn-01625832-v"),
    ("Input", "oewn-02214901-v"),
    ("Loop", "oewn-00960366-v"),
    ("Branch", "oewn-00676526-v"),
    ("Return", "oewn-02314987-v"),
]

# These are prose/motion/consumption/emotion roots deliberately rejected as
# computational predicates.  Filtering members (rather than blanketing every
# synset) keeps useful neighbours such as receive while excluding generic
# narrative verbs and the ambiguous adjective/adverb senses from the
# prototype.
EXCLUDED_LEMMAS = {
    "be", "have", "do", "go", "come", "get", "run", "walk", "eat", "drink",
    "sleep", "love", "wait", "rain", "watch", "very", "last", "laugh",
    "smile", "bake", "cook", "travel", "swim",
    # Inflected forms are included defensively; WordNet also lists some of
    # these as independent adjective/noun lemmas.
    "walked", "rained", "slept", "drank", "waited", "loved", "laughed",
    "smiled", "baked", "cooked", "traveled", "travelled", "swam",
    # Ambiguous lexical material that must not be suffix-mangled or mapped.
    "plus", "times", "minus", "equals", "exists", "series", "bus",
    "gas", "class",
    # Keep the direct scalar-magnitude anchors precise to increase/decrease.
    "diminish", "lessen", "fall",
}

# Standard English irregular verb/adjective forms.  The harvester verifies
# determinism and emits these as a sorted Rust lookup table.  This is
# morphology, not semantic mapping.
IRREGULAR = {
    "am": "be", "are": "be", "was": "be", "were": "be", "been": "be",
    "being": "be", "is": "be",
    "has": "have", "had": "have", "having": "have",
    "does": "do", "did": "do", "done": "do", "doing": "do",
    "goes": "go", "went": "go", "gone": "go",
    "bought": "buy", "brought": "bring", "built": "build",
    "became": "become", "began": "begin", "begun": "begin",
    "broke": "break", "broken": "break",
    "came": "come", "cut": "cut",
    "drank": "drink", "drunk": "drink",
    "drew": "draw", "drawn": "draw",
    "ate": "eat", "eaten": "eat",
    "fell": "fall", "fallen": "fall",
    "felt": "feel", "found": "find", "fought": "fight",
    "flew": "fly", "flown": "fly", "forgot": "forget", "forgotten": "forget",
    "gave": "give", "given": "give",
    "got": "get", "gotten": "get",
    "grew": "grow", "grown": "grow",
    "held": "hold", "kept": "keep",
    "knew": "know", "known": "know",
    "laid": "lay", "led": "lead", "left": "leave", "lent": "lend",
    "lost": "lose", "made": "make", "meant": "mean", "met": "meet",
    "paid": "pay", "put": "put", "read": "read", "ridden": "ride",
    "rode": "ride", "ran": "run", "said": "say", "saw": "see",
    "seen": "see", "sent": "send", "set": "set", "shown": "show",
    "showed": "show", "slept": "sleep", "spoke": "speak",
    "spoken": "speak", "spent": "spend", "split": "split",
    "stood": "stand", "stole": "steal", "stolen": "steal",
    "swept": "sweep", "swam": "swim", "swum": "swim",
    "taken": "take", "took": "take", "taught": "teach",
    "told": "tell", "thought": "think", "threw": "throw",
    "thrown": "throw", "understood": "understand", "woke": "wake",
    "woken": "wake", "wore": "wear", "worn": "wear", "won": "win",
    "wrote": "write", "written": "write",
    "better": "good", "best": "good", "worse": "bad", "worst": "bad",
}

POS_MAP = {"v": "Verb", "n": "Noun", "a": "Adj", "s": "Adj", "r": "Adv"}
POS_ORDER = {"Verb": 0, "Noun": 1, "Adj": 2, "Adv": 3}
CONCEPT_ORDER = {
    name: index for index, name in enumerate([
        "Add", "Sub", "Mul", "Div", "Mod", "Negate", "Double", "Halve",
        "Assign", "Transfer", "Swap", "CompareGt", "CompareLt", "CompareEq",
        "CompareNe", "Create", "Destroy", "Output", "Input", "Loop",
        "Branch", "Return",
    ])
}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def rust_string(value: str) -> str:
    return '"' + value.replace("\\", "\\\\").replace('"', '\\"') + '"'


def locate_wordnet(data_dir: Path) -> Path:
    candidates = [
        data_dir,
        data_dir / "english-wordnet-2025.xml.gz",
        data_dir / "english-wordnet" / "english-wordnet-2025.xml.gz",
    ]
    for candidate in candidates:
        if candidate.is_file() and candidate.suffix == ".gz":
            return candidate
    raise SystemExit(
        f"could not find english-wordnet-2025.xml.gz beneath {data_dir}"
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--data-dir", required=True, type=Path)
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT)
    args = parser.parse_args()

    source = locate_wordnet(args.data_dir)
    digest = sha256(source)

    synset_pos: dict[str, str] = {}
    members: dict[str, list[tuple[str, str]]] = defaultdict(list)
    hyponyms: dict[str, list[str]] = defaultdict(list)
    lexical_entries = 0
    synset_count = 0
    verb_lemmas: set[str] = set()
    lemma_re = re.compile(r"^[a-z][a-z'-]*$")

    with gzip.open(source, "rb") as handle:
        for event, element in ET.iterparse(handle, events=("end",)):
            if element.tag == "LexicalEntry":
                lemma_el = element.find("Lemma")
                if lemma_el is not None:
                    lemma = (lemma_el.get("writtenForm") or "").lower().replace("_", " ")
                    pos = lemma_el.get("partOfSpeech") or ""
                    if lemma and pos:
                        lexical_entries += 1
                        if pos == "v" and lemma_re.fullmatch(lemma):
                            verb_lemmas.add(lemma)
                        for sense in element.findall("Sense"):
                            synset = sense.get("synset")
                            if synset:
                                members[synset].append((lemma, pos))
                element.clear()
            elif element.tag == "Synset":
                synset = element.get("id") or ""
                pos = element.get("partOfSpeech") or ""
                if synset:
                    synset_count += 1
                    synset_pos[synset] = pos
                    for relation in element.findall("SynsetRelation"):
                        if relation.get("relType") == "hypernym":
                            target = relation.get("target") or ""
                            if target:
                                hyponyms[target].append(synset)
                element.clear()

    all_anchors = ANCHORS + DIRECT_ANCHORS
    anchor_by_id: dict[str, str] = {}
    for concept, synset in all_anchors:
        if synset not in synset_pos:
            raise SystemExit(f"anchor synset missing from OEWN: {synset} ({concept})")
        anchor_by_id[synset] = concept
    direct_ids = {synset for _, synset in DIRECT_ANCHORS}

    # Multi-source BFS over hyponym edges.  Every synset keeps the shortest
    # distance to an anchor; ties follow anchor declaration order.
    owner: dict[str, tuple[str, int]] = {}
    queue: deque[tuple[str, int]] = deque()
    for concept, synset in all_anchors:
        if synset not in owner:
            owner[synset] = (concept, 0)
            queue.append((synset, 0))
    while queue:
        synset, depth = queue.popleft()
        concept = owner[synset][0]
        if depth >= 3 or synset in direct_ids:
            continue
        for child in hyponyms.get(synset, []):
            if child not in owner:
                owner[child] = (concept, depth + 1)
                queue.append((child, depth + 1))

    best: dict[tuple[str, str, str], tuple[int, str]] = {}
    for synset, (concept, depth) in owner.items():
        for lemma, pos_letter in members.get(synset, []):
            if lemma in EXCLUDED_LEMMAS or not lemma_re.fullmatch(lemma):
                continue
            pos = POS_MAP.get(pos_letter)
            if pos is None or pos_letter != synset_pos.get(synset):
                continue
            key = (lemma, pos, concept)
            candidate = (depth, synset)
            if key not in best or candidate < best[key]:
                best[key] = candidate

    entries = [
        (lemma, pos, concept, synset, depth)
        for (lemma, pos, concept), (depth, synset) in best.items()
    ]
    entries.sort(key=lambda item: (
        item[0], POS_ORDER[item[1]], CONCEPT_ORDER[item[2]], item[4], item[3]
    ))

    out_dir = args.out_dir
    out_dir.mkdir(parents=True, exist_ok=True)
    wordnet_path = out_dir / "wordnet_concepts.rs"
    with wordnet_path.open("w", encoding="utf-8") as handle:
        handle.write("//! AUTO-GENERATED by goldmines/tools/harvest_wordnet_r1.py.\n")
        handle.write("//! Source: Open English WordNet 2025 (WN-LMF), CC-BY 4.0.\n")
        handle.write(f"//! SHA-256: {digest}\n")
        handle.write(f"//! Anchors: {len(all_anchors)}; harvested entries: {len(entries)}.\n")
        handle.write("//! Rows are sorted by (lemma, pos, concept); depth <= 3.\n\n")
        handle.write("use crate::{Pos, SemanticConcept, WordNetEntry};\n\n")
        handle.write("/// WordNet-derived computational vocabulary.\n")
        handle.write("#[rustfmt::skip]\n")
        handle.write("pub static WORDNET_ENTRIES: &[WordNetEntry] = &[\n")
        for lemma, pos, concept, synset, depth in entries:
            handle.write(
                f"    WordNetEntry::new({rust_string(lemma)}, Pos::{pos}, "
                f"SemanticConcept::{concept}, {rust_string(synset)}, {depth}),\n"
            )
        handle.write("];\n\n")
        handle.write(f"/// Number of generated WordNet rows.\n")
        handle.write(f"pub const WORDNET_ENTRY_COUNT: usize = {len(entries)};\n")

    irregular = sorted((form, lemma) for form, lemma in IRREGULAR.items())
    known_verbs = sorted(verb_lemmas)
    morphology_path = out_dir / "morphology.rs"
    with morphology_path.open("w", encoding="utf-8") as handle:
        handle.write("//! AUTO-GENERATED irregular English morphology table.\n")
        handle.write("//! Sources: OEWN 2025 verb lemmas + standard irregular forms.\n\n")
        handle.write("/// Irregular inflected form -> lemma.\n")
        handle.write("#[rustfmt::skip]\n")
        handle.write("pub static IRREGULAR_LEMMAS: &[(&str, &str)] = &[\n")
        for form, lemma in irregular:
            handle.write(f"    ({rust_string(form)}, {rust_string(lemma)}),\n")
        handle.write("];\n\n")
        handle.write(f"/// Number of irregular rows.\n")
        handle.write(f"pub const IRREGULAR_LEMMA_COUNT: usize = {len(irregular)};\n\n")
        handle.write("/// Sorted single-word English verb lemmas used for morphology validation.\n")
        handle.write("#[rustfmt::skip]\n")
        handle.write("pub static VERB_LEMMAS: &[&str] = &[\n")
        for lemma in known_verbs:
            handle.write(f"    {rust_string(lemma)},\n")
        handle.write("];\n\n")
        handle.write("/// Number of known verb lemmas.\n")
        handle.write(f"pub const VERB_LEMMA_COUNT: usize = {len(known_verbs)};\n")

    print(f"WordNet lexical entries: {lexical_entries}")
    print(f"WordNet synsets: {synset_count}")
    print(f"anchors: {len(all_anchors)}")
    print(f"reachable synsets: {len(owner)}")
    print(f"generated WordNet rows: {len(entries)}")
    print(f"irregular lemmas: {len(irregular)}")
    print(f"known verb lemmas: {len(known_verbs)}")
    print(f"wrote {wordnet_path}")
    print(f"wrote {morphology_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
