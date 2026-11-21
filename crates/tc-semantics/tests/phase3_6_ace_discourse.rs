//! Phase 3.6 ACE-inspired discourse, comma precedence, quantifier, and
//! paraphrase regressions.  Every executable program asserts exact parity
//! between the reference IR interpreter and the static x86-64 ELF64 backend.

use std::fmt::Write as _;
use std::io::Write as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use tc_codegen_x86::compile_to_elf;
use tc_ir::{interpret, verify_module};
use tc_semantics::{
    SourceFile, compile_english_to_ir, format_paraphrase, format_paraphrase_entries, lower_project,
};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_path(tag: &str, extension: &str) -> PathBuf {
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "tc_r1_phase36_{}_{}_{tag}.{extension}",
        std::process::id(),
        counter
    ))
}

#[derive(Debug)]
struct RunResult {
    exit_code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn run_elf(path: &Path, args: &[&str], stdin: &[u8]) -> RunResult {
    assert!(!args.is_empty(), "argv must include argv[0]");
    let mut child = Command::new(path)
        .arg0(args[0])
        .args(&args[1..])
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ELF");
    child
        .stdin
        .take()
        .expect("stdin pipe")
        .write_all(stdin)
        .expect("write stdin");
    let output = child.wait_with_output().expect("wait ELF");
    RunResult {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: output.stdout,
        stderr: output.stderr,
    }
}

fn wrap(value: i64) -> i32 {
    i32::try_from(value.rem_euclid(256)).unwrap_or_default()
}

fn compile_sources(name: &str, sources: &[SourceFile]) -> tc_semantics::CompiledProject {
    let compiled = compile_english_to_ir(sources, "main")
        .unwrap_or_else(|errors| panic!("{name}: English compilation failed: {errors:#?}"));
    verify_module(&compiled.module)
        .unwrap_or_else(|errors| panic!("{name}: IR verification failed: {errors:#?}"));
    compiled
}

fn assert_english(name: &str, text: &str, expected_exit: i32, expected_stdout: &[u8]) {
    assert_english_files(name, &[("main.tc", text)], expected_exit, expected_stdout);
}

fn assert_english_files(
    name: &str,
    files: &[(&str, &str)],
    expected_exit: i32,
    expected_stdout: &[u8],
) {
    let sources = files
        .iter()
        .map(|(file, text)| SourceFile::new(*file, *text))
        .collect::<Vec<_>>();
    let compiled = compile_sources(name, &sources);
    let args = ["prog".to_string()];
    let interpreted = interpret(&compiled.module, &args, &[], 50_000_000)
        .unwrap_or_else(|error| panic!("{name}: interpreter failed: {error:?}"));
    assert_eq!(
        interpreted.exit_code,
        expected_exit,
        "{name}: interpreter exit; stdout={:?}",
        String::from_utf8_lossy(&interpreted.stdout)
    );
    assert_eq!(
        interpreted.stdout, expected_stdout,
        "{name}: interpreter stdout"
    );
    let path = unique_path(name, "elf");
    compile_to_elf(&compiled.module, &path)
        .unwrap_or_else(|error| panic!("{name}: ELF compilation failed: {error}"));
    let elf = run_elf(&path, &["prog"], &[]);
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        elf.exit_code,
        interpreted.exit_code,
        "{name}: ELF/interpreter exit mismatch; stderr={:?}",
        String::from_utf8_lossy(&elf.stderr)
    );
    assert_eq!(
        elf.stdout, interpreted.stdout,
        "{name}: ELF/interpreter stdout mismatch"
    );
    assert_eq!(elf.stderr, interpreted.stderr, "{name}: stderr mismatch");
    assert_eq!(elf.exit_code, expected_exit, "{name}: ELF exit");
    assert_eq!(elf.stdout, expected_stdout, "{name}: ELF stdout");
}

fn compile_error(name: &str, text: &str) -> String {
    let sources = [SourceFile::new("main.tc", text)];
    match compile_english_to_ir(&sources, "main") {
        Err(errors) => {
            assert!(
                errors.iter().any(|error| !error.message.is_empty()),
                "{name}: expected a structured error"
            );
            errors
                .iter()
                .map(|error| error.message.clone())
                .collect::<Vec<_>>()
                .join("\n")
        }
        Ok(_) => panic!("{name}: expected compilation to fail"),
    }
}

fn assert_error_contains(name: &str, text: &str, needle: &str) {
    let message = compile_error(name, text);
    assert!(
        message.contains(needle),
        "{name}: expected `{needle}` in error, got: {message}"
    );
}

