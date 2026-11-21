#!/usr/bin/env python3
"""Harvest CU Boulder VerbNet 3.4 into deterministic semantic frame tables.

Reads all 329 VerbNet class XML files read-only, walks class/subclass
inheritance, extracts members, syntax prepositions, thematic roles and
semantic PRED names, and maps them to the closed VerbPredicate set with the
originating VerbNet class id as provenance.
"""
from __future__ import annotations

import argparse
import re
import sys
import xml.etree.ElementTree as ET
from collections import defaultdict
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
DEFAULT_OUT = REPO / "crates" / "tc-lexicon" / "src" / "generated"

PRED_ORDER = {
    "TransferAdd": 0,
    "TransferSub": 1,
    "TransferMove": 2,
    "Assign": 3,
    "Create": 4,
    "Destroy": 5,
    "Emit": 6,
    "Receive": 7,
    "Combine": 8,
    "Transform": 9,
}

# Narrative, motion, consumption, emotion and weather material is excluded.
# `get` is retained because VerbNet's obtain/get classes are genuine Receive
# predicates; `have`/`be`/`do`/`go`/`come`/`run` remain excluded as generic
# light/heavy verbs.
EXCLUDED_LEMMAS = {
    "be", "have", "do", "go", "come", "run", "walk", "eat", "drink", "sleep",
    "love", "wait", "rain", "watch", "very", "last", "laugh", "smile", "bake",
    "cook", "travel", "swim",
}

SUB_PREFIXES = (
    "remove-10.1", "banish-10.2", "steal-10.5", "rob-10.6.4",
    "cheat-10.6.1", "fire-10.10", "consume-66", "pay-68", "withdraw-82",
    "resign-10.11",
)
ASSIGN_PREFIXES = ("put-9.1", "keep-15.2", "future_having-13.3")
CREATE_PREFIXES = ("create-26.4", "build-26.1", "engender-27.1")
DESTROY_PREFIXES = ("destroy-44",)
EMIT_PREFIXES = (
    "say-37.7", "indicate-78", "transfer_mesg-37.1.1", "scribble-25.2",
    "reflexive_appearance-48.1.2", "pronounce-29.3.1",
)
RECEIVE_PREFIXES = (
    "obtain-13.5.2", "get-13.5.1", "learn-14", "comprehend-87.2",
    "accept-77.1",
)
MOVE_PREFIXES = ("give-13.1", "send-11.1")
COMBINE_PREFIXES = ("mix-22.1", "amalgamate-22.2")
TRANSFORM_PREFIXES = (
    "multiply-108", "adjust-26.9", "convert-26.6.2", "other_cos-45.4",
    "calibratable_cos-45.6",
)

SYNTHETIC_PREPS = {
    "TransferSub": (("from", "Source"),),
    "TransferAdd": (("to", "Destination"), ("into", "Destination")),
    "Combine": (
        ("to", "Destination"), ("into", "Destination"), ("with", "Instrument")
    ),
    "TransferMove": (("from", "Source"), ("to", "Destination")),
    "Receive": (("from", "Source"),),
    "Emit": (("to", "Recipient"), ("for", "Recipient")),
    "Assign": (),
    "Create": (),
    "Destroy": (),
    "Transform": (
        ("by", "Extent"), ("with", "Instrument"), ("to", "Destination")
    ),
}


def rust_string(value: str) -> str:
    return '"' + value.replace("\\", "\\\\").replace('"', '\\"') + '"'


def locate_verbnet(data_dir: Path) -> Path:
    candidates = [data_dir, data_dir / "verbnet3.4", data_dir / "verbnet"]
    for candidate in candidates:
        if candidate.is_dir() and any(candidate.glob("*.xml")):
            return candidate
    raise SystemExit(f"could not find VerbNet 3.4 XML classes beneath {data_dir}")


def any_prefix(tokens: str, prefixes: tuple[str, ...]) -> bool:
    return any(token.startswith(prefixes) for token in tokens.split())


