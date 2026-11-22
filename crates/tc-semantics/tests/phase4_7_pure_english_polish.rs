//! Phase 4.7 pure-English polish suite: collision-safe auto-phrases,
//! parenthetical types, multi-word identifiers, and grouped `Update` sentences.
//!
//! Every executable test asserts exact `ELF64 == Reference IR Interpreter`
//! parity on exit status, stdout, stderr, and created files.

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
        "tc_r1_phase4_7_{}_{}_{tag}",
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
        run.native.stdout, run.interpreted.stdout,
        "{name}: stdout mismatch"
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
        .expect_err("malformed polished English must produce a structured error");
    assert!(
        errors
            .iter()
            .any(|error| error.message.to_ascii_lowercase().contains(fragment)),
        "{name}: expected `{fragment}` in {errors:#?}"
    );
}

// ---------------------------------------------------------------------------
// 1. Collision-safe automatic phrase registration
// ---------------------------------------------------------------------------

#[test]
fn auto_phrase_rounds_from_parenthetical_header() {
    assert_program(
        "v47-auto-round",
        concat!(
            "To round n (an unsigned number) up to 16 bytes, giving an unsigned number.\n",
            "Add 15 to n, clear bits 15 from n, and give back n.\n\n",
            "Let need be round 7 up to 16 bytes.\n",
            "Give back need as a whole number.\n",
        ),
        16,
        b"",
    );
}

#[test]
fn auto_phrase_smaller_alias_from_find_header() {
    assert_program(
        "v47-auto-smaller",
        concat!(
            "To find the smaller of a and b (unsigned numbers), giving an unsigned number.\n",
            "Give back a if a is less than b, otherwise give back b.\n\n",
            "Let result be smaller of 9 and 4.\n",
            "Give back result as a whole number.\n",
        ),
        4,
        b"",
    );
}

#[test]
fn void_function_omits_return_clause() {
    assert_program(
        "v47-void",
        concat!(
            "Keep a shared count as a whole number.\n",
            "To bump n (a whole number).\n",
            "Add n to count.\n\n",
            "bump 5.\n",
            "Give back count.\n",
        ),
        5,
        b"",
    );
}

#[test]
fn auto_phrase_supports_zero_parameter_headers() {
    assert_program(
        "v47-zero-param",
        concat!(
            "To print a newline, named newline.\n",
            "Give back.\n\n",
            "To count bytes used, named count_bytes, giving an unsigned number.\n",
            "Give back 7.\n\n",
            "Print a newline.\n",
            "Let counted be count bytes used.\n",
            "Give back counted as a whole number.\n",
        ),
        7,
        b"",
    );
}

#[test]
fn auto_phrase_does_not_hijack_set() {
    assert_program(
        "v47-collision-set",
        concat!(
            "To set value to n (a whole number).\n",
            "Give back.\n\n",
            "Let value be 1.\n",
            "Set value to 2.\n",
            "Give back value.\n",
        ),
        2,
        b"",
    );
}

#[test]
fn auto_phrase_does_not_hijack_add_or_remove() {
    assert_program(
        "v47-collision-add",
        concat!(
            "To add amount (a whole number) to total (a whole number).\n",
            "Give back.\n\n",
            "To remove amount (a whole number) from total (a whole number).\n",
            "Give back.\n\n",
            "Keep a shared total as a whole number.\n",
            "Add 10 to total.\n",
            "Remove 2 from total.\n",
            "Give back total.\n",
        ),
        8,
        b"",
    );
}

#[test]
fn auto_phrase_does_not_hijack_if_or_while() {
    assert_program(
        "v47-collision-control",
        concat!(
            "To if ready (a whole number), giving a whole number.\n",
            "Give back 1.\n\n",
            "Let i be 0.\n",
            "While i is less than 3, add 1 to i.\n",
            "If i equals 3, add 10 to i.\n",
            "Give back i.\n",
        ),
        13,
        b"",
    );
}

