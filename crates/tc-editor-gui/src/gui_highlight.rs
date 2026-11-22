//! Converts `tc_editor` semantic highlighting into egui layout jobs.

use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{Color32, FontId};
use tc_editor::highlight::{LineState, TokenStyle, highlight_line};
use tc_editor::{Theme, ThemeId};

use crate::gui_theme::GuiTheme;

/// Returns the semantic line state for a one-based line number.
#[must_use]
pub fn line_state(line: usize, error_lines: &[usize], omitted_lines: &[usize]) -> LineState {
    if error_lines.contains(&line) {
        LineState::Error
    } else if omitted_lines.contains(&line) {
        LineState::ProseOmitted
    } else {
        LineState::Normal
    }
}

fn syntax_color(theme: &GuiTheme, kind: TokenStyle) -> Color32 {
    match kind {
        TokenStyle::ControlAnchor => theme.syntax_control,
        TokenStyle::ActionVerb => theme.syntax_verb,
        TokenStyle::AnaphorOrQuantifier => theme.syntax_anaphor,
        TokenStyle::Type => theme.syntax_type,
        TokenStyle::Number => theme.syntax_number,
        TokenStyle::String => theme.syntax_string,
        TokenStyle::Operator => theme.syntax_operator,
        TokenStyle::Identifier => theme.ink,
        TokenStyle::Muted => theme.muted,
        TokenStyle::Omitted => theme.syntax_omitted,
        TokenStyle::SearchMatch => theme.warning_fg,
    }
}

/// Formats one semantic span with the GUI palette.
#[must_use]
pub fn span_format(theme: &GuiTheme, kind: TokenStyle, font_size: f32) -> TextFormat {
    let omitted = matches!(kind, TokenStyle::Omitted);
    TextFormat {
        font_id: FontId::monospace(font_size),
        color: syntax_color(theme, kind),
        background: if matches!(kind, TokenStyle::SearchMatch) {
            theme.warning_bg
        } else {
            Color32::TRANSPARENT
        },
        italics: omitted,
        ..TextFormat::default()
    }
}

/// Builds a complete multi-line syntax-highlighted layout job.
#[must_use]
pub fn highlight_job(
    text: &str,
    error_lines: &[usize],
    omitted_lines: &[usize],
    theme: &GuiTheme,
    font_size: f32,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    let terminal_theme = Theme::from_id(if theme.mode == crate::gui_theme::ThemeMode::Light {
        ThemeId::Midnight
    } else {
        ThemeId::Monochrome
    });
    for (index, raw) in text.split_inclusive('\n').enumerate() {
        let number = index + 1;
        let (line, newline) = match raw.strip_suffix('\n') {
            Some(line) => (line, true),
            None => (raw, false),
        };
        let state = line_state(number, error_lines, omitted_lines);
        let spans = highlight_line(line, state, None, &terminal_theme);
        if spans.is_empty() {
            let mut format = span_format(theme, TokenStyle::Identifier, font_size);
            if state == LineState::Error {
                format.color = theme.error_line;
                format.background = theme.error_bg;
            }
            job.append("", 0.0, format);
        } else {
            for span in spans {
                let mut format = span_format(theme, span.kind, font_size);
                if state == LineState::Error {
                    format.color = theme.error_line;
                    format.background = theme.error_bg;
                }
                job.append(&span.text, 0.0, format);
            }
        }
        if newline {
            let mut format = span_format(theme, TokenStyle::Identifier, font_size);
            if state == LineState::Error {
                format.color = theme.error_line;
                format.background = theme.error_bg;
            }
            job.append("\n", 0.0, format);
        }
    }
    job
}
