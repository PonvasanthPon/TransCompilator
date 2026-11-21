//! Phase 3.5 permanent English/front-end regressions for Core hardening.

use std::os::unix::process::CommandExt as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use tc_codegen_x86::compile_to_elf;
use tc_ir::{interpret, verify_module};
use tc_semantics::{SourceFile, compile_english_to_ir, lower_project};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_path(tag: &str) -> PathBuf {
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "tc_r1_battletest_semantics_{}_{}_{tag}.elf",
        std::process::id(),
        counter
    ))
}

fn assert_english(name: &str, files: &[(&str, &str)], expected_exit: i32, expected_stdout: &[u8]) {
    let sources = files
        .iter()
        .map(|(file, text)| SourceFile::new(*file, *text))
        .collect::<Vec<_>>();
    let compiled = compile_english_to_ir(&sources, "main")
        .unwrap_or_else(|errors| panic!("{name}: compilation failed: {errors:#?}"));
    verify_module(&compiled.module).unwrap_or_else(|errors| panic!("{name}: IR: {errors:#?}"));
    let args = vec!["prog".to_string()];
    let interp = interpret(&compiled.module, &args, &[], 50_000_000).expect("interpreter");
    assert_eq!(interp.exit_code, expected_exit, "{name}: interpreter exit");
    assert_eq!(interp.stdout, expected_stdout, "{name}: interpreter stdout");
    let path = unique_path(name);
    compile_to_elf(&compiled.module, &path).expect("ELF");
    let output = Command::new(&path)
        .arg0("prog")
        .env_clear()
        .stdin(Stdio::null())
        .output()
        .expect("run ELF");
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        interp.exit_code,
        "{name}: ELF/interpreter exit"
    );
    assert_eq!(output.stdout, interp.stdout, "{name}: stdout");
    assert_eq!(output.stderr, interp.stderr, "{name}: stderr");
    assert_eq!(output.status.code().unwrap_or(-1), expected_exit);
}

#[test]
fn english_float_to_int_saturates() {
    assert_english(
        "f2i",
        &[(
            "main.tc",
            "Return ((1.0e300 as i64) bitand 255) + ((200.0 as i8) as i64) + (((0.0 / 0.0) as i64) * 0).\n",
        )],
        126,
        b"",
    );
}

#[test]
fn global_pointer_mmap_indexing_uses_pointer_value() {
    assert_english(
        "gptr",
        &[(
            "main.tc",
            "Define global p as ptr<u8>.\nSet p to syscall(9, 0, 4096, 3, 0x22, -1, 0) as u64 as ptr<u8>.\nSet p[0] to 65.\nSet p[1] to 66.\nReturn (p[0] as i64) + (p[1] as i64).\n",
        )],
        131,
        b"",
    );
}

#[test]
fn memcopy_statement_copies_struct_bytes() {
    assert_english(
        "memcopy",
        &[(
            "main.tc",
            "Define struct A with x as i64 and y as u16.\nDefine struct B with x as i64 and y as u16.\na is A.\nb is B.\nSet a.x to 7.\nSet a.y to 9.\nCopy 16 bytes from address of a to address of b.\nReturn b.x + (b.y as i64).\n",
        )],
        16,
        b"",
    );
}

#[test]
fn fnptr_struct_field_and_dispatch_table() {
    assert_english(
        "fnptr",
        &[(
            "main.tc",
            "Define function add taking a as i64 and b as i64 returning i64:\n    Return a + b.\nDefine function mul taking a as i64 and b as i64 returning i64:\n    Return a * b.\nDefine struct Ops with f as fn(i64, i64) -> i64.\nops is Ops.\nSet ops.f to function add.\nfirst is ops.f(2, 3).\nDefine global table as array<fn(i64, i64) -> i64, 2>.\nSet table[0] to function add.\nSet table[1] to function mul.\nReturn first + table[1](4, 5).\n",
        )],
        25,
        b"",
    );
}

#[test]
fn type_directed_phrase_overloads_across_modules() {
    assert_english(
        "phrase-overload",
        &[
            (
                "a.tc",
                "Public function emit taking x as i64 returning i64:\n    Phrase: \"emit <x>\"\n    Return x.\n",
            ),
            (
                "b.tc",
                "Public function emit taking x as f64 returning i64:\n    Phrase: \"emit <x>\"\n    Return x as i64.\n",
            ),
            (
                "c.tc",
                "Public function emit taking x as ptr<u8> returning i64:\n    Phrase: \"emit <x>\"\n    Return 1.\n",
            ),
            (
                "main.tc",
                "Import a.\nImport b.\nImport c.\nEmit 42.\nEmit 3.5.\nEmit bytes_ptr(\"x\").\nReturn 7.\n",
            ),
        ],
        7,
        b"",
    );
}

