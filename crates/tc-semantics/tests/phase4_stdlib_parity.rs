//! Phase 4 English standard-library parity: interpreter and static ELF64 in
//! isolated sandbox directories.

use std::collections::{HashMap, VecDeque};
use std::io::Write as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

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
        if tokens.first().is_some_and(|token| token.is("import")) {
            if let Some(name) = tokens.get(1).filter(|token| token.is_word()) {
                imports.push(name.lower.clone());
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
        "tc_r1_phase4_{}_{}_{tag}",
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

#[allow(clippy::too_many_arguments)]
fn assert_program_full(
    name: &str,
    source: &str,
    stdin: &[u8],
    expected_exit: i32,
    expected_stdout: &[u8],
    expected_stderr: &[u8],
    expected_files: &[(&str, &[u8])],
) {
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
    .unwrap_or_else(|error| panic!("{name}: interpreter failed: {error:?}"));
    let elf_path = work.join("program.elf");
    tc_codegen_x86::compile_to_elf(&compiled.module, &elf_path)
        .unwrap_or_else(|error| panic!("{name}: ELF failed: {error}"));
    let native = run_elf(&elf_path, &elf_dir, stdin);
    let interpreter_files = directory_files(&interp_dir);
    let elf_files = directory_files(&elf_dir);
    assert_eq!(
        native.exit_code, interpreted.exit_code,
        "{name}: exit mismatch"
    );
    assert_eq!(native.stdout, interpreted.stdout, "{name}: stdout mismatch");
    assert_eq!(native.stderr, interpreted.stderr, "{name}: stderr mismatch");
    assert_eq!(
        interpreter_files, elf_files,
        "{name}: created file mismatch"
    );
    assert_eq!(native.exit_code, expected_exit, "{name}: exit code");
    assert_eq!(
        native.stdout,
        expected_stdout,
        "{name}: stdout\n{}",
        String::from_utf8_lossy(&native.stdout)
    );
    assert_eq!(
        native.stderr,
        expected_stderr,
        "{name}: stderr\n{}",
        String::from_utf8_lossy(&native.stderr)
    );
    for (file, expected) in expected_files {
        assert_eq!(
            std::fs::read(interp_dir.join(file)).unwrap_or_default(),
            *expected,
            "{name}: interpreter file {file}"
        );
        assert_eq!(
            std::fs::read(elf_dir.join(file)).unwrap_or_default(),
            *expected,
            "{name}: ELF file {file}"
        );
    }
    let _ = std::fs::remove_dir_all(work);
}

fn assert_program(name: &str, source: &str, expected_exit: i32, expected_stdout: &[u8]) {
    assert_program_full(name, source, b"", expected_exit, expected_stdout, b"", &[]);
}

fn assert_program_stdin(
    name: &str,
    source: &str,
    stdin: &[u8],
    expected_exit: i32,
    expected_stdout: &[u8],
) {
    assert_program_full(
        name,
        source,
        stdin,
        expected_exit,
        expected_stdout,
        b"",
        &[],
    );
}

fn assert_panic(name: &str, source: &str, message: &str) {
    assert_program_full(
        name,
        source,
        b"",
        1,
        b"",
        format!("{message}\n").as_bytes(),
        &[],
    );
}

// ---------------------------------------------------------------------------
// std/core
// ---------------------------------------------------------------------------

#[test]
fn std_option_some_and_none_i64_and_point() {
    assert_program(
        "option",
        concat!(
            "Import core.\nImport io.\n",
            "Define local a as Option<i64>.\na = option_some(5).\n",
            "Say option_is_some(address of a).\nSay option_unwrap_or(address of a, 0).\n",
            "Define local b as Option<i64>.\nb = option_none().\n",
            "Say option_is_some(address of b).\nSay option_unwrap_or(address of b, 9).\n",
            "Define struct Point with x as i64 and y as i64.\n",
            "Define local p as Point.\nSet p.x to 2.\nSet p.y to 3.\n",
            "Define local op as Option<Point>.\nop = option_some(p).\n",
            "Say op.value.x.\nSay op.value.y.\nReturn 0.\n",
        ),
        0,
        b"true\n5\nfalse\n9\n2\n3\n",
    );
}

#[test]
fn std_result_ok_and_err_unwrap_or() {
    assert_program(
        "result",
        concat!(
            "Import core.\nImport io.\n",
            "Define local r as Result<i64, i64>.\nr = result_ok(5).\n",
            "Say result_is_ok(address of r).\nSay result_unwrap_or(address of r, 0).\n",
            "Define local e as Result<i64, i64>.\ne = result_err(7).\n",
            "Say result_is_ok(address of e).\nSay result_unwrap_or(address of e, 42).\n",
            "Return 0.\n",
        ),
        0,
        b"true\n5\nfalse\n42\n",
    );
}

#[test]
fn std_panic_writes_stderr_and_exits_1() {
    assert_panic(
        "panic",
        "Import core.\npanic(bytes_ptr(\"boom\"), bytes_len(\"boom\")).\nReturn 0.\n",
        "boom",
    );
}

#[test]
fn std_assert_true_pass_and_fail() {
    assert_panic(
        "assert",
        concat!(
            "Import core.\n",
            "assert_true(true, bytes_ptr(\"never\"), bytes_len(\"never\")).\n",
            "assert_true(false, bytes_ptr(\"assert failed\"), bytes_len(\"assert failed\")).\n",
            "Return 0.\n",
        ),
        "assert failed",
    );
}

#[test]
fn std_mem_zero_and_mem_eq() {
    assert_program(
        "mem",
        concat!(
            "Import core.\nImport io.\n",
            "Define global a as array<u8, 8>.\nDefine global b as array<u8, 8>.\n",
            "Set a[0] to 7.\nSet b[0] to 7.\n",
            "mem_zero(address of a[0], 8).\nmem_zero(address of b[0], 8).\n",
            "Say mem_eq(address of a[0], address of b[0], 8).\n",
            "Set b[3] to 9.\nSay mem_eq(address of a[0], address of b[0], 8).\n",
            "Return 0.\n",
        ),
        0,
        b"true\nfalse\n",
    );
}

// ---------------------------------------------------------------------------
// std/alloc
// ---------------------------------------------------------------------------

#[test]
fn std_alloc_basic_and_free_zero_leaks() {
    assert_program(
        "alloc-basic",
        concat!(
            "Import alloc.\nImport io.\n",
            "Define local p as ptr<u8>.\np = heap_alloc(64).\n",
            "Set p[0] to 65.\nSay p[0] as i64.\nheap_free(p).\n",
            "Say heap_bytes_in_use() as i64.\nReturn 0.\n",
        ),
        0,
        b"65\n0\n",
    );
}

#[test]
fn std_alloc_16_byte_alignment_verified() {
    assert_program(
        "alloc-align",
        concat!(
            "Import alloc.\nImport io.\n",
            "Define local i as i64.\nDefine local size as u64.\nsize = 3 as u64.\n",
            "While i < 20:\n",
            "    Define local p as ptr<u8>.\n    p = heap_alloc(size).\n",
            "    If (p as u64) % 16 != 0:\n        Return 1.\n",
            "    heap_free(p).\n    size += 7.\n    i += 1.\n",
            "Say heap_bytes_in_use() as i64.\nReturn 0.\n",
        ),
        0,
        b"0\n",
    );
}

#[test]
fn std_alloc_split_and_coalesce_reuse() {
    assert_program(
        "alloc-coalesce",
        concat!(
            "Import alloc.\nImport io.\n",
            "Define local a as ptr<u8>.\nDefine local b as ptr<u8>.\nDefine local c as ptr<u8>.\n",
            "a = heap_alloc(64).\nb = heap_alloc(64).\nc = heap_alloc(64).\n",
            "heap_free(c).\nheap_free(b).\n",
            "Define local d as ptr<u8>.\nd = heap_alloc(160).\n",
            "If d as u64 == b as u64:\n    Say 1.\nElse:\n    Say 0.\n",
            "heap_free(a).\nheap_free(d).\nSay heap_bytes_in_use() as i64.\nReturn 0.\n",
        ),
        0,
        b"1\n0\n",
    );
}

#[test]
fn std_alloc_large_over_64kb() {
    assert_program(
        "alloc-large",
        concat!(
            "Import alloc.\nImport io.\n",
            "Define local big as ptr<u8>.\nbig = heap_alloc(131072).\n",
            "Set big[0] to 1.\nSet big[131071] to 2.\n",
            "Say big[0] as i64.\nSay big[131071] as i64.\n",
            "heap_free(big).\nSay heap_bytes_in_use() as i64.\nReturn 0.\n",
        ),
        0,
        b"1\n2\n0\n",
    );
}

#[test]
fn std_alloc_realloc_preserves_data() {
    assert_program(
        "alloc-realloc",
        concat!(
            "Import alloc.\nImport io.\n",
            "Define local p as ptr<u8>.\np = heap_alloc(32).\n",
            "Set p[0] to 11.\nSet p[31] to 22.\n",
            "Define local q as ptr<u8>.\nq = heap_realloc(p, 32, 256).\n",
            "Say q[0] as i64.\nSay q[31] as i64.\n",
            "heap_free(q).\nSay heap_bytes_in_use() as i64.\nReturn 0.\n",
        ),
        0,
        b"11\n22\n0\n",
    );
}

#[test]
fn std_alloc_100_cycle_stress_no_corruption() {
    assert_program(
        "alloc-stress",
        concat!(
            "Import alloc.\nImport io.\n",
            "Define local i as i64.\n",
            "While i < 100:\n",
            "    Define local p as ptr<u8>.\n    p = heap_alloc(32).\n",
            "    Set p[0] to 7.\n",
            "    If p[0] != 7:\n        Return 1.\n",
            "    heap_free(p).\n    i += 1.\n",
            "Say heap_bytes_in_use() as i64.\nReturn 0.\n",
        ),
        0,
        b"0\n",
    );
}

// ---------------------------------------------------------------------------
// std/io
// ---------------------------------------------------------------------------

#[test]
fn std_io_say_i64_zero_pos_neg_min() {
    assert_program(
        "io-i64",
        concat!(
            "Import io.\n",
            "Say 0.\nSay 42.\nSay -42.\nSay -9223372036854775808.\nReturn 0.\n",
        ),
        0,
        b"0\n42\n-42\n-9223372036854775808\n",
    );
}

#[test]
fn std_io_say_u64_max() {
    assert_program(
        "io-u64",
        "Import io.\nSay 18446744073709551615.\nReturn 0.\n",
        0,
        b"18446744073709551615\n",
    );
}

#[test]
fn std_io_say_f64_exact_4_decimals() {
    assert_program(
        "io-f64",
        concat!(
            "Import io.\n",
            "Say 4.75.\nSay 19.635.\nSay 1.4142.\nSay 3.0.\nSay -0.0.\n",
            "Say -4.75.\nSay 0.99999.\nReturn 0.\n",
        ),
        0,
        b"4.7500\n19.6350\n1.4142\n3.0000\n0.0000\n-4.7500\n1.0000\n",
    );
}

#[test]
fn std_io_say_bool_true_false() {
    assert_program(
        "io-bool",
        "Import io.\nSay true.\nSay false.\nReturn 0.\n",
        0,
        b"true\nfalse\n",
    );
}

#[test]
fn std_io_phrase_overload_i64_f64_bool_in_same_program() {
    assert_program(
        "io-overload",
        "Import io.\nSay 42.\nSay 3.5.\nSay true.\nReturn 0.\n",
        0,
        b"42\n3.5000\ntrue\n",
    );
}

#[test]
fn std_io_read_i64_stdin_consecutive_lines() {
    assert_program_stdin(
        "io-read",
        concat!(
            "Import io.\n",
            "a is read_i64_stdin().\nb is read_i64_stdin().\nc is read_i64_stdin().\n",
            "Say a.\nSay b.\nSay c.\nReturn 0.\n",
        ),
        b"12\n-34\n56\n",
        0,
        b"12\n-34\n56\n",
    );
}

#[test]
fn std_io_read_f64_stdin() {
    assert_program_stdin(
        "io-readf",
        concat!(
            "Import io.\n",
            "a is read_f64_stdin().\nb is read_f64_stdin().\nSay a.\nSay b.\nReturn 0.\n",
        ),
        b"3.1416\n-2.5\n",
        0,
        b"3.1416\n-2.5000\n",
    );
}

#[test]
fn std_io_print_bytes_phrase() {
    assert_program(
        "io-bytes",
        "Import io.\nsay bytes bytes_ptr(\"raw\") of length 3.\nReturn 0.\n",
        0,
        b"raw\n",
    );
}

#[test]
fn std_io_format_f64_rounding_and_carry() {
    assert_program(
        "io-round",
        "Import io.\nSay 0.99999.\nSay 9.99999.\nSay -0.00001.\nReturn 0.\n",
        0,
        b"1.0000\n10.0000\n0.0000\n",
    );
}

// ---------------------------------------------------------------------------
// std/string
// ---------------------------------------------------------------------------

#[test]
fn std_string_from_bytes_and_say() {
    assert_program(
        "str-say",
        concat!(
            "Import string.\nImport io.\n",
            "s is str_from_bytes(bytes_ptr(\"hello\"), bytes_len(\"hello\")).\n",
            "Say address of s.\nstr_free(address of s).\nReturn 0.\n",
        ),
        0,
        b"hello\n",
    );
}

#[test]
fn std_string_concat_and_join_phrase() {
    assert_program(
        "str-join",
        concat!(
            "Import string.\nImport io.\n",
            "a is str_from_bytes(bytes_ptr(\"Hello\"), bytes_len(\"Hello\")).\n",
            "b is str_from_bytes(bytes_ptr(\" world\"), bytes_len(\" world\")).\n",
            "c is join address of a and address of b.\n",
            "Say address of c.\n",
            "str_free(address of a).\nstr_free(address of b).\nstr_free(address of c).\n",
            "Return 0.\n",
        ),
        0,
        b"Hello world\n",
    );
}

#[test]
fn std_string_eq_true_and_false() {
    assert_program(
        "str-eq",
        concat!(
            "Import string.\nImport io.\n",
            "a is str_from_bytes(bytes_ptr(\"same\"), bytes_len(\"same\")).\n",
            "b is str_from_bytes(bytes_ptr(\"same\"), bytes_len(\"same\")).\n",
            "c is str_from_bytes(bytes_ptr(\"diff\"), bytes_len(\"diff\")).\n",
            "Say str_eq(address of a, address of b).\nSay str_eq(address of a, address of c).\n",
            "str_free(address of a).\nstr_free(address of b).\nstr_free(address of c).\nReturn 0.\n",
        ),
        0,
        b"true\nfalse\n",
    );
}

#[test]
fn std_string_push_bytes_auto_growth() {
    assert_program(
        "str-growth",
        concat!(
            "Import string.\nImport io.\n",
            "s is str_from_bytes(bytes_ptr(\"ab\"), bytes_len(\"ab\")).\n",
            "str_push_bytes(address of s, bytes_ptr(\"cdefghij\"), 8).\n",
            "Say address of s.\n",
            "str_free(address of s).\nReturn 0.\n",
        ),
        0,
        b"abcdefghij\n",
    );
}

#[test]
fn std_string_clone_clear_free_zero_leaks() {
    assert_program(
        "str-clone",
        concat!(
            "Import alloc.\nImport string.\nImport io.\n",
            "a is str_from_bytes(bytes_ptr(\"copy\"), bytes_len(\"copy\")).\n",
            "b is str_clone(address of a).\n",
            "Say address of b.\n",
            "str_clear(address of b).\nDefine local n as u64.\nn = length of address of b.\nSay n.\n",
            "str_free(address of a).\nstr_free(address of b).\n",
            "Say heap_bytes_in_use() as i64.\nReturn 0.\n",
        ),
        0,
        b"copy\n0\n0\n",
    );
}

#[test]
fn std_string_nul_terminated_for_syscalls() {
    assert_program(
        "str-nul",
        concat!(
            "Import string.\nImport io.\n",
            "s is str_from_bytes(bytes_ptr(\"path\"), bytes_len(\"path\")).\n",
            "Say s.data[s.len] as i64.\nstr_free(address of s).\nReturn 0.\n",
        ),
        0,
        b"0\n",
    );
}

#[test]
fn std_string_len_phrase() {
    assert_program(
        "str-len-phrase",
        concat!(
            "Import string.\nImport io.\n",
            "s is str_from_bytes(bytes_ptr(\"abcdef\"), bytes_len(\"abcdef\")).\n",
            "n is length of address of s.\nSay n.\nstr_free(address of s).\nReturn 0.\n",
        ),
        0,
        b"6\n",
    );
}

// ---------------------------------------------------------------------------
// std/list
// ---------------------------------------------------------------------------

#[test]
fn std_list_i64_push_25_growth_pop_first_last_count() {
    assert_program(
        "list-i64",
        concat!(
            "Import list.\nImport io.\n",
            "Define local nums as List<i64>.\n",
            "Define local i as i64.\nWhile i < 25:\n    push i to address of nums.\n    i += 1.\n",
            "Say nums.cap.\nDefine local count as u64.\ncount = count of address of nums.\nSay count.\n",
            "Define local first_value as i64.\nfirst_value = first item of address of nums.\nSay first_value.\n",
            "Define local last_value as i64.\nlast_value = last item of address of nums.\nSay last_value.\n",
            "Define local popped as i64.\npopped = pop from address of nums.\nSay popped.\n",
            "count = count of address of nums.\nSay count.\n",
            "list_free(address of nums).\nReturn 0.\n",
        ),
        0,
        b"32\n25\n0\n24\n24\n24\n",
    );
}

#[test]
fn std_list_f64_push_sum_and_print() {
    assert_program(
        "list-f64",
        concat!(
            "Import list.\nImport io.\n",
            "Define local values as List<f64>.\n",
            "push 1.5 to address of values.\npush 2.25 to address of values.\n",
            "Define local total as f64.\n",
            "total = list_get(address of values, 0) + list_get(address of values, 1).\n",
            "Say total.\nlist_free(address of values).\nReturn 0.\n",
        ),
        0,
        b"3.7500\n",
    );
}

#[test]
fn std_list_string_push_and_print() {
    assert_program(
        "list-str",
        concat!(
            "Import list.\nImport string.\nImport io.\n",
            "Define local words as List<TcString>.\n",
            "a is str_from_bytes(bytes_ptr(\"alpha\"), bytes_len(\"alpha\")).\n",
            "b is str_from_bytes(bytes_ptr(\"beta\"), bytes_len(\"beta\")).\n",
            "push a to address of words.\npush b to address of words.\n",
            "Define local first_word as TcString.\nfirst_word = list_get(address of words, 0).\n",
            "Say address of first_word.\n",
            "str_free(address of a).\nstr_free(address of b).\nlist_free(address of words).\n",
            "Return 0.\n",
        ),
        0,
        b"alpha\n",
    );
}

#[test]
fn std_list_struct_point_push_pop_and_get() {
    assert_program(
        "list-point",
        concat!(
            "Import list.\nImport io.\n",
            "Define struct Point with x as i64 and y as i64.\n",
            "Define local points as List<Point>.\n",
            "Define local p as Point.\nSet p.x to 4.\nSet p.y to 5.\npush p to address of points.\n",
            "Define local q as Point.\nq = pop from address of points.\n",
            "Say q.x.\nSay q.y.\nlist_free(address of points).\nReturn 0.\n",
        ),
        0,
        b"4\n5\n",
    );
}

#[test]
fn std_list_clear_and_reuse() {
    assert_program(
        "list-clear",
        concat!(
            "Import list.\nImport io.\n",
            "Define local nums as List<i64>.\n",
            "push 1 to address of nums.\npush 2 to address of nums.\npush 3 to address of nums.\n",
            "list_clear(address of nums).\nDefine local count as u64.\ncount = count of address of nums.\nSay count.\n",
            "push 9 to address of nums.\ncount = count of address of nums.\nSay count.\nSay list_get(address of nums, 0).\n",
            "list_free(address of nums).\nReturn 0.\n",
        ),
        0,
        b"0\n1\n9\n",
    );
}

#[test]
fn std_list_sort_i64_and_struct_via_fnptr_comparator() {
    assert_program(
        "list-sort",
        concat!(
            "Import list.\nImport io.\n",
            "Define function cmp_i64 taking a as ptr<i64> and b as ptr<i64> returning i64:\n",
            "    If a[0] < b[0]:\n        Return -1.\n",
            "    If a[0] > b[0]:\n        Return 1.\n    Return 0.\n",
            "Define local nums as List<i64>.\n",
            "push 9 to address of nums.\npush 1 to address of nums.\npush 7 to address of nums.\n",
            "push 3 to address of nums.\npush 5 to address of nums.\n",
            "list_sort(address of nums, function cmp_i64).\n",
            "Define local i as u64.\nWhile i < 5:\n    Say list_get(address of nums, i).\n    i += 1.\n",
            "list_free(address of nums).\nReturn 0.\n",
        ),
        0,
        b"1\n3\n5\n7\n9\n",
    );
}

#[test]
fn std_list_bounds_panic_on_oob_get() {
    assert_panic(
        "list-oob",
        concat!(
            "Import list.\n",
            "Define local nums as List<i64>.\n",
            "Return list_get(address of nums, 0).\n",
        ),
        "list index out of bounds",
    );
}

#[test]
fn std_list_empty_pop_panic() {
    assert_panic(
        "list-pop-panic",
        concat!(
            "Import list.\n",
            "Define local nums as List<i64>.\n",
            "Return pop from address of nums.\n",
        ),
        "pop from empty list",
    );
}

#[test]
fn std_list_free_zero_leaks() {
    assert_program(
        "list-leaks",
        concat!(
            "Import alloc.\nImport list.\nImport io.\n",
            "Define local nums as List<i64>.\n",
            "push 1 to address of nums.\npush 2 to address of nums.\n",
            "list_free(address of nums).\nSay heap_bytes_in_use() as i64.\nReturn 0.\n",
        ),
        0,
        b"0\n",
    );
}

// ---------------------------------------------------------------------------
// std/fs
// ---------------------------------------------------------------------------

#[test]
fn std_fs_save_and_display_string_i64_f64() {
    assert_program_full(
        "fs-save",
        concat!(
            "Import fs.\nImport string.\nImport io.\n",
            "name is str_from_bytes(bytes_ptr(\"data.txt\"), bytes_len(\"data.txt\")).\n",
            "text is str_from_bytes(bytes_ptr(\"hello\"), bytes_len(\"hello\")).\n",
            "save address of text to file address of name.\n",
            "save 42 to file address of name.\n",
            "save 3.5 to file address of name.\n",
            "display file address of name.\n",
            "str_free(address of text).\nstr_free(address of name).\nReturn 0.\n",
        ),
        b"",
        0,
        b"3.5000\n",
        b"",
        &[("data.txt", b"3.5000")],
    );
}

#[test]
fn std_fs_file_exists_and_delete_file() {
    assert_program_full(
        "fs-exists",
        concat!(
            "Import fs.\nImport string.\nImport io.\n",
            "name is str_from_bytes(bytes_ptr(\"exists.txt\"), bytes_len(\"exists.txt\")).\n",
            "save 7 to file address of name.\n",
            "a is file exists address of name.\nSay a.\n",
            "delete file address of name.\n",
            "b is file exists address of name.\nSay b.\n",
            "str_free(address of name).\nReturn 0.\n",
        ),
        b"",
        0,
        b"true\nfalse\n",
        b"",
        &[],
    );
}

#[test]
fn std_fs_computed_string_path() {
    assert_program_full(
        "fs-path",
        concat!(
            "Import fs.\nImport string.\nImport io.\n",
            "prefix is str_from_bytes(bytes_ptr(\"out\"), bytes_len(\"out\")).\n",
            "suffix is str_from_bytes(bytes_ptr(\".txt\"), bytes_len(\".txt\")).\n",
            "name is join address of prefix and address of suffix.\n",
            "save 99 to file address of name.\n",
            "display file address of name.\n",
            "str_free(address of prefix).\nstr_free(address of suffix).\nstr_free(address of name).\n",
            "Return 0.\n",
        ),
        b"",
        0,
        b"99\n",
        b"",
        &[("out.txt", b"99")],
    );
}

#[test]
fn std_fs_persistent_file_handle_read_seek_rewind() {
    assert_program_full(
        "fs-seek",
        concat!(
            "Import fs.\nImport string.\nImport io.\n",
            "name is str_from_bytes(bytes_ptr(\"seek.txt\"), bytes_len(\"seek.txt\")).\n",
            "opened is file_open_write(address of name).\n",
            "Define local handle as File.\nhandle = opened.ok_val.\n",
            "text is str_from_bytes(bytes_ptr(\"ABCDEFGH\"), bytes_len(\"ABCDEFGH\")).\n",
            "file_write_bytes(address of handle, text.data, text.len).\n",
            "file_seek(address of handle, 0, 0).\n",
            "Define global buf as array<u8, 16>.\n",
            "n is file_read_bytes(address of handle, address of buf[0], 4).\n",
            "Say n.\nSay bytes address of buf[0] of length n as u64.\n",
            "file_seek(address of handle, 0, 0).\n",
            "m is file_read_bytes(address of handle, address of buf[0], 8).\n",
            "Say m.\nSay bytes address of buf[0] of length m as u64.\n",
            "file_close(address of handle).\n",
            "str_free(address of text).\nstr_free(address of name).\nReturn 0.\n",
        ),
        b"",
        0,
        b"4\nABCD\n8\nABCDEFGH\n",
        b"",
        &[("seek.txt", b"ABCDEFGH")],
    );
}

#[test]
fn std_fs_open_missing_returns_result_err() {
    assert_program(
        "fs-missing",
        concat!(
            "Import fs.\nImport string.\nImport io.\n",
            "name is str_from_bytes(bytes_ptr(\"missing.txt\"), bytes_len(\"missing.txt\")).\n",
            "opened is file_open_read(address of name).\n",
            "Say opened.is_ok.\n",
            "If not opened.is_ok:\n    Say opened.err_val < 0.\n",
            "str_free(address of name).\nReturn 0.\n",
        ),
        0,
        b"false\ntrue\n",
    );
}

#[test]
fn std_fs_append_mode() {
    assert_program_full(
        "fs-append",
        concat!(
            "Import fs.\nImport string.\nImport io.\n",
            "name is str_from_bytes(bytes_ptr(\"append.txt\"), bytes_len(\"append.txt\")).\n",
            "first is str_from_bytes(bytes_ptr(\"ab\"), bytes_len(\"ab\")).\n",
            "second is str_from_bytes(bytes_ptr(\"cd\"), bytes_len(\"cd\")).\n",
            "save address of first to file address of name.\n",
            "opened is file_open_append(address of name).\n",
            "Define local handle as File.\nhandle = opened.ok_val.\n",
            "file_write_bytes(address of handle, second.data, second.len).\n",
            "file_close(address of handle).\n",
            "display file address of name.\n",
            "str_free(address of first).\nstr_free(address of second).\nstr_free(address of name).\n",
            "Return 0.\n",
        ),
        b"",
        0,
        b"abcd\n",
        b"",
        &[("append.txt", b"abcd")],
    );
}

// ---------------------------------------------------------------------------
// std/math
// ---------------------------------------------------------------------------

#[test]
fn std_math_sqrt_exact_4_decimals() {
    assert_program(
        "math-sqrt",
        concat!(
            "Import math.\nImport io.\n",
            "Say sqrt(2.0).\nSay sqrt(9.0).\nSay sqrt(0.0).\n",
            "Say sqrt(0.25).\nSay sqrt(10000.0).\nReturn 0.\n",
        ),
        0,
        b"1.4142\n3.0000\n0.0000\n0.5000\n100.0000\n",
    );
}

#[test]
fn std_math_sqrt_negative_panics() {
    assert_panic(
        "math-neg",
        "Import math.\nImport io.\nSay sqrt(-1.0).\nReturn 0.\n",
        "sqrt of negative number",
    );
}

#[test]
fn std_math_abs_min_max_pow_clamp() {
    assert_program(
        "math-misc",
        concat!(
            "Import math.\nImport io.\n",
            "Say abs_i64(-9).\nSay abs_f64(-2.5).\n",
            "Say min_i64(4, 7).\nSay max_i64(4, 7).\n",
            "Say min_f64(1.5, 2.5).\nSay max_f64(1.5, 2.5).\n",
            "Say pow_i64(2, 10).\nSay clamp_i64(150, 1, 100).\nReturn 0.\n",
        ),
        0,
        b"9\n2.5000\n4\n7\n1.5000\n2.5000\n1024\n100\n",
    );
}

// ---------------------------------------------------------------------------
// std/map
// ---------------------------------------------------------------------------

#[test]
fn std_intmap_put_get_overwrite_remove_rehash() {
    assert_program(
        "intmap",
        concat!(
            "Import map.\nImport core.\nImport io.\n",
            "Define local table as IntMap<i64>.\n",
            "Define local i as i64.\nWhile i < 40:\n",
            "    intmap_put(address of table, i, i * 10).\n    i += 1.\n",
            "Say intmap_len(address of table).\n",
            "intmap_put(address of table, 7, 999).\n",
            "Define local found as Option<i64>.\nfound = intmap_get(address of table, 7).\n",
            "Say found.value.\n",
            "intmap_remove(address of table, 3).\n",
            "Define local removed as Option<i64>.\nremoved = intmap_get(address of table, 3).\n",
            "Say removed.has_value.\n",
            "Say intmap_len(address of table).\n",
            "intmap_free(address of table).\nReturn 0.\n",
        ),
        0,
        b"40\n999\nfalse\n39\n",
    );
}

#[test]
fn std_strmap_put_get_remove_rehash() {
    assert_program(
        "strmap",
        concat!(
            "Import map.\nImport string.\nImport core.\nImport io.\n",
            "Define local table as StrMap<i64>.\n",
            "alpha is str_from_bytes(bytes_ptr(\"alpha\"), bytes_len(\"alpha\")).\n",
            "beta is str_from_bytes(bytes_ptr(\"beta\"), bytes_len(\"beta\")).\n",
            "strmap_put(address of table, address of alpha, 1).\n",
            "strmap_put(address of table, address of beta, 2).\n",
            "strmap_put(address of table, address of alpha, 9).\n",
            "Define local got as Option<i64>.\ngot = strmap_get(address of table, address of alpha).\n",
            "Say got.value.\nSay strmap_len(address of table).\n",
            "strmap_remove(address of table, address of beta).\n",
            "Say strmap_contains(address of table, address of beta).\n",
            "Say strmap_len(address of table).\n",
            "strmap_free(address of table).\n",
            "str_free(address of alpha).\nstr_free(address of beta).\nReturn 0.\n",
        ),
        0,
        b"9\n2\nfalse\n1\n",
    );
}

#[test]
fn std_intmap_of_struct_point() {
    assert_program(
        "intmap-point",
        concat!(
            "Import map.\nImport core.\nImport io.\n",
            "Define struct Point with x as i64 and y as i64.\n",
            "Define local table as IntMap<Point>.\n",
            "Define local p as Point.\nSet p.x to 4.\nSet p.y to 5.\n",
            "intmap_put(address of table, 1, p).\n",
            "Define local got as Option<Point>.\ngot = intmap_get(address of table, 1).\n",
            "Say got.value.x.\nSay got.value.y.\n",
            "intmap_free(address of table).\nReturn 0.\n",
        ),
        0,
        b"4\n5\n",
    );
}

#[test]
fn std_map_free_zero_leaks() {
    assert_program(
        "map-leaks",
        concat!(
            "Import alloc.\nImport map.\nImport io.\n",
            "Define local table as IntMap<i64>.\n",
            "intmap_put(address of table, 1, 2).\n",
            "intmap_free(address of table).\n",
            "Say heap_bytes_in_use() as i64.\nReturn 0.\n",
        ),
        0,
        b"0\n",
    );
}

#[test]
fn std_map_remove_and_reinsert_same_key() {
    assert_program(
        "map-reinsert",
        concat!(
            "Import map.\nImport core.\nImport io.\n",
            "Define local table as IntMap<i64>.\n",
            "intmap_put(address of table, 5, 50).\n",
            "intmap_remove(address of table, 5).\n",
            "intmap_put(address of table, 5, 55).\n",
            "Define local got as Option<i64>.\ngot = intmap_get(address of table, 5).\n",
            "Say got.value.\nSay intmap_len(address of table).\n",
            "intmap_free(address of table).\nReturn 0.\n",
        ),
        0,
        b"55\n1\n",
    );
}

// ---------------------------------------------------------------------------
// End-to-end showcase and static ELF audit
// ---------------------------------------------------------------------------

#[test]
fn std_full_showcase_pure_english_program() {
    assert_program_full(
        "showcase",
        concat!(
            "Yesterday Ravi called me twice.\n",
            "Import alloc.\nImport core.\nImport io.\nImport math.\nImport list.\nImport string.\nImport fs.\n",
            "Define local values as List<i64>.\n",
            "push 3 to address of values.\npush 4 to address of values.\n",
            "Define local total as f64.\n",
            "total = sqrt(list_get(address of values, 0) as f64 + list_get(address of values, 1) as f64).\n",
            "Say total.\n",
            "name is str_from_bytes(bytes_ptr(\"show.txt\"), bytes_len(\"show.txt\")).\n",
            "save 77 to file address of name.\n",
            "display file address of name.\n",
            "delete file address of name.\n",
            "list_free(address of values).\nstr_free(address of name).\n",
            "Say heap_bytes_in_use() as i64.\nReturn 0.\n",
        ),
        b"",
        0,
        b"2.6458\n77\n0\n",
        b"",
        &[],
    );
}

#[test]
fn std_full_showcase_static_zero_libc_elf() {
    let source = concat!(
        "Import io.\nImport list.\nImport math.\nImport string.\nImport map.\n",
        "Define local nums as List<i64>.\npush 2 to address of nums.\n",
        "Say list_get(address of nums, 0).\n",
        "Say sqrt(9.0).\n",
        "Define local table as IntMap<i64>.\nintmap_put(address of table, 1, 2).\n",
        "Say intmap_len(address of table).\n",
        "list_free(address of nums).\nintmap_free(address of table).\nReturn 0.\n",
    );
    let (sources, entry) = build_sources(source).expect("sources");
    let compiled = compile_english_to_ir(&sources, &entry).expect("compile");
    let work = work_dir("static");
    let elf = work.join("app.elf");
    tc_codegen_x86::compile_to_elf(&compiled.module, &elf).expect("ELF");
    let ldd = Command::new("ldd").arg(&elf).output().expect("ldd");
    let ldd_text = format!(
        "{}{}",
        String::from_utf8_lossy(&ldd.stdout),
        String::from_utf8_lossy(&ldd.stderr)
    );
    assert!(ldd_text.contains("not a dynamic executable"), "{ldd_text}");
    let dynamic = Command::new("readelf")
        .arg("-d")
        .arg(&elf)
        .output()
        .expect("readelf -d");
    assert!(String::from_utf8_lossy(&dynamic.stdout).contains("There is no dynamic section"));
    let program = Command::new("readelf")
        .arg("-l")
        .arg(&elf)
        .output()
        .expect("readelf -l");
    assert!(!String::from_utf8_lossy(&program.stdout).contains("INTERP"));
    let _ = std::fs::remove_dir_all(work);
}

#[test]
fn std_option_none_point_returns_default_value() {
    assert_program(
        "option-none-point",
        concat!(
            "Import core.\nImport io.\n",
            "Define struct Point with x as i64.\n",
            "Define local p as Point.\nSet p.x to 8.\n",
            "Define local missing as Option<Point>.\nmissing = option_none().\n",
            "Say option_is_some(address of missing).\n",
            "Define local fallback as Point.\nfallback = option_unwrap_or(address of missing, p).\n",
            "Say fallback.x.\nReturn 0.\n",
        ),
        0,
        b"false\n8\n",
    );
}

#[test]
fn std_io_format_i64_into_and_f64_into_helpers() {
    assert_program(
        "io-format",
        concat!(
            "Import io.\n",
            "Define global buf as array<u8, 64>.\n",
            "n is format_i64_into(-123, address of buf[0]).\n",
            "say bytes address of buf[0] of length n.\n",
            "m is format_f64_into(7.25, address of buf[0]).\n",
            "say bytes address of buf[0] of length m.\nReturn 0.\n",
        ),
        0,
        b"-123\n7.2500\n",
    );
}

#[test]
fn std_string_empty_and_push_from_empty() {
    assert_program(
        "str-empty",
        concat!(
            "Import string.\nImport io.\n",
            "s is str_new().\n",
            "Define local n as u64.\nn = length of address of s.\nSay n.\n",
            "str_push_bytes(address of s, bytes_ptr(\"x\"), 1).\n",
            "Say address of s.\nstr_free(address of s).\nReturn 0.\n",
        ),
        0,
        b"0\nx\n",
    );
}

#[test]
fn std_list_generic_new_with_expected_type() {
    assert_program(
        "list-new",
        concat!(
            "Import list.\nImport io.\n",
            "Define local nums as List<i64>.\nnums = list_new().\n",
            "push 6 to address of nums.\n",
            "Define local n as u64.\nn = count of address of nums.\nSay n.\n",
            "list_free(address of nums).\nReturn 0.\n",
        ),
        0,
        b"1\n",
    );
}

#[test]
fn std_alloc_zero_size_and_null_free_are_safe() {
    assert_program(
        "alloc-zero",
        concat!(
            "Import alloc.\nImport io.\n",
            "p is heap_alloc(0).\n",
            "Define local location as u64.\nlocation = p as u64.\n",
            "If location == 0:\n    Return 1.\nSay true.\n",
            "heap_free(p).\n",
            "Define local nothing as ptr<u8>.\nheap_free(nothing).\n",
            "Say heap_bytes_in_use() as i64.\nReturn 0.\n",
        ),
        0,
        b"true\n0\n",
    );
}

#[test]
fn std_alloc_free_block_reused_for_same_size() {
    assert_program(
        "alloc-reuse",
        concat!(
            "Import alloc.\nImport io.\n",
            "a is heap_alloc(64).\n",
            "Define local first as u64.\nfirst = a as u64.\n",
            "heap_free(a).\n",
            "b is heap_alloc(64).\n",
            "Define local second as u64.\nsecond = b as u64.\n",
            "Say second == first.\n",
            "heap_free(b).\n",
            "Say heap_bytes_in_use() as i64.\nReturn 0.\n",
        ),
        0,
        b"true\n0\n",
    );
}

#[test]
fn std_io_read_i64_into_phrase_targets_local() {
    assert_program_stdin(
        "io-read-into",
        concat!(
            "Import io.\n",
            "Define local first as i64.\n",
            "read number into address of first.\n",
            "Say first.\n",
            "Define local second as i64.\n",
            "ask for address of second.\n",
            "Say second.\nReturn 0.\n",
        ),
        b"11\n-22\n",
        0,
        b"11\n-22\n",
    );
}

#[test]
fn std_string_equality_after_independent_growth() {
    assert_program(
        "str-eq-growth",
        concat!(
            "Import string.\nImport io.\n",
            "a is str_from_bytes(bytes_ptr(\"x\"), bytes_len(\"x\")).\n",
            "b is str_from_bytes(bytes_ptr(\"x\"), bytes_len(\"x\")).\n",
            "Say str_eq(address of a, address of b).\n",
            "str_push_bytes(address of b, bytes_ptr(\"y\"), 1).\n",
            "Say str_eq(address of a, address of b).\n",
            "Say address of a.\nSay address of b.\n",
            "str_free(address of a).\nstr_free(address of b).\nReturn 0.\n",
        ),
        0,
        b"true\nfalse\nx\nxy\n",
    );
}

#[test]
fn std_list_set_get_and_len() {
    assert_program(
        "list-set",
        concat!(
            "Import list.\nImport io.\n",
            "Define local nums as List<i64>.\n",
            "push 1 to address of nums.\npush 2 to address of nums.\n",
            "list_set(address of nums, 0, 7).\n",
            "Say list_get(address of nums, 0).\n",
            "Say list_get(address of nums, 1).\n",
            "Define local count as u64.\ncount = count of address of nums.\nSay count.\n",
            "list_free(address of nums).\nReturn 0.\n",
        ),
        0,
        b"7\n2\n2\n",
    );
}

#[test]
fn std_fs_read_file_to_str_roundtrip() {
    assert_program_full(
        "fs-read-str",
        concat!(
            "Import alloc.\nImport fs.\nImport string.\nImport io.\n",
            "name is str_from_bytes(bytes_ptr(\"read.txt\"), bytes_len(\"read.txt\")).\n",
            "text is str_from_bytes(bytes_ptr(\"roundtrip\"), bytes_len(\"roundtrip\")).\n",
            "save address of text to file address of name.\n",
            "opened is read_file_to_str(address of name).\n",
            "Define local loaded as TcString.\nloaded = opened.ok_val.\n",
            "Say address of loaded.\n",
            "Define local size as u64.\nsize = length of address of loaded.\nSay size.\n",
            "str_free(address of loaded).\nstr_free(address of text).\nstr_free(address of name).\n",
            "Say heap_bytes_in_use() as i64.\nReturn 0.\n",
        ),
        b"",
        0,
        b"roundtrip\n9\n0\n",
        b"",
        &[("read.txt", b"roundtrip")],
    );
}

#[test]
fn std_fs_seek_current_and_end() {
    assert_program_full(
        "fs-seek-cur",
        concat!(
            "Import fs.\nImport string.\nImport io.\n",
            "name is str_from_bytes(bytes_ptr(\"seek-cur.txt\"), bytes_len(\"seek-cur.txt\")).\n",
            "opened is file_open_write(address of name).\n",
            "Define local handle as File.\nhandle = opened.ok_val.\n",
            "text is str_from_bytes(bytes_ptr(\"0123456789\"), bytes_len(\"0123456789\")).\n",
            "file_write_bytes(address of handle, text.data, text.len).\n",
            "file_seek(address of handle, 4, 0).\n",
            "Define global buf as array<u8, 8>.\n",
            "n is file_read_bytes(address of handle, address of buf[0], 2).\n",
            "Say n.\nSay bytes address of buf[0] of length n as u64.\n",
            "position is file_seek(address of handle, 0, 1).\nSay position.\n",
            "size is file_seek(address of handle, 0, 2).\nSay size.\n",
            "file_close(address of handle).\n",
            "str_free(address of text).\nstr_free(address of name).\nReturn 0.\n",
        ),
        b"",
        0,
        b"2\n45\n6\n10\n",
        b"",
        &[("seek-cur.txt", b"0123456789")],
    );
}

#[test]
fn std_math_sqrt_tiny_and_large() {
    assert_program(
        "math-sqrt-range",
        concat!(
            "Import math.\nImport io.\n",
            "Say sqrt(0.0001).\nSay sqrt(1.0).\nSay sqrt(1000000.0).\nReturn 0.\n",
        ),
        0,
        b"0.0100\n1.0000\n1000.0000\n",
    );
}

#[test]
fn std_math_pow_and_clamp_edges() {
    assert_program(
        "math-edges",
        concat!(
            "Import math.\nImport io.\n",
            "Say pow_i64(2, 0).\nSay pow_i64(5, -1).\nSay pow_i64(0, 0).\n",
            "Say clamp_i64(-5, 0, 10).\nSay clamp_i64(5, 0, 10).\nSay clamp_i64(15, 0, 10).\n",
            "Say abs_f64(-0.0).\nReturn 0.\n",
        ),
        0,
        b"1\n0\n1\n0\n5\n10\n0.0000\n",
    );
}

#[test]
fn std_intmap_bulk_remove_then_reinsert() {
    assert_program(
        "intmap-tombstones",
        concat!(
            "Import map.\nImport core.\nImport io.\n",
            "Define local table as IntMap<i64>.\n",
            "Define local i as i64.\n",
            "While i < 20:\n    intmap_put(address of table, i, i).\n    i += 1.\n",
            "i = 0.\nWhile i < 20:\n    intmap_remove(address of table, i).\n    i += 1.\n",
            "Say intmap_len(address of table).\n",
            "i = 0.\nWhile i < 20:\n    intmap_put(address of table, i, i * 3).\n    i += 1.\n",
            "Say intmap_len(address of table).\n",
            "Define local found as Option<i64>.\nfound = intmap_get(address of table, 19).\nSay found.value.\n",
            "intmap_free(address of table).\nReturn 0.\n",
        ),
        0,
        b"0\n20\n57\n",
    );
}

#[test]
fn std_full_showcase_file_bytes_and_all_modules() {
    assert_program_full(
        "showcase-files",
        concat!(
            "Import alloc.\nImport core.\nImport io.\nImport math.\nImport list.\nImport string.\nImport fs.\nImport map.\n",
            "Define local values as List<i64>.\npush 2 to address of values.\npush 8 to address of values.\n",
            "Define local total as f64.\ntotal = sqrt(list_get(address of values, 0) as f64 + list_get(address of values, 1) as f64).\nSay total.\n",
            "Define local table as IntMap<i64>.\nintmap_put(address of table, 1, 41).\n",
            "Define local got as Option<i64>.\ngot = intmap_get(address of table, 1).\nSay got.value.\n",
            "name is str_from_bytes(bytes_ptr(\"persist.txt\"), bytes_len(\"persist.txt\")).\n",
            "text is str_from_bytes(bytes_ptr(\"kept\"), bytes_len(\"kept\")).\n",
            "save address of text to file address of name.\n",
            "display file address of name.\n",
            "str_free(address of text).\nstr_free(address of name).\n",
            "list_free(address of values).\nintmap_free(address of table).\n",
            "Say heap_bytes_in_use() as i64.\nReturn 0.\n",
        ),
        b"",
        0,
        b"3.1623\n41\nkept\n0\n",
        b"",
        &[("persist.txt", b"kept")],
    );
}
