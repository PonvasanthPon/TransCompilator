//! Phase 4.6b Version 2 pure-prose English parity suite and the automated
//! eight-point readability gate for every `std/*.eng` module.
//!
//! Every executable test compiles the English source to Core-7 IR, runs the
//! reference interpreter, compiles the same module to a static x86_64 ELF64,
//! and asserts exact parity on exit status, stdout, stderr, and created files.

use std::collections::{HashMap, VecDeque};
use std::io::Write as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use tc_editor::runner::PRESETS;
use tc_ir::{interpret_in_dir, verify_module};
use tc_semantics::{RawItem, SourceFile, compile_english_to_ir, segment_source, tokenize};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn std_dir() -> PathBuf {
    if let Some(value) = std::env::var_os("TC_STD_DIR") {
        return PathBuf::from(value);
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../std");
    manifest.canonicalize().unwrap_or(manifest)
}

fn imports_of(source: &SourceFile) -> Vec<String> {
    let document = segment_source(source);
    let mut imports = Vec::new();
    for item in document.items {
        let RawItem::Sentence(sentence) = item else {
            continue;
        };
        let tokens = tokenize(&sentence.text);
        let Some(first) = tokens.first() else {
            continue;
        };
        if !first.is("import") && !first.is("use") {
            continue;
        }
        for token in tokens.iter().skip(1) {
            if token.is("as") {
                break;
            }
            if token.is_word() && !token.is("and") && !token.is("module") {
                imports.push(token.lower.clone());
            }
        }
    }
    imports
}

fn build_sources(source: &str) -> Result<(Vec<SourceFile>, String), String> {
    let entry = SourceFile::new("main.tc", source.to_string());
    let directory = std_dir();
    let mut queue = VecDeque::from([entry.clone()]);
    let mut seen = HashMap::from([("main".to_string(), true)]);
    let mut sources = Vec::new();
    while let Some(current) = queue.pop_front() {
        for import in imports_of(&current) {
            if seen.contains_key(&import) {
                continue;
            }
            let path = directory.join(format!("{import}.eng"));
            if !path.is_file() {
                return Err(format!(
                    "std module `{import}` not found at {}",
                    path.display()
                ));
            }
            let text = std::fs::read_to_string(&path)
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
            let module = SourceFile::new(format!("{import}.tc"), text);
            seen.insert(import, true);
            queue.push_back(module);
        }
        sources.push(current);
    }
    Ok((sources, "main".to_string()))
}

fn work_dir(tag: &str) -> PathBuf {
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "tc_r1_phase4_6b_{}_{}_{tag}",
        std::process::id(),
        counter
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(path.join("interp")).expect("interp dir");
    std::fs::create_dir_all(path.join("elf")).expect("elf dir");
    path
}

fn directory_files(directory: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files = std::fs::read_dir(directory)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.path().is_file())
                .map(|entry| {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let bytes = std::fs::read(entry.path()).unwrap_or_default();
                    (name, bytes)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
}

#[derive(Debug)]
struct ProcessOutput {
    exit_code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn run_elf(path: &Path, directory: &Path, stdin: &[u8]) -> ProcessOutput {
    let mut child = Command::new(path)
        .arg0("program")
        .env_clear()
        .current_dir(directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ELF");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin)
        .expect("write stdin");
    let output = child.wait_with_output().expect("wait ELF");
    ProcessOutput {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: output.stdout,
        stderr: output.stderr,
    }
}

struct CompiledRun {
    native: ProcessOutput,
    interpreted: ProcessOutput,
    elf_path: PathBuf,
    work_dir: PathBuf,
    interpreter_files: Vec<(String, Vec<u8>)>,
    elf_files: Vec<(String, Vec<u8>)>,
}

impl Drop for CompiledRun {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.work_dir);
    }
}

fn compile_and_run(name: &str, source: &str, stdin: &[u8]) -> CompiledRun {
    let (sources, entry) = build_sources(source)
        .unwrap_or_else(|error| panic!("{name}: source loading failed: {error}"));
    let compiled = compile_english_to_ir(&sources, &entry)
        .unwrap_or_else(|errors| panic!("{name}: compilation failed: {errors:#?}"));
    verify_module(&compiled.module)
        .unwrap_or_else(|errors| panic!("{name}: IR verify failed: {errors:#?}"));
    let work = work_dir(name);
    let interp_dir = work.join("interp");
    let elf_dir = work.join("elf");
    let interpreted = interpret_in_dir(
        &compiled.module,
        &["program".to_string()],
        stdin,
        &interp_dir,
        50_000_000,
    )
    .map_or_else(
        |error| panic!("{name}: interpreter failed: {error:?}"),
        |output| ProcessOutput {
            exit_code: output.exit_code,
            stdout: output.stdout,
            stderr: output.stderr,
        },
    );
    let elf_path = work.join("program.elf");
    tc_codegen_x86::compile_to_elf(&compiled.module, &elf_path)
        .unwrap_or_else(|error| panic!("{name}: ELF failed: {error}"));
    let native = run_elf(&elf_path, &elf_dir, stdin);
    let interpreter_files = directory_files(&interp_dir);
    let elf_files = directory_files(&elf_dir);
    CompiledRun {
        native,
        interpreted,
        elf_path,
        work_dir: work,
        interpreter_files,
        elf_files,
    }
}

fn assert_parity(name: &str, source: &str) -> CompiledRun {
    let run = compile_and_run(name, source, b"");
    assert_eq!(
        run.native.exit_code, run.interpreted.exit_code,
        "{name}: exit mismatch"
    );
    assert_eq!(
        run.native.stdout,
        run.interpreted.stdout,
        "{name}: stdout mismatch\nnative:\n{}\ninterpreter:\n{}",
        String::from_utf8_lossy(&run.native.stdout),
        String::from_utf8_lossy(&run.interpreted.stdout)
    );
    assert_eq!(
        run.native.stderr, run.interpreted.stderr,
        "{name}: stderr mismatch"
    );
    assert_eq!(
        run.interpreter_files, run.elf_files,
        "{name}: created file mismatch"
    );
    run
}

fn assert_program(
    name: &str,
    source: &str,
    expected_exit: i32,
    expected_stdout: &[u8],
) -> CompiledRun {
    let run = assert_parity(name, source);
    assert_eq!(run.native.exit_code, expected_exit, "{name}: exit code");
    assert_eq!(
        run.native.stdout,
        expected_stdout,
        "{name}: stdout\n{}",
        String::from_utf8_lossy(&run.native.stdout)
    );
    assert!(run.native.stderr.is_empty(), "{name}: unexpected stderr");
    run
}

fn assert_compile_error(name: &str, source: &str, fragment: &str) {
    let (sources, entry) = build_sources(source)
        .unwrap_or_else(|error| panic!("{name}: source loading failed: {error}"));
    let errors = compile_english_to_ir(&sources, &entry)
        .expect_err("malformed Version 2 prose must produce a structured error");
    assert!(
        errors
            .iter()
            .any(|error| error.message.to_ascii_lowercase().contains(fragment)),
        "{name}: expected `{fragment}` in {errors:#?}"
    );
}

// ---------------------------------------------------------------------------
// Paragraph and one-sentence functions
// ---------------------------------------------------------------------------

#[test]
fn one_sentence_function_returns_smaller() {
    assert_program(
        "v2-one-sentence",
        concat!(
            "To find the smaller of unsigned numbers a and b, named smaller, returning an unsigned number, ",
            "give back a if a is less than b, otherwise give back b.\n",
            "Return smaller(3, 5) as a whole number.\n",
        ),
        3,
        b"",
    );
}

#[test]
fn paragraph_function_ends_at_blank_line() {
    assert_program(
        "v2-paragraph",
        concat!(
            "To twice given value as a whole number, named twice, returning a whole number.\n",
            "Give back value plus value.\n\n",
            "To triple given value as a whole number, named triple, returning a whole number.\n",
            "Give back value times 3.\n\n",
            "Return twice(21) plus triple(2).\n",
        ),
        48,
        b"",
    );
}

#[test]
fn generic_for_t_paragraph_function() {
    assert_program(
        "v2-generic",
        concat!(
            "To pick_first for T given a as T, b as T, named pick_first, returning T.\n",
            "Give back a.\n\n",
            "Return pick_first(7, 9).\n",
        ),
        7,
        b"",
    );
}

#[test]
fn three_paragraph_functions_call_each_other() {
    assert_program(
        "v2-multifunction",
        concat!(
            "To double given n as a whole number, named double, returning a whole number.\n",
            "Give back n plus n.\n\n",
            "To quadruple given n as a whole number, named quadruple, returning a whole number.\n",
            "Give back double(double(n)).\n\n",
            "To eightfold given n as a whole number, named eightfold, returning a whole number.\n",
            "Give back quadruple(double(n)).\n\n",
            "Return eightfold(5).\n",
        ),
        40,
        b"",
    );
}

#[test]
fn explicit_done_still_terminates_a_function() {
    assert_program(
        "v2-done",
        concat!(
            "To bump given value as a whole number, named bump, returning a whole number.\n",
            "Give back value plus 1.\n",
            "Done.\n",
            "Return bump(41).\n",
        ),
        42,
        b"",
    );
}

#[test]
fn inline_function_body_and_multi_action_chain() {
    assert_program(
        "v2-inline-chain",
        concat!(
            "To round an unsigned number n up to 16 bytes, named round16, returning an unsigned number.\n",
            "Call this as \"round <n> up to 16 bytes\".\n",
            "Add 15 to n, clear bits 15 from n, and give back n.\n\n",
            "Let need be round 7 up to 16 bytes.\n",
            "Return need as a whole number.\n",
        ),
        16,
        b"",
    );
}

// ---------------------------------------------------------------------------
// Trailing if, inline naming, and multi-action chains
// ---------------------------------------------------------------------------

#[test]
fn trailing_if_returns_with_otherwise() {
    assert_program(
        "v2-trailing-if",
        concat!(
            "To smaller given a as a whole number, b as a whole number, named smaller, returning a whole number.\n",
            "Give back a if a is less than b, otherwise give back b.\n\n",
            "Return smaller(9, 4).\n",
        ),
        4,
        b"",
    );
}

#[test]
fn trailing_if_void_early_return() {
    assert_program(
        "v2-early-return",
        concat!(
            "Keep a shared touched as a whole number.\n",
            "To touch given p as a pointer to byte, named touch, returning nothing.\n",
            "Stop and return if p is empty.\n",
            "Set touched to touched plus 1.\n",
            "Give back.\n\n",
            "Keep a shared target as a byte.\n",
            "touch(no address).\n",
            "touch(address of target).\n",
            "Return touched as a whole number.\n",
        ),
        1,
        b"",
    );
}

#[test]
fn trailing_if_assignment_with_put() {
    assert_program(
        "v2-trailing-put",
        concat!(
            "To choose given flag as a whole number, named choose, returning a whole number.\n",
            "Let x be 0.\n",
            "Put 10 in x if flag equals 1, otherwise put 20 in x.\n",
            "Give back x.\n\n",
            "Return choose(1) plus choose(0).\n",
        ),
        30,
        b"",
    );
}

#[test]
fn inline_naming_and_make_it() {
    assert_program(
        "v2-inline-naming",
        concat!(
            "To round an unsigned number n up to 16 bytes, named round16, returning an unsigned number.\n",
            "Call this as \"round <n> up to 16 bytes\".\n",
            "Add 15 to n, clear bits 15 from n, and give back n.\n\n",
            "Round 0 up to 16 bytes and call it need.\n",
            "If need is less than 16, make it 16 as an unsigned number.\n",
            "Return need as a whole number.\n",
        ),
        16,
        b"",
    );
}

#[test]
fn start_at_expression_and_call_it() {
    assert_program(
        "v2-start-at",
        concat!(
            "A record Node has a value as a whole number.\n",
            "Keep a shared node as Node.\n",
            "To read_node given p as a pointer to Node, named read_node, returning a whole number.\n",
            "Start at p and call it spot.\n",
            "Give back spot's value.\n\n",
            "Set node's value to 33.\n",
            "Return read_node(address of node).\n",
        ),
        33,
        b"",
    );
}

#[test]
fn make_variable_with_it_anaphor() {
    assert_program(
        "v2-make-it",
        concat!(
            "To reset given value as a whole number, named reset, returning a whole number.\n",
            "Let n be value.\n",
            "Put 5 in n.\n",
            "Make it 9.\n",
            "Give back n.\n\n",
            "Return reset(1).\n",
        ),
        9,
        b"",
    );
}

#[test]
fn multi_action_chain_splits_and_gives_back() {
    assert_program(
        "v2-chain",
        concat!(
            "To round_up given n as an unsigned number, named round_up, returning an unsigned number.\n",
            "Add 15 to n, clear bits 15 from n, and give back n.\n\n",
            "Return round_up(17) as a whole number.\n",
        ),
        32,
        b"",
    );
}

// ---------------------------------------------------------------------------
// Natural pointer, field, and loop English
// ---------------------------------------------------------------------------

#[test]
fn pointer_offset_after_and_before_roundtrip() {
    assert_program(
        "v2-offsets",
        concat!(
            "A record Header has a size as an unsigned number, a flag as an unsigned number, ",
            "a next as a pointer to Header, and a pad as an unsigned number.\n",
            "To payload given block as a pointer to Header, named payload, returning a pointer to byte.\n",
            "Call this as \"payload of <block>\".\n",
            "Give back 32 bytes after block as a pointer to byte.\n\n",
            "To header given p as a pointer to byte, named header_of, returning a pointer to Header.\n",
            "Call this as \"header of <p>\".\n",
            "Give back 32 bytes before p as a pointer to Header.\n\n",
            "Keep a shared h as Header.\n",
            "Set h's size to 99.\n",
            "Set h's flag to 7.\n",
            "Let p be payload(address of h).\n",
            "Let recovered be header_of(p).\n",
            "Give back recovered's size as a whole number.\n",
        ),
        99,
        b"",
    );
}

#[test]
fn pointer_offset_sum_expression() {
    assert_program(
        "v2-offset-sum",
        concat!(
            "A record Header has a size as an unsigned number, a flag as an unsigned number, ",
            "a next as a pointer to Header, and a pad as an unsigned number.\n",
            "Keep a shared h as Header.\n",
            "Set h's flag to 7.\n",
            "Let base be address of h.\n",
            "Let payload be 32 plus 8 bytes after base as a pointer to byte.\n",
            "Let recovered be 32 bytes before payload as a pointer to Header.\n",
            "Give back recovered's size as a whole number.\n",
        ),
        7,
        b"",
    );
}

#[test]
fn null_checks_in_all_four_natural_forms() {
    assert_program(
        "v2-nulls",
        concat!(
            "Keep a shared p as a pointer to byte.\n",
            "Keep a shared q as a pointer to byte.\n",
            "Keep a shared b as a byte.\n",
            "Set p to no address.\n",
            "Set q to address of b.\n",
            "Let a be 0.\n",
            "If p is empty, add 1 to a.\n",
            "If q is not empty, add 2 to a.\n",
            "If p has no address, add 4 to a.\n",
            "If q has an address, add 8 to a.\n",
            "Return a.\n",
        ),
        15,
        b"",
    );
}

#[test]
fn move_traverses_three_linked_nodes() {
    assert_program(
        "v2-move",
        concat!(
            "A record Node has a value as a whole number and a next as a pointer to Node.\n",
            "Keep a shared first as Node.\n",
            "Keep a shared second as Node.\n",
            "Keep a shared third as Node.\n",
            "To walk given start as a pointer to Node, named walk, returning a whole number.\n",
            "Let current be start.\n",
            "Move current to current's next.\n",
            "Move current to current's next.\n",
            "Give back current's value.\n\n",
            "Set first's value to 4.\n",
            "Set second's value to 9.\n",
            "Set third's value to 16.\n",
            "Set first's next to address of second.\n",
            "Set second's next to address of third.\n",
            "Set third's next to no address.\n",
            "Return walk(address of first).\n",
        ),
        16,
        b"",
    );
}

#[test]
fn possessive_fields_on_value_structs() {
    assert_program(
        "v2-possessive-value",
        concat!(
            "A record Block has a size as a whole number and a flag as a whole number.\n",
            "To inspect named inspect, returning a whole number.\n",
            "Let block_value be Block.\n",
            "Set block_value's size to 10.\n",
            "Add 5 to its size.\n",
            "Set the block_value's flag to 3.\n",
            "Give back block_value's size plus block_value's flag.\n\n",
            "Return inspect().\n",
        ),
        18,
        b"",
    );
}

#[test]
fn possessive_fields_on_pointer_structs() {
    assert_program(
        "v2-possessive-pointer",
        concat!(
            "A record Node has a size as a whole number and a next as a pointer to Node.\n",
            "Keep a shared n as Node.\n",
            "To grow given p as a pointer to Node, named grow, returning a whole number.\n",
            "Set p's size to 42.\n",
            "Add 8 to its size.\n",
            "Give back p's size.\n\n",
            "Set n's size to 1.\n",
            "Return grow(address of n).\n",
        ),
        50,
        b"",
    );
}

#[test]
fn until_prose_loop() {
    assert_program(
        "v2-until",
        concat!(
            "To count_up given limit as a whole number, named count_up, returning a whole number.\n",
            "Let i be 0.\n",
            "Until i is at least limit, add 1 to i.\n",
            "Give back i.\n\n",
            "Return count_up(4).\n",
        ),
        4,
        b"",
    );
}

#[test]
fn until_supports_multiple_actions() {
    assert_program(
        "v2-until-two-actions",
        concat!(
            "To count_by_two given limit as a whole number, named count_by_two, returning a whole number.\n",
            "Let i be 0.\n",
            "Let steps be 0.\n",
            "Until i is at least limit, add 2 to i, add 1 to steps.\n",
            "Give back steps.\n\n",
            "Return count_by_two(6).\n",
        ),
        3,
        b"",
    );
}

#[test]
fn flat_while_paragraph_body() {
    assert_program(
        "v2-flat-while",
        concat!(
            "To sum_to given limit as a whole number, named sum_to, returning a whole number.\n",
            "Let total be 0.\n",
            "Let i be 1.\n",
            "While i is at most limit.\n",
            "Add i to total.\n",
            "Add 1 to i.\n",
            "End the loop.\n",
            "Give back total.\n\n",
            "Return sum_to(5).\n",
        ),
        15,
        b"",
    );
}

#[test]
fn flat_if_paragraph_body() {
    assert_program(
        "v2-flat-if",
        concat!(
            "To bucket given n as a whole number, named bucket, returning a whole number.\n",
            "If n is more than 100.\n",
            "Give back 2.\n",
            "End the condition.\n",
            "Give back 1.\n\n",
            "Return bucket(200) plus bucket(1).\n",
        ),
        3,
        b"",
    );
}

#[test]
fn apply_function_pointer_with_with() {
    assert_program(
        "v2-apply",
        concat!(
            "To subtract given a as a whole number, b as a whole number, named subtract, returning a whole number.\n",
            "Give back a minus b.\n\n",
            "To use_it given compare as a function taking whole number and whole number returning whole number, ",
            "named use_it, returning a whole number.\n",
            "Give back apply compare to 9 with 4.\n\n",
            "Return use_it(function subtract).\n",
        ),
        5,
        b"",
    );
}

// ---------------------------------------------------------------------------
// End-to-end Version 2 std/ showcase
// ---------------------------------------------------------------------------

const FLOWING_USER_PROGRAM: &str = concat!(
    "Use io and math.\n\n",
    "Width is 3.0.\n",
    "Height is 4.0.\n",
    "Let diagonal be square root of width times width plus height times height.\n",
    "Say diagonal.\n",
    "Put 500 in cash.\n",
    "Add 150 to it.\n",
    "Remove 75 from it.\n",
    "Say it.\n",
    "Give back 0.\n",
);

#[test]
fn flowing_user_program_imports_io_and_math() {
    assert_program("v2-user-io-math", FLOWING_USER_PROGRAM, 0, b"5.0000\n575\n");
}

#[test]
fn flowing_user_program_builds_static_zero_libc_elf() {
    let run = assert_program("v2-user-static", FLOWING_USER_PROGRAM, 0, b"5.0000\n575\n");
    let ldd = Command::new("ldd")
        .arg(&run.elf_path)
        .output()
        .expect("ldd available");
    let ldd_text = format!(
        "{}{}",
        String::from_utf8_lossy(&ldd.stdout),
        String::from_utf8_lossy(&ldd.stderr)
    );
    assert!(
        ldd_text.contains("not a dynamic executable"),
        "ldd: {ldd_text}"
    );
    let dynamic = Command::new("readelf")
        .arg("-d")
        .arg(&run.elf_path)
        .output()
        .expect("readelf available");
    assert!(
        String::from_utf8_lossy(&dynamic.stdout).contains("There is no dynamic section"),
        "readelf -d: {}",
        String::from_utf8_lossy(&dynamic.stdout)
    );
    let segments = Command::new("readelf")
        .arg("-l")
        .arg(&run.elf_path)
        .output()
        .expect("readelf segments");
    let segment_text = String::from_utf8_lossy(&segments.stdout);
    assert!(
        !segment_text.lines().any(|line| line.contains("INTERP")),
        "program interpreter segment present: {segment_text}"
    );
}

#[test]
fn preset_01_discourse_cash_matches_reference_interpreter() {
    assert_parity("v2-preset-01", PRESETS[0].source);
}

#[test]
fn preset_02_struct_anaphora_matches_reference_interpreter() {
    assert_parity("v2-preset-02", PRESETS[1].source);
}

#[test]
fn preset_03_quantifiers_prime_matches_reference_interpreter() {
    assert_parity("v2-preset-03", PRESETS[2].source);
}

#[test]
fn preset_04_phrase_syscall_hello_matches_reference_interpreter() {
    assert_parity("v2-preset-04", PRESETS[3].source);
}

#[test]
fn preset_05_passive_and_transfers_matches_reference_interpreter() {
    assert_parity("v2-preset-05", PRESETS[4].source);
}

#[test]
fn preset_06_std_io_and_math_matches_reference_interpreter() {
    assert_parity("v2-preset-06", PRESETS[5].source);
}

#[test]
fn preset_07_std_generic_list_matches_reference_interpreter() {
    assert_parity("v2-preset-07", PRESETS[6].source);
}

#[test]
fn preset_08_std_file_and_map_matches_reference_interpreter() {
    assert_parity("v2-preset-08", PRESETS[7].source);
}

// ---------------------------------------------------------------------------
// Eight-point Version 2 readability gate
// ---------------------------------------------------------------------------

fn strip_string_literals(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut in_string = false;
    let mut escaped = false;
    for character in text.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            output.push(if character == '\n' { '\n' } else { ' ' });
        } else if character == '"' {
            in_string = true;
            output.push(' ');
        } else {
            output.push(character);
        }
    }
    output
}

