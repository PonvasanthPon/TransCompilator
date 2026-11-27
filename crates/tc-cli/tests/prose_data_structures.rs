//! Regressions for sequence-backed linked lists and array-backed trees.

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
fn sequence_backed_list_segment_reversal() {
    assert_program(
        "list-segment",
        "Let values be a list of 1, 2, 3, 4, and 5.\n\
Let left edge be 2.\n\
Let right edge be 4.\n\
Let left be left edge minus 1.\n\
Let right be right edge minus 1.\n\
Repeat while left is less than right, swap item left of values and item right of values, add 1 to left, subtract 1 from right.\n\
\n\
For each value in values, print value.\n",
        &["1", "4", "3", "2", "5"],
    );
}

#[test]
fn array_backed_tree_recursive_inorder() {
    assert_program(
        "array-tree",
        "Keep a shared values as an empty list of whole numbers.\n\
Append 1 to values.\n\
Append 2 to values.\n\
Append 3 to values.\n\
Keep a shared lefts as an empty list of whole numbers.\n\
Append -1 to lefts.\n\
Append 2 to lefts.\n\
Append -1 to lefts.\n\
Keep a shared rights as an empty list of whole numbers.\n\
Append 1 to rights.\n\
Append -1 to rights.\n\
Append -1 to rights.\n\
Keep a shared order as an empty list of whole numbers.\n\
To gather from node (a whole number).\n\
If node is less than 0, return.\n\
Gather from item node of lefts.\n\
Append node to order.\n\
Gather from item node of rights.\n\
\n\
Gather from 0.\n\
For each node in order, print item node of values.\n",
        &["1", "3", "2"],
    );
}