// ---------------------------------------------------------------------------
// 2. Parenthetical record fields, parameters, and `giving`
// ---------------------------------------------------------------------------

#[test]
fn parenthetical_record_fields() {
    assert_program(
        "v47-record",
        concat!(
            "A record Point has x (a whole number) and y (a whole number).\n",
            "Let point be Point.\n",
            "Set point's x to 3.\n",
            "Set point's y to 4.\n",
            "Give back point's x plus point's y.\n",
        ),
        7,
        b"",
    );
}

#[test]
fn parenthetical_plural_record_fields() {
    assert_program(
        "v47-record-plural",
        concat!(
            "A record Pair has a and b (whole numbers).\n",
            "Let pair be Pair.\n",
            "Set pair's a to 2.\n",
            "Set pair's b to 5.\n",
            "Give back pair's a plus pair's b.\n",
        ),
        7,
        b"",
    );
}

#[test]
fn parenthetical_plural_function_parameters() {
    assert_program(
        "v47-plural-params",
        concat!(
            "To add_pair a and b (whole numbers), giving a whole number.\n",
            "Give back a plus b.\n\n",
            "Return add_pair(4, 6).\n",
        ),
        10,
        b"",
    );
}

#[test]
fn giving_is_a_return_synonym() {
    assert_program(
        "v47-giving",
        concat!(
            "To make_seven, giving a whole number.\n",
            "Give back 7.\n\n",
            "Return make_seven().\n",
        ),
        7,
        b"",
    );
}

#[test]
fn auto_derived_function_name_calls_cleanly() {
    assert_program(
        "v47-derived-name",
        concat!(
            "To double value (a whole number), giving a whole number.\n",
            "Give back value plus value.\n\n",
            "Return double(21).\n",
        ),
        42,
        b"",
    );
}

// ---------------------------------------------------------------------------
// 3. Multi-word English identifiers
// ---------------------------------------------------------------------------

#[test]
fn multi_word_shared_global() {
    assert_program(
        "v47-global",
        concat!(
            "Keep a shared bytes in use as an unsigned number.\n",
            "Add 5 to bytes in use.\n",
            "Add 2 to bytes in use.\n",
            "Give back bytes in use as a whole number.\n",
        ),
        7,
        b"",
    );
}

#[test]
fn multi_word_record_field() {
    assert_program(
        "v47-field",
        concat!(
            "A record Block has a size (an unsigned number), a free flag (an unsigned number), ",
            "and a next link (a pointer to Block).\n",
            "Let block be Block.\n",
            "Set block's free flag to 1.\n",
            "Set block's next link to no address.\n",
            "Give back block's free flag as a whole number.\n",
        ),
        1,
        b"",
    );
}

#[test]
fn multi_word_local_variable() {
    assert_program(
        "v47-local",
        concat!(
            "To probe named probe, returning a whole number.\n",
            "Let remaining space be 9 as a whole number.\n",
            "Add 1 to remaining space.\n",
            "Give back remaining space.\n\n",
            "Return probe().\n",
        ),
        10,
        b"",
    );
}

#[test]
fn multi_word_function_parameter() {
    assert_program(
        "v47-param",
        concat!(
            "To combine remaining space (a whole number) with total (a whole number), ",
            "named combine_values, giving a whole number.\n",
            "Give back remaining space plus total.\n\n",
            "Return combine_values(3, 4).\n",
        ),
        7,
        b"",
    );
}

#[test]
fn multi_word_possessives_and_its() {
    assert_program(
        "v47-possessive",
        concat!(
            "A record MemoryBlock has a free flag (an unsigned number), ",
            "a next link (a pointer to MemoryBlock), and padding (an unsigned number).\n",
            "To inspect block (a pointer to MemoryBlock), giving a whole number.\n",
            "Set block's free flag to 3.\n",
            "Add 1 to its free flag.\n",
            "Give back block's free flag as a whole number.\n\n",
            "Keep a shared node as MemoryBlock.\n",
            "Return inspect(address of node).\n",
        ),
        4,
        b"",
    );
}

