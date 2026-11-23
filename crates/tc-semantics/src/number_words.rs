//! Deterministic English cardinal number words.
//!
//! Supports `zero` through `ninety-nine`, `hundred`, `thousand`, and
//! `million`, with optional `and`. Articles `a`/`an` are never numbers.

/// Replaces hyphens inside number-word compounds with spaces.
#[must_use]
pub(crate) fn normalize_hyphens(text: &str) -> String {
    let tokens = crate::token::tokenize(text);
    let mut replacements = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if token.text != "-" {
            continue;
        }
        let Some(previous) = index.checked_sub(1).and_then(|value| tokens.get(value)) else {
            continue;
        };
        let Some(next) = tokens.get(index + 1) else {
            continue;
        };
        if previous.is_word() && next.is_word() && is_word(&previous.lower) && is_word(&next.lower)
        {
            replacements.push((token.start, token.end));
        }
    }
    if replacements.is_empty() {
        return text.to_string();
    }
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;
    for (start, end) in replacements {
        output.push_str(&text[cursor..start]);
        output.push(' ');
        cursor = end;
    }
    output.push_str(&text[cursor..]);
    output
}

/// Parses one complete English number-word phrase.
#[must_use]
pub(crate) fn parse(text: &str) -> Option<i64> {
    let lower = text.trim().to_ascii_lowercase();
    if lower.is_empty()
        || !lower
            .chars()
            .all(|character| character.is_ascii_alphabetic() || matches!(character, ' ' | '-'))
    {
        return None;
    }
    let words = lower
        .replace('-', " ")
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    if words.is_empty() || matches!(words.first().map(String::as_str), Some("a" | "an")) {
        return None;
    }
    let mut total = 0_i64;
    let mut current = 0_i64;
    let mut saw_number = false;
    for word in &words {
        if word == "and" {
            continue;
        }
        if let Some(value) = unit(word) {
            current += value;
            saw_number = true;
            continue;
        }
        if let Some(value) = tens(word) {
            current += value;
            saw_number = true;
            continue;
        }
        match word.as_str() {
            "hundred" => {
                current = current.max(1) * 100;
                saw_number = true;
            }
            "thousand" => {
                total += current.max(1) * 1_000;
                current = 0;
                saw_number = true;
            }
            "million" => {
                total += current.max(1) * 1_000_000;
                current = 0;
                saw_number = true;
            }
            _ => return None,
        }
    }
    saw_number.then_some(total + current)
}

/// Returns true for a word that may participate in a number phrase.
#[must_use]
pub(crate) fn is_word(word: &str) -> bool {
    matches!(
        word,
        "zero"
            | "one"
            | "two"
            | "three"
            | "four"
            | "five"
            | "six"
            | "seven"
            | "eight"
            | "nine"
            | "ten"
            | "eleven"
            | "twelve"
            | "thirteen"
            | "fourteen"
            | "fifteen"
            | "sixteen"
            | "seventeen"
            | "eighteen"
            | "nineteen"
            | "twenty"
            | "thirty"
            | "forty"
            | "fifty"
            | "sixty"
            | "seventy"
            | "eighty"
            | "ninety"
            | "hundred"
            | "thousand"
            | "million"
            | "and"
    )
}

fn unit(word: &str) -> Option<i64> {
    Some(match word {
        "zero" => 0,
        "one" => 1,
        "two" => 2,
        "three" => 3,
        "four" => 4,
        "five" => 5,
        "six" => 6,
        "seven" => 7,
        "eight" => 8,
        "nine" => 9,
        "ten" => 10,
        "eleven" => 11,
        "twelve" => 12,
        "thirteen" => 13,
        "fourteen" => 14,
        "fifteen" => 15,
        "sixteen" => 16,
        "seventeen" => 17,
        "eighteen" => 18,
        "nineteen" => 19,
        _ => return None,
    })
}

fn tens(word: &str) -> Option<i64> {
    Some(match word {
        "twenty" => 20,
        "thirty" => 30,
        "forty" => 40,
        "fifty" => 50,
        "sixty" => 60,
        "seventy" => 70,
        "eighty" => 80,
        "ninety" => 90,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn parses_units_and_compounds() {
        assert_eq!(parse("zero"), Some(0));
        assert_eq!(parse("twenty-three"), Some(23));
        assert_eq!(parse("twenty three"), Some(23));
        assert_eq!(parse("two hundred and fifty"), Some(250));
        assert_eq!(parse("two thousand five hundred"), Some(2500));
        assert_eq!(parse("one million and one"), Some(1_000_001));
    }

    #[test]
    fn rejects_articles_and_unknown_words() {
        assert_eq!(parse("a"), None);
        assert_eq!(parse("an"), None);
        assert_eq!(parse("twenty bananas"), None);
    }
}
