//! Deterministic discourse-level program synthesis patterns.
//!
//! This module recognizes paragraph-level range introductions, referential loop
//! continuations, post-loop closers, and existential `check whether` sentences.
//! It never invents semantics: callers lower the resulting structures through
//! the existing quantifier, condition, and value machinery.

use crate::quantifier::{GlobalQuantifier, QuantDomainSpec};
use crate::textutil::find_top_level_word;
use crate::token::{TokenKind, decode_string_token, tokenize};

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

/// A running extremum tracker over emitted values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LargestTracker {
    /// True for maximum, false for minimum.
    pub maximum: bool,
}

/// Parses `keep track of the largest value you printed` / `track the minimum`.
pub(crate) fn parse_largest_tracker(text: &str) -> Option<LargestTracker> {
    let lower = trimmed_lower(text);
    let words = lower.split_whitespace().collect::<Vec<_>>();
    let first = words.first().copied()?;
    if !matches!(
        first,
        "keep" | "track" | "record" | "remember" | "maintain" | "store"
    ) {
        return None;
    }
    if !words.iter().any(|word| {
        matches!(
            *word,
            "largest"
                | "biggest"
                | "maximum"
                | "max"
                | "greatest"
                | "smallest"
                | "least"
                | "minimum"
                | "min"
        )
    }) {
        return None;
    }
    let maximum = !words
        .iter()
        .any(|word| matches!(*word, "smallest" | "least" | "minimum" | "min" | "lowest"));
    Some(LargestTracker { maximum })
}

fn singular_word(word: &str) -> String {
    if word.ends_with("ies") && word.len() > 3 {
        format!("{}y", &word[..word.len() - 3])
    } else if (word.ends_with("ses")
        || word.ends_with("xes")
        || word.ends_with("zes")
        || word.ends_with("ches")
        || word.ends_with("shes"))
        && word.len() > 3
    {
        word[..word.len() - 2].to_string()
    } else if word.ends_with('s') && !word.ends_with("ss") && word.len() > 1 {
        word[..word.len() - 1].to_string()
    } else {
        word.to_string()
    }
}

/// Parses `Imagine a red box starting with 5 and a blue box starting with 16.`
/// into named starting values.
pub(crate) fn parse_state_declaration(text: &str) -> Option<Vec<(String, String)>> {
    let tokens = tokenize(text);
    let first = tokens.first()?;
    if !matches!(
        first.lower.as_str(),
        "imagine" | "suppose" | "assume" | "consider"
    ) {
        return None;
    }
    let rest = text[first.end..].trim();
    let clauses = split_top_level_list(rest, "and");
    if clauses.len() < 2 {
        return None;
    }
    let mut declarations = Vec::new();
    for clause in clauses {
        let lower = clause.to_ascii_lowercase();
        let (marker, marker_len) = ["starting with", "holding", "containing", "with"]
            .iter()
            .find_map(|marker| lower.find(marker).map(|index| (*marker, index)))?;
        let name_text = clause[..marker_len].trim();
        let value = clause[marker_len + marker.len()..]
            .trim()
            .trim_end_matches(['.', ',', ':'])
            .trim();
        let words = tokenize(name_text)
            .into_iter()
            .filter(tc_syntax::Token::is_word)
            .map(|token| token.lower)
            .filter(|word| !matches!(word.as_str(), "a" | "an" | "the" | "some" | "one"))
            .collect::<Vec<_>>();
        if words.is_empty() || value.is_empty() {
            return None;
        }
        let last = words.last().map(|word| singular_word(word))?;
        let mut name_words = words.clone();
        if let Some(last_word) = name_words.last_mut() {
            *last_word = last;
        }
        declarations.push((name_words.join("_"), value.to_string()));
    }
    Some(declarations)
}

/// Parses a loop-wide move guard such as `if a move would ever cause either box
/// to drop below 5, skip that move`.
pub(crate) fn parse_move_guard(text: &str) -> Option<String> {
    let lower = trimmed_lower(text);
    let lower = lower
        .strip_prefix("however, ")
        .or_else(|| lower.strip_prefix("however "))
        .unwrap_or(&lower)
        .trim();
    if !lower.starts_with("if ") || !lower.contains("move") {
        return None;
    }
    if !(lower.contains("skip that move")
        || lower.contains("skip the move")
        || lower.contains("skip it")
        || lower.contains("leave both boxes unchanged")
        || lower.contains("leave the boxes unchanged"))
    {
        return None;
    }
    let (_, end) = [
        "drop below",
        "fall below",
        "go below",
        "dip below",
        "become less than",
    ]
    .iter()
    .find_map(|phrase| top_level_phrase(text, phrase))?;
    let floor = text[end..]
        .trim()
        .trim_end_matches(['.', ',', ':'])
        .split(',')
        .next()
        .unwrap_or_default()
        .trim();
    (!floor.is_empty()).then(|| floor.to_string())
}

