//! Deterministic discourse-level program synthesis patterns.
//!
//! This module recognizes paragraph-level range introductions, referential loop
//! continuations, post-loop closers, and existential `check whether` sentences.
//! It never invents semantics: callers lower the resulting structures through
//! the existing quantifier, condition, and value machinery.

use crate::quantifier::{GlobalQuantifier, QuantDomainSpec};
use crate::textutil::find_top_level_word;
use crate::token::{Token, TokenKind, tokenize};

/// Quantifier used by a `check whether` sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CheckKind {
    /// At least one element matches.
    Any,
    /// Every element matches.
    Every,
    /// No element matches.
    No,
}

/// Domain for a `check whether` inner loop.
#[derive(Debug, Clone)]
pub(crate) enum CheckDomain {
    /// Inclusive range.
    Range { start: String, end: String },
    /// Values smaller than the active loop referent.
    Smaller { start: String },
    /// Values larger than the active loop referent.
    Larger { end: String },
    /// Existing list variable.
    List { name: String },
}

/// One parsed `check whether` sentence.
#[derive(Debug, Clone)]
pub(crate) struct CheckSpec {
    /// Quantifier semantics.
    pub kind: CheckKind,
    /// Surface noun used as the inner loop name.
    pub noun: String,
    /// Inner-domain specification.
    pub domain: CheckDomain,
    /// Predicate text, either `divides ...` or a standard condition.
    pub predicate: String,
}

/// One parsed discourse loop introduction.
#[derive(Debug, Clone)]
pub(crate) struct LoopIntroduction {
    /// Existing global quantifier shape.
    pub quantifier: GlobalQuantifier,
}

fn top_level_phrase(text: &str, phrase: &str) -> Option<(usize, usize)> {
    let words = phrase.split_whitespace().collect::<Vec<_>>();
    let tokens = tokenize(text);
    let mut depth = 0_i32;
    for (index, token) in tokens.iter().enumerate() {
        if token.kind == TokenKind::Symbol {
            match token.text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                _ => {}
            }
        }
        if depth != 0 {
            continue;
        }
        let matched = words.iter().enumerate().all(|(offset, word)| {
            tokens
                .get(index + offset)
                .is_some_and(|candidate| candidate.kind == TokenKind::Word && candidate.is(word))
        });
        if matched {
            return Some((token.start, tokens[index + words.len() - 1].end));
        }
    }
    None
}

fn trimmed_lower(text: &str) -> String {
    text.trim()
        .trim_end_matches(['.', ',', ':'])
        .trim()
        .to_ascii_lowercase()
}

fn capture_name(tokens: &[Token], start: usize, end: usize) -> Option<String> {
    let words = tokens
        .get(start..end)?
        .iter()
        .filter(|token| token.is_word())
        .map(|token| token.lower.clone())
        .collect::<Vec<_>>();
    (!words.is_empty()).then(|| words.join("_"))
}

/// Parses `Look at every x from a up to b.` / `each x in list`.
pub(crate) fn parse_loop_introduction(text: &str) -> Option<LoopIntroduction> {
    let tokens = tokenize(text);
    let quantifier_index = tokens.iter().position(|token| {
        token.kind == TokenKind::Word && (token.is("every") || token.is("each"))
    })?;
    if quantifier_index == 0 {
        return None;
    }
    let opener = tokens
        .iter()
        .take(quantifier_index)
        .filter(|token| token.is_word())
        .map(|token| token.lower.as_str())
        .collect::<Vec<_>>();
    let allowed = [
        "look", "at", "consider", "examine", "take", "go", "through", "check",
    ];
    if opener.is_empty() || !opener.iter().all(|word| allowed.contains(word)) {
        return None;
    }
    let domain_index = tokens
        .iter()
        .enumerate()
        .skip(quantifier_index + 1)
        .find(|(_, token)| token.is("from") || token.is("in"))
        .map(|(index, _)| index)?;
    let var = capture_name(&tokens, quantifier_index + 1, domain_index)?;
    let domain_token = &tokens[domain_index];
    if domain_token.is("in") {
        let list = capture_name(&tokens, domain_index + 1, tokens.len())?;
        return Some(LoopIntroduction {
            quantifier: GlobalQuantifier {
                var: var.clone(),
                domain: QuantDomainSpec::Array { arr: list },
                body: None,
                display: format!("For each {var} in list"),
            },
        });
    }
    let rest = &text[domain_token.end..];
    let (start, end) = parse_range_bounds(rest)?;
    Some(LoopIntroduction {
        quantifier: GlobalQuantifier {
            var: var.clone(),
            domain: QuantDomainSpec::Range {
                start: start.clone(),
                end: end.clone(),
                step: None,
            },
            body: None,
            display: format!("For every {var} from {start} to {end}"),
        },
    })
}

fn parse_range_bounds(text: &str) -> Option<(String, String)> {
    let (through_start, through_end) = if let Some(value) = top_level_phrase(text, "up to") {
        value
    } else if let Some(value) = top_level_phrase(text, "through") {
        value
    } else {
        find_top_level_word(text, "to")?
    };
    let start = text[..through_start].trim();
    let end = text[through_end..]
        .trim()
        .trim_end_matches(['.', ',', ':'])
        .trim();
    (!start.is_empty() && !end.is_empty()).then(|| (start.to_string(), end.to_string()))
}

