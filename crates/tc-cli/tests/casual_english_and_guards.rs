//! Casual-English parity, typo guards, zero-executable warnings, and reproducibility.

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

fn assert_compile_failure(name: &str, source: &str, needles: &[&str]) {
    let result = run_elf_with_parity(source, None, "");
    assert!(!result.ok, "{name}: unexpectedly compiled");
    let text = result.lines.join("\n").to_ascii_lowercase();
    for needle in needles {
        assert!(
            text.contains(&needle.to_ascii_lowercase()),
            "{name}: missing `{needle}` in:\n{}",
            result.lines.join("\n")
        );
    }
}

fn assert_omitted(name: &str, source: &str) {
    let result = run_elf_with_parity(source, None, "");
    assert!(result.ok, "{name}: {}", result.lines.join("\n"));
    assert!(stdout_lines(&result).is_empty(), "{name}");
    assert!(!result.omitted_lines.is_empty(), "{name}");
}

fn temp_dir(tag: &str) -> PathBuf {
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "tc_r1_casual_{}_{}_{tag}",
        std::process::id(),
        counter
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("temp dir");
    path
}

#[test]
fn show_me_variable() {
    assert_program("show-me", "Let x be 7. Show me x.\n", &["7"]);
}

#[test]
fn tell_me_variable() {
    assert_program("tell-me", "Let x be 8. Tell me x.\n", &["8"]);
}

#[test]
fn print_out_variable() {
    assert_program("print-out", "Let x be 9. Print out x.\n", &["9"]);
}

#[test]
fn write_out_variable() {
    assert_program("write-out", "Let total be 9. Write out total.\n", &["9"]);
}

#[test]
fn echo_literal() {
    assert_program("echo", "Echo 10.\n", &["10"]);
}

#[test]
fn tell_me_recursive_np_where() {
    assert_program(
        "tell-me-np",
        "Tell me the sum of a and b, where a is 12 and b is 23.\n",
        &["35"],
    );
}

#[test]
fn square_variable() {
    assert_program("square", "Let x be 5. Square x. Say x.\n", &["25"]);
}

#[test]
fn cube_variable() {
    assert_program("cube", "Let x be 3. Cube x. Say x.\n", &["27"]);
}

#[test]
fn double_variable() {
    assert_program("double", "Let x be 5. Double x. Say x.\n", &["10"]);
}

#[test]
fn halve_variable() {
    assert_program("halve", "Let x be 10. Halve x. Say x.\n", &["5"]);
}

#[test]
fn negate_variable() {
    assert_program("negate", "Let x be 5. Negate x. Say x.\n", &["-5"]);
}

#[test]
fn square_record_field() {
    assert_program(
        "square-field",
        concat!(
            "A record Waypoint has distance (a whole number) and toll (a whole number).\n",
            "Create a Waypoint called pondy.\n",
            "Update pondy so its distance is 5 and its toll is 2.\n",
            "Square pondy's distance.\n",
            "Say pondy's distance.\n",
        ),
        &["25"],
    );
}

#[test]
fn double_it_anaphor() {
    assert_program("double-it", "Let x be 5. Double it. Say x.\n", &["10"]);
}

#[test]
fn halve_its_field_anaphor() {
    assert_program(
        "halve-its",
        concat!(
            "A record Waypoint has distance (a whole number) and toll (a whole number).\n",
            "Create a Waypoint called pondy.\n",
            "Update pondy so its distance is 5 and its toll is 10.\n",
            "Halve its toll.\n",
            "Say pondy's toll.\n",
        ),
        &["5"],
    );
}

#[test]
fn square_decimal() {
    assert_program(
        "square-f64",
        "Let x be 1.5. Square x. Say x.\n",
        &["2.2500"],
    );
}

#[test]
fn double_decimal() {
    assert_program(
        "double-f64",
        "Let x be 1.5. Double x. Say x.\n",
        &["3.0000"],
    );
}

