//! Fast LeetCode MBT manifest integrity and stratified parity sample.
//!
//! The full 42-problem (and growing) three-engine sweep lives in
//! `scripts/run_leetcode_mbt.py` behind `make mbt`. Default `make test` keeps
//! a manifest check plus 25 evenly spaced programs so CI stays fast.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use tc_editor::runner::{RunResult, run_elf_with_parity};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

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

#[test]
fn leetcode_manifest_and_stratified_sample() {
    let bench = repo_root().join("benchmarks/leetcode");
    let problems_dir = bench.join("problems");
    let expected_dir = bench.join("expected");
    let mut programs = fs::read_dir(&problems_dir)
        .expect("problems directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "eng"))
        .collect::<Vec<_>>();
    programs.sort();
    assert!(
        programs.len() >= 42,
        "expected at least 42 solved programs, found {}",
        programs.len()
    );

    let mut seen = HashSet::new();
    for program in &programs {
        let program_stem = program
            .file_stem()
            .and_then(|value| value.to_str())
            .expect("UTF-8 program stem");
        let (number, slug) = program_stem.split_once('_').expect("NNNN_slug filename");
        let id = number.parse::<u32>().expect("numeric problem number");
        assert!(seen.insert(id), "duplicate problem number {id}");
        assert_eq!(number.len(), 4, "problem number must be zero-padded");
        assert!(!slug.is_empty(), "missing slug for {program_stem}");
        let expected = expected_dir.join(format!("{program_stem}.out"));
        assert!(
            expected.is_file(),
            "missing expected output for {program_stem}"
        );
        let text = fs::read_to_string(&expected).expect("expected output");
        assert!(!text.is_empty(), "empty expected output for {program_stem}");
    }

    // A deterministic stratified sample across the whole solved range.
    let sample_size = programs.len().min(25);
    let stride = programs.len() as f64 / sample_size as f64;
    for index in 0..sample_size {
        let position = ((index as f64) * stride).floor() as usize;
        let program = &programs[position.min(programs.len() - 1)];
        let program_stem = program
            .file_stem()
            .and_then(|value| value.to_str())
            .expect("UTF-8 program stem");
        let source = fs::read_to_string(program).expect("program source");
        let expected = fs::read_to_string(expected_dir.join(format!("{program_stem}.out")))
            .expect("expected output");

        let result = run_elf_with_parity(&source, Some(Path::new(program)), "");
        assert!(
            result.ok,
            "{program_stem}: {}\n{}",
            result.summary,
            result.lines.join("\n")
        );
        assert_eq!(
            stdout_lines(&result),
            expected.lines().map(str::to_string).collect::<Vec<_>>(),
            "{program_stem}: stdout mismatch"
        );
    }
}
