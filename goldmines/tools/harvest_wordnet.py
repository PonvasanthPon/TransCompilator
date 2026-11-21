#!/usr/bin/env python3
"""Harvest Open English WordNet 2025 (WN-LMF XML) into a static C lexicon.

Pure Python 3 standard library only.  The output is a sorted table of
(lemma, part-of-speech) -> canonical TransCompilator concept with the real
Open English WordNet synset id that justified every entry.
"""
from __future__ import annotations

import gzip
import hashlib
import re
import sys
import xml.etree.ElementTree as ET
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RAW = ROOT / "goldmines" / "raw"
OUT = ROOT / "generated" / "tc_wordnet_lexicon.h"
LOG = ROOT / "logs" / "goldmine_harvest.log"

# concept: (priority, [(anchor lemma, pos)], [gloss keywords])
CONCEPTS = [
    ("CONCEPT_MOD", 1,
     [("modulo", "n"), ("modulus", "n"), ("remainder", "n")],
     ["remainder", "modulo", "modulus", "division"]),
    ("CONCEPT_DIV", 2,
     [("divide", "v"), ("quotient", "n"), ("division", "n")],
     ["divide", "quotient", "division", "fraction"]),
    ("CONCEPT_MUL", 3,
     [("multiply", "v"), ("product", "n"), ("multiplication", "n")],
     ["multiply", "product", "multiplication", "arithmetic"]),
    ("CONCEPT_SUB", 4,
     [("subtract", "v"), ("deduct", "v"), ("decrease", "v"),
      ("diminish", "v"), ("reduce", "v"), ("difference", "n"),
      ("subtraction", "n")],
     ["subtract", "deduct", "decrease", "diminish", "difference",
      "arithmetic", "quantity", "number"]),
    ("CONCEPT_ADD", 5,
     [("add", "v"), ("sum", "v"), ("sum", "n"), ("total", "n"),
      ("addition", "n"), ("increase", "v"), ("augment", "v"),
      ("increment", "v")],
     ["add", "sum", "total", "addition", "increase", "arithmetic",
      "quantity", "number"]),
    ("CONCEPT_HALVE", 6, [("halve", "v")], ["halve", "half", "divide"]),
    ("CONCEPT_DOUBLE", 7, [("double", "v")], ["double", "twice", "multiply"]),
    ("CONCEPT_TRIPLE", 8, [("triple", "v")], ["triple", "threefold", "multiply"]),
    ("CONCEPT_MAX", 9,
     [("maximum", "n"), ("maximum", "a"), ("largest", "a"),
      ("biggest", "a"), ("highest", "a"), ("greatest", "a")],
     ["maximum", "largest", "greatest", "highest", "most"]),
    ("CONCEPT_MIN", 10,
     [("minimum", "n"), ("minimum", "a"), ("smallest", "a"),
      ("lowest", "a"), ("least", "a")],
     ["minimum", "smallest", "least", "lowest"]),
    ("CONCEPT_GT", 11,
     [("exceed", "v"), ("surpass", "v"), ("outstrip", "v"),
      ("greater", "a"), ("bigger", "a"), ("larger", "a"), ("higher", "a")],
     ["greater", "exceed", "surpass", "more", "larger", "higher"]),
    ("CONCEPT_LT", 12,
     [("less", "a"), ("smaller", "a"), ("lower", "a"), ("fewer", "a")],
     ["less", "smaller", "lower", "fewer"]),
    ("CONCEPT_EQ", 13,
     [("equal", "a"), ("same", "a"), ("identical", "a")],
     ["equal", "same", "identical"]),
    ("CONCEPT_ABS", 14, [("absolute", "a")], ["absolute", "value", "magnitude"]),
    ("CONCEPT_NEG", 15, [("negate", "v"), ("invert", "v")],
     ["negate", "invert", "opposite"]),
    ("CONCEPT_SWAP", 16,
     [("swap", "v"), ("exchange", "v"), ("interchange", "v"), ("switch", "v")],
     ["swap", "exchange", "interchange", "replace"]),
    ("CONCEPT_PRINT", 17,
     [("print", "v"), ("display", "v"), ("show", "v"), ("output", "v"),
      ("reveal", "v"), ("report", "v"), ("write", "v"), ("announce", "v"),
      ("exhibit", "v")],
     ["print", "display", "show", "output", "reveal", "report", "write",
      "communicate", "make known", "record"]),
    ("CONCEPT_SET", 18,
     [("set", "v"), ("assign", "v"), ("store", "v"), ("initialize", "v"),
      ("define", "v"), ("record", "v")],
     ["set", "assign", "store", "initialize", "definite", "place"]),
    ("CONCEPT_START", 19,
     [("start", "v"), ("begin", "v"), ("choose", "v"), ("select", "v"),
      ("pick", "v")],
     ["start", "begin", "choose", "select", "pick"]),
    ("CONCEPT_COMPUTE", 20,
     [("compute", "v"), ("calculate", "v"), ("evaluate", "v"),
      ("determine", "v"), ("find", "v")],
     ["compute", "calculate", "evaluate", "determine", "find", "number"]),
    ("CONCEPT_CHECK", 21,
     [("check", "v"), ("test", "v"), ("verify", "v"), ("inspect", "v"),
      ("compare", "v"), ("confirm", "v")],
     ["check", "test", "verify", "inspect", "compare", "confirm"]),
    ("CONCEPT_LOOP", 22,
     [("repeat", "v"), ("iterate", "v"), ("loop", "v"), ("continue", "v")],
     ["repeat", "iterate", "loop", "continue"]),
    ("CONCEPT_EXIT", 23,
     [("exit", "v"), ("terminate", "v"), ("quit", "v"), ("halt", "v"),
      ("stop", "v"), ("return", "v")],
     ["exit", "terminate", "quit", "halt", "stop", "return"]),
    ("CONCEPT_RESULT_NOUN", 24,
     [("result", "n"), ("answer", "n"), ("output", "n"), ("value", "n"),
      ("outcome", "n"), ("number", "n"), ("quantity", "n"), ("amount", "n")],
     ["result", "answer", "value", "outcome", "quantity", "amount"]),
    ("CONCEPT_ARRAY", 25,
     [("list", "n"), ("array", "n"), ("sequence", "n")],
     ["list", "array", "sequence", "ordering"]),
]

