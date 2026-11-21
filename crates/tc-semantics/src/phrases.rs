//! Declarative `Phrase:` signature matching.
//!
//! A phrase pattern contains literal lowercase words and `<param>` captures.
//! Matching is greedy, requires full token consumption, and returns the source
//! spelling of each captured expression so the expression parser can preserve
//! string literals exactly.

use tc_typeck::{AstType, PhrasePart, PhrasePattern, Visibility};

use crate::token::{Token, TokenKind, tokenize};

/// A phrase pattern together with the function it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhraseBinding {
    /// Defining module.
    pub module: String,
    /// Function name.
    pub function: String,
    /// Function visibility.
    pub vis: Visibility,
    /// Pattern parts.
    pub pattern: PhrasePattern,
    /// Function parameter types in declaration order.
    pub params: Vec<AstType>,
    /// Generic parameter names.
    pub type_params: Vec<String>,
}

/// A successful phrase match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhraseMatch {
    /// Defining module.
    pub module: String,
    /// Function name.
    pub function: String,
    /// Captured argument source text in parameter order.
    pub args: Vec<String>,
    /// Number of literal words in the winning pattern.
    pub score: usize,
    /// Parameter types of the matched function.
    pub params: Vec<AstType>,
    /// Generic parameter names of the matched function.
    pub type_params: Vec<String>,
    /// Canonical source pattern, e.g. `emit <v> to stderr`.
    pub pattern: String,
}

fn pattern_words(pattern: &PhrasePattern) -> impl Iterator<Item = &PhrasePart> {
    pattern.parts.iter()
}

fn pattern_text(pattern: &PhrasePattern) -> String {
    pattern
        .parts
        .iter()
        .map(|part| match part {
            PhrasePart::Word(word) => word.clone(),
            PhrasePart::Param(param) => format!("<{param}>"),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn next_word_index(tokens: &[Token], start: usize) -> Option<usize> {
    let mut depth = 0_i32;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        if token.kind == TokenKind::Symbol {
            match token.text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                _ => {}
            }
        }
        if depth == 0 && token.kind == TokenKind::Word {
            return Some(index);
        }
    }
    None
}

fn capture_text(text: &str, tokens: &[Token], start: usize, end: usize) -> String {
    if start >= end {
        return String::new();
    }
    let first = &tokens[start];
    let last = &tokens[end - 1];
    if last.end <= text.len() && first.start < last.end {
        text[first.start..last.end].trim().to_string()
    } else {
        tokens[start..end]
            .iter()
            .map(|token| token.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn match_binding(binding: &PhraseBinding, text: &str, tokens: &[Token]) -> Option<PhraseMatch> {
    let parts = pattern_words(&binding.pattern).collect::<Vec<_>>();
    let mut token_index = 0;
    let mut args = Vec::new();
    let mut score = 0;
    let mut parameter_index = 0;
    let mut part_index = 0;
    while part_index < parts.len() {
        match &parts[part_index] {
            PhrasePart::Word(word) => {
                let token = tokens.get(token_index)?;
                if token.kind != TokenKind::Word || !token.is(word) {
                    return None;
                }
                token_index += 1;
                score += 1;
                part_index += 1;
            }
            PhrasePart::Param(_) => {
                let next_literal = parts[part_index + 1..].iter().find_map(|part| match part {
                    PhrasePart::Word(word) => Some(word.clone()),
                    PhrasePart::Param(_) => None,
                });
                let end = if let Some(literal) = next_literal {
                    let mut search = token_index;
                    let mut found = None;
                    while let Some(index) = next_word_index(tokens, search) {
                        if tokens[index].is(&literal) {
                            found = Some(index);
                            break;
                        }
                        search = index + 1;
                    }
                    found.unwrap_or(tokens.len())
                } else {
                    tokens.len()
                };
                if end <= token_index {
                    return None;
                }
                args.push(capture_text(text, tokens, token_index, end));
                parameter_index += 1;
                token_index = end;
                part_index += 1;
            }
        }
    }
    if token_index != tokens.len() || args.len() != parameter_index {
        return None;
    }
    Some(PhraseMatch {
        module: binding.module.clone(),
        function: binding.function.clone(),
        args,
        score,
        params: binding.params.clone(),
        type_params: binding.type_params.clone(),
        pattern: pattern_text(&binding.pattern),
    })
}

/// Returns every full phrase match, in binding order.
#[must_use]
pub fn match_all(text: &str, bindings: &[PhraseBinding]) -> Vec<PhraseMatch> {
    let tokens = tokenize(text);
    if tokens.is_empty() {
        return Vec::new();
    }
    bindings
        .iter()
        .filter_map(|binding| match_binding(binding, text, &tokens))
        .collect()
}

/// Matches a cleaned clause against all in-scope phrase bindings, preferring
/// patterns with the most literal words.
#[must_use]
pub fn match_phrase(text: &str, bindings: &[PhraseBinding]) -> Option<PhraseMatch> {
    match_all(text, bindings).into_iter().max_by(|left, right| {
        left.score
            .cmp(&right.score)
            .then_with(|| left.params.len().cmp(&right.params.len()))
            .then_with(|| right.function.cmp(&left.function))
    })
}

/// Returns true when the sentence starts with the literal `phrase` keyword.
#[must_use]
pub fn is_phrase_declaration(text: &str) -> bool {
    text.trim_start()
        .to_ascii_lowercase()
        .starts_with("phrase:")
}
