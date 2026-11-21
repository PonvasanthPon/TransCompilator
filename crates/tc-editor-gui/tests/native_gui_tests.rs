//! Headless tests for the native eframe/egui R1 Editor.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use tc_editor::highlight::{LineState, TokenStyle, highlight_line};
use tc_editor::{Theme, ThemeId};
use tc_editor_gui::gui_highlight::span_format;
use tc_editor_gui::{
    GuiTheme, OutputTab, R1GuiApp, ThemeMode, Workspace, egui, highlight_job, line_state, std_index,
};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "r1_native_gui_{}_{}_{tag}",
        std::process::id(),
        counter
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("workspace");
    path
}

fn app(tag: &str) -> (R1GuiApp, PathBuf) {
    let dir = temp_dir(tag);
    (R1GuiApp::headless(&dir), dir)
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn section_colors(job: &egui::text::LayoutJob) -> Vec<egui::Color32> {
    job.sections
        .iter()
        .map(|section| section.format.color)
        .collect()
}

#[test]
fn gui_default_theme_is_light_with_clean_palette() {
    let theme = GuiTheme::light();
    assert_eq!(theme.mode, ThemeMode::Light);
    assert_eq!(theme.editor_bg, egui::Color32::from_rgb(255, 255, 255));
    assert_eq!(theme.window_bg, egui::Color32::from_rgb(246, 248, 250));
    assert_eq!(theme.ink, egui::Color32::from_rgb(31, 35, 40));
    assert_eq!(theme.accent, egui::Color32::from_rgb(9, 105, 218));
    assert_eq!(theme.success_fg, egui::Color32::from_rgb(26, 127, 55));
    assert_eq!(theme.error_fg, egui::Color32::from_rgb(207, 34, 46));
    assert_eq!(
        R1GuiApp::headless(Path::new(".")).theme_mode(),
        ThemeMode::Light
    );
}

#[test]
fn gui_soft_dark_theme_is_optional_and_not_default() {
    let dark = GuiTheme::dark();
    assert_eq!(dark.mode, ThemeMode::Dark);
    assert_ne!(dark.editor_bg, GuiTheme::light().editor_bg);
    let mut editor = R1GuiApp::headless(Path::new("."));
    assert_eq!(editor.theme_mode(), ThemeMode::Light);
    editor.toggle_theme();
    assert_eq!(editor.theme_mode(), ThemeMode::Dark);
    editor.toggle_theme();
    assert_eq!(editor.theme_mode(), ThemeMode::Light);
}

#[test]
fn gui_highlight_layout_job_uses_tc_lexicon() {
    let theme = GuiTheme::light();
    let job = highlight_job("If Remove 42 from i64 \"abc\".\n", &[], &[], &theme, 13.0);
    let colors = section_colors(&job);
    assert!(colors.contains(&theme.syntax_control));
    assert!(colors.contains(&theme.syntax_verb));
    assert!(colors.contains(&theme.syntax_number));
    assert!(colors.contains(&theme.syntax_type));
    assert!(colors.contains(&theme.syntax_string));
    assert_eq!(job.text, "If Remove 42 from i64 \"abc\".\n");
}

#[test]
fn gui_highlight_marks_omitted_prose_italic_gray() {
    let theme = GuiTheme::light();
    let job = highlight_job("It was raining outside.\n", &[], &[1], &theme, 13.0);
    assert_eq!(job.sections[0].format.color, theme.syntax_omitted);
    assert!(job.sections[0].format.italics);
    assert_eq!(line_state(1, &[], &[1]), LineState::ProseOmitted);
}

#[test]
fn gui_highlight_marks_error_line_background() {
    let theme = GuiTheme::light();
    let job = highlight_job("Add 10 to it.\n", &[1], &[], &theme, 13.0);
    assert_eq!(job.sections[0].format.background, theme.error_bg);
    assert_eq!(line_state(1, &[1], &[]), LineState::Error);
}

#[test]
fn gui_highlight_span_format_covers_all_public_styles() {
    let theme = GuiTheme::light();
    for kind in [
        TokenStyle::ControlAnchor,
        TokenStyle::ActionVerb,
        TokenStyle::AnaphorOrQuantifier,
        TokenStyle::Type,
        TokenStyle::Number,
        TokenStyle::String,
        TokenStyle::Operator,
        TokenStyle::Identifier,
        TokenStyle::Muted,
        TokenStyle::Omitted,
        TokenStyle::SearchMatch,
    ] {
        let format = span_format(&theme, kind, 13.0);
        assert!(format.font_id.size > 0.0);
    }
}

#[test]
fn gui_workspace_lists_eng_files_and_atomic_save_appends_eng() {
    let dir = temp_dir("workspace-save");
    fs::write(dir.join("a.eng"), "Return 0.\n").expect("a");
    fs::create_dir_all(dir.join("nested")).expect("nested");
    fs::write(dir.join("nested/b.eng"), "Return 1.\n").expect("b");
    fs::write(dir.join("ignore.txt"), "x").expect("txt");
    let mut workspace = Workspace::new(&dir).expect("workspace");
    let files = workspace.files();
    assert!(files.contains(&"a.eng".to_string()));
    assert!(files.contains(&"nested/b.eng".to_string()));
    assert!(!files.iter().any(|name| {
        std::path::Path::new(name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
    }));
    let path = workspace.save("demo", "Return 7.\n").expect("save");
    assert_eq!(
        path.file_name().and_then(|value| value.to_str()),
        Some("demo.eng")
    );
    assert!(!workspace.dirty());
    let (_, content) = workspace.open("demo.eng").expect("open");
    assert_eq!(content, "Return 7.\n");
    assert!(!dir.join("demo.eng.tmp").exists());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn gui_workspace_blocks_path_traversal() {
    let dir = temp_dir("workspace-traversal");
    let mut workspace = Workspace::new(&dir).expect("workspace");
    for path in ["../escape.eng", "/etc/passwd", "a/../../escape.eng"] {
        assert!(workspace.save(path, "bad").is_err(), "{path}");
    }
    assert!(workspace.open("../../etc/passwd").is_err());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn gui_app_tracks_dirty_then_saves_and_clears_it() {
    let (mut editor, dir) = app("dirty");
    assert!(!editor.workspace().dirty());
    editor.set_editor_text("Return 3.\n");
    assert!(editor.workspace().dirty());
    let path = editor.save_current().expect("save");
    assert!(path.is_file());
    assert!(!editor.workspace().dirty());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn gui_app_error_updates_gutter_and_clears_on_edit() {
    let (mut editor, dir) = app("gutter");
    editor.set_editor_text("Add 10 to it.\n");
    editor.run();
    assert!(!editor.error_lines().is_empty());
    assert!(editor.gutter_text().contains('✗'));
    editor.set_editor_text("Return 0.\n");
    assert!(editor.error_lines().is_empty());
    assert!(!editor.gutter_text().contains('✗'));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn gui_app_loads_and_clears_new_buffer() {
    let (mut editor, dir) = app("new-buffer");
    editor.set_editor_text("Return 9.\n");
    editor.new_buffer();
    assert!(editor.editor_text().is_empty());
    assert_eq!(editor.tab(), OutputTab::Run);
    assert!(!editor.workspace().dirty());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn gui_app_loads_all_8_presets_and_runs_exact_parity() {
    let (mut editor, dir) = app("presets-run");
    for index in 0..8 {
        assert!(editor.load_preset(index));
        editor.run();
        let result = editor.result().expect("run result");
        assert!(result.ok, "preset {index}: {}", result.summary);
        assert!(
            result.summary.contains("PARITY: EXACT MATCH"),
            "preset {index}: {}",
            result.summary
        );
    }
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn gui_app_all_presets_paraphrase_trace_and_ir() {
    let (mut editor, dir) = app("presets-inspectors");
    for index in 0..8 {
        assert!(editor.load_preset(index));
        editor.paraphrase();
        assert!(editor.result().expect("paraphrase").ok, "preset {index}");
        editor.trace();
        assert!(editor.result().expect("trace").ok, "preset {index}");
        editor.ir();
        assert!(editor.result().expect("ir").ok, "preset {index}");
    }
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn gui_app_build_emits_static_zero_libc_elf() {
    let (mut editor, dir) = app("build");
    editor.set_editor_text("Import io.\nSay 42.\nReturn 0.\n");
    let path = editor.save_current().expect("save");
    editor.build();
    let result = editor.result().expect("build result");
    assert!(result.ok, "{}", result.summary);
    let elf = path.with_extension("elf");
    assert!(elf.is_file());
    let readelf = Command::new("readelf")
        .arg("-d")
        .arg(&elf)
        .output()
        .expect("readelf");
    assert!(String::from_utf8_lossy(&readelf.stdout).contains("There is no dynamic section"));
    let ldd = Command::new("ldd").arg(&elf).output().expect("ldd");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&ldd.stdout),
        String::from_utf8_lossy(&ldd.stderr)
    );
    assert!(text.contains("not a dynamic executable"));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn gui_std_reference_lists_all_8_natural_english_std_modules() {
    let modules = std_index();
    assert_eq!(modules.len(), 8);
    let names = modules
        .iter()
        .map(|module| module.name.as_str())
        .collect::<Vec<_>>();
    for expected in [
        "alloc.eng",
        "core.eng",
        "fs.eng",
        "io.eng",
        "list.eng",
        "map.eng",
        "math.eng",
        "string.eng",
    ] {
        assert!(names.contains(&expected), "{names:?}");
    }
    assert!(modules.iter().any(|module| !module.functions.is_empty()));
}

#[test]
fn gui_cli_version_and_check_flags() {
    let binary = workspace_root().join("target/debug/r1-editor-gui");
    assert!(binary.is_file(), "run make build first");
    let version = Command::new(&binary)
        .arg("--version")
        .output()
        .expect("version");
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        "R1 Editor GUI 0.1.0 (tc_r1)"
    );
    let dir = temp_dir("cli-check");
    let good = dir.join("good.eng");
    fs::write(&good, "Return 0.\n").expect("good");
    let check = Command::new(&binary)
        .arg("--check")
        .arg(&good)
        .output()
        .expect("check");
    assert!(check.status.success(), "{check:?}");
    assert!(String::from_utf8_lossy(&check.stdout).contains("CHECK PASS"));
    let bad = dir.join("bad.eng");
    fs::write(&bad, "Add 10 to it.\n").expect("bad");
    let check = Command::new(&binary)
        .arg("--check")
        .arg(&bad)
        .output()
        .expect("bad check");
    assert!(!check.status.success());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn gui_zero_http_or_html_audit() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_files(&source, &mut files);
    for file in files {
        let name = file
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        assert_ne!(name, "http.rs");
        assert_ne!(name, "json.rs");
        assert_ne!(name, "api.rs");
        assert_ne!(name, "frontend.html");
        let text = fs::read_to_string(&file).expect("source");
        for forbidden in [
            "TcpListener",
            "frontend.html",
            "xdg-open",
            "chromium",
            "google-chrome",
            "text/html",
            "<html",
            "http://",
            "https://",
        ] {
            assert!(
                !text.contains(forbidden),
                "{} contains {forbidden}",
                file.display()
            );
        }
    }
}

fn collect_files(directory: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(directory) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                collect_files(&path, files);
            } else {
                files.push(path);
            }
        }
    }
}

#[test]
fn gui_std_function_phrases_are_indexed() {
    let modules = std_index();
    let phrases = modules
        .iter()
        .flat_map(|module| &module.functions)
        .flat_map(|function| &function.phrases)
        .collect::<Vec<_>>();
    assert!(phrases.iter().any(|phrase| phrase.contains("<n>")));
    assert!(phrases.iter().any(|phrase| phrase.contains("<x>")));
}

#[test]
fn gui_preset_names_are_unique_and_have_eight_entries() {
    let names = tc_editor::runner::PRESETS
        .iter()
        .map(|preset| preset.name)
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 8);
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 8);
    assert_eq!(names[5], "06_std_io_and_math.eng");
    assert_eq!(names[6], "07_std_generic_list.eng");
    assert_eq!(names[7], "08_std_file_and_map.eng");
}

#[test]
fn gui_theme_span_colors_match_highlight_categories() {
    let theme = GuiTheme::light();
    let terminal = Theme::from_id(ThemeId::Midnight);
    let spans = highlight_line("If Remove 42 from i64", LineState::Normal, None, &terminal);
    assert!(
        spans
            .iter()
            .any(|span| span.kind == TokenStyle::ControlAnchor)
    );
    assert!(spans.iter().any(|span| span.kind == TokenStyle::ActionVerb));
    assert!(spans.iter().any(|span| span.kind == TokenStyle::Type));
    let job = highlight_job("If Remove 42 from i64", &[], &[], &theme, 13.0);
    assert!(
        job.sections
            .iter()
            .any(|section| section.format.color == theme.syntax_control)
    );
}

#[test]
fn gui_open_file_roundtrip_uses_workspace() {
    let (mut editor, dir) = app("open-roundtrip");
    fs::write(dir.join("saved.eng"), "Return 11.\n").expect("write");
    editor.open_file("saved.eng").expect("open");
    assert_eq!(editor.editor_text(), "Return 11.\n");
    assert_eq!(editor.status(), "Opened saved.eng");
    let _ = fs::remove_dir_all(dir);
}
