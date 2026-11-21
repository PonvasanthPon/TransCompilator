//! Headless Phase 4 editor and `.eng` integration tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use tc_editor::{
    EditorApp, KeyEvent, Mode, PRESETS, PromptKind, TextBuffer, decode_keys, run_elf_with_parity,
    run_ir, run_paraphrase, run_trace,
};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "r1_editor_test_{}_{}_{tag}",
        std::process::id(),
        counter
    ));
    fs::create_dir_all(&path).expect("temp dir");
    path
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

fn editor_binary() -> PathBuf {
    workspace_root().join("target/debug/r1-editor")
}

fn type_text(app: &mut EditorApp, text: &str) {
    for character in text.chars() {
        let event = if character == '\n' {
            KeyEvent::Enter
        } else {
            KeyEvent::Char(character)
        };
        let _ = app.handle_key(event);
    }
}

// ---------------------------------------------------------------------------
// Buffer and editing
// ---------------------------------------------------------------------------

#[test]
fn buf_insert_backspace_delete_multiline() {
    let mut buffer = TextBuffer::from_text("ab\ncd");
    buffer.set_cursor(0, 1);
    buffer.insert_char('X');
    assert_eq!(buffer.lines()[0], "aXb");
    buffer.backspace();
    assert_eq!(buffer.lines()[0], "ab");
    buffer.move_end();
    buffer.delete_char();
    assert_eq!(buffer.lines()[0], "abcd");
    assert_eq!(buffer.cursor(), (0, 2));
}

#[test]
fn buf_utf8_multibyte_chars_safe() {
    let mut buffer = TextBuffer::from_text("வணக்கம் 😀");
    let length = buffer.lines()[0].chars().count();
    buffer.set_cursor(0, length);
    buffer.insert_char('é');
    buffer.backspace();
    buffer.set_cursor(0, 0);
    buffer.insert_char('x');
    buffer.move_left();
    buffer.delete_char();
    assert!(buffer.to_text().contains('😀'));
    assert!(buffer.to_text().contains("வணக்கம்"));
}

#[test]
fn buf_auto_indent_after_colon() {
    let mut buffer = TextBuffer::from_text("If x > 0:");
    buffer.move_end();
    buffer.insert_newline();
    assert_eq!(buffer.lines()[1], "    ");
    assert_eq!(buffer.cursor(), (1, 4));
    buffer.insert_char('x');
    assert_eq!(buffer.lines()[1], "    x");
}

#[test]
fn buf_undo_redo_roundtrip() {
    let mut buffer = TextBuffer::new();
    buffer.insert_char('a');
    buffer.insert_char('b');
    buffer.insert_char('c');
    assert_eq!(buffer.to_text(), "abc\n");
    assert!(buffer.undo());
    assert_eq!(buffer.to_text(), "ab\n");
    assert!(buffer.undo());
    assert!(buffer.undo());
    assert_eq!(buffer.to_text(), "\n");
    assert!(!buffer.undo());
    assert!(buffer.redo());
    assert_eq!(buffer.to_text(), "a\n");
}

#[test]
fn buf_find_next_wraps_case_insensitive() {
    let mut buffer = TextBuffer::from_text("Alpha beta alpha");
    buffer.set_cursor(0, 0);
    assert!(buffer.find_next("alpha"));
    assert_eq!(buffer.cursor(), (0, 11));
    assert!(buffer.find_next("ALPHA"));
    assert_eq!(buffer.cursor(), (0, 0));
    assert!(!buffer.find_next("missing"));
}

