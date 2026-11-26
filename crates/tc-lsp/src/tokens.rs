//! Monochrome semantic-token construction for live `.eng` documents.
//!
//! Highlights are derived from the lowered project trace and symbol index so
//! omitted prose, ACE anaphors, multi-word variables, fields, verbs, and
//! spoken numbers receive exact semantic spans.

use std::collections::{HashMap, HashSet};

use tc_lexicon::classify_verb;
use tc_semantics::{
    IntentVerdict, SentenceTrace, Token, TokenKind, is_english_number_word, parse_english_number,
    tokenize,
};

use crate::analysis::{Analysis, SymbolIndex};

/// Advertised LSP semantic token types.
pub(crate) const TOKEN_TYPES: [&str; 6] = [
    "keyword", "function", "variable", "string", "number", "comment",
];

/// Advertised LSP semantic token modifiers, in bit order.
pub(crate) const TOKEN_MODIFIERS: [&str; 3] = ["declaration", "readonly", "documentation"];

/// Keyword token type index.
pub(crate) const KEYWORD: u32 = 0;
/// Function token type index.
pub(crate) const FUNCTION: u32 = 1;
/// Variable token type index.
pub(crate) const VARIABLE: u32 = 2;
/// String token type index.
pub(crate) const STRING: u32 = 3;
/// Number token type index.
pub(crate) const NUMBER: u32 = 4;
/// Comment token type index.
pub(crate) const COMMENT: u32 = 5;

/// Declaration modifier bit.
pub(crate) const DECLARATION: u32 = 1 << 0;
/// Readonly anaphor modifier bit.
pub(crate) const READONLY: u32 = 1 << 1;
/// Omitted-prose documentation modifier bit.
pub(crate) const DOCUMENTATION: u32 = 1 << 2;

/// One UTF-16 positioned semantic token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SemanticToken {
    /// Zero-based line.
    pub line: u32,
    /// Zero-based UTF-16 start column.
    pub start: u32,
    /// UTF-16 length.
    pub length: u32,
    /// Token type legend index.
    pub token_type: u32,
    /// Token modifier bitset.
    pub modifiers: u32,
}

#[derive(Debug, Clone)]
struct Candidate {
    line: u32,
    start: usize,
    end: usize,
    token_type: u32,
    modifiers: u32,
    priority: u8,
}

#[derive(Debug, Default)]
struct LanguageIndex {
    variable_phrases: Vec<Vec<String>>,
    variable_words: HashSet<String>,
    field_phrases: Vec<Vec<String>>,
    field_words: HashSet<String>,
    phrase_heads: HashSet<String>,
}

impl LanguageIndex {
    fn new(symbols: &SymbolIndex) -> Self {
        let mut index = Self::default();
        for variable in &symbols.variables {
            index.add_variable(&variable.name);
        }
        for global in &symbols.globals {
            index.add_variable(&global.name);
        }
        for ty in &symbols.types {
            for field in &ty.fields {
                index.add_field(&field.name);
            }
        }
        for phrase in &symbols.phrases {
            if let Some(head) = phrase
                .split_whitespace()
                .find(|part| !part.starts_with('<'))
                .map(str::to_ascii_lowercase)
                .filter(|part| {
                    part.chars()
                        .all(|character| character.is_ascii_alphabetic())
                })
            {
                index.phrase_heads.insert(head);
            }
        }
        index.finish();
        index
    }

    fn add_variable(&mut self, name: &str) {
        add_name(name, &mut self.variable_phrases, &mut self.variable_words);
    }

    fn add_field(&mut self, name: &str) {
        add_name(name, &mut self.field_phrases, &mut self.field_words);
    }

    fn finish(&mut self) {
        sort_phrases(&mut self.variable_phrases);
        sort_phrases(&mut self.field_phrases);
    }
}

