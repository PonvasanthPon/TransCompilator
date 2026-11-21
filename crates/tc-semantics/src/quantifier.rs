//! Deterministic quantifier parsing and hygienic desugaring metadata.
//!
//! Global quantifiers become Core-7 `Loop` statements; local quantifiers
//! become short-circuiting prelude loops plus one boolean temporary.  No new
//! IR opcodes are introduced.

use crate::token::{Token, TokenKind, tokenize};

/// Inclusive integer range or fixed-size array domain.
#[derive(Debug, Clone)]
pub(crate) enum QuantDomainSpec {
    /// `from <start> to <end> [by <step>]`.
    Range {
        /// Range start expression text.
        start: String,
        /// Inclusive range end expression text.
        end: String,
        /// Optional step expression text.
        step: Option<String>,
    },
    /// Fixed-size array variable.
    Array {
        /// Array variable name.
        arr: String,
    },
}

/// Local boolean quantifier kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalQuantKind {
    /// Universal: true when every element satisfies the predicate.
    Every,
    /// Existential: true when at least one element satisfies the predicate.
    Some,
    /// Negative existential: true when no element satisfies the predicate.
    No,
}

/// How the predicate was written.
#[derive(Debug, Clone)]
pub(crate) enum PredSpec {
    /// Implicit subject: `is more than 0`, `equals 42`, `> 0`.
    Implicit(String),
    /// Explicit subject: `satisfies x > 0`, `where x * x < 100`.
    Explicit(String),
}

/// One parsed local boolean quantifier.
#[derive(Debug, Clone)]
pub(crate) struct LocalQuantSpec {
    /// Quantifier kind.
    pub kind: LocalQuantKind,
    /// Bound variable name.
    pub var: String,
    /// Quantification domain.
    pub domain: QuantDomainSpec,
    /// Predicate source text.
    pub pred: PredSpec,
    /// Deterministic paraphrase rendering.
    pub display: String,
}

/// One parsed global statement quantifier.
#[derive(Debug, Clone)]
pub(crate) struct GlobalQuantifier {
    /// Bound variable name.
    pub var: String,
    /// Loop domain.
    pub domain: QuantDomainSpec,
    /// Inline body after the comma, when present.
    pub body: Option<String>,
    /// Deterministic paraphrase rendering.
    pub display: String,
}

fn is_quantifier_word(word: &str) -> bool {
    matches!(word, "every" | "each" | "some" | "no")
}

fn top_level_word(tokens: &[Token], word: &str, start: usize) -> Option<usize> {
    let mut depth = 0_i32;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        if token.kind == TokenKind::Symbol {
            match token.text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                _ => {}
            }
        }
        if depth == 0 && token.kind == TokenKind::Word && token.is(word) {
            return Some(index);
        }
    }
    None
}

fn top_level_comma(tokens: &[Token], start: usize) -> Option<usize> {
    let mut depth = 0_i32;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        if token.kind == TokenKind::Symbol {
            match token.text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                "," if depth == 0 => return Some(index),
                _ => {}
            }
        }
    }
    None
}

fn text_between(text: &str, start: usize, end: usize) -> String {
    text[start..end].trim().to_string()
}

fn predicate_starter(token: &Token) -> bool {
    match token.kind {
        TokenKind::Word => matches!(
            token.lower.as_str(),
            "is" | "are"
                | "equals"
                | "equal"
                | "has"
                | "have"
                | "contains"
                | "matches"
                | "more"
                | "less"
                | "greater"
                | "smaller"
                | "at"
                | "not"
                | "satisfies"
                | "where"
        ),
        TokenKind::Symbol => matches!(token.text.as_str(), "==" | "!=" | "<" | "<=" | ">" | ">="),
        _ => false,
    }
}

fn first_predicate(tokens: &[Token], start: usize) -> Option<usize> {
    let mut depth = 0_i32;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        if token.kind == TokenKind::Symbol {
            match token.text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                _ => {}
            }
        }
        if depth == 0 && predicate_starter(token) {
            return Some(index);
        }
    }
    None
}

fn display_range(start: &str, end: &str, step: Option<&str>) -> String {
    let step_text = step.unwrap_or("1");
    format!("from {start} to {end} (inclusive, step {step_text})")
}

