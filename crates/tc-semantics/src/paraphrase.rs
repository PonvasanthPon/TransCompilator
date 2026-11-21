//! Deterministic paraphrase echo of the exact semantic decisions made while
//! lowering English into Core-7.
//!
//! The paraphraser never re-parses source text.  Entries are recorded at the
//! moment an anaphor is bound, a condition tree is built, or a phrase target is
//! selected, so the echo cannot disagree with the compiler.

use std::fmt::Write as _;

use crate::intent::IntentVerdict;
use crate::{Span, token::tokenize};

/// One sentence-level paraphrase record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParaphraseEntry {
    /// Source location of the sentence.
    pub span: Span,
    /// Original cleaned English sentence.
    pub original: String,
    /// Final three-way intent verdict.
    pub verdict: IntentVerdict,
    /// Canonical resolved rendering (empty for omitted/unsupported text).
    pub canonical: String,
    /// Explicit discourse resolutions, e.g. `[it -> cash]`.
    pub resolutions: Vec<String>,
}

/// All paraphrase entries for one module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParaphraseDoc {
    /// Module name.
    pub module: String,
    /// Entries in source order.
    pub entries: Vec<ParaphraseEntry>,
}

/// Renders all modules with explicit headers.
#[must_use]
pub fn format_paraphrase(docs: &[ParaphraseDoc]) -> String {
    let mut output = String::new();
    for doc in docs {
        let _ = writeln!(output, "=== Paraphrase (module: {}) ===", doc.module);
        output.push_str(&format_entries(&doc.entries));
    }
    output
}

/// Renders only the entry lines, without module headers.
#[must_use]
pub fn format_paraphrase_entries(entries: &[ParaphraseEntry]) -> String {
    format_entries(entries)
}

fn format_entries(entries: &[ParaphraseEntry]) -> String {
    let mut output = String::new();
    for entry in entries {
        let _ = write!(output, "[L{}] ", entry.span.line);
        match &entry.verdict {
            IntentVerdict::Exec => {
                let canonical = if entry.canonical.is_empty() {
                    normalized_original(&entry.original)
                } else {
                    entry.canonical.clone()
                };
                output.push_str(&canonical);
                for annotation in &entry.resolutions {
                    output.push(' ');
                    output.push_str(annotation);
                }
            }
            IntentVerdict::CommentOmitted { reason } => {
                let _ = write!(
                    output,
                    "[OMITTED: {reason}] {}",
                    with_period(&entry.original)
                );
            }
            IntentVerdict::Unsupported { reason } => {
                let _ = write!(
                    output,
                    "[UNSUPPORTED: {reason}] {}",
                    with_period(&entry.original)
                );
            }
        }
        output.push('\n');
    }
    output
}

fn with_period(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.ends_with(['.', '!', '?', ':', ';']) {
        trimmed.to_string()
    } else {
        format!("{trimmed}.")
    }
}

fn normalized_original(text: &str) -> String {
    let trimmed = text.trim().trim_end_matches(['.', ',', ';', '!', '?']);
    if trimmed.is_empty() {
        return String::new();
    }
    let mut characters = trimmed.chars();
    let first = characters.next().unwrap_or_default();
    let mut output = first.to_uppercase().collect::<String>();
    output.extend(characters);
    output.push('.');
    output
}

/// Produces a canonical assignment/mutation rendering when the resolved
/// sentence uses a familiar computational verb shape.
#[must_use]
pub fn canonicalize_statement(text: &str) -> String {
    let trimmed = text.trim().trim_end_matches(['.', ',', ';', '!', '?']);
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.to_ascii_lowercase().starts_with("for every ")
        || trimmed.to_ascii_lowercase().starts_with("for each ")
    {
        return trimmed.to_string();
    }
    let first = crate::textutil::first_word(trimmed);
    let resolution = tc_lexicon::classify_verb(&first, None);
    if resolution
        .as_ref()
        .is_some_and(|value| value.predicate == Some(tc_lexicon::VerbPredicate::Assign))
    {
        let tokens = tokenize(trimmed);
        let arguments_start = tokens.first().map_or(0, |token| token.end);
        let arguments = trimmed[arguments_start..].trim();
        if let Some((start, end)) = crate::textutil::find_top_level_word(arguments, "in")
            .or_else(|| crate::textutil::find_top_level_word(arguments, "into"))
        {
            let value = arguments[..start].trim();
            let target = arguments[end..].trim();
            if !value.is_empty() && !target.is_empty() {
                return format!("Set {target} to {value}.");
            }
        }
        if let Some((start, end)) = crate::textutil::find_top_level_word(arguments, "to") {
            let before = arguments[..start].trim();
            let after = arguments[end..].trim();
            if !before.is_empty() && !after.is_empty() {
                if simple_identifier(after) && !simple_identifier(before) {
                    return format!("Set {after} to {before}.");
                }
                return format!("Set {before} to {after}.");
            }
        }
    }
    if first == "let" {
        if let Some((start, end)) = crate::textutil::find_top_level_word(trimmed, "be") {
            let target = trimmed[4..start].trim();
            let value = trimmed[end..].trim();
            if !target.is_empty() && !value.is_empty() {
                return format!("Set {target} to {value}.");
            }
        }
    }
    for keyword in ["is", "becomes", "equals"] {
        if let Some((start, end)) = crate::textutil::find_top_level_word(trimmed, keyword) {
            let target = trimmed[..start].trim();
            let value = trimmed[end..].trim();
            if !target.is_empty()
                && !value.is_empty()
                && !target.contains(char::is_whitespace)
                && !matches!(
                    target.to_ascii_lowercase().as_str(),
                    "it" | "its" | "he" | "she" | "they" | "we" | "i"
                )
            {
                return format!("Set {target} to {value}.");
            }
        }
    }
    normalized_original(trimmed)
}

fn simple_identifier(text: &str) -> bool {
    let tokens = tokenize(text);
    tokens.len() == 1
        && tokens[0].kind == crate::TokenKind::Word
        && tokens[0]
            .text
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}
