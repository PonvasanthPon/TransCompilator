//! Strict three-way intent gate: `EXEC`, `COMMENT_OMITTED`, or `UNSUPPORTED`.
//!
//! Omission is decided from grammar and computational evidence only.  No list
//! of prose nouns exists anywhere in this module or in the rest of the crate.

use std::fmt;

use tc_lexicon::{
    ClosedClass, SemanticConcept, VerbPredicate, VerbResolution, classify_verb,
    lookup_closed_class, prep_role,
};

use crate::phrases::PhraseMatch;
use crate::textutil::{
    find_top_level_symbol, find_top_level_word, first_word, is_lvalue_text, split_where,
    starts_with_word,
};
use crate::token::{Token, TokenKind, tokenize};
use crate::{ClauseTree, Voice};

/// Verdict for one English sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntentVerdict {
    /// Lower this sentence into Core-7.
    Exec,
    /// Deliberate narrative or filler text; omit from the program.
    CommentOmitted {
        /// Why the sentence was omitted.
        reason: String,
    },
    /// Computational-looking text with no supported Core-7 lowering.
    Unsupported {
        /// Loud explanation shown by `tc trace` and compilation.
        reason: String,
    },
}

impl IntentVerdict {
    /// Returns true for [`IntentVerdict::Exec`].
    #[must_use]
    pub fn is_exec(&self) -> bool {
        matches!(self, Self::Exec)
    }

    /// Returns true for [`IntentVerdict::CommentOmitted`].
    #[must_use]
    pub fn is_comment(&self) -> bool {
        matches!(self, Self::CommentOmitted { .. })
    }

    /// Returns true for [`IntentVerdict::Unsupported`].
    #[must_use]
    pub fn is_unsupported(&self) -> bool {
        matches!(self, Self::Unsupported { .. })
    }

    /// Returns the explanatory reason for non-execution verdicts.
    #[must_use]
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Exec => None,
            Self::CommentOmitted { reason } | Self::Unsupported { reason } => Some(reason),
        }
    }
}

impl fmt::Display for IntentVerdict {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exec => formatter.write_str("EXEC"),
            Self::CommentOmitted { reason } => write!(formatter, "COMMENT_OMITTED ({reason})"),
            Self::Unsupported { reason } => write!(formatter, "UNSUPPORTED ({reason})"),
        }
    }
}

fn is_control_anchor(word: &str) -> bool {
    matches!(
        word,
        "import"
            | "struct"
            | "union"
            | "global"
            | "function"
            | "phrase"
            | "if"
            | "else"
            | "otherwise"
            | "unless"
            | "while"
            | "repeat"
            | "loop"
            | "break"
            | "continue"
            | "return"
            | "syscall"
            | "trap"
            | "define"
            | "public"
            | "private"
    )
}

struct VerbHit {
    resolution: VerbResolution,
    index: usize,
}

fn is_auxiliary_word(word: &str) -> bool {
    matches!(
        word,
        "am" | "is"
            | "are"
            | "was"
            | "were"
            | "be"
            | "been"
            | "being"
            | "has"
            | "have"
            | "had"
            | "do"
            | "does"
            | "did"
    )
}

fn next_preposition(tokens: &[Token], start: usize) -> Option<String> {
    tokens.iter().skip(start).find_map(|token| {
        if token.kind == TokenKind::Word
            && matches!(
                lookup_closed_class(&token.lower),
                Some(ClosedClass::Preposition)
            )
        {
            Some(token.lower.clone())
        } else {
            None
        }
    })
}

fn first_computational_verb(text: &str) -> Option<VerbHit> {
    let tokens = tokenize(text);
    for (index, token) in tokens.iter().enumerate() {
        if token.kind != TokenKind::Word || is_auxiliary_word(&token.lower) {
            continue;
        }
        let prep = next_preposition(&tokens, index + 1);
        let with_prep = prep
            .as_deref()
            .and_then(|prep| classify_verb(&token.lower, Some(prep)));
        let Some(resolution) = with_prep.or_else(|| classify_verb(&token.lower, None)) else {
            continue;
        };
        if resolution.predicate.is_some() || resolution.concept.is_some() {
            return Some(VerbHit { resolution, index });
        }
    }
    None
}

fn verb_region_is_empty(text: &str, verb_index: usize) -> bool {
    let tokens = tokenize(text);
    tokens
        .iter()
        .skip(verb_index + 1)
        .all(|token| token.kind == TokenKind::Symbol)
}

