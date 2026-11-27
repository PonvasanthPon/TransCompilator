//! Regressions for text comparisons, repeated lists, and item swaps.

use std::path::Path;

use tc_editor::runner::{RunResult, run_elf_with_parity};

fn stdout_lines(result: &RunResult) -> Vec<String> {
    let mut output = Vec::new();
    let mut in_stdout = false;
    for line in &result.lines {
        if line == "--- stdout ---" {
            in_stdout = true;
            continue;
        }
        if in_stdout && line.starts_with("--- ") {
            break;
        }
        if in_stdout {
            output.push(line.clone());
        }
    }
    output
}

fn assert_program(name: &str, source: &str, expected: &[&str]) {
    let result = run_elf_with_parity(source, Some(Path::new(name)), "");
    assert!(result.ok, "{name}: {}", result.lines.join("\n"));
    assert_eq!(stdout_lines(&result), expected, "{name}");
}

#[test]
fn text_and_character_comparisons() {
    assert_program(
        "text-compare",
        "Let word be \"abc\".\n\
Let piece be the slice of word from 0 to 2.\n\
If piece is \"ab\", print \"slice\".\n\
If word isn't equal to \"abd\", print \"word\".\n\
If character 0 of word is \"a\", print \"char\".\n\
If character 1 of word isn't equal to \"c\", print \"char-neq\".\n",
        &["slice", "word", "char", "char-neq"],
    );
}

#[test]
fn repeated_zero_and_value_lists() {
    assert_program(
        "repeated-lists",
        "Let table be a list of 5 zeros.\n\
Print the number of items in table.\n\
For each value in table, print value.\n\
Let copies be a list of 3 copies of 7.\n\
For each value in copies, print value.\n",
        &["5", "0", "0", "0", "0", "0", "7", "7", "7"],
    );
}

#[test]
fn swap_list_items_by_position() {
    assert_program(
        "swap-items",
        "Let nums be a list of 1, 2, 3, and 4.\n\
Swap item 0 of nums and item 3 of nums.\n\
Swap item 1 of nums and item 2 of nums.\n\
For each value in nums, print value.\n",
        &["4", "3", "2", "1"],
    );
}

#[test]
fn repeated_list_supports_dp_updates() {
    assert_program(
        "dp-list",
        "Let ways be a list of 4 zeros.\n\
Set item 0 of ways to 1.\n\
For every position from 1 to 3.\n\
If position is 1, set item 1 of ways to 1.\n\
If position is more than 1, set item position of ways to item (position minus 1) of ways plus item (position minus 2) of ways.\n\
\n\
Print item 3 of ways.\n",
        &["3"],
    );
}