#[test]
fn halve_decimal() {
    assert_program("halve-f64", "Let x be 2.5. Halve x. Say x.\n", &["1.2500"]);
}

#[test]
fn repeat_inline_comma() {
    assert_program(
        "repeat-inline",
        "Repeat 3 times, print \"tick\".\n",
        &["tick", "tick", "tick"],
    );
}

#[test]
fn repeat_inline_then_chain() {
    assert_program(
        "repeat-then",
        "Let x be 0.\nRepeat 3 times, add 1 to x, then print x.\n",
        &["1", "2", "3"],
    );
}

#[test]
fn inline_if_true_branch() {
    assert_program(
        "if-true",
        "Let x be 3.\nIf x is 3, print \"done\".\nOtherwise, print \"other\".\n",
        &["done"],
    );
}

#[test]
fn inline_if_false_branch() {
    assert_program(
        "if-false",
        "Let x be 5.\nIf x is 3, print \"done\".\nOtherwise, print \"other\".\n",
        &["other"],
    );
}

#[test]
fn else_comma_alias() {
    assert_program(
        "else-comma",
        "Let x be 5.\nIf x is 3, print \"done\".\nElse, print \"other\".\n",
        &["other"],
    );
}

#[test]
fn orphan_otherwise_is_clean_error() {
    assert_compile_failure(
        "orphan-otherwise",
        "Otherwise, print \"other\".\nReturn 0.\n",
        &["otherwise", "must follow an `if`"],
    );
}

#[test]
fn for_each_list_i64_inline() {
    assert_program(
        "foreach-i64",
        concat!(
            "Use io and list.\n",
            "Let prices be List of whole number.\n",
            "Push 3 to address of prices.\n",
            "Push 5 to address of prices.\n",
            "Let total be 0.\n",
            "For each price in prices, add price to total.\n",
            "Say total.\n",
            "Free list address of prices.\n",
        ),
        &["8"],
    );
}

#[test]
fn for_each_list_i64_block_period() {
    assert_program(
        "foreach-block",
        concat!(
            "Use io and list.\n",
            "Let prices be List of whole number.\n",
            "Push 3 to address of prices.\n",
            "Push 5 to address of prices.\n",
            "For each price in prices.\n",
            "    Say price.\n",
            "End.\n",
            "Free list address of prices.\n",
        ),
        &["3", "5"],
    );
}

#[test]
fn for_each_list_decimal() {
    assert_program(
        "foreach-f64",
        concat!(
            "Use io and list.\n",
            "Let values be List of decimal.\n",
            "Push 1.5 to address of values.\n",
            "Push 2.5 to address of values.\n",
            "Let total be 0.0.\n",
            "For each value in values, add value to total.\n",
            "Say total.\n",
            "Free list address of values.\n",
        ),
        &["4.0000"],
    );
}

#[test]
fn for_each_list_text() {
    assert_program(
        "foreach-text",
        concat!(
            "Use alloc, core, io, list, and string.\n",
            "Let words be List of TcString.\n",
            "Let a be create text from \"a\" of length 1.\n",
            "Let btext be create text from \"b\" of length 1.\n",
            "Push a to address of words.\n",
            "Push btext to address of words.\n",
            "For each word in words, show address of word.\n",
            "Free list address of words.\n",
            "Release text address of a.\n",
            "Release text address of btext.\n",
        ),
        &["a", "b"],
    );
}

#[test]
fn for_each_list_record_its_field() {
    assert_program(
        "foreach-record",
        concat!(
            "Use io and list.\n",
            "A record Waypoint has distance (a whole number) and toll (a whole number).\n",
            "Let route be List of Waypoint.\n",
            "Let waypoint be Waypoint.\n",
            "Set waypoint's distance to 4.\n",
            "Push waypoint to address of route.\n",
            "Set waypoint's distance to 7.\n",
            "Push waypoint to address of route.\n",
            "Let total be 0.\n",
            "For each waypoint in route, add its distance to total.\n",
            "Say total.\n",
            "Free list address of route.\n",
        ),
        &["11"],
    );
}