/// Parses `Stop immediately as soon as <condition>`.
pub(crate) fn parse_stop_condition(text: &str) -> Option<String> {
    let lower = trimmed_lower(text);
    if !(lower.starts_with("stop ") || lower.starts_with("halt ") || lower == "stop") {
        return None;
    }
    let (_, end) = ["as soon as", "as early as", "when", "once", "if"]
        .iter()
        .find_map(|phrase| top_level_phrase(text, phrase))?;
    let condition = text[end..].trim().trim_end_matches(['.', ',', ':']).trim();
    (!condition.is_empty()).then(|| condition.to_string())
}

/// One parsed discourse loop introduction.
#[derive(Debug, Clone)]
pub(crate) struct LoopIntroduction {
    /// Existing global quantifier shape.
    pub quantifier: GlobalQuantifier,
}

/// A generated sequence introduced by `Start with a and b`.
#[derive(Debug, Clone)]
pub(crate) struct SequenceSeed {
    /// Initial terms, in order.
    pub values: Vec<String>,
}

/// Recurrence rule for a generated sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SequenceRecurrence {
    /// Next term is the sum of the previous two.
    SumPreviousTwo,
}

/// One parsed sequence-generation rule.
#[derive(Debug, Clone)]
pub(crate) struct SequenceGeneration {
    /// Recurrence rule.
    pub recurrence: SequenceRecurrence,
    /// Exclusive or inclusive upper bound for generated terms.
    pub bound: String,
    /// True when `bound` itself is allowed.
    pub inclusive: bool,
}

/// One parsed iteration over an entire generated sequence.
#[derive(Debug, Clone)]
pub(crate) struct SequenceIteration {
    /// Loop variable name.
    pub var: String,
    /// Inline body action, if present.
    pub body: Option<String>,
}

fn split_top_level_list(text: &str, separator: &str) -> Vec<String> {
    let tokens = tokenize(text);
    let mut depth = 0_i32;
    let mut parts = Vec::new();
    let mut start = 0;
    for token in &tokens {
        if token.kind == TokenKind::Symbol {
            match token.text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                "," if depth == 0 && separator == "," => {
                    parts.push(text[start..token.start].trim().to_string());
                    start = token.end;
                }
                _ => {}
            }
        } else if depth == 0 && token.is_word() && token.is(separator) {
            parts.push(text[start..token.start].trim().to_string());
            start = token.end;
        }
    }
    parts.push(text[start..].trim().to_string());
    parts.retain(|part| !part.is_empty());
    parts
}

/// Parses `Line up 6 switches in a circle with the starting states on, off, ...`.
pub(crate) fn parse_switch_setup(text: &str) -> Option<Vec<bool>> {
    let lower = trimmed_lower(text);
    if !(lower.starts_with("line up ")
        || lower.starts_with("arrange ")
        || lower.starts_with("place ")
        || lower.starts_with("put "))
    {
        return None;
    }
    if !lower.contains("switch") || !lower.contains("circle") {
        return None;
    }
    let (_, end) = top_level_phrase(text, "starting states")?;
    let mut states = Vec::new();
    for token in tokenize(&text[end..]) {
        if token.is("on") || token.is("true") {
            states.push(true);
        } else if token.is("off") || token.is("false") {
            states.push(false);
        }
    }
    (states.len() >= 2).then_some(states)
}

/// Parses `Run 5 update rounds.`
pub(crate) fn parse_switch_rounds(text: &str) -> Option<usize> {
    let lower = trimmed_lower(text);
    if !(lower.starts_with("run ") || lower.starts_with("perform ") || lower.starts_with("do ")) {
        return None;
    }
    if !lower.contains("round") {
        return None;
    }
    let value = tokenize(text).into_iter().find_map(|token| {
        token
            .text
            .parse::<i64>()
            .ok()
            .or_else(|| crate::number_words::parse(&token.text))
    })?;
    usize::try_from(value).ok()
}

/// True for switch-rule or history sentences that can be skipped before the report.
pub(crate) fn is_switch_rule(text: &str) -> bool {
    let lower = trimmed_lower(text);
    lower.contains("neighbor")
        || lower.contains("neighbour")
        || lower.contains("simultaneously")
        || lower.contains("history of the full row")
        || lower.contains("matches an earlier state")
}

/// True for the final switch-simulation report.
pub(crate) fn is_switch_report(text: &str) -> bool {
    let lower = trimmed_lower(text);
    lower.contains("how many switches")
        && (lower.contains("repeating round number")
            || lower.contains("first repeating round")
            || lower.contains("repeat round"))
}

/// Parses `Inspect the text "..."` and returns the quoted literal.
pub(crate) fn parse_inspect_text(text: &str) -> Option<String> {
    let lower = trimmed_lower(text);
    if !(lower.starts_with("inspect the text ")
        || lower.starts_with("scan the text ")
        || lower.starts_with("examine the text ")
        || lower.starts_with("read the text "))
    {
        return None;
    }
    let token = tokenize(text)
        .into_iter()
        .find(|token| token.kind == TokenKind::String)?;
    let bytes = decode_string_token(&token.text)?;
    String::from_utf8(bytes).ok()
}

