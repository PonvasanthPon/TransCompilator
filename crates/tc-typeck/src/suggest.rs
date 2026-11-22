//! Deterministic plain-English typo suggestions.
//!
//! The helper is deliberately tiny and `std`-only: candidates are normalized
//! by lowercasing and treating underscores as spaces, then ranked by
//! `(edit distance, candidate spelling)` so every diagnostic is reproducible.

/// Normalizes a name for human-friendly comparison.
fn normalize(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut previous_space = false;
    for character in value.trim().chars() {
        let mapped = if character == '_' { ' ' } else { character };
        if mapped.is_ascii_whitespace() {
            if !previous_space {
                normalized.push(' ');
            }
            previous_space = true;
        } else {
            normalized.push(mapped.to_ascii_lowercase());
            previous_space = false;
        }
    }
    normalized.trim().to_string()
}

/// Returns the closest candidate within `max_dist` edits.
#[must_use]
pub fn closest_match<'a>(
    query: &str,
    candidates: impl IntoIterator<Item = &'a str>,
    max_dist: usize,
) -> Option<&'a str> {
    let query = normalize(query);
    let mut best: Option<(usize, &'a str)> = None;
    for candidate in candidates {
        let normalized = normalize(candidate);
        let distance = levenshtein(&query, &normalized);
        if distance > max_dist {
            continue;
        }
        let replace = match best {
            Some((best_distance, best_candidate)) => {
                (distance, candidate) < (best_distance, best_candidate)
            }
            None => true,
        };
        if replace {
            best = Some((distance, candidate));
        }
    }
    best.map(|(_, candidate)| candidate)
}

/// Renders a canonical identifier with natural spaces.
#[must_use]
pub fn humanize(name: &str) -> String {
    name.replace('_', " ")
}

fn levenshtein(left: &str, right: &str) -> usize {
    let left = left.chars().collect::<Vec<_>>();
    let right = right.chars().collect::<Vec<_>>();
    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }
    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    let mut current = vec![0; right.len() + 1];
    for (row, left_character) in left.iter().enumerate() {
        current[0] = row + 1;
        for (column, right_character) in right.iter().enumerate() {
            let substitution = previous[column] + usize::from(left_character != right_character);
            let insertion = current[column] + 1;
            let deletion = previous[column + 1] + 1;
            current[column + 1] = substitution.min(insertion).min(deletion);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

#[cfg(test)]
mod tests {
    use super::{closest_match, humanize};

    #[test]
    fn matches_multi_word_with_underscores() {
        assert_eq!(
            closest_match("travle_budget", ["travel_budget", "emergency_fund"], 2),
            Some("travel_budget")
        );
        assert_eq!(humanize("travel_budget"), "travel budget");
    }

    #[test]
    fn tie_breaks_deterministically() {
        assert_eq!(closest_match("cat", ["bat", "hat"], 1), Some("bat"));
        assert_eq!(closest_match("zzz", ["cat"], 2), None);
    }
}
