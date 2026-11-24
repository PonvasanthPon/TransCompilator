//! Harmless English discourse-opener and wrapper normalization.
//!
//! The normalizer only removes leading politeness/sequence markers. It never
//! changes spans or text inside string literals, and never strips `Then`,
//! `Otherwise`, or `Else`.

use crate::token::{TokenKind, tokenize};

/// Normalizes one sentence while preserving its original source span.
#[must_use]
pub(crate) fn sentence(text: &str) -> String {
    let mut current = text.trim().to_string();
    loop {
        if let Some((start, end)) =
            crate::textutil::find_top_level_phrase(&current, "ends up being")
        {
            current = format!("{}is{}", &current[..start], &current[end..]);
            continue;
        }
        if let Some((start, end)) = crate::textutil::find_top_level_phrase(&current, "ends up") {
            current = format!("{}is{}", &current[..start], &current[end..]);
            continue;
        }
        if let Some((start, end)) = crate::textutil::find_top_level_phrase(&current, "goes over") {
            current = format!("{}is greater than{}", &current[..start], &current[end..]);
            continue;
        }
        if let Some((start, end)) = crate::textutil::find_top_level_phrase(&current, "goes above") {
            current = format!("{}is greater than{}", &current[..start], &current[end..]);
            continue;
        }
        if let Some((start, end)) = crate::textutil::find_top_level_phrase(&current, "exceeds") {
            current = format!("{}is greater than{}", &current[..start], &current[end..]);
            continue;
        }
        if let Some(rest) = current
            .strip_prefix("Whenever ")
            .or_else(|| current.strip_prefix("whenever "))
        {
            current = format!("If {rest}");
            continue;
        }
        if let Some(rest) = current
            .strip_prefix("When ")
            .or_else(|| current.strip_prefix("when "))
        {
            current = format!("If {rest}");
            continue;
        }
        if let Some(rewritten) = halving_resultative(&current) {
            current = rewritten;
            continue;
        }
        let lower = current.to_ascii_lowercase();
        if lower.starts_with("add up ") {
            let rest = current[7..].trim().trim_end_matches('.').trim();
            current = if crate::textutil::find_top_level_word(rest, "to").is_some() {
                format!("add {rest}")
            } else {
                format!("print the sum of {rest}")
            };
            continue;
        }
        if lower.starts_with("sum up ") {
            let rest = current[7..].trim().trim_end_matches('.').trim();
            current = format!("print the sum of {rest}");
            continue;
        }
        if let Some(rest) = polite_question(&current) {
            let mut body = rest.trim_start_matches([',', ' ']).trim().to_string();
            let trimmed = body.trim_end_matches(['?', '!', '.']).trim().to_string();
            if trimmed.is_empty() {
                break;
            }
            body = trimmed;
            current = body;
            continue;
        }
        if lower.starts_with("let's ") {
            current = current[6..].trim_start().to_string();
            continue;
        }
        if lower.starts_with("let us ") {
            current = current[7..].trim_start().to_string();
            continue;
        }
        let tokens = tokenize(&current);
        let Some(first) = tokens.first().filter(|token| token.kind == TokenKind::Word) else {
            break;
        };
        if !matches!(
            first.lower.as_str(),
            "please" | "now" | "first" | "next" | "finally" | "also" | "just"
        ) {
            break;
        }
        let after_marker = tokens
            .get(1)
            .filter(|token| token.kind == TokenKind::Word || token.text == "=");
        if after_marker.is_some_and(|token| {
            token.is_word()
                && matches!(
                    token.lower.as_str(),
                    "is" | "are" | "becomes" | "equals" | "be"
                )
                || token.text == "="
        }) {
            break;
        }
        let rest = current[first.end..].trim_start();
        let rest = rest
            .strip_prefix(',')
            .map_or(rest, |value| value.trim_start());
        if rest.is_empty() {
            break;
        }
        current = rest.to_string();
    }
    current
}

fn halving_resultative(text: &str) -> Option<String> {
    let trimmed = text.trim().trim_end_matches(['.', '!', '?']).trim();
    let tokens = tokenize(trimmed);
    let mut depth = 0_i32;
    let mut halves = None;
    for (index, token) in tokens.iter().enumerate() {
        if token.kind == TokenKind::Symbol {
            match token.text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                _ => {}
            }
            continue;
        }
        if depth == 0
            && token.is("half")
            && tokens
                .get(index.wrapping_sub(1))
                .is_some_and(|previous| previous.is("in"))
        {
            halves = Some(index);
        }
    }
    let half_index = halves?;
    if tokens
        .iter()
        .skip(half_index + 1)
        .any(|token| token.kind != TokenKind::Symbol)
    {
        return None;
    }
    let verb = tokens.first()?;
    if !verb.is_word() || verb.is("print") || verb.is("say") || verb.is("show") {
        return None;
    }
    let resolution = tc_lexicon::classify_verb(&verb.lower, None)?;
    if !matches!(
        resolution.concept,
        Some(tc_lexicon::SemanticConcept::Div | tc_lexicon::SemanticConcept::Destroy)
    ) {
        return None;
    }
    let in_index = half_index.checked_sub(1)?;
    let object_start = tokens.get(1)?.start;
    let object = &trimmed[object_start..tokens[in_index].start];
    let object = object
        .strip_prefix("the ")
        .or_else(|| object.strip_prefix("The "))
        .unwrap_or(object)
        .trim();
    if object.is_empty()
        || !object.split_whitespace().all(|word| {
            word.chars().all(|character| {
                character.is_ascii_alphanumeric() || character == '_' || character == '\''
            })
        })
    {
        return None;
    }
    Some(format!("Set {object} to half of {object}."))
}

fn polite_question(text: &str) -> Option<&str> {
    let trimmed = text.trim_end_matches(['?', '.', '!']).trim_end();
    let lower = trimmed.to_ascii_lowercase();
    for prefix in ["can you ", "could you ", "would you "] {
        if lower.starts_with(prefix) {
            let rest = trimmed[prefix.len()..].trim_start();
            let rest_lower = rest.to_ascii_lowercase();
            if rest_lower.starts_with("please ") {
                return Some(rest[7..].trim_start());
            }
            return Some(rest);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::sentence;

    #[test]
    fn strips_stacked_openers() {
        assert_eq!(sentence("Now, please print x."), "print x.");
        assert_eq!(sentence("First let's show x."), "show x.");
        assert_eq!(
            sentence("Just print \"please, now\"."),
            "print \"please, now\"."
        );
    }

    #[test]
    fn preserves_then_and_otherwise() {
        assert_eq!(sentence("Otherwise, print x."), "Otherwise, print x.");
        assert_eq!(sentence("Then print x."), "Then print x.");
    }

    #[test]
    fn converts_polite_questions() {
        assert_eq!(
            sentence("Could you please show me the sum of 12 and 23?"),
            "show me the sum of 12 and 23"
        );
    }
}
