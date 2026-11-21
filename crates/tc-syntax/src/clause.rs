//! Dependency-tree normalization into a finite set of clause roles.

use std::fmt;

use tc_lexicon::{ClosedClass, classify_verb, lemmatize, lookup_closed_class};

use crate::Span;
use crate::lg_ffi::{LgLink, LgParse, LinkGrammarParser};
use crate::segment::RawSentence;
use crate::token::{Token, TokenKind, tokenize};

/// Grammatical voice of a clause.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Voice {
    /// Subject performs the action.
    Active,
    /// Subject receives the action.
    Passive,
    /// Subject is omitted.
    Imperative,
}

/// One prepositional phrase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepPhrase {
    /// Lowercase preposition, with `out of` normalized to `out_of`.
    pub prep: String,
    /// Tokens following the preposition.
    pub tokens: Vec<Token>,
}

/// The main verb of a clause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerbHead {
    /// Original spelling reported by Link Grammar or the tokenizer.
    pub raw: String,
    /// Deterministic lemma.
    pub lemma: String,
    /// True when the form is marked past tense by the parser.
    pub is_past: bool,
}

/// Normalized clause structure shared by intent classification and lowering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClauseTree {
    /// Cleaned sentence text.
    pub raw: String,
    /// Source location.
    pub span: Span,
    /// All lexical tokens in source order.
    pub tokens: Vec<Token>,
    /// Voice.
    pub voice: Voice,
    /// Subject words, if any.
    pub subject: Option<Vec<Token>>,
    /// Main verb, if one was found.
    pub verb: Option<VerbHead>,
    /// Direct object words, if any.
    pub direct_object: Option<Vec<Token>>,
    /// Prepositional phrases.
    pub prep_phrases: Vec<PrepPhrase>,
    /// Number of words reported by Link Grammar, excluding the two walls.
    pub lg_words: usize,
    /// Number of dependency links reported by Link Grammar.
    pub lg_links: usize,
    /// True when Link Grammar produced a complete linkage.
    pub lg_used: bool,
}

impl fmt::Display for Voice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Active => "active",
            Self::Passive => "passive",
            Self::Imperative => "imperative",
        })
    }
}

fn strip_annotations(word: &str) -> String {
    let mut result = String::new();
    let mut depth = 0_i32;
    for character in word.chars() {
        match character {
            '[' => depth += 1,
            ']' => depth = (depth - 1).max(0),
            _ if depth == 0 => result.push(character),
            _ => {}
        }
    }
    result
}

fn split_tag(word: &str) -> (String, String) {
    let cleaned = strip_annotations(word);
    if let Some((head, suffix)) = cleaned.rsplit_once('.') {
        let looks_like_tag = suffix.chars().next().is_some_and(|character| {
            matches!(
                character,
                'v' | 'n' | 'a' | 'r' | 'p' | 'd' | 's' | 'q' | 'e'
            )
        }) && suffix
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-');
        if looks_like_tag {
            return (head.to_string(), suffix.to_string());
        }
    }
    (cleaned, String::new())
}

fn base_word(word: &str) -> String {
    let (head, _) = split_tag(word);
    head
}

fn word_is_past(word: &str) -> bool {
    let (head, suffix) = split_tag(word);
    suffix.contains('d') || head.ends_with("ed")
}

fn auxiliary(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "am" | "is" | "are" | "was" | "were" | "be" | "been" | "being" | "has" | "have" | "had"
    )
}

fn is_preposition(word: &str) -> bool {
    matches!(lookup_closed_class(word), Some(ClosedClass::Preposition))
}

fn token_for_lg_word(word: &str, tokens: &[Token], cursor: &mut usize) -> Option<Token> {
    let base = base_word(word);
    let lower = base.to_ascii_lowercase();
    if base.is_empty() {
        return None;
    }
    for (index, token) in tokens.iter().enumerate().skip(*cursor) {
        if token.lower == lower {
            *cursor = index + 1;
            return Some(token.clone());
        }
    }
    Some(Token {
        text: base.clone(),
        lower,
        kind: TokenKind::Word,
        start: 0,
        end: 0,
    })
}

fn subject_indices(links: &[LgLink]) -> Vec<usize> {
    links
        .iter()
        .filter(|link| link.label.starts_with('S'))
        .map(|link| link.left.min(link.right))
        .collect()
}

fn object_indices(links: &[LgLink]) -> Vec<usize> {
    links
        .iter()
        .filter(|link| link.label.starts_with('O'))
        .map(|link| link.left.max(link.right))
        .collect()
}

fn passive_link(links: &[LgLink]) -> Option<&LgLink> {
    links
        .iter()
        .find(|link| link.label.starts_with("Pv") || link.label.starts_with("PV"))
}

fn wall_verb(links: &[LgLink]) -> Option<usize> {
    links
        .iter()
        .find(|link| link.left == 0 && link.label.starts_with('W'))
        .map(|link| link.right)
}