/// Parses `For every <var> from ...` / `For every <var> in ...`.
///
/// Returns `Ok(None)` when the statement is not a global quantifier.
pub(crate) fn parse_global(text: &str) -> Result<Option<GlobalQuantifier>, String> {
    let tokens = tokenize(text);
    if !tokens
        .first()
        .is_some_and(|token| token.kind == TokenKind::Word && token.is("for"))
    {
        return Ok(None);
    }
    let Some(kind_token) = tokens.get(1) else {
        return Ok(None);
    };
    if kind_token.kind != TokenKind::Word || !matches!(kind_token.lower.as_str(), "every" | "each")
    {
        return Ok(None);
    }
    let Some(var_token) = tokens.get(2) else {
        return Err("global quantifier requires a bound variable".to_string());
    };
    if var_token.kind != TokenKind::Word {
        return Err("global quantifier requires a bound variable".to_string());
    }
    let var = var_token.lower.clone();
    let Some(domain_keyword) = tokens.get(3) else {
        return Err(format!(
            "global quantifier `{var}` needs `from <start> to <end>` or `in <array>`"
        ));
    };
    if domain_keyword.is("from") {
        let to_index = top_level_word(&tokens, "to", 4)
            .ok_or_else(|| format!("range quantifier `{var}` is missing `to`"))?;
        let start = text_between(text, domain_keyword.end, tokens[to_index].start);
        if start.is_empty() {
            return Err(format!("range quantifier `{var}` has an empty start bound"));
        }
        let comma = top_level_comma(&tokens, to_index + 1);
        let by_index = top_level_word(&tokens, "by", to_index + 1);
        let end_boundary = by_index
            .filter(|index| comma.is_none_or(|comma| *index < comma))
            .or(comma)
            .unwrap_or(tokens.len());
        let end = text_between(
            text,
            tokens[to_index].end,
            tokens
                .get(end_boundary)
                .map_or(text.len(), |token| token.start),
        );
        if end.is_empty() {
            return Err(format!("range quantifier `{var}` has an empty end bound"));
        }
        let step = by_index
            .filter(|index| comma.is_none_or(|comma| *index < comma))
            .map(|index| {
                let boundary = comma.unwrap_or(tokens.len());
                text_between(
                    text,
                    tokens[index].end,
                    tokens.get(boundary).map_or(text.len(), |token| token.start),
                )
            });
        if step.as_ref().is_some_and(String::is_empty) {
            return Err(format!("range quantifier `{var}` has an empty step"));
        }
        let body = comma.map(|index| text_between(text, tokens[index].end, text.len()));
        let body = body.filter(|value| !value.is_empty());
        let display = format!(
            "For every {var} {}",
            display_range(&start, &end, step.as_deref())
        );
        return Ok(Some(GlobalQuantifier {
            var,
            domain: QuantDomainSpec::Range { start, end, step },
            body,
            display,
        }));
    }
    if domain_keyword.is("in") {
        let Some(arr_token) = tokens.get(4) else {
            return Err(format!("array quantifier `{var}` is missing an array name"));
        };
        if arr_token.kind != TokenKind::Word {
            return Err(format!(
                "array quantifier `{var}` needs a fixed-size array name"
            ));
        }
        let arr = arr_token.lower.clone();
        let comma = top_level_comma(&tokens, 5);
        let body = comma.map(|index| text_between(text, tokens[index].end, text.len()));
        let body = body.filter(|value| !value.is_empty());
        let display = format!("For every {var} in {arr}");
        return Ok(Some(GlobalQuantifier {
            var,
            domain: QuantDomainSpec::Array { arr },
            body,
            display,
        }));
    }
    Err(format!(
        "global quantifier `{var}` needs `from <start> to <end>` or `in <array>`"
    ))
}

fn explicit_marker(tokens: &[Token], start: usize) -> Option<usize> {
    let mut depth = 0_i32;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        if token.kind == TokenKind::Symbol {
            match token.text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                _ => {}
            }
        }
        if depth == 0
            && token.kind == TokenKind::Word
            && (token.is("satisfies") || token.is("where") || token.is("such"))
        {
            return Some(index);
        }
    }
    None
}

