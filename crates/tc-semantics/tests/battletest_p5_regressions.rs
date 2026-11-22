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
