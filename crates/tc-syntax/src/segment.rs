//! Paragraph, block, and sentence segmentation for English `.tc` sources.
//!
//! Segmentation is deliberately lexical: it preserves quoted strings,
//! decimals, negative numbers, and indentation while turning a source file
//! into a tree of headed blocks and leaf sentences.  Discourse fillers are
//! recorded on each sentence for trace output.

use tc_lexicon::{ClosedClass, lookup_closed_class};

use crate::{Span, tokenize};

/// One natural-language sentence with its original source location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawSentence {
    /// Cleaned sentence text without a trailing period or filler prefix.
    pub text: String,
    /// Original sentence text, including any trailing terminator.
    pub raw: String,
    /// Source location of the first content character.
    pub span: Span,
    /// Leading discourse fillers removed from `text`.
    pub fillers: Vec<String>,
}

/// A leaf sentence or a nested headed block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawItem {
    /// A plain sentence.
    Sentence(RawSentence),
    /// A block introduced by a colon-terminated header.
    Block(RawBlock),
}

/// A headed block such as an `If`, `While`, function definition, or struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawBlock {
    /// Block header, or `None` for the document root.
    pub header: Option<RawSentence>,
    /// Ordered block contents.
    pub items: Vec<RawItem>,
    /// Source location of the header or first item.
    pub span: Span,
}

/// A segmented source document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RawDocument {
    /// Top-level items in source order.
    pub items: Vec<RawItem>,
}

/// A segmented source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    /// Module/file name, conventionally `name.tc`.
    pub name: String,
    /// Complete UTF-8 source text.
    pub text: String,
}

impl SourceFile {
    /// Creates a source file.
    #[must_use]
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            text: text.into(),
        }
    }

    /// Returns the module name derived from the file stem, lowercased.
    #[must_use]
    pub fn module_name(&self) -> String {
        let stem = self
            .name
            .rsplit('/')
            .next()
            .unwrap_or(&self.name)
            .strip_suffix(".tc")
            .unwrap_or_else(|| {
                self.name
                    .rsplit('/')
                    .next()
                    .unwrap_or(&self.name)
                    .strip_suffix(".TC")
                    .unwrap_or(&self.name)
            });
        stem.to_ascii_lowercase()
    }
}

#[derive(Debug, Clone)]
struct Unit {
    indent: usize,
    sentence: RawSentence,
    inline_body: Option<String>,
    inline_column: u32,
}

fn leading_indent(line: &str) -> (usize, usize) {
    let mut indent = 0;
    let mut bytes = 0;
    for character in line.chars() {
        if character == ' ' {
            indent += 1;
            bytes += 1;
        } else if character == '\t' {
            indent += 4;
            bytes += 1;
        } else {
            break;
        }
    }
    (indent, bytes)
}

fn strip_step_marker(text: &str) -> &str {
    let trimmed = text.trim_start();
    let mut digits = 0;
    for character in trimmed.chars() {
        if character.is_ascii_digit() {
            digits += 1;
        } else {
            break;
        }
    }
    if digits > 0 {
        let rest = &trimmed[digits..];
        if rest.starts_with('.') && rest[1..].starts_with(char::is_whitespace) {
            return rest[1..].trim_start();
        }
        if rest.starts_with(')') && rest[1..].starts_with(char::is_whitespace) {
            return rest[1..].trim_start();
        }
    }
    if trimmed.starts_with('-') || trimmed.starts_with('*') {
        let rest = &trimmed[1..];
        if rest.starts_with(char::is_whitespace) {
            return rest.trim_start();
        }
    }
    trimmed
}

fn structural_header(text: &str) -> bool {
    let lower = text.trim_start().to_ascii_lowercase();
    let first = lower.split_whitespace().next().unwrap_or("");
    if matches!(
        first,
        "if" | "else"
            | "otherwise"
            | "unless"
            | "while"
            | "repeat"
            | "loop"
            | "define"
            | "public"
            | "private"
            | "struct"
            | "union"
            | "function"
            | "for"
            | "match"
            | "switch"
            | "when"
    ) {
        return true;
    }
    let mut words = lower.split_whitespace();
    matches!(words.next(), Some("define" | "public" | "private"))
        && matches!(
            words.next(),
            Some("struct" | "union" | "function" | "global")
        )
}

fn split_at_header_colon(text: &str) -> Option<(String, Option<String>, usize)> {
    let mut in_string = false;
    let mut escaped = false;
    let mut depth = 0_i32;
    for (index, character) in text.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        match character {
            '"' => in_string = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ':' if depth == 0 => {
                let header = text[..index].trim();
                let body = text[index + 1..].trim();
                if body.is_empty() {
                    return Some((header.to_string(), None, index + 1));
                }
                if structural_header(header) {
                    return Some((header.to_string(), Some(body.to_string()), index + 1));
                }
                return None;
            }
            _ => {}
        }
    }
    None
}

