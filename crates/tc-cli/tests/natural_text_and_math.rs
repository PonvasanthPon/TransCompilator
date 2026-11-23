//! Natural text predicates, zero-boilerplate string/math preludes, and trim.

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

#[test]
fn zero_boilerplate_trim_starts_with_and_square_root() {
    assert_program(
        "natural-text-math",
        concat!(
            "Let raw be \"  42  \".\n",
            "Let clean be trim raw.\n",
            "Print the length of clean.\n",
            "If clean starts with \"4\", print \"starts with four\".\n",
            "Print the square root of 81.0.\n",
        ),
        &["2", "starts with four", "9.0000"],
    );
}

#[test]
fn text_ends_with_predicate() {
    assert_program(
        "ends-with",
        concat!(
            "Let clean be trim \"  42  \".\n",
            "If clean ends with \"2\", print \"ends\".\n",
        ),
        &["ends"],
    );
}

#[test]
fn text_does_not_start_with_predicate() {
    assert_program(
        "not-starts-with",
        concat!(
            "Let clean be trim \"  42  \".\n",
            "If clean does not start with \"9\", print \"not starts\".\n",
        ),
        &["not starts"],
    );
}

#[test]
fn text_does_not_end_with_predicate() {
    assert_program(
        "not-ends-with",
        concat!(
            "Let clean be trim \"  42  \".\n",
            "If clean does not end with \"9\", print \"not ends\".\n",
        ),
        &["not ends"],
    );
}

#[test]
fn text_contains_predicate() {
    assert_program(
        "text-contains",
        concat!(
            "Let clean be trim \"  42  \".\n",
            "If clean contains \"4\", print \"contains\".\n",
        ),
        &["contains"],
    );
}

#[test]
fn text_does_not_contain_predicate() {
    assert_program(
        "text-not-contains",
        concat!(
            "Let clean be trim \"  42  \".\n",
            "If clean does not contain \"9\", print \"not contains\".\n",
        ),
        &["not contains"],
    );
}

#[test]
fn byte_literal_variable_contains_predicate() {
    assert_program(
        "bytes-contains",
        concat!(
            "Let raw be \" 42 \".\n",
            "If raw contains \"4\", print \"found\".\n",
        ),
        &["found"],
    );
}

#[test]
fn list_contains_predicate() {
    assert_program(
        "list-contains",
        concat!(
            "Let prices be a list of 10, 20, and 30.\n",
            "If prices contains 20, print \"found\".\n",
        ),
        &["found"],
    );
}

#[test]
fn list_does_not_contain_predicate() {
    assert_program(
        "list-not-contains",
        concat!(
            "Let prices be a list of 10, 20, and 30.\n",
            "If prices does not contain 99, print \"missing\".\n",
        ),
        &["missing"],
    );
}

#[test]
fn square_root_of_whole_number_promotes_to_decimal() {
    assert_program("sqrt-i64", "Print the square root of 81.\n", &["9.0000"]);
}

#[test]
fn square_root_of_decimal_without_article() {
    assert_program("sqrt-f64", "Print square root of 81.0.\n", &["9.0000"]);
}

#[test]
fn explicit_use_string_and_math_remain_idempotent() {
    assert_program(
        "explicit-prelude",
        concat!(
            "Use string and math.\n",
            "Let raw be \"  42  \".\n",
            "Let clean be trim raw.\n",
            "If clean starts with \"4\", print \"starts\".\n",
            "Print the square root of 81.\n",
        ),
        &["starts", "9.0000"],
    );
}

#[test]
fn trimmed_alias_and_trimmed_literal() {
    assert_program(
        "trimmed-alias",
        concat!(
            "Let raw be trimmed \"  bye  \".\n",
            "If raw ends with \"e\", print \"alias ok\".\n",
        ),
        &["alias ok"],
    );
}
