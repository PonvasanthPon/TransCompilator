//! Regressions for the four general prose combinators.

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
fn unindented_for_loop_with_closer() {
    assert_program(
        "prose-for",
        "Let total be 0.\n\
For every number from 1 to 3.\n\
Add number to total.\n\
At the end, print total.\n",
        &["6"],
    );
}

#[test]
fn unindented_while_loop() {
    assert_program(
        "prose-while",
        "Let count be 0.\n\
While count is less than 3.\n\
Add 1 to count.\n\
Print count.\n",
        &["1", "2", "3"],
    );
}

#[test]
fn then_stop_breaks_inline_if() {
    assert_program(
        "prose-stop",
        "For every number from 1 to 5.\n\
Print number.\n\
If number is 3, print 99, then stop.\n",
        &["1", "2", "3", "99"],
    );
}

#[test]
fn skip_to_next_continues() {
    assert_program(
        "prose-skip",
        "For every number from 1 to 5.\n\
If number is 3, skip to the next one.\n\
Print number.\n",
        &["1", "2", "4", "5"],
    );
}

#[test]
fn english_index_and_min_max_math() {
    assert_program(
        "prose-math",
        "Let values be a list of 10, 20, and 30.\n\
Let last be the last position of values.\n\
Print last.\n\
Let next be one more than last.\n\
Print next.\n\
Let middle be the midpoint of 0 and last.\n\
Print middle.\n\
Let high be the larger of 10 and 20.\n\
Print high.\n\
Let low be the smaller of 10 and 20.\n\
Print low.\n\
Print the first item of values.\n",
        &["2", "3", "1", "20", "10", "10"],
    );
}

#[test]
fn empty_text_append_and_character() {
    assert_program(
        "prose-text",
        "Let result be empty text.\n\
Append \"IV\" to result.\n\
Append \"X\" to result.\n\
Print result.\n\
Print the length of result.\n\
Print character 0 of result.\n",
        &["IVX", "3", "73"],
    );
}

#[test]
fn append_text_and_byte() {
    assert_program(
        "prose-append",
        "Let word be \"abc\".\n\
Let result be empty text.\n\
Append word to result.\n\
Append character 0 of word to result.\n\
Print result.\n",
        &["abca"],
    );
}

#[test]
fn slice_text_between_positions() {
    assert_program(
        "prose-slice",
        "Let word be \"MCMXCIV\".\n\
Print the slice of word from 0 to 3.\n\
Print the slice of word from 3 to 7.\n",
        &["MCM", "XCIV"],
    );
}

#[test]
fn prose_two_sum_with_last_position_and_stop() {
    assert_program(
        "prose-two-sum",
        "Start with the list of numbers 2, 7, 11, and 15.\n\
Let target be 9.\n\
For every first from 0 to the last position of numbers.\n\
For every second from one more than first to the last position of numbers, if item first of numbers plus item second of numbers is target, print first, print second, then stop.\n",
        &["0", "1"],
    );
}

#[test]
fn prose_search_and_roman_and_power() {
    assert_program(
        "prose-search",
        "Start with the list of numbers 4, 5, 6, 7, 0, 1, and 2.\n\
Let target be 0.\n\
Let answer be -1.\n\
For every position from 0 to the last position of numbers.\n\
If item position of numbers is target, set answer to position, then stop.\n\
\n\
Print answer.\n",
        &["4"],
    );
    assert_program(
        "prose-roman",
        "Let word be \"MCMXCIV\".\n\
Let final be one less than the number of characters in word.\n\
Let total be 0.\n\
For every position from 0 to final.\n\
Let current be 0.\n\
Let following be 0.\n\
Let symbol be character position of word.\n\
If symbol is 77, set current to 1000.\n\
If symbol is 68, set current to 500.\n\
If symbol is 67, set current to 100.\n\
If symbol is 76, set current to 50.\n\
If symbol is 88, set current to 10.\n\
If symbol is 86, set current to 5.\n\
If symbol is 73, set current to 1.\n\
Let next position be one more than position.\n\
If position is less than final, set following to character next position of word.\n\
If following is 77, set following to 1000.\n\
If following is 68, set following to 500.\n\
If following is 67, set following to 100.\n\
If following is 76, set following to 50.\n\
If following is 88, set following to 10.\n\
If following is 86, set following to 5.\n\
If following is 73, set following to 1.\n\
If current is less than following, set total to total minus current, otherwise set total to total plus current.\n\
\n\
Print total.\n",
        &["1994"],
    );
    assert_program(
        "prose-power",
        "Let result be 1.0.\n\
Let factor be 2.0.\n\
Let power be 10.\n\
Repeat while power is more than 0.\n\
If power modulo 2 is 1, set result to result times factor.\n\
Set factor to factor times factor.\n\
Set power to power divided by 2.\n\
\n\
Print result.\n",
        &["1024.0000"],
    );
}
