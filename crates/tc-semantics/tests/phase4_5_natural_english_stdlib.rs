//! Phase 4.5 natural-English systems syntax and readable `std/` audit.

use std::collections::{HashMap, VecDeque};
use std::io::Write as _;
use std::os::unix::process::CommandExt as _;
use std::path::PathBuf;
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
                return Err(format!("std module `{import}` not found"));
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
        "tc_r1_phase45_{}_{}_{tag}",
        std::process::id(),
        counter
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(path.join("interp")).expect("interp");
    std::fs::create_dir_all(path.join("elf")).expect("elf");
    path
}

fn assert_english(name: &str, source: &str, expected_exit: i32, expected_stdout: &[u8]) {
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
        b"",
        &interp_dir,
        50_000_000,
    )
    .unwrap_or_else(|error| panic!("{name}: interpreter failed: {error:?}"));
    let elf_path = work.join("program.elf");
    tc_codegen_x86::compile_to_elf(&compiled.module, &elf_path)
        .unwrap_or_else(|error| panic!("{name}: ELF failed: {error}"));
    let mut child = Command::new(&elf_path)
        .arg0("program")
        .env_clear()
        .current_dir(&elf_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ELF");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(b"")
        .expect("stdin");
    let native = child.wait_with_output().expect("wait ELF");
    let native_exit = native.status.code().unwrap_or(-1);
    assert_eq!(native_exit, interpreted.exit_code, "{name}: exit mismatch");
    assert_eq!(native.stdout, interpreted.stdout, "{name}: stdout mismatch");
    assert_eq!(native.stderr, interpreted.stderr, "{name}: stderr mismatch");
    assert_eq!(native_exit, expected_exit, "{name}: exit code");
    assert_eq!(
        native.stdout,
        expected_stdout,
        "{name}: stdout\n{}",
        String::from_utf8_lossy(&native.stdout)
    );
    let _ = std::fs::remove_dir_all(work);
}

#[test]
fn natural_use_of_multiple_modules() {
    assert_english(
        "use-multi",
        "Use core, io, and math.\nSay sqrt(9.0).\nReturn 0.\n",
        0,
        b"3.0000\n",
    );
}

#[test]
fn natural_record_with_articles_and_descriptors() {
    assert_english(
        "record-descriptors",
        concat!(
            "Use io.\n",
            "A public record Pair has a left value as a whole number and a right value as a whole number.\n",
            "p is Pair.\nSet p.left to 3.\nSet p.right to 4.\n",
            "Let total be p.left + p.right.\nSay total.\nReturn 0.\n",
        ),
        0,
        b"7\n",
    );
}

#[test]
fn natural_generic_record_and_type_phrase() {
    assert_english(
        "generic-record",
        concat!(
            "Use core, io, and list.\n",
            "Define local nums as List of whole number.\n",
            "push 4 to address of nums.\n",
            "Let value be list_get(address of nums, 0).\n",
            "Say value.\nlist_free(address of nums).\nReturn 0.\n",
        ),
        0,
        b"4\n",
    );
}

#[test]
fn natural_union_holds_clause() {
    assert_english(
        "union-holds",
        concat!(
            "Use core.\n",
            "A public union Bits holds f as a decimal or u as an unsigned number.\n",
            "bits is Bits.\nSet bits.f to 1.5.\n",
            "Return ((bits.u shr 52) as i64) bitand 255.\n",
        ),
        255,
        b"",
    );
}

#[test]
fn natural_shared_global_declaration() {
    assert_english(
        "shared-global",
        concat!(
            "Use core.\n",
            "Keep a public shared counter as a whole number.\n",
            "Set counter to 5.\nAdd 2 to counter.\nReturn counter.\n",
        ),
        7,
        b"",
    );
}

#[test]
fn natural_function_type_first_parameters() {
    assert_english(
        "function-type-first",
        concat!(
            "Use core.\n",
            "To add given a whole number a, a whole number b, returning a whole number.\n",
            "    Give back a + b.\n",
            "Done.\n",
            "Return add(2, 3).\n",
        ),
        5,
        b"",
    );
}

#[test]
fn natural_function_name_as_parameters() {
    assert_english(
        "function-name-as",
        concat!(
            "Use core.\n",
            "To scale given value as a whole number, by as a whole number, returning a whole number.\n",
            "    Give back value * by.\n",
            "Done.\n",
            "Return scale(6, 7).\n",
        ),
        42,
        b"",
    );
}

#[test]
fn natural_done_function_without_indentation() {
    assert_english(
        "function-flat",
        concat!(
            "Use core.\n",
            "To five returning a whole number.\n",
            "Give back 5.\n",
            "Done.\n",
            "Return five().\n",
        ),
        5,
        b"",
    );
}

#[test]
fn natural_call_this_single_phrase() {
    assert_english(
        "call-single",
        concat!(
            "Use io.\n",
            "To emit given x as a whole number, returning nothing.\n",
            "    Call this as \"emit <x>\".\n",
            "    Say x.\n",
            "Done.\n",
            "emit 9.\nReturn 0.\n",
        ),
        0,
        b"9\n",
    );
}