fn contains_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(index, _)| {
        let before = text[..index].chars().next_back();
        let after = text[index + word.len()..].chars().next();
        !before.is_some_and(|value| value.is_ascii_alphanumeric() || value == '_')
            && !after.is_some_and(|value| value.is_ascii_alphanumeric() || value == '_')
    })
}

fn contains_dot_field(text: &str) -> bool {
    let bytes = text.as_bytes();
    for index in 1..bytes.len().saturating_sub(1) {
        if bytes[index] != b'.' {
            continue;
        }
        let before = bytes[index - 1];
        let after = bytes[index + 1];
        if (before.is_ascii_alphabetic() || before == b'_')
            && (after.is_ascii_alphanumeric() || after == b'_')
        {
            return true;
        }
    }
    false
}

fn contains_call_parenthesis(text: &str) -> bool {
    let bytes = text.as_bytes();
    for index in 1..bytes.len() {
        if bytes[index] == b'('
            && (bytes[index - 1].is_ascii_alphanumeric() || bytes[index - 1] == b'_')
        {
            return true;
        }
    }
    false
}

#[test]
fn std_files_are_pure_version2_english() {
    let directory = std_dir();
    let modules = [
        "core.eng",
        "alloc.eng",
        "io.eng",
        "string.eng",
        "list.eng",
        "fs.eng",
        "math.eng",
        "map.eng",
    ];
    let mut checked = 0;
    for module in modules {
        let path = directory.join(module);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
        let code = strip_string_literals(&source);
        assert!(
            !source.to_ascii_lowercase().contains("define local"),
            "{module}: contains `Define local`"
        );
        for operator in ["==", "!=", "<=", ">=", "<", ">"] {
            assert!(
                !code.contains(operator),
                "{module}: contains symbolic comparator `{operator}`"
            );
        }
        for line in code.lines() {
            assert!(
                !line.trim_end().ends_with(':'),
                "{module}: colon-style header `{}`",
                line.trim()
            );
            assert!(
                !line.starts_with("    ") && !line.starts_with('\t'),
                "{module}: indented code block `{}`",
                line.trim_end()
            );
        }
        for token in [
            "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64",
        ] {
            assert!(
                !contains_word(&code, token),
                "{module}: shorthand C type `{token}`"
            );
        }
        assert!(!code.contains("ptr<"), "{module}: contains `ptr<`");
        for token in ["bitand", "bitnot", "bitor", "bitxor", "shl", "shr"] {
            assert!(
                !contains_word(&code, token),
                "{module}: C bitwise token `{token}`"
            );
        }
        assert!(
            !code.contains("syscall("),
            "{module}: raw syscall call expression"
        );
        assert!(!contains_dot_field(&code), "{module}: dot field notation");
        assert!(
            !contains_call_parenthesis(&code),
            "{module}: C-style helper call parentheses"
        );
        checked += 1;
    }
    assert_eq!(checked, 8);
}

// ---------------------------------------------------------------------------
// Negative diagnostics: malformed prose must be a clean `Err`, never a panic
// ---------------------------------------------------------------------------

#[test]
fn negative_until_without_condition_is_a_clean_error() {
    assert_compile_error(
        "v2-neg-until",
        "Let i be 0.\nUntil , add 1 to i.\nReturn 0.\n",
        "empty condition",
    );
}

#[test]
fn negative_trailing_if_without_condition_is_a_clean_error() {
    assert_compile_error(
        "v2-neg-if",
        concat!(
            "To broken given n as a whole number, named broken, returning a whole number.\n",
            "Give back 5 if .\n\n",
            "Return broken(1).\n",
        ),
        "unexpected trailing token `if`",
    );
}

#[test]
fn negative_dangling_it_is_a_clean_error() {
    assert_compile_error(
        "v2-neg-it",
        "Add 10 to it.\nReturn 0.\n",
        "unresolved_anaphor",
    );
}