/// Parses a local `every` / `some` / `no` quantifier at the start of `text`.
pub(crate) fn parse_local_quantifier(text: &str) -> Result<LocalQuantSpec, String> {
    let tokens = tokenize(text);
    let Some(kind_token) = tokens.first() else {
        return Err("empty local quantifier".to_string());
    };
    if kind_token.kind != TokenKind::Word || !is_quantifier_word(&kind_token.lower) {
        return Err("expected every, each, some, or no".to_string());
    }
    let kind = match kind_token.lower.as_str() {
        "every" | "each" => LocalQuantKind::Every,
        "some" => LocalQuantKind::Some,
        _ => LocalQuantKind::No,
    };
    let Some(var_token) = tokens.get(1) else {
        return Err("local quantifier requires a bound variable".to_string());
    };
    if var_token.kind != TokenKind::Word {
        return Err("local quantifier requires a bound variable".to_string());
    }
    let var = var_token.lower.clone();
    let Some(domain_keyword) = tokens.get(2) else {
        return Err(format!(
            "local quantifier `{var}` needs `in <array>` or `from <start> to <end>`"
        ));
    };
    let display_head = format!("{} {var}", kind_token.lower);
    if domain_keyword.is("in") {
        let Some(arr_token) = tokens.get(3) else {
            return Err(format!("local quantifier `{var}` is missing an array name"));
        };
        if arr_token.kind != TokenKind::Word {
            return Err(format!("local quantifier `{var}` needs an array name"));
        }
        let arr = arr_token.lower.clone();
        let pred_start = 4;
        let Some(first) = tokens.get(pred_start) else {
            return Err(format!("local quantifier `{var}` is missing a predicate"));
        };
        let (pred, display_tail) = if first.is("satisfies") || first.is("where") {
            let text_start = first.end;
            (
                PredSpec::Explicit(text_between(text, text_start, text.len())),
                format!(
                    "satisfies {{{}}}",
                    text_between(text, text_start, text.len())
                ),
            )
        } else if first.is("such") && tokens.get(pred_start + 1).is_some_and(|t| t.is("that")) {
            let text_start = tokens[pred_start + 1].end;
            (
                PredSpec::Explicit(text_between(text, text_start, text.len())),
                format!(
                    "satisfies {{{}}}",
                    text_between(text, text_start, text.len())
                ),
            )
        } else {
            (
                PredSpec::Implicit(text_between(text, first.start, text.len())),
                text_between(text, first.start, text.len()),
            )
        };
        let pred_empty = match &pred {
            PredSpec::Implicit(text) | PredSpec::Explicit(text) => text.is_empty(),
        };
        if pred_empty {
            return Err(format!("local quantifier `{var}` has an empty predicate"));
        }
        return Ok(LocalQuantSpec {
            kind,
            var,
            domain: QuantDomainSpec::Array { arr: arr.clone() },
            pred,
            display: format!("{display_head} in {arr} {display_tail}"),
        });
    }
    if domain_keyword.is("from") {
        let to_index = top_level_word(&tokens, "to", 3)
            .ok_or_else(|| format!("range quantifier `{var}` is missing `to`"))?;
        let start = text_between(text, domain_keyword.end, tokens[to_index].start);
        if start.is_empty() {
            return Err(format!("range quantifier `{var}` has an empty start bound"));
        }
        let marker = explicit_marker(&tokens, to_index + 1);
        let predicate_index = marker.or_else(|| first_predicate(&tokens, to_index + 1));
        let Some(predicate_index) = predicate_index else {
            return Err(format!("range quantifier `{var}` is missing a predicate"));
        };
        let by_index =
            top_level_word(&tokens, "by", to_index + 1).filter(|index| *index < predicate_index);
        let end_boundary = by_index.unwrap_or(predicate_index);
        let end = text_between(text, tokens[to_index].end, tokens[end_boundary].start);
        if end.is_empty() {
            return Err(format!("range quantifier `{var}` has an empty end bound"));
        }
        let step = by_index
            .map(|index| text_between(text, tokens[index].end, tokens[predicate_index].start));
        if step.as_ref().is_some_and(String::is_empty) {
            return Err(format!("range quantifier `{var}` has an empty step"));
        }
        let marker_token = &tokens[predicate_index];
        let (pred, _display_tail) = if marker_token.is("satisfies") || marker_token.is("where") {
            let pred = text_between(text, marker_token.end, text.len());
            (
                PredSpec::Explicit(pred.clone()),
                format!("satisfies {{{pred}}}"),
            )
        } else if marker_token.is("such")
            && tokens
                .get(predicate_index + 1)
                .is_some_and(|token| token.is("that"))
        {
            let pred = text_between(text, tokens[predicate_index + 1].end, text.len());
            (
                PredSpec::Explicit(pred.clone()),
                format!("satisfies {{{pred}}}"),
            )
        } else {
            let pred = text_between(text, marker_token.start, text.len());
            (PredSpec::Implicit(pred.clone()), pred)
        };
        let pred_empty = match &pred {
            PredSpec::Implicit(text) | PredSpec::Explicit(text) => text.is_empty(),
        };
        if pred_empty {
            return Err(format!("local quantifier `{var}` has an empty predicate"));
        }
        return Ok(LocalQuantSpec {
            kind,
            var,
            domain: QuantDomainSpec::Range { start, end, step },
            pred,
            display: format!(
                "{display_head} {}",
                display_range_from_parts(&tokens, text, to_index, by_index, predicate_index)
            ),
        });
    }
    Err(format!(
        "local quantifier `{var}` needs `in <array>` or `from <start> to <end>`"
    ))
}

fn display_range_from_parts(
    tokens: &[Token],
    text: &str,
    to_index: usize,
    by_index: Option<usize>,
    predicate_index: usize,
) -> String {
    let start = text_between(text, tokens[2].end, tokens[to_index].start);
    let end_boundary = by_index.unwrap_or(predicate_index);
    let end = text_between(text, tokens[to_index].end, tokens[end_boundary].start);
    let step =
        by_index.map(|index| text_between(text, tokens[index].end, tokens[predicate_index].start));
    format!(
        "from {start} to {end} (inclusive, step {})",
        step.unwrap_or_else(|| "1".to_string())
    )
}