fn paraphrase(text: &str) -> String {
    let sources = [SourceFile::new("main.tc", text)];
    let project = lower_project(&sources, "main").expect("paraphrase lowering");
    format_paraphrase(&project.paraphrase)
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn tc_binary() -> PathBuf {
    workspace_root().join("target/debug/tc")
}

// ---------------------------------------------------------------------------
// Group A - pronoun anaphora `it`
// ---------------------------------------------------------------------------

#[test]
fn anaphor_it_basic_chain() {
    assert_english(
        "it-basic",
        "Put 500 in cash.\nAdd 100 to it.\nRemove 50 from it.\nDouble it.\nReturn it.\n",
        wrap(1100),
        b"",
    );
}

#[test]
fn anaphor_it_most_recent_binding() {
    assert_english(
        "it-recent",
        "Put 10 in a.\nPut 20 in b.\nAdd 5 to it.\nReturn a * 100 + b.\n",
        wrap(1025),
        b"",
    );
}

#[test]
fn anaphor_it_recency_updated_by_mutation() {
    assert_english(
        "it-touch",
        "Put 10 in a.\nPut 20 in b.\nAdd 3 to a.\nDouble it.\nReturn a * 100 + b.\n",
        wrap(2620),
        b"",
    );
}

#[test]
fn anaphor_it_rhs_before_lhs_intro() {
    assert_english(
        "it-rhs",
        "Put 10 in x.\ny is it + 5.\nAdd 1 to it.\nReturn x * 100 + y.\n",
        wrap(1016),
        b"",
    );
}

#[test]
fn anaphor_it_in_if_condition_and_body() {
    assert_english(
        "it-if",
        "Put 50 in cash.\nIf it is more than 20, remove 10 from it.\nReturn it.\n",
        40,
        b"",
    );
}

#[test]
fn anaphor_it_in_while_loop() {
    assert_english(
        "it-while",
        "Put 1 in n.\nWhile it is less than 5, add 1 to it.\nReturn it.\n",
        5,
        b"",
    );
}

#[test]
fn anaphor_it_in_repeat_loop() {
    assert_english(
        "it-repeat",
        "Put 10 in x.\nRepeat 4 times, add 3 to it.\nReturn it.\n",
        22,
        b"",
    );
}

#[test]
fn anaphor_it_function_param() {
    assert_english(
        "it-param",
        "Define function dbl taking n as i64 returning i64:\n    Double it.\n    Return it.\nReturn dbl(21).\n",
        42,
        b"",
    );
}

#[test]
fn anaphor_it_after_transfer() {
    assert_english(
        "it-transfer",
        "Put 100 in bank.\nPut 20 in wallet.\nTransfer 30 from bank to wallet.\nAdd 5 to it.\nReturn wallet.\n",
        55,
        b"",
    );
}

#[test]
fn anaphor_it_passive_voice() {
    assert_english(
        "it-passive",
        "Put 100 in cash.\n25 is removed from it.\n10 is added to it.\nReturn it.\n",
        85,
        b"",
    );
}

#[test]
fn anaphor_it_with_float() {
    assert_english(
        "it-float",
        "Put 2.5 in r.\nMultiply it by 4.0.\nReturn it as i64.\n",
        10,
        b"",
    );
}

#[test]
fn anaphor_it_with_phrase_call() {
    assert_english(
        "it-phrase",
        concat!(
            "Define function echo taking v as i64 returning i64:\n",
            "    Phrase: \"echo <v>\"\n",
            "    Return v.\n",
            "Put 42 in x.\nReturn echo it.\n",
        ),
        42,
        b"",
    );
}

// ---------------------------------------------------------------------------
// Group B - possessive and definite noun phrases
// ---------------------------------------------------------------------------

#[test]
fn anaphor_its_field_basic() {
    assert_english(
        "its-basic",
        "Define struct Point with x as i64 and y as i64.\nCreate a Point p.\nSet its x to 7.\nSet its y to 8.\nReturn p.x + p.y.\n",
        15,
        b"",
    );
}

#[test]
fn anaphor_its_field_ptr_param() {
    assert_english(
        "its-ptr",
        concat!(
            "Define struct Point with x as i64 and y as i64.\n",
            "Define function bump taking p as ptr<Point> returning i64:\n",
            "    Set its x to 11.\n",
            "    Return its x.\n",
            "Create a Point q.\n",
            "Return bump(address of q).\n",
        ),
        11,
        b"",
    );
}

#[test]
fn anaphor_its_field_specificity_fallback() {
    assert_english(
        "its-specific",
        concat!(
            "Define struct Point with x as i64 and y as i64.\n",
            "Define struct Rect with w as i64 and h as i64.\n",
            "Create a Point p.\nCreate a Rect r.\n",
            "Set its w to 20.\nSet its x to 5.\n",
            "Return r.w * 100 + p.x.\n",
        ),
        wrap(2005),
        b"",
    );
}

#[test]
fn anaphor_definite_type_basic() {
    assert_english(
        "the-type",
        "Define struct Point with x as i64 and y as i64.\nCreate a Point called origin.\nSet the point's x to 12.\nSet the point's y to 3.\nReturn origin.x + origin.y.\n",
        15,
        b"",
    );
}

#[test]
fn anaphor_definite_type_recency() {
    assert_english(
        "the-recent",
        "Define struct Point with x as i64 and y as i64.\nCreate a Point a.\nCreate a Point b.\nSet the point's x to 9.\nReturn a.x * 10 + b.x.\n",
        9,
        b"",
    );
}

#[test]
fn anaphor_appositive_name() {
    assert_english(
        "appositive",
        "Define struct Point with x as i64 and y as i64.\nCreate a Point a.\nCreate a Point b.\nSet the point a's x to 4.\nSet the point b's x to 6.\nReturn a.x * 10 + b.x.\n",
        46,
        b"",
    );
}

#[test]
fn anaphor_definite_type_in_phrase_call() {
    assert_english(
        "the-phrase",
        concat!(
            "Define struct Point with x as i64 and y as i64.\n",
            "Create a Point origin.\nSet its x to 33.\n",
            "Define function read_x taking pt as Point returning i64:\n",
            "    Phrase: \"read x of <pt>\"\n",
            "    Return its x.\n",
            "Return read x of the point.\n",
        ),
        33,
        b"",
    );
}

#[test]
fn anaphor_definite_type_shadowed_by_var() {
    assert_english(
        "the-shadow",
        "Define struct Point with x as i64 and y as i64.\nCreate a Point origin.\nPut 77 in point.\nAdd 3 to the point.\nReturn point.\n",
        80,
        b"",
    );
}

#[test]
fn anaphor_generic_struct_its_field() {
    assert_english(
        "its-generic",
        concat!(
            "Define struct Pair of T and U with first as T and second as U.\n",
            "Create a Pair of i64 and i64 called pr.\n",
            "Set its first to 19.\nSet its second to 23.\n",
            "Return its first + its second.\n",
        ),
        42,
        b"",
    );
}

#[test]
fn anaphor_for_every_struct_array_its_field() {
    assert_english(
        "its-array",
        concat!(
            "Define struct Point with x as i64 and y as i64.\n",
            "Define local pts as array<Point, 2>.\n",
            "Set pts[0].x to 3.\nSet pts[1].x to 4.\n",
            "Put 0 in sum.\nFor every pt in pts, add its x to sum.\n",
            "Return sum.\n",
        ),
        7,
        b"",
    );
}

// ---------------------------------------------------------------------------
// Group C - scope accessibility, expletive prose, negative errors
// ---------------------------------------------------------------------------

#[test]
fn scope_if_inner_does_not_leak_to_outer() {
    assert_english(
        "scope-if",
        "Put 10 in x.\nIf x > 5:\n    Put 99 in inner.\nReturn it.\n",
        10,
        b"",
    );
}

#[test]
fn scope_if_inner_does_not_leak_to_else() {
    assert_error_contains(
        "scope-else",
        "If 1 == 0:\n    Put 99 in inner.\nElse:\n    Return it.\n",
        "UNRESOLVED_ANAPHOR",
    );
}

#[test]
fn scope_loop_inner_does_not_leak() {
    assert_english(
        "scope-loop",
        "Put 7 in outer.\nRepeat 2 times:\n    Put 88 in tmp.\nReturn it.\n",
        7,
        b"",
    );
}

#[test]
fn scope_where_does_not_leak() {
    assert_english(
        "scope-where",
        "Put 5 in base.\nShow_or_ret is base + 1, where a is 10.\nAdd 1 to it.\nReturn it.\n",
        7,
        b"",
    );
}

#[test]
fn scope_function_isolation() {
    assert_error_contains(
        "scope-fn",
        concat!(
            "Define function a taking n as i64 returning i64:\n",
            "    Put n in secret.\n    Return 0.\n",
            "Define function b returning i64:\n",
            "    Return it.\n",
            "Return a(1).\n",
        ),
        "UNRESOLVED_ANAPHOR",
    );
}

#[test]
fn expletive_it_prose_omitted() {
    assert_english(
        "expletive",
        "Put 10 in x.\nIt was raining heavily.\nIt is 3 pm.\nIt seems fine.\nAdd 5 to it.\nReturn it.\n",
        15,
        b"",
    );
}

#[test]
fn err_unresolved_it_at_start() {
    assert_error_contains("err-it", "Add 10 to it.\n", "UNRESOLVED_ANAPHOR");
}

#[test]
fn err_unresolved_its_field_no_struct() {
    assert_error_contains(
        "err-its",
        "Put 10 in x.\nSet its z to 5.\n",
        "UNRESOLVED_ANAPHOR",
    );
}

#[test]
fn err_unresolved_its_field_missing() {
    assert_error_contains(
        "err-its-field",
        "Define struct Point with x as i64.\nCreate a Point p.\nSet its nonexistent to 5.\n",
        "UNRESOLVED_ANAPHOR",
    );
}

#[test]
fn err_unresolved_definite_type() {
    assert_error_contains(
        "err-the",
        "Define struct Point with x as i64.\nSet the point's x to 5.\n",
        "UNRESOLVED_ANAPHOR",
    );
}

// ---------------------------------------------------------------------------
// Group D - comma coordinator precedence and scoped negation
// ---------------------------------------------------------------------------

#[test]
fn prec_bare_and_beats_or() {
    assert_english(
        "prec-bare",
        "a is 0.\nb is 0.\nc is 1.\nres is 0.\nIf a == 0 or b == 0 and c == 1, add 10 to res.\nReturn res.\n",
        10,
        b"",
    );
}

#[test]
fn prec_comma_and_reverses_or() {
    assert_english(
        "prec-comma",
        "a is 0.\nb is 1.\nc is 0.\nres is 0.\nIf a == 0 or b == 0, and c == 1, add 10 to res.\nReturn res.\n",
        0,
        b"",
    );
}

#[test]
fn prec_comma_or_lowest() {
    assert_english(
        "prec-comma-or",
        "a is 1.\nb is 1.\nc is 0.\nres is 0.\nIf a == 1 or b == 1, and c == 0, or a == 1, add 10 to res.\nReturn res.\n",
        10,
        b"",
    );
}

#[test]
fn prec_comma_and_with_indented_block() {
    assert_english(
        "prec-block",
        "a is 0.\nb is 1.\nc is 0.\nres is 0.\nIf a == 0 or b == 1, and c == 0:\n    Add 10 to res.\nReturn res.\n",
        10,
        b"",
    );
}

#[test]
fn prec_comma_and_with_inline_body() {
    assert_english(
        "prec-inline",
        "a is 0.\nb is 1.\nc is 0.\nres is 0.\nIf a == 0 or b == 1, and c == 0, add 10 to res.\nReturn res.\n",
        10,
        b"",
    );
}

#[test]
fn prec_comma_and_with_then() {
    assert_english(
        "prec-then",
        "a is 0.\nb is 1.\nc is 0.\nres is 0.\nIf a == 0 or b == 1, and c == 0, then add 10 to res.\nReturn res.\n",
        10,
        b"",
    );
}

#[test]
fn prec_between_inside_comma_and() {
    assert_english(
        "prec-between",
        "a is 0.\nb is 1.\nc is 0.\nres is 0.\nIf a is between 1 and 5 or b is between 0 and 2, and c == 0, add 4 to res.\nReturn res.\n",
        4,
        b"",
    );
}

#[test]
fn prec_scoped_negation_single() {
    assert_english(
        "neg-single",
        "a is 0.\nb is 1.\nres is 0.\nIf it is false that a == 1 and b == 1, add 5 to res.\nReturn res.\n",
        5,
        b"",
    );
}

#[test]
fn prec_scoped_negation_repeated_that() {
    assert_english(
        "neg-that",
        "a is 0.\nb is 1.\nres is 0.\nIf it is false that a == 0 and that b == 1, add 7 to res.\nReturn res + 3.\n",
        3,
        b"",
    );
}

#[test]
fn prec_short_circuit_preserved() {
    assert_english(
        "prec-short",
        concat!(
            "Define global calls as i64.\n",
            "Define function side_effect taking v as i64 returning i64:\n",
            "    calls is calls + 1.\n    Return 1.\n",
            "a is 0.\nb is 0.\n",
            "If a == 1 or b == 1, and side_effect(1) == 1, return 9.\n",
            "Return calls.\n",
        ),
        0,
        b"",
    );
}

// ---------------------------------------------------------------------------
// Group E - global quantifiers
// ---------------------------------------------------------------------------

#[test]
fn for_every_range_inline() {
    assert_english(
        "for-inline",
        "Put 0 in sum.\nFor every i from 1 to 10, add i to sum.\nReturn sum.\n",
        55,
        b"",
    );
}

#[test]
fn for_every_range_indented_block() {
    assert_english(
        "for-block",
        "Put 0 in sum.\nFor every i from 1 to 5:\n    Add i * i to sum.\nReturn sum.\n",
        55,
        b"",
    );
}

#[test]
fn for_every_range_with_step() {
    assert_english(
        "for-step",
        "Put 0 in sum.\nFor every i from 0 to 10 by 2, add i to sum.\nReturn sum.\n",
        30,
        b"",
    );
}

#[test]
fn for_every_range_empty() {
    assert_english(
        "for-empty",
        "Put 0 in sum.\nFor every i from 10 to 5, add 1 to sum.\nReturn sum.\n",
        0,
        b"",
    );
}

#[test]
fn for_every_range_continue_no_inf_loop() {
    assert_english(
        "for-continue",
        "Put 0 in sum.\nFor every i from 1 to 5:\n    If i == 3, continue.\n    Add i to sum.\nReturn sum.\n",
        12,
        b"",
    );
}

#[test]
fn for_every_range_break() {
    assert_english(
        "for-break",
        "Put 0 in sum.\nFor every i from 1 to 100:\n    If i == 6, break.\n    Add i to sum.\nReturn sum.\n",
        15,
        b"",
    );
}

#[test]
fn for_every_nested_2d_range() {
    assert_english(
        "for-nested",
        "Put 0 in count.\nFor every i from 1 to 4:\n    For every j from 1 to 3:\n        Add 1 to count.\nReturn count.\n",
        12,
        b"",
    );
}

#[test]
fn for_every_fixed_array() {
    assert_english(
        "for-array",
        concat!(
            "Define local arr as array<i64, 5>.\n",
            "Set arr[0] to 10.\nSet arr[1] to 20.\nSet arr[2] to 30.\n",
            "Set arr[3] to 40.\nSet arr[4] to 50.\n",
            "Put 0 in sum.\nFor every v in arr, add v to sum.\nReturn sum.\n",
        ),
        150,
        b"",
    );
}

// ---------------------------------------------------------------------------
// Group F - local boolean quantifiers
// ---------------------------------------------------------------------------

#[test]
fn quant_every_array_true_and_false() {
    assert_english(
        "quant-every",
        concat!(
            "Define local good as array<i64, 3>.\n",
            "Set good[0] to 1.\nSet good[1] to 2.\nSet good[2] to 3.\n",
            "Define local bad as array<i64, 3>.\n",
            "Set bad[0] to 1.\nSet bad[1] to -2.\nSet bad[2] to 3.\n",
            "res is 0.\n",
            "If every x in good is more than 0, add 1 to res.\n",
            "If every x in bad is more than 0, add 10 to res.\n",
            "Return res.\n",
        ),
        1,
        b"",
    );
}

#[test]
fn quant_some_array_true_and_false() {
    assert_english(
        "quant-some",
        concat!(
            "Define local hit as array<i64, 3>.\n",
            "Set hit[0] to 1.\nSet hit[1] to 42.\nSet hit[2] to 3.\n",
            "Define local miss as array<i64, 3>.\n",
            "Set miss[0] to 1.\nSet miss[1] to 2.\nSet miss[2] to 3.\n",
            "res is 0.\n",
            "If some x in hit equals 42, add 1 to res.\n",
            "If some x in miss equals 42, add 10 to res.\n",
            "Return res.\n",
        ),
        1,
        b"",
    );
}

#[test]
fn quant_no_array_true_and_false() {
    assert_english(
        "quant-no",
        concat!(
            "Define local good as array<i64, 3>.\n",
            "Set good[0] to 1.\nSet good[1] to 0.\nSet good[2] to 3.\n",
            "Define local bad as array<i64, 3>.\n",
            "Set bad[0] to 1.\nSet bad[1] to -1.\nSet bad[2] to 3.\n",
            "res is 0.\n",
            "If no x in good is less than 0, add 1 to res.\n",
            "If no x in bad is less than 0, add 10 to res.\n",
            "Return res.\n",
        ),
        1,
        b"",
    );
}

#[test]
fn quant_range_primality_check() {
    assert_english(
        "quant-prime",
        concat!(
            "Define function is_prime taking n as i64 returning i64:\n",
            "    If n < 2, return 0.\n",
            "    If no d from 2 to n - 1 satisfies n % d == 0, return 1.\n",
            "    Return 0.\n",
            "Return is_prime(29) * 10 + is_prime(28).\n",
        ),
        10,
        b"",
    );
}

#[test]
fn quant_short_circuit_in_and_or() {
    assert_english(
        "quant-short",
        concat!(
            "Define global calls as i64.\n",
            "Define function bump taking v as i64 returning i64:\n",
            "    calls is calls + 1.\n    Return 1.\n",
            "Define local arr as array<i64, 3>.\n",
            "Set arr[0] to 5.\nSet arr[1] to 6.\nSet arr[2] to 7.\n",
            "If false and (some x in arr satisfies bump(1) == 1), return 9.\n",
            "Return calls.\n",
        ),
        0,
        b"",
    );
}

#[test]
fn quant_inside_while_re_evaluates() {
    assert_english(
        "quant-while",
        concat!(
            "Define local arr as array<i64, 3>.\n",
            "Set arr[0] to 1.\nSet arr[1] to 2.\nSet arr[2] to 3.\n",
            "Put 0 in sum.\n",
            "While some x in arr is less than 5:\n",
            "    Add 1 to arr[0].\n    Add 1 to arr[1].\n    Add 1 to arr[2].\n",
            "    Add 1 to sum.\n",
            "Return sum.\n",
        ),
        4,
        b"",
    );
}

#[test]
fn quant_combined_with_comma_precedence() {
    assert_english(
        "quant-comma",
        concat!(
            "Define local a as array<i64, 2>.\n",
            "Set a[0] to 1.\nSet a[1] to 2.\n",
            "Define local b as array<i64, 2>.\n",
            "Set b[0] to 9.\nSet b[1] to 0.\n",
            "flag is 1.\nres is 0.\n",
            "If every x in a is more than 0 or some y in b equals 9, and flag == 1, add 4 to res.\n",
            "Return res.\n",
        ),
        4,
        b"",
    );
}

#[test]
fn quant_bound_var_does_not_leak() {
    assert_error_contains(
        "quant-leak",
        concat!(
            "Define local arr as array<i64, 3>.\n",
            "Set arr[0] to 1.\n",
            "If every q in arr is more than 0, add 1 to ok.\n",
            "Return q.\n",
        ),
        "q",
    );
}

// ---------------------------------------------------------------------------
// Group G - specificity, paraphrase, and CLI
// ---------------------------------------------------------------------------

#[test]
fn ditransitive_transfer_beats_transitive() {
    assert_english(
        "ditransitive",
        "Put 100 in bank.\nPut 100 in wallet.\nTransfer 30 from bank to wallet.\nReturn bank * 1000 + wallet.\n",
        wrap(70_130),
        b"",
    );
}

#[test]
fn phrase_specificity_longer_literal_wins() {
    assert_english(
        "phrase-specific",
        concat!(
            "Define global log as i64.\n",
            "Define function emit_plain taking v as i64 returning i64:\n",
            "    Phrase: \"emit <v>\"\n",
            "    log is log + v.\n    Return 0.\n",
            "Define function emit_err taking v as i64 returning i64:\n",
            "    Phrase: \"emit <v> to stderr\"\n",
            "    log is log + v * 10.\n    Return 0.\n",
            "Emit 3 to stderr.\nReturn log.\n",
        ),
        30,
        b"",
    );
}

#[test]
fn phrase_exact_tie_fails_ambiguous() {
    assert_error_contains(
        "phrase-ambiguous",
        concat!(
            "Define function a1 taking v as i64 returning i64:\n",
            "    Phrase: \"ping <v>\"\n    Return v.\n",
            "Define function a2 taking v as i64 returning i64:\n",
            "    Phrase: \"ping <v>\"\n    Return v.\n",
            "Ping 3.\nReturn 0.\n",
        ),
        "ambiguous Phrase dispatch",
    );
}

#[test]
fn paraphrase_shows_anaphor_annotations() {
    let output = paraphrase(concat!(
        "Define struct Point with x as i64 and y as i64.\n",
        "Put 500 in cash.\nAdd 100 to it.\n",
        "Create a Point origin.\n",
        "Set its x to 7.\n",
        "Set the point's y to 8.\n",
        "Return cash.\n",
    ));
    assert!(output.contains("[it -> cash]"), "{output}");
    assert!(output.contains("[its x -> origin.x]"), "{output}");
    assert!(output.contains("[the point -> origin]"), "{output}");
}

#[test]
fn paraphrase_shows_comma_grouping() {
    let output = paraphrase(concat!(
        "a is 0.\nb is 0.\nc is 1.\nres is 0.\n",
        "If a == 0 or b == 0, and c == 1, add 1 to res.\n",
        "If a == 0 or b == 0 and c == 1, add 1 to res.\n",
        "Return res.\n",
    ));
    assert!(
        output.contains("{(a == 0 or b == 0) and c == 1}"),
        "{output}"
    );
    assert!(
        output.contains("{a == 0 or (b == 0 and c == 1)}"),
        "{output}"
    );
}

#[test]
fn paraphrase_shows_omitted_prose() {
    let output = paraphrase(
        "Yesterday Ravi called me twice.\nIt was raining outside.\nPut 1 in x.\nReturn x.\n",
    );
    assert!(output.contains("[OMITTED:"), "{output}");
    assert!(
        output.contains("Yesterday Ravi called me twice"),
        "{output}"
    );
}

#[test]
fn paraphrase_shows_quantifiers_and_phrases() {
    let output = paraphrase(concat!(
        "Define function emit_i64 taking v as i64 returning i64:\n",
        "    Phrase: \"emit <v>\"\n    Return v.\n",
        "Put 1 in x.\nFor every i from 1 to 3, add i to x.\n",
        "Emit x.\nReturn x.\n",
    ));
    assert!(
        output.contains("For every i from 1 to 3 (inclusive, step 1)"),
        "{output}"
    );
    assert!(output.contains("[phrase: \"emit <v>\"]"), "{output}");
}

#[test]
fn cli_trace_with_paraphrase_flag() {
    let binary = tc_binary();
    assert!(binary.is_file(), "run `make build` before this test");
    let path = unique_path("trace-paraphrase", "tc");
    std::fs::write(&path, "Put 5 in x.\nAdd 1 to it.\nReturn x.\n").expect("write source");
    let output = Command::new(binary)
        .arg("trace")
        .arg(&path)
        .arg("--paraphrase")
        .env_clear()
        .output()
        .expect("trace command");
    let _ = std::fs::remove_file(&path);
    assert!(output.status.success(), "{:?}", output.status);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--- TC-IR ---"), "{stdout}");
    assert!(stdout.contains("=== Paraphrase ==="), "{stdout}");
    assert!(stdout.contains("[it -> x]"), "{stdout}");
}

#[test]
fn phase3_6_elf_is_static_and_zero_libc() {
    let compiled = compile_sources(
        "static",
        &[SourceFile::new(
            "main.tc",
            "Put 1 in x.\nFor every i from 1 to 3, add i to x.\nReturn x.\n",
        )],
    );
    let path = unique_path("static", "elf");
    compile_to_elf(&compiled.module, &path).expect("ELF");
    let ldd = Command::new("ldd").arg(&path).output().expect("ldd");
    let ldd_text = format!(
        "{}{}",
        String::from_utf8_lossy(&ldd.stdout),
        String::from_utf8_lossy(&ldd.stderr)
    );
    assert!(ldd_text.contains("not a dynamic executable"), "{ldd_text}");
    let dynamic = Command::new("readelf")
        .arg("-d")
        .arg(&path)
        .output()
        .expect("readelf -d");
    let dynamic_text = String::from_utf8_lossy(&dynamic.stdout);
    assert!(
        dynamic_text.contains("There is no dynamic section"),
        "{dynamic_text}"
    );
    let program = Command::new("readelf")
        .arg("-l")
        .arg(&path)
        .output()
        .expect("readelf -l");
    assert!(
        !String::from_utf8_lossy(&program.stdout).contains("INTERP"),
        "unexpected INTERP segment"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_paraphrase_command_prints_module_header() {
    let binary = tc_binary();
    assert!(binary.is_file(), "run `make build` before this test");
    let path = unique_path("paraphrase-cli", "tc");
    std::fs::write(&path, "Put 5 in x.\nAdd 1 to it.\nReturn x.\n").expect("write source");
    let output = Command::new(binary)
        .arg("paraphrase")
        .arg(&path)
        .env_clear()
        .output()
        .expect("paraphrase command");
    let _ = std::fs::remove_file(&path);
    assert!(output.status.success(), "{:?}", output.status);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("=== Paraphrase (module:"), "{stdout}");
    assert!(stdout.contains("[it -> x]"), "{stdout}");
}

#[test]
fn format_paraphrase_entries_is_stable() {
    let sources = [SourceFile::new("main.tc", "Put 3 in x.\nReturn x.\n")];
    let project = lower_project(&sources, "main").expect("lower");
    let entries = project
        .paraphrase
        .first()
        .map(|doc| format_paraphrase_entries(&doc.entries))
        .unwrap_or_default();
    assert!(entries.contains("[L1] Set x to 3."), "{entries}");
    assert!(entries.contains("[L2] Return x."), "{entries}");
}

#[test]
fn quant_boolean_assignment() {
    assert_english(
        "quant-assign",
        concat!(
            "Define local nums as array<i64, 3>.\n",
            "Set nums[0] to 1.\nSet nums[1] to 2.\nSet nums[2] to 3.\n",
            "flag is every x in nums is more than 0.\n",
            "If flag, return 1.\nReturn 0.\n",
        ),
        1,
        b"",
    );
}

#[test]
fn for_each_range_and_global_anaphora() {
    assert_english(
        "for-each-global",
        concat!(
            "Define global cash as i64.\n",
            "Put 500 in cash.\n",
            "Add 100 to it.\n",
            "Put 0 in sum.\nFor each i from 1 to 3, add i to sum.\n",
            "Return sum + 255.\n",
        ),
        wrap(261),
        b"",
    );
}

// ---------------------------------------------------------------------------
// Group H - deterministic 150-seed differential parity fuzz
// ---------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(
            seed.wrapping_mul(2_862_933_555_777_941_757)
                .wrapping_add(3_037_000_493),
        )
    }

    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    fn range(&mut self, low: i64, high: i64) -> i64 {
        if high <= low {
            return low;
        }
        let span = u64::try_from(high - low + 1).unwrap_or(1);
        low + i64::try_from(self.next() % span).unwrap_or(0)
    }
}