# Ultra-ambiguous lemmas are admitted as explicit anchors but are not allowed
# to drag their whole synset in (the stopword guard from the Step 5 contract).
NO_EXPAND = {"set", "show", "start", "make", "get", "put", "take", "give",
             "run", "work", "play", "call", "turn", "move", "hold", "keep",
             "bring", "look", "see", "have", "do", "go", "come", "be",
             "number", "value", "output", "result", "list", "array",
             "greater", "less", "same", "equal", "lower", "higher",
             "bigger", "smaller", "larger", "fewer", "more", "least",
             "out", "figure"}


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def clean_lemma(s: str) -> str:
    return s.strip().lower().replace("_", " ")


def main() -> int:
    src = RAW / "english-wordnet" / "english-wordnet-2025.xml.gz"
    if not src.exists():
        sys.exit(f"missing {src}")
    digest = sha256(src)

    synset_pos: dict[str, str] = {}
    synset_gloss: dict[str, str] = {}
    synset_members: dict[str, list[tuple[str, str]]] = defaultdict(list)
    synset_hyponyms: dict[str, list[str]] = defaultdict(list)
    lemma_synsets: dict[tuple[str, str], list[str]] = defaultdict(list)
    lexical_entries = 0
    synsets = 0

    with gzip.open(src, "rb") as fh:
        for event, el in ET.iterparse(fh, events=("end",)):
            if el.tag == "LexicalEntry":
                lemma_el = el.find("Lemma")
                if lemma_el is None:
                    continue
                lemma = clean_lemma(lemma_el.get("writtenForm", ""))
                pos = lemma_el.get("partOfSpeech", "")
                if not lemma or not re.fullmatch(r"[a-z][a-z' -]*", lemma) or " " in lemma:
                    continue
                lexical_entries += 1
                for sense in el.findall("Sense"):
                    sid = sense.get("synset")
                    if not sid:
                        continue
                    synset_members[sid].append((lemma, pos))
                    lemma_synsets[(lemma, pos)].append(sid)
                el.clear()
            elif el.tag == "Synset":
                sid = el.get("id", "")
                pos = el.get("partOfSpeech", "")
                if not sid:
                    continue
                synsets += 1
                synset_pos[sid] = pos
                for d in el.findall("Definition"):
                    synset_gloss[sid] = (synset_gloss.get(sid, "") + " " +
                                         (d.text or "")).strip().lower()
                for rel in el.findall("SynsetRelation"):
                    if rel.get("relType") == "hyponym":
                        synset_hyponyms[sid].append(rel.get("target", ""))
                el.clear()

    # Deduplicate membership lists.
    for sid in list(synset_members):
        synset_members[sid] = sorted(set(synset_members[sid]))

    def score_synset(sid: str, keywords: list[str], anchor: str) -> int:
        gloss = synset_gloss.get(sid, "")
        score = 0
        for kw in keywords:
            if kw in gloss:
                score += 3
        if anchor in gloss:
            score += 2
        if synset_pos.get(sid) in ("v", "n", "a"):
            score += 1
        return score

    chosen: dict[tuple[str, str], tuple[str, str]] = {}
    conflicts: list[str] = []
    provenance: dict[str, dict] = {}

    def assign(lemma: str, pos: str, concept: str, sid: str) -> None:
        key = (lemma, pos)
        old = chosen.get(key)
        if old and old[0] != concept:
            conflicts.append(f"{lemma}/{pos}: {old[0]}({old[1]}) keeps priority over {concept}({sid})")
            return
        chosen[key] = (concept, sid)

    # 1) Explicit anchors first, in concept priority order.  An anchor is an
    #    authoritative hand-selected sense, so it must never be stolen by a
    #    later synset expansion (e.g. "check" must stay CONCEPT_CHECK even if a
    #    compute synset happens to contain it).
    anchor_synsets: list[tuple[str, str, str, str, list[str]]] = []
    for concept, _priority, anchors, keywords in CONCEPTS:
        prov = provenance.setdefault(concept, {"anchors": 0, "synsets": set(),
                                               "members": 0})
        for lemma, pos in anchors:
            sids = lemma_synsets.get((lemma, pos), [])
            if not sids:
                conflicts.append(f"anchor {lemma}/{pos} not present in OEWN for {concept}")
                continue
            best = max(sids, key=lambda s: (score_synset(s, keywords, lemma), s))
            prov["anchors"] += 1
            prov["synsets"].add(best)
            assign(lemma, pos, concept, best)
            anchor_synsets.append((concept, lemma, pos, best, keywords))

    # 2) Expand the selected anchor synsets.  Only members not already claimed
    #    by an explicit anchor are admitted.
    for concept, lemma, pos, best, keywords in anchor_synsets:
        if lemma in NO_EXPAND:
            continue
        prov = provenance.setdefault(concept, {"anchors": 0, "synsets": set(),
                                               "members": 0})
        for member, mpos in synset_members.get(best, []):
            if mpos != pos:
                continue
            if member in NO_EXPAND:
                continue
            if len(member) < 2 or not member.isalpha():
                continue
            assign(member, mpos, concept, best)
            prov["members"] += 1
        # One hop of hyponyms, only when the hyponym gloss also looks
        # computational; this stays genuinely WordNet-derived.
        for target in synset_hyponyms.get(best, [])[:12]:
            if not any(kw in synset_gloss.get(target, "") for kw in keywords):
                continue
            prov["synsets"].add(target)
            for member, mpos in synset_members.get(target, []):
                if mpos != pos or member in NO_EXPAND:
                    continue
                if len(member) < 2 or not member.isalpha():
                    continue
                assign(member, mpos, concept, target)
                prov["members"] += 1

    entries = sorted(((lemma, pos, concept, sid)
                      for (lemma, pos), (concept, sid) in chosen.items()),
                     key=lambda x: (x[0], x[1]))

    OUT.parent.mkdir(parents=True, exist_ok=True)
    LOG.parent.mkdir(parents=True, exist_ok=True)
    with OUT.open("w", encoding="utf-8") as fh:
        fh.write("/* AUTO-GENERATED by goldmines/tools/harvest_wordnet.py - DO NOT EDIT BY HAND */\n")
        fh.write("/* Source: Open English WordNet 2025 (WN-LMF XML), "
                 f"sha256={digest} */\n")
        fh.write(f"/* Harvested entries: {len(entries)} from {synsets} synsets / "
                 f"{lexical_entries} lexical entries */\n")
        fh.write("#ifndef TC_WORDNET_LEXICON_H\n#define TC_WORDNET_LEXICON_H\n\n")
        fh.write('#include "tc_goldmine.h"\n\n')
        fh.write("static const TCWordNetEntry TC_WORDNET_LEXICON[] = {\n")
        for lemma, pos, concept, sid in entries:
            esc = lemma.replace("\\", "\\\\").replace('"', '\\"')
            fh.write(f'    {{ "{esc}", \'{pos}\', {concept}, "{sid}" }},\n')
        fh.write("};\n")
        fh.write("static const size_t TC_WORDNET_LEXICON_COUNT =\n")
        fh.write("    sizeof(TC_WORDNET_LEXICON) / sizeof(TC_WORDNET_LEXICON[0]);\n\n")
        fh.write("#endif\n")

    samples = ["deduct", "diminish", "reveal", "surpass", "halve", "triple",
               "interchange", "quotient", "remainder", "display", "evaluate"]
    with LOG.open("a", encoding="utf-8") as fh:
        fh.write("\n=== Open English WordNet 2025 harvest ===\n")
        fh.write(f"source: {src.name}\nsha256: {digest}\n")
        fh.write(f"lexical entries scanned: {lexical_entries}\n")
        fh.write(f"synsets scanned: {synsets}\n")
        fh.write(f"unique harvested (lemma,pos) entries: {len(entries)}\n")
        for concept, _p, _a, _k in CONCEPTS:
            prov = provenance.get(concept, {"anchors": 0, "synsets": set(),
                                            "members": 0})
            fh.write(f"  {concept}: anchors={prov['anchors']} "
                     f"synsets={len(prov['synsets'])} expanded_members={prov['members']}\n")
        fh.write("sample provenance:\n")
        for word in samples:
            hits = [e for e in entries if e[0] == word]
            for lemma, pos, concept, sid in hits:
                fh.write(f"  {lemma}/{pos} -> {concept} [{sid}]\n")
        fh.write(f"collision/guard notes: {len(conflicts)}\n")
        for c in conflicts[:40]:
            fh.write(f"  note: {c}\n")
    print(f"harvested {len(entries)} WordNet entries from {synsets} synsets "
          f"({conflicts.__len__()} guard notes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
