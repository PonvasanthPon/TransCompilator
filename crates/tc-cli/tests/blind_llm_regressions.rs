//! Blind-LLM regression corpus: natural English programs written without
//! knowledge of the compiler's internal subset.

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
    let result = run_elf_with_parity(source, None, "");
    assert!(
        result.ok,
        "{name}: {}\n{}",
        result.summary,
        result.lines.join("\n")
    );
    assert_eq!(stdout_lines(&result), expected, "{name}");
}

const FILTERED_RANGE_PIPELINE: &str = "Take all the numbers between 1 and 30. Skip any multiple of 4, unless it is also divisible by 6. For each remaining number, if it is odd, triple it and add 1; otherwise cut it in half. Print the final value only if it ends up greater than 20, and keep track of the largest value you printed. Once you finish going through all of them, print that largest value.\n";

#[test]
fn filtered_range_pipeline_parity_and_exact_output() {
    assert_program(
        "filtered-range-pipeline",
        FILTERED_RANGE_PIPELINE,
        &[
            "22", "28", "34", "40", "46", "52", "58", "64", "70", "76", "82", "88", "88",
        ],
    );
}

#[test]
fn filtered_range_pipeline_generalizes_values_and_nouns() {
    assert_program(
        "values-and-divisor",
        "Take all the values between 10 and 20. Skip any multiple of 6, unless it is also divisible by 4. For each remaining value, if it is odd, triple it and add 1; otherwise cut it in half. Print the final value only if it ends up greater than 12, and keep track of the largest value you printed. Once you finish going through all of them, print that largest value.\n",
        &["34", "40", "46", "52", "58", "58"],
    );
}

#[test]
fn filtered_range_pipeline_generalizes_filter_verb_and_phrasing() {
    assert_program(
        "ignore-divisible",
        "Go through all the numbers between 1 and 15. Ignore any number that is divisible by 3, unless it is also even. For each remaining number, if it is odd, triple it and add 1. Otherwise cut it in half. Print the final value only if it ends up greater than 10, and keep track of the largest value you printed. Once you finish going through all of them, print that largest value.\n",
        &["16", "22", "34", "40", "40"],
    );
}

#[test]
fn largest_value_referent_without_tracker_is_a_clean_anaphor_error() {
    let result = run_elf_with_parity(
        "Take all the numbers between 1 and 5. Print that largest value.\n",
        None,
        "",
    );
    assert!(!result.ok);
    assert!(
        result
            .lines
            .iter()
            .any(|line| line.contains("UNRESOLVED_ANAPHOR")
                && line.contains("no preceding `keep track of the largest`")),
        "{}",
        result.lines.join("\n")
    );
}

#[test]
fn tracking_minimum_uses_the_smaller_emitted_value() {
    assert_program(
        "minimum-tracker",
        "Take all the numbers between 4 and 9. Print the final value, and keep track of the smallest value you printed. Once you finish going through all of them, print that smallest value.\n",
        &["4", "5", "6", "7", "8", "9", "4"],
    );
}
