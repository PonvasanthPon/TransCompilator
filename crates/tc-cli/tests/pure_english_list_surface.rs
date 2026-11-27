//! Regressions for the no-boilerplate natural list and string surface.

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
fn remove_every_value_and_count_without_boilerplate() {
    assert_program(
        "remove-every",
        "Start with the list of numbers 3, 2, 2, and 3.\n\
Remove every 3 from the list.\n\
Print the number of items in the list.\n\
For each item in the list, print item.\n",
        &["2", "2", "2"],
    );
}

#[test]
fn item_read_write_append_remove_and_sort() {
    assert_program(
        "list-surface",
        "Let nums be a list of 10, 30, and 20.\n\
Print item 1 of nums.\n\
Set item 1 of nums to 99.\n\
Print item 1 of nums.\n\
Append 40 to nums.\n\
Remove the last item from nums.\n\
Remove item 0 from nums.\n\
Sort nums.\n\
For each number in nums, print number.\n",
        &["30", "99", "20", "99"],
    );
}

#[test]
fn multi_word_names_and_unsigned_loop_bound() {
    assert_program(
        "multiword",
        "Let nums be a list of 5, 6, and 7.\n\
Let write index start at 0.\n\
Let left count be 3.\n\
Let n be count of nums.\n\
For every i from 0 to n - 1:\n\
    Print item i of nums.\n\
\n\
Add 1 to write index.\n\
Print left count.\n\
Print write index.\n",
        &["5", "6", "7", "3", "1"],
    );
}

#[test]
fn character_index_without_pointer_plumbing() {
    assert_program(
        "character",
        "Let word be \"abc\".\n\
Let i be 1.\n\
Print character i of word.\n\
If character 0 of word is 97, print \"a\".\n",
        &["98", "a"],
    );
}