fn add_name(name: &str, phrases: &mut Vec<Vec<String>>, words: &mut HashSet<String>) {
    let normalized = name.to_ascii_lowercase().replace('_', " ");
    let parts = normalized
        .split_whitespace()
        .filter(|part| {
            part.chars()
                .all(|character| character.is_ascii_alphanumeric())
        })
        .map(str::to_string)
        .collect::<Vec<_>>();
    match parts.as_slice() {
        [] => {}
        [word] => {
            words.insert(word.clone());
        }
        _ => phrases.push(parts),
    }
}

fn sort_phrases(phrases: &mut Vec<Vec<String>>) {
    let mut seen = HashSet::new();
    phrases.retain(|phrase| seen.insert(phrase.join(" ")));
    phrases.sort_by(|left, right| {
        right
            .len()
            .cmp(&left.len())
            .then_with(|| left.join(" ").cmp(&right.join(" ")))
    });
}

/// Builds every semantic token for one document in strict delta order.
pub(crate) fn semantic_tokens(text: &str, analysis: &Analysis) -> Vec<SemanticToken> {
    semantic_tokens_for(text, &analysis.entry, &analysis.trace, &analysis.symbols)
}

/// Builds semantic tokens from an explicit trace and symbol index.
pub(crate) fn semantic_tokens_for(
    text: &str,
    entry_module: &str,
    trace: &[SentenceTrace],
    symbols: &SymbolIndex,
) -> Vec<SemanticToken> {
    let lines = text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect::<Vec<_>>();
    let index = LanguageIndex::new(symbols);
    let omitted = omitted_spans(&lines, entry_module, trace);
    let mut candidates = Vec::new();
    for (line_index, line) in lines.iter().enumerate() {
        let line_number = u32::try_from(line_index).unwrap_or(u32::MAX);
        if let Some((start, end)) = explicit_comment(line) {
            candidates.push(Candidate {
                line: line_number,
                start,
                end,
                token_type: COMMENT,
                modifiers: 0,
                priority: 100,
            });
            continue;
        }
        let comment_spans = omitted.get(&line_number).cloned().unwrap_or_default();
        let mut line_candidates = lexical_candidates(line, line_number, &index);
        line_candidates.retain(|candidate| {
            !comment_spans
                .iter()
                .any(|(start, end)| candidate.start < *end && candidate.end > *start)
        });
        candidates.extend(line_candidates);
        for (start, end) in comment_spans {
            candidates.push(Candidate {
                line: line_number,
                start,
                end,
                token_type: COMMENT,
                modifiers: DOCUMENTATION,
                priority: 100,
            });
        }
    }
    candidates.sort_by(|left, right| {
        left.line
            .cmp(&right.line)
            .then_with(|| left.start.cmp(&right.start))
            .then_with(|| right.end.cmp(&left.end))
            .then_with(|| right.priority.cmp(&left.priority))
    });
    let mut tokens = Vec::new();
    let mut last_line = 0_u32;
    let mut last_end = 0_usize;
    let mut has_last = false;
    for candidate in candidates {
        if candidate.end <= candidate.start {
            continue;
        }
        if has_last && candidate.line == last_line && candidate.start < last_end {
            continue;
        }
        let Some(line) = lines.get(candidate.line as usize) else {
            continue;
        };
        let start = line[..candidate.start].encode_utf16().count();
        let length = line[candidate.start..candidate.end].encode_utf16().count();
        let Ok(start) = u32::try_from(start) else {
            continue;
        };
        let Ok(length) = u32::try_from(length) else {
            continue;
        };
        if length == 0 {
            continue;
        }
        tokens.push(SemanticToken {
            line: candidate.line,
            start,
            length,
            token_type: candidate.token_type,
            modifiers: candidate.modifiers,
        });
        last_line = candidate.line;
        last_end = candidate.end;
        has_last = true;
    }
    tokens
}

