//! Paraphrase-invariance fluency suite for openers, number words, predicates,
//! inline lists, count/length queries, and static ELF64/IR parity.
//!
//! Coverage spans eight core tasks:
//! 1. literal and variable output through polite/sequence openers;
//! 2. arithmetic through spoken English number words;
//! 3. assignment and update phrasing;
//! 4. conditional comparisons and natural everyday predicates;
//! 5. loops, repetition, and iteration;
//! 6. inline list construction and aggregate queries;
//! 7. text length, count, and value wrappers;
//! 8. combined programs checked against static ELF64/IR parity.

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

fn run_checked(name: &str, source: &str) -> RunResult {
    let result = run_elf_with_parity(source, None, "");
    assert!(
        result.ok,
        "{name}: {}\n{}",
        result.summary,
        result.lines.join("\n")
    );
    assert!(
        result
            .summary
            .contains("EXACT MATCH (ELF64 == IR Interpreter)"),
        "{name}: missing ELF64/IR parity: {}",
        result.summary
    );
    result
}

fn assert_pair(name: &str, canonical: &str, paraphrase: &str, expected: &[&str]) {
    let canonical_result = run_checked(&format!("{name}/canonical"), canonical);
    let paraphrase_result = run_checked(&format!("{name}/paraphrase"), paraphrase);
    assert_eq!(
        stdout_lines(&canonical_result),
        expected,
        "{name}/canonical"
    );
    assert_eq!(
        stdout_lines(&paraphrase_result),
        expected,
        "{name}/paraphrase"
    );
}

fn temp_dir(tag: &str) -> PathBuf {
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "tc_r1_fluency_{}_{}_{tag}",
        std::process::id(),
        counter
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("temp dir");
    path
}

#[test]
fn fluency_open_print() {
    assert_pair("open_print", "Print 42.\n", "Please print 42.\n", &["42"]);
}

#[test]
fn fluency_open_show() {
    assert_pair(
        "open_show",
        "Let x be 7.\nShow x.\n",
        "Let x be 7.\nNow, please show me x.\n",
        &["7"],
    );
}

#[test]
fn fluency_open_let() {
    assert_pair(
        "open_let",
        "Let x be 5.\nAdd 1 to x.\nSay x.\n",
        "Let x be 5.\nFirst, let's add 1 to x.\nSay x.\n",
        &["6"],
    );
}

#[test]
fn fluency_open_lets() {
    assert_pair("open_lets", "Print 1.\n", "Let's print 1.\n", &["1"]);
}

#[test]
fn fluency_open_letus() {
    assert_pair("open_letus", "Print 2.\n", "Let us print 2.\n", &["2"]);
}

#[test]
fn fluency_open_next() {
    assert_pair("open_next", "Print 3.\n", "Next, just output 3.\n", &["3"]);
}

#[test]
fn fluency_open_finally() {
    assert_pair("open_finally", "Print 4.\n", "Finally, echo 4.\n", &["4"]);
}

#[test]
fn fluency_open_also() {
    assert_pair("open_also", "Print 5.\n", "Also print 5.\n", &["5"]);
}

#[test]
fn fluency_open_question() {
    assert_pair(
        "open_question",
        "Print the sum of 12 and 23.\n",
        "Can you show me the sum of 12 and 23?\n",
        &["35"],
    );
}

#[test]
fn fluency_open_could() {
    assert_pair(
        "open_could",
        "Print the value of 7.\n",
        "Could you please print the value of 7?\n",
        &["7"],
    );
}

#[test]
fn fluency_open_given() {
    assert_pair(
        "open_given",
        "Given a is 12 and b is 23, print the sum of a and b.\n",
        "Given a is twelve and b is twenty-three, please tell me the sum of a and b.\n",
        &["35"],
    );
}

#[test]
fn fluency_open_if() {
    assert_pair(
        "open_if",
        "If true, print 1.\n",
        "If true, please print 1.\n",
        &["1"],
    );
}