/// True for a bracket-scan rule sentence that can be skipped before the report.
pub(crate) fn is_bracket_rule(text: &str) -> bool {
    let lower = trimmed_lower(text);
    lower.contains("opening bracket")
        || lower.contains("closing bracket")
        || lower.contains("open brackets")
        || lower.contains("deepest level")
        || lower.contains("unmatched")
}

/// True for the final bracket-scan report sentence.
pub(crate) fn is_bracket_report(text: &str) -> bool {
    let lower = trimmed_lower(text);
    lower.contains("fails to match") && lower.contains("position") && lower.contains("deepest")
}

/// A circle of consecutive numbers.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CircleSetup {
    /// First number.
    pub start: i64,
    /// Last number.
    pub end: i64,
}

/// A count-and-remove rule for a circle.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CircleRemoval {
    /// Optional starting number, defaulting to the first circle entry.
    pub start_at: Option<i64>,
    /// Count step.
    pub step: i64,
}

fn parse_integer_token(token: &str) -> Option<i64> {
    token
        .parse::<i64>()
        .ok()
        .or_else(|| crate::number_words::parse(token))
}

fn parse_ordinal(token: &str) -> Option<usize> {
    let lower = token.to_ascii_lowercase();
    let digits = lower
        .strip_suffix("st")
        .or_else(|| lower.strip_suffix("nd"))
        .or_else(|| lower.strip_suffix("rd"))
        .or_else(|| lower.strip_suffix("th"));
    if let Some(digits) = digits
        && let Ok(value) = digits.parse::<usize>()
    {
        return (value > 0).then_some(value);
    }
    let value = match lower.as_str() {
        "first" => 1,
        "second" => 2,
        "third" => 3,
        "fourth" => 4,
        "fifth" => 5,
        "sixth" => 6,
        "seventh" => 7,
        "eighth" => 8,
        "ninth" => 9,
        "tenth" => 10,
        "eleventh" => 11,
        "twelfth" => 12,
        "thirteenth" => 13,
        "fourteenth" => 14,
        "fifteenth" => 15,
        "sixteenth" => 16,
        "seventeenth" => 17,
        "eighteenth" => 18,
        "nineteenth" => 19,
        "twentieth" => 20,
        _ => return None,
    };
    Some(value)
}

/// Parses `Arrange the numbers from 1 to 10 in a circle.`
pub(crate) fn parse_circle_arrangement(text: &str) -> Option<CircleSetup> {
    let lower = trimmed_lower(text);
    if !(lower.starts_with("arrange the numbers ")
        || lower.starts_with("arrange numbers ")
        || lower.starts_with("place the numbers ")
        || lower.starts_with("place numbers ")
        || lower.starts_with("put the numbers ")
        || lower.starts_with("put numbers "))
    {
        return None;
    }
    if !lower.contains("circle") {
        return None;
    }
    let (_, from_end) = top_level_phrase(text, "from")?;
    let after_from = &text[from_end..];
    let (to_start, _) = top_level_phrase(after_from, "to")?;
    let start_text = after_from[..to_start].trim();
    let end_text = tokenize(&after_from[to_start + 2..])
        .into_iter()
        .find(|token| token.kind == TokenKind::Number || token.is_word())?
        .text;
    let start = parse_integer_token(start_text)?;
    let end = parse_integer_token(&end_text)?;
    (start <= end).then_some(CircleSetup { start, end })
}

/// Parses `Starting at 1, count around the circle and remove every third
/// remaining number until only one number is left.`
pub(crate) fn parse_circle_removal(text: &str) -> Option<CircleRemoval> {
    let lower = trimmed_lower(text);
    if !lower.contains("circle") || !lower.contains("remove every") {
        return None;
    }
    let start_at = top_level_phrase(text, "starting at")
        .or_else(|| top_level_phrase(text, "start at"))
        .and_then(|(_, end)| {
            tokenize(&text[end..])
                .into_iter()
                .find(|token| token.kind == TokenKind::Number || token.is_word())
                .and_then(|token| parse_integer_token(&token.text))
        });
    let (_, end) = top_level_phrase(text, "remove every")?;
    let token = tokenize(&text[end..])
        .into_iter()
        .find(|token| token.kind == TokenKind::Number || token.is_word())?;
    let step = parse_ordinal(&token.text)
        .and_then(|value| i64::try_from(value).ok())
        .or_else(|| parse_integer_token(&token.text))?;
    (step > 0).then_some(CircleRemoval { start_at, step })
}

