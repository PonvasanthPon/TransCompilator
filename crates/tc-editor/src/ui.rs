//! ANSI split-pane renderer. Rendering is pure and testable without a TTY.

use crate::app::{EditorApp, Mode};
use crate::runner::{OutputTab, PRESETS};

const RESET: &str = "\x1b[0m";
const REVERSE: &str = "\x1b[7m";
const CURSOR: &str = "\x1b[?25h";

fn sanitize(text: &str) -> String {
    let mut output = String::new();
    for character in text.chars() {
        if character == '\t' {
            output.push_str("    ");
        } else if character.is_control() {
            output.push(' ');
        } else {
            output.push(character);
        }
    }
    output
}

fn truncate(text: &str, width: usize) -> String {
    text.chars().take(width).collect()
}

fn pad(text: &str, width: usize) -> String {
    let mut value = truncate(text, width);
    let length = value.chars().count();
    if length < width {
        value.push_str(&" ".repeat(width - length));
    }
    value
}

fn tab_bar(active: OutputTab) -> String {
    [
        OutputTab::Run,
        OutputTab::Paraphrase,
        OutputTab::Trace,
        OutputTab::Ir,
        OutputTab::Build,
    ]
    .iter()
    .map(|tab| {
        if *tab == active {
            format!("[{}]", tab.label())
        } else {
            tab.label().to_string()
        }
    })
    .collect::<Vec<_>>()
    .join(" ")
}

fn output_content(app: &EditorApp) -> Vec<String> {
    match app.mode {
        Mode::PresetPicker => PRESETS
            .iter()
            .enumerate()
            .map(|(index, preset)| {
                format!(" {}  {:<32} {}", index + 1, preset.name, preset.description)
            })
            .collect(),
        Mode::Help => vec![
            "R1 Editor - Help".to_string(),
            String::new(),
            "Ctrl+S save   Ctrl+O open   Ctrl+N new   Ctrl+Q quit".to_string(),
            "Ctrl+Z undo   Ctrl+Y redo   Ctrl+F find  Ctrl+E pane".to_string(),
            "F1 help       F2 presets    F3 stdin".to_string(),
            "F5 run ELF64 + parity        F6 paraphrase".to_string(),
            "F7 trace      F8 Core-7 IR   F9 build .elf".to_string(),
            String::new(),
            "Presets are complete .eng programs; press F2 and 1-5.".to_string(),
        ],
        _ => {
            let mut lines = vec![app.output.summary.clone()];
            lines.extend(app.output.lines.iter().cloned());
            lines
        }
    }
}

fn editor_lines(app: &EditorApp, width: usize, height: usize) -> Vec<String> {
    let (cursor_row, cursor_col) = app.buffer.cursor();
    let (scroll_row, scroll_col) = app.buffer.scroll();
    let mut first_row = scroll_row;
    if cursor_row < first_row {
        first_row = cursor_row;
    } else if height > 0 && cursor_row >= first_row + height {
        first_row = cursor_row + 1 - height;
    }
    let mut first_col = scroll_col;
    let content_width = width.saturating_sub(7);
    if cursor_col < first_col {
        first_col = cursor_col;
    } else if content_width > 0 && cursor_col >= first_col + content_width {
        first_col = cursor_col + 1 - content_width;
    }
    let mut lines = Vec::with_capacity(height);
    for offset in 0..height {
        let row = first_row + offset;
        if row >= app.buffer.lines().len() {
            lines.push(pad("", width));
            continue;
        }
        let source = sanitize(&app.buffer.lines()[row]);
        let visible = source
            .chars()
            .skip(first_col)
            .take(content_width)
            .collect::<Vec<_>>();
        let mut content = String::new();
        for (index, character) in visible.iter().enumerate() {
            let absolute = first_col + index;
            if row == cursor_row && absolute == cursor_col {
                content.push_str(REVERSE);
                content.push(*character);
                content.push_str(RESET);
            } else {
                content.push(*character);
            }
        }
        if row == cursor_row && cursor_col >= first_col + visible.len() {
            content.push_str(REVERSE);
            content.push(' ');
            content.push_str(RESET);
        }
        let prefix = format!("{:>4} | ", row + 1);
        lines.push(pad(&format!("{prefix}{content}"), width));
    }
    lines
}

