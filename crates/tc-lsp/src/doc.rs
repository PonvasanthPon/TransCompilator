//! Open document storage, `file://` URI conversion, and UTF-16 positions.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Zero-based LSP position in UTF-16 code units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// Zero-based line.
    pub line: u32,
    /// Zero-based UTF-16 character offset.
    pub character: u32,
}

impl Position {
    /// Creates a position.
    #[must_use]
    pub const fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

/// One zero-based range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextRange {
    /// Inclusive start.
    pub start: Position,
    /// Exclusive end.
    pub end: Position,
}

impl TextRange {
    /// Creates a range.
    #[must_use]
    pub const fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }
}

/// One open text document.
#[derive(Debug, Clone)]
pub struct Document {
    /// Original URI.
    pub uri: String,
    /// Resolved filesystem path.
    pub path: PathBuf,
    /// Current full text.
    pub text: String,
    /// Latest client version.
    pub version: i64,
}

impl Document {
    /// Builds a document from an LSP open notification.
    #[must_use]
    pub fn new(uri: impl Into<String>, text: impl Into<String>, version: i64) -> Self {
        let uri = uri.into();
        let path = uri_to_path(&uri);
        Self {
            uri,
            path,
            text: text.into(),
            version,
        }
    }

    /// Converts an LSP position into a byte offset.
    #[must_use]
    pub fn offset_at(&self, position: Position) -> usize {
        offset_at(&self.text, position)
    }

    /// Converts a byte offset into an LSP position.
    #[must_use]
    pub fn position_at(&self, offset: usize) -> Position {
        position_at(&self.text, offset)
    }

    /// Returns the text of one zero-based line without its newline.
    #[must_use]
    pub fn line_text(&self, line: u32) -> &str {
        line_text(&self.text, line)
    }
}

/// Converts a zero-based UTF-16 position into a byte offset.
#[must_use]
pub fn offset_at(text: &str, position: Position) -> usize {
    let target_line = position.line as usize;
    let mut line = 0_usize;
    let mut start = 0_usize;
    for (index, character) in text.char_indices() {
        if line == target_line {
            break;
        }
        if character == '\n' {
            line += 1;
            start = index + 1;
        }
    }
    if line != target_line {
        return text.len();
    }
    let end = text[start..]
        .find('\n')
        .map_or(text.len(), |offset| start + offset);
    let mut utf16 = 0_u32;
    for (offset, character) in text[start..end].char_indices() {
        if utf16 >= position.character {
            return start + offset;
        }
        utf16 += character.len_utf16() as u32;
    }
    end
}

/// Converts a byte offset into a zero-based UTF-16 position.
#[must_use]
pub fn position_at(text: &str, offset: usize) -> Position {
    let offset = offset.min(text.len());
    let mut line = 0_u32;
    let mut start = 0_usize;
    for (index, character) in text.char_indices() {
        if index >= offset {
            break;
        }
        if character == '\n' {
            line += 1;
            start = index + 1;
        }
    }
    let character = text[start..offset]
        .chars()
        .map(|value| value.len_utf16() as u32)
        .sum();
    Position::new(line, character)
}

/// Returns one line of text without the trailing newline.
#[must_use]
pub fn line_text(text: &str, line: u32) -> &str {
    text.split('\n')
        .nth(line as usize)
        .unwrap_or("")
        .trim_end_matches('\r')
}

/// Returns the identifier-like word at a position.
#[must_use]
pub fn word_at(text: &str, position: Position) -> Option<(String, TextRange)> {
    let offset = offset_at(text, position);
    let line = line_text(text, position.line);
    let line_start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    let local = offset.saturating_sub(line_start).min(line.len());
    let is_word = |character: char| character.is_ascii_alphanumeric() || character == '_';
    let mut start = local;
    while start > 0 {
        let Some((index, character)) = line[..start].char_indices().next_back() else {
            break;
        };
        if !is_word(character) {
            break;
        }
        start = index;
    }
    let mut end = local;
    while end < line.len() {
        let Some(character) = line[end..].chars().next() else {
            break;
        };
        if !is_word(character) {
            break;
        }
        end += character.len_utf8();
    }
    if start >= end || !line[start..].chars().next().is_some_and(is_word) {
        return None;
    }
    let start_position = position_at(text, line_start + start);
    let end_position = position_at(text, line_start + end);
    Some((
        line[start..end].to_ascii_lowercase(),
        TextRange::new(start_position, end_position),
    ))
}

/// Converts a `file://` URI into a path.
#[must_use]
pub fn uri_to_path(uri: &str) -> PathBuf {
    let rest = uri.strip_prefix("file://").unwrap_or(uri);
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    PathBuf::from(percent_decode(rest))
}

/// Converts a path into a `file://` URI.
#[must_use]
pub fn path_to_uri(path: &Path) -> String {
    let text = path.to_string_lossy();
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    format!("file://{encoded}")
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = hex(bytes[index + 1]);
            let low = hex(bytes[index + 2]);
            if let (Some(high), Some(low)) = (high, low) {
                output.push((high << 4) | low);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(output).unwrap_or_else(|_| text.to_string())
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