/// Splits text at top-level sentence terminators while preserving quoted
/// strings, decimal points, and bracketed expressions.
#[must_use]
pub fn split_sentences(text: &str) -> Vec<(usize, String)> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut index = 0;
    let mut in_string = false;
    let mut escaped = false;
    let mut depth = 0_i32;
    let bytes = text.as_bytes();
    while index < bytes.len() {
        let character = text[index..].chars().next().unwrap_or('\0');
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            index += character.len_utf8();
            continue;
        }
        match character {
            '"' => in_string = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '.' | ';' | '!' | '?' if depth == 0 => {
                let next = text[index + character.len_utf8()..]
                    .chars()
                    .next()
                    .unwrap_or('\0');
                if character == '!' && next == '=' {
                    index += character.len_utf8();
                    continue;
                }
                let boundary = character != '.'
                    || next.is_whitespace()
                    || next == '\0'
                    || next == ')'
                    || next == '"';
                if boundary {
                    let piece = text[start..index].trim();
                    if !piece.is_empty() {
                        result.push((start, piece.to_string()));
                    }
                    index += character.len_utf8();
                    while index < bytes.len()
                        && text[index..]
                            .chars()
                            .next()
                            .is_some_and(char::is_whitespace)
                    {
                        index += text[index..].chars().next().map_or(0, char::len_utf8);
                    }
                    start = index;
                    continue;
                }
            }
            _ => {}
        }
        index += character.len_utf8();
    }
    let piece = text[start..].trim();
    if !piece.is_empty() {
        result.push((start, piece.to_string()));
    }
    result
}

fn starts_phrase(rest: &str, phrase: &str) -> Option<usize> {
    let lower = rest.to_ascii_lowercase();
    if !lower.starts_with(phrase) {
        return None;
    }
    let boundary = rest[phrase.len()..].chars().next();
    if boundary
        .is_none_or(|character| character.is_whitespace() || character == ',' || character == ';')
    {
        Some(phrase.len())
    } else {
        None
    }
}

fn strip_fillers(text: &str, column: u32) -> (String, Vec<String>, u32) {
    let mut remaining = text.trim_start().to_string();
    let mut fillers = Vec::new();
    let mut column =
        column + u32::try_from(text.len().saturating_sub(remaining.len())).unwrap_or_default();
    loop {
        let lower = remaining.to_ascii_lowercase();
        let mut consumed = 0;
        if let Some(length) = starts_phrase(&lower, "after that") {
            consumed = length;
        } else {
            let first = lower
                .split(|character: char| {
                    character.is_whitespace() || character == ',' || character == ';'
                })
                .next()
                .unwrap_or("");
            if matches!(
                lookup_closed_class(first),
                Some(ClosedClass::DiscourseFiller)
            ) && matches!(
                first,
                "first"
                    | "next"
                    | "then"
                    | "finally"
                    | "okay"
                    | "ok"
                    | "alright"
                    | "now"
                    | "please"
                    | "also"
                    | "so"
                    | "just"
                    | "after"
                    | "afterwards"
            ) {
                // A comma proves discourse use.  Bare forms are stripped only
                // for unambiguous polite/interjection words, so identifiers
                // such as `first` remain available.
                let after_word = remaining[first.len()..].trim_start();
                let comma = after_word.starts_with(',');
                let bare_allowed = matches!(first, "please" | "okay" | "ok" | "alright");
                if comma || bare_allowed {
                    consumed = first.len();
                }
            }
        }
        if consumed == 0 {
            break;
        }
        let after = remaining[consumed..].trim_start();
        let after = after.strip_prefix(',').unwrap_or(after).trim_start();
        if after.is_empty() {
            break;
        }
        let filler = remaining[..consumed].trim().to_string();
        fillers.push(filler);
        column += u32::try_from(remaining.len() - after.len()).unwrap_or_default();
        remaining = after.to_string();
    }
    (remaining, fillers, column)
}

fn sentence_from_piece(piece: &str, raw_piece: &str, line: u32, base_column: u32) -> RawSentence {
    let (text, fillers, column) = strip_fillers(piece, base_column);
    RawSentence {
        text,
        raw: raw_piece.trim().to_string(),
        span: Span { line, col: column },
        fillers,
    }
}