/// Parses the ordinal in `print the fourth number that was removed ...`.
pub(crate) fn parse_circle_report(text: &str) -> Option<usize> {
    let lower = trimmed_lower(text);
    if !lower.contains("removed") {
        return None;
    }
    if !(lower.contains("single number left")
        || lower.contains("only one number left")
        || lower.contains("last number left")
        || lower.contains("one number remaining"))
    {
        return None;
    }
    let tokens = tokenize(text);
    let removed = tokens.iter().position(|token| token.is("removed"))?;
    tokens
        .iter()
        .take(removed)
        .rev()
        .filter(|token| token.is_word() || token.kind == TokenKind::Number)
        .find_map(|token| parse_ordinal(&token.text))
}

/// One cardinal grid direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GridDirection {
    North,
    East,
    South,
    West,
}

/// One relative turn on the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GridTurn {
    Right,
    Left,
    Around,
}

/// One literal grid move clause.
#[derive(Debug, Clone)]
pub(crate) struct GridMove {
    /// Optional turn before walking.
    pub turn: Option<GridTurn>,
    /// Forward steps after the turn.
    pub steps: i64,
}

/// Starting pose for a literal grid walk.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GridStart {
    /// Starting east-west coordinate.
    pub x: i64,
    /// Starting north-south coordinate.
    pub y: i64,
    /// Starting facing direction.
    pub facing: GridDirection,
}

fn parse_grid_direction(word: &str) -> Option<GridDirection> {
    match word {
        "north" | "up" => Some(GridDirection::North),
        "east" | "rightward" => Some(GridDirection::East),
        "south" | "down" => Some(GridDirection::South),
        "west" | "leftward" => Some(GridDirection::West),
        _ => None,
    }
}

/// Parses `Imagine standing at the center of a grid, facing north.`
pub(crate) fn parse_grid_start(text: &str) -> Option<GridStart> {
    let lower = trimmed_lower(text);
    if !(lower.starts_with("imagine standing ")
        || lower.starts_with("imagine you are standing ")
        || lower.starts_with("start standing ")
        || lower.starts_with("begin standing "))
    {
        return None;
    }
    let (x, y) = if lower.contains("center") || lower.contains("centre") || lower.contains("origin")
    {
        (0, 0)
    } else {
        return None;
    };
    let (_, end) = top_level_phrase(text, "facing")?;
    let direction = tokenize(&text[end..])
        .into_iter()
        .find(tc_syntax::Token::is_word)
        .map(|token| token.lower)?;
    let facing = parse_grid_direction(&direction)?;
    Some(GridStart { x, y, facing })
}

/// Parses `Follow these moves in order: walk forward 3 steps, turn right ...`.
pub(crate) fn parse_grid_moves(text: &str) -> Option<Vec<GridMove>> {
    let lower = trimmed_lower(text);
    if !(lower.starts_with("follow these moves ")
        || lower.starts_with("follow the moves ")
        || lower.starts_with("make these moves ")
        || lower.starts_with("perform these moves ")
        || lower.starts_with("follow this path "))
    {
        return None;
    }
    let colon = text.find(':')?;
    let rest = &text[colon + 1..];
    let clauses = split_top_level_list(rest, ",");
    if clauses.is_empty() {
        return None;
    }
    let mut moves = Vec::new();
    for clause in clauses {
        let mut clause = clause.trim();
        for prefix in ["and finally ", "and then ", "and ", "finally ", "then "] {
            if let Some(after) = clause.to_ascii_lowercase().strip_prefix(prefix) {
                clause = &clause[clause.len() - after.len()..];
                break;
            }
        }
        let lower_clause = clause.to_ascii_lowercase();
        let turn = if lower_clause.contains("turn right") || lower_clause.contains("turn clockwise")
        {
            Some(GridTurn::Right)
        } else if lower_clause.contains("turn left")
            || lower_clause.contains("turn counterclockwise")
            || lower_clause.contains("turn anticlockwise")
        {
            Some(GridTurn::Left)
        } else if lower_clause.contains("turn around") || lower_clause.contains("turn back") {
            Some(GridTurn::Around)
        } else {
            None
        };
        let mut steps = 0;
        for phrase in [
            "walk forward ",
            "walk ",
            "move forward ",
            "move ",
            "go forward ",
            "step forward ",
        ] {
            if let Some((_, end)) = top_level_phrase(clause, phrase) {
                if let Some(token) = tokenize(&clause[end..])
                    .into_iter()
                    .find(|token| token.is_word() || token.kind == TokenKind::Number)
                {
                    steps = token
                        .text
                        .parse::<i64>()
                        .ok()
                        .or_else(|| crate::number_words::parse(&token.text))
                        .unwrap_or(0);
                }
                break;
            }
        }
        if turn.is_none() && steps == 0 {
            return None;
        }
        moves.push(GridMove { turn, steps });
    }
    Some(moves)
}

/// True for the final grid-walk report sentence.
pub(crate) fn is_grid_report(text: &str) -> bool {
    let lower = trimmed_lower(text);
    lower.contains("crossed your own path")
        && lower.contains("east-west")
        && lower.contains("north-south")
}

