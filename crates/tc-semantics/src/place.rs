//! Single-source target/place parsing for every assignment and mutation.

use crate::token::{Token, TokenKind, tokenize};
use tc_lexicon::lookup_closed_class;

/// The finite target shapes accepted by assignment and mutation frames.
#[derive(Debug, Clone)]
pub(crate) enum PlaceSpec {
    /// Local/global variable, including canonicalized multi-word names.
    Var {
        /// Canonical name.
        name: String,
    },
    /// Discourse pronoun `it`.
    AnaphorIt,
    /// Field on the active discourse aggregate: `its <field>`.
    AnaphorField {
        /// Field name.
        field: String,
    },
    /// Explicit possessive or dotted field.
    NamedField {
        /// Base variable or type instance.
        base: String,
        /// True for `the <type>'s <field>`.
        is_definite_type: bool,
        /// Field name.
        field: String,
    },
    /// Indexed place, including `item <index> of <base>`.
    Indexed {
        /// Base place.
        base: Box<PlaceSpec>,
        /// Index expression tokens.
        index_tokens: Vec<Token>,
    },
    /// Pointer dereference, including `value at <ptr>`.
    Deref {
        /// Pointer expression tokens.
        ptr_tokens: Vec<Token>,
    },
}

pub(crate) fn canonical_bare_target(text: &str) -> String {
    let trimmed = text.trim();
    let tokens = tokenize(trimmed);
    if tokens.len() == 1
        && tokens[0].kind == TokenKind::Word
        && tokens[0]
            .text
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return tokens[0].lower.clone();
    }
    let lower = trimmed.to_ascii_lowercase();
    let first = tokens
        .first()
        .filter(|token| token.is_word())
        .map_or("", |token| token.lower.as_str());
    if matches!(
        first,
        "the"
            | "a"
            | "an"
            | "it"
            | "its"
            | "this"
            | "that"
            | "these"
            | "those"
            | "address"
            | "my"
            | "your"
            | "his"
            | "her"
            | "our"
            | "their"
            | "i"
            | "we"
            | "he"
            | "she"
            | "they"
            | "you"
    ) || lower.starts_with("address ")
        || tokens.iter().any(|token| token.is("of"))
    {
        return trimmed.to_string();
    }
    if tokens
        .iter()
        .any(|token| token.is_word() && lookup_closed_class(&token.lower).is_some())
    {
        return trimmed.to_string();
    }
    if (2..=4).contains(&tokens.len())
        && tokens.iter().all(Token::is_word)
        && tokens
            .windows(2)
            .all(|pair| trimmed[pair[0].end..pair[1].start].trim().is_empty())
    {
        return tokens
            .iter()
            .map(|token| token.lower.as_str())
            .collect::<Vec<_>>()
            .join("_");
    }
    trimmed.to_string()
}

/// Canonicalizes every supported assignment/mutation target shape.
pub(crate) fn canonical_assignment_target(text: &str) -> String {
    let trimmed = text.trim();
    let tokens = tokenize(trimmed);
    if let Some(possessive) = tokens
        .iter()
        .position(|token| token.text == "'s" || token.text == "\u{2019}s")
    {
        let base = trimmed[..tokens[possessive].start].trim();
        let canonical = canonical_bare_target(base);
        if canonical != base {
            return format!("{canonical}{}", &trimmed[tokens[possessive].start..]);
        }
        return trimmed.to_string();
    }
    canonical_bare_target(trimmed)
}

fn field_after(text: &str, token: &Token) -> Option<String> {
    let rest = text[token.end..].trim();
    let field = rest.split_whitespace().next()?.trim_end_matches('.');
    (!field.is_empty()).then(|| field.to_ascii_lowercase())
}