fn omitted_spans(
    lines: &[&str],
    entry_module: &str,
    trace: &[SentenceTrace],
) -> HashMap<u32, Vec<(usize, usize)>> {
    let mut spans = HashMap::new();
    for entry in trace {
        if entry.module != entry_module
            || !matches!(entry.intent, IntentVerdict::CommentOmitted { .. })
        {
            continue;
        }
        let Ok(line_index) = usize::try_from(entry.line.saturating_sub(1)) else {
            continue;
        };
        let Some(line) = lines.get(line_index) else {
            continue;
        };
        let needle = entry.text.trim();
        if needle.is_empty() {
            continue;
        }
        let start = line
            .find(needle)
            .or_else(|| line.to_ascii_lowercase().find(&needle.to_ascii_lowercase()));
        let (start, mut end) = start.map_or_else(
            || {
                let start = line.len().saturating_sub(line.trim_start().len());
                (start, line.trim_end().len())
            },
            |start| (start, start + needle.len()),
        );
        if end > line.len() {
            end = line.trim_end().len();
        }
        let remainder = &line[end..];
        if remainder
            .chars()
            .all(|character| character.is_whitespace() || ".,;:!?'\"".contains(character))
        {
            end = line.trim_end().len();
        }
        if end > start {
            let line_number = u32::try_from(line_index).unwrap_or(u32::MAX);
            spans
                .entry(line_number)
                .or_insert_with(Vec::new)
                .push((start, end));
        }
    }
    spans
}

fn explicit_comment(line: &str) -> Option<(usize, usize)> {
    let start = line.len().saturating_sub(line.trim_start().len());
    let trimmed = &line[start..];
    (trimmed.starts_with('#') || trimmed.starts_with("//")).then(|| (start, line.trim_end().len()))
}

fn lexical_candidates(line: &str, line_number: u32, index: &LanguageIndex) -> Vec<Candidate> {
    let tokens = tokenize(line);
    if tokens.is_empty() {
        return Vec::new();
    }
    let conditional = tokens.iter().any(|token| {
        matches!(
            token.lower.as_str(),
            "if" | "unless" | "while" | "until" | "otherwise"
        )
    });
    let mut candidates = Vec::new();
    number_candidates(line, line_number, &tokens, &mut candidates);
    let mut index_position = 0;
    while index_position < tokens.len() {
        let token = &tokens[index_position];
        match token.kind {
            TokenKind::String => {
                push(
                    &mut candidates,
                    line_number,
                    token.start,
                    token.end,
                    STRING,
                    0,
                    90,
                );
            }
            TokenKind::Number => {
                push(
                    &mut candidates,
                    line_number,
                    token.start,
                    token.end,
                    NUMBER,
                    0,
                    80,
                );
            }
            TokenKind::Word => {
                word_candidates(
                    line_number,
                    &tokens,
                    index_position,
                    index,
                    conditional,
                    &mut candidates,
                );
            }
            TokenKind::Symbol => {}
        }
        index_position += 1;
    }
    if declaration_line(&tokens)
        && let Some(candidate) = candidates
            .iter_mut()
            .find(|candidate| candidate.token_type == VARIABLE)
    {
        candidate.modifiers |= DECLARATION;
    }
    candidates
}