/// Parses `Take the sentence "..."` and returns the quoted literal.
pub(crate) fn parse_sentence_literal(text: &str) -> Option<String> {
    let lower = trimmed_lower(text);
    if !(lower.starts_with("take the sentence ")
        || lower.starts_with("take this sentence ")
        || lower.starts_with("consider the sentence ")
        || lower.starts_with("here is the sentence ")
        || lower.starts_with("given the sentence "))
    {
        return None;
    }
    let token = tokenize(text)
        .into_iter()
        .find(|token| token.kind == TokenKind::String)?;
    let bytes = decode_string_token(&token.text)?;
    String::from_utf8(bytes).ok()
}

/// True for `Split it into words, and count how many times each word appears.`
pub(crate) fn is_word_split_request(text: &str) -> bool {
    let lower = trimmed_lower(text);
    lower.contains("split") && lower.contains("words")
}

/// True for a separate `count how many times each word appears` sentence.
pub(crate) fn is_word_count_request(text: &str) -> bool {
    let lower = trimmed_lower(text);
    lower.contains("count") && lower.contains("how many times") && lower.contains("word")
}

/// Parses `Ignore any word that has fewer than 4 letters, unless it appears
/// more than once.` into `(min_letters, keep_repeated)`.
pub(crate) fn parse_word_filter(text: &str) -> Option<(usize, bool)> {
    let lower = trimmed_lower(text);
    if !(lower.starts_with("ignore ")
        || lower.starts_with("exclude ")
        || lower.starts_with("skip ")
        || lower.starts_with("filter out "))
    {
        return None;
    }
    if !lower.contains("word") {
        return None;
    }
    let (_, end) = ["fewer than", "less than", "shorter than"]
        .iter()
        .find_map(|phrase| top_level_phrase(text, phrase))?;
    let word = tokenize(&text[end..])
        .into_iter()
        .find(|token| token.is_word() || token.kind == TokenKind::Number)?
        .text;
    let minimum = word
        .parse::<i64>()
        .ok()
        .or_else(|| crate::number_words::parse(&word))?;
    let minimum = usize::try_from(minimum).ok()?;
    let keep_repeated = lower.contains("unless it appears more than once")
        || lower.contains("unless it occurs more than once")
        || lower.contains("unless repeated")
        || lower.contains("unless it is repeated");
    Some((minimum, keep_repeated))
}

/// True for the final word-frequency output request.
pub(crate) fn is_word_output_request(text: &str) -> bool {
    let lower = trimmed_lower(text);
    lower.contains("remaining words")
        && lower.contains("print")
        && (lower.contains("most times")
            || lower.contains("most often")
            || lower.contains("most frequently")
            || lower.contains("highest count"))
}

/// A list introduced with inline starting values.
#[derive(Debug, Clone)]
pub(crate) struct NamedListStart {
    /// Variable name.
    pub name: String,
    /// Comma/and separated element text.
    pub elements: String,
}

/// Parses `Start with the list of numbers 14, 7, ..., and 18.`
pub(crate) fn parse_named_list_start(text: &str) -> Option<NamedListStart> {
    let lower = trimmed_lower(text);
    let rest = lower
        .strip_prefix("start with ")
        .or_else(|| lower.strip_prefix("begin with "))
        .or_else(|| lower.strip_prefix("start from "))?;
    if !rest.contains("list") {
        return None;
    }
    let mut body = rest
        .strip_prefix("the list of ")
        .or_else(|| rest.strip_prefix("a list of "))
        .or_else(|| rest.strip_prefix("the list "))
        .or_else(|| rest.strip_prefix("a list "))?;
    let mut name = "items".to_string();
    for noun in [
        "whole numbers ",
        "numbers ",
        "values ",
        "items ",
        "integers ",
    ] {
        if let Some(after) = body.strip_prefix(noun) {
            name = noun.trim().replace(' ', "_");
            body = after;
            break;
        }
    }
    let elements = body.trim();
    if elements.is_empty() || !(elements.contains(',') || elements.contains(" and ")) {
        return None;
    }
    Some(NamedListStart {
        name,
        elements: elements.to_string(),
    })
}

/// One adjacent-pair list pass.
#[derive(Debug, Clone)]
pub(crate) struct AdjacentPass;

/// Parses `Go through the list from left to right, comparing each number with
/// the one immediately after it.`
pub(crate) fn parse_adjacent_pass(text: &str) -> Option<AdjacentPass> {
    let lower = trimmed_lower(text);
    if !(lower.starts_with("go through ")
        || lower.starts_with("walk through ")
        || lower.starts_with("scan ")
        || lower.starts_with("iterate over ")
        || lower.starts_with("loop over ")
        || lower.starts_with("pass through "))
    {
        return None;
    }
    if !lower.contains("list") {
        return None;
    }
    let (_, _end) = top_level_phrase(text, "comparing each ")?;
    Some(AdjacentPass)
}