#[test]
fn fluency_open_say() {
    assert_pair(
        "open_say",
        "Print 10.\n",
        "Could you please say 10?\n",
        &["10"],
    );
}

#[test]
fn fluency_open_addup() {
    assert_pair(
        "open_addup",
        "Let x be 0.\nAdd 5 to x.\nSay x.\n",
        "Let x be 0.\nAdd up 5 to x.\nSay x.\n",
        &["5"],
    );
}

#[test]
fn fluency_open_sumup() {
    assert_pair(
        "open_sumup",
        "Print the sum of 10, 20, and 30.\n",
        "Sum up 10, 20, and 30.\n",
        &["60"],
    );
}

#[test]
fn fluency_num_zero() {
    assert_pair("num_zero", "Print 0.\n", "Print zero.\n", &["0"]);
}

#[test]
fn fluency_num_seven() {
    assert_pair("num_seven", "Print 7.\n", "Print seven.\n", &["7"]);
}

#[test]
fn fluency_num_23hyphen() {
    assert_pair(
        "num_23hyphen",
        "Print 23.\n",
        "Print twenty-three.\n",
        &["23"],
    );
}

#[test]
fn fluency_num_23space() {
    assert_pair(
        "num_23space",
        "Print twenty-three.\n",
        "Print twenty three.\n",
        &["23"],
    );
}

#[test]
fn fluency_num_sum() {
    assert_pair(
        "num_sum",
        "Print the sum of 12 and 23.\n",
        "Print the sum of twelve and twenty-three.\n",
        &["35"],
    );
}

#[test]
fn fluency_num_99() {
    assert_pair("num_99", "Print 99.\n", "Print ninety-nine.\n", &["99"]);
}

#[test]
fn fluency_num_101() {
    assert_pair(
        "num_101",
        "Print 101.\n",
        "Print one hundred and one.\n",
        &["101"],
    );
}

#[test]
fn fluency_num_250() {
    assert_pair(
        "num_250",
        "Print 250.\n",
        "Print two hundred and fifty.\n",
        &["250"],
    );
}

#[test]
fn fluency_num_1000() {
    assert_pair(
        "num_1000",
        "Print 1000.\n",
        "Print one thousand.\n",
        &["1000"],
    );
}

#[test]
fn fluency_num_1000001() {
    assert_pair(
        "num_1000001",
        "Print 1000001.\n",
        "Print one million and one.\n",
        &["1000001"],
    );
}

#[test]
fn fluency_num_assign() {
    assert_pair(
        "num_assign",
        "Let x be 10.\nAdd 10 to x.\nSay x.\n",
        "Let x be ten.\nAdd ten to x.\nSay x.\n",
        &["20"],
    );
}

#[test]
fn fluency_num_terms() {
    assert_pair(
        "num_terms",
        "Print the sum of 12 and 13.\n",
        "Print the sum of twelve and thirteen.\n",
        &["25"],
    );
}

#[test]
fn fluency_num_repeat() {
    assert_pair(
        "num_repeat",
        "Repeat 3 times, print \"x\".\n",
        "Repeat three times, print \"x\".\n",
        &["x", "x", "x"],
    );
}

#[test]
fn fluency_num_foreach() {
    assert_pair(
        "num_foreach",
        "For every i from 1 to 3, print i.\n",
        "For every i from one to three, print i.\n",
        &["1", "2", "3"],
    );
}

#[test]
fn fluency_num_while() {
    assert_pair(
        "num_while",
        "Let n be 0.\nWhile n is less than 3, add 1 to n.\nSay n.\n",
        "Let n be zero.\nWhile n is less than three, add one to n.\nSay n.\n",
        &["3"],
    );
}

#[test]
fn fluency_num_shadow() {
    assert_pair(
        "num_shadow",
        "Let one be 42.\nSay one.\n",
        "Let one be 42.\nPrint one.\n",
        &["42"],
    );
}