fn template_one(rng: &mut Rng) -> (String, i64) {
    let initial = rng.range(0, 50);
    let added = rng.range(0, 50);
    let removed = rng.range(0, 20);
    let threshold = rng.range(0, 100);
    let repeats = rng.range(0, 4);
    let increment = rng.range(0, 10);
    let program = format!(
        "Put {initial} in x.\nIt was raining outside.\nAdd {added} to it.\n\
         If it is more than {threshold}, remove {removed} from it.\n\
         Repeat {repeats} times, add {increment} to it.\nIt is 3 pm.\nReturn it.\n"
    );
    let mut expected = initial + added;
    if expected > threshold {
        expected -= removed;
    }
    expected += repeats * increment;
    (program, expected)
}

fn template_two(rng: &mut Rng) -> (String, i64) {
    let a = rng.range(0, 1);
    let b = rng.range(0, 1);
    let c = rng.range(0, 1);
    let pattern = rng.range(0, 5);
    let (condition, expected) = match pattern {
        0 => (
            "a == 1 or b == 1 and c == 1",
            (a == 1) || ((b == 1) && (c == 1)),
        ),
        1 => (
            "a == 1 or b == 1, and c == 1",
            ((a == 1) || (b == 1)) && (c == 1),
        ),
        2 => (
            "a == 0 or b == 0, and c == 1",
            ((a == 0) || (b == 0)) && (c == 1),
        ),
        3 => ("it is false that a == 1 and b == 1", ((a != 1) && (b == 1))),
        4 => (
            "it is false that a == 0 and that b == 1",
            !((a == 0) && (b == 1)),
        ),
        _ => (
            "a == 1 and b == 1, or c == 0",
            ((a == 1) && (b == 1)) || (c == 0),
        ),
    };
    let program = format!(
        "a is {a}.\nb is {b}.\nc is {c}.\nres is 0.\n\
         If {condition}, add 1 to res.\nReturn res.\n"
    );
    (program, i64::from(expected))
}