fn target_side_lvalue(text: &str, verb_index: usize) -> bool {
    let tokens = tokenize(text);
    let Some(prep_index) = tokens.iter().enumerate().find_map(|(index, token)| {
        (index > verb_index
            && token.kind == TokenKind::Word
            && matches!(
                lookup_closed_class(&token.lower),
                Some(ClosedClass::Preposition)
            ))
        .then_some(index)
    }) else {
        return false;
    };
    let after_verb = tokens
        .get(verb_index + 1)
        .map_or(text.len(), |token| token.start);
    let before_prep = text[after_verb..tokens[prep_index].start].trim();
    let after_prep = text[tokens[prep_index].end..].trim();
    (!before_prep.is_empty() && is_lvalue_text(before_prep))
        || (!after_prep.is_empty() && is_lvalue_text(after_prep))
}
fn has_preposition(text: &str) -> bool {
    tokenize(text).iter().any(|token| {
        token.kind == TokenKind::Word
            && matches!(
                lookup_closed_class(&token.lower),
                Some(ClosedClass::Preposition)
            )
    })
}

fn assignment_rhs_starts_with_verb(rhs: &str) -> bool {
    let all_tokens = tokenize(rhs);
    if all_tokens
        .iter()
        .filter(|token| token.kind == TokenKind::Word)
        .count()
        < 2
    {
        return false;
    }
    let first_index = all_tokens
        .iter()
        .position(|token| token.kind == TokenKind::Word);
    let Some(first_index) = first_index else {
        return false;
    };
    if all_tokens
        .get(first_index + 1)
        .is_some_and(|token| token.text == "<")
    {
        return false;
    }
    let first = all_tokens[first_index].clone();
    if is_auxiliary_word(&first.lower) {
        return false;
    }
    let tokens = tokenize(rhs);
    let prep = next_preposition(&tokens, 1);
    let resolution = prep
        .as_deref()
        .and_then(|prep| classify_verb(&first.lower, Some(prep)))
        .or_else(|| classify_verb(&first.lower, None));
    resolution.is_some_and(|value| value.predicate.is_some() || value.concept.is_some())
}

fn assignment_shape(text: &str) -> bool {
    let (core, _) = split_where(text);
    for symbol in ["+=", "-=", "*=", "/="] {
        if let Some((start, end)) = find_top_level_symbol(&core, symbol) {
            let lhs = core[..start].trim();
            let rhs = core[end..].trim();
            return is_lvalue_text(lhs) && !rhs.is_empty();
        }
    }
    if starts_with_word(&core, "let") {
        if let Some((start, end)) = find_top_level_word(&core, "be") {
            let lhs = core[4..start].trim();
            let rhs = core[end..].trim();
            return is_lvalue_text(lhs) && !rhs.is_empty();
        }
    }
    for keyword in ["is", "becomes", "equals"] {
        if let Some((start, end)) = find_top_level_word(&core, keyword) {
            let lhs = core[..start].trim();
            let rhs = core[end..].trim();
            if lhs.is_empty() || rhs.is_empty() {
                continue;
            }
            if !is_lvalue_text(lhs) || lhs.chars().any(char::is_whitespace) {
                continue;
            }
            let left_lower = first_word(lhs);
            if left_lower != "i"
                && matches!(lookup_closed_class(&left_lower), Some(ClosedClass::Pronoun))
            {
                continue;
            }
            if assignment_rhs_starts_with_verb(rhs) {
                continue;
            }
            return true;
        }
    }
    if let Some((start, end)) = find_top_level_symbol(&core, "=") {
        let lhs = core[..start].trim();
        let rhs = core[end..].trim();
        if is_lvalue_text(lhs) && !rhs.is_empty() {
            return true;
        }
    }
    false
}

fn supported_predicate(resolution: &VerbResolution, text: &str, verb_index: usize) -> bool {
    let Some(predicate) = resolution.predicate else {
        return supported_concept(resolution, text, verb_index);
    };
    let has_prep = has_preposition(text);
    match predicate {
        VerbPredicate::Assign => has_prep && target_side_lvalue(text, verb_index),
        VerbPredicate::TransferAdd | VerbPredicate::TransferSub => {
            has_prep && target_side_lvalue(text, verb_index)
        }
        VerbPredicate::TransferMove => {
            let tokens = tokenize(text);
            let roles = tokens
                .iter()
                .filter_map(|token| {
                    (token.kind == TokenKind::Word)
                        .then(|| prep_role(&token.lower))
                        .flatten()
                })
                .collect::<Vec<_>>();
            roles.contains(&tc_lexicon::PrepRole::Source)
                && roles.contains(&tc_lexicon::PrepRole::Destination)
        }
        VerbPredicate::Combine => has_prep && target_side_lvalue(text, verb_index),
        VerbPredicate::Transform => match resolution.concept {
            Some(SemanticConcept::Double | SemanticConcept::Halve | SemanticConcept::Negate) => {
                !verb_region_is_empty(text, verb_index)
            }
            Some(SemanticConcept::Mul | SemanticConcept::Div) => has_prep,
            _ => has_prep,
        },
        VerbPredicate::Create
        | VerbPredicate::Destroy
        | VerbPredicate::Emit
        | VerbPredicate::Receive => false,
    }
}