#[test]
fn undeclared_words_are_never_joined() {
    assert_program(
        "v47-prose-safety",
        concat!(
            "It was raining outside.\n",
            "Yesterday Ravi called me twice.\n",
            "Put 3 in value.\n",
            "Return value.\n",
        ),
        3,
        b"",
    );
}

// ---------------------------------------------------------------------------
// 4. Grouped `Update` sentences and `becomes`
// ---------------------------------------------------------------------------

#[test]
fn update_value_struct_three_fields() {
    assert_program(
        "v47-update-value",
        concat!(
            "A record Block has a size (an unsigned number), a free flag (an unsigned number), ",
            "a next link (a pointer to Block), and padding (an unsigned number).\n",
            "To update_block named update_block, returning a whole number.\n",
            "Let rest be Block.\n",
            "Update rest so its size is 42, its free flag is 1, and its next link is no address.\n",
            "Give back (rest's size plus rest's free flag) as a whole number.\n\n",
            "Return update_block().\n",
        ),
        43,
        b"",
    );
}

#[test]
fn update_pointer_struct_two_fields() {
    assert_program(
        "v47-update-pointer",
        concat!(
            "A record Block has a size (an unsigned number) and a free flag (an unsigned number).\n",
            "To update_ptr given block as a pointer to Block, named update_ptr, returning a whole number.\n",
            "Update block so its size is 9 and its free flag is 1.\n",
            "Give back (block's size plus block's free flag) as a whole number.\n\n",
            "Keep a shared node as Block.\n",
            "Return update_ptr(address of node).\n",
        ),
        10,
        b"",
    );
}

#[test]
fn update_accepts_comma_and_and() {
    assert_program(
        "v47-update-and",
        concat!(
            "A record Block has a size (an unsigned number) and a free flag (an unsigned number).\n",
            "Let block be Block.\n",
            "Update block so its size is 3, and its free flag is 4.\n",
            "Give back (block's size plus block's free flag) as a whole number.\n",
        ),
        7,
        b"",
    );
}

#[test]
fn single_field_becomes_mutates_a_struct() {
    assert_program(
        "v47-becomes",
        concat!(
            "A record Block has a size (an unsigned number).\n",
            "Let block be Block.\n",
            "Set block's size to 1.\n",
            "block's size becomes 64.\n",
            "Give back block's size as a whole number.\n",
        ),
        64,
        b"",
    );
}

#[test]
fn its_field_becomes_mutates_the_active_struct() {
    assert_program(
        "v47-its-becomes",
        concat!(
            "A record Block has a size (an unsigned number).\n",
            "Let block be Block.\n",
            "Set block's size to 2.\n",
            "Its size becomes 8.\n",
            "Give back block's size as a whole number.\n",
        ),
        8,
        b"",
    );
}

// ---------------------------------------------------------------------------
// 5. End-to-end polish and presets
// ---------------------------------------------------------------------------

const POLISHED_USER_PROGRAM: &str = concat!(
    "Use io and math.\n\n",
    "Width is 3.0.\n",
    "Height is 4.0.\n",
    "Let diagonal be square root of width times width plus height times height.\n",
    "Say diagonal.\n",
    "Give back 0.\n",
);

#[test]
fn polished_user_program_imports_io_and_math() {
    assert_program("v47-user-io-math", POLISHED_USER_PROGRAM, 0, b"5.0000\n");
}

#[test]
fn preset_06_matches_reference_interpreter() {
    assert_parity("v47-preset-06", PRESETS[5].source);
}

#[test]
fn preset_07_matches_reference_interpreter() {
    assert_parity("v47-preset-07", PRESETS[6].source);
}

#[test]
fn preset_08_matches_reference_interpreter() {
    assert_parity("v47-preset-08", PRESETS[7].source);
}