fn first_tagged_verb(words: &[String]) -> Option<usize> {
    words.iter().enumerate().find_map(|(index, word)| {
        let (_, suffix) = split_tag(word);
        (index > 0 && suffix.starts_with('v')).then_some(index)
    })
}

fn sentence_verb_index(parse: &LgParse, subjects: &[usize], passive: bool) -> Option<usize> {
    if passive {
        if let Some(link) = passive_link(&parse.links) {
            return Some(link.right.max(link.left));
        }
    }
    if let Some(subject) = subjects.first() {
        if let Some(link) = parse
            .links
            .iter()
            .find(|link| link.label.starts_with('S') && link.left.min(link.right) == *subject)
        {
            return Some(link.left.max(link.right));
        }
    }
    wall_verb(&parse.links).or_else(|| first_tagged_verb(&parse.words))
}

fn collect_from_indices(indices: &[usize], parse: &LgParse, tokens: &[Token]) -> Vec<Token> {
    let mut cursor = 0;
    let mut result = Vec::new();
    for index in indices {
        if let Some(word) = parse.words.get(*index) {
            if let Some(token) = token_for_lg_word(word, tokens, &mut cursor) {
                if !result
                    .iter()
                    .any(|existing: &Token| existing.lower == token.lower)
                {
                    result.push(token);
                }
            }
        }
    }
    result
}

fn token_prep_phrases(tokens: &[Token]) -> Vec<PrepPhrase> {
    let mut result = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if token.kind == TokenKind::Word
            && token.is("out")
            && tokens.get(index + 1).is_some_and(|next| next.is("of"))
        {
            let mut body = Vec::new();
            let mut cursor = index + 2;
            while cursor < tokens.len()
                && !is_preposition(&tokens[cursor].lower)
                && tokens[cursor].kind != TokenKind::Symbol
            {
                body.push(tokens[cursor].clone());
                cursor += 1;
            }
            result.push(PrepPhrase {
                prep: "out_of".to_string(),
                tokens: body,
            });
            index = cursor;
            continue;
        }
        if token.kind == TokenKind::Word && is_preposition(&token.lower) {
            let mut body = Vec::new();
            let mut cursor = index + 1;
            while cursor < tokens.len() {
                let candidate = &tokens[cursor];
                if candidate.kind == TokenKind::Word && is_preposition(&candidate.lower) {
                    break;
                }
                if candidate.kind == TokenKind::Symbol
                    && matches!(
                        candidate.text.as_str(),
                        "," | ";" | ":" | "=" | "(" | ")" | "." | "!" | "?" | "[" | "]"
                    )
                {
                    break;
                }
                body.push(candidate.clone());
                cursor += 1;
            }
            result.push(PrepPhrase {
                prep: token.lower.clone(),
                tokens: body,
            });
            index = cursor;
            continue;
        }
        index += 1;
    }
    result
}

fn link_prep_indices(parse: &LgParse) -> Vec<usize> {
    let mut indices = Vec::new();
    for link in &parse.links {
        if link.label.starts_with("MV")
            || link.label.starts_with("Mp")
            || link.label.starts_with("Pp")
        {
            for index in [link.left, link.right] {
                if let Some(word) = parse.words.get(index) {
                    if is_preposition(&base_word(word).to_ascii_lowercase())
                        && !indices.contains(&index)
                    {
                        indices.push(index);
                    }
                }
            }
        }
    }
    indices.sort_unstable();
    indices
}

fn fallback_voice(tokens: &[Token]) -> (Voice, Option<usize>) {
    let first_verb = tokens
        .iter()
        .position(|token| token.is_word() && classify_verb(&token.lower, None).is_some());
    for (index, token) in tokens.iter().enumerate() {
        if token.is_word() && auxiliary(&token.lower) {
            if let Some(next) = tokens.get(index + 1) {
                let resolved = classify_verb(&next.lower, None);
                if resolved
                    .is_some_and(|value| value.predicate.is_some() || value.concept.is_some())
                {
                    return (Voice::Passive, Some(index + 1));
                }
            }
        }
    }
    if let Some(index) = first_verb {
        if index == 0 {
            return (Voice::Imperative, Some(index));
        }
        return (Voice::Active, Some(index));
    }
    (Voice::Active, None)
}

fn fallback_subject(
    tokens: &[Token],
    verb_index: Option<usize>,
    voice: Voice,
) -> Option<Vec<Token>> {
    let verb = verb_index?;
    if voice == Voice::Imperative {
        return None;
    }
    let limit = if voice == Voice::Passive {
        tokens[..verb]
            .iter()
            .position(|token| token.is_word() && auxiliary(&token.lower))
            .unwrap_or(verb)
    } else {
        verb
    };
    let subject = tokens[..limit]
        .iter()
        .filter(|token| token.is_word())
        .cloned()
        .collect::<Vec<_>>();
    (!subject.is_empty()).then_some(subject)
}