fn supported_concept(resolution: &VerbResolution, text: &str, verb_index: usize) -> bool {
    match resolution.concept {
        Some(
            SemanticConcept::Add
            | SemanticConcept::Sub
            | SemanticConcept::Assign
            | SemanticConcept::Transfer,
        ) => has_preposition(text) || !verb_region_is_empty(text, verb_index),
        Some(SemanticConcept::Mul | SemanticConcept::Div) => has_preposition(text),
        Some(SemanticConcept::Double | SemanticConcept::Halve | SemanticConcept::Negate) => {
            !verb_region_is_empty(text, verb_index)
        }
        Some(
            SemanticConcept::Mod
            | SemanticConcept::Output
            | SemanticConcept::Input
            | SemanticConcept::Swap
            | SemanticConcept::CompareGt
            | SemanticConcept::CompareLt
            | SemanticConcept::CompareEq
            | SemanticConcept::CompareNe
            | SemanticConcept::Loop
            | SemanticConcept::Branch
            | SemanticConcept::Return
            | SemanticConcept::Create
            | SemanticConcept::Destroy,
        )
        | None => false,
    }
}

fn first_word_hit(text: &str) -> Option<VerbHit> {
    let tokens = tokenize(text);
    let (index, token) = tokens
        .iter()
        .enumerate()
        .find(|(_, token)| token.kind == TokenKind::Word)?;
    if is_auxiliary_word(&token.lower) || is_control_anchor(&token.lower) {
        return None;
    }
    let prep = next_preposition(&tokens, index + 1);
    let resolution = prep
        .as_deref()
        .and_then(|prep| classify_verb(&token.lower, Some(prep)))
        .or_else(|| classify_verb(&token.lower, None))?;
    (resolution.predicate.is_some() || resolution.concept.is_some())
        .then_some(VerbHit { resolution, index })
}

fn clause_verb_hit(text: &str, clause: &ClauseTree) -> Option<VerbHit> {
    let verb = clause.verb.as_ref()?;
    if is_auxiliary_word(&verb.raw.to_ascii_lowercase()) {
        return None;
    }
    let tokens = tokenize(text);
    let verb_index = tokens
        .iter()
        .position(|token| token.is_word() && token.is(&verb.raw.to_ascii_lowercase()))
        .unwrap_or(0);
    let prep = next_preposition(&tokens, verb_index + 1);
    let resolution = prep
        .as_deref()
        .and_then(|prep| classify_verb(&verb.raw, Some(prep)))
        .or_else(|| classify_verb(&verb.raw, None))?;
    (resolution.predicate.is_some() || resolution.concept.is_some()).then_some(VerbHit {
        resolution,
        index: verb_index,
    })
}

fn main_verb_hit(text: &str, clause: &ClauseTree) -> Option<VerbHit> {
    if let Some(hit) = first_word_hit(text) {
        return Some(hit);
    }
    clause_verb_hit(text, clause)
}

fn supported_computational_operation(text: &str, clause: &ClauseTree) -> Option<VerbHit> {
    let hit = main_verb_hit(text, clause)?;
    let supported = if hit.resolution.predicate.is_some() {
        supported_predicate(&hit.resolution, text, hit.index)
    } else {
        supported_concept(&hit.resolution, text, hit.index)
    };
    supported.then_some(hit)
}

fn narrative(clause: &ClauseTree) -> bool {
    if clause.verb.as_ref().is_some_and(|verb| verb.is_past) {
        return true;
    }
    if clause.voice == Voice::Imperative {
        return false;
    }
    let Some(subject) = clause.subject.as_ref() else {
        return false;
    };
    let Some(first) = subject.iter().find(|token| token.is_word()) else {
        return false;
    };
    matches!(
        lookup_closed_class(&first.lower),
        Some(ClosedClass::Pronoun)
    ) && matches!(
        first.lower.as_str(),
        "i" | "we" | "he" | "she" | "it" | "they" | "you"
    )
}