#[test]
fn narrative_show_me_is_omitted() {
    assert_omitted(
        "narr-show-me",
        "She showed me the photo yesterday.\nReturn 0.\n",
    );
}

#[test]
fn narrative_tell_me_is_omitted() {
    assert_omitted(
        "narr-tell-me",
        "He told me a story last night.\nReturn 0.\n",
    );
}

#[test]
fn narrative_print_out_is_omitted() {
    assert_omitted(
        "narr-print-out",
        "She printed out the essay yesterday.\nReturn 0.\n",
    );
}

#[test]
fn narrative_write_out_is_omitted() {
    assert_omitted(
        "narr-write-out",
        "She wrote out the recipe yesterday.\nReturn 0.\n",
    );
}

#[test]
fn narrative_squared_is_omitted() {
    assert_omitted("narr-square", "He squared his shoulders.\nReturn 0.\n");
}

#[test]
fn narrative_echo_is_omitted() {
    assert_omitted(
        "narr-echo",
        "She echoed the greeting yesterday.\nReturn 0.\n",
    );
}

#[test]
fn typo_printt_cli_rejected_with_suggestion() {
    let directory = temp_dir("typo");
    let source = directory.join("typo.eng");
    fs::write(&source, "Printt 5.\n").expect("source");
    let output = Command::new(env!("CARGO_BIN_EXE_tc"))
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(directory.join("typo.elf"))
        .output()
        .expect("run tc");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
    assert!(stderr.contains("unrecognized imperative"), "{stderr}");
    assert!(stderr.contains("did you mean 'print'?"), "{stderr}");
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn typo_sqare_cli_rejected_with_suggestion() {
    let directory = temp_dir("typo-square");
    let source = directory.join("typo.eng");
    fs::write(&source, "Squar 5.\n").expect("source");
    let output = Command::new(env!("CARGO_BIN_EXE_tc"))
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(directory.join("typo.elf"))
        .output()
        .expect("run tc");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
    assert!(stderr.contains("unrecognized imperative"), "{stderr}");
    assert!(stderr.contains("did you mean 'square'?"), "{stderr}");
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn frobnicate_warns_zero_executable_statements() {
    let directory = temp_dir("zero");
    let source = directory.join("zero.eng");
    fs::write(&source, "Frobnicate the flux capacitor.\n").expect("source");
    let output = Command::new(env!("CARGO_BIN_EXE_tc"))
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(directory.join("zero.elf"))
        .output()
        .expect("run tc");
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("program produced 0 executable statements"),
        "{stderr}"
    );
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn reproducible_elf64_sha256() {
    let directory = temp_dir("repro");
    let source = directory.join("repro.eng");
    fs::write(
        &source,
        "Print the sum of a and b, where a is 12 and b is 23.\n",
    )
    .expect("source");
    let first = directory.join("first.elf");
    let second = directory.join("second.elf");
    for output in [&first, &second] {
        let status = Command::new(env!("CARGO_BIN_EXE_tc"))
            .arg("build")
            .arg(&source)
            .arg("-o")
            .arg(output)
            .status()
            .expect("run tc");
        assert!(status.success());
    }
    let first_hash = Command::new("sha256sum")
        .arg(&first)
        .output()
        .expect("sha256sum");
    let second_hash = Command::new("sha256sum")
        .arg(&second)
        .output()
        .expect("sha256sum");
    let first_digest = first_hash
        .stdout
        .split(|byte| *byte == b' ')
        .next()
        .unwrap_or_default();
    let second_digest = second_hash
        .stdout
        .split(|byte| *byte == b' ')
        .next()
        .unwrap_or_default();
    assert_eq!(first_digest, second_digest);
    let _ = fs::remove_dir_all(directory);
}
