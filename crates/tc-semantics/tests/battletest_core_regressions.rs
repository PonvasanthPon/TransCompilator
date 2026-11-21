//! Phase 3.5 permanent English/front-end regressions for Core hardening.

use std::os::unix::process::CommandExt as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use tc_codegen_x86::compile_to_elf;
use tc_ir::{interpret, verify_module};
use tc_semantics::{SourceFile, compile_english_to_ir};

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