def predicate_for(
    verb: str,
    class_id: str,
    grouping: str,
    fn_mapping: str,
    preds: set[str],
) -> str | None:
    group = (grouping or "").lower()
    fn = (fn_mapping or "").lower()
    cls = class_id.lower()

    # Member-level grouping/fn metadata is the most precise provenance.
    if any_prefix(group, ("delete.", "erase.", "destroy.")) or "destroying" in fn:
        return "Destroy"
    if any_prefix(group, ("deposit.",)):
        return "TransferAdd"
    if any_prefix(group, ("subtract.", "deduct.", "remove.", "withdraw.", "spend.")):
        return "TransferSub"
    if any_prefix(group, ("add.",)):
        return "TransferAdd"
    if any_prefix(group, ("combine.",)):
        return "Combine"
    if any_prefix(
        group, ("store.", "keep.", "assign.", "put.", "place.", "set.")
    ):
        return "Assign"
    if any_prefix(
        group, ("create.", "build.", "construct.", "produce.", "make.")
    ):
        return "Create"
    if any_prefix(
        group, ("say.", "tell.", "show.", "print.", "display.")
    ):
        return "Emit"
    if any_prefix(
        group, ("obtain.", "receive.", "accept.", "read.", "get.")
    ):
        return "Receive"
    if any_prefix(group, ("give.", "send.", "transfer.", "move.")):
        return "TransferMove"
    if any_prefix(group, ("multiply.", "double.", "divide.", "halve.")):
        return "Transform"

    # Class hierarchy / class-id families.
    if cls.startswith(SUB_PREFIXES):
        return "TransferSub"
    if cls.startswith(ASSIGN_PREFIXES):
        return "Assign"
    if cls.startswith(CREATE_PREFIXES):
        return "Create"
    if cls.startswith(DESTROY_PREFIXES):
        return "Destroy"
    if cls.startswith(EMIT_PREFIXES):
        return "Emit"
    if cls.startswith(RECEIVE_PREFIXES):
        return "Receive"
    if cls.startswith(MOVE_PREFIXES):
        return "TransferMove"
    if cls.startswith("contribute-13.2"):
        return "TransferAdd"
    if cls.startswith(COMBINE_PREFIXES):
        return "Combine"
    if cls.startswith(TRANSFORM_PREFIXES):
        return "Transform"

    # Semantic PRED fallback.  This is VerbNet data, not a verb lookup list.
    if preds & {"transfer_info", "has_information", "emit"}:
        return "Emit"
    return None


