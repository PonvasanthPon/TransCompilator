//! Small RFC 8259 JSON parser and serializer used by the LSP transport.
//!
//! The parser is deliberately strict and allocation-friendly. It supports the
//! complete JSON string escape set, Unicode surrogate pairs, arrays, objects,
//! booleans, null, and integer/float numbers.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// One JSON value.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// Signed integer.
    Number(i64),
    /// Floating-point number.
    Float(f64),
    /// UTF-8 string.
    String(String),
    /// Ordered array.
    Array(Vec<Json>),
    /// Member map.
    Object(BTreeMap<String, Json>),
}

impl Json {
    /// Looks up an object member.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Json> {
        self.as_object()?.get(key)
    }

    /// Returns a string value.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    /// Returns an integer value.
    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }

    /// Returns a boolean value.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    /// Returns an array.
    #[must_use]
    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Self::Array(values) => Some(values),
            _ => None,
        }
    }

    /// Returns an object.
    #[must_use]
    pub fn as_object(&self) -> Option<&BTreeMap<String, Json>> {
        match self {
            Self::Object(values) => Some(values),
            _ => None,
        }
    }

    /// Builds an object from string keys.
    #[must_use]
    pub fn object(entries: impl IntoIterator<Item = (impl Into<String>, Json)>) -> Self {
        Self::Object(
            entries
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        )
    }

    /// Builds a string value.
    #[must_use]
    pub fn string(value: impl Into<String>) -> Self {
        Self::String(value.into())
    }
}

struct Parser {
    characters: Vec<char>,
    position: usize,
}

impl Parser {
    fn new(source: &str) -> Self {
        Self {
            characters: source.chars().collect(),
            position: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.characters.get(self.position).copied()
    }

    fn next(&mut self) -> Option<char> {
        let value = self.peek()?;
        self.position += 1;
        Some(value)
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\r' | '\n')) {
            self.position += 1;
        }
    }

    fn expect(&mut self, expected: char) -> Result<(), String> {
        if self.next() == Some(expected) {
            Ok(())
        } else {
            Err(format!("expected `{expected}`"))
        }
    }

    fn parse_value(&mut self) -> Result<Json, String> {
        self.skip_whitespace();
        match self.peek() {
            Some('n') => {
                self.literal("null")?;
                Ok(Json::Null)
            }
            Some('t') => {
                self.literal("true")?;
                Ok(Json::Bool(true))
            }
            Some('f') => {
                self.literal("false")?;
                Ok(Json::Bool(false))
            }
            Some('"') => Ok(Json::String(self.parse_string()?)),
            Some('[') => self.parse_array(),
            Some('{') => self.parse_object(),
            Some('-' | '0'..='9') => self.parse_number(),
            Some(other) => Err(format!("unexpected JSON character `{other}`")),
            None => Err("unexpected end of JSON".to_string()),
        }
    }

    fn literal(&mut self, text: &str) -> Result<(), String> {
        for expected in text.chars() {
            if self.next() != Some(expected) {
                return Err(format!("expected `{text}`"));
            }
        }
        Ok(())
    }

    fn parse_array(&mut self) -> Result<Json, String> {
        self.expect('[')?;
        let mut values = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(']') {
            self.position += 1;
            return Ok(Json::Array(values));
        }
        loop {
            values.push(self.parse_value()?);
            self.skip_whitespace();
            match self.next() {
                Some(',') => {}
                Some(']') => return Ok(Json::Array(values)),
                _ => return Err("expected `,` or `]` in array".to_string()),
            }
        }
    }

    fn parse_object(&mut self) -> Result<Json, String> {
        self.expect('{')?;
        let mut values = BTreeMap::new();
        self.skip_whitespace();
        if self.peek() == Some('}') {
            self.position += 1;
            return Ok(Json::Object(values));
        }
        loop {
            self.skip_whitespace();
            let key = self.parse_string()?;
            self.skip_whitespace();
            self.expect(':')?;
            let value = self.parse_value()?;
            values.insert(key, value);
            self.skip_whitespace();
            match self.next() {
                Some(',') => {}
                Some('}') => return Ok(Json::Object(values)),
                _ => return Err("expected `,` or `}` in object".to_string()),
            }
        }
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect('"')?;
        let mut output = String::new();
        loop {
            let Some(character) = self.next() else {
                return Err("unterminated JSON string".to_string());
            };
            match character {
                '"' => return Ok(output),
                '\\' => {
                    let Some(escaped) = self.next() else {
                        return Err("unterminated JSON escape".to_string());
                    };
                    match escaped {
                        '"' => output.push('"'),
                        '\\' => output.push('\\'),
                        '/' => output.push('/'),
                        'b' => output.push('\u{0008}'),
                        'f' => output.push('\u{000c}'),
                        'n' => output.push('\n'),
                        'r' => output.push('\r'),
                        't' => output.push('\t'),
                        'u' => output.push(self.parse_unicode_escape()?),
                        other => return Err(format!("unsupported JSON escape `\\{other}`")),
                    }
                }
                value if value.is_control() => {
                    return Err("control character in JSON string".to_string());
                }
                value => output.push(value),
            }
        }
    }

