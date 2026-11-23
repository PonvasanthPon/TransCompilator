//! Discourse-level synthesis regressions: paragraph loops, checks, ellipsis, counts.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use tc_editor::runner::{RunResult, run_elf_with_parity};

static COUNTER: AtomicU64 = AtomicU64::new(0);

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

fn temp_dir(tag: &str) -> PathBuf {
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!(
        "tc_r1_discourse_{}_{}_{tag}",
        std::process::id(),
        counter
    ));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("temp directory");
    directory
}

fn assert_command_failure(source: &str, needles: &[&str]) {
    let directory = temp_dir("failure");
    let program = directory.join("program.eng");
    fs::write(&program, source).expect("program");
    let output = Command::new(env!("CARGO_BIN_EXE_tc"))
        .arg("build")
        .arg(&program)
        .arg("-o")
        .arg(program.with_extension("elf"))
        .output()
        .expect("tc build");
    assert!(!output.status.success(), "{source}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    for needle in needles {
        assert!(stderr.contains(needle), "missing `{needle}`:\n{stderr}");
    }
    let _ = fs::remove_dir_all(&directory);
}

const PRIME_PROGRAM: &str = "Look at every number from 2 up to 50. For each number, check whether any smaller number starting from 2 divides into it evenly. If none of them do, print that number out and count it. Once you are done, print how many numbers were found.\n";

#[test]
fn exact_four_sentence_prime_finder_parity_and_output() {
    let result = run_elf_with_parity(PRIME_PROGRAM, None, "");
    assert!(result.ok, "{}\n{}", result.summary, result.lines.join("\n"));
    assert_eq!(
        stdout_lines(&result),
        [
            "2", "3", "5", "7", "11", "13", "17", "19", "23", "29", "31", "37", "41", "43", "47",
            "15"
        ]
    );
}

#[test]
fn generalized_value_range_and_count() {
    assert_program(
        "general-primes",
        "Look at every value from 10 up to 30. For each value, check whether any smaller value starting from 2 divides into it evenly. If none of them do, print that value out and count it. Once you are done, print how many values were found.\n",
        &["11", "13", "17", "19", "23", "29", "6"],
    );
}

#[test]
fn positive_ellipsis_selects_composites() {
    assert_program(
        "positive-ellipsis",
        "Look at every n from 4 up to 12. For each n, check whether any smaller n starting from 2 divides into it evenly. If any of them do, print that n out and count it. Once you are done, print how many numbers were found.\n",
        &["4", "6", "8", "9", "10", "12", "6"],
    );
}

#[test]
fn consider_every_even_values_and_count() {
    assert_program(
        "even-count",
        "Consider every n from 1 up to 10. If n is even, print that n out and count it. When finished, print how many were found.\n",
        &["2", "4", "6", "8", "10", "5"],
    );
}

#[test]
fn standalone_that_anaphor_with_trailing_out() {
    assert_program("that-out", "Let x be 42.\nPrint that x out.\n", &["42"]);
}

#[test]
fn list_check_with_positive_ellipsis() {
    assert_program(
        "list-check",
        concat!(
            "Let prices be a list of 10, 20, and 30.\n",
            "Check whether any item in prices is greater than 25.\n",
            "If any of them do, print \"expensive\".\n",
        ),
        &["expensive"],
    );
}

#[test]
fn list_discourse_loop_with_implicit_counter() {
    assert_program(
        "list-loop",
        concat!(
            "Let prices be a list of 10, 20, and 30.\n",
            "Consider each price in prices. If price is greater than 15, print that price out and count it. After that, print how many were found.\n",
        ),
        &["20", "30", "2"],
    );
}

#[test]
fn ellipsis_without_check_is_a_clean_anaphor_error() {
    assert_command_failure(
        "If none of them do, print \"x\".\n",
        &["UNRESOLVED_ANAPHOR", "no preceding `check whether`"],
    );
}

#[test]
fn empty_loop_body_is_rejected() {
    assert_command_failure(
        "Let x be 5.\nRepeat while x is greater than 1:\n\nPrint x.\n",
        &["empty loop body", ":2:1:"],
    );
}

#[test]
fn grouped_collatz_without_blank_header_still_passes() {
    let result = run_elf_with_parity(
        concat!(
            "Let current be 27, and let steps start at 0.\n\n",
            "Repeat while current is greater than 1:\n",
            "If current is even, divide current by 2.\n",
            "Otherwise, multiply current by 3 and add 1 to current.\n",
            "Add 1 to steps, then print current.\n",
        ),
        None,
        "",
    );
    assert!(result.ok, "{}\n{}", result.summary, result.lines.join("\n"));
    assert_eq!(stdout_lines(&result).len(), 111);
}

#[test]
fn through_range_and_odd_predicate() {
    assert_program(
        "through-odd",
        "Look at every n from 1 through 5. If n is odd, print that n out and count it. Once finished, print how many were found.\n",
        &["1", "3", "5", "3"],
    );
}

#[test]
fn universal_check_all_of_them() {
    assert_program(
        "universal-check",
        concat!(
            "Check whether every number from 2 up to 6 is greater than 1.\n",
            "If all of them do, print \"all\".\n",
        ),
        &["all"],
    );
}

#[test]
fn count_query_without_counter_is_a_clean_anaphor_error() {
    assert_command_failure(
        "Print how many numbers were found.\n",
        &["UNRESOLVED_ANAPHOR", "implicit `count`"],
    );
}

#[test]
fn check_range_outside_discourse_loop() {
    assert_program(
        "range-check",
        concat!(
            "Check whether any number from 2 up to 10 is greater than 5.\n",
            "If any of them do, print \"found\".\n",
        ),
        &["found"],
    );
}

#[test]
fn once_finished_closes_loop_and_queries_count() {
    assert_program(
        "once-finished",
        "Look at every n from 1 up to 5. If n is greater than 3, print n out and count it. Once finished, print how many were found.\n",
        &["4", "5", "2"],
    );
}
