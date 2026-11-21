//! Headless tests for the visual and ergonomic TUI redesign.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use tc_editor::ui::render_rows;
use tc_editor::{
    EditorApp, KeyEvent, LineState, Theme, ThemeId, TokenStyle, decode_keys, highlight_line,
};

fn theme() -> Theme {
    Theme::from_id(ThemeId::Midnight)
}

fn matching<'a>(spans: &'a [tc_editor::StyledSpan], text: &str) -> Vec<&'a tc_editor::StyledSpan> {
    spans.iter().filter(|span| span.text == text).collect()
}

fn type_text(app: &mut EditorApp, text: &str) {
    for character in text.chars() {
        let _ = app.handle_key(if character == '\n' {
            KeyEvent::Enter
        } else {
            KeyEvent::Char(character)
        });
    }
}

fn temp_path(tag: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    std::env::temp_dir().join(format!("r1_tui_{}_{}_{tag}.eng", std::process::id(), stamp))
}

#[test]
fn highlight_uses_lexicon_verbs_and_types() {
    let palette = theme();
    let spans = highlight_line(
        "If Remove Add Transfer it its every i64 f64 42 3.14 \"hello\"",
        LineState::Normal,
        None,
        &palette,
    );
    assert_eq!(matching(&spans, "If")[0].kind, TokenStyle::ControlAnchor);
    for verb in ["Remove", "Add", "Transfer"] {
        assert_eq!(
            matching(&spans, verb)[0].kind,
            TokenStyle::ActionVerb,
            "{verb}"
        );
    }
    assert_eq!(matching(&spans, "i64")[0].kind, TokenStyle::Type);
    assert_eq!(matching(&spans, "f64")[0].kind, TokenStyle::Type);
    assert_eq!(matching(&spans, "42")[0].kind, TokenStyle::Number);
    assert_eq!(matching(&spans, "3.14")[0].kind, TokenStyle::Number);
    assert_eq!(matching(&spans, "\"hello\"")[0].kind, TokenStyle::String);
    for word in ["it", "its", "every"] {
        assert_eq!(
            matching(&spans, word)[0].kind,
            TokenStyle::AnaphorOrQuantifier
        );
    }
}

#[test]
fn highlight_prose_omitted_dims_entire_line() {
    let spans = highlight_line(
        "It was raining outside.",
        LineState::ProseOmitted,
        None,
        &theme(),
    );
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, TokenStyle::Omitted);
    assert_eq!(spans[0].text, "It was raining outside.");
    assert!(spans[0].style.italic);
    assert!(spans[0].style.dim);
}

#[test]
fn highlight_error_line_sets_error_style() {
    let palette = theme();
    let spans = highlight_line("Add 1 to it.", LineState::Error, None, &palette);
    assert!(!spans.is_empty());
    for span in spans {
        assert_eq!(span.style.bg, Some(palette.error_bg));
    }
}

#[test]
fn highlight_search_query_highlights_match_case_insensitive() {
    let palette = theme();
    let spans = highlight_line("cash Cash CASH", LineState::Normal, Some("cash"), &palette);
    let matches = spans
        .iter()
        .filter(|span| span.kind == TokenStyle::SearchMatch)
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 3);
    assert_eq!(matches[0].text.to_ascii_lowercase(), "cash");
    assert_eq!(matches[2].text.to_ascii_lowercase(), "cash");
}

#[test]
fn highlight_utf8_multibyte_safe() {
    let line = "Set \"வணக்கம் 😀\" to 1.";
    let spans = highlight_line(line, LineState::Normal, None, &theme());
    let rendered = spans
        .iter()
        .map(|span| span.text.as_str())
        .collect::<String>();
    assert_eq!(rendered, line);
}

#[test]
fn theme_cycle_f4_midnight_high_contrast_monochrome() {
    let mut app = EditorApp::with_theme(ThemeId::Midnight);
    assert_eq!(app.theme.id, ThemeId::Midnight);
    let _ = app.handle_key(KeyEvent::F(4));
    assert_eq!(app.theme.id, ThemeId::HighContrast);
    let _ = app.handle_key(KeyEvent::F(4));
    assert_eq!(app.theme.id, ThemeId::Monochrome);
    let _ = app.handle_key(KeyEvent::F(4));
    assert_eq!(app.theme.id, ThemeId::Midnight);
    assert_eq!(ThemeId::HighContrast.name(), "High-Contrast");
}

#[test]
fn theme_monochrome_emits_zero_rgb_escapes() {
    let mut app = EditorApp::with_theme(ThemeId::Monochrome);
    let _ = app.load_preset(0);
    let _ = app.handle_key(KeyEvent::F(6));
    let frame = app.render();
    assert!(!frame.contains("38;2;"));
    assert!(!frame.contains("48;2;"));
    assert!(frame.contains('╭'));
}

#[test]
fn ui_wide_140x40_and_narrow_80x24_box_borders_aligned() {
    let mut app = EditorApp::with_theme(ThemeId::Midnight);
    let _ = app.load_preset(0);
    for (width, height) in [(140, 40), (80, 24)] {
        let rows = render_rows(&app, width, height);
        assert_eq!(rows.len(), height);
        for row in &rows {
            assert_eq!(row.chars().count(), width, "row width mismatch");
        }
        let joined = rows.join("\n");
        assert_eq!(joined.matches('╭').count(), 2);
        assert_eq!(joined.matches('╮').count(), 2);
        assert_eq!(joined.matches('╰').count(), 2);
        assert_eq!(joined.matches('╯').count(), 2);
        assert!(joined.contains("[F5 Run]"));
        assert!(joined.contains("F5/^R Run"));
    }
    let frame = app.render();
    assert!(frame.contains("\x1b[?2026h"));
    assert!(frame.contains("\x1b[?2026l"));
}