/// Parses one place without mutating lowering state.
pub(crate) fn parse_place(text: &str) -> Result<PlaceSpec, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("empty assignment target".to_string());
    }
    let canonical = canonical_assignment_target(trimmed);
    let tokens = tokenize(&canonical);
    if canonical.eq_ignore_ascii_case("it") {
        return Ok(PlaceSpec::AnaphorIt);
    }
    if let Some(rest) = canonical
        .strip_prefix("its ")
        .or_else(|| canonical.strip_prefix("Its "))
    {
        let field = rest.trim().to_ascii_lowercase();
        return if field.is_empty() {
            Err("`its` requires a field name".to_string())
        } else {
            Ok(PlaceSpec::AnaphorField { field })
        };
    }
    if let Some(possessive) = tokens
        .iter()
        .position(|token| token.text == "'s" || token.text == "\u{2019}s")
    {
        let base_text = canonical[..tokens[possessive].start].trim();
        let Some(field) = field_after(&canonical, &tokens[possessive]) else {
            return Err("possessive target is missing a field".to_string());
        };
        let definite = base_text.to_ascii_lowercase().starts_with("the ");
        let base = base_text
            .strip_prefix("the ")
            .or_else(|| base_text.strip_prefix("The "))
            .unwrap_or(base_text)
            .trim()
            .to_ascii_lowercase();
        if crate::suggest::is_reserved_control_word(&base) {
            return Err(format!(
                "'{base}' is a reserved control word — choose a different variable name (for example, 'counter')"
            ));
        }
        return Ok(PlaceSpec::NamedField {
            base,
            is_definite_type: definite,
            field,
        });
    }
    if let Some(dot) = tokens.iter().position(|token| token.text == ".") {
        let base = canonical[..tokens[dot].start].trim().to_ascii_lowercase();
        if crate::suggest::is_reserved_control_word(&base) {
            return Err(format!(
                "'{base}' is a reserved control word — choose a different variable name (for example, 'counter')"
            ));
        }
        let Some(field) = field_after(&canonical, &tokens[dot]) else {
            return Err("dotted target is missing a field".to_string());
        };
        return Ok(PlaceSpec::NamedField {
            base,
            is_definite_type: false,
            field,
        });
    }
    if let Some(bracket) = tokens.iter().position(|token| token.text == "[") {
        let base_text = canonical[..tokens[bracket].start].trim();
        let close = tokens
            .iter()
            .rposition(|token| token.text == "]")
            .ok_or_else(|| "indexed target is missing `]`".to_string())?;
        if close <= bracket + 1 {
            return Err("indexed target is missing an index".to_string());
        }
        return Ok(PlaceSpec::Indexed {
            base: Box::new(parse_place(base_text)?),
            index_tokens: tokens[bracket + 1..close].to_vec(),
        });
    }
    if tokens.first().is_some_and(|token| token.text == "*") {
        return Ok(PlaceSpec::Deref {
            ptr_tokens: tokens[1..].to_vec(),
        });
    }
    if canonical.to_ascii_lowercase().starts_with("value at ") {
        return Ok(PlaceSpec::Deref {
            ptr_tokens: tokenize(canonical["value at ".len()..].trim()),
        });
    }
    if canonical.to_ascii_lowercase().starts_with("item ") {
        if let Some((start, end)) = crate::textutil::find_top_level_word(&canonical, "of") {
            let index_tokens = tokenize(canonical["item ".len()..start].trim());
            let base_text = canonical[end..].trim();
            if !index_tokens.is_empty() && !base_text.is_empty() {
                return Ok(PlaceSpec::Indexed {
                    base: Box::new(parse_place(base_text)?),
                    index_tokens,
                });
            }
        }
    }
    let name = canonical.to_ascii_lowercase();
    if crate::suggest::is_reserved_control_word(&name) {
        return Err(format!(
            "'{name}' is a reserved control word — choose a different variable name (for example, 'counter')"
        ));
    }
    Ok(PlaceSpec::Var { name })
}

impl PlaceSpec {
    /// Renders a canonical source spelling for the lowerer.
    pub(crate) fn render(&self) -> String {
        match self {
            Self::Var { name, .. } => name.clone(),
            Self::AnaphorIt => "it".to_string(),
            Self::AnaphorField { field } => format!("its {field}"),
            Self::NamedField {
                base,
                is_definite_type,
                field,
            } => {
                if *is_definite_type {
                    format!("the {base}'s {field}")
                } else {
                    format!("{base}'s {field}")
                }
            }
            Self::Indexed { base, index_tokens } => format!(
                "{}[{}]",
                base.render(),
                index_tokens
                    .iter()
                    .map(|token| token.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            Self::Deref { ptr_tokens } => format!(
                "value at {}",
                ptr_tokens
                    .iter()
                    .map(|token| token.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        }
    }
}
