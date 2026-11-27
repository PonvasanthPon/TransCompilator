//! Regressions for unindented prose recursion, shared state, early return,
//! sorted text characters, and whole-number text formatting.

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
fn recursive_prose_supports_shared_lists_and_early_return() {
    assert_program(
        "recursive-shared-list",
        "Keep a shared seen as an empty list of whole numbers.\n\
Keep a shared total as 0.\n\
To gather from number (a whole number).\n\
If number is 0, return.\n\
Append number to seen.\n\
Add number to total.\n\
Gather from number minus 1.\n\
\n\
Gather from 3.\n\
Print total.\n\
For each value in seen, print value.\n",
        &["6", "3", "2", "1"],
    );
}

#[test]
fn recursive_prose_supports_shared_text_and_backtracking() {
    assert_program(
        "recursive-parentheses",
        "Keep a shared current as empty text.\n\
Keep a shared answers as an empty list of text.\n\
To build from opened (a whole number) with closed (a whole number).\n\
If current's len is 4, let copy be create text from current's data of length current's len, append copy to answers, then return.\n\
If opened is less than 2, append \"(\" to current, build from opened plus 1 with closed, remove last character from current.\n\
If closed is less than opened, append \")\" to current, build from opened with closed plus 1, remove last character from current.\n\
\n\
Build from 0 with 0.\n\
For each answer in answers, print answer.\n",
        &["(())", "()()"],
    );
}

#[test]
fn recursive_prose_function_can_contain_a_paragraph_loop() {
    assert_program(
        "recursive-loop",
        "To show row from number (a whole number).\n\
For every step from 0 to number.\n\
Print step.\n\
\n\
Show row from 2.\n\
Print \"done\".\n",
        &["0", "1", "2", "done"],
    );
}

#[test]
fn sort_characters_format_and_compare_text() {
    assert_program(
        "sorted-text",
        "Let text be \"cba\".\n\
Print sort characters of text.\n\
Let sorted be the sorted characters of \"bca\".\n\
Print sorted.\n\
Let piece be format whole number 42 as text.\n\
Print piece.\n\
Print compare text \"abc\" with text \"abd\".\n",
        &["abc", "abc", "42", "-1"],
    );
}
