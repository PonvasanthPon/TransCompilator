//! Synchronized ANSI renderer: rounded panes, pills, tabs, scrollbars,
//! semantic colors, and centered modal dialogs.

use crate::app::{EditorApp, Mode, PromptKind};
use crate::highlight::{LineState, StyledSpan, TokenStyle, highlight_line};
use crate::runner::{OutputTab, PRESETS};
use crate::terminal::{SYNC_BEGIN, SYNC_END};
use crate::theme::{Style, Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cell {
    character: char,
    style: Style,
}

#[derive(Debug, Clone, Copy)]
struct Rect {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

impl Rect {
    fn right(self) -> usize {
        self.x + self.width.saturating_sub(1)
    }

    fn bottom(self) -> usize {
        self.y + self.height.saturating_sub(1)
    }
}

struct Canvas {
    width: usize,
    height: usize,
    cells: Vec<Cell>,
}

impl Canvas {
    fn new(width: usize, height: usize, style: Style) -> Self {
        Self {
            width,
            height,
            cells: vec![
                Cell {
                    character: ' ',
                    style
                };
                width * height
            ],
        }
    }

    fn put(&mut self, x: usize, y: usize, character: char, style: Style) {
        if x < self.width && y < self.height {
            self.cells[y * self.width + x] = Cell { character, style };
        }
    }

    fn fill(&mut self, rect: Rect, style: Style) {
        for y in rect.y..rect.y + rect.height {
            for x in rect.x..rect.x + rect.width {
                self.put(x, y, ' ', style);
            }
        }
    }

    fn put_text(&mut self, x: usize, y: usize, text: &str, style: Style) -> usize {
        let mut column = x;
        for character in text.chars() {
            if column >= self.width {
                break;
            }
            let character = if character == '\t' {
                for _ in 0..4 {
                    self.put(column, y, ' ', style);
                    column += 1;
                }
                continue;
            } else if character.is_control() {
                ' '
            } else {
                character
            };
            self.put(column, y, character, style);
            column += 1;
        }
        column
    }

    fn put_clipped(&mut self, x: usize, y: usize, width: usize, text: &str, style: Style) -> usize {
        if width == 0 {
            return x;
        }
        let characters = sanitize(text).chars().collect::<Vec<_>>();
        if characters.len() <= width {
            self.put_text(x, y, &characters.iter().collect::<String>(), style);
            return x + characters.len();
        }
        let mut rendered = characters[..width.saturating_sub(1)]
            .iter()
            .collect::<String>();
        rendered.push('…');
        self.put_text(x, y, &rendered, style)
    }

    fn render(&self, theme: &Theme, cursor: Option<(usize, usize)>) -> String {
        let mut frame = String::new();
        frame.push_str(SYNC_BEGIN);
        frame.push_str("\x1b[2J\x1b[H");
        for row in 0..self.height {
            let _ = std::fmt::Write::write_fmt(&mut frame, format_args!("\x1b[{};1H", row + 1));
            let mut column = 0;
            while column < self.width {
                let style = self.cells[row * self.width + column].style;
                let mut text = String::new();
                while column < self.width && self.cells[row * self.width + column].style == style {
                    text.push(self.cells[row * self.width + column].character);
                    column += 1;
                }
                frame.push_str(&theme.paint(style, &text));
            }
        }
        match cursor {
            Some((row, column)) => {
                let _ = std::fmt::Write::write_fmt(
                    &mut frame,
                    format_args!(
                        "\x1b[{};{}H\x1b[?25h",
                        row.min(self.height.saturating_sub(1)) + 1,
                        column.min(self.width.saturating_sub(1)) + 1
                    ),
                );
            }
            None => frame.push_str("\x1b[?25l"),
        }
        frame.push_str(SYNC_END);
        frame
    }
}

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

fn span(text: impl Into<String>, kind: TokenStyle, style: Style) -> StyledSpan {
    StyledSpan {
        text: text.into(),
        kind,
        style,
    }
}

fn normal_line(text: &str, theme: &Theme) -> Vec<StyledSpan> {
    vec![span(text, TokenStyle::Identifier, theme.body())]
}

fn styled_chars(spans: &[StyledSpan]) -> Vec<(char, Style)> {
    let mut characters = Vec::new();
    for item in spans {
        for character in item.text.chars() {
            if character == '\t' {
                for _ in 0..4 {
                    characters.push((' ', item.style));
                }
            } else if character.is_control() {
                characters.push((' ', item.style));
            } else {
                characters.push((character, item.style));
            }
        }
    }
    characters
}

fn draw_spans(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    spans: &[StyledSpan],
    ellipsis_style: Style,
) {
    let characters = styled_chars(spans);
    if characters.len() > width && width > 0 {
        for (offset, (character, style)) in characters.iter().take(width - 1).enumerate() {
            canvas.put(x + offset, y, *character, *style);
        }
        canvas.put(x + width - 1, y, '…', ellipsis_style);
        return;
    }
    for (offset, (character, style)) in characters.iter().enumerate().take(width) {
        canvas.put(x + offset, y, *character, *style);
    }
}

fn focused_border(app: &EditorApp) -> Style {
    Style::fg(app.theme.focus_border)
}

fn unfocused_border(app: &EditorApp) -> Style {
    Style::fg(app.theme.unfocused_border)
}

fn draw_box(canvas: &mut Canvas, rect: Rect, app: &EditorApp, focused: bool, title: Option<&str>) {
    if rect.width < 3 || rect.height < 3 {
        return;
    }
    let border = if focused {
        focused_border(app)
    } else {
        unfocused_border(app)
    };
    canvas.fill(rect, Style::on(app.theme.text, app.theme.panel_bg));
    for x in rect.x + 1..rect.right() {
        canvas.put(x, rect.y, '─', border);
        canvas.put(x, rect.bottom(), '─', border);
    }
    for y in rect.y + 1..rect.bottom() {
        canvas.put(rect.x, y, '│', border);
        canvas.put(rect.right(), y, '│', border);
    }
    canvas.put(rect.x, rect.y, '╭', border);
    canvas.put(rect.right(), rect.y, '╮', border);
    canvas.put(rect.x, rect.bottom(), '╰', border);
    canvas.put(rect.right(), rect.bottom(), '╯', border);
    if let Some(title) = title {
        let label = format!(" {title} ");
        let style = if focused {
            Style::on(app.theme.canvas_bg, app.theme.focus_border).bold()
        } else {
            Style::on(app.theme.text, app.theme.panel_bg).bold()
        };
        canvas.put_clipped(
            rect.x + 2,
            rect.y,
            rect.width.saturating_sub(4),
            &label,
            style,
        );
    }
}

fn draw_scrollbar(
    canvas: &mut Canvas,
    rect: Rect,
    app: &EditorApp,
    total: usize,
    view: usize,
    offset: usize,
    focused: bool,
) {
    let track = rect.height.saturating_sub(2);
    if total <= view || track == 0 {
        return;
    }
    let thumb = (track * view / total).max(1).min(track);
    let maximum = total.saturating_sub(view);
    let start = (offset.min(maximum) * (track - thumb))
        .checked_div(maximum)
        .unwrap_or(0);
    let style = if focused {
        focused_border(app)
    } else {
        unfocused_border(app)
    };
    for row in 0..track {
        let character = if row >= start && row < start + thumb {
            '┃'
        } else {
            '│'
        };
        canvas.put(rect.right(), rect.y + 1 + row, character, style);
    }
}

fn header(canvas: &mut Canvas, app: &EditorApp) {
    let theme = &app.theme;
    let style = Style::on(theme.text, theme.header_bg);
    canvas.fill(
        Rect {
            x: 0,
            y: 0,
            width: canvas.width,
            height: 1,
        },
        style,
    );
    let mut column = canvas.put_text(0, 0, " ◆ R1 Editor ", theme.pill());
    let file = app.file_label();
    let file_label = if app.buffer.file_path().is_some() {
        file
    } else {
        format!("[{file}]")
    };
    column = canvas.put_text(column + 1, 0, &file_label, Style::fg(theme.text).bold());
    let dirty = if app.buffer.is_dirty() {
        " ●"
    } else {
        " ✓"
    };
    let dirty_style = if app.buffer.is_dirty() {
        Style::fg(theme.warning)
    } else {
        Style::fg(theme.success)
    };
    column = canvas.put_text(column, 0, dirty, dirty_style);
    let (row, column_number) = app.buffer.cursor();
    let metadata = format!(
        "Ln {}, Col {} │ {} lines │ .eng │ {} ",
        row + 1,
        column_number + 1,
        app.buffer.lines().len(),
        theme.id.name()
    );
    let metadata_width = metadata.chars().count();
    if column + 2 < canvas.width {
        let start = canvas.width.saturating_sub(metadata_width);
        if start > column + 1 {
            canvas.put_clipped(start, 0, metadata_width, &metadata, Style::fg(theme.muted));
        }
    }
}

fn editor_interior(rect: Rect) -> (usize, usize, usize, usize) {
    let glyph = rect.x + 1;
    let numbers = glyph + 1;
    let bar = numbers + 5;
    let content = bar + 2;
    let width = rect.right().saturating_sub(content);
    (glyph, numbers, content, width)
}

fn draw_editor(canvas: &mut Canvas, app: &EditorApp, rect: Rect) -> Option<(usize, usize)> {
    let focused = app.mode != Mode::OutputFocus;
    let border_title = format!("Editor: {}", app.file_label());
    draw_box(canvas, rect, app, focused, Some(&border_title));
    let rows = rect.height.saturating_sub(2);
    if rows == 0 {
        return None;
    }
    let (cursor_row, cursor_col) = app.buffer.cursor();
    let (scroll_row, scroll_col) = app.buffer.scroll();
    let mut first_row = scroll_row;
    if cursor_row < first_row {
        first_row = cursor_row;
    } else if cursor_row >= first_row + rows {
        first_row = cursor_row + 1 - rows;
    }
    let (glyph_x, number_x, content_x, content_width) = editor_interior(rect);
    let mut first_col = scroll_col;
    if cursor_col < first_col {
        first_col = cursor_col;
    } else if content_width > 0 && cursor_col >= first_col + content_width {
        first_col = cursor_col + 1 - content_width;
    }
    for offset in 0..rows {
        let line_index = first_row + offset;
        let y = rect.y + 1 + offset;
        if line_index >= app.buffer.lines().len() {
            canvas.fill(
                Rect {
                    x: rect.x + 1,
                    y,
                    width: rect.width.saturating_sub(2),
                    height: 1,
                },
                Style::on(app.theme.text, app.theme.panel_bg),
            );
            continue;
        }
        let state = app.line_state(line_index + 1);
        let active = line_index == cursor_row && focused;
        let row_background = if state == LineState::Error {
            app.theme.error_bg
        } else if active {
            app.theme.active_line_bg
        } else {
            app.theme.panel_bg
        };
        canvas.fill(
            Rect {
                x: rect.x + 1,
                y,
                width: rect.width.saturating_sub(2),
                height: 1,
            },
            Style::on(app.theme.text, row_background),
        );
        let glyph = match state {
            LineState::Error => ('✗', Style::fg(app.theme.error).bold()),
            LineState::ProseOmitted => ('·', Style::fg(app.theme.muted)),
            LineState::Normal if active => ('▸', Style::fg(app.theme.active_line_number)),
            LineState::Normal => (' ', Style::fg(app.theme.muted)),
        };
        canvas.put(glyph_x, y, glyph.0, glyph.1);
        let number_style = if active {
            Style::fg(app.theme.active_line_number).bold()
        } else {
            Style::fg(app.theme.muted)
        };
        canvas.put_text(number_x, y, &format!("{:>4}", line_index + 1), number_style);
        canvas.put(
            number_x + 5,
            y,
            '│',
            if focused {
                Style::fg(app.theme.unfocused_border)
            } else {
                Style::fg(app.theme.muted)
            },
        );
        canvas.put(number_x + 6, y, ' ', Style::fg(app.theme.text));
        if content_width > 0 {
            let spans = highlight_line(
                &app.buffer.lines()[line_index],
                state,
                app.search_query.as_deref(),
                &app.theme,
            );
            let characters = styled_chars(&spans);
            let visible = if characters.len() > first_col {
                &characters[first_col..]
            } else {
                &[]
            };
            if visible.len() > content_width {
                for (offset, (character, style)) in
                    visible.iter().take(content_width - 1).enumerate()
                {
                    canvas.put(content_x + offset, y, *character, *style);
                }
                canvas.put(
                    content_x + content_width - 1,
                    y,
                    '…',
                    Style::fg(app.theme.muted),
                );
            } else {
                for (offset, (character, style)) in visible.iter().enumerate() {
                    canvas.put(content_x + offset, y, *character, *style);
                }
            }
            if active && cursor_col >= first_col {
                let cursor_x = content_x + (cursor_col - first_col);
                if cursor_x < rect.right() {
                    let style = canvas.cells[y * canvas.width + cursor_x].style.reverse();
                    canvas.put(cursor_x, y, ' ', style);
                }
            }
        }
    }
    draw_scrollbar(
        canvas,
        rect,
        app,
        app.buffer.lines().len(),
        rows,
        first_row,
        focused,
    );
    Some((
        1 + cursor_row.saturating_sub(first_row),
        content_x + cursor_col.saturating_sub(first_col),
    ))
}

fn tab_segments(app: &EditorApp) -> Vec<(&'static str, &'static str, bool)> {
    vec![
        ("F5", "Run", app.tab == OutputTab::Run),
        ("F6", "Paraphrase", app.tab == OutputTab::Paraphrase),
        ("F7", "Trace", app.tab == OutputTab::Trace),
        ("F8", "IR", app.tab == OutputTab::Ir),
        ("F9", "Build", app.tab == OutputTab::Build),
    ]
}

fn draw_output(canvas: &mut Canvas, app: &EditorApp, rect: Rect) -> Option<(usize, usize)> {
    let focused = app.mode == Mode::OutputFocus;
    draw_box(canvas, rect, app, focused, None);
    let mut x = rect.x + 2;
    for (key, label, active) in tab_segments(app) {
        let text = format!("[{key} {label}]");
        let style = if active {
            Style::on(app.theme.canvas_bg, app.theme.focus_border).bold()
        } else {
            Style::fg(app.theme.muted)
        };
        if x + text.chars().count() + 1 >= rect.right() {
            break;
        }
        x = canvas.put_text(x, rect.y, &text, style) + 1;
    }
    let content = output_content(app);
    let rows = rect.height.saturating_sub(2);
    let mut output_row = None;
    for offset in 0..rows {
        let index = app.output_scroll + offset;
        let y = rect.y + 1 + offset;
        if index == 0 {
            output_row = Some((y, rect.x + 2));
        }
        if let Some(line) = content.get(index) {
            draw_spans(
                canvas,
                rect.x + 2,
                y,
                rect.width.saturating_sub(4),
                line,
                Style::fg(app.theme.muted),
            );
        }
    }
    draw_scrollbar(
        canvas,
        rect,
        app,
        content.len(),
        rows,
        app.output_scroll,
        focused,
    );
    output_row.map(|(row, _)| (row - rect.y, rect.x + 2))
}

#[allow(clippy::too_many_lines)]
fn output_content(app: &EditorApp) -> Vec<Vec<StyledSpan>> {
    match app.tab {
        OutputTab::Run => run_content(app),
        OutputTab::Paraphrase => paraphrase_content(app),
        OutputTab::Trace => trace_content(app),
        OutputTab::Ir => ir_content(app),
        OutputTab::Build => build_content(app),
    }
}

fn run_content(app: &EditorApp) -> Vec<Vec<StyledSpan>> {
    let theme = &app.theme;
    let mut lines = Vec::new();
    let (label, style) = if app.output.ok {
        (" ✓ PASS ", theme.success_pill())
    } else {
        (" ✗ FAIL ", theme.error_pill())
    };
    let mut banner = vec![
        span("RUN ", TokenStyle::Muted, Style::fg(theme.muted).bold()),
        span(label, TokenStyle::Identifier, style),
    ];
    if let Some((before, after)) = app.output.summary.split_once("PARITY:") {
        banner.push(span(
            format!(" {before}PARITY:"),
            TokenStyle::Identifier,
            theme.body(),
        ));
        banner.push(span(
            after.to_string(),
            TokenStyle::ActionVerb,
            Style::fg(if app.output.ok {
                theme.success
            } else {
                theme.error
            })
            .bold(),
        ));
    } else {
        banner.push(span(
            format!(" {}", app.output.summary),
            TokenStyle::Identifier,
            theme.body(),
        ));
    }
    lines.push(banner);
    if !app.output.ok && !app.output.lines.is_empty() {
        lines.push(vec![span(
            "── DIAGNOSTICS ──",
            TokenStyle::ControlAnchor,
            Style::fg(theme.control).bold(),
        )]);
    }
    for line in &app.output.lines {
        if line.starts_with("---") && line.ends_with("---") {
            let name = line.trim_matches('-').trim().to_ascii_uppercase();
            lines.push(vec![span(
                format!("── {name} ──"),
                TokenStyle::ControlAnchor,
                Style::fg(theme.control).bold(),
            )]);
        } else if line.starts_with('[')
            || line.contains("UNRESOLVED")
            || line.contains("TypeCheck")
            || line.contains("FAILED")
        {
            lines.push(vec![span(
                format!("✗ {line}"),
                TokenStyle::Omitted,
                Style::fg(theme.error).bold(),
            )]);
        } else {
            lines.push(normal_line(line, theme));
        }
    }
    lines
}

fn parse_paraphrase_line(line: &str, app: &EditorApp) -> Vec<StyledSpan> {
    let theme = &app.theme;
    if line.starts_with("[OMITTED") {
        return vec![span(
            line,
            TokenStyle::Omitted,
            Style::fg(theme.omitted).dim().italic(),
        )];
    }
    if line.starts_with("[UNSUPPORTED") {
        return vec![span(
            line,
            TokenStyle::Omitted,
            Style::fg(theme.error).bold(),
        )];
    }
    if line.starts_with("===") {
        return vec![span(
            line,
            TokenStyle::ControlAnchor,
            Style::fg(theme.control).bold(),
        )];
    }
    let characters = line.chars().collect::<Vec<_>>();
    let mut spans = Vec::new();
    let mut index = 0;
    let mut plain = String::new();
    let flush = |plain: &mut String, spans: &mut Vec<StyledSpan>| {
        if !plain.is_empty() {
            spans.push(span(
                std::mem::take(plain),
                TokenStyle::Identifier,
                theme.body(),
            ));
        }
    };
    while index < characters.len() {
        let character = characters[index];
        if character == '[' {
            flush(&mut plain, &mut spans);
            let mut end = index + 1;
            while end < characters.len() && characters[end] != ']' {
                end += 1;
            }
            if end >= characters.len() {
                plain.push('[');
                index += 1;
                continue;
            }
            let text = characters[index..=end].iter().collect::<String>();
            let inner = text.trim_start_matches('[').trim_end_matches(']');
            let (kind, style) = if inner.starts_with("OMITTED") {
                (TokenStyle::Omitted, Style::fg(theme.omitted).dim().italic())
            } else if inner.starts_with("UNSUPPORTED") {
                (TokenStyle::Omitted, Style::fg(theme.error).bold())
            } else if inner.starts_with("it ")
                || inner.starts_with("its ")
                || inner.starts_with("the ")
            {
                (
                    TokenStyle::AnaphorOrQuantifier,
                    Style::fg(theme.anaphor).bold(),
                )
            } else if inner.starts_with("phrase:") {
                (TokenStyle::ControlAnchor, Style::fg(theme.control))
            } else if inner.starts_with('L') {
                (TokenStyle::Muted, Style::fg(theme.muted))
            } else {
                (TokenStyle::Identifier, theme.body())
            };
            spans.push(span(text, kind, style));
            index = end + 1;
            continue;
        }
        if character == '{' {
            flush(&mut plain, &mut spans);
            let mut end = index + 1;
            while end < characters.len() && characters[end] != '}' {
                end += 1;
            }
            if end >= characters.len() {
                plain.push('{');
                index += 1;
                continue;
            }
            let text = characters[index..=end].iter().collect::<String>();
            spans.push(span(
                text,
                TokenStyle::Type,
                Style::fg(theme.warning).bold(),
            ));
            index = end + 1;
            continue;
        }
        plain.push(character);
        index += 1;
    }
    flush(&mut plain, &mut spans);
    spans
}

fn paraphrase_content(app: &EditorApp) -> Vec<Vec<StyledSpan>> {
    let mut lines = vec![vec![span(
        "PARAPHRASE",
        TokenStyle::ControlAnchor,
        Style::fg(app.theme.control).bold(),
    )]];
    for line in &app.output.lines {
        lines.push(parse_paraphrase_line(line, app));
    }
    lines
}

fn trace_content(app: &EditorApp) -> Vec<Vec<StyledSpan>> {
    let theme = &app.theme;
    let mut lines = vec![vec![span(
        "TRACE",
        TokenStyle::ControlAnchor,
        Style::fg(theme.control).bold(),
    )]];
    for line in &app.output.lines {
        if line.starts_with("  ") {
            lines.push(vec![span(
                line,
                TokenStyle::Identifier,
                Style::fg(theme.text),
            )]);
            continue;
        }
        let mut spans = Vec::new();
        for piece in line.split_inclusive(char::is_whitespace) {
            let trimmed = piece.trim();
            let (kind, style) = match trimmed {
                "EXEC" => (TokenStyle::ActionVerb, Style::fg(theme.success).bold()),
                "COMMENT_OMITTED" => (TokenStyle::Omitted, Style::fg(theme.omitted).dim().italic()),
                "UNSUPPORTED" => (TokenStyle::Omitted, Style::fg(theme.error).bold()),
                value if value.starts_with("verbnet=") || value.starts_with("wordnet=") => {
                    (TokenStyle::AnaphorOrQuantifier, Style::fg(theme.anaphor))
                }
                _ => (TokenStyle::Identifier, theme.body()),
            };
            spans.push(span(piece, kind, style));
        }
        lines.push(spans);
    }
    lines
}

fn ir_content(app: &EditorApp) -> Vec<Vec<StyledSpan>> {
    let mut lines = vec![vec![span(
        "TC-IR",
        TokenStyle::ControlAnchor,
        Style::fg(app.theme.control).bold(),
    )]];
    for line in &app.output.lines {
        lines.push(highlight_line(line, LineState::Normal, None, &app.theme));
    }
    lines
}

fn build_content(app: &EditorApp) -> Vec<Vec<StyledSpan>> {
    let theme = &app.theme;
    let mut lines = Vec::new();
    let (label, style) = if app.output.ok {
        (" ✓ BUILT ", theme.success_pill())
    } else {
        (" ✗ FAIL ", theme.error_pill())
    };
    lines.push(vec![
        span("BUILD ", TokenStyle::Muted, Style::fg(theme.muted).bold()),
        span(label, TokenStyle::Identifier, style),
        span(
            format!(" {}", app.output.summary),
            TokenStyle::Identifier,
            theme.body(),
        ),
    ]);
    if app.output.ok {
        lines.push(vec![span(
            " STATIC ZERO-LIBC ELF64 ",
            TokenStyle::ActionVerb,
            theme.success_pill(),
        )]);
    }
    for line in &app.output.lines {
        if line.contains("STATIC ELF64: yes") || line.contains("no dynamic section") {
            lines.push(vec![span(
                line,
                TokenStyle::ActionVerb,
                Style::fg(theme.success),
            )]);
        } else {
            lines.push(normal_line(line, theme));
        }
    }
    lines
}

fn status_bar(canvas: &mut Canvas, app: &EditorApp) {
    let y = canvas.height.saturating_sub(2);
    canvas.fill(
        Rect {
            x: 0,
            y,
            width: canvas.width,
            height: 1,
        },
        Style::on(app.theme.text, app.theme.header_bg),
    );
    let (label, style) = if app.mode == Mode::Prompt {
        let label = match app.prompt_kind {
            Some(PromptKind::SaveAs) => " SAVE (.eng) ▸ ",
            Some(PromptKind::Open) => " OPEN (.eng) ▸ ",
            Some(PromptKind::Find) => " FIND ▸ ",
            Some(PromptKind::Stdin) => " STDIN ▸ ",
            None => " PROMPT ▸ ",
        };
        (label, app.theme.pill())
    } else if !app.output.ok {
        (" ERROR ", app.theme.error_pill())
    } else if app.status.starts_with("Saved") {
        (" SAVED ", app.theme.success_pill())
    } else if app.status.starts_with("Theme") {
        (" THEME ", app.theme.warning_pill())
    } else if app.tab == OutputTab::Run && app.status.contains("ELF64 PASS") {
        (" PASS ", app.theme.success_pill())
    } else {
        (" READY ", app.theme.pill())
    };
    let mut x = canvas.put_text(0, y, label, style);
    let message = if app.mode == Mode::Prompt {
        app.prompt.clone()
    } else if app.mode == Mode::OutputFocus {
        format!("{}  [OUTPUT FOCUS — ^E to edit]", app.status)
    } else {
        app.status.clone()
    };
    x = canvas.put_text(x + 1, y, &message, Style::fg(app.theme.text));
    let _ = x;
}

fn legend(canvas: &mut Canvas, app: &EditorApp) {
    let y = canvas.height.saturating_sub(1);
    canvas.fill(
        Rect {
            x: 0,
            y,
            width: canvas.width,
            height: 1,
        },
        Style::on(app.theme.muted, app.theme.header_bg),
    );
    let text = "^S Save  ^O Open  ^N New  F2 Presets  F5/^R Run  F6/^P Paraphrase  F7/^T Trace  F8/^I IR  F9/^B Build  F4 Theme  F1 Help  ^Q Quit";
    canvas.put_clipped(
        0,
        y,
        canvas.width,
        text,
        Style::on(app.theme.text, app.theme.header_bg),
    );
}

fn modal_rect(canvas: &Canvas, width: usize, height: usize) -> Rect {
    let width = width.min(canvas.width.saturating_sub(4)).max(20);
    let height = height.min(canvas.height.saturating_sub(4)).max(5);
    Rect {
        x: (canvas.width.saturating_sub(width)) / 2,
        y: (canvas.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}

fn draw_preset_modal(canvas: &mut Canvas, app: &EditorApp) {
    let rect = modal_rect(canvas, 78, 10);
    draw_box(canvas, rect, app, true, Some("F2 · Preset Picker"));
    for (index, preset) in PRESETS.iter().enumerate() {
        let y = rect.y + 1 + index;
        if y >= rect.bottom() {
            break;
        }
        let text = format!(" {} {} — {}", index + 1, preset.name, preset.description);
        let style = if index == app.preset_selection {
            Style::on(app.theme.canvas_bg, app.theme.focus_border).bold()
        } else {
            app.theme.body()
        };
        canvas.fill(
            Rect {
                x: rect.x + 1,
                y,
                width: rect.width.saturating_sub(2),
                height: 1,
            },
            style,
        );
        canvas.put_clipped(rect.x + 2, y, rect.width.saturating_sub(4), &text, style);
    }
    let hint = "↑/↓ select · Enter load · 1-5 direct · Esc cancel";
    canvas.put_clipped(
        rect.x + 2,
        rect.bottom().saturating_sub(1),
        rect.width.saturating_sub(4),
        hint,
        Style::fg(app.theme.muted),
    );
}

fn draw_help_modal(canvas: &mut Canvas, app: &EditorApp) {
    let rect = modal_rect(canvas, 82, 14);
    draw_box(canvas, rect, app, true, Some("F1 · R1 EDITOR HELP"));
    let left = [
        "FILE",
        "  ^S  Save",
        "  ^O  Open",
        "  ^N  New",
        "  ^Q  Quit",
        "EDIT",
        "  ^Z  Undo",
        "  ^Y  Redo",
        "  ^F  Find",
    ];
    let right = [
        "RUN & INSPECT",
        "  F5  Run + parity",
        "  F6  Paraphrase",
        "  F7  Trace",
        "  F8  Core-7 IR",
        "  F9  Build .elf",
        "VIEW / THEME",
        "  F4  Cycle theme",
        "  ^E  Pane focus",
    ];
    let column_width = rect.width.saturating_sub(4) / 2;
    for (offset, line) in left.iter().enumerate() {
        let style = if line.starts_with(char::is_uppercase) {
            Style::fg(app.theme.focus_border).bold()
        } else {
            app.theme.body()
        };
        canvas.put_clipped(rect.x + 2, rect.y + 1 + offset, column_width, line, style);
    }
    for (offset, line) in right.iter().enumerate() {
        let style = if line.starts_with(char::is_uppercase) {
            Style::fg(app.theme.focus_border).bold()
        } else {
            app.theme.body()
        };
        canvas.put_clipped(
            rect.x + 2 + column_width,
            rect.y + 1 + offset,
            column_width,
            line,
            style,
        );
    }
    canvas.put_clipped(
        rect.x + 2,
        rect.bottom().saturating_sub(1),
        rect.width.saturating_sub(4),
        "Esc or F1 closes help",
        Style::fg(app.theme.muted),
    );
}

/// Renders one complete synchronized ANSI frame.
#[must_use]
pub fn render(app: &EditorApp, width: usize, height: usize) -> String {
    let (canvas, cursor) = build_frame(app, width, height);
    canvas.render(&app.theme, cursor)
}

/// Returns the visible character grid, useful for headless alignment tests.
#[must_use]
pub fn render_rows(app: &EditorApp, width: usize, height: usize) -> Vec<String> {
    let (canvas, _) = build_frame(app, width, height);
    (0..canvas.height)
        .map(|row| {
            (0..canvas.width)
                .map(|column| canvas.cells[row * canvas.width + column].character)
                .collect()
        })
        .collect()
}

#[allow(clippy::too_many_lines)]
fn build_frame(app: &EditorApp, width: usize, height: usize) -> (Canvas, Option<(usize, usize)>) {
    let width = width.max(20);
    let height = height.max(6);
    let mut canvas = Canvas::new(
        width,
        height,
        Style::on(app.theme.text, app.theme.canvas_bg),
    );
    header(&mut canvas, app);
    let main_y = 1;
    let main_height = height.saturating_sub(3);
    let mut editor_cursor = None;
    if width >= 100 {
        let editor_width = (width * 56 / 100).max(20);
        let output_x = editor_width + 1;
        let output_width = width.saturating_sub(output_x).max(20);
        let editor_rect = Rect {
            x: 0,
            y: main_y,
            width: editor_width.min(width),
            height: main_height,
        };
        let output_rect = Rect {
            x: output_x,
            y: main_y,
            width: output_width,
            height: main_height,
        };
        let relative = draw_editor(&mut canvas, app, editor_rect);
        if let Some((row, column)) = relative {
            editor_cursor = Some((main_y + row, column));
        }
        let _ = draw_output(&mut canvas, app, output_rect);
    } else {
        let editor_height = (main_height * 58 / 100).max(3);
        let output_y = main_y + editor_height + 1;
        let output_height = main_height.saturating_sub(editor_height + 1).max(3);
        let editor_rect = Rect {
            x: 0,
            y: main_y,
            width,
            height: editor_height.min(main_height),
        };
        let output_rect = Rect {
            x: 0,
            y: output_y,
            width,
            height: output_height,
        };
        let relative = draw_editor(&mut canvas, app, editor_rect);
        if let Some((row, column)) = relative {
            editor_cursor = Some((main_y + row, column));
        }
        let _ = draw_output(&mut canvas, app, output_rect);
    }
    match app.mode {
        Mode::PresetPicker => draw_preset_modal(&mut canvas, app),
        Mode::Help => draw_help_modal(&mut canvas, app),
        _ => {}
    }
    status_bar(&mut canvas, app);
    legend(&mut canvas, app);
    let cursor = if app.mode == Mode::Prompt {
        let label_width = app
            .prompt_kind
            .map_or(0, |kind| kind.label().chars().count() + 2);
        Some((
            height.saturating_sub(2),
            1 + label_width + app.prompt.chars().count(),
        ))
    } else if matches!(
        app.mode,
        Mode::PresetPicker | Mode::Help | Mode::OutputFocus
    ) {
        None
    } else {
        editor_cursor
    };
    (canvas, cursor)
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