/// Parses the post-pass filter sentence, returning its predicate text.
pub(crate) fn parse_pass_cleanup(text: &str) -> Option<String> {
    let lower = trimmed_lower(text);
    let mut rest = lower.as_str();
    for opener in [
        "after finishing that pass, ",
        "after finishing the pass, ",
        "after that pass, ",
        "after the pass, ",
        "once that pass is finished, ",
        "once the pass is finished, ",
    ] {
        if let Some(after) = rest.strip_prefix(opener) {
            rest = after;
            break;
        }
    }
    if !(rest.starts_with("remove ")
        || rest.starts_with("delete ")
        || rest.starts_with("filter out ")
        || rest.starts_with("drop "))
    {
        return None;
    }
    if !(rest.contains("from the list")
        || rest.contains("from it")
        || rest.contains("from the numbers")
        || rest.contains("from the values")
        || rest.contains("from the items"))
    {
        return None;
    }
    let (_, end) = ["that is ", "which is ", "that are ", "which are "]
        .iter()
        .find_map(|phrase| top_level_phrase(text, phrase))?;
    let predicate = text[end..].trim().trim_end_matches(['.', ',', ':']).trim();
    (!predicate.is_empty()).then(|| predicate.to_string())
}

/// Parses `Start with 1 and 2.`
pub(crate) fn parse_sequence_seed(text: &str) -> Option<SequenceSeed> {
    let tokens = tokenize(text);
    let [first, second, ..] = tokens.as_slice() else {
        return None;
    };
    if !(first.is("start") || first.is("begin")) || !(second.is("with") || second.is("from")) {
        return None;
    }
    let rest = text[second.end..].trim();
    let values = split_top_level_list(rest, "and");
    if values.len() < 2 {
        return None;
    }
    Some(SequenceSeed { values })
}

/// Parses `Keep generating the next number by adding the previous two numbers
/// together, as long as the new number does not exceed 200.`
pub(crate) fn parse_sequence_generation(text: &str) -> Option<SequenceGeneration> {
    let lower = trimmed_lower(text);
    if !(lower.starts_with("keep generating ")
        || lower.starts_with("keep going ")
        || lower.starts_with("continue generating ")
        || lower.starts_with("keep building ")
        || lower.starts_with("generate ")
        || lower.starts_with("continue "))
    {
        return None;
    }
    let by = find_top_level_word(text, "by")?;
    let after_by = &text[by.1..];
    let separator = ["as long as", "while", "until"]
        .iter()
        .find_map(|phrase| top_level_phrase(after_by, phrase))
        .or_else(|| {
            let tokens = tokenize(after_by);
            tokens
                .iter()
                .find(|token| token.text == ",")
                .map(|token| (token.start, token.end))
        })?;
    let recurrence_text = after_by[..separator.0].trim().to_ascii_lowercase();
    let adding = recurrence_text.contains("adding")
        || recurrence_text.contains("add ")
        || recurrence_text.contains("sum of");
    let previous_two = recurrence_text.contains("previous two")
        || recurrence_text.contains("two previous")
        || recurrence_text.contains("last two")
        || recurrence_text.contains("two last");
    if !(adding && previous_two) {
        return None;
    }
    let condition = after_by[separator.1..]
        .trim()
        .trim_end_matches(['.', ',', ':'])
        .trim();
    parse_sequence_bound(condition)
}

const SEQUENCE_BOUND_PREFIXES: [(&str, bool); 7] = [
    ("does not exceed ", true),
    ("is at most ", true),
    ("is no more than ", true),
    ("is not greater than ", true),
    ("is less than ", false),
    ("is under ", false),
    ("exceeds ", false),
];

fn parse_sequence_bound(condition: &str) -> Option<SequenceGeneration> {
    let mut lower = condition.to_ascii_lowercase();
    for prefix in [
        "the new number ",
        "the next number ",
        "the new value ",
        "the next value ",
        "the result ",
        "it ",
    ] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            lower = rest.to_string();
            break;
        }
    }
    for (prefix, inclusive) in SEQUENCE_BOUND_PREFIXES {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let bound = condition[condition.len() - rest.len()..]
                .trim()
                .trim_end_matches(['.', ',', ':'])
                .trim();
            return (!bound.is_empty()).then(|| SequenceGeneration {
                recurrence: SequenceRecurrence::SumPreviousTwo,
                bound: bound.to_string(),
                inclusive,
            });
        }
    }
    None
}

