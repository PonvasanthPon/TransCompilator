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
