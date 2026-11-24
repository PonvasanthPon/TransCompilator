//! Harmless English discourse-opener and wrapper normalization.
//!
//! The normalizer only removes leading politeness/sequence markers. It never
//! changes spans or text inside string literals, and never strips `Then`,
//! `Otherwise`, or `Else`.

use crate::token::{TokenKind, tokenize};

fn contraction_replacement(stem: &str, suffix: &str) -> Option<&'static str> {
    match suffix {
        "t" => match stem {
            "isn" => Some("is not"),
            "aren" => Some("are not"),
            "wasn" => Some("was not"),
            "weren" => Some("were not"),
            "doesn" => Some("does not"),
            "don" => Some("do not"),
            "didn" => Some("did not"),
            "hasn" => Some("has not"),
            "haven" => Some("have not"),
            "hadn" => Some("had not"),
            "can" => Some("cannot"),
            "won" => Some("will not"),
            "wouldn" => Some("would not"),
            "couldn" => Some("could not"),
            "shouldn" => Some("should not"),
            "mustn" => Some("must not"),
            "shan" => Some("shall not"),
            _ => None,
        },
        "s" => match stem {
            "it" => Some("it is"),
            "that" => Some("that is"),
            "there" => Some("there is"),
            "here" => Some("here is"),
            "what" => Some("what is"),
            "who" => Some("who is"),
            "let" => Some("let us"),
            _ => None,
        },
        "re" => match stem {
            "you" => Some("you are"),
            "we" => Some("we are"),
            "they" => Some("they are"),
            _ => None,
        },
        "m" if stem == "i" => Some("I am"),
        "ll" => match stem {
            "you" => Some("you will"),
            "we" => Some("we will"),
            "they" => Some("they will"),
            "it" => Some("it will"),
            _ => None,
        },
        "ve" => match stem {
            "you" => Some("you have"),
            "we" => Some("we have"),
            "they" => Some("they have"),
            _ => None,
        },
        _ => None,
    }
}

/// Expands common English contractions outside quoted strings.
#[must_use]
pub(crate) fn expand_contractions(text: &str) -> String {
    let tokens = tokenize(text);
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if token.kind != TokenKind::Word {
            index += 1;
            continue;
        }
        let mut suffix = None;
        let mut consumed = 0;
        if let Some(next) = tokens.get(index + 1) {
            if next.text == "'" {
                if let Some(word) = tokens.get(index + 2)
                    && text[token.end..next.start].is_empty()
                    && text[next.end..word.start].is_empty()
                {
                    suffix = Some(word.lower.clone());
                    consumed = 3;
                }
            } else if next.kind == TokenKind::Symbol
                && next.text.starts_with('\'')
                && next.text.len() > 1
                && text[token.end..next.start].is_empty()
            {
                suffix = Some(next.text[1..].to_ascii_lowercase());
                consumed = 2;
            }
        }
        let replacement = suffix
            .as_deref()
            .and_then(|suffix| contraction_replacement(&token.lower, suffix));
        if let (Some(replacement_text), true) = (replacement, consumed > 0) {
            output.push_str(&text[cursor..token.start]);
            output.push_str(replacement_text);
            cursor = tokens[index + consumed - 1].end;
            index += consumed;
        } else {
            index += 1;
        }
    }
    output.push_str(&text[cursor..]);
    output
}

fn strip_casual_openers(text: &str) -> String {
    let mut current = text.trim().to_string();
    loop {
        let lower = current.to_ascii_lowercase();
        let mut changed = false;
        for (prefix, replacement) in [
            ("start by setting ", "set "),
            ("start by putting ", "put "),
            ("start by ", ""),
            ("go ahead and ", ""),
            ("at the end, ", ""),
            ("at the end ", ""),
            ("when that is done, ", ""),
            ("when that is done ", ""),
            ("once everything is finished, ", ""),
            ("once everything is finished ", ""),
            ("when everything is finished, ", ""),
            ("once everything is done, ", ""),
            ("when everything is done, ", ""),
            ("tell me the result", "print the result"),
            ("tell me ", "print "),
            ("show me ", "print "),
        ] {
            if lower.starts_with(prefix) {
                let rest = current[prefix.len()..].trim_start();
                current = format!("{replacement}{rest}");
                changed = true;
                break;
            }
        }
        if !changed {
            return current;
        }
    }
}