#[test]
fn ui_long_line_truncates_with_ellipsis_without_breaking_border() {
    let mut app = EditorApp::with_theme(ThemeId::Midnight);
    app.buffer
        .replace_text(&format!("Return {}.\n", "x".repeat(240)));
    let rows = render_rows(&app, 80, 24);
    assert!(rows.iter().any(|row| row.contains('…')));
    for row in rows {
        assert_eq!(row.chars().count(), 80);
    }
}

#[test]
fn ui_preset_picker_modal_arrow_navigation_and_enter_loads_preset() {
    let mut app = EditorApp::with_theme(ThemeId::Midnight);
    let _ = app.handle_key(KeyEvent::F(2));
    let picker = render_rows(&app, 100, 30).join("\n");
    assert!(picker.contains("Preset Picker"));
    assert!(picker.contains("01_discourse_cash.eng"));
    let _ = app.handle_key(KeyEvent::Down);
    assert_eq!(app.preset_selection, 1);
    let _ = app.handle_key(KeyEvent::Enter);
    assert_eq!(app.mode, tc_editor::Mode::Editing);
    assert!(app.buffer.to_text().contains("Define struct Point"));
}

#[test]
fn ui_help_modal_renders_and_closes_on_esc() {
    let mut app = EditorApp::with_theme(ThemeId::Midnight);
    let _ = app.handle_key(KeyEvent::F(1));
    let help = render_rows(&app, 100, 30).join("\n");
    assert!(help.contains("R1 EDITOR HELP"));
    assert!(help.contains("RUN & INSPECT"));
    let _ = app.handle_key(KeyEvent::Esc);
    assert_eq!(app.mode, tc_editor::Mode::Editing);
    let closed = render_rows(&app, 100, 30).join("\n");
    assert!(!closed.contains("RUN & INSPECT"));
}

#[test]
fn ui_colorizes_paraphrase_anaphors_and_trace_verdicts() {
    let mut app = EditorApp::with_theme(ThemeId::Midnight);
    let _ = app.load_preset(0);
    let _ = app.handle_key(KeyEvent::F(6));
    let paraphrase = app.render();
    assert!(paraphrase.contains("[it -> cash]"));
    assert!(paraphrase.contains("[OMITTED:"));
    assert!(paraphrase.contains("38;2;42;195;222"));
    assert!(paraphrase.contains("38;2;224;175;104"));
    let _ = app.handle_key(KeyEvent::F(7));
    let trace = app.render();
    assert!(trace.contains("EXEC"));
    assert!(trace.contains("COMMENT_OMITTED"));
    assert!(trace.contains("38;2;115;218;202"));
}

#[test]
fn app_updates_line_states_on_run_and_clears_on_edit() {
    let mut app = EditorApp::with_theme(ThemeId::Midnight);
    type_text(&mut app, "Add 10 to it.\n");
    let _ = app.handle_key(KeyEvent::F(5));
    assert_eq!(app.line_state(1), LineState::Error);
    let error_frame = render_rows(&app, 100, 30).join("\n");
    assert!(error_frame.contains('✗'));
    let _ = app.handle_key(KeyEvent::Up);
    let _ = app.handle_key(KeyEvent::End);
    let _ = app.handle_key(KeyEvent::Char(' '));
    assert_eq!(app.line_state(1), LineState::Normal);
}

#[test]
fn app_marks_omitted_prose_after_paraphrase() {
    let mut app = EditorApp::with_theme(ThemeId::Midnight);
    let _ = app.load_preset(0);
    let _ = app.handle_key(KeyEvent::F(5));
    assert_eq!(app.line_state(1), LineState::ProseOmitted);
    assert_eq!(app.line_state(2), LineState::ProseOmitted);
    let _ = app.handle_key(KeyEvent::F(6));
    assert_eq!(app.line_state(1), LineState::ProseOmitted);
    assert_eq!(app.line_state(2), LineState::ProseOmitted);
    let _ = app.handle_key(KeyEvent::Char('X'));
    assert_eq!(app.line_state(1), LineState::Normal);
    assert_eq!(app.line_state(2), LineState::ProseOmitted);
}

#[test]
fn keys_decode_ctrl_left_and_ctrl_right_word_jump() {
    assert_eq!(
        decode_keys(b"\x1b[1;5D\x1b[1;5C"),
        vec![KeyEvent::CtrlLeft, KeyEvent::CtrlRight]
    );
}

#[test]
fn buf_move_word_left_right() {
    let mut buffer = tc_editor::TextBuffer::from_text("alpha beta gamma");
    buffer.move_end();
    buffer.move_word_left();
    assert_eq!(buffer.cursor(), (0, 11));
    buffer.move_word_left();
    assert_eq!(buffer.cursor(), (0, 6));
    buffer.move_word_right();
    assert_eq!(buffer.cursor(), (0, 11));
}

#[test]
fn app_save_target_and_prompt_still_work_after_redesign() {
    let path = temp_path("save");
    let mut app = EditorApp::with_theme(ThemeId::Midnight);
    app.buffer.set_file_path(Some(path.clone()));
    type_text(&mut app, "Return 7.");
    let _ = app.handle_key(KeyEvent::Ctrl('s'));
    assert!(path.is_file());
    let frame = render_rows(&app, 100, 30).join("\n");
    assert!(frame.contains("SAVED"));
    let _ = fs::remove_file(path);
}
