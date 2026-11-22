//! Convergence campaign permanent regressions.

use tc_editor::runner::run_elf_with_parity;

#[test]
fn compound_put_chains_share_the_new_referent() {
    let source = concat!(
        "Use io.\n",
        "Put 100 in cash, add 50 to it, remove 20 from it, and say it.\n",
        "Give back 0.\n",
    );
    let result = run_elf_with_parity(source, None, "");
    assert!(result.ok, "{}", result.summary);
    assert!(
        result.lines.iter().any(|line| line == "130"),
        "{:?}",
        result.lines
    );
}

#[test]
fn say_accepts_a_direct_string_literal() {
    let source = concat!(
        "Use io.\n",
        "Let cash be 600.\n",
        "If cash is more than 500, say \"good\".\n",
        "Say \"plain text\".\n",
        "Give back 0.\n",
    );
    let result = run_elf_with_parity(source, None, "");
    assert!(result.ok, "{}", result.summary);
    assert!(
        result.lines.iter().any(|line| line == "good"),
        "{:?}",
        result.lines
    );
    assert!(
        result.lines.iter().any(|line| line == "plain text"),
        "{:?}",
        result.lines
    );
}

#[test]
fn output_synonyms_resolve_across_standard_types() {
    let source = concat!(
        "Use alloc, core, io, string.\n",
        "Let name be create text from \"hello\" of length 5.\n",
        "Show 42.\n",
        "Print 4.5.\n",
        "Display true.\n",
        "Say address of name.\n",
        "Show address of name.\n",
        "Print address of name.\n",
        "Display address of name.\n",
        "Release text address of name.\n",
        "Give back 0.\n",
    );
    let result = run_elf_with_parity(source, None, "");
    assert!(result.ok, "{}", result.summary);
    assert!(
        result.lines.iter().any(|line| line == "42"),
        "{:?}",
        result.lines
    );
    assert!(
        result.lines.iter().any(|line| line == "4.5000"),
        "{:?}",
        result.lines
    );
    assert!(
        result.lines.iter().any(|line| line == "true"),
        "{:?}",
        result.lines
    );
    assert!(
        result.lines.iter().filter(|line| *line == "hello").count() == 4,
        "{:?}",
        result.lines
    );
}

#[test]
fn compound_set_chains_share_the_new_referent() {
    let source = concat!(
        "Use io.\n",
        "Set cash to 100, add 50 to it, remove 25 from it, and say it.\n",
        "Give back 0.\n",
    );
    let result = run_elf_with_parity(source, None, "");
    assert!(result.ok, "{}", result.summary);
    assert!(
        result.lines.iter().any(|line| line == "125"),
        "{:?}",
        result.lines
    );
}

#[test]
fn set_expressions_with_parenthesized_operands_stay_one_action() {
    let source = concat!(
        "Use alloc, core, io, map.\n",
        "Let value be 7.\n",
        "Set value to exclusive or of value and (shift value right by 1).\n",
        "Say value.\n",
        "Let table be IntMap of whole number.\n",
        "Put value into int map address of table for key 3.\n",
        "Let found be look up 3 in int map address of table.\n",
        "Say found.value.\n",
        "Free int map address of table.\n",
        "Give back 0.\n",
    );
    let result = run_elf_with_parity(source, None, "");
    assert!(result.ok, "{}", result.summary);
    assert!(
        result.lines.iter().filter(|line| **line == "4").count() == 2,
        "{:?}",
        result.lines
    );
}