// ---------------------------------------------------------------------------
// 6. Negative diagnostics
// ---------------------------------------------------------------------------

#[test]
fn negative_update_missing_value_is_a_clean_error() {
    assert_compile_error(
        "v47-neg-update",
        concat!(
            "A record Block has a size (an unsigned number).\n",
            "Let block be Block.\n",
            "Update block so its size is.\n",
            "Return 0.\n",
        ),
        "update clause",
    );
}

#[test]
fn negative_update_unknown_field_is_a_clean_error() {
    assert_compile_error(
        "v47-neg-field",
        concat!(
            "A record Block has a size (an unsigned number).\n",
            "Let block be Block.\n",
            "Update block so its missing field is 1.\n",
            "Return 0.\n",
        ),
        "assignment target",
    );
}

#[test]
fn negative_parenthetical_type_is_a_clean_error() {
    assert_compile_error(
        "v47-neg-type",
        concat!(
            "To bad n (a nonsense number), giving a whole number.\n",
            "Give back n.\n\n",
            "Return bad(1).\n",
        ),
        "type",
    );
}

// ---------------------------------------------------------------------------
// 7. Phase 4.7 readability gate across all eight `std/*.eng` modules
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
        if bytes[index] == b'.' {
            let before = bytes[index - 1];
            let after = bytes[index + 1];
            if (before.is_ascii_alphabetic() || before == b'_')
                && (after.is_ascii_alphanumeric() || after == b'_')
            {
                return true;
            }
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

fn strip_named_alias(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    for keyword in [" named ", " called "] {
        if let Some(position) = lower.find(keyword) {
            let alias_start = position + keyword.len();
            let rest = &line[alias_start..];
            let alias_end = rest
                .find([',', '.'])
                .map_or(line.len(), |offset| alias_start + offset);
            return format!("{}{}", &line[..position], &line[alias_end..]);
        }
    }
    line.to_string()
}

fn strip_public_compatibility(line: &str) -> String {
    let mut cleaned = line.to_string();
    for field in [
        "has_value",
        "is_ok",
        "ok_val",
        "err_val",
        "old_size",
        "new_size",
    ] {
        cleaned = cleaned.replace(field, "field");
    }
    cleaned
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
        assert!(
            !code.to_ascii_lowercase().contains(", returning nothing")
                && !code.to_ascii_lowercase().contains(", giving nothing"),
            "{module}: still writes an explicit void return clause"
        );
        let mut consecutive_set_fields = 0;
        let mut in_public_function = false;
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
            let trimmed = line.trim_start();
            if trimmed.to_ascii_lowercase().starts_with("publicly, to ")
                || trimmed.to_ascii_lowercase().starts_with("publicly to ")
            {
                in_public_function = true;
            } else if trimmed.starts_with("To ")
                || trimmed.to_ascii_lowercase().starts_with("public function ")
                || trimmed.to_ascii_lowercase().starts_with("function ")
            {
                in_public_function = false;
            }
            assert!(
                !trimmed.starts_with("Call this as") || in_public_function,
                "{module}: `Call this as` appears outside a public API header: `{trimmed}`"
            );
            if trimmed.starts_with("Set ")
                && !trimmed.starts_with("Set item ")
                && !trimmed.starts_with("Set element ")
                && trimmed.contains("'s ")
            {
                consecutive_set_fields += 1;
                assert!(
                    consecutive_set_fields < 3,
                    "{module}: three consecutive `Set <x>'s` lines begin at `{trimmed}`"
                );
            } else {
                consecutive_set_fields = 0;
            }
            let without_alias = strip_named_alias(line);
            let public_header = trimmed.to_ascii_lowercase().starts_with("publicly, to ")
                || trimmed.to_ascii_lowercase().starts_with("publicly to ");
            let without_compatibility = strip_public_compatibility(&without_alias);
            assert!(
                public_header || !without_compatibility.contains('_'),
                "{module}: underscore identifier outside a public alias: `{trimmed}`"
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
    }
}
