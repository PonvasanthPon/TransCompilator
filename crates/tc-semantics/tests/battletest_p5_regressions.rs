//! Phase 5 full-stack regression tests.

use tc_editor::runner::run_elf_with_parity;

#[test]
fn assignment_verbs_accept_phrase_expressions() {
    let source = concat!(
        "Use alloc, core, io, list.\n",
        "Let values be List of decimal.\n",
        "Push 1.5 to address of values.\n",
        "Let result be 0.0.\n",
        "Set result to last item of address of values.\n",
        "Say result.\n",
        "Free list address of values.\n",
        "Give back 0.\n",
    );
    let result = run_elf_with_parity(source, None, "");
    assert!(result.ok, "{}", result.summary);
    assert!(
        result.lines.iter().any(|line| line == "1.5000"),
        "{:?}",
        result.lines
    );
}

#[test]
fn paragraph_functions_preserve_indented_control_blocks() {
    let source = concat!(
        "Use io.\n",
        "To pick given a as a whole number, b as a whole number, named pick, returning a whole number.\n",
        "If a is less than b:\n",
        "    Give back a.\n",
        "Give back b.\n\n",
        "Say pick(77, 37).\n",
        "Give back 0.\n",
    );
    let result = run_elf_with_parity(source, None, "");
    assert!(result.ok, "{}", result.summary);
    assert!(
        result.lines.iter().any(|line| line == "37"),
        "{:?}",
        result.lines
    );
}

#[test]
fn int_map_grows_beyond_256_entries() {
    let source = concat!(
        "Use alloc, core, io, map.\n",
        "Let table be IntMap of whole number.\n",
        "Let k be 0.\n",
        "While k is less than 270.\n",
        "Put k into int map address of table for key k.\n",
        "Add 1 to k.\n",
        "End the loop.\n",
        "Let count be count of int map address of table.\n",
        "Say count.\n",
        "Free int map address of table.\n",
        "Let used be bytes in use.\n",
        "If used is not 0, stop with error \"map leak\".\n",
        "Give back 0.\n",
    );
    let result = run_elf_with_parity(source, None, "");
    assert!(result.ok, "{}", result.summary);
    assert!(
        result.lines.iter().any(|line| line == "270"),
        "{:?}",
        result.lines
    );
}

#[test]
fn text_map_grows_beyond_256_entries() {
    let source = concat!(
        "Use alloc, core, io, map, string.\n",
        "Keep a shared key store as array<TcString, 260>.\n",
        "Let table be StrMap of whole number.\n",
        "Let buffer be block of 16 byte.\n",
        "Let k be 0.\n",
        "While k is less than 260.\n",
        "Let length be format k into address of item 0 of buffer.\n",
        "Let key be create text from address of item 0 of buffer of length length.\n",
        "Set item k of key store to key.\n",
        "Put k into text map address of table for text address of item k of key store.\n",
        "Add 1 to k.\n",
        "End the loop.\n",
        "Let count be count of text map address of table.\n",
        "Say count.\n",
        "Free text map address of table.\n",
        "Let j be 0.\n",
        "While j is less than 260.\n",
        "Release text address of item j of key store.\n",
        "Add 1 to j.\n",
        "End the loop.\n",
        "Let used be bytes in use.\n",
        "If used is not 0, stop with error \"text map leak\".\n",
        "Give back 0.\n",
    );
    let result = run_elf_with_parity(source, None, "");
    assert!(result.ok, "{}", result.summary);
    assert!(
        result.lines.iter().any(|line| line == "260"),
        "{:?}",
        result.lines
    );
}
