//! Shared lexical helpers for intent classification and lowering.

use crate::token::{TokenKind, tokenize};

/// Returns the first token whose lowercase spelling equals `word`.
#[must_use]
pub(crate) fn first_word(text: &str) -> String {
    tokenize(text)
        .into_iter()
        .find(|token| token.kind == TokenKind::Word)
        .map_or_else(String::new, |token| token.lower)
}

/// Returns true when the text starts with `word` (case-insensitive).
#[must_use]
pub(crate) fn starts_with_word(text: &str, word: &str) -> bool {
    text.trim_start()
        .to_ascii_lowercase()
        .starts_with(&format!("{word} "))
        || text.trim().eq_ignore_ascii_case(word)
}

/// Finds a top-level word token, ignoring strings and bracketed expressions.
#[must_use]
pub(crate) fn find_top_level_word(text: &str, word: &str) -> Option<(usize, usize)> {
    let tokens = tokenize(text);
    let mut depth = 0_i32;
    for token in &tokens {
        if token.kind == TokenKind::Symbol {
            match token.text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                _ => {}
            }
        }
        if depth == 0 && token.kind == TokenKind::Word && token.is(word) {
            return Some((token.start, token.end));
        }
    }
    None
}

/// Finds a top-level symbol token.
#[must_use]
pub(crate) fn find_top_level_symbol(text: &str, symbol: &str) -> Option<(usize, usize)> {
    let tokens = tokenize(text);
    let mut depth = 0_i32;
    for token in &tokens {
        if token.kind == TokenKind::Symbol {
            match token.text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                _ => {}
            }
            if depth == 0 && token.text == symbol {
                return Some((token.start, token.end));
            }
        }
    }
    None
}

/// Splits text on a top-level literal separator (word or punctuation).
#[must_use]
pub(crate) fn split_top_level(text: &str, separator: &str) -> Vec<String> {
    let tokens = tokenize(text);
    let mut pieces = Vec::new();
    let mut depth = 0_i32;
    let mut start = 0;
    for token in &tokens {
        if token.kind == TokenKind::Symbol {
            match token.text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                _ => {}
            }
        }
        let matches = if token.kind == TokenKind::Symbol {
            token.text == separator
        } else {
            token.is(separator)
        };
        if depth == 0 && matches {
            let piece = text[start..token.start].trim();
            if !piece.is_empty() {
                pieces.push(piece.to_string());
            }
            start = token.end;
        }
    }
    let tail = text[start..].trim();
    if !tail.is_empty() {
        pieces.push(tail.to_string());
    }
    pieces
}

/// Splits an optional trailing `where` clause from a statement or expression.
#[must_use]
pub(crate) fn split_where(text: &str) -> (String, Option<String>) {
    if let Some((start, end)) = find_top_level_word(text, "where") {
        let core = text[..start].trim().to_string();
        let bindings = text[end..].trim().to_string();
        if bindings.is_empty() {
            (core, None)
        } else {
            (core, Some(bindings))
        }
    } else {
        (text.trim().to_string(), None)
    }
}

/// Returns true when the token slice looks like a mutable lvalue.
#[must_use]
pub(crate) fn is_lvalue_text(text: &str) -> bool {
    let tokens = tokenize(text);
    if tokens.is_empty() {
        return false;
    }
    let mut index = 0;
    if tokens[index].is("deref") || tokens[index].text == "*" {
        return tokens.len() > 1;
    }
    if tokens[index].is_word() {
        index += 1;
        if tokens.get(index).is_some_and(|token| token.is("of")) {
            return tokens.len() > index + 1;
        }
    } else if tokens[index].text == "(" {
        let mut depth = 0_i32;
        while index < tokens.len() {
            if tokens[index].text == "(" {
                depth += 1;
            } else if tokens[index].text == ")" {
                depth -= 1;
                if depth == 0 {
                    index += 1;
                    break;
                }
            }
            index += 1;
        }
    } else {
        return false;
    }
    while index < tokens.len() {
        if tokens[index].text == "." || tokens[index].text == "'s" {
            index += 1;
            if index >= tokens.len() || !tokens[index].is_word() {
                return false;
            }
            index += 1;
        } else if tokens[index].text == "[" {
            let mut depth = 0_i32;
            while index < tokens.len() {
                if tokens[index].text == "[" {
                    depth += 1;
                } else if tokens[index].text == "]" {
                    depth -= 1;
                    if depth == 0 {
                        index += 1;
                        break;
                    }
                }
                index += 1;
            }
        } else {
            return false;
        }
    }
    true
}
