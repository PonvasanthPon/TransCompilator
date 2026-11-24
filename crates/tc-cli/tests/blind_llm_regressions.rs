//! Blind-LLM regression corpus: natural English programs written without
//! knowledge of the compiler's internal subset.

use tc_editor::runner::{RunResult, run_elf_with_parity};
use tc_ir::Stmt;
use tc_semantics::{SourceFile, compile_english_to_ir};

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

const FIBONACCI_PIPELINE: &str = "Start with 1 and 2. Keep generating the next number by adding the previous two numbers together, as long as the new number does not exceed 200. For each number in this entire sequence, including the starting two, add it to a running total. Whenever that total goes over 100, subtract 100 from it and count an overflow. At the end, print the final running total and how many overflows occurred.\n";

const TWO_BOX_PIPELINE: &str = "Imagine a red box starting with 5 and a blue box starting with 16. Walk through every number from 1 up to 40. If the number is a multiple of 3, move 4 from the blue box into the red box; otherwise move 1 from the red box into the blue box. However, if a move would ever cause either box to drop below 5, skip that move and leave both boxes unchanged. Stop immediately as soon as the red box holds more than twice as much as the blue box. Finally, print the number where you stopped, followed by the amounts in the red box and the blue box.\n";

const ADJACENT_PASS_PIPELINE: &str = "Start with the list of numbers 14, 7, 22, 9, 15, 4, and 18. Go through the list from left to right, comparing each number with the one immediately after it. Whenever the left number is larger than the right one, swap their positions and count a swap. After finishing that pass, remove any number from the list that is divisible by 3. Finally, print the remaining numbers in order, followed by the total number of swaps made.\n";

fn count_core7_control_flow(statements: &[Stmt], loops: &mut usize, branches: &mut usize) {
    for statement in statements {
        match statement {
            Stmt::Loop { body } => {
                *loops += 1;
                count_core7_control_flow(body, loops, branches);
            }
            Stmt::If {
                then_body,
                else_body,
                ..
            } => {
                *branches += 1;
                count_core7_control_flow(then_body, loops, branches);
                count_core7_control_flow(else_body, loops, branches);
            }
            _ => {}
        }
    }
}

#[test]
fn retained_rounds_one_to_four_lower_to_runtime_core7_control_flow() {
    let programs = [
        ("filtered-range", FILTERED_RANGE_PIPELINE),
        ("fibonacci-sequence", FIBONACCI_PIPELINE),
        ("two-box-simulation", TWO_BOX_PIPELINE),
        ("adjacent-list-pass", ADJACENT_PASS_PIPELINE),
    ];
    for (name, source) in programs {
        let project = compile_english_to_ir(&[SourceFile::new("main.tc", source)], "main")
            .unwrap_or_else(|errors| panic!("{name}: {errors:#?}"));
        let entry = &project.module.functions[project.module.entry.index()];
        let mut loops = 0;
        let mut branches = 0;
        count_core7_control_flow(&entry.body, &mut loops, &mut branches);
        assert!(
            loops > 0,
            "{name}: expected runtime Core-7 loop instructions, found only straight-line output"
        );
        assert!(
            branches > 0,
            "{name}: expected runtime Core-7 conditional branches, found only straight-line output"
        );
    }
}

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
fn adjacent_swap_pass_then_filter_and_print() {
    assert_program(
        "adjacent-swap-pass",
        "Start with the list of numbers 14, 7, 22, 9, 15, 4, and 18. Go through the list from left to right, comparing each number with the one immediately after it. Whenever the left number is larger than the right one, swap their positions and count a swap. After finishing that pass, remove any number from the list that is divisible by 3. Finally, print the remaining numbers in order, followed by the total number of swaps made.\n",
        &["7", "14", "4", "22", "5"],
    );
}

#[test]
fn adjacent_swap_pass_generalizes_values_and_divisor() {
    assert_program(
        "adjacent-swap-generalized",
        "Start with the list of numbers 20, 5, 10, 8, 27, 3, and 11. Go through the list from left to right, comparing each number with the one immediately after it. Whenever the left number is larger than the right one, swap their positions and count a swap. After finishing that pass, remove any number from the list that is divisible by 4. Finally, print the remaining numbers in order, followed by the total number of swaps made.\n",
        &["5", "10", "3", "11", "27", "5"],
    );
}

#[test]
fn two_box_simulation_with_guarded_moves_and_early_stop() {
    assert_program(
        "two-box-simulation",
        "Imagine a red box starting with 5 and a blue box starting with 16. Walk through every number from 1 up to 40. If the number is a multiple of 3, move 4 from the blue box into the red box; otherwise move 1 from the red box into the blue box. However, if a move would ever cause either box to drop below 5, skip that move and leave both boxes unchanged. Stop immediately as soon as the red box holds more than twice as much as the blue box. Finally, print the number where you stopped, followed by the amounts in the red box and the blue box.\n",
        &["12", "15", "6"],
    );
}

#[test]
fn two_box_simulation_generalizes_colors_amounts_and_floor() {
    assert_program(
        "two-box-generalized",
        "Imagine a green box starting with 10 and an orange box starting with 12. Walk through every number from 1 up to 30. If the number is a multiple of 3, move 4 from the orange box into the green box; otherwise move 1 from the green box into the orange box. However, if a move would ever cause either box to drop below 4, skip that move and leave both boxes unchanged. Stop immediately as soon as the green box holds more than twice as much as the orange box. Finally, print the number where you stopped, followed by the amounts in the green box and the orange box.\n",
        &["9", "16", "6"],
    );
}

#[test]
fn fibonacci_sequence_running_total_and_overflow_count() {
    assert_program(
        "fibonacci-running-total",
        "Start with 1 and 2. Keep generating the next number by adding the previous two numbers together, as long as the new number does not exceed 200. For each number in this entire sequence, including the starting two, add it to a running total. Whenever that total goes over 100, subtract 100 from it and count an overflow. At the end, print the final running total and how many overflows occurred.\n",
        &["75", "3"],
    );
}

#[test]
fn fibonacci_sequence_generalizes_seeds_bound_and_modulus() {
    assert_program(
        "fibonacci-generalized",
        "Start with 1 and 3. Keep generating the next number by adding the previous two numbers together, as long as the new number does not exceed 100. For each number in this entire sequence, including the starting two, add it to a running total. Whenever that total goes over 50, subtract 50 from it and count an overflow. At the end, print the final running total and how many overflows occurred.\n",
        &["46", "3"],
    );
}

#[test]
fn overflow_query_without_counter_is_a_clean_anaphor_error() {
    let result = run_elf_with_parity("Print how many overflows occurred.\n", None, "");
    assert!(!result.ok);
    assert!(
        result.lines.iter().any(|line| {
            line.contains("UNRESOLVED_ANAPHOR")
                && line.contains("count query `overflow`")
                && line.contains("count overflow")
        }),
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