/// Strips `For each <var>,` / `For every <var>,` when it continues an active
/// discourse loop without a new domain.
pub(crate) fn continuation_rest(text: &str, var: &str) -> Option<String> {
    let tokens = tokenize(text);
    let [first, second, named, comma, ..] = tokens.as_slice() else {
        return None;
    };
    if !first.is("for")
        || !(second.is("each") || second.is("every"))
        || !named.is_word()
        || comma.text != ","
    {
        return None;
    }
    let name = &named.lower[..];
    if name != var && name.replace('_', " ") != var {
        return None;
    }
    let rest = text[comma.end..].trim();
    (!rest.is_empty()).then(|| rest.to_string())
}

/// Returns the post-loop statement after a discourse closer opener.
pub(crate) fn post_loop_rest(text: &str) -> Option<String> {
    let lower = trimmed_lower(text);
    for opener in [
        "once you are done",
        "when you are done",
        "once finished",
        "when finished",
        "afterwards",
        "afterward",
        "after checking all of them",
        "after that",
    ] {
        if lower == opener {
            return None;
        }
        if let Some(rest) = lower.strip_prefix(opener) {
            if let Some(rest) = rest.trim_start().strip_prefix(',') {
                let offset = text.to_ascii_lowercase().find(opener)? + opener.len();
                let original = text[offset..].trim_start();
                let original = original.strip_prefix(',').unwrap_or(original).trim();
                let _ = rest;
                return (!original.is_empty()).then(|| original.to_string());
            }
        }
    }
    None
}

/// Parses a `check whether ...` sentence body (without the leading verb).
pub(crate) fn parse_check(text: &str) -> Option<CheckSpec> {
    let tokens = tokenize(text);
    let (kind, quantifier_end) = if tokens.first().is_some_and(|token| token.is("at"))
        && tokens.get(1).is_some_and(|token| token.is("least"))
        && tokens.get(2).is_some_and(|token| token.is("one"))
    {
        (CheckKind::Any, tokens[2].end)
    } else {
        let token = tokens.first()?;
        let kind = match token.lower.as_str() {
            "any" | "some" | "a" => CheckKind::Any,
            "every" | "all" => CheckKind::Every,
            "no" => CheckKind::No,
            _ => return None,
        };
        (kind, token.end)
    };
    let predicate_words = [
        "divides",
        "is",
        "are",
        "equals",
        "contains",
        "starts",
        "ends",
        "matches",
        "satisfies",
        "has",
        "does",
    ];
    let predicate_index = tokens
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, token)| token.is_word() && predicate_words.contains(&token.lower.as_str()))
        .map(|(index, _)| index)?;
    let predicate_token = &tokens[predicate_index];
    let range_text = text[quantifier_end..predicate_token.start].trim();
    let predicate = text[predicate_token.start..]
        .trim()
        .trim_end_matches(['.', ',', ':'])
        .trim()
        .to_string();
    let noun = tokens
        .iter()
        .skip(1)
        .take(predicate_index - 1)
        .find(|token| {
            token.is_word()
                && !matches!(
                    token.lower.as_str(),
                    "smaller" | "larger" | "number" | "item" | "value" | "starting" | "from"
                )
        })
        .map_or_else(|| "item".to_string(), |token| token.lower.clone());
    let domain = parse_check_domain(range_text)?;
    Some(CheckSpec {
        kind,
        noun,
        domain,
        predicate,
    })
}

fn parse_check_domain(text: &str) -> Option<CheckDomain> {
    let lower = text.trim().to_ascii_lowercase();
    if let Some((_, end)) = find_top_level_word(text, "in") {
        let name = text[end..].trim().trim_end_matches('.').trim();
        if !name.is_empty() {
            return Some(CheckDomain::List {
                name: name.to_ascii_lowercase().replace(' ', "_"),
            });
        }
    }
    if lower.starts_with("smaller") {
        let (_, end) = top_level_phrase(text, "starting from")
            .or_else(|| find_top_level_word(text, "from"))?;
        let start = text[end..].trim().trim_end_matches('.').trim();
        return (!start.is_empty()).then(|| CheckDomain::Smaller {
            start: start.to_string(),
        });
    }
    if lower.starts_with("larger") {
        let (_, end) =
            top_level_phrase(text, "up to").or_else(|| find_top_level_word(text, "to"))?;
        let end_text = text[end..].trim().trim_end_matches('.').trim();
        return (!end_text.is_empty()).then(|| CheckDomain::Larger {
            end: end_text.to_string(),
        });
    }
    if let Some((_, end)) = top_level_phrase(text, "between") {
        let rest = &text[end..];
        let (low_end, high_end) = find_top_level_word(rest, "and")?;
        let high = rest[high_end..].trim().trim_end_matches('.').trim();
        let low = rest[..low_end].trim();
        return (!low.is_empty() && !high.is_empty()).then(|| CheckDomain::Range {
            start: low.to_string(),
            end: high.to_string(),
        });
    }
    if let Some((_, from_end)) = find_top_level_word(text, "from") {
        let (start, end) = parse_range_bounds(&text[from_end..])?;
        return Some(CheckDomain::Range { start, end });
    }
    let (start, end) = parse_range_bounds(text)?;
    Some(CheckDomain::Range { start, end })
}