/// Normalizes one sentence while preserving its original source span.
#[must_use]
pub(crate) fn sentence(text: &str) -> String {
    let expanded = expand_contractions(text);
    let mut current = strip_casual_openers(&expanded);
    loop {
        if let Some((start, end)) = crate::textutil::find_top_level_phrase(&current, " up by ")
            .or_else(|| crate::textutil::find_top_level_phrase(&current, " upward by "))
            .or_else(|| crate::textutil::find_top_level_phrase(&current, " down by "))
            .or_else(|| crate::textutil::find_top_level_phrase(&current, " downward by "))
        {
            let before = current[..start].trim();
            let amount = current[end..].trim();
            let tokens = tokenize(before);
            if let Some(first) = tokens.first() {
                let target = before[first.end..].trim();
                let first_lower = first.lower.as_str();
                if !target.is_empty()
                    && !before.contains(',')
                    && amount.contains(|character: char| character.is_ascii_digit())
                    && !matches!(
                        first_lower,
                        "the"
                            | "a"
                            | "an"
                            | "it"
                            | "he"
                            | "she"
                            | "they"
                            | "we"
                            | "i"
                            | "otherwise"
                            | "else"
                            | "then"
                    )
                {
                    let down = current[..end].to_ascii_lowercase().contains("down");
                    current = if down {
                        format!("subtract {amount} from {target}")
                    } else {
                        format!("add {amount} to {target}")
                    };
                    continue;
                }
            }
        }
        if let Some((start, end)) =
            crate::textutil::find_top_level_phrase(&current, "ends up being")
        {
            current = format!("{}is{}", &current[..start], &current[end..]);
            continue;
        }
        if let Some((start, end)) =
            crate::textutil::find_top_level_phrase(&current, "the number where you stopped")
        {
            current = format!(
                "{}the stopping number{}",
                &current[..start],
                &current[end..]
            );
            continue;
        }
        if let Some((start, end)) =
            crate::textutil::find_top_level_phrase(&current, "the number at which you stopped")
        {
            current = format!(
                "{}the stopping number{}",
                &current[..start],
                &current[end..]
            );
            continue;
        }
        if let Some((start, end)) =
            crate::textutil::find_top_level_phrase(&current, "the number you stopped at")
        {
            current = format!(
                "{}the stopping number{}",
                &current[..start],
                &current[end..]
            );
            continue;
        }
        if let Some((start, end)) =
            crate::textutil::find_top_level_phrase(&current, "the number where it stopped")
        {
            current = format!(
                "{}the stopping number{}",
                &current[..start],
                &current[end..]
            );
            continue;
        }
        for (phrase, replacement) in [
            ("does not equal", "is not equal to"),
            ("do not equal", "are not equal to"),
            ("is not bigger than", "is at most"),
            ("is not larger than", "is at most"),
            ("is not higher than", "is at most"),
            ("is not smaller than", "is at least"),
            ("is not lower than", "is at least"),
            ("is no bigger than", "is at most"),
            ("is no larger than", "is at most"),
            ("is no smaller than", "is at least"),
            ("is no less than", "is at least"),
            ("is higher than", "is greater than"),
            ("is lower than", "is less than"),
            ("is larger than", "is greater than"),
            ("is bigger than", "is greater than"),
            ("is smaller than", "is less than"),
        ] {
            if let Some((start, end)) = crate::textutil::find_top_level_phrase(&current, phrase) {
                current = format!("{}{}{}", &current[..start], replacement, &current[end..]);
                break;
            }
        }
        for (phrase, replacement) in [
            ("holds more than twice as much as", "is more than twice"),
            ("has more than twice as much as", "is more than twice"),
            ("is more than twice as much as", "is more than twice"),
            ("holds more than", "is more than"),
            ("contains more than", "is more than"),
            ("holds at least", "is at least"),
            ("holds less than", "is less than"),
        ] {
            if let Some((start, end)) = crate::textutil::find_top_level_phrase(&current, phrase) {
                current = format!("{}{}{}", &current[..start], replacement, &current[end..]);
                break;
            }
        }
        let mut stripped_amount_phrase = false;
        for phrase in [
            "the amounts in ",
            "the amount in ",
            "the values in ",
            "the value in ",
            "the contents of ",
        ] {
            if let Some((start, end)) = crate::textutil::find_top_level_phrase(&current, phrase) {
                current = format!("{}{}", &current[..start], &current[end..]);
                stripped_amount_phrase = true;
                break;
            }
        }
        if stripped_amount_phrase {
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