#[test]
fn hex_binary_and_octal_literals() {
    assert_english(
        "radix",
        &[("main.tc", "Return 0x10 + 0b10 + 0o7 + 0x1F.\n")],
        56,
        b"",
    );
}

#[test]
fn malformed_nested_control_fails_without_panic() {
    let source = SourceFile::new(
        "main.tc",
        "If x is 1:\n    If y is 2:\n        Return 1.\nReturn 0.\n",
    );
    let result = compile_english_to_ir(&[source], "main");
    let errors = result.expect_err("undeclared condition variables must fail cleanly");
    assert!(
        errors
            .iter()
            .all(|error| !error.message.contains("panicked")),
        "diagnostics must never contain panic text"
    );
}

#[test]
fn unsigned_local_initializes_with_unsigned_zero() {
    assert_english(
        "unsigned-local",
        &[(
            "main.tc",
            "Define local x as u8.\nSet x to 255.\nSet x to x + 1.\nReturn x as i64.\n",
        )],
        0,
        b"",
    );
}

#[test]
fn null_literal_is_context_typed() {
    assert_english(
        "null-context",
        &[(
            "main.tc",
            "Define local p as ptr<i64>.\nIf p is not null and *p is 42:\n    Return 99.\nReturn 1.\n",
        )],
        1,
        b"",
    );
}

#[test]
fn type_changing_inner_assignment_shadows() {
    assert_english(
        "shadow",
        &[(
            "main.tc",
            "x is 5.\nIf true:\n    x is 1.5.\n    If true:\n        x is 9.\nReturn x.\n",
        )],
        5,
        b"",
    );
}

#[test]
fn global_function_pointer_is_mutated_and_called() {
    assert_english(
        "global-fnptr",
        &[(
            "main.tc",
            concat!(
                "Define function add taking a as i64 and b as i64 returning i64:\n    Return a + b.\n",
                "Define global fp as fn(i64, i64) -> i64.\nSet fp to function add.\nReturn fp(6, 7).\n",
            ),
        )],
        13,
        b"",
    );
}

#[test]
fn identifier_named_size_is_not_the_sizeof_operator() {
    assert_english(
        "size-ident",
        &[(
            "main.tc",
            "Define function f taking size as i64 returning i64:\n    Return size + 1.\nReturn f(6).\n",
        )],
        7,
        b"",
    );
}

#[test]
fn pointer_to_pointer_indexing_and_store() {
    assert_english(
        "ptr-ptr",
        &[(
            "main.tc",
            concat!(
                "Define global p as ptr<ptr<u8>>.\n",
                "Set p to syscall(9, 0, 4096, 3, 0x22, -1, 0) as u64 as ptr<ptr<u8>>.\n",
                "Set p[0] to syscall(9, 0, 4096, 3, 0x22, -1, 0) as u64 as ptr<u8>.\n",
                "Set p[0][3] to 90.\nReturn p[0][3] as i64.\n",
            ),
        )],
        90,
        b"",
    );
}

#[test]
fn passive_scalar_operations_mutate_the_subject() {
    assert_english(
        "passive-scalars",
        &[(
            "main.tc",
            "x is 10.\nx is increased by 5.\nx is multiplied by 3.\nx is decreased by 5.\nReturn x.\n",
        )],
        40,
        b"",
    );
}

#[test]
fn generic_function_can_declare_and_return_aggregate_locals() {
    assert_english(
        "generic-aggregate",
        &[(
            "main.tc",
            concat!(
                "Define struct Pair of T and U with first as T and second as U.\n",
                "Define function wrap of T taking x as T returning Pair<T, i64>:\n",
                "    r is Pair<T, i64>.\n    Set r.first to x.\n    Set r.second to 5.\n",
                "    Return r.\n",
                "p is wrap(7).\nReturn p.first + p.second.\n",
            ),
        )],
        12,
        b"",
    );
}