fn unsupported_reason(text: &str) -> String {
    if let Some(hit) = first_computational_verb(text) {
        let verb = hit.resolution.lemma;
        let predicate = hit
            .resolution
            .predicate
            .map_or_else(|| "no-predicate".to_string(), |value| format!("{value:?}"));
        return format!(
            "computational verb `{verb}` ({predicate}) has no Core-7 lowering; declare a Phrase: signature or use an assignment/transfer/arithmetic form"
        );
    }
    if let Some(verb) = clause_verb_word(text) {
        return format!(
            "imperative verb `{verb}` is not in the computational lexicon; add a Phrase: signature or a supported Core-7 form"
        );
    }
    "directive is outside the finite Core-7 English subset".to_string()
}

fn clause_verb_word(text: &str) -> Option<String> {
    let tokens = tokenize(text);
    let first = tokens.iter().find(|token| token.is_word())?;
    if is_control_anchor(&first.lower) {
        return None;
    }
    Some(first.lower.clone())
}

fn is_call_expression(text: &str) -> bool {
    let tokens = tokenize(text);
    let Some(open) = tokens.iter().position(|token| token.text == "(") else {
        return false;
    };
    if open == 0 || open + 1 >= tokens.len() {
        return false;
    }
    let callee = &tokens[open - 1];
    if callee.kind != TokenKind::Word || callee.end != tokens[open].start {
        return false;
    }
    let significant = tokens
        .iter()
        .rposition(|token| !matches!(token.text.as_str(), "." | ","))
        .unwrap_or(0);
    if tokens[significant].text != ")" {
        return false;
    }
    let mut depth = 0_i32;
    for token in &tokens[open..] {
        if token.text == "(" {
            depth += 1;
        } else if token.text == ")" {
            depth -= 1;
            if depth < 0 {
                return false;
            }
        }
    }
    depth == 0
}

/// Classifies one sentence using only grammar, structural anchors, lexicon
/// evidence, and in-scope declarative phrases.
#[must_use]
pub fn classify(text: &str, clause: &ClauseTree, phrase: Option<&PhraseMatch>) -> IntentVerdict {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return IntentVerdict::CommentOmitted {
            reason: "empty sentence".to_string(),
        };
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower == "end" || lower == "stop" {
        return IntentVerdict::CommentOmitted {
            reason: "explicit block closer".to_string(),
        };
    }
    if lower.starts_with("phrase:") {
        return IntentVerdict::CommentOmitted {
            reason: "phrase declaration".to_string(),
        };
    }
    let first = first_word(trimmed);
    if is_control_anchor(&first) {
        return IntentVerdict::Exec;
    }
    if phrase.is_some() {
        return IntentVerdict::Exec;
    }
    if is_call_expression(trimmed) {
        return IntentVerdict::Exec;
    }
    let (core, _) = split_where(trimmed);
    let core = core.as_str();
    if assignment_shape(core) {
        return IntentVerdict::Exec;
    }
    if supported_computational_operation(core, clause).is_some() {
        return IntentVerdict::Exec;
    }
    if let Some(hit) = main_verb_hit(core, clause) {
        if clause.voice != Voice::Imperative && narrative(clause) {
            return IntentVerdict::CommentOmitted {
                reason: "past-tense or third-person narrative prose".to_string(),
            };
        }
        let reason = format!(
            "computational verb `{}` ({:?}) has no Core-7 lowering; declare a Phrase: signature or use an assignment/transfer/arithmetic form",
            hit.resolution.lemma, hit.resolution.predicate
        );
        return IntentVerdict::Unsupported { reason };
    }
    if clause.voice == Voice::Imperative && clause.verb.is_some() {
        let reason = unsupported_reason(trimmed);
        return IntentVerdict::Unsupported { reason };
    }
    IntentVerdict::CommentOmitted {
        reason: "no computational directive".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::IntentVerdict;
    use crate::LinkGrammarParser;
    use crate::{RawSentence, Span, analyze_clause};

    fn verdict(text: &str) -> IntentVerdict {
        let parser = LinkGrammarParser::new().expect("parser");
        let sentence = RawSentence {
            text: text.to_string(),
            raw: text.to_string(),
            span: Span::new(1, 1),
            fillers: Vec::new(),
        };
        let clause = analyze_clause(&parser, &sentence);
        super::classify(text, &clause, None)
    }

    #[test]
    fn narrative_is_omitted_and_directive_executes() {
        assert!(verdict("I had 2 cups of coffee.").is_comment());
        assert!(verdict("Yesterday Ravi called me twice.").is_comment());
        assert!(verdict("i is 1").is_exec());
        assert!(verdict("If x is 1").is_exec());
        assert!(verdict("Start a new thread.").is_unsupported());
    }
}
