//! Minimal zero-dependency JSON value, parser, and serializer.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// JSON value.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    /// JSON null.
    Null,
    /// Boolean.
    Bool(bool),
    /// Number.
    Number(f64),
    /// String.
    String(String),
    /// Array.
    Array(Vec<Json>),
    /// Object with deterministic key order.
    Object(BTreeMap<String, Json>),
}

impl Json {
    /// Builds an object.
    #[must_use]
    pub fn object(entries: impl IntoIterator<Item = (impl Into<String>, Json)>) -> Self {
        Self::Object(
            entries
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        )
    }

    /// Builds an array.
    #[must_use]
    pub fn array(values: Vec<Json>) -> Self {
        Self::Array(values)
    }

    /// Builds a string value.
    #[must_use]
    pub fn string(value: impl Into<String>) -> Self {
        Self::String(value.into())
    }

    /// Builds an integer number.
    #[must_use]
    pub fn int(value: i64) -> Self {
        Self::Number(value as f64)
    }

    /// Builds an unsigned integer number.
    #[must_use]
    pub fn uint(value: usize) -> Self {
        Self::Number(value as f64)
    }

    /// Returns an object member.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Self::Object(map) => map.get(key),
            _ => None,
        }
    }

    /// Returns a string member.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    /// Returns a boolean member.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    /// Returns an array member.
    #[must_use]
    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Self::Array(values) => Some(values),
            _ => None,
        }
    }

    /// Returns a number as `usize`.
    #[must_use]
    pub fn as_usize(&self) -> Option<usize> {
        match self {
            Self::Number(value) if *value >= 0.0 && value.is_finite() => Some(*value as usize),
            _ => None,
        }
    }
}

struct Parser<'a> {
    text: &'a [u8],
    position: usize,
}