/// Parses `For each number in this entire sequence, including the starting two,
/// add it to a running total.`
pub(crate) fn parse_sequence_iteration(text: &str) -> Option<SequenceIteration> {
    let tokens = tokenize(text);
    if !tokens.first().is_some_and(|token| token.is("for"))
        || !tokens
            .get(1)
            .is_some_and(|token| token.is("each") || token.is("every"))
    {
        return None;
    }
    let in_index = tokens
        .iter()
        .enumerate()
        .skip(2)
        .find(|(_, token)| token.is("in"))
        .map(|(index, _)| index)?;
    let var_words = tokens[2..in_index]
        .iter()
        .filter(|token| token.is_word())
        .map(|token| token.lower.clone())
        .collect::<Vec<_>>();
    let var = singular_referent(&var_words)?;
    let sequence_index = tokens
        .iter()
        .enumerate()
        .skip(in_index + 1)
        .find(|(_, token)| token.is("sequence") || token.is("series"))
        .map(|(index, _)| index)?;
    let comma_index = tokens
        .iter()
        .enumerate()
        .skip(sequence_index + 1)
        .find(|(_, token)| token.text == ",")
        .map(|(index, _)| index)?;
    let after_comma = &text[tokens[comma_index].end..];
    let body = if after_comma
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("including ")
    {
        let next_comma = tokenize(after_comma)
            .into_iter()
            .find(|token| token.text == ",")?;
        after_comma[next_comma.end..].trim()
    } else {
        after_comma.trim()
    };
    let body = (!body.is_empty()).then(|| body.to_string());
    Some(SequenceIteration { var, body })
}

const FILTER_PRONOUNS: [&str; 6] = ["it", "they", "he", "she", "we", "you"];
const FILTER_DETERMINERS: [&str; 10] = [
    "any", "all", "every", "each", "the", "those", "these", "a", "an", "also",
];
const FILTER_GENERIC_NOUNS: [&str; 12] = [
    "number", "numbers", "value", "values", "item", "items", "element", "elements", "one", "ones",
    "entry", "entries",
];

fn predicate_condition(var: &str, phrase: &str) -> Option<String> {
    let mut words = phrase
        .split_whitespace()
        .map(|word| word.trim_matches([',', '.', ':']).to_ascii_lowercase())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    if words
        .first()
        .is_some_and(|word| FILTER_PRONOUNS.contains(&word.as_str()))
    {
        words.remove(0);
    }
    if words
        .first()
        .is_some_and(|word| matches!(word.as_str(), "is" | "are"))
    {
        words.remove(0);
    }
    while words
        .first()
        .is_some_and(|word| FILTER_DETERMINERS.contains(&word.as_str()))
    {
        words.remove(0);
    }
    if words
        .first()
        .is_some_and(|word| matches!(word.as_str(), "that" | "which"))
    {
        words.remove(0);
    }
    if words
        .first()
        .is_some_and(|word| FILTER_GENERIC_NOUNS.contains(&word.as_str()))
    {
        words.remove(0);
        if words
            .first()
            .is_some_and(|word| matches!(word.as_str(), "that" | "which"))
        {
            words.remove(0);
        }
    }
    if words
        .last()
        .is_some_and(|word| FILTER_GENERIC_NOUNS.contains(&word.as_str()))
    {
        words.pop();
    }
    if words.is_empty() {
        return None;
    }
    let phrase = words.join(" ");
    let first = words.first().map(String::as_str).unwrap_or_default();
    if matches!(
        first,
        "divisible"
            | "greater"
            | "less"
            | "more"
            | "smaller"
            | "larger"
            | "equal"
            | "between"
            | "odd"
            | "even"
            | "positive"
            | "negative"
            | "zero"
            | "true"
            | "false"
            | "is"
            | "are"
    ) {
        if first == "is" || first == "are" {
            return Some(format!("{var} is {}", words[1..].join(" ")));
        }
        return Some(format!("{var} is {phrase}"));
    }
    if let Some(rest) = phrase.strip_prefix("multiple of ") {
        return Some(format!("{var} is a multiple of {rest}"));
    }
    if let Some(rest) = phrase.strip_prefix("multiples of ") {
        return Some(format!("{var} is a multiple of {rest}"));
    }
    if let Some(rest) = phrase.strip_prefix("factor of ") {
        return Some(format!("{var} is a factor of {rest}"));
    }
    Some(format!("{var} is {phrase}"))
}

