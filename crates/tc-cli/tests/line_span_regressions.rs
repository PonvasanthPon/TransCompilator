//! Exact source-span regressions for normalization and inline clause expansion.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

const ELEVEN_LINE_PROGRAM: &str = concat!(
    "She showed me the book yesterday.\n",
    "\n",
    "Let n be six.\n",
    "Square n.\n",
    "If n is divisible by nine, print \"yes\".\n",
    "Otherwise, print \"no\".\n",
    "Add four to it, then echo it.\n",
    "\n",
    "She showed me the book yesterday.\n",
    "\n",
    "Printt \"12\".\n",
);

fn temp_dir(tag: &str) -> PathBuf {
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!(
        "tc_r1_line_spans_{}_{}_{tag}",
        std::process::id(),
        counter
    ));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("temp directory");
    directory
}

fn write_program(directory: &Path, name: &str, source: &str) -> PathBuf {
    let program = directory.join(format!("{name}.eng"));
    fs::write(&program, source).expect("program");
    program
}

fn assert_diagnostic_line(command: &str, program: &Path, expected: &str, forbidden: &str) {
    let mut invocation = Command::new(env!("CARGO_BIN_EXE_tc"));
    invocation.arg(command).arg(program);
    if command == "build" {
        invocation.arg("-o").arg(program.with_extension("elf"));
    }
    let output = invocation.output().expect("run tc");
    assert!(
        !output.status.success(),
        "{command} unexpectedly succeeded for {}",
        program.display()
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(expected),
        "{command} missing `{expected}`:\n{stderr}"
    );
    assert!(
        !stderr.contains(forbidden),
        "{command} contains forbidden `{forbidden}`:\n{stderr}"
    );
}

#[test]
fn exact_eleven_line_program_reports_printt_on_line_11() {
    let directory = temp_dir("eleven");
    let program = write_program(&directory, "repro11", ELEVEN_LINE_PROGRAM);
    for command in ["build", "run", "interp"] {
        assert_diagnostic_line(command, &program, ":11:1: Lowering:", ":12:1:");
    }
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn line_one_typo_without_use_keyword_reports_line_1() {
    let directory = temp_dir("line1");
    let program = write_program(&directory, "repro1", "Printt \"12\".\n");
    for command in ["build", "run", "interp"] {
        assert_diagnostic_line(command, &program, ":1:1: Lowering:", ":2:1:");
    }
    let _ = fs::remove_dir_all(&directory);
}