#[allow(clippy::too_many_arguments)]
fn word_candidates(
    line_number: u32,
    tokens: &[Token],
    position: usize,
    index: &LanguageIndex,
    conditional: bool,
    candidates: &mut Vec<Candidate>,
) {
    let token = &tokens[position];
    let lower = token.lower.as_str();
    if lower == "the"
        && let Some(next) = tokens.get(position + 1)
        && next.is("result")
    {
        push(
            candidates,
            line_number,
            token.start,
            next.end,
            VARIABLE,
            READONLY,
            88,
        );
        return;
    }
    if matches!(lower, "it" | "its" | "them" | "their") {
        let mut end = token.end;
        if lower == "its"
            && let Some(next) = tokens.get(position + 1)
            && index.field_words.contains(&next.lower)
        {
            end = next.end;
        }
        push(
            candidates,
            line_number,
            token.start,
            end,
            VARIABLE,
            READONLY,
            87,
        );
        return;
    }
    let article_can_be_variable = tokens.get(position + 1).is_none_or(|next| {
        next.kind == TokenKind::Symbol
            || matches!(
                next.lower.as_str(),
                "is" | "be" | "and" | "or" | "to" | "in" | "from" | "plus" | "minus" | "times"
            )
    });
    if lower == "a"
        && token.text == "a"
        && index.variable_words.contains("a")
        && article_can_be_variable
    {
        push(
            candidates,
            line_number,
            token.start,
            token.end,
            VARIABLE,
            0,
            65,
        );
        return;
    }
    if is_control_keyword(tokens, position, conditional) {
        push(
            candidates,
            line_number,
            token.start,
            token.end,
            KEYWORD,
            0,
            30,
        );
        return;
    }
    if is_glue(lower) {
        return;
    }
    let clause_leader = is_clause_leader(tokens, position);
    if !clause_leader {
        if let Some(end) = match_phrase(tokens, position, &index.variable_phrases)
            .or_else(|| match_phrase(tokens, position, &index.field_phrases))
        {
            push(
                candidates,
                line_number,
                token.start,
                tokens[end - 1].end,
                VARIABLE,
                0,
                70,
            );
            return;
        }
        if let Some(end) = possessive_field_end(tokens, position, index) {
            push(
                candidates,
                line_number,
                token.start,
                tokens[end - 1].end,
                VARIABLE,
                0,
                75,
            );
            return;
        }
        if index.variable_words.contains(lower) || index.field_words.contains(lower) {
            push(
                candidates,
                line_number,
                token.start,
                token.end,
                VARIABLE,
                0,
                60,
            );
            return;
        }
    }
    if is_predicate_keyword(lower) {
        push(
            candidates,
            line_number,
            token.start,
            token.end,
            KEYWORD,
            0,
            25,
        );
        return;
    }
    if is_function(lower, index) {
        push(
            candidates,
            line_number,
            token.start,
            token.end,
            FUNCTION,
            0,
            50,
        );
        return;
    }
    if index.variable_words.contains(lower) || index.field_words.contains(lower) {
        push(
            candidates,
            line_number,
            token.start,
            token.end,
            VARIABLE,
            0,
            60,
        );
    }
}

fn number_candidates(
    line: &str,
    line_number: u32,
    tokens: &[Token],
    candidates: &mut Vec<Candidate>,
) {
    let mut position = 0;
    while position < tokens.len() {
        let token = &tokens[position];
        if !token.is_word() || token.is("and") || !is_english_number_word(&token.lower) {
            position += 1;
            continue;
        }
        let mut end = position;
        while let Some(next) = tokens.get(end + 1) {
            if next.is_word() && is_english_number_word(&next.lower) {
                if next.is("and") {
                    let Some(after) = tokens.get(end + 2) else {
                        break;
                    };
                    if !after.is_word() || !is_english_number_word(&after.lower) {
                        break;
                    }
                }
                end += 1;
                continue;
            }
            if next.text == "-"
                && let Some(after) = tokens.get(end + 2)
                && after.is_word()
                && is_english_number_word(&after.lower)
            {
                end += 2;
                continue;
            }
            break;
        }
        let phrase = &line[token.start..tokens[end].end];
        if parse_english_number(phrase).is_some() {
            push(
                candidates,
                line_number,
                token.start,
                tokens[end].end,
                NUMBER,
                0,
                80,
            );
        }
        position = end + 1;
    }
}

fn match_phrase(tokens: &[Token], start: usize, phrases: &[Vec<String>]) -> Option<usize> {
    phrases.iter().find_map(|phrase| {
        if start + phrase.len() > tokens.len() {
            return None;
        }
        phrase
            .iter()
            .enumerate()
            .all(|(offset, word)| {
                let token = &tokens[start + offset];
                token.is_word() && token.lower == *word
            })
            .then_some(start + phrase.len())
    })
}