#[test]
fn buf_save_appends_eng_extension() {
    let dir = temp_dir("save_eng");
    let target = dir.join("budget");
    let mut buffer = TextBuffer::from_text("Return 1.");
    let saved = buffer.save_to_file(&target).expect("save");
    assert_eq!(
        saved.extension().and_then(|value| value.to_str()),
        Some("eng")
    );
    assert!(saved.is_file());
    assert!(!buffer.is_dirty());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn buf_load_and_save_eng_roundtrip() {
    let dir = temp_dir("roundtrip");
    let path = dir.join("story.eng");
    fs::write(&path, "a\r\nb\r\n").expect("write");
    let mut buffer = TextBuffer::load_from_file(&path).expect("load");
    assert_eq!(buffer.lines(), &["a".to_string(), "b".to_string()]);
    assert_eq!(buffer.to_text(), "a\nb\n");
    buffer.set_cursor(1, 1);
    buffer.insert_char('!');
    let saved = buffer.save_to_file(&path).expect("save");
    assert_eq!(fs::read_to_string(saved).expect("read"), "a\nb!\n");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn buf_cursor_clamping_and_scrolling() {
    let mut buffer = TextBuffer::from_text("short\nmuch longer line\nx");
    buffer.set_cursor(999, 999);
    assert_eq!(buffer.cursor(), (2, 1));
    buffer.set_cursor(1, 999);
    assert_eq!(buffer.cursor(), (1, 16));
    buffer.set_cursor(0, 0);
    buffer.ensure_cursor_visible(2, 4);
    assert_eq!(buffer.scroll(), (0, 0));
    buffer.set_cursor(2, 0);
    buffer.ensure_cursor_visible(2, 4);
    assert_eq!(buffer.scroll().0, 1);
}

#[test]
fn buf_tab_inserts_four_spaces() {
    let mut buffer = TextBuffer::new();
    buffer.insert_tab();
    assert_eq!(buffer.to_text(), "    \n");
    assert_eq!(buffer.cursor(), (0, 4));
}

#[test]
fn buf_to_text_has_trailing_newline() {
    assert_eq!(TextBuffer::from_text("one\ntwo").to_text(), "one\ntwo\n");
    assert_eq!(TextBuffer::new().to_text(), "\n");
}

// ---------------------------------------------------------------------------
// Key decoder
// ---------------------------------------------------------------------------

#[test]
fn keys_arrows_home_end_pgup_pgdn() {
    let events = decode_keys(b"\x1b[A\x1b[B\x1b[C\x1b[D\x1b[H\x1b[F\x1b[5~\x1b[6~\x1b[3~");
    assert_eq!(
        events,
        vec![
            KeyEvent::Up,
            KeyEvent::Down,
            KeyEvent::Right,
            KeyEvent::Left,
            KeyEvent::Home,
            KeyEvent::End,
            KeyEvent::PageUp,
            KeyEvent::PageDown,
            KeyEvent::Delete,
        ]
    );
}

#[test]
fn keys_function_keys_f1_to_f9() {
    let ss3 = decode_keys(b"\x1bOP\x1bOQ\x1bOR\x1bOS");
    assert_eq!(
        ss3,
        vec![
            KeyEvent::F(1),
            KeyEvent::F(2),
            KeyEvent::F(3),
            KeyEvent::F(4)
        ]
    );
    let csi =
        decode_keys(b"\x1b[11~\x1b[12~\x1b[13~\x1b[14~\x1b[15~\x1b[17~\x1b[18~\x1b[19~\x1b[20~");
    assert_eq!(
        csi,
        vec![
            KeyEvent::F(1),
            KeyEvent::F(2),
            KeyEvent::F(3),
            KeyEvent::F(4),
            KeyEvent::F(5),
            KeyEvent::F(6),
            KeyEvent::F(7),
            KeyEvent::F(8),
            KeyEvent::F(9),
        ]
    );
}

#[test]
fn keys_ctrl_shortcuts() {
    let events = decode_keys(b"\x13\x11\x12\x10\x14\x09\x15\x1a\x19\x06");
    assert_eq!(
        events,
        vec![
            KeyEvent::Ctrl('s'),
            KeyEvent::Ctrl('q'),
            KeyEvent::Ctrl('r'),
            KeyEvent::Ctrl('p'),
            KeyEvent::Ctrl('t'),
            KeyEvent::Tab,
            KeyEvent::Ctrl('u'),
            KeyEvent::Ctrl('z'),
            KeyEvent::Ctrl('y'),
            KeyEvent::Ctrl('f'),
        ]
    );
}

#[test]
fn keys_utf8_multibyte_decode() {
    let events = decode_keys("வணக்கம் 😀".as_bytes());
    let characters = events
        .iter()
        .filter_map(|event| match event {
            KeyEvent::Char(character) => Some(*character),
            _ => None,
        })
        .collect::<String>();
    assert_eq!(characters, "வணக்கம் 😀");
}

#[test]
fn keys_backspace_and_enter() {
    assert_eq!(
        decode_keys(b"\r\n\x7f\x08"),
        vec![
            KeyEvent::Enter,
            KeyEvent::Enter,
            KeyEvent::Backspace,
            KeyEvent::Backspace,
        ]
    );
}

// ---------------------------------------------------------------------------
// In-process runner and presets
// ---------------------------------------------------------------------------

fn assert_preset_works(index: usize) {
    let preset = &PRESETS[index];
    let parity = run_elf_with_parity(preset.source, None, "");
    assert!(parity.ok, "{} parity: {}", preset.name, parity.summary);
    assert!(parity.summary.contains("PARITY: EXACT MATCH"));
    let paraphrase = run_paraphrase(preset.source, None);
    assert!(paraphrase.ok, "{} paraphrase", preset.name);
    assert!(paraphrase.lines.join("\n").contains("Paraphrase"));
    let trace = run_trace(preset.source, None);
    assert!(trace.ok, "{} trace: {}", preset.name, trace.summary);
    let ir = run_ir(preset.source, None);
    assert!(ir.ok, "{} IR", preset.name);
    assert!(ir.lines.join("\n").contains("tc_fn"));
}

#[test]
fn preset_01_discourse_cash() {
    assert_preset_works(0);
}

#[test]
fn preset_02_struct_anaphora() {
    assert_preset_works(1);
}

#[test]
fn preset_03_quantifiers_prime() {
    assert_preset_works(2);
}

#[test]
fn preset_04_phrase_syscall_hello() {
    assert_preset_works(3);
}

#[test]
fn preset_05_passive_and_transfers() {
    assert_preset_works(4);
}

#[test]
fn runner_reports_unresolved_anaphor_cleanly() {
    let result = run_elf_with_parity("Add 10 to it.\n", None, "");
    assert!(!result.ok);
    let text = result.lines.join("\n");
    assert!(text.contains("UNRESOLVED_ANAPHOR"), "{text}");
}

#[test]
fn runner_build_elf_writes_static_zero_libc_binary() {
    let dir = temp_dir("build_elf");
    let path = dir.join("program.eng");
    fs::write(&path, PRESETS[1].source).expect("write");
    let result = tc_editor::build_elf_artifact(PRESETS[1].source, Some(&path));
    assert!(result.ok, "{} {}", result.summary, result.lines.join("\n"));
    let elf = dir.join("program.elf");
    assert!(elf.is_file());
    let readelf = Command::new("readelf")
        .arg("-d")
        .arg(&elf)
        .output()
        .expect("readelf");
    assert!(String::from_utf8_lossy(&readelf.stdout).contains("There is no dynamic section"));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn runner_stdin_piping_parity() {
    let source = concat!(
        "Define global buf as array<u8, 8>.\n",
        "syscall(0, 0, address of buf[0], 8).\n",
        "Return buf[0] as i64.\n",
    );
    let result = run_elf_with_parity(source, None, "A");
    assert!(result.ok, "{} {}", result.summary, result.lines.join("\n"));
    assert!(result.summary.contains("exit=65"), "{}", result.summary);
}

#[test]
fn runner_imports_eng_module() {
    let dir = temp_dir("imports");
    let main = dir.join("main.eng");
    fs::write(&main, "Import helper.\nReturn triple(14).\n").expect("main");
    fs::write(
        dir.join("helper.eng"),
        "Public function triple taking n as i64 returning i64:\n    Return n * 3.\n",
    )
    .expect("helper eng");
    fs::write(
        dir.join("helper.tc"),
        "Public function triple taking n as i64 returning i64:\n    Return n * 100.\n",
    )
    .expect("helper tc");
    let result = run_elf_with_parity("Import helper.\nReturn triple(14).\n", Some(&main), "");
    assert!(result.ok, "{} {}", result.summary, result.lines.join("\n"));
    assert!(result.summary.contains("exit=42"), "{}", result.summary);
    let _ = fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// App state machine and renderer
// ---------------------------------------------------------------------------

#[test]
fn app_typing_saving_and_running_eng_via_key_events() {
    let dir = temp_dir("app_run");
    let target = dir.join("typed.eng");
    let mut app = EditorApp::with_target(target.clone());
    type_text(&mut app, "Put 5 in x.\nAdd 1 to it.\nReturn x.\n");
    let _ = app.handle_key(KeyEvent::Ctrl('s'));
    assert!(target.is_file());
    let _ = app.handle_key(KeyEvent::F(5));
    assert!(app.output.ok, "{}", app.output.summary);
    let _ = app.handle_key(KeyEvent::F(6));
    assert!(app.output.lines.join("\n").contains("Paraphrase"));
    let _ = app.handle_key(KeyEvent::F(7));
    assert!(app.output.ok);
    let _ = app.handle_key(KeyEvent::F(8));
    assert!(app.output.lines.join("\n").contains("tc_fn"));
    let frame = app.render();
    assert!(frame.contains("R1 Editor"));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn app_dirty_quit_guard_requires_confirm() {
    let mut app = EditorApp::new();
    type_text(&mut app, "x");
    let _ = app.handle_key(KeyEvent::Ctrl('q'));
    assert!(!app.should_quit);
    assert!(app.quit_confirm);
    let _ = app.handle_key(KeyEvent::Esc);
    assert!(!app.quit_confirm);
    let _ = app.handle_key(KeyEvent::Ctrl('q'));
    let _ = app.handle_key(KeyEvent::Ctrl('q'));
    assert!(app.should_quit);
}

#[test]
fn app_preset_picker_loads_preset_1() {
    let mut app = EditorApp::new();
    let _ = app.handle_key(KeyEvent::F(2));
    assert_eq!(app.mode, Mode::PresetPicker);
    let _ = app.handle_key(KeyEvent::Char('1'));
    assert_eq!(app.mode, Mode::Editing);
    assert!(app.buffer.to_text().contains("For every i from 1 to 5"));
}

#[test]
fn ui_renders_wide_side_by_side_and_narrow_stacked() {
    let mut app = EditorApp::new();
    app.load_preset(0);
    let wide = tc_editor::ui::render(&app, 120, 35);
    let narrow = tc_editor::ui::render(&app, 80, 24);
    assert!(wide.contains("R1 Editor"));
    assert!(narrow.contains("R1 Editor"));
    assert!(wide.contains("RUN"));
    assert!(narrow.contains("RUN"));
    assert_ne!(wide, narrow);
    assert!(wide.len() > 100);
}

#[test]
fn app_prompt_save_as_appends_eng() {
    let dir = temp_dir("prompt_save");
    let target = dir.join("draft");
    let mut app = EditorApp::new();
    type_text(&mut app, "Return 1.");
    let _ = app.handle_key(KeyEvent::Ctrl('s'));
    assert_eq!(app.mode, Mode::Prompt);
    assert_eq!(app.prompt_kind, Some(PromptKind::SaveAs));
    for character in target.to_string_lossy().chars() {
        let _ = app.handle_key(KeyEvent::Char(character));
    }
    let _ = app.handle_key(KeyEvent::Enter);
    let expected = dir.join("draft.eng");
    assert!(expected.is_file());
    assert_eq!(app.mode, Mode::Editing);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn app_open_and_edit() {
    let dir = temp_dir("app_open");
    let path = dir.join("open.eng");
    fs::write(&path, "Return 1.\n").expect("write");
    let mut app = EditorApp::new();
    app.open_path(&path).expect("open");
    assert!(app.buffer.to_text().contains("Return 1."));
    let _ = app.handle_key(KeyEvent::End);
    let _ = app.handle_key(KeyEvent::Char(' '));
    assert!(app.buffer.is_dirty());
    let _ = fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// tc-cli `.eng` integration
// ---------------------------------------------------------------------------

#[test]
fn cli_build_run_interp_paraphrase_trace_on_eng_file() {
    let tc = tc_binary();
    assert!(tc.is_file(), "run make build first");
    let dir = temp_dir("cli_eng");
    let main = dir.join("main.eng");
    fs::write(&main, "Put 5 in x.\nAdd 1 to it.\nReturn x.\n").expect("write");
    let run = Command::new(&tc)
        .arg("run")
        .arg(&main)
        .output()
        .expect("run");
    assert_eq!(run.status.code(), Some(6));
    let interp = Command::new(&tc)
        .arg("interp")
        .arg(&main)
        .output()
        .expect("interp");
    assert_eq!(interp.status.code(), Some(6));
    let paraphrase = Command::new(&tc)
        .arg("paraphrase")
        .arg(&main)
        .output()
        .expect("paraphrase");
    assert!(paraphrase.status.success());
    assert!(String::from_utf8_lossy(&paraphrase.stdout).contains("Set x to 5."));
    let trace = Command::new(&tc)
        .arg("trace")
        .arg(&main)
        .arg("--paraphrase")
        .output()
        .expect("trace");
    let stdout = String::from_utf8_lossy(&trace.stdout);
    assert!(stdout.contains("--- TC-IR ---"));
    assert!(stdout.contains("=== Paraphrase ==="));
    let elf = dir.join("main.elf");
    let build = Command::new(&tc)
        .arg("build")
        .arg(&main)
        .arg("-o")
        .arg(&elf)
        .output()
        .expect("build");
    assert!(build.status.success());
    let executed = Command::new(&elf).output().expect("execute");
    assert_eq!(executed.status.code(), Some(6));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn cli_eng_multi_file_import_prefers_eng_extension() {
    let tc = tc_binary();
    let dir = temp_dir("cli_import_eng");
    let main = dir.join("main.eng");
    fs::write(&main, "Import helper.\nReturn triple(14).\n").expect("main");
    fs::write(
        dir.join("helper.eng"),
        "Public function triple taking n as i64 returning i64:\n    Return n * 3.\n",
    )
    .expect("helper eng");
    fs::write(
        dir.join("helper.tc"),
        "Public function triple taking n as i64 returning i64:\n    Return n * 100.\n",
    )
    .expect("helper tc");
    let run = Command::new(&tc)
        .arg("run")
        .arg(&main)
        .output()
        .expect("run");
    assert_eq!(run.status.code(), Some(42));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn cli_import_falls_back_to_tc_extension() {
    let tc = tc_binary();
    let dir = temp_dir("cli_import_tc");
    let main = dir.join("main.eng");
    fs::write(&main, "Import helper.\nReturn triple(7).\n").expect("main");
    fs::write(
        dir.join("helper.tc"),
        "Public function triple taking n as i64 returning i64:\n    Return n * 3.\n",
    )
    .expect("helper tc");
    let run = Command::new(&tc)
        .arg("run")
        .arg(&main)
        .output()
        .expect("run");
    assert_eq!(run.status.code(), Some(21));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn r1_editor_cli_version_and_check_flag() {
    let editor = editor_binary();
    assert!(editor.is_file(), "run make build first");
    let version = Command::new(&editor)
        .arg("--version")
        .output()
        .expect("version");
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        "R1 Editor 0.1.0 (tc_r1)"
    );
    let presets = Command::new(&editor)
        .arg("--list-presets")
        .output()
        .expect("presets");
    assert!(presets.status.success());
    assert_eq!(String::from_utf8_lossy(&presets.stdout).lines().count(), 8);
    let dir = temp_dir("editor_check");
    let path = dir.join("check.eng");
    fs::write(&path, PRESETS[0].source).expect("write");
    let check = Command::new(&editor)
        .arg("--check")
        .arg(&path)
        .output()
        .expect("check");
    assert!(check.status.success());
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(stdout.contains("PARITY: EXACT MATCH"), "{stdout}");
    assert!(stdout.contains("PARAPHRASE"), "{stdout}");
    let _ = fs::remove_dir_all(dir);
}