fn fallback_object(
    tokens: &[Token],
    verb_index: Option<usize>,
    voice: Voice,
) -> Option<Vec<Token>> {
    if voice != Voice::Active {
        return None;
    }
    let verb = verb_index?;
    let object = tokens[verb + 1..]
        .iter()
        .take_while(|token| !(token.is_word() && is_preposition(&token.lower)))
        .filter(|token| token.is_word() || token.kind == TokenKind::Number)
        .cloned()
        .collect::<Vec<_>>();
    (!object.is_empty()).then_some(object)
}

/// Converts a sentence into a normalized clause tree using Link Grammar when
/// available and deterministic token roles otherwise.
#[must_use]
pub fn analyze_clause(parser: &LinkGrammarParser, sentence: &RawSentence) -> ClauseTree {
    let tokens = tokenize(&sentence.text);
    let parse = parser.parse(&sentence.text);
    let (fallback_voice_kind, fallback_verb) = fallback_voice(&tokens);
    let mut parse_subjects = subject_indices(&parse.links);
    parse_subjects.sort_unstable();
    parse_subjects.dedup();
    let passive = passive_link(&parse.links).is_some()
        || (fallback_voice_kind == Voice::Passive && !parse.is_used());
    let voice = if passive {
        Voice::Passive
    } else if parse.is_used() {
        if parse_subjects.is_empty()
            && wall_verb(&parse.links).is_some_and(|index| {
                parse
                    .words
                    .get(index)
                    .is_some_and(|word| split_tag(word).1.starts_with('v'))
            })
        {
            Voice::Imperative
        } else {
            Voice::Active
        }
    } else {
        fallback_voice_kind
    };

    let mut verb_index = if parse.is_used() {
        sentence_verb_index(&parse, &parse_subjects, passive).or(fallback_verb)
    } else {
        fallback_verb
    };
    if !passive {
        if let Some(index) = verb_index {
            let is_auxiliary = parse
                .words
                .get(index)
                .is_some_and(|word| auxiliary(&base_word(word)));
            if is_auxiliary {
                let alternative =
                    parse
                        .words
                        .iter()
                        .enumerate()
                        .find_map(|(candidate_index, word)| {
                            let (head, suffix) = split_tag(word);
                            (candidate_index > 0
                                && suffix.starts_with('v')
                                && !auxiliary(&head)
                                && classify_verb(&head, None).is_some())
                            .then_some(candidate_index)
                        });
                if let Some(index) = alternative {
                    verb_index = Some(index);
                }
            }
        }
    }

    let verb = verb_index
        .and_then(|index| parse.words.get(index))
        .map(|word| {
            let raw = base_word(word);
            VerbHead {
                lemma: lemmatize(&raw).into_owned(),
                is_past: voice != Voice::Imperative && word_is_past(word),
                raw,
            }
        })
        .or_else(|| {
            verb_index
                .and_then(|index| tokens.get(index))
                .map(|token| VerbHead {
                    raw: token.text.clone(),
                    lemma: lemmatize(&token.lower).into_owned(),
                    is_past: token.lower.ends_with("ed"),
                })
        });

    let subject = if parse.is_used() {
        let values = collect_from_indices(&parse_subjects, &parse, &tokens);
        if values.is_empty() {
            fallback_subject(&tokens, fallback_verb, fallback_voice_kind)
        } else {
            Some(values)
        }
    } else {
        fallback_subject(&tokens, fallback_verb, voice)
    };

    let direct_object = if parse.is_used() {
        let indices = object_indices(&parse.links);
        let values = collect_from_indices(&indices, &parse, &tokens);
        if values.is_empty() {
            fallback_object(&tokens, fallback_verb, fallback_voice_kind)
        } else {
            Some(values)
        }
    } else {
        fallback_object(&tokens, fallback_verb, voice)
    };

    let mut prep_phrases = token_prep_phrases(&tokens);
    for index in link_prep_indices(&parse) {
        if let Some(word) = parse.words.get(index) {
            let prep = base_word(word).to_ascii_lowercase();
            let mut cursor = 0;
            for value in &parse.words {
                let _ = token_for_lg_word(value, &tokens, &mut cursor);
            }
            let body = tokens
                .iter()
                .skip_while(|token| !token.is(&prep))
                .skip(1)
                .take_while(|token| !(token.is_word() && is_preposition(&token.lower)))
                .cloned()
                .collect::<Vec<_>>();
            if !prep_phrases.iter().any(|existing| existing.prep == prep) {
                prep_phrases.push(PrepPhrase { prep, tokens: body });
            }
        }
    }

    ClauseTree {
        raw: sentence.text.clone(),
        span: sentence.span,
        tokens,
        voice,
        subject,
        verb,
        direct_object,
        prep_phrases,
        lg_words: parse.words.len().saturating_sub(2),
        lg_links: parse.links.len(),
        lg_used: parse.is_used(),
    }
}
