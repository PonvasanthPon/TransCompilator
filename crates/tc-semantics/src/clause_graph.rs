//! Compositional English clause decomposition.
//!
//! The graph recognizes leading `Given`/`Assuming` bindings, trailing `where`
//! bindings, sequential `then` chains, and compound declarations.  It operates
//! on token spans only: strings and parenthesized expressions are respected,
//! and an `and`/comma separator is consumed only when the following segment
//! starts a new binding.

use crate::textutil::{find_top_level_phrase, find_top_level_symbol, find_top_level_word};
use crate::token::{TokenKind, tokenize};

/// One decomposed sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClauseGraph {
    /// Binding clauses such as `a is 12`.
    pub bindings: Vec<String>,
    /// Main/sequential clauses in source order.
    pub clauses: Vec<String>,
    /// Names introduced by a compound declaration.
    pub compound_names: Vec<String>,
    /// True when bindings should be scoped locally (`where`).
    pub scoped: bool,
}

/// Decomposes one sentence when a compositional shape is present.
pub(crate) fn decompose(text: &str) -> Option<ClauseGraph> {
    let text = text.trim().trim_end_matches(['.', ',']).trim();
    if text.is_empty() {
        return None;
    }
    if let Some(graph) = decompose_leading(text) {
        return Some(graph);
    }
    if let Some(graph) = decompose_where(text) {
        return Some(graph);
    }
    if let Some(clauses) = split_then(text) {
        if clauses.len() > 1 {
            return Some(ClauseGraph {
                bindings: Vec::new(),
                clauses,
                compound_names: Vec::new(),
                scoped: false,
            });
        }
    }
    if let Some((clauses, names)) = split_compound_declaration(text) {
        return Some(ClauseGraph {
            bindings: Vec::new(),
            clauses,
            compound_names: names,
            scoped: false,
        });
    }
    let first = tokenize(text)
        .into_iter()
        .find(|token| token.kind == TokenKind::Word)
        .map(|token| token.lower)
        .unwrap_or_default();
    if matches!(
        first.as_str(),
        "if" | "while"
            | "unless"
            | "until"
            | "when"
            | "for"
            | "repeat"
            | "loop"
            | "update"
            | "put"
            | "set"
            | "let"
            | "make"
            | "keep"
            | "store"
            | "move"
            | "copy"
            | "create"
            | "add"
            | "remove"
            | "transfer"
    ) {
        return None;
    }
    let bindings = split_binding_sequence(text)?;
    if bindings.len() > 1 {
        let names = bindings
            .iter()
            .filter_map(|value| binding_name(value))
            .collect();
        return Some(ClauseGraph {
            bindings,
            clauses: Vec::new(),
            compound_names: names,
            scoped: false,
        });
    }
    None
}

fn decompose_leading(text: &str) -> Option<ClauseGraph> {
    let tokens = tokenize(text);
    let first = tokens.first()?;
    if !first.is("given") && !first.is("assuming") {
        return None;
    }
    let mut start = first.end;
    if let Some(that) = tokens.get(1).filter(|token| token.is("that")) {
        start = that.end;
    }
    let rest = text[start..].trim_start();
    let comma = find_top_level_symbol(rest, ",").map(|(start, _)| start)?;
    let bindings = split_binding_sequence(rest[..comma].trim())?;
    let main = rest[comma + 1..].trim();
    if main.is_empty() {
        return None;
    }
    let clauses = split_then(main).unwrap_or_else(|| vec![main.to_string()]);
    let names = bindings
        .iter()
        .filter_map(|value| binding_name(value))
        .collect();
    Some(ClauseGraph {
        bindings,
        clauses,
        compound_names: names,
        scoped: false,
    })
}

fn decompose_where(text: &str) -> Option<ClauseGraph> {
    let (start, end) = find_top_level_word(text, "where")?;
    let main = text[..start].trim();
    let bindings_text = text[end..].trim();
    if main.is_empty() || bindings_text.is_empty() {
        return None;
    }
    let bindings = split_binding_sequence(bindings_text)?;
    let names = bindings
        .iter()
        .filter_map(|value| binding_name(value))
        .collect();
    let clauses = split_then(main).unwrap_or_else(|| vec![main.to_string()]);
    Some(ClauseGraph {
        bindings,
        clauses,
        compound_names: names,
        scoped: true,
    })
}

/// Splits `, then ...` / `and then ...` chains unless the sentence is a
/// conditional `if`/`when`/`unless` header.
pub(crate) fn split_then(text: &str) -> Option<Vec<String>> {
    let tokens = tokenize(text);
    let first = tokens.iter().find(|token| token.kind == TokenKind::Word)?;
    if first.is("if")
        || first.is("when")
        || first.is("unless")
        || first.is("while")
        || first.is("repeat")
        || first.is("loop")
        || first.is("for")
    {
        return None;
    }
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0_i32;
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if token.kind == TokenKind::Symbol {
            match token.text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                _ => {}
            }
        }
        if depth == 0 {
            let then_index = if token.text == "," {
                tokens
                    .get(index + 1)
                    .filter(|next| next.is("then"))
                    .map(|_| index + 1)
            } else if token.is("and") {
                tokens
                    .get(index + 1)
                    .filter(|next| next.is("then"))
                    .map(|_| index + 1)
            } else {
                None
            };
            if let Some(then_index) = then_index {
                let clause = text[start..token.start].trim();
                if !clause.is_empty() {
                    parts.push(clause.to_string());
                    start = tokens[then_index].end;
                    index = then_index + 1;
                    continue;
                }
            }
        }
        index += 1;
    }
    if !parts.is_empty() {
        let tail = text[start..].trim();
        if !tail.is_empty() {
            parts.push(tail.to_string());
        }
        return Some(parts);
    }
    None
}