fn template_three(rng: &mut Rng) -> (String, i64) {
    let x1 = rng.range(0, 20);
    let y1 = rng.range(0, 20);
    let x2 = rng.range(0, 20);
    let w = rng.range(0, 20);
    let program = format!(
        "Define struct Point with x as i64 and y as i64.\n\
         Define struct Rect with w as i64 and h as i64.\n\
         Create a Point p1.\nCreate a Point p2.\nCreate a Rect r1.\n\
         Set its x to {x1}.\nSet its w to {w}.\n\
         Set the point's y to {y1}.\nSet the point p1's x to {x2}.\n\
         Return p1.x * 10000 + p2.x * 1000 + p2.y * 100 + r1.w.\n"
    );
    let expected = x2 * 10_000 + x1 * 1_000 + y1 * 100 + w;
    (program, expected)
}

fn template_four(rng: &mut Rng) -> (String, i64) {
    let low = rng.range(0, 8);
    let high = low + rng.range(0, 8);
    let step = rng.range(1, 3);
    let skip_at = rng.range(low - 2, high + 2);
    let stop_at = rng.range(low - 2, high + 2);
    let program = format!(
        "Put 0 in sum.\nFor every i from {low} to {high} by {step}:\n    If i == {skip_at}, continue.\n    If i == {stop_at}, break.\n    Add i to sum.\nReturn sum.\n"
    );
    let mut sum = 0_i64;
    let mut index = low;
    while index <= high {
        if index == skip_at {
            index += step;
            continue;
        }
        if index == stop_at {
            break;
        }
        sum += index;
        index += step;
    }
    (program, sum)
}

