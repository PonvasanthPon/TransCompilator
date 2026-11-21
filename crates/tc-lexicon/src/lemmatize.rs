//! Deterministic English lemmatization with lexicon-validated suffix rules.

use std::borrow::Cow;

use crate::generated::morphology::{IRREGULAR_LEMMAS, VERB_LEMMAS};

/// Words that end in an inflection-like suffix but must never be mangled.
/// Sorted for binary search.
static PROTECTED: &[&str] = &[
    "always", "bus", "class", "equals", "exists", "gas", "less", "means", "minus", "news",
    "physics", "plus", "series", "species", "times", "towards", "unless",
];

fn irregular(word: &str) -> Option<&'static str> {
    IRREGULAR_LEMMAS
        .binary_search_by(|(form, _)| (*form).cmp(word))
        .ok()
        .map(|index| IRREGULAR_LEMMAS[index].1)
}

fn is_known_verb(word: &str) -> bool {
    VERB_LEMMAS.binary_search(&word).is_ok()
}

fn first_known(candidates: impl IntoIterator<Item = String>) -> Option<String> {
    candidates
        .into_iter()
        .find(|candidate| !candidate.is_empty() && is_known_verb(candidate))
}

fn collapse_double(stem: &str) -> Option<String> {
    let mut chars = stem.chars().rev();
    let last = chars.next()?;
    let previous = chars.next()?;
    if last == previous && last.is_ascii_alphabetic() {
        Some(stem[..stem.len() - last.len_utf8()].to_string())
    } else {
        None
    }
}

fn regular_base(word: &str) -> Option<String> {
    if word.len() <= 3 {
        return None;
    }
    if let Some(stem) = word.strip_suffix("ies") {
        let candidate = format!("{stem}y");
        if is_known_verb(&candidate) {
            return Some(candidate);
        }
    }
    if let Some(stem) = word.strip_suffix("ied") {
        let candidate = format!("{stem}y");
        if is_known_verb(&candidate) {
            return Some(candidate);
        }
    }
    if let Some(stem) = word.strip_suffix("ing") {
        let mut candidates = vec![stem.to_string(), format!("{stem}e")];
        if let Some(collapsed) = collapse_double(stem) {
            candidates.push(collapsed);
        }
        if let Some(base) = first_known(candidates) {
            return Some(base);
        }
    }
    if let Some(stem) = word.strip_suffix("ed") {
        let mut candidates = vec![stem.to_string(), format!("{stem}e")];
        if let Some(collapsed) = collapse_double(stem) {
            candidates.push(collapsed);
        }
        if let Some(base) = first_known(candidates) {
            return Some(base);
        }
    }
    if let Some(stem) = word.strip_suffix("es") {
        let single = &word[..word.len() - 1];
        if let Some(base) = first_known([stem.to_string(), single.to_string()]) {
            return Some(base);
        }
    }
    if let Some(stem) = word.strip_suffix('s') {
        if is_known_verb(stem) {
            return Some(stem.to_string());
        }
    }
    None
}

fn unchanged(word: &str, lower: String) -> Cow<'_, str> {
    if lower == word {
        Cow::Borrowed(word)
    } else {
        Cow::Owned(lower)
    }
}

fn static_base(word: &str, lower: &str, base: &'static str) -> Cow<'static, str> {
    if lower == word {
        Cow::Borrowed(base)
    } else {
        Cow::Owned(base.to_string())
    }
}

/// Lowercases and lemmatizes an English word using the generated irregular
/// table and deterministic, lexicon-validated suffix rules.
#[must_use]
pub fn lemmatize(word: &str) -> Cow<'_, str> {
    let lower = word.to_lowercase();
    if let Some(base) = irregular(&lower) {
        return static_base(word, &lower, base);
    }
    if PROTECTED.binary_search(&lower.as_str()).is_ok() {
        return unchanged(word, lower);
    }
    if let Some(base) = regular_base(&lower) {
        return Cow::Owned(base);
    }
    unchanged(word, lower)
}

#[cfg(test)]
mod tests {
    use super::lemmatize;

    #[test]
    fn irregular_and_regular_examples() {
        assert_eq!(lemmatize("spent").as_ref(), "spend");
        assert_eq!(lemmatize("removed").as_ref(), "remove");
        assert_eq!(lemmatize("running").as_ref(), "run");
        assert_eq!(lemmatize("multiplied").as_ref(), "multiply");
        assert_eq!(lemmatize("increases").as_ref(), "increase");
        assert_eq!(lemmatize("less").as_ref(), "less");
    }
}