#[test]
fn natural_call_this_multiple_phrases() {
    assert_english(
        "call-multiple",
        concat!(
            "Use io.\n",
            "To show2 given x as a whole number, returning nothing.\n",
            "    You can call this as \"alpha <x>\", \"beta <x>\", or \"gamma <x>\".\n",
            "    Say x.\n",
            "Done.\n",
            "alpha 4.\nbeta 5.\nReturn 0.\n",
        ),
        0,
        b"4\n5\n",
    );
}

#[test]
fn natural_clear_bits_statement() {
    assert_english(
        "clear-bits",
        "Use core.\nLet n be 255.\nClear bits 15 from n.\nReturn n.\n",
        240,
        b"",
    );
}

#[test]
fn natural_set_bits_statement() {
    assert_english(
        "set-bits",
        "Use core.\nLet n be 240.\nSet bits 15 in n.\nReturn n.\n",
        255,
        b"",
    );
}

#[test]
fn natural_shift_left_expression() {
    assert_english(
        "shift-left",
        "Use core.\nLet n be 3.\nReturn shift n left by 4.\n",
        48,
        b"",
    );
}

#[test]
fn natural_shift_right_expression() {
    assert_english(
        "shift-right",
        "Use core.\nReturn shift 48 right by 4.\n",
        3,
        b"",
    );
}

#[test]
fn natural_bitwise_and_or_xor_inverse() {
    assert_english(
        "bitwise-phrases",
        concat!(
            "Use core.\n",
            "Let a be 12.\nLet b be 10.\n",
            "Let c be bitwise and of a and b.\n",
            "Let d be bitwise or of a and b.\n",
            "Let e be exclusive or of a and b.\n",
            "Let f be bitwise inverse of 0.\n",
            "Return c + d + e + f + 1.\n",
        ),
        28,
        b"",
    );
}

#[test]
fn natural_convert_expression() {
    assert_english(
        "convert",
        "Use core.\nLet n be convert 65 to a byte.\nReturn n as a whole number.\n",
        65,
        b"",
    );
}

#[test]
fn natural_byte_size_of_type() {
    assert_english(
        "byte-size",
        "Use core.\nReturn (byte size of a whole number) as a whole number.\n",
        8,
        b"",
    );
}

#[test]
fn natural_alignment_of_type() {
    assert_english(
        "alignment",
        "Use core.\nReturn (alignment of a whole number) as a whole number.\n",
        8,
        b"",
    );
}

#[test]
fn natural_memory_location_and_value_at() {
    assert_english(
        "memory-location",
        concat!(
            "Use core.\n",
            "Let n be 42.\n",
            "Let p be memory location of n.\n",
            "Return value at p.\n",
        ),
        42,
        b"",
    );
}

#[test]
fn natural_write_at_statement() {
    assert_english(
        "write-at",
        concat!(
            "Use core.\n",
            "Let n be 1.\n",
            "Let p be memory location of n.\n",
            "Write 9 at p.\n",
            "Return n.\n",
        ),
        9,
        b"",
    );
}

#[test]
fn natural_item_and_set_item_statements() {
    assert_english(
        "item-set",
        concat!(
            "Use core.\n",
            "Define global buf as block of 4 byte.\n",
            "Set item 2 of buf to 7.\n",
            "Return (item 2 of buf) as a whole number.\n",
        ),
        7,
        b"",
    );
}

#[test]
fn natural_copy_bytes_statement() {
    assert_english(
        "copy-bytes",
        concat!(
            "Use alloc and core.\n",
            "Let src be heap_alloc(4).\n",
            "Let dst be heap_alloc(4).\n",
            "Set item 0 of src to 1.\nSet item 1 of src to 2.\n",
            "Copy 2 bytes from src to dst.\n",
            "Let result be (item 1 of dst) as a whole number.\n",
            "heap_free(src).\nheap_free(dst).\nReturn result.\n",
        ),
        2,
        b"",
    );
}

#[test]
fn natural_ask_linux_kernel_expression() {
    assert_english(
        "ask-expression",
        concat!(
            "Use core.\n",
            "Define global buf as block of 1 byte.\n",
            "Let n be ask Linux kernel 0 with 0, address of item 0 of buf, and 0.\n",
            "Return n as a whole number.\n",
        ),
        0,
        b"",
    );
}

#[test]
fn natural_ask_linux_kernel_statement() {
    assert_english(
        "ask-statement",
        "Use core.\nAsk Linux kernel 1 with 1, \"hi\", and 2.\nReturn 0.\n",
        0,
        b"hi",
    );
}

#[test]
fn natural_null_address_aliases() {
    assert_english(
        "null-alias",
        concat!(
            "Use core.\n",
            "Let p be no address as a pointer to byte.\n",
            "If p as an unsigned number == 0:\n    Return 7.\nReturn 0.\n",
        ),
        7,
        b"",
    );
}