impl Parser<'_> {
    fn skip_whitespace(&mut self) {
        while self
            .text
            .get(self.position)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.position += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.text.get(self.position).copied()
    }

    fn parse_value(&mut self) -> Result<Json, String> {
        self.skip_whitespace();
        match self.peek() {
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'"') => self.parse_string().map(Json::String),
            Some(b't') => {
                self.expect_literal(b"true")?;
                Ok(Json::Bool(true))
            }
            Some(b'f') => {
                self.expect_literal(b"false")?;
                Ok(Json::Bool(false))
            }
            Some(b'n') => {
                self.expect_literal(b"null")?;
                Ok(Json::Null)
            }
            Some(b'-' | b'0'..=b'9') => self.parse_number(),
            _ => Err("expected a JSON value".to_string()),
        }
    }

    fn expect_literal(&mut self, literal: &[u8]) -> Result<(), String> {
        if self.text[self.position..].starts_with(literal) {
            self.position += literal.len();
            Ok(())
        } else {
            Err("invalid JSON literal".to_string())
        }
    }

    fn parse_object(&mut self) -> Result<Json, String> {
        self.position += 1;
        let mut map = BTreeMap::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.position += 1;
            return Ok(Json::Object(map));
        }
        loop {
            self.skip_whitespace();
            let key = self.parse_string()?;
            self.skip_whitespace();
            if self.peek() != Some(b':') {
                return Err("expected `:`".to_string());
            }
            self.position += 1;
            let value = self.parse_value()?;
            map.insert(key, value);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => {
                    self.position += 1;
                }
                Some(b'}') => {
                    self.position += 1;
                    break;
                }
                _ => return Err("expected `,` or `}`".to_string()),
            }
        }
        Ok(Json::Object(map))
    }

    fn parse_array(&mut self) -> Result<Json, String> {
        self.position += 1;
        let mut values = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.position += 1;
            return Ok(Json::Array(values));
        }
        loop {
            values.push(self.parse_value()?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => {
                    self.position += 1;
                }
                Some(b']') => {
                    self.position += 1;
                    break;
                }
                _ => return Err("expected `,` or `]`".to_string()),
            }
        }
        Ok(Json::Array(values))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        if self.peek() != Some(b'"') {
            return Err("expected string".to_string());
        }
        self.position += 1;
        let mut output = String::new();
        while let Some(byte) = self.peek() {
            self.position += 1;
            match byte {
                b'"' => return Ok(output),
                b'\\' => {
                    let escaped = self
                        .peek()
                        .ok_or_else(|| "unterminated escape".to_string())?;
                    self.position += 1;
                    match escaped {
                        b'"' => output.push('"'),
                        b'\\' => output.push('\\'),
                        b'/' => output.push('/'),
                        b'b' => output.push('\u{0008}'),
                        b'f' => output.push('\u{000c}'),
                        b'n' => output.push('\n'),
                        b'r' => output.push('\r'),
                        b't' => output.push('\t'),
                        b'u' => {
                            let mut code = 0_u32;
                            for _ in 0..4 {
                                let digit = self
                                    .peek()
                                    .and_then(|value| char::from(value).to_digit(16))
                                    .ok_or_else(|| "invalid unicode escape".to_string())?;
                                self.position += 1;
                                code = code * 16 + digit;
                            }
                            output.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        }
                        _ => return Err("invalid escape".to_string()),
                    }
                }
                0x00..=0x1f => return Err("control character in string".to_string()),
                0x20..=0x7f => output.push(char::from(byte)),
                _ => {
                    let start = self.position - 1;
                    let width = utf8_width(byte);
                    if width == 0 || start + width > self.text.len() {
                        return Err("invalid UTF-8".to_string());
                    }
                    let text = std::str::from_utf8(&self.text[start..start + width])
                        .map_err(|_| "invalid UTF-8".to_string())?;
                    output.push_str(text);
                    self.position = start + width;
                }
            }
        }
        Err("unterminated string".to_string())
    }

    fn parse_number(&mut self) -> Result<Json, String> {
        let start = self.position;
        while self
            .peek()
            .is_some_and(|byte| matches!(byte, b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E'))
        {
            self.position += 1;
        }
        let text = std::str::from_utf8(&self.text[start..self.position])
            .map_err(|_| "invalid number".to_string())?;
        let value = text
            .parse::<f64>()
            .map_err(|_| "invalid JSON number".to_string())?;
        Ok(Json::Number(value))
    }
}

fn utf8_width(byte: u8) -> usize {
    if byte < 0x80 {
        1
    } else if byte & 0xE0 == 0xC0 {
        2
    } else if byte & 0xF0 == 0xE0 {
        3
    } else if byte & 0xF8 == 0xF0 {
        4
    } else {
        0
    }
}

/// Parses a complete JSON document.
///
/// # Errors
///
/// Returns a descriptive parse error for malformed input.
pub fn parse(text: &str) -> Result<Json, String> {
    let mut parser = Parser {
        text: text.as_bytes(),
        position: 0,
    };
    let value = parser.parse_value()?;
    parser.skip_whitespace();
    if parser.position != parser.text.len() {
        return Err("trailing JSON input".to_string());
    }
    Ok(value)
}

fn escape_string(text: &str, output: &mut String) {
    output.push('"');
    for character in text.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\u{0008}' => output.push_str("\\b"),
            '\u{000c}' => output.push_str("\\f"),
            character if character.is_control() => {
                let _ = write!(output, "\\u{:04x}", u32::from(character));
            }
            character => output.push(character),
        }
    }
    output.push('"');
}

/// Serializes a JSON value.
#[must_use]
pub fn stringify(value: &Json) -> String {
    let mut output = String::new();
    write_value(value, &mut output);
    output
}

fn write_value(value: &Json, output: &mut String) {
    match value {
        Json::Null => output.push_str("null"),
        Json::Bool(flag) => output.push_str(if *flag { "true" } else { "false" }),
        Json::Number(number) => {
            if number.is_finite() && number.fract() == 0.0 && number.abs() < 9.0e15 {
                let _ = write!(output, "{}", *number as i64);
            } else if number.is_finite() {
                let _ = write!(output, "{number}");
            } else {
                output.push_str("null");
            }
        }
        Json::String(text) => escape_string(text, output),
        Json::Array(values) => {
            output.push('[');
            for (index, item) in values.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_value(item, output);
            }
            output.push(']');
        }
        Json::Object(map) => {
            output.push('{');
            for (index, (key, item)) in map.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                escape_string(key, output);
                output.push(':');
                write_value(item, output);
            }
            output.push('}');
        }
    }
}