#[test]
fn ambiguous_phrase_dispatch_is_a_clean_error() {
    let sources = [
        SourceFile::new(
            "a.tc",
            "Public function emit taking x as i64 returning i64:\n    Phrase: \"emit <x>\"\n    Return x.\n",
        ),
        SourceFile::new(
            "b.tc",
            "Public function emit taking x as i64 returning i64:\n    Phrase: \"emit <x>\"\n    Return x.\n",
        ),
        SourceFile::new("main.tc", "Import a.\nImport b.\nEmit 42.\nReturn 0.\n"),
    ];
    let errors =
        compile_english_to_ir(&sources, "main").expect_err("ambiguous phrase dispatch must fail");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("ambiguous Phrase")),
        "{errors:#?}"
    );
}

#[test]
fn let_us_discourse_is_comment_omitted() {
    let project = lower_project(
        &[SourceFile::new(
            "main.tc",
            "x is 1. Let us check the total. Let's check again. Return x.\n",
        )],
        "main",
    )
    .expect("lower");
    for entry in &project.trace {
        if entry.text.starts_with("Let") {
            assert!(entry.intent.is_comment(), "{entry:?}");
        }
    }
}

#[test]
fn past_transfer_prose_with_location_preposition_is_comment_omitted() {
    let project = lower_project(
        &[SourceFile::new(
            "main.tc",
            "We spent 200 rupees on food. Return 7.\n",
        )],
        "main",
    )
    .expect("lower");
    let entry = project
        .trace
        .iter()
        .find(|entry| entry.text == "We spent 200 rupees on food")
        .expect("trace");
    assert!(entry.intent.is_comment(), "{entry:?}");
}

#[test]
fn recursive_aggregate_layout_is_a_clean_error() {
    let errors = compile_english_to_ir(
        &[SourceFile::new(
            "main.tc",
            "Define struct Bad with next as Bad.\nb is Bad.\nReturn 0.\n",
        )],
        "main",
    )
    .expect_err("recursive aggregate must fail cleanly");
    assert!(
        errors.iter().any(|error| error.message.contains("layout")),
        "{errors:#?}"
    );
}

#[test]
fn non_ascii_symbols_are_tokenized_without_panicking() {
    for text in ["\u{feff}", "\u{feff}Return 0.\n", "“hello”\n", "🚀"] {
        let tokens = tc_syntax::tokenize(text);
        assert!(!tokens.is_empty() || text.is_empty());
        let result = compile_english_to_ir(&[SourceFile::new("main.tc", text)], "main");
        if let Err(errors) = result {
            for error in errors {
                assert!(!error.message.contains("panicked"));
            }
        }
    }
}

#[test]
fn very_long_expression_returns_a_clean_complexity_error() {
    let text = format!(
        "Return {}.\n",
        (0..5000).map(|_| "1").collect::<Vec<_>>().join(" + ")
    );
    let errors = compile_english_to_ir(&[SourceFile::new("main.tc", text)], "main")
        .expect_err("over-budget expression must fail cleanly");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("complexity limit")),
        "{errors:#?}"
    );
}

#[test]
fn very_deep_type_syntax_returns_a_clean_limit_error() {
    let mut ty = String::from("i64");
    for _ in 0..200 {
        ty = format!("array<{ty}, 2>");
    }
    let text = format!("Define global x as {ty}.\nReturn 0.\n");
    let errors = compile_english_to_ir(&[SourceFile::new("main.tc", text)], "main")
        .expect_err("over-deep type must fail cleanly");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("type nesting")),
        "{errors:#?}"
    );
}

#[test]
fn numeric_passive_add_preserves_the_target() {
    assert_english(
        "numeric-passive",
        &[("main.tc", "x is 10.\n1 is added to x.\nReturn x + 31.\n")],
        42,
        b"",
    );
}

#[test]
fn noun_subject_that_is_also_a_verb_does_not_hijack_passive_voice() {
    assert_english(
        "cash-passive",
        &[(
            "main.tc",
            "cash is 42.\ncash is increased by 11.\nReturn cash.\n",
        )],
        53,
        b"",
    );
}

#[test]
fn reserved_control_keywords_reject_trailing_operands() {
    let project = lower_project(
        &[SourceFile::new("main.tc", "continue 10 of x.\nReturn 0.\n")],
        "main",
    )
    .expect("lower");
    let entry = project
        .trace
        .iter()
        .find(|entry| entry.text.starts_with("continue"))
        .expect("trace");
    assert!(entry.intent.is_unsupported(), "{entry:?}");
    assert!(
        compile_english_to_ir(
            &[SourceFile::new("main.tc", "continue 10 of x.\nReturn 0.\n")],
            "main"
        )
        .is_err()
    );
}