    fn parse_unicode_escape(&mut self) -> Result<char, String> {
        let first = self.parse_hex_quad()?;
        if (0xD800..=0xDBFF).contains(&first) {
            if self.next() != Some('\\') || self.next() != Some('u') {
                return Err("high surrogate is missing a low surrogate".to_string());
            }
            let second = self.parse_hex_quad()?;
            if !(0xDC00..=0xDFFF).contains(&second) {
                return Err("invalid low surrogate".to_string());
            }
            let value = 0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00);
            char::from_u32(value).ok_or_else(|| "invalid Unicode scalar value".to_string())
        } else if (0xDC00..=0xDFFF).contains(&first) {
            Err("unexpected low surrogate".to_string())
        } else {
            char::from_u32(first).ok_or_else(|| "invalid Unicode scalar value".to_string())
        }
    }

    fn parse_hex_quad(&mut self) -> Result<u32, String> {
        let mut value = 0_u32;
        for _ in 0..4 {
            let Some(character) = self.next() else {
                return Err("short Unicode escape".to_string());
            };
            let digit = character
                .to_digit(16)
                .ok_or_else(|| "invalid Unicode digit".to_string())?;
            value = (value << 4) | digit;
        }
        Ok(value)
    }

    fn parse_number(&mut self) -> Result<Json, String> {
        let start = self.position;
        if self.peek() == Some('-') {
            self.position += 1;
        }
        while matches!(self.peek(), Some('0'..='9')) {
            self.position += 1;
        }
        let mut float = false;
        if self.peek() == Some('.') {
            float = true;
            self.position += 1;
            while matches!(self.peek(), Some('0'..='9')) {
                self.position += 1;
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            float = true;
            self.position += 1;
            if matches!(self.peek(), Some('+' | '-')) {
                self.position += 1;
            }
            while matches!(self.peek(), Some('0'..='9')) {
                self.position += 1;
            }
        }
        let text = self.characters[start..self.position]
            .iter()
            .collect::<String>();
        if float {
            text.parse::<f64>()
                .map(Json::Float)
                .map_err(|_| "invalid JSON number".to_string())
        } else {
            text.parse::<i64>()
                .map(Json::Number)
                .map_err(|_| "invalid JSON integer".to_string())
        }
    }

    fn parse_document(&mut self) -> Result<Json, String> {
        let value = self.parse_value()?;
        self.skip_whitespace();
        if self.position != self.characters.len() {
            return Err("trailing data after JSON value".to_string());
        }
        Ok(value)
    }
}

/// Parses one complete JSON document.
///
/// # Errors
///
/// Returns a message when the input is not valid JSON.
pub fn parse(source: &str) -> Result<Json, String> {
    Parser::new(source).parse_document()
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
        Json::Bool(true) => output.push_str("true"),
        Json::Bool(false) => output.push_str("false"),
        Json::Number(number) => {
            let _ = write!(output, "{number}");
        }
        Json::Float(number) => {
            let _ = write!(output, "{number:?}");
        }
        Json::String(text) => write_string(text, output),
        Json::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_value(value, output);
            }
            output.push(']');
        }
        Json::Object(values) => {
            output.push('{');
            for (index, (key, value)) in values.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_string(key, output);
                output.push(':');
                write_value(value, output);
            }
            output.push('}');
        }
    }
}

fn write_string(text: &str, output: &mut String) {
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
            value if value.is_control() => {
                let _ = write!(output, "\\u{:04x}", u32::from(value));
            }
            value => output.push(value),
        }
    }
    output.push('"');
}
