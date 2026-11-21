//! Lightweight lexical tokens shared by segmentation, clauses, and semantics.

/// Coarse token category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    /// An English word or identifier.
    Word,
    /// A decimal or integer literal.
    Number,
    /// A quoted string literal, including its delimiters.
    String,
    /// An operator or punctuation symbol.
    Symbol,
}

/// One token with its original spelling and byte range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// Original source spelling.
    pub text: String,
    /// Case-folded spelling used for words.
    pub lower: String,
    /// Token category.
    pub kind: TokenKind,
    /// Inclusive byte offset in the containing sentence.
    pub start: usize,
    /// Exclusive byte offset in the containing sentence.
    pub end: usize,
}

impl Token {
    /// Returns true when this token is the given lowercase word or symbol.
    #[must_use]
    pub fn is(&self, value: &str) -> bool {
        self.lower == value
    }

    /// Returns true when this token is a word.
    #[must_use]
    pub fn is_word(&self) -> bool {
        self.kind == TokenKind::Word
    }
}

fn is_ident_start(character: char) -> bool {
    character.is_ascii_alphabetic() || character == '_'
}

fn is_ident_continue(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

/// Tokenizes a sentence without losing string literals or decimal numbers.
#[must_use]
pub fn tokenize(text: &str) -> Vec<Token> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let character = text[index..].chars().next().unwrap_or('\0');
        if character.is_whitespace() {
            index += character.len_utf8();
            continue;
        }
        let start = index;
        if character == '"' {
            index += 1;
            let mut escaped = false;
            while index < bytes.len() {
                let current = text[index..].chars().next().unwrap_or('\0');
                index += current.len_utf8();
                if escaped {
                    escaped = false;
                } else if current == '\\' {
                    escaped = true;
                } else if current == '"' {
                    break;
                }
            }
            let raw = &text[start..index];
            tokens.push(Token {
                text: raw.to_string(),
                lower: raw.to_string(),
                kind: TokenKind::String,
                start,
                end: index,
            });
            continue;
        }
        if is_ident_start(character) {
            index += character.len_utf8();
            while index < bytes.len() {
                let current = text[index..].chars().next().unwrap_or('\0');
                if is_ident_continue(current) {
                    index += current.len_utf8();
                } else {
                    break;
                }
            }
            let raw = &text[start..index];
            tokens.push(Token {
                text: raw.to_string(),
                lower: raw.to_ascii_lowercase(),
                kind: TokenKind::Word,
                start,
                end: index,
            });
            continue;
        }
        if character.is_ascii_digit() {
            if character == '0' {
                if let Some(prefix) = text[index + 1..].chars().next() {
                    if matches!(prefix, 'x' | 'X' | 'b' | 'B' | 'o' | 'O') {
                        index += 2;
                        while index < bytes.len() {
                            let current = text[index..].chars().next().unwrap_or('\0');
                            if current.is_ascii_alphanumeric() || current == '_' {
                                index += current.len_utf8();
                            } else {
                                break;
                            }
                        }
                        let raw = &text[start..index];
                        tokens.push(Token {
                            text: raw.to_string(),
                            lower: raw.to_string(),
                            kind: TokenKind::Number,
                            start,
                            end: index,
                        });
                        continue;
                    }
                }
            }
            index += 1;
            while index < bytes.len() {
                let current = text[index..].chars().next().unwrap_or('\0');
                if current.is_ascii_digit() {
                    index += 1;
                } else if current == '.' {
                    let next = text[index + 1..].chars().next();
                    if next.is_some_and(|value| value.is_ascii_digit()) {
                        index += 1;
                    } else {
                        break;
                    }
                } else if current == 'e' || current == 'E' {
                    index += 1;
                    if text[index..].starts_with(['+', '-']) {
                        index += 1;
                    }
                } else {
                    break;
                }
            }
            let raw = &text[start..index];
            tokens.push(Token {
                text: raw.to_string(),
                lower: raw.to_string(),
                kind: TokenKind::Number,
                start,
                end: index,
            });
            continue;
        }
        if character == '\'' && text[index + 1..].starts_with('s') {
            let mut end = index + 2;
            if text[end..].starts_with(|value: char| value.is_ascii_alphabetic() || value == '_') {
                // Keep the possessive marker separate from the following word.
            }
            let raw = &text[start..end];
            tokens.push(Token {
                text: raw.to_string(),
                lower: "'s".to_string(),
                kind: TokenKind::Symbol,
                start,
                end,
            });
            index = end;
            let _ = &mut end;
            continue;
        }
        let mut symbol = character.to_string();
        if index + 1 < bytes.len() {
            let pair = &text[index..index + 2];
            if matches!(
                pair,
                "==" | "!=" | "<=" | ">=" | "+=" | "-=" | "*=" | "/=" | "&&" | "||" | "->"
            ) {
                symbol = pair.to_string();
            }
        }
        if symbol.len() > 1 {
            index += 2;
        } else {
            index += character.len_utf8();
        }
        tokens.push(Token {
            text: symbol.clone(),
            lower: symbol,
            kind: TokenKind::Symbol,
            start,
            end: index,
        });
    }
    tokens
}

/// Decodes a quoted Rust-style byte string token.  Returns `None` when the
/// token is not a well-formed quoted literal.
#[must_use]
pub fn decode_string_token(raw: &str) -> Option<Vec<u8>> {
    let inner = raw.strip_prefix('"')?.strip_suffix('"')?;
    let mut bytes = Vec::new();
    let mut chars = inner.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            let mut buffer = [0_u8; 4];
            bytes.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
            continue;
        }
        let escaped = chars.next()?;
        match escaped {
            'n' => bytes.push(b'\n'),
            'r' => bytes.push(b'\r'),
            't' => bytes.push(b'\t'),
            '0' => bytes.push(0),
            '\\' => bytes.push(b'\\'),
            '"' => bytes.push(b'"'),
            'x' => {
                let high = chars.next()?.to_digit(16)?;
                let low = chars.next()?.to_digit(16)?;
                bytes.push(u8::try_from(high * 16 + low).ok()?);
            }
            other => {
                let mut buffer = [0_u8; 4];
                bytes.extend_from_slice(other.encode_utf8(&mut buffer).as_bytes());
            }
        }
    }
    Some(bytes)
}
