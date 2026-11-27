//! Regressions for text comparisons, repeated lists, item swaps, prose maps,
//! text splitting/joining, and multi-action branches.

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

#[test]
fn otherwise_and_multi_action_branch() {
    assert_program(
        "otherwise",
        "Let value be 0.\n\
If value is 0, print \"zero\"; otherwise, print \"positive\".\n",
        &["zero"],
    );
    assert_program(
        "multi-action",
        "Let total be 0.\n\
For every number from 1 to 3.\n\
If number is 2, add 10 to total, print number.\n\
Otherwise, add 1 to total.\n\
\n\
Print total.\n",
        &["2", "12"],
    );
}

#[test]
fn natural_map_frequency_prose() {
    assert_program(
        "map-prose",
        "Let counts be an empty map.\n\
Add 1 to entry 1 of counts.\n\
Add 1 to entry 1 of counts.\n\
Set entry 2 of counts to 5.\n\
Print entry 1 of counts.\n\
Print entry 2 of counts.\n\
Print the number of entries in counts.\n\
If counts has entry 2, print \"has\".\n\
If counts does not have entry 3, print \"missing\".\n",
        &["2", "5", "2", "has", "missing"],
    );
}

#[test]
fn split_and_join_text() {
    assert_program(
        "split-join",
        "Let text be empty text.\n\
Append \"a,b,c\" to text.\n\
Let parts be split text by \",\".\n\
Print the number of items in parts.\n\
Let joined be join parts with \"-\".\n\
Print joined.\n",
        &["3", "a-b-c"],
    );
}

#[test]
fn reverse_integer_list() {
    assert_program(
        "reverse-list",
        "Let nums be a list of 1, 2, and 3.\n\
Reverse nums.\n\
For each value in nums, print value.\n",
        &["3", "2", "1"],
    );
}

#[test]
fn otherwise_if_chain_across_lines() {
    assert_program(
        "otherwise-if-zero",
        "Let value be 0.\n\
If value is less than 0, print \"negative\".\n\
Otherwise, if value is 0, print \"zero\".\n\
Otherwise, print \"positive\".\n",
        &["zero"],
    );
    assert_program(
        "otherwise-if-positive",
        "Let value be 5.\n\
If value is less than 0, print \"negative\".\n\
Otherwise, if value is 0, print \"zero\".\n\
Otherwise, print \"positive\".\n",
        &["positive"],
    );
}

#[test]
fn map_entry_values_and_subtraction() {
    assert_program(
        "map-entry",
        "Let counts be an empty map.\n\
Add 1 to entry 1 of counts.\n\
Add 1 to entry 1 of counts.\n\
Set entry 2 of counts to 5.\n\
Subtract 1 from entry 2 of counts.\n\
If entry 2 of counts is 4, print \"four\".\n\
Print entry 9 of counts.\n",
        &["four", "0"],
    );
}

#[test]
fn text_list_items_support_append_and_remove() {
    assert_program(
        "text-list-item",
        "Let rows be an empty list of text.\n\
Append \"\" to rows.\n\
Append \"\" to rows.\n\
Append \"a\" to item 0 of rows.\n\
Append \"!\" to item 0 of rows.\n\
Append \"b\" to item 1 of rows.\n\
Print item 0 of rows.\n\
Print item 1 of rows.\n\
Let nums be a list of 10, 20, and 30.\n\
Remove item 1 of nums.\n\
For each value in nums, print value.\n",
        &["a!", "b", "10", "30"],
    );
}