/// Parses `Skip any multiple of 4, unless it is also divisible by 6.` into a
/// guard condition that keeps the element when the guard is true.
pub(crate) fn parse_loop_filter(text: &str, var: &str) -> Option<String> {
    let tokens = tokenize(text);
    let first = tokens.first()?;
    if first.kind != TokenKind::Word {
        return None;
    }
    let filter_end = if first.is("leave") || first.is("filter") || first.is("throw") {
        let second = tokens.get(1)?;
        second.is("out").then_some(second.end)?
    } else if matches!(
        first.lower.as_str(),
        "skip" | "ignore" | "exclude" | "omit" | "drop" | "bypass"
    ) {
        first.end
    } else {
        return None;
    };
    let body = text[filter_end..].trim();
    if body.is_empty() {
        return None;
    }
    let (base_text, exception_text) =
        if let Some((start, end)) = find_top_level_word(body, "unless") {
            (body[..start].trim(), Some(body[end..].trim()))
        } else {
            (body, None)
        };
    let base = predicate_condition(var, base_text)?;
    let Some(exception_text) = exception_text else {
        return Some(format!("not ({base})"));
    };
    let exception = predicate_condition(var, exception_text)?;
    Some(format!("not ({base}) or {exception}"))
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

fn singular_referent(words: &[String]) -> Option<String> {
    const DETERMINERS: [&str; 6] = ["the", "a", "an", "of", "all", "remaining"];
    let word = words
        .iter()
        .rev()
        .find(|word| !DETERMINERS.contains(&word.as_str()))?;
    let singular = if word.ends_with("ies") && word.len() > 3 {
        format!("{}y", &word[..word.len() - 3])
    } else if (word.ends_with("ses")
        || word.ends_with("xes")
        || word.ends_with("zes")
        || word.ends_with("ches")
        || word.ends_with("shes"))
        && word.len() > 3
    {
        word[..word.len() - 2].to_string()
    } else if word.ends_with('s') && !word.ends_with("ss") && word.len() > 1 {
        word[..word.len() - 1].to_string()
    } else {
        word.clone()
    };
    (!singular.is_empty()).then_some(singular)
}

/// Parses `Look at every x from a up to b.` / `each x in list` /
/// `Take all the numbers between a and b`.
pub(crate) fn parse_loop_introduction(text: &str) -> Option<LoopIntroduction> {
    let tokens = tokenize(text);
    let quantifier_index = tokens.iter().position(|token| {
        token.kind == TokenKind::Word && (token.is("every") || token.is("each") || token.is("all"))
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
        "look", "at", "consider", "examine", "take", "go", "through", "check", "run", "process",
        "work", "walk", "across", "over",
    ];
    if opener.is_empty() || !opener.iter().all(|word| allowed.contains(word)) {
        return None;
    }
    let domain_index = tokens
        .iter()
        .enumerate()
        .skip(quantifier_index + 1)
        .find(|(_, token)| token.is("from") || token.is("in") || token.is("between"))
        .map(|(index, _)| index)?;
    let var_words = tokens
        .get(quantifier_index + 1..domain_index)?
        .iter()
        .filter(|token| token.is_word())
        .map(|token| token.lower.clone())
        .collect::<Vec<_>>();
    let var = singular_referent(&var_words)?;
    let domain_token = &tokens[domain_index];
    if domain_token.is("in") {
        let list_words = tokens
            .get(domain_index + 1..)?
            .iter()
            .filter(|token| token.is_word())
            .map(|token| token.lower.clone())
            .filter(|word| !matches!(word.as_str(), "the" | "a" | "an"))
            .collect::<Vec<_>>();
        let list = (!list_words.is_empty()).then(|| list_words.join("_"))?;
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
    let (start, end) = if domain_token.is("between") {
        let (low_end, high_start) = find_top_level_word(rest, "and")?;
        let start = rest[..low_end].trim();
        let end = rest[high_start..]
            .trim()
            .trim_end_matches(['.', ',', ':'])
            .trim();
        (!start.is_empty() && !end.is_empty()).then(|| (start.to_string(), end.to_string()))?
    } else {
        parse_range_bounds(rest)?
    };
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

/// Strips `For each <var>,` / `For every remaining <var>,` when it continues
/// an active discourse loop without a new domain.
pub(crate) fn continuation_rest(text: &str, var: &str) -> Option<String> {
    let tokens = tokenize(text);
    if !tokens.first().is_some_and(|token| token.is("for"))
        || !tokens
            .get(1)
            .is_some_and(|token| token.is("each") || token.is("every"))
    {
        return None;
    }
    let comma = tokens.iter().position(|token| token.text == ",")?;
    if comma <= 2 {
        return None;
    }
    let words = tokens[2..comma]
        .iter()
        .filter(|token| token.is_word())
        .map(|token| token.lower.clone())
        .collect::<Vec<_>>();
    let name = singular_referent(&words)?;
    if name != var && name.replace('_', " ") != var {
        return None;
    }
    let rest = text[tokens[comma].end..].trim();
    (!rest.is_empty()).then(|| rest.to_string())
}

fn top_level_comma(text: &str) -> Option<(usize, usize)> {
    let tokens = tokenize(text);
    let mut depth = 0_i32;
    for token in &tokens {
        if token.kind == TokenKind::Symbol {
            match token.text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                "," if depth == 0 => return Some((token.start, token.end)),
                _ => {}
            }
        }
    }
    None
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
        "at the very end",
        "at the end",
        "in the end",
        "finally",
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
    let (comma_start, comma_end) = top_level_comma(text)?;
    let prefix = trimmed_lower(&text[..comma_start]);
    let words = prefix.split_whitespace().collect::<Vec<_>>();
    let opener = words.first().copied()?;
    if !matches!(opener, "once" | "when" | "after") {
        return None;
    }
    if !words
        .get(1)
        .is_some_and(|word| *word == "you" || *word == "we")
    {
        return None;
    }
    let completion = words.iter().skip(2).any(|word| {
        matches!(
            *word,
            "done" | "done," | "finished" | "finish" | "complete" | "completed" | "through"
        )
    });
    if !completion {
        return None;
    }
    let rest = text[comma_end..].trim();
    (!rest.is_empty()).then(|| rest.to_string())
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
