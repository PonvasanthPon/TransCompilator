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
