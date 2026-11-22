//! Plain-English suggestions shared by lowering diagnostics.

/// Normalizes a name so underscores and spaces compare identically.
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
pub(crate) fn closest_match<'a>(
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
pub(crate) fn humanize(name: &str) -> String {
    name.replace('_', " ")
}

/// Returns ` — did you mean '...'?` when a close candidate exists.
pub(crate) fn hint_for<'a>(query: &str, candidates: impl IntoIterator<Item = &'a str>) -> String {
    closest_match(query, candidates, 2).map_or_else(String::new, |candidate| {
        format!(" — did you mean '{}'?", humanize(candidate))
    })
}

/// Returns true for finite control words that may not name user declarations.
pub(crate) fn is_reserved_control_word(word: &str) -> bool {
    matches!(
        word,
        "stop"
            | "if"
            | "else"
            | "otherwise"
            | "while"
            | "until"
            | "repeat"
            | "loop"
            | "return"
            | "break"
            | "continue"
            | "use"
            | "import"
    )
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