/// Splits a binding list into `<name> is <expr>` segments.
pub(crate) fn split_binding_sequence(text: &str) -> Option<Vec<String>> {
    let pieces = conditional_split(text, |segment| binding_operator(segment).is_some())?;
    if pieces.is_empty() || pieces.iter().any(|piece| binding_operator(piece).is_none()) {
        return None;
    }
    Some(pieces)
}

/// Splits a compound `Let` / `Set` / `Put` declaration into standalone clauses.
pub(crate) fn split_compound_declaration(text: &str) -> Option<(Vec<String>, Vec<String>)> {
    let tokens = tokenize(text);
    let first = tokens.first().filter(|token| token.is_word())?;
    let keyword = if first.is("let") {
        "Let"
    } else if first.is("set") {
        "Set"
    } else if first.is("put") {
        "Put"
    } else {
        return None;
    };
    if tokenize(text).iter().any(|token| {
        token.is_word() && matches!(token.lower.as_str(), "if" | "when" | "unless" | "otherwise")
    }) {
        return None;
    }
    let pieces = conditional_split(text, |segment| {
        let first = tokenize(segment)
            .into_iter()
            .find(|token| token.kind == TokenKind::Word)
            .map(|token| token.lower)
            .unwrap_or_default();
        if first == keyword.to_ascii_lowercase() {
            return true;
        }
        if matches!(
            first.as_str(),
            "add"
                | "remove"
                | "say"
                | "print"
                | "show"
                | "display"
                | "update"
                | "transfer"
                | "move"
                | "copy"
                | "create"
                | "make"
                | "keep"
                | "store"
                | "return"
                | "give"
        ) {
            return false;
        }
        binding_operator(segment).is_some()
            || (keyword == "Put" && find_top_level_word(segment, "in").is_some())
    })?;
    if pieces.len() < 2 {
        return None;
    }
    let mut clauses = Vec::new();
    let mut names = Vec::new();
    for (index, piece) in pieces.iter().enumerate() {
        let trimmed = piece.trim();
        let clause = if index == 0 || starts_with_keyword(trimmed, keyword) {
            trimmed.to_string()
        } else {
            format!("{keyword} {trimmed}")
        };
        let name_source = if starts_with_keyword(trimmed, keyword) {
            let tokens = tokenize(trimmed);
            tokens
                .first()
                .map_or(trimmed, |token| trimmed[token.end..].trim())
        } else {
            trimmed
        };
        let name = if keyword == "Put" {
            find_top_level_word(name_source, "in")
                .map(|(_, end)| canonical_name(name_source[end..].trim()))
        } else {
            binding_operator(name_source)
                .map(|(start, _)| canonical_name(name_source[..start].trim()))
        };
        if let Some(name) = name {
            if !name.is_empty() {
                names.push(name);
            }
        }
        clauses.push(clause);
    }
    Some((clauses, names))
}

fn starts_with_keyword(text: &str, keyword: &str) -> bool {
    tokenize(text)
        .first()
        .is_some_and(|token| token.is(keyword.to_ascii_lowercase().as_str()))
}

fn conditional_split(text: &str, starts_new: impl Fn(&str) -> bool) -> Option<Vec<String>> {
    let tokens = tokenize(text);
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut depth = 0_i32;
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if token.kind == TokenKind::Symbol {
            match token.text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                _ => {}
            }
        }
        let separator = depth == 0
            && (token.text == "," || token.is("and"))
            && !text[start..token.start].trim().is_empty();
        if separator {
            let rest = text[token.end..].trim_start();
            if starts_new(next_segment(rest)) {
                pieces.push(
                    text[start..token.start]
                        .trim()
                        .trim_end_matches(',')
                        .trim()
                        .to_string(),
                );
                start = token.end;
            }
        }
        index += 1;
    }
    let tail = text[start..].trim().trim_end_matches(',').trim();
    if !tail.is_empty() {
        pieces.push(tail.to_string());
    }
    pieces.retain(|piece| !piece.is_empty());
    (!pieces.is_empty()).then_some(pieces)
}

fn next_segment(text: &str) -> &str {
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
        if depth == 0 && (token.text == "," || token.is("and")) {
            return text[..token.start].trim();
        }
    }
    text.trim()
}

fn binding_operator(text: &str) -> Option<(usize, usize)> {
    find_top_level_word(text, "is")
        .or_else(|| find_top_level_word(text, "equals"))
        .or_else(|| find_top_level_word(text, "be"))
        .or_else(|| find_top_level_word(text, "to"))
        .or_else(|| find_top_level_phrase(text, "start at"))
        .or_else(|| find_top_level_phrase(text, "starts at"))
        .or_else(|| find_top_level_phrase(text, "begin at"))
        .or_else(|| find_top_level_phrase(text, "begins at"))
        .or_else(|| find_top_level_symbol(text, "="))
}

fn binding_name(text: &str) -> Option<String> {
    let (start, _) = binding_operator(text)?;
    let name = canonical_name(text[..start].trim());
    (!name.is_empty()).then_some(name)
}

fn canonical_name(text: &str) -> String {
    text.split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join("_")
}

/// Returns the binding parts `(name, expression)`.
pub(crate) fn binding_parts(text: &str) -> Option<(String, String)> {
    let (start, end) = binding_operator(text)?;
    let name = canonical_name(text[..start].trim());
    let expression = text[end..].trim().to_string();
    if name.is_empty() || expression.is_empty() {
        return None;
    }
    Some((name, expression))
}