fn possessive_field_end(tokens: &[Token], start: usize, index: &LanguageIndex) -> Option<usize> {
    let base = tokens.get(start)?;
    let possessive = tokens.get(start + 1)?;
    let field = tokens.get(start + 2)?;
    (base.is_word()
        && possessive.text == "'s"
        && field.is_word()
        && index.field_words.contains(&field.lower))
    .then_some(start + 3)
}

fn push(
    candidates: &mut Vec<Candidate>,
    line: u32,
    start: usize,
    end: usize,
    token_type: u32,
    modifiers: u32,
    priority: u8,
) {
    candidates.push(Candidate {
        line,
        start,
        end,
        token_type,
        modifiers,
        priority,
    });
}

fn declaration_line(tokens: &[Token]) -> bool {
    let first = tokens.first().map(|token| token.lower.as_str());
    let second = tokens.get(1).map(|token| token.lower.as_str());
    matches!(
        first,
        Some("let" | "set" | "put" | "given" | "assuming" | "create" | "update" | "to" | "phrase:")
    ) || (first == Some("for") && matches!(second, Some("each" | "every")))
        || tokens.iter().any(|token| token.is("where"))
}

fn is_glue(word: &str) -> bool {
    matches!(
        word,
        "the"
            | "of"
            | "a"
            | "an"
            | "by"
            | "me"
            | "out"
            | "is"
            | "be"
            | "are"
            | "was"
            | "to"
            | "from"
            | "for"
            | "in"
            | "as"
            | "at"
            | "with"
            | "into"
            | "onto"
    )
}

fn is_control_keyword(tokens: &[Token], position: usize, conditional: bool) -> bool {
    let lower = tokens[position].lower.as_str();
    if matches!(lower, "and" | "or") {
        return conditional;
    }
    if lower == "for" {
        return tokens
            .get(position + 1)
            .is_some_and(|next| next.is("each") || next.is("every"));
    }
    if lower == "to" {
        return is_clause_leader(tokens, position);
    }
    matches!(
        lower,
        "if" | "otherwise"
            | "else"
            | "while"
            | "until"
            | "repeat"
            | "each"
            | "every"
            | "end"
            | "where"
            | "given"
            | "assuming"
            | "then"
            | "let"
            | "put"
            | "set"
            | "keep"
            | "use"
            | "return"
            | "record"
            | "phrase"
            | "unless"
            | "give"
            | "back"
    )
}

fn is_predicate_keyword(word: &str) -> bool {
    matches!(
        word,
        "least"
            | "most"
            | "no"
            | "less"
            | "more"
            | "than"
            | "even"
            | "odd"
            | "positive"
            | "negative"
            | "zero"
            | "nonzero"
            | "divisible"
            | "between"
            | "empty"
            | "exceeds"
            | "equals"
            | "differs"
            | "not"
            | "times"
    )
}

fn is_clause_leader(tokens: &[Token], position: usize) -> bool {
    let mut index = 0;
    while index < position
        && matches!(
            tokens[index].lower.as_str(),
            "please" | "now" | "first" | "next" | "finally" | "also" | "just"
        )
    {
        index += 1;
    }
    index == position
}

fn is_function(word: &str, index: &LanguageIndex) -> bool {
    index.phrase_heads.contains(word)
        || matches!(
            word,
            "print"
                | "show"
                | "tell"
                | "display"
                | "output"
                | "echo"
                | "say"
                | "add"
                | "remove"
                | "swap"
                | "square"
                | "cube"
                | "double"
                | "halve"
                | "negate"
                | "update"
                | "create"
                | "sum"
                | "average"
                | "minimum"
                | "maximum"
        )
        || classify_verb(word, None).is_some()
}
