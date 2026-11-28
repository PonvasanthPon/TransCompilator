//! Regressions for writer-English declarations, placement verbs, text copy,
//! character literals, lexicographic ordering, and dynamic spaces.

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
fn writer_declarations_support_compound_and_subject_verb_forms() {
    assert_program(
        "writer-declarations",
        "The word holds \"babad\". The stack starts as an empty list of whole numbers.\n\
Start with the numbers 2, 7, 11, and 15 in numbers, a target of 9, and a carry of 0.\n\
Show target.\n\
Show item 2 of numbers.\n\
Show carry.\n\
Show the length of word.\n\
Show the number of items in stack.\n",
        &["9", "11", "0", "5", "0"],
    );
}

#[test]
fn placement_verbs_and_text_copy() {
    assert_program(
        "writer-placement",
        "The items start as an empty list of whole numbers.\n\
The text is empty text.\n\
Place 3 into items.\n\
Put 4 into items.\n\
Record 5 in items.\n\
Collect 6 in items.\n\
Add 7 onto items.\n\
Place \"a\" into text.\n\
The copy is a copy of text.\n\
Place \"!\" into copy.\n\
Show text.\n\
Show copy.\n\
Show item 4 of items.\n",
        &["a", "a!", "7"],
    );
}

#[test]
fn bitwise_assignment_and_shift_prose() {
    assert_program(
        "writer-bits",
        "The value is 5. The low is 0. The mixed is 0. The moved is 0. The raised is 0. The flipped is 0.\n\
Set low to bitwise and of value and 1.\n\
Set mixed to exclusive or of value and 3.\n\
Set moved to shift value right by 1.\n\
Set raised to shift value left by 1.\n\
Set flipped to bitwise inverse of 0.\n\
Show low.\n\
Show mixed.\n\
Show moved.\n\
Show raised.\n\
Show flipped.\n",
        &["1", "6", "2", "10", "-1"],
    );
}

#[test]
fn character_literals_ordering_and_dynamic_spaces() {
    assert_program(
        "writer-characters",
        "The word is \"a(b)\". The other is \"a(b)\".\n\
For every position from 0 to 3.\n\
If character position of word is \"(\", Show \"open\".\n\
If character position of word is between \"a\" and \"z\", Show \"letter\".\n\
\n\
If word matches other, Show \"same\".\n\
If word comes before \"b\", Show \"before\".\n\
Show 3 spaces.\n",
        &["letter", "open", "letter", "same", "before", "   "],
    );
}
