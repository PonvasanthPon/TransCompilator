//! Single-source expression and phrase-value parsing for every semantic frame.

use tc_typeck::AstExpr;

use crate::phrases::PhraseMatch;
use crate::{SemanticError, Span};

/// Coarse intent of a value expression, useful to frame construction and
/// diagnostics.  The actual typed expression is produced by `parse_value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValueKind {
    /// Numeric/string/boolean literal.
    Literal,
    /// Variable or field reference.
    Place,
    /// Function or phrase call.
    Call,
    /// Arithmetic/bitwise/pointer expression.
    Expression,
}

/// One normalized value before typed lowering.
#[derive(Debug, Clone)]
pub(crate) struct ValueSpec {
    /// Original/canonical source text.
    pub text: String,
    /// Coarse expression kind.
    pub kind: ValueKind,
}

impl ValueSpec {
    /// Classifies a value without losing its source text.
    pub(crate) fn classify(text: &str) -> Self {
        let trimmed = text.trim();
        let lower = trimmed.to_ascii_lowercase();
        let kind = if trimmed.starts_with('"')
            || trimmed.starts_with('\'')
            || lower == "true"
            || lower == "false"
            || lower == "no address"
            || lower == "null"
            || trimmed
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_digit() || ch == '-' || ch == '.')
        {
            ValueKind::Literal
        } else if trimmed.contains('(') || lower.starts_with("create ") {
            ValueKind::Call
        } else if trimmed.contains(" of ")
            || trimmed.contains('\'')
            || trimmed.contains('.')
            || trimmed.contains('[')
        {
            ValueKind::Place
        } else if trimmed.contains(' ')
            || trimmed.contains('+')
            || trimmed.contains('-')
            || trimmed.contains('*')
            || trimmed.contains('/')
            || trimmed.contains('%')
            || trimmed.contains('<')
            || trimmed.contains('>')
        {
            ValueKind::Expression
        } else {
            ValueKind::Place
        };
        Self {
            text: trimmed.to_string(),
            kind,
        }
    }
}

/// Callbacks required by the single value parser.  `Lowerer` implements this
/// trait in `lower.rs`; all statement/control/phrase paths share this one
/// parser.
pub(crate) trait ValueContext {
    /// Match a declarative phrase value.
    fn match_value_phrase(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<PhraseMatch>, SemanticError>;

    /// Lower a matched phrase call.
    fn value_phrase_expression(
        &mut self,
        phrase: &PhraseMatch,
        span: Span,
    ) -> Result<AstExpr, SemanticError>;

    /// Parse a Core-7 expression after anaphor/name resolution.
    fn value_expression(&mut self, text: &str, span: Span) -> Result<AstExpr, SemanticError>;
}

/// The one value parser used by assignment, transfer, update, return,
/// conditions, and phrase arguments.
pub(crate) fn parse_value<C: ValueContext>(
    context: &mut C,
    text: &str,
    span: Span,
) -> Result<AstExpr, SemanticError> {
    let specification = ValueSpec::classify(text);
    let _ = specification.kind;
    if let Some(phrase) = context.match_value_phrase(&specification.text, span)? {
        return context.value_phrase_expression(&phrase, span);
    }
    context.value_expression(&specification.text, span)
}