#[test]
fn fluency_num_condition() {
    assert_pair(
        "num_condition",
        "Let x be 12.\nIf x is divisible by 3, print \"yes\".\n",
        "Let x be twelve.\nIf x is divisible by three, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_pred_even() {
    assert_pair(
        "pred_even",
        "Let x be 12.\nIf x is even, print \"yes\".\n",
        "Let x be 12.\nIf x is divisible by two, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_pred_odd() {
    assert_pair(
        "pred_odd",
        "Let x be 13.\nIf x is odd, print \"yes\".\n",
        "Let x be 13.\nIf x is not even, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_pred_notodd() {
    assert_pair(
        "pred_notodd",
        "Let x be 12.\nIf x is not odd, print \"yes\".\n",
        "Let x be 12.\nIf x is even, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_pred_positive() {
    assert_pair(
        "pred_positive",
        "Let x be 3.\nIf x is positive, print \"yes\".\n",
        "Let x be 3.\nIf x is more than 0, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_pred_negative() {
    assert_pair(
        "pred_negative",
        "Let x be -5.\nIf x is negative, print \"yes\".\n",
        "Let x be -5.\nIf x is less than 0, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_pred_zero() {
    assert_pair(
        "pred_zero",
        "Let x be 0.\nIf x is zero, print \"yes\".\n",
        "Let x be 0.\nIf x equals 0, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_pred_nonzero() {
    assert_pair(
        "pred_nonzero",
        "Let x be 12.\nIf x is nonzero, print \"yes\".\n",
        "Let x be 12.\nIf x is not zero, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_pred_div3() {
    assert_pair(
        "pred_div3",
        "Let x be 12.\nIf x is divisible by 3, print \"yes\".\n",
        "Let x be 12.\nIf x is divisible by three, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_pred_notdiv5() {
    assert_pair(
        "pred_notdiv5",
        "Let x be 12.\nIf x is not divisible by 5, print \"yes\".\n",
        "Let x be 12.\nIf x is not divisible by five, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_pred_between() {
    assert_pair(
        "pred_between",
        "Let x be 12.\nIf x is between 10 and 20, print \"yes\".\n",
        "Let x be 12.\nIf 10 is no more than x and x is no more than 20, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_pred_notbetween() {
    assert_pair(
        "pred_notbetween",
        "Let x be 5.\nIf x is not between 10 and 20, print \"yes\".\n",
        "Let x be 5.\nIf x is below 10, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_pred_atleast() {
    assert_pair(
        "pred_atleast",
        "Let x be 12.\nIf x is at least 10, print \"yes\".\n",
        "Let x be 12.\nIf x is no less than 10, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_pred_atmost() {
    assert_pair(
        "pred_atmost",
        "Let x be 12.\nIf x is at most 12, print \"yes\".\n",
        "Let x be 12.\nIf x is no more than 12, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_pred_exceeds() {
    assert_pair(
        "pred_exceeds",
        "Let x be 12.\nIf x exceeds 5, print \"yes\".\n",
        "Let x be 12.\nIf x is more than 5, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_pred_above() {
    assert_pair(
        "pred_above",
        "Let x be 12.\nIf x is above 5, print \"yes\".\n",
        "Let x be 12.\nIf x is over 5, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_pred_below() {
    assert_pair(
        "pred_below",
        "Let x be 3.\nIf x is below 5, print \"yes\".\n",
        "Let x be 3.\nIf x is under 5, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_pred_fewer() {
    assert_pair(
        "pred_fewer",
        "Let x be 3.\nIf x is fewer than 5, print \"yes\".\n",
        "Let x be 3.\nIf x is less than 5, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_pred_equals() {
    assert_pair(
        "pred_equals",
        "Let x be 12.\nIf x equals 12, print \"yes\".\n",
        "Let x be 12.\nIf x is equal to 12, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_pred_same() {
    assert_pair(
        "pred_same",
        "Let x be 12.\nIf x is the same as 12, print \"yes\".\n",
        "Let x be 12.\nIf x is equal to twelve, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_pred_differs() {
    assert_pair(
        "pred_differs",
        "Let x be 12.\nIf x differs from 5, print \"yes\".\n",
        "Let x be 12.\nIf x is different from 5, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_pred_combined() {
    assert_pair(
        "pred_combined",
        "Let x be 12.\nIf x is even and x is positive, print \"yes\".\n",
        "Let x be 12.\nIf x is divisible by two and x is more than zero, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_pred_text_empty() {
    assert_pair(
        "pred_text_empty",
        "Let s be \"\".\nIf s is empty, print \"yes\".\n",
        "Let s be \"\".\nIf the length of s equals 0, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_pred_text_notempty() {
    assert_pair(
        "pred_text_notempty",
        "Let s be \"hi\".\nIf s is not empty, print \"yes\".\n",
        "Let s be \"hi\".\nIf the length of s is more than 0, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_list_sum() {
    assert_pair(
        "list_sum",
        "Let p be a list of 10, 20, and 30.\nPrint the sum of p.\n",
        "Let p be a list of 10, 20, 30.\nPrint the sum of p.\n",
        &["60"],
    );
}

#[test]
fn fluency_list_count() {
    assert_pair(
        "list_count",
        "Let p be a list of 10, 20, and 30.\nPrint the number of items in p.\n",
        "Let p be a list of 10, 20, 30.\nPrint the size of p.\n",
        &["3"],
    );
}

#[test]
fn fluency_list_average() {
    assert_pair(
        "list_average",
        "Let p be a list of 10, 20, and 30.\nPrint the average of p.\n",
        "Let p be a list of 10, 20, 30.\nPrint the average of p.\n",
        &["20.0000"],
    );
}

#[test]
fn fluency_list_min() {
    assert_pair(
        "list_min",
        "Let p be a list of 10, 20, and 30.\nPrint the minimum of p.\n",
        "Let p be a list of 10, 20, 30.\nPrint the minimum of p.\n",
        &["10"],
    );
}

#[test]
fn fluency_list_max() {
    assert_pair(
        "list_max",
        "Let p be a list of 10, 20, and 30.\nPrint the maximum of p.\n",
        "Let p be a list of 10, 20, 30.\nPrint the maximum of p.\n",
        &["30"],
    );
}

#[test]
fn fluency_list_foreach() {
    assert_pair(
        "list_foreach",
        "Let p be a list of 10, 20, and 30.\nFor each v in p, show v.\n",
        "Let p be a list of 10, 20, 30.\nFor each v in p, print v.\n",
        &["10", "20", "30"],
    );
}

#[test]
fn fluency_list_forevery() {
    assert_pair(
        "list_forevery",
        "Let p be a list of 10, 20, and 30.\nFor every v in p, show v.\n",
        "Let p be a list of 10, 20, 30.\nFor each v in p, say v.\n",
        &["10", "20", "30"],
    );
}

#[test]
fn fluency_list_empty() {
    assert_pair(
        "list_empty",
        "Let p be an empty list of whole numbers.\nIf p is empty, print \"yes\".\n",
        "Let p be an empty list of whole numbers.\nIf the number of items in p equals 0, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_list_empty_text() {
    assert_pair(
        "list_empty_text",
        "Let p be an empty list of text.\nIf p is empty, print \"yes\".\n",
        "Let p be an empty list of text.\nIf the number of items in p equals zero, print \"yes\".\n",
        &["yes"],
    );
}

#[test]
fn fluency_list_names() {
    assert_pair(
        "list_names",
        "Let names be a list of \"Alice\", \"Bob\", and \"Cara\".\nPrint the number of items in names.\n",
        "Let names be a list of \"Alice\", \"Bob\", \"Cara\".\nPrint the size of names.\n",
        &["3"],
    );
}

#[test]
fn fluency_list_name_lengths() {
    assert_pair(
        "list_name_lengths",
        "Let names be a list of \"Alice\", \"Bob\", and \"Cara\".\nFor each name in names, print the length of name.\n",
        "Let names be a list of \"Alice\", \"Bob\", \"Cara\".\nFor each name in names, print the number of characters in name.\n",
        &["5", "3", "4"],
    );
}

#[test]
fn fluency_list_decimals() {
    assert_pair(
        "list_decimals",
        "Let r be a list of 1.5, 2.5, and 3.5.\nPrint the sum of r.\n",
        "Let r be a list of 1.5, 2.5, 3.5.\nPrint the sum of r.\n",
        &["7.5000"],
    );
}

#[test]
fn fluency_list_variables() {
    assert_pair(
        "list_variables",
        "Let a be 10.\nLet b be 20.\nLet p be a list of a, b, and 30.\nPrint the sum of p.\n",
        "Let a be 10.\nLet b be 20.\nLet p be a list of 10, 20, and 30.\nPrint the sum of p.\n",
        &["60"],
    );
}

#[test]
fn fluency_list_where() {
    assert_pair(
        "list_where",
        "Let p be a list of a, b, and 30, where a is 10 and b is 20.\nPrint the sum of p.\n",
        "Let p be a list of 10, 20, and 30.\nPrint the sum of p.\n",
        &["60"],
    );
}

#[test]
fn fluency_list_single() {
    assert_pair(
        "list_single",
        "Let p be a list of 7.\nPrint the sum of p.\n",
        "Let p be a list of 7.\nPrint sum of p.\n",
        &["7"],
    );
}

#[test]
fn fluency_list_two_and() {
    assert_pair(
        "list_two_and",
        "Let p be a list of 1 and 2.\nPrint the sum of p.\n",
        "Let p be a list of 1, 2.\nPrint the sum of p.\n",
        &["3"],
    );
}

#[test]
fn fluency_list_sum_bare() {
    assert_pair(
        "list_sum_bare",
        "Let p be a list of 10, 20, and 30.\nPrint sum of p.\n",
        "Let p be a list of 10, 20, 30.\nPrint the sum of p.\n",
        &["60"],
    );
}

#[test]
fn fluency_list_count_the() {
    assert_pair(
        "list_count_the",
        "Let s be \"hello\".\nPrint the count of s.\n",
        "Let s be \"hello\".\nPrint the number of characters in s.\n",
        &["5"],
    );
}

#[test]
fn fluency_len_var() {
    assert_pair(
        "len_var",
        "Let s be \"hello\".\nPrint the length of s.\n",
        "Let s be \"hello\".\nPrint the number of characters in s.\n",
        &["5"],
    );
}

#[test]
fn fluency_len_literal() {
    assert_pair(
        "len_literal",
        "Print the length of \"world\".\n",
        "Print 5.\n",
        &["5"],
    );
}

#[test]
fn fluency_len_empty() {
    assert_pair(
        "len_empty",
        "Let s be \"\".\nPrint the length of s.\n",
        "Print 0.\n",
        &["0"],
    );
}

#[test]
fn fluency_len_size() {
    assert_pair(
        "len_size",
        "Let s be \"hello\".\nPrint the size of s.\n",
        "Let s be \"hello\".\nPrint the length of s.\n",
        &["5"],
    );
}

#[test]
fn fluency_len_count() {
    assert_pair(
        "len_count",
        "Let s be \"hello\".\nPrint the count of s.\n",
        "Let s be \"hello\".\nPrint the length of s.\n",
        &["5"],
    );
}

#[test]
fn fluency_wrap_value() {
    assert_pair(
        "wrap_value",
        "Let x be 7.\nPrint x.\n",
        "Let x be 7.\nPrint the value of x.\n",
        &["7"],
    );
}

#[test]
fn fluency_wrap_number() {
    assert_pair(
        "wrap_number",
        "Let x be 7.\nPrint x.\n",
        "Let x be 7.\nPrint the number x.\n",
        &["7"],
    );
}

#[test]
fn fluency_wrap_number_add() {
    assert_pair(
        "wrap_number_add",
        "Let x be 5.\nAdd 10 to x.\nSay x.\n",
        "Let x be 5.\nAdd the number 10 to x.\nSay x.\n",
        &["15"],
    );
}

#[test]
fn fluency_wrap_value_sum() {
    assert_pair(
        "wrap_value_sum",
        "Print the sum of 1 and 2.\n",
        "Print the value of the sum of 1 and 2.\n",
        &["3"],
    );
}

#[test]
fn fluency_len_where() {
    assert_pair(
        "len_where",
        "Given name is \"Ada\", print the length of name.\n",
        "Given name is \"Ada\", print the number of characters in name.\n",
        &["3"],
    );
}

#[test]
fn fluency_int_open_list() {
    assert_pair(
        "int_open_list",
        "Let p be a list of a, b, and 30, where a is 10 and b is 20.\nPrint the sum of p.\n",
        "Please let p be a list of a, b, and 30, where a is ten and b is twenty.\nShow the sum of p.\n",
        &["60"],
    );
}

#[test]
fn fluency_int_repeat_open() {
    assert_pair(
        "int_repeat_open",
        "Repeat 3 times, print \"x\".\n",
        "First, let's repeat three times, print \"x\".\n",
        &["x", "x", "x"],
    );
}

#[test]
fn fluency_int_foreach_pred() {
    assert_pair(
        "int_foreach_pred",
        "Let p be a list of two, four, and five.\nFor each v in p, say v.\n",
        "Let p be a list of 2, 4, and 5.\nFor every v in p, print v.\n",
        &["2", "4", "5"],
    );
}

#[test]
fn fluency_int_number_word_length() {
    assert_pair(
        "int_number_word_length",
        "Let s be \"abc\".\nPrint the length of s.\n",
        "Let s be \"abc\".\nPrint the number of characters in s.\n",
        &["3"],
    );
}

#[test]
fn fluency_int_all() {
    assert_pair(
        "int_all",
        "Let p be a list of 10, 20, and 30.\nPrint the sum of p.\nPrint the number of items in p.\nPrint the average of p.\n",
        "Please let p be a list of ten, twenty, and thirty.\nSum up p.\nPrint the size of p.\nShow the average of p.\n",
        &["60", "3", "20.0000"],
    );
}

#[test]
fn fluency_combined_program_is_static_elf64_with_ir_parity() {
    let source = concat!(
        "Please let prices be a list of ten, twenty, and thirty.\n",
        "If the number of items in prices is at least three, sum up prices.\n",
        "Print the average of prices.\n",
        "Print the count of \"abcd\".\n",
    );
    let expected = ["60", "20.0000", "4"];
    let result = run_checked("combined/static", source);
    assert_eq!(stdout_lines(&result), expected);

    let directory = temp_dir("static");
    let program = directory.join("combined.eng");
    let binary = directory.join("combined.elf");
    fs::write(&program, source).expect("program");
    let build = Command::new(env!("CARGO_BIN_EXE_tc"))
        .arg("build")
        .arg(&program)
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("run tc build");
    assert!(
        build.status.success(),
        "tc build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let readelf = Command::new("readelf")
        .arg("-d")
        .arg(&binary)
        .output()
        .expect("run readelf");
    let text = String::from_utf8_lossy(&readelf.stdout);
    assert!(
        text.contains("There is no dynamic section"),
        "combined program is not a static ELF64: {text}"
    );
    let run = Command::new(&binary).output().expect("run combined ELF64");
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert_eq!(
        stdout.lines().collect::<Vec<_>>(),
        expected,
        "static ELF64 output differs from IR parity run"
    );
    let _ = fs::remove_dir_all(&directory);
}