#[test]
fn pointer_typed_struct_field_can_be_indexed() {
    assert_english(
        "ptr-field",
        &[(
            "main.tc",
            concat!(
                "Define struct Holder of T with p as ptr<T>.\nh is Holder<u8>.\n",
                "Define global buf as array<u8, 4>.\n",
                "Set h.p to address of buf[0].\nSet h.p[1] to 8.\n",
                "Return buf[1] as i64.\n",
            ),
        )],
        8,
        b"",
    );
}

#[test]
fn pointer_field_assignment_advances_loop_variable() {
    assert_english(
        "ptr-field-loop",
        &[(
            "main.tc",
            concat!(
                "Define struct Node with next as ptr<Node> and value as i64.\n",
                "Define local first as Node.\nDefine local second as Node.\n",
                "Set first.value to 1.\nSet second.value to 2.\n",
                "Set first.next to address of second.\n",
                "Set second.next to 0 as u64 as ptr<Node>.\n",
                "Define local current as ptr<Node>.\ncurrent = address of first.\n",
                "Define local count as i64.\n",
                "While current as u64 != 0:\n",
                "    count += current.value.\n",
                "    current = current.next.\n",
                "Return count.\n",
            ),
        )],
        3,
        b"",
    );
}

#[test]
fn imported_public_struct_pointer_field_indexes_correctly() {
    assert_english(
        "imported-field",
        &[
            (
                "helper.tc",
                "Public struct Buffer with data as ptr<u8> and len as u64.\n",
            ),
            (
                "main.tc",
                concat!(
                    "Import helper.\n",
                    "Define global bytes as array<u8, 4>.\n",
                    "b is Buffer.\nSet b.data to address of bytes[0].\nSet b.len to 2.\n",
                    "Set b.data[b.len] to 7.\n",
                    "Return bytes[2] as i64.\n",
                ),
            ),
        ],
        7,
        b"",
    );
}

#[test]
fn bare_call_statement_with_address_arguments_lowers_as_expression() {
    assert_english(
        "bare-call",
        &[(
            "main.tc",
            concat!(
                "Define function zzz taking target as ptr<i64> and value as i64 returning void:\n",
                "    Set target[0] to value.\n",
                "Define local x as i64.\n",
                "zzz(address of x, 42).\n",
                "Return x.\n",
            ),
        )],
        42,
        b"",
    );
}

#[test]
fn phrase_arguments_reorder_to_parameter_names() {
    assert_english(
        "phrase-reorder",
        &[(
            "main.tc",
            concat!(
                "To join given left as i64, right as i64, named join, returning i64.\n",
                "Call this as \"join <right> and <left>\".\n",
                "Give back left * 10 + right.\n\n",
                "Return join 2 and 1.\n",
            ),
        )],
        12,
        b"",
    );
}

#[test]
fn generic_call_infers_unconstrained_parameter_from_expected_type() {
    assert_english(
        "generic-expected",
        &[(
            "main.tc",
            concat!(
                "Define struct Pair of T and E with first as T and second as E.\n",
                "Define function make of T and E taking value as T returning Pair<T, E>:\n",
                "    Define local result_value as Pair<T, E>.\n",
                "    Set result_value.first to value.\n",
                "    Return result_value.\n",
                "Define local p as Pair<i64, i64>.\n",
                "p = make(5).\n",
                "Return p.first.\n",
            ),
        )],
        5,
        b"",
    );
}

#[test]
fn signed_i64_min_literal_stays_negative() {
    assert_english(
        "i64-min",
        &[(
            "main.tc",
            "If -9223372036854775808 < 0:\n    Return 1.\nReturn 0.\n",
        )],
        1,
        b"",
    );
}

#[test]
fn phrase_result_assignment_with_is_lowers_as_call() {
    assert_english(
        "phrase-rhs",
        &[(
            "main.tc",
            concat!(
                "To twice given x as i64, named twice, returning i64.\n",
                "Call this as \"double value <x>\".\n",
                "Give back x * 2.\n\n",
                "result is double value 21.\n",
                "Return result.\n",
            ),
        )],
        42,
        b"",
    );
}

#[test]
fn imported_public_union_metadata_is_available_to_main() {
    assert_english(
        "imported-union",
        &[
            (
                "helper.tc",
                "Public union Bits with f as f64 and u as u64.\n",
            ),
            (
                "main.tc",
                concat!(
                    "Import helper.\n",
                    "bits is Bits.\nSet bits.f to 1.5.\n",
                    "Return ((bits.u shr 52) as i64) bitand 255.\n",
                ),
            ),
        ],
        255,
        b"",
    );
}