/// Renders one complete ANSI frame.
#[must_use]
pub fn render(app: &EditorApp, width: usize, height: usize) -> String {
    let width = width.max(20);
    let height = height.max(6);
    let mut frame = String::new();
    frame.push_str("\x1b[2J\x1b[H");

    let dirty = if app.buffer.is_dirty() { "*" } else { "" };
    let (row, column) = app.buffer.cursor();
    let header = format!(
        " R1 Editor  |  {}{}  |  Line {}, Col {}  |  Tab: {} ",
        app.file_label(),
        dirty,
        row + 1,
        column + 1,
        app.tab.label()
    );
    let _ = std::fmt::Write::write_fmt(
        &mut frame,
        format_args!("\x1b[1;1H\x1b[2K{}", pad(&header, width)),
    );

    let status_row = height - 1;
    let legend_row = height;
    let main_height = height.saturating_sub(3);
    let wide = width >= 100;

    let output_lines = output_content(app);
    let mut cursor_position;

    if wide {
        let editor_width = width * 55 / 100;
        let separator = editor_width + 1;
        let output_column = editor_width + 2;
        let output_width = width.saturating_sub(output_column - 1).max(1);
        let editor = editor_lines(app, editor_width, main_height);
        let mut output = vec![pad(&tab_bar(app.tab), output_width)];
        for line in output_lines
            .iter()
            .skip(app.output_scroll)
            .take(main_height.saturating_sub(1))
        {
            output.push(pad(&sanitize(line), output_width));
        }
        while output.len() < main_height {
            output.push(pad("", output_width));
        }
        for offset in 0..main_height {
            let screen_row = 2 + offset;
            let _ = std::fmt::Write::write_fmt(
                &mut frame,
                format_args!(
                    "\x1b[{screen_row};1H\x1b[2K{}\x1b[{screen_row};{separator}H|",
                    editor.get(offset).map_or("", String::as_str)
                ),
            );
            let _ = std::fmt::Write::write_fmt(
                &mut frame,
                format_args!(
                    "\x1b[{screen_row};{output_column}H{}",
                    output.get(offset).map_or("", String::as_str)
                ),
            );
        }
        let (scroll_row, scroll_col) = app.buffer.scroll();
        let mut first_row = scroll_row;
        if row < first_row {
            first_row = row;
        } else if main_height > 0 && row >= first_row + main_height {
            first_row = row + 1 - main_height;
        }
        let content_width = editor_width.saturating_sub(7);
        let mut first_col = scroll_col;
        if column < first_col {
            first_col = column;
        } else if content_width > 0 && column >= first_col + content_width {
            first_col = column + 1 - content_width;
        }
        cursor_position = (
            2 + row.saturating_sub(first_row),
            8 + column.saturating_sub(first_col),
        );
    } else {
        let editor_height = (main_height * 60 / 100).max(1);
        let output_height = main_height.saturating_sub(editor_height + 1);
        let editor = editor_lines(app, width, editor_height);
        for (offset, line) in editor.iter().enumerate() {
            let screen_row = 2 + offset;
            let _ = std::fmt::Write::write_fmt(
                &mut frame,
                format_args!("\x1b[{screen_row};1H\x1b[2K{line}"),
            );
        }
        let separator_row = 2 + editor_height;
        let _ = std::fmt::Write::write_fmt(
            &mut frame,
            format_args!("\x1b[{separator_row};1H\x1b[2K{}", "-".repeat(width)),
        );
        let mut output = vec![pad(&tab_bar(app.tab), width)];
        for line in output_lines
            .iter()
            .skip(app.output_scroll)
            .take(output_height.saturating_sub(1))
        {
            output.push(pad(&sanitize(line), width));
        }
        for (offset, line) in output.iter().enumerate() {
            let screen_row = separator_row + 1 + offset;
            if screen_row >= status_row {
                break;
            }
            let _ = std::fmt::Write::write_fmt(
                &mut frame,
                format_args!("\x1b[{screen_row};1H\x1b[2K{line}"),
            );
        }
        let (scroll_row, scroll_col) = app.buffer.scroll();
        let mut first_row = scroll_row;
        if row < first_row {
            first_row = row;
        } else if editor_height > 0 && row >= first_row + editor_height {
            first_row = row + 1 - editor_height;
        }
        let content_width = width.saturating_sub(7);
        let mut first_col = scroll_col;
        if column < first_col {
            first_col = column;
        } else if content_width > 0 && column >= first_col + content_width {
            first_col = column + 1 - content_width;
        }
        cursor_position = (
            2 + row.saturating_sub(first_row),
            8 + column.saturating_sub(first_col),
        );
    }

    let status = if app.mode == Mode::Prompt {
        let label = app.prompt_kind.map_or("", |kind| kind.label());
        format!("{label}{}", app.prompt)
    } else {
        app.status.clone()
    };
    let _ = std::fmt::Write::write_fmt(
        &mut frame,
        format_args!(
            "\x1b[{status_row};1H\x1b[2K{}",
            pad(&sanitize(&status), width)
        ),
    );
    let legend = "^S Save | ^O Open | ^N New | F2 Presets | F5/^R Run | F6/^P Paraphrase | F7/^T Trace | F8/^I IR | F9/^B Build | ^E Pane | ^Q Quit";
    let _ = std::fmt::Write::write_fmt(
        &mut frame,
        format_args!("\x1b[{legend_row};1H\x1b[2K{}", pad(legend, width)),
    );

    if app.mode == Mode::Prompt {
        let label_width = app
            .prompt_kind
            .map_or(0, |kind| kind.label().chars().count());
        cursor_position = (status_row, label_width + app.prompt.chars().count() + 1);
    }
    let cursor_row = cursor_position.0.clamp(1, height);
    let cursor_column = cursor_position.1.clamp(1, width);
    let _ = std::fmt::Write::write_fmt(
        &mut frame,
        format_args!("\x1b[{cursor_row};{cursor_column}H{CURSOR}"),
    );
    frame
}

#[cfg(test)]
mod tests {
    use super::render;
    use crate::app::EditorApp;

    #[test]
    fn renderer_always_includes_header_and_legend() {
        let app = EditorApp::new();
        let frame = render(&app, 120, 35);
        assert!(frame.contains("R1 Editor"));
        assert!(frame.contains("F5/^R Run"));
    }
}