fn units_from_line(line_text: &str, line_number: u32) -> Vec<Unit> {
    let (indent, offset) = leading_indent(line_text);
    let content = &line_text[offset..];
    let content = strip_step_marker(content);
    let base_column = u32::try_from(offset + 1).unwrap_or(1);
    let mut units = Vec::new();
    let (filler_text, fillers, content_column) = strip_fillers(content, base_column);
    if filler_text.is_empty() {
        return units;
    }
    let (header, inline_body, inline_column) = match split_at_header_colon(&filler_text) {
        Some((header, body, column)) => (header, Some(body.unwrap_or_default()), column),
        None => (filler_text.clone(), None, 0),
    };
    if let Some(body) = inline_body {
        let header_sentence = RawSentence {
            text: header,
            raw: content.to_string(),
            span: Span {
                line: line_number,
                col: content_column,
            },
            fillers,
        };
        units.push(Unit {
            indent,
            sentence: header_sentence,
            inline_body: Some(body),
            inline_column: u32::try_from(content_column as usize + inline_column + 1)
                .unwrap_or(content_column),
        });
        return units;
    }
    for (index, (offset, piece)) in split_sentences(&header).into_iter().enumerate() {
        let column =
            content_column + u32::try_from(header[..offset].chars().count()).unwrap_or_default();
        let mut sentence = sentence_from_piece(&piece, &piece, line_number, column);
        if index == 0 {
            sentence.fillers.clone_from(&fillers);
        }
        if !sentence.text.is_empty() {
            units.push(Unit {
                indent,
                sentence,
                inline_body: None,
                inline_column: content_column,
            });
        }
    }
    units
}

fn push_inline_units(units: &mut Vec<Unit>, parent: &Unit) {
    let Some(body) = &parent.inline_body else {
        return;
    };
    if body.trim().is_empty() {
        return;
    }
    for (offset, piece) in split_sentences(body) {
        let column = parent.inline_column
            + u32::try_from(body[..offset].chars().count()).unwrap_or_default();
        let sentence = sentence_from_piece(&piece, &piece, parent.sentence.span.line, column);
        if !sentence.text.is_empty() {
            units.push(Unit {
                indent: parent.indent + 4,
                sentence,
                inline_body: None,
                inline_column: parent.inline_column,
            });
        }
    }
}

/// Splits one source file into a structured block tree.
#[must_use]
pub fn segment_source(source: &SourceFile) -> RawDocument {
    let mut units = Vec::new();
    for (index, line) in source.text.lines().enumerate() {
        let line_number = u32::try_from(index + 1).unwrap_or(u32::MAX);
        let line_units = units_from_line(line, line_number);
        for unit in line_units {
            let mut inline = Vec::new();
            push_inline_units(&mut inline, &unit);
            units.push(unit);
            units.append(&mut inline);
        }
    }

    let root_span = Span { line: 1, col: 1 };
    let mut stack: Vec<RawBlock> = vec![RawBlock {
        header: None,
        items: Vec::new(),
        span: root_span,
    }];
    let mut stack_indents: Vec<usize> = vec![0];
    for unit in units {
        while stack.len() > 1
            && stack_indents
                .last()
                .is_some_and(|indent| *indent >= unit.indent)
        {
            let finished = stack.pop().expect("root remains");
            stack_indents.pop();
            if let Some(parent) = stack.last_mut() {
                parent.items.push(RawItem::Block(finished));
            }
        }
        if unit.inline_body.is_some() {
            let span = unit.sentence.span;
            let block = RawBlock {
                header: Some(unit.sentence),
                items: Vec::new(),
                span,
            };
            stack.push(block);
            stack_indents.push(unit.indent);
        } else {
            stack
                .last_mut()
                .expect("root")
                .items
                .push(RawItem::Sentence(unit.sentence));
        }
    }
    while stack.len() > 1 {
        let finished = stack.pop().expect("root remains");
        stack_indents.pop();
        if let Some(parent) = stack.last_mut() {
            parent.items.push(RawItem::Block(finished));
        }
    }
    let root = stack.pop().expect("root");
    RawDocument { items: root.items }
}

/// Tokenizes a sentence for downstream clause and expression analysis.
#[must_use]
pub fn sentence_tokens(sentence: &RawSentence) -> Vec<crate::Token> {
    tokenize(&sentence.text)
}

#[cfg(test)]
mod tests {
    use super::{SourceFile, segment_source};

    #[test]
    fn preserves_decimals_and_strings() {
        let source = SourceFile::new(
            "main.tc",
            "Set x to 12.5.\nWrite \"hello. world\" and \"a: b\".\n",
        );
        let document = segment_source(&source);
        let text = format!("{document:?}");
        assert!(text.contains("Set x to 12.5"));
        assert!(text.contains("hello. world"));
        assert!(text.contains("a: b"));
    }

    #[test]
    fn builds_indented_blocks_and_numbered_steps() {
        let source = SourceFile::new(
            "main.tc",
            "If x is 1:\n    Add 2 to x.\nElse:\n    Remove 1 from x.\n1. Put 2 in y.\n",
        );
        let document = segment_source(&source);
        assert_eq!(document.items.len(), 3);
        let debug = format!("{:?}", document.items);
        assert!(debug.contains("If x is 1"));
        assert!(debug.contains("Else"));
        assert!(debug.contains("Put 2 in y"));
    }
}
