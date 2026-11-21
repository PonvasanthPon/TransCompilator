//! UTF-8-safe editable text buffer with undo/redo, search, and `.eng` I/O.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const MAX_UNDO: usize = 200;

#[derive(Debug, Clone)]
struct Snapshot {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
}

/// A multi-line text buffer whose cursor column is measured in characters,
/// never raw UTF-8 bytes.
#[derive(Debug, Clone)]
pub struct TextBuffer {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
    scroll_row: usize,
    scroll_col: usize,
    file_path: Option<PathBuf>,
    dirty: bool,
    undo_stack: Vec<Snapshot>,
    redo_stack: Vec<Snapshot>,
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}

fn char_offset(line: &str, column: usize) -> usize {
    line.char_indices()
        .nth(column)
        .map_or(line.len(), |(offset, _)| offset)
}

fn char_count(line: &str) -> usize {
    line.chars().count()
}

fn leading_indent(line: &str) -> String {
    line.chars()
        .take_while(|character| *character == ' ' || *character == '\t')
        .collect()
}

fn normalized_lines(text: &str) -> Vec<String> {
    let normalized = text.replace("\r\n", "\n");
    let mut lines = normalized
        .split('\n')
        .map(str::to_string)
        .collect::<Vec<_>>();
    if normalized.ends_with('\n') {
        let _ = lines.pop();
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

impl TextBuffer {
    /// Creates an empty buffer with one line.
    #[must_use]
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            cursor_row: 0,
            cursor_col: 0,
            scroll_row: 0,
            scroll_col: 0,
            file_path: None,
            dirty: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Creates a buffer from text.
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        Self {
            lines: normalized_lines(text),
            cursor_row: 0,
            cursor_col: 0,
            scroll_row: 0,
            scroll_col: 0,
            file_path: None,
            dirty: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Loads a file, normalizing CRLF line endings.
    ///
    /// # Errors
    ///
    /// Returns the underlying I/O error when the file cannot be read.
    pub fn load_from_file(path: &Path) -> io::Result<Self> {
        let text = fs::read_to_string(path)?;
        let mut buffer = Self::from_text(&text);
        buffer.file_path = Some(path.to_path_buf());
        buffer.dirty = false;
        Ok(buffer)
    }

    /// Atomically saves the buffer. A path with no extension gets `.eng`.
    ///
    /// # Errors
    ///
    /// Returns the underlying I/O error when the temporary file or rename
    /// cannot be written.
    pub fn save_to_file(&mut self, path: &Path) -> io::Result<PathBuf> {
        let mut target = path.to_path_buf();
        if target.extension().is_none() {
            target.set_extension("eng");
        }
        let mut temporary = target.as_os_str().to_os_string();
        temporary.push(".tmp");
        let temporary = PathBuf::from(temporary);
        fs::write(&temporary, self.to_text())?;
        fs::rename(&temporary, &target)?;
        self.file_path = Some(target.clone());
        self.dirty = false;
        Ok(target)
    }

    /// Returns all lines joined with a trailing newline.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut text = self.lines.join("\n");
        text.push('\n');
        text
    }

    /// Returns the immutable line slice.
    #[must_use]
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Returns the cursor as `(row, column)` character coordinates.
    #[must_use]
    pub fn cursor(&self) -> (usize, usize) {
        (self.cursor_row, self.cursor_col)
    }

    /// Returns the current cursor row.
    #[must_use]
    pub fn cursor_row(&self) -> usize {
        self.cursor_row
    }

    /// Returns the current cursor character column.
    #[must_use]
    pub fn cursor_col(&self) -> usize {
        self.cursor_col
    }

    /// Returns the current scroll position.
    #[must_use]
    pub fn scroll(&self) -> (usize, usize) {
        (self.scroll_row, self.scroll_col)
    }

    /// Returns the backing path, when the buffer has been saved or opened.
    #[must_use]
    pub fn file_path(&self) -> Option<&Path> {
        self.file_path.as_deref()
    }

    /// Returns the dirty flag.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Marks the buffer clean without saving (used by tests and fresh buffers).
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// Marks the buffer dirty (used when loading an in-memory preset).
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Sets the target path without writing.
    pub fn set_file_path(&mut self, path: Option<PathBuf>) {
        self.file_path = path;
    }

    /// Replaces all contents and clears undo history.
    pub fn replace_text(&mut self, text: &str) {
        self.lines = normalized_lines(text);
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.scroll_row = 0;
        self.scroll_col = 0;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.dirty = false;
    }

    /// Sets the cursor, clamping row and column.
    pub fn set_cursor(&mut self, row: usize, column: usize) {
        self.cursor_row = row.min(self.lines.len().saturating_sub(1));
        self.cursor_col = column.min(char_count(&self.lines[self.cursor_row]));
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            lines: self.lines.clone(),
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
        }
    }

    fn begin_edit(&mut self) {
        self.undo_stack.push(self.snapshot());
        if self.undo_stack.len() > MAX_UNDO {
            let _ = self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
        self.dirty = true;
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.lines = snapshot.lines;
        self.cursor_row = snapshot.cursor_row.min(self.lines.len().saturating_sub(1));
        self.cursor_col = snapshot
            .cursor_col
            .min(char_count(&self.lines[self.cursor_row]));
    }

    /// Inserts one character at the cursor.
    pub fn insert_char(&mut self, character: char) {
        self.begin_edit();
        let offset = char_offset(&self.lines[self.cursor_row], self.cursor_col);
        self.lines[self.cursor_row].insert(offset, character);
        self.cursor_col += 1;
    }

    /// Inserts four spaces at the cursor.
    pub fn insert_tab(&mut self) {
        self.insert_spaces(4);
    }

    fn insert_spaces(&mut self, count: usize) {
        self.begin_edit();
        let offset = char_offset(&self.lines[self.cursor_row], self.cursor_col);
        self.lines[self.cursor_row].insert_str(offset, &" ".repeat(count));
        self.cursor_col += count;
    }

    /// Splits the line at the cursor, preserving leading indentation and
    /// adding four spaces after a trailing colon.
    pub fn insert_newline(&mut self) {
        self.begin_edit();
        let line = self.lines[self.cursor_row].clone();
        let offset = char_offset(&line, self.cursor_col);
        let left = line[..offset].to_string();
        let right = line[offset..].to_string();
        let mut indent = leading_indent(&line);
        if left.trim_end().ends_with(':') {
            indent.push_str("    ");
        }
        let indent_chars = char_count(&indent);
        self.lines[self.cursor_row] = left;
        self.lines
            .insert(self.cursor_row + 1, format!("{indent}{right}"));
        self.cursor_row += 1;
        self.cursor_col = indent_chars;
    }

    /// Backspaces one character, or joins with the previous line at column 0.
    pub fn backspace(&mut self) {
        if self.cursor_col > 0 {
            self.begin_edit();
            let line = &mut self.lines[self.cursor_row];
            let offset = char_offset(line, self.cursor_col);
            if let Some((previous, _)) = line[..offset].char_indices().last() {
                line.remove(previous);
                self.cursor_col -= 1;
            }
            return;
        }
        if self.cursor_row > 0 {
            self.begin_edit();
            let current = self.lines.remove(self.cursor_row);
            self.cursor_row -= 1;
            self.cursor_col = char_count(&self.lines[self.cursor_row]);
            self.lines[self.cursor_row].push_str(&current);
        }
    }

    /// Deletes the character under the cursor, or joins the next line at end.
    pub fn delete_char(&mut self) {
        let line_len = char_count(&self.lines[self.cursor_row]);
        if self.cursor_col < line_len {
            self.begin_edit();
            let line = &mut self.lines[self.cursor_row];
            let offset = char_offset(line, self.cursor_col);
            let _ = line.remove(offset);
            return;
        }
        if self.cursor_row + 1 < self.lines.len() {
            self.begin_edit();
            let next = self.lines.remove(self.cursor_row + 1);
            self.lines[self.cursor_row].push_str(&next);
        }
    }

    /// Moves up one line.
    pub fn move_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.clamp_column();
        }
    }

    /// Moves down one line.
    pub fn move_down(&mut self) {
        if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.clamp_column();
        }
    }

    /// Moves left one character, wrapping to the previous line at column 0.
    pub fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = char_count(&self.lines[self.cursor_row]);
        }
    }

    /// Moves right one character, wrapping to the next line at end.
    pub fn move_right(&mut self) {
        if self.cursor_col < char_count(&self.lines[self.cursor_row]) {
            self.cursor_col += 1;
        } else if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = 0;
        }
    }

    /// Moves to the first character of the line.
    pub fn move_home(&mut self) {
        self.cursor_col = 0;
    }

    /// Moves to the end of the line.
    pub fn move_end(&mut self) {
        self.cursor_col = char_count(&self.lines[self.cursor_row]);
    }

    /// Moves up by `rows` lines.
    pub fn page_up(&mut self, rows: usize) {
        self.cursor_row = self.cursor_row.saturating_sub(rows);
        self.clamp_column();
    }

    /// Moves down by `rows` lines.
    pub fn page_down(&mut self, rows: usize) {
        self.cursor_row = (self.cursor_row + rows).min(self.lines.len().saturating_sub(1));
        self.clamp_column();
    }

    fn clamp_column(&mut self) {
        self.cursor_col = self
            .cursor_col
            .min(char_count(&self.lines[self.cursor_row]));
    }

    /// Undoes one edit, returning false when the stack is empty.
    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo_stack.pop() else {
            return false;
        };
        self.redo_stack.push(self.snapshot());
        if self.redo_stack.len() > MAX_UNDO {
            let _ = self.redo_stack.remove(0);
        }
        self.restore(previous);
        self.dirty = true;
        true
    }

    /// Redoes one edit, returning false when the stack is empty.
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo_stack.pop() else {
            return false;
        };
        self.undo_stack.push(self.snapshot());
        self.restore(next);
        self.dirty = true;
        true
    }

    /// Finds the next case-insensitive occurrence, wrapping at the end.
    pub fn find_next(&mut self, query: &str) -> bool {
        let needle = query
            .chars()
            .map(|character| character.to_ascii_lowercase())
            .collect::<Vec<_>>();
        if needle.is_empty() {
            return false;
        }
        let total = self.lines.len();
        if total == 0 {
            return false;
        }
        for offset in 0..=total {
            let row = (self.cursor_row + offset) % total;
            let haystack = self.lines[row]
                .chars()
                .map(|character| character.to_ascii_lowercase())
                .collect::<Vec<_>>();
            let start = if offset == 0 {
                self.cursor_col.saturating_add(1)
            } else {
                0
            };
            if start >= haystack.len() {
                continue;
            }
            let last = haystack.len().saturating_sub(needle.len());
            if start > last {
                continue;
            }
            for position in start..=last {
                if haystack[position..position + needle.len()] == needle[..] {
                    self.cursor_row = row;
                    self.cursor_col = position;
                    return true;
                }
            }
        }
        false
    }

    /// Adjusts the scroll origin so the cursor stays visible.
    pub fn ensure_cursor_visible(&mut self, rows: usize, columns: usize) {
        if rows == 0 && columns == 0 {
            self.scroll_row = self.cursor_row;
            self.scroll_col = self.cursor_col;
            return;
        }
        if self.cursor_row < self.scroll_row {
            self.scroll_row = self.cursor_row;
        } else if rows > 0 && self.cursor_row >= self.scroll_row + rows {
            self.scroll_row = self.cursor_row + 1 - rows;
        }
        if self.cursor_col < self.scroll_col {
            self.scroll_col = self.cursor_col;
        } else if columns > 0 && self.cursor_col >= self.scroll_col + columns {
            self.scroll_col = self.cursor_col + 1 - columns;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TextBuffer;

    #[test]
    fn buffer_defaults_to_one_line() {
        let buffer = TextBuffer::new();
        assert_eq!(buffer.lines(), &[String::new()]);
    }

    #[test]
    fn buffer_char_offsets_ignore_multibyte_width() {
        let mut buffer = TextBuffer::from_text("abc");
        buffer.set_cursor(0, 1);
        buffer.insert_char('é');
        assert_eq!(buffer.lines()[0], "aébc");
        assert_eq!(buffer.cursor(), (0, 2));
        buffer.backspace();
        assert_eq!(buffer.lines()[0], "abc");
    }
}