#[test]
fn natural_types_for_fields_and_values() {
    assert_english(
        "all-natural-types",
        concat!(
            "Use core.\n",
            "A record Sample has a byte_value as a byte, a signed_value as a short number, a count_value as a count, a decimal_value as a decimal, and a flag_value as a truth value.\n",
            "s is Sample.\n",
            "Set s.byte_value to 4.\nSet s.signed_value to 1.\nSet s.count_value to 3.\n",
            "Set s.decimal_value to 1.5.\nSet s.flag_value to true.\n",
            "Return s.byte_value as a whole number + s.signed_value as a whole number + s.count_value as a whole number.\n",
        ),
        8,
        b"",
    );
}

#[test]
fn natural_all_eight_std_modules_run_together() {
    assert_english(
        "all-std-modules",
        concat!(
            "Use alloc, core, fs, io, list, map, math, and string.\n",
            "Define local nums as List of whole number.\n",
            "push 2 to address of nums.\npush 8 to address of nums.\n",
            "Define local total as decimal.\n",
            "total = sqrt(list_get(address of nums, 0) as decimal + list_get(address of nums, 1) as decimal).\n",
            "Say total.\n",
            "Define local table as IntMap of whole number.\n",
            "intmap_put(address of table, 1, 41).\n",
            "Define local got as Option of whole number.\n",
            "got = intmap_get(address of table, 1).\nSay got.value.\n",
            "name is str_from_bytes(bytes_ptr(\"phase45.txt\"), bytes_len(\"phase45.txt\")).\n",
            "save 77 to file address of name.\n",
            "display file address of name.\n",
            "delete file address of name.\n",
            "str_free(address of name).\nlist_free(address of nums).\nintmap_free(address of table).\n",
            "Say heap_bytes_in_use() as i64.\nReturn 0.\n",
        ),
        0,
        b"3.1623\n41\n77\n0\n",
    );
}

fn contains_token(text: &str, token: &str) -> bool {
    let mut start = 0;
    while let Some(offset) = text[start..].find(token) {
        let position = start + offset;
        let before = text[..position].chars().next_back();
        let after = text[position + token.len()..].chars().next();
        let word = |character: Option<char>| {
            character.is_some_and(|value| value.is_ascii_alphanumeric() || value == '_')
        };
        if !word(before) && !word(after) {
            return true;
        }
        start = position + token.len();
    }
    false
}

#[test]
fn natural_std_readability_gate() {
    let forbidden = [
        "u64",
        "i64",
        "f64",
        "u8",
        "i8",
        "u16",
        "i16",
        "u32",
        "i32",
        "f32",
        "ptr<",
        "bitand",
        "bitnot",
        "bitor",
        "bitxor",
        "shl",
        "shr",
        "syscall(",
        "Define struct",
    ];
    let directory = std_dir();
    let mut files = std::fs::read_dir(&directory)
        .expect("std dir")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("eng"))
        .collect::<Vec<_>>();
    files.sort();
    assert_eq!(files.len(), 8);
    for file in files {
        let text = std::fs::read_to_string(&file).expect("std file");
        for token in forbidden {
            assert!(
                !contains_token(&text, token),
                "{} contains {token}",
                file.display()
            );
        }
        let first = text
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("");
        assert!(
            first.ends_with('.'),
            "{} lacks a module description",
            file.display()
        );
        assert!(
            first.len() > 20,
            "{} description is too short: {first}",
            file.display()
        );
        for line in text.lines() {
            let trimmed = line.trim();
            assert!(
                !trimmed.starts_with("function ") || !trimmed.ends_with(':'),
                "{} still has a colon function header: {trimmed}",
                file.display()
            );
        }
    }
}

#[test]
fn natural_std_uses_english_function_and_record_forms() {
    for name in [
        "core.eng",
        "alloc.eng",
        "io.eng",
        "list.eng",
        "fs.eng",
        "map.eng",
    ] {
        let text = std::fs::read_to_string(std_dir().join(name)).expect("std file");
        assert!(
            text.contains("To ") || text.contains("Publicly, to "),
            "{name} is missing prose function headers"
        );
    }
    let alloc = std::fs::read_to_string(std_dir().join("alloc.eng")).expect("alloc");
    assert!(alloc.to_ascii_lowercase().contains("clear bits 15 from n"));
    assert!(alloc.contains("ask Linux kernel 9 with"));
    assert!(alloc.contains("Give back"));
}

#[test]
fn natural_std_whitespace_and_phrases_survive() {
    let io = std::fs::read_to_string(std_dir().join("io.eng")).expect("io");
    assert!(io.contains("You can call this as \"say <n>\""));
    assert!(io.contains("\"square root of <x>\"") || !io.contains("sqrt"));
    let math = std::fs::read_to_string(std_dir().join("math.eng")).expect("math");
    assert!(math.contains("Call this as \"square root of <x>\"."));
    let list = std::fs::read_to_string(std_dir().join("list.eng")).expect("list");
    assert!(list.contains("Call this as \"push <val> to <items>\"."));
}
