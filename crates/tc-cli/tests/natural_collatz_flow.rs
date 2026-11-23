//! Natural Collatz-flow regressions: bindings, output parts, and paragraph loops.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use tc_editor::runner::{RunResult, run_elf_with_parity};

static COUNTER: AtomicU64 = AtomicU64::new(0);

const COLLATZ: &str = concat!(
    "Let current be 27, and let steps start at 0.\n",
    "\n",
    "Print \"Tracing sequence from \" with current.\n",
    "\n",
    "Repeat while current is greater than 1:\n",
    "If current is even, divide current by 2.\n",
    "Otherwise, multiply current by 3 and add 1 to current.\n",
    "Add 1 to steps, then print current.\n",
    "\n",
    "Print \"Reached 1 in \" with steps and \" steps.\"\n",
);

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
        "tc_r1_collatz_{}_{}_{tag}",
        std::process::id(),
        counter
    ));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("temp directory");
    directory
}

fn assert_diagnostic_line(command: &str, program: &Path, expected: &str, forbidden: &str) {
    let mut invocation = Command::new(env!("CARGO_BIN_EXE_tc"));
    invocation.arg(command).arg(program);
    if command == "build" {
        invocation.arg("-o").arg(program.with_extension("elf"));
    }
    let output = invocation.output().expect("run tc");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(expected), "{stderr}");
    assert!(!stderr.contains(forbidden), "{stderr}");
}

#[test]
fn exact_collatz_program_has_exact_parity_and_113_lines() {
    let result = run_elf_with_parity(COLLATZ, None, "");
    assert!(result.ok, "{}\n{}", result.summary, result.lines.join("\n"));
    let output = stdout_lines(&result);
    assert_eq!(output.len(), 113, "{output:?}");
    assert_eq!(
        output.first().map(String::as_str),
        Some("Tracing sequence from 27")
    );
    assert_eq!(
        output.last().map(String::as_str),
        Some("Reached 1 in 111 steps.")
    );
}

#[test]
fn trim_text_accepts_ptr_u8_bytes() {
    assert_program(
        "trim-text",
        concat!(
            "Use string. Use math.\n",
            "Let raw be \"  42  \".\n",
            "Let clean be trim text raw.\n",
            "Print the length of clean.\n",
            "If clean starts with \"4\", print \"starts with four\".\n",
            "Print square root of 81.0.\n",
        ),
        &["2", "starts with four", "9.0000"],
    );
}

#[test]
fn compound_let_and_start_at_bindings() {
    assert_program(
        "let-start-at",
        concat!(
            "Let a be 5, and let b start at 10.\n",
            "Print \"a=\" with a and \", b=\" with b.\n",
        ),
        &["a=5, b=10"],
    );
}

#[test]
fn compound_let_without_comma() {
    assert_program(
        "let-and-let",
        concat!("Let a be 10 and let b be 20.\n", "Print a plus b.\n",),
        &["30"],
    );
}

#[test]
fn compound_set_chain() {
    assert_program(
        "set-and-set",
        concat!(
            "Let a be 0.\n",
            "Let b be 0.\n",
            "Set a to 1, and set b to 2.\n",
            "Print a plus b.\n",
        ),
        &["3"],
    );
}

#[test]
fn start_at_and_begin_at_aliases() {
    assert_program(
        "begin-at",
        concat!(
            "Let a starts at 3.\n",
            "Let b begins at 4.\n",
            "Print a plus b.\n",
        ),
        &["7"],
    );
}

#[test]
fn multi_part_sum_np_is_not_split() {
    assert_program(
        "multi-sum",
        "Print \"Sum: \" with the sum of a and b and \".\", where a is 12 and b is 23.\n",
        &["Sum: 35."],
    );
}

#[test]
fn multi_part_with_multiple_groups() {
    assert_program(
        "multi-groups",
        concat!(
            "Let a be 12.\n",
            "Let b be 23.\n",
            "Print \"a=\" with a and \", b=\" with b.\n",
        ),
        &["a=12, b=23"],
    );
}

#[test]
fn multi_part_followed_by_joiner() {
    assert_program("followed-by", "Print \"X=\" followed by 7.\n", &["X=7"]);
}

#[test]
fn repeat_until_paragraph_loop_without_end() {
    assert_program(
        "repeat-until",
        concat!(
            "Let n be 0.\n",
            "Repeat until n is 5:\n",
            "Add 1 to n.\n",
            "\n",
            "Print n.\n",
        ),
        &["5"],
    );
}

#[test]
fn repeat_while_paragraph_loop_without_end() {
    assert_program(
        "repeat-while",
        concat!(
            "Let n be 0.\n",
            "Repeat while n is less than 4:\n",
            "Add 1 to n.\n",
            "\n",
            "Print n.\n",
        ),
        &["4"],
    );
}

#[test]
fn plain_while_colon_paragraph_loop_without_end() {
    assert_program(
        "while-colon",
        concat!(
            "Let n be 0.\n",
            "While n is less than 3:\n",
            "Add 1 to n.\n",
            "\n",
            "Print n.\n",
        ),
        &["3"],
    );
}

#[test]
fn inline_if_compound_and_mutations() {
    assert_program(
        "if-compound",
        concat!(
            "Let x be 2.\n",
            "If x is 2, multiply x by 3 and add 4 to x.\n",
            "Print x.\n",
        ),
        &["10"],
    );
}

#[test]
fn inline_otherwise_compound_and_mutations() {
    assert_program(
        "otherwise-compound",
        concat!(
            "Let x be 3.\n",
            "If x is 2, print \"no\".\n",
            "Otherwise, multiply x by 3 and add 1 to x.\n",
            "Print x.\n",
        ),
        &["10"],
    );
}

#[test]
fn quote_final_period_question_and_exclamation_are_executable() {
    assert_program(
        "quote-punctuation",
        concat!("Print \"Hi.\"\n", "Print \"Ready?\"\n", "Print \"Go!\"\n",),
        &["Hi.", "Ready?", "Go!"],
    );
}

#[test]
fn multipart_emit_on_show_verb() {
    assert_program(
        "show-multipart",
        concat!("Let n be 3.\n", "Show \"n=\" with n.\n",),
        &["n=3"],
    );
}

#[test]
fn paragraph_loop_keeps_following_error_on_exact_line() {
    let directory = temp_dir("line-span");
    let program = directory.join("loop_span.eng");
    fs::write(
        &program,
        concat!(
            "Let n be 0.\n",
            "Repeat while n is less than 2:\n",
            "Add 1 to n.\n",
            "\n",
            "Printt \"x\".\n",
        ),
    )
    .expect("program");
    for command in ["build", "run", "interp"] {
        assert_diagnostic_line(command, &program, ":5:1: Lowering:", ":6:1:");
    }
    let _ = fs::remove_dir_all(&directory);
}
