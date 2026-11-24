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
fn message_spread_reports_target_minute_and_never_heard_count() {
    assert_program(
        "message-spread",
        "Consider 8 people: Alice, Bob, Charlie, Diana, Eve, Frank, Grace, and Henry. Alice can message Bob and Charlie; Bob can message Diana and Eve; Charlie can message Eve; Eve can message Frank and Henry; Diana can message Grace; and Grace can message Frank. Alice starts with a secret message at minute 0. At each subsequent minute, everyone who first heard the message during the previous minute forwards it to all the people they can message, except that Eve never forwards anything to anyone, and anyone who has already heard the message ignores duplicates. Keep advancing minute by minute until nobody new hears the message. At the end, print the minute when Frank first heard the message, followed by how many of the 8 people never heard it at all.\n",
        &["4", "1"],
    );
}

#[test]
fn message_spread_generalizes_graph_and_blocked_forwarder() {
    assert_program(
        "message-spread-generalized",
        "Consider 5 people: Ana, Ben, Cara, Dan, and Eli. Ana can message Ben and Cara; Ben can message Dan; Cara can message Dan and Eli; Dan can message Eli. Ana starts with a secret message at minute 0. At each subsequent minute, everyone who first heard the message during the previous minute forwards it to all the people they can message, except that Cara never forwards anything to anyone, and anyone who has already heard the message ignores duplicates. Keep advancing minute by minute until nobody new hears the message. At the end, print the minute when Eli first heard the message, followed by how many of the 5 people never heard it at all.\n",
        &["3", "0"],
    );
}

#[test]
fn switch_ring_simulation_reports_on_count_and_first_repeat() {
    assert_program(
        "switch-ring",
        "Line up 6 switches in a circle with the starting states on, off, on, on, off, and off. Run 5 update rounds. In each round, determine the next state of every switch simultaneously based on its two neighbors at the start of that round: a switch becomes on if exactly one of its two neighbors is on, and otherwise it becomes off. Keep a history of the full row of states after every round, and note the first round number whose resulting pattern matches an earlier state. After all 5 rounds finish, print how many switches are currently on, followed by that first repeating round number.\n",
        &["4", "3"],
    );
}

#[test]
fn switch_ring_generalizes_size_initial_state_and_rounds() {
    assert_program(
        "switch-ring-generalized",
        "Line up 5 switches in a circle with the starting states on, on, off, on, and off. Run 5 update rounds. In each round, determine the next state of every switch simultaneously based on its two neighbors at the start of that round: a switch becomes on if exactly one of its two neighbors is on, and otherwise it becomes off. Keep a history of the full row of states after every round, and note the first round number whose resulting pattern matches an earlier state. After all 5 rounds finish, print how many switches are currently on, followed by that first repeating round number.\n",
        &["4", "4"],
    );
}

#[test]
fn bracket_scan_reports_mismatch_position_and_deepest_nesting() {
    assert_program(
        "bracket-scan",
        "Inspect the text \"([{}])([]{})([)]\" one character at a time from left to right. Whenever you see an opening bracket, remember it as the most recent unmatched one; whenever you see a closing bracket, check whether it matches the most recent unmatched opening bracket and clear that pair if it does. Keep track of the deepest level of simultaneous open brackets you reach. If a closing bracket ever fails to match, or appears when no opening bracket is waiting, stop immediately and print the 1-based position of that broken character, followed by the deepest nesting level reached so far.\n",
        &["15", "3"],
    );
}

#[test]
fn bracket_scan_generalizes_text_and_mismatch() {
    assert_program(
        "bracket-scan-generalized",
        "Inspect the text \"([)]\" one character at a time from left to right. Whenever you see an opening bracket, remember it as the most recent unmatched one; whenever you see a closing bracket, check whether it matches the most recent unmatched opening bracket and clear that pair if it does. Keep track of the deepest level of simultaneous open brackets you reach. If a closing bracket ever fails to match, or appears when no opening bracket is waiting, stop immediately and print the 1-based position of that broken character, followed by the deepest nesting level reached so far.\n",
        &["3", "2"],
    );
}

#[test]
fn circle_removal_reports_requested_removed_and_survivor() {
    assert_program(
        "circle-removal",
        "Arrange the numbers from 1 to 10 in a circle. Starting at 1, count around the circle and remove every third remaining number until only one number is left. Remember the order in which the numbers were removed. At the end, print the fourth number that was removed, followed by the single number left in the circle.\n",
        &["2", "4"],
    );
}

#[test]
fn circle_removal_generalizes_range_step_and_ordinal() {
    assert_program(
        "circle-removal-generalized",
        "Arrange the numbers from 1 to 8 in a circle. Starting at 1, count around the circle and remove every fourth remaining number until only one number is left. Remember the order in which the numbers were removed. At the end, print the second number that was removed, followed by the single number left in the circle.\n",
        &["8", "6"],
    );
}

#[test]
fn grid_walk_counts_revisits_and_manhattan_distance() {
    assert_program(
        "grid-walk",
        "Imagine standing at the center of a grid, facing north. Follow these moves in order: walk forward 3 steps, turn right and walk forward 4 steps, turn right and walk forward 2 steps, turn right and walk forward 5 steps, and finally turn right and walk forward 3 steps. Take one step at a time, and count every time you step onto a spot you have already visited, including your starting spot. When all the moves are finished, print how many times you crossed your own path, followed by your east-west distance plus your north-south distance from the starting spot.\n",
        &["1", "5"],
    );
}

#[test]
fn grid_walk_generalizes_moves_and_returns_to_start() {
    assert_program(
        "grid-walk-square",
        "Imagine standing at the center of a grid, facing north. Follow these moves in order: walk forward 2 steps, turn right and walk forward 2 steps, turn right and walk forward 2 steps, and finally turn right and walk forward 2 steps. Take one step at a time, and count every time you step onto a spot you have already visited, including your starting spot. When all the moves are finished, print how many times you crossed your own path, followed by your east-west distance plus your north-south distance from the starting spot.\n",
        &["1", "0"],
    );
}

#[test]
fn literal_sentence_word_frequency_with_length_filter() {
    assert_program(
        "word-frequency",
        "Take the sentence \"the quick brown fox jumps over the lazy dog and the fox runs\". Split it into words, and count how many times each word appears. Ignore any word that has fewer than 4 letters, unless it appears more than once. Among the remaining words, print each word along with its count, and finally print the word that appeared the most times.\n",
        &[
            "the 3", "quick 1", "brown 1", "fox 2", "jumps 1", "over 1", "lazy 1", "runs 1", "the",
        ],
    );
}

#[test]
fn word_frequency_generalizes_text_and_minimum_length() {
    assert_program(
        "word-frequency-generalized",
        "Take the sentence \"one fish two fish red fish blue fish and a cat\". Split it into words, and count how many times each word appears. Ignore any word that has fewer than 3 letters, unless it appears more than once. Among the remaining words, print each word along with its count, and finally print the word that appeared the most times.\n",
        &[
            "one 1", "fish 4", "two 1", "red 1", "blue 1", "and 1", "cat 1", "fish",
        ],
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