fn template_five(rng: &mut Rng) -> (String, i64) {
    let values = (0..6).map(|_| rng.range(-10, 20)).collect::<Vec<_>>();
    let target = rng.range(-10, 20);
    let flag = rng.range(0, 1);
    let pattern = rng.range(0, 4);
    let (condition, expected) = match pattern {
        0 => (
            "every x in arr is more than 0, and flag == 1",
            values.iter().all(|value| *value > 0) && flag == 1,
        ),
        1 => (
            "some x in arr equals TARGET, or flag == 0",
            values.contains(&target) || flag == 0,
        ),
        2 => (
            "no x in arr is less than 0, and flag == 1",
            values.iter().all(|value| *value >= 0) && flag == 1,
        ),
        3 => (
            "every x in arr is more than 0 or some x in arr equals TARGET, and flag == 1",
            values.iter().all(|value| *value > 0) || (values.contains(&target) && flag == 1),
        ),
        _ => (
            "it is false that some x in arr is less than 0 and that flag == 0",
            !(values.iter().any(|value| *value < 0) && flag == 0),
        ),
    };
    let condition = condition.replace("TARGET", &target.to_string());
    let mut program = String::from("Define local arr as array<i64, 6>.\n");
    for (index, value) in values.iter().enumerate() {
        let _ = writeln!(program, "Set arr[{index}] to {value}.");
    }
    let _ = writeln!(program, "flag is {flag}.");
    let _ = writeln!(program, "res is 0.");
    let _ = writeln!(program, "If {condition}, add 1 to res.");
    let _ = writeln!(program, "Return res.");
    (program, i64::from(expected))
}