def role_for(prep: str, predicate: str) -> str:
    prep = prep.lower()
    if prep in ("from", "out_of", "off"):
        return "Source"
    if prep in ("into", "onto", "in"):
        return "Destination"
    if prep == "to":
        if predicate in {
            "TransferAdd", "Combine", "TransferMove", "Assign", "Transform"
        }:
            return "Destination"
        return "Recipient"
    if prep == "by":
        return "Extent"
    if prep in ("with", "using"):
        return "Instrument"
    if prep in ("of", "about"):
        return "Theme"
    if prep == "for":
        return "Recipient"
    if prep in ("at", "on", "inside"):
        return "Location"
    return "Theme"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--data-dir", required=True, type=Path)
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT)
    args = parser.parse_args()

    source = locate_verbnet(args.data_dir)
    files = sorted(source.glob("*.xml"))
    frames: set[tuple[str, str, str, str]] = set()
    classes: set[str] = set()
    member_occurrences = 0
    frame_elements = 0
    parse_errors = 0
    verb_re = re.compile(r"^[a-z][a-z'-]*$")

    def scan(
        element: ET.Element,
        inherited_members: list[tuple[str, str, str]],
        inherited_frames: list[tuple[frozenset[str], frozenset[str]]],
        inherited_class: str | None,
    ) -> None:
        nonlocal member_occurrences, frame_elements
        class_id = element.get("ID") or inherited_class or "unknown"
        classes.add(class_id)
        members = list(inherited_members)
        members_el = element.find("MEMBERS")
        if members_el is not None:
            for member in members_el.findall("MEMBER"):
                name = (member.get("name") or "").strip().lower().replace("_", " ")
                if name:
                    members.append(
                        (
                            name,
                            member.get("grouping") or "",
                            member.get("fn_mapping") or "",
                        )
                    )
                    member_occurrences += 1

        local_frames: list[tuple[frozenset[str], frozenset[str]]] = []
        frames_el = element.find("FRAMES")
        if frames_el is not None:
            for frame in frames_el.findall("FRAME"):
                preps: set[str] = set()
                syntax = frame.find("SYNTAX")
                if syntax is not None:
                    for prep_el in syntax.iter("PREP"):
                        values = (prep_el.get("value") or "").split()
                        for value in values:
                            value = value.strip().lower().lstrip("?")
                            if value:
                                preps.add(value)
                preds: set[str] = set()
                semantics = frame.find("SEMANTICS")
                if semantics is not None:
                    for pred in semantics.iter("PRED"):
                        value = pred.get("value")
                        if value:
                            preds.add(value)
                local_frames.append((frozenset(preps), frozenset(preds)))
                frame_elements += 1

        effective_frames = inherited_frames + local_frames
        actual_preps: set[str] = set()
        all_preds: set[str] = set()
        for preps, preds in effective_frames:
            actual_preps |= preps
            all_preds |= preds

        for verb, grouping, fn_mapping in members:
            if verb in EXCLUDED_LEMMAS or not verb_re.fullmatch(verb):
                continue
            predicate = predicate_for(
                verb, class_id, grouping, fn_mapping, all_preds
            )
            if predicate is None:
                continue
            frames.add((verb, "", predicate, class_id))
            for prep, role in SYNTHETIC_PREPS.get(predicate, ()):
                frames.add((verb, prep, predicate, class_id))
            for prep in actual_preps:
                role = role_for(prep, predicate)
                frames.add((verb, prep, predicate, class_id))

        subclasses = element.find("SUBCLASSES")
        if subclasses is not None:
            for child in subclasses.findall("VNSUBCLASS"):
                scan(child, members, effective_frames, class_id)

    for path in files:
        try:
            root = ET.parse(path).getroot()
        except ET.ParseError:
            parse_errors += 1
            continue
        scan(root, [], [], None)

    ordered = sorted(
        frames,
        key=lambda frame: (
            frame[0], frame[1], PRED_ORDER[frame[2]], frame[3]
        ),
    )

    out_dir = args.out_dir
    out_dir.mkdir(parents=True, exist_ok=True)
    target = out_dir / "verbnet_frames.rs"
    with target.open("w", encoding="utf-8") as handle:
        handle.write("//! AUTO-GENERATED by goldmines/tools/harvest_verbnet_r1.py.\n")
        handle.write("//! Source: CU Boulder VerbNet 3.4, 329 class XML files.\n")
        handle.write(
            "//! Sorted by (verb, prep, predicate, class_id), deduplicated.\n"
        )
        handle.write(
            f"//! Classes/subclasses: {len(classes)}; frames: {len(ordered)}.\n\n"
        )
        handle.write(
            "use crate::{PrepRole, VerbNetFrame, VerbPredicate};\n\n"
        )
        handle.write("/// VerbNet-derived semantic predicate frames.\n")
        handle.write("#[rustfmt::skip]\n")
        handle.write("pub static VERBNET_FRAMES: &[VerbNetFrame] = &[\n")
        for verb, prep, predicate, class_id in ordered:
            handle.write(
                f"    VerbNetFrame::new({rust_string(verb)}, {rust_string(prep)}, "
                f"PrepRole::{role_for(prep, predicate)}, "
                f"VerbPredicate::{predicate}, {rust_string(class_id)}),\n"
            )
        handle.write("];\n\n")
        handle.write("/// Number of generated VerbNet frames.\n")
        handle.write(f"pub const VERBNET_FRAME_COUNT: usize = {len(ordered)};\n")

    print(f"VerbNet XML files: {len(files)} (parse errors: {parse_errors})")
    print(f"classes/subclasses: {len(classes)}")
    print(f"member occurrences: {member_occurrences}")
    print(f"frame elements: {frame_elements}")
    print(f"generated frames: {len(ordered)}")
    print(f"wrote {target}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