fn run_seed(seed: u64) -> i32 {
    let mut rng = Rng::new(seed);
    let (program, expected) = match seed % 5 {
        0 => template_one(&mut rng),
        1 => template_two(&mut rng),
        2 => template_three(&mut rng),
        3 => template_four(&mut rng),
        _ => template_five(&mut rng),
    };
    let sources = [SourceFile::new("main.tc", program.clone())];
    let compiled = compile_english_to_ir(&sources, "main")
        .unwrap_or_else(|errors| panic!("seed {seed}: compile failed for\n{program}\n{errors:#?}"));
    verify_module(&compiled.module)
        .unwrap_or_else(|errors| panic!("seed {seed}: IR verification failed: {errors:#?}"));
    let args = ["prog".to_string()];
    let interpreted = interpret(&compiled.module, &args, &[], 50_000_000)
        .unwrap_or_else(|error| panic!("seed {seed}: interpreter failed: {error:?}"));
    assert_eq!(
        i64::from(interpreted.exit_code),
        expected.rem_euclid(256),
        "seed {seed}: expected mismatch for\n{program}"
    );
    let path = unique_path(&format!("fuzz{seed}"), "elf");
    compile_to_elf(&compiled.module, &path)
        .unwrap_or_else(|error| panic!("seed {seed}: ELF failed: {error}"));
    let elf = run_elf(&path, &["prog"], &[]);
    let _ = std::fs::remove_file(&path);
    assert_eq!(elf.exit_code, interpreted.exit_code, "seed {seed}: exit");
    assert_eq!(elf.stdout, interpreted.stdout, "seed {seed}: stdout");
    assert_eq!(elf.stderr, interpreted.stderr, "seed {seed}: stderr");
    assert!(!format_paraphrase(&compiled.paraphrase).is_empty());
    interpreted.exit_code
}

#[test]
fn fuzz_150_ace_discourse_and_precedence_parity() {
    let mut executed = 0;
    for seed in 1..=150 {
        let exit = run_seed(seed);
        assert!((0..=255).contains(&exit));
        executed += 1;
    }
    assert_eq!(executed, 150);
}
