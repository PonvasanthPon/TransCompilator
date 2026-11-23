//! Finite semantic frames shared by every statement, control block, phrase,
//! and declaration lowering path.

use tc_lexicon::{SemanticConcept, classify_verb};
use tc_typeck::{AstExpr, AstStmt};

use crate::lower::{Lowerer, strip_leading_word};
use crate::place::{PlaceSpec, parse_place};
use crate::place::{canonical_assignment_target, canonical_bare_target};
use crate::textutil::{
    find_top_level_word, first_word as first_word_of, split_top_level, starts_with_word,
};
use crate::token::tokenize;
use crate::value::ValueSpec;
use crate::{SemanticError, SemanticErrorKind, Span};

/// Transfer/mutation operations that share place + amount lowering.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransferOp {
    /// Additive transfer.
    Add,
    /// Subtractive transfer.
    Sub,
    /// Source-to-destination move.
    Move,
    /// Multiplication.
    Mul,
    /// Division.
    Div,
    /// Multiply by two.
    Double,
    /// Divide by two.
    Halve,
    /// Arithmetic negation.
    Negate,
    /// Add one or a supplied amount.
    Increment,
    /// Subtract one or a supplied amount.
    Decrement,
    /// Reset to zero or a supplied value.
    Reset,
    /// Clear selected bits.
    ClearBits,
    /// Set selected bits.
    SetBits,
    /// Adjust by a supplied amount.
    Adjust,
}

/// Finite frame IR produced by the normalizer and consumed by `lower_frame`.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) enum SemanticFrame {
    /// Concept-driven output: Print/Show/Display/Output/Say/Report.
    Emit {
        /// Value to print.
        value: ValueSpec,
    },
    /// Assignment forms: Let/Put/Set/Store/Keep/Make/becomes/is/=/Reset/Copy/Convert.
    Assign {
        /// Parsed destination place.
        target: PlaceSpec,
        /// Parsed value.
        value: ValueSpec,
        /// Whether an undeclared single target may be introduced.
        allow_declare: bool,
    },
    /// Mutating transfer/arithmetic forms.
    Transfer {
        /// Operation.
        op: TransferOp,
        /// Optional amount; `None` means the operation's default.
        amount: Option<ValueSpec>,
        /// Optional source.
        source: Option<PlaceSpec>,
        /// Optional destination.
        dest: Option<PlaceSpec>,
    },
    /// Swap/exchange two places.
    Swap {
        /// Left place.
        lhs: PlaceSpec,
        /// Right place.
        rhs: PlaceSpec,
    },
    /// Field-wise aggregate update.
    Update {
        /// Aggregate target.
        target: PlaceSpec,
        /// Field/value pairs.
        fields: Vec<(String, ValueSpec)>,
    },
    /// Control flow wrapper.
    Control(ControlFrame),
    /// Declarative phrase/auto-phrase call.
    PhraseCall(PhraseCallFrame),
    /// Declaration wrapper.
    Decl(DeclFrame),
}

/// Control-flow frame wrapper.  The body is normalized by the existing
/// recursive control lowerer.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct ControlFrame {
    /// Header text (`If ...`, `While ...`, etc.).
    pub header: String,
    /// Inline or block body.
    pub body: String,
}

/// Phrase call frame.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct PhraseCallFrame {
    /// Phrase text.
    pub text: String,
}

/// Declaration frame.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct DeclFrame {
    /// Declaration text.
    pub text: String,
}

impl SemanticFrame {
    /// Assign constructor.
    pub(crate) fn assign(target: PlaceSpec, value: ValueSpec, allow_declare: bool) -> Self {
        Self::Assign {
            target,
            value,
            allow_declare,
        }
    }

    /// Transfer constructor.
    pub(crate) fn transfer(
        op: TransferOp,
        amount: Option<ValueSpec>,
        source: Option<PlaceSpec>,
        dest: Option<PlaceSpec>,
    ) -> Self {
        Self::Transfer {
            op,
            amount,
            source,
            dest,
        }
    }
}

impl Lowerer<'_> {
    pub(crate) fn semantic_place(
        &self,
        text: &str,
        span: Span,
    ) -> Result<PlaceSpec, SemanticError> {
        parse_place(text).map_err(|message| self.error(span, SemanticErrorKind::Lowering, message))
    }

    pub(crate) fn lower_frame(
        &mut self,
        frame: SemanticFrame,
        span: Span,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        match frame {
            SemanticFrame::Emit { value } => self.lower_emit_value(&value.text, span),
            SemanticFrame::Assign {
                target,
                value,
                allow_declare,
            } => {
                let value = self.parse_value(&value.text, span)?;
                self.assign_text(&target.render(), value, span, allow_declare)
            }
            SemanticFrame::Transfer {
                op,
                amount,
                source,
                dest,
            } => self.lower_transfer_frame(op, amount, source, dest, span),
            SemanticFrame::Swap { lhs, rhs } => self.lower_swap_places(lhs, rhs, span),
            SemanticFrame::Update { target, fields } => {
                let mut statements = Vec::new();
                for (field, value) in fields {
                    let value = self.parse_value(&value.text, span)?;
                    let place = format!("{}'s {field}", target.render());
                    statements.extend(self.assign_text(&place, value, span, false)?);
                }
                Ok(statements)
            }
            SemanticFrame::Control(_)
            | SemanticFrame::PhraseCall(_)
            | SemanticFrame::Decl(_) => Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                "control, phrase, and declaration frames are lowered by their dedicated dispatchers",
            )),
        }
    }

    pub(crate) fn lower_emit_value(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        let value = self.parse_value(text, span)?;
        let statement = if let AstExpr::BytesLit(bytes) = &value {
            AstExpr::Call {
                module: None,
                name: "print_bytes".to_string(),
                type_args: Vec::new(),
                args: vec![value.clone(), AstExpr::BytesLen(bytes.clone())],
            }
        } else {
            let ty = self.shallow_type_opt(&value);
            let name = match ty {
                Some(
                    tc_typeck::AstType::U8
                    | tc_typeck::AstType::U16
                    | tc_typeck::AstType::U32
                    | tc_typeck::AstType::U64,
                ) => "print_u64",
                Some(tc_typeck::AstType::F32 | tc_typeck::AstType::F64) => "print_f64",
                Some(tc_typeck::AstType::Bool) => "print_bool",
                _ => "print_i64",
            };
            AstExpr::Call {
                module: None,
                name: name.to_string(),
                type_args: Vec::new(),
                args: vec![value.clone()],
            }
        };
        Ok(vec![AstStmt::Expr(statement, self.span(span))])
    }

    pub(crate) fn lower_transfer_frame(
        &mut self,
        op: TransferOp,
        amount: Option<ValueSpec>,
        source: Option<PlaceSpec>,
        dest: Option<PlaceSpec>,
        span: Span,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        let parse_amount = |lower: &mut Self| -> Result<AstExpr, SemanticError> {
            match &amount {
                Some(value) => lower.parse_value(&value.text, span),
                None => Ok(AstExpr::ConstInt(1, None)),
            }
        };
        match op {
            TransferOp::Increment => {
                let amount = parse_amount(self)?;
                let Some(dest) = dest else {
                    return Err(self.error(
                        span,
                        SemanticErrorKind::Lowering,
                        "increment requires a destination place",
                    ));
                };
                self.write_arithmetic(&dest.render(), tc_ir::BinOp::Add, amount, span)
            }
            TransferOp::Decrement => {
                let amount = parse_amount(self)?;
                let Some(dest) = dest else {
                    return Err(self.error(
                        span,
                        SemanticErrorKind::Lowering,
                        "decrement requires a destination place",
                    ));
                };
                self.write_arithmetic(&dest.render(), tc_ir::BinOp::Sub, amount, span)
            }
            TransferOp::Add | TransferOp::Sub | TransferOp::Mul | TransferOp::Div => {
                let Some(dest) = dest else {
                    return Err(self.error(
                        span,
                        SemanticErrorKind::Lowering,
                        "transfer requires a destination place",
                    ));
                };
                let amount = amount
                    .as_ref()
                    .map_or(Ok(AstExpr::ConstInt(1, None)), |value| {
                        self.parse_value(&value.text, span)
                    })?;
                let operator = match op {
                    TransferOp::Add => tc_ir::BinOp::Add,
                    TransferOp::Sub => tc_ir::BinOp::Sub,
                    TransferOp::Mul => tc_ir::BinOp::Mul,
                    _ => tc_ir::BinOp::Div,
                };
                self.write_arithmetic(&dest.render(), operator, amount, span)
            }
            TransferOp::Move => {
                let (Some(source), Some(dest)) = (source, dest) else {
                    return Err(self.error(
                        span,
                        SemanticErrorKind::Lowering,
                        "move requires source and destination places",
                    ));
                };
                let amount = amount
                    .as_ref()
                    .map_or(Ok(AstExpr::ConstInt(1, None)), |value| {
                        self.parse_value(&value.text, span)
                    })?;
                let mut statements = self.write_arithmetic(
                    &source.render(),
                    tc_ir::BinOp::Sub,
                    amount.clone(),
                    span,
                )?;
                statements.extend(self.write_arithmetic(
                    &dest.render(),
                    tc_ir::BinOp::Add,
                    amount,
                    span,
                )?);
                Ok(statements)
            }
            TransferOp::Double | TransferOp::Halve | TransferOp::Negate => {
                let Some(dest) = dest else {
                    return Err(self.error(
                        span,
                        SemanticErrorKind::Lowering,
                        "transform requires a destination place",
                    ));
                };
                let operator = match op {
                    TransferOp::Double => tc_ir::BinOp::Mul,
                    TransferOp::Halve => tc_ir::BinOp::Div,
                    _ => tc_ir::BinOp::Add,
                };
                match op {
                    TransferOp::Negate => {
                        let (prelude, target) = self.resolve_target(&dest.render(), span)?;
                        let value = AstExpr::Unary {
                            op: tc_ir::UnOp::Neg,
                            val: Box::new(target.read()),
                        };
                        let mut statements = prelude;
                        statements.push(self.write_target(&target, value, span));
                        Ok(statements)
                    }
                    _ => self.write_arithmetic(
                        &dest.render(),
                        operator,
                        AstExpr::ConstInt(2, None),
                        span,
                    ),
                }
            }
            TransferOp::Reset => {
                let Some(dest) = dest else {
                    return Err(self.error(
                        span,
                        SemanticErrorKind::Lowering,
                        "reset requires a destination place",
                    ));
                };
                let value = amount
                    .as_ref()
                    .map_or(Ok(AstExpr::ConstInt(0, None)), |value| {
                        self.parse_value(&value.text, span)
                    })?;
                self.assign_text(&dest.render(), value, span, true)
            }
            TransferOp::Adjust => {
                let Some(dest) = dest else {
                    return Err(self.error(
                        span,
                        SemanticErrorKind::Lowering,
                        "adjust requires a destination place",
                    ));
                };
                let amount = parse_amount(self)?;
                self.write_arithmetic(&dest.render(), tc_ir::BinOp::Add, amount, span)
            }
            TransferOp::ClearBits | TransferOp::SetBits => Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                "bitwise frames are lowered by the bitwise dispatcher",
            )),
        }
    }

    pub(crate) fn lower_swap_places(
        &mut self,
        lhs: PlaceSpec,
        rhs: PlaceSpec,
        span: Span,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        let (mut prelude, lhs) = self.resolve_target(&lhs.render(), span)?;
        let (rhs_prelude, rhs) = self.resolve_target(&rhs.render(), span)?;
        prelude.extend(rhs_prelude);
        let ty = self.shallow_type(&lhs.read());
        let temporary = self.next_temp("swap");
        self.declare_temp(&temporary, ty.clone());
        prelude.push(AstStmt::Let {
            name: temporary.clone(),
            ty: Some(ty),
            init: lhs.read(),
            span: self.span(span),
        });
        prelude.push(self.write_target(&lhs, rhs.read(), span));
        prelude.push(self.write_target(&rhs, AstExpr::Var(temporary), span));
        Ok(prelude)
    }

    pub(crate) fn try_semantic_frame(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<Vec<AstStmt>>, SemanticError> {
        let tokens = tokenize(text);
        let Some(first) = tokens.first().filter(|token| token.is_word()) else {
            return Ok(None);
        };
        if matches!(
            first.lower.as_str(),
            "let" | "put" | "set" | "store" | "keep" | "make" | "update" | "create"
        ) {
            return Ok(None);
        }
        let Some(resolution) = classify_verb(&first.lower, None) else {
            return Ok(None);
        };
        let rest = text[first.end..].trim().trim_end_matches('.');
        let emit_alternative = starts_with_word(rest, "out")
            && resolution.predicate != Some(tc_lexicon::VerbPredicate::Emit)
            && tc_lexicon::lookup_verbnet(&resolution.lemma, None)
                .iter()
                .any(|frame| frame.predicate == tc_lexicon::VerbPredicate::Emit);
        let is_emit = resolution.concept == Some(SemanticConcept::Output)
            || (resolution.predicate == Some(tc_lexicon::VerbPredicate::Emit)
                && resolution.concept.is_none())
            || emit_alternative;
        if is_emit {
            let mut object = rest;
            loop {
                if starts_with_word(object, "out")
                    || starts_with_word(object, "me")
                    || starts_with_word(object, "us")
                {
                    let word = first_word_of(object);
                    object = strip_leading_word(object, &word).trim();
                    continue;
                }
                break;
            }
            if !object.is_empty() {
                if let Some(parts) = self.split_emit_parts(object, span) {
                    return self.lower_emit_parts(&parts, span).map(Some);
                }
                return self
                    .lower_frame(
                        SemanticFrame::Emit {
                            value: ValueSpec::classify(object),
                        },
                        span,
                    )
                    .map(Some);
            }
            return Ok(None);
        }
        if matches!(
            resolution.concept,
            Some(
                SemanticConcept::Square
                    | SemanticConcept::Cube
                    | SemanticConcept::Double
                    | SemanticConcept::Halve
                    | SemanticConcept::Negate
            )
        ) {
            let target = self.semantic_place(rest, span)?;
            let rendered = target.render();
            let numeric_two = self
                .resolve_target(rest, span)
                .ok()
                .and_then(|(_, target)| self.shallow_type_opt(&target.read()))
                .is_some_and(|ty| matches!(ty, tc_typeck::AstType::F32 | tc_typeck::AstType::F64));
            let two = if numeric_two { "2.0" } else { "2" };
            let value = match resolution.concept {
                Some(SemanticConcept::Square) => format!("{rendered} times {rendered}"),
                Some(SemanticConcept::Cube) => {
                    format!("{rendered} times {rendered} times {rendered}")
                }
                Some(SemanticConcept::Double) => format!("{rendered} times {two}"),
                Some(SemanticConcept::Halve) => format!("{rendered} divided by {two}"),
                _ => format!("negative {rendered}"),
            };
            return self
                .lower_frame(
                    SemanticFrame::assign(target, ValueSpec::classify(&value), false),
                    span,
                )
                .map(Some);
        }
        let frame = match resolution.concept {
            Some(SemanticConcept::Increment) | Some(SemanticConcept::Decrement) => {
                let upper = matches!(resolution.concept, Some(SemanticConcept::Increment));
                let (target, amount) = if let Some((start, end)) = find_top_level_word(rest, "by") {
                    (
                        rest[..start].trim(),
                        Some(ValueSpec::classify(rest[end..].trim())),
                    )
                } else {
                    (rest.trim(), None)
                };
                let place = self.semantic_place(target, span)?;
                SemanticFrame::transfer(
                    if upper {
                        TransferOp::Increment
                    } else {
                        TransferOp::Decrement
                    },
                    amount,
                    None,
                    Some(place),
                )
            }
            Some(SemanticConcept::Reset) => {
                let (target, amount) = if let Some((start, end)) = find_top_level_word(rest, "to") {
                    (
                        rest[..start].trim(),
                        Some(ValueSpec::classify(rest[end..].trim())),
                    )
                } else {
                    (rest.trim(), Some(ValueSpec::classify("0")))
                };
                return self
                    .lower_frame(
                        SemanticFrame::assign(
                            self.semantic_place(target, span)?,
                            amount.unwrap(),
                            true,
                        ),
                        span,
                    )
                    .map(Some);
            }
            Some(SemanticConcept::Swap) => {
                let Some((start, end)) = find_top_level_word(rest, "and") else {
                    return Ok(None);
                };
                return self
                    .lower_frame(
                        SemanticFrame::Swap {
                            lhs: self.semantic_place(rest[..start].trim(), span)?,
                            rhs: self.semantic_place(rest[end..].trim(), span)?,
                        },
                        span,
                    )
                    .map(Some);
            }
            Some(SemanticConcept::Copy) => {
                let Some((start, end)) =
                    find_top_level_word(rest, "into").or_else(|| find_top_level_word(rest, "to"))
                else {
                    return Ok(None);
                };
                let source = rest[..start].trim();
                let dest = self.semantic_place(rest[end..].trim(), span)?;
                return self
                    .lower_frame(
                        SemanticFrame::assign(dest, ValueSpec::classify(source), true),
                        span,
                    )
                    .map(Some);
            }
            Some(SemanticConcept::Convert) => {
                let Some((start, end)) = find_top_level_word(rest, "to") else {
                    return Ok(None);
                };
                let target = rest[..start].trim();
                let ty_text = rest[end..].trim();
                let ty_lower = ty_text.to_ascii_lowercase();
                if !(ty_lower.starts_with("a ")
                    || ty_lower.starts_with("an ")
                    || ty_lower.starts_with("the ")
                    || [
                        "number",
                        "decimal",
                        "byte",
                        "truth value",
                        "pointer",
                        "text",
                    ]
                    .iter()
                    .any(|word| ty_lower.contains(word)))
                {
                    return Ok(None);
                }
                let Ok(to) = crate::expr::parse_type_text(ty_text) else {
                    return Ok(None);
                };
                let value = self.parse_value(target, span)?;
                let cast = AstExpr::Cast {
                    val: Box::new(value),
                    to,
                };
                return Ok(Some(vec![AstStmt::Expr(cast, self.span(span))]));
            }
            Some(SemanticConcept::Adjust) => {
                if let Some((start, end)) = find_top_level_word(rest, "by") {
                    let place = self.semantic_place(rest[..start].trim(), span)?;
                    return self
                        .lower_frame(
                            SemanticFrame::transfer(
                                TransferOp::Adjust,
                                Some(ValueSpec::classify(rest[end..].trim())),
                                None,
                                Some(place),
                            ),
                            span,
                        )
                        .map(Some);
                }
                if let Some((start, end)) = find_top_level_word(rest, "to") {
                    return self
                        .lower_frame(
                            SemanticFrame::assign(
                                self.semantic_place(rest[..start].trim(), span)?,
                                ValueSpec::classify(rest[end..].trim()),
                                false,
                            ),
                            span,
                        )
                        .map(Some);
                }
                return Ok(None);
            }
            _ => return Ok(None),
        };
        self.lower_frame(frame, span).map(Some)
    }
}

impl Lowerer<'_> {
    fn split_update_clauses(&self, text: &str) -> Vec<String> {
        let mut clauses = Vec::new();
        for comma_piece in split_top_level(text, ",") {
            let tokens = tokenize(&comma_piece);
            let mut start = 0;
            let mut depth = 0_i32;
            for (index, token) in tokens.iter().enumerate() {
                match token.text.as_str() {
                    "(" | "[" | "{" => depth += 1,
                    ")" | "]" | "}" => depth -= 1,
                    _ => {}
                }
                if depth == 0
                    && token.is("and")
                    && tokens
                        .get(index + 1)
                        .is_some_and(|next| next.is("its") || next.is("the"))
                {
                    let clause = comma_piece[start..token.start].trim();
                    if !clause.is_empty() {
                        clauses.push(clause.to_string());
                    }
                    start = token.end;
                }
            }
            let tail = comma_piece[start..].trim();
            if !tail.is_empty() {
                clauses.push(tail.to_string());
            }
        }
        clauses
    }

    pub(crate) fn try_update(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<Vec<AstStmt>>, SemanticError> {
        if !starts_with_word(text, "update") {
            return Ok(None);
        }
        let rest = strip_leading_word(text, "update");
        let Some((start, end)) = find_top_level_word(rest, "so") else {
            return Ok(None);
        };
        let canonical_target = canonical_assignment_target(rest[..start].trim());
        let target = canonical_target.as_str();
        let clauses_text = rest[end..].trim();
        if target.is_empty() || clauses_text.is_empty() {
            return Ok(None);
        }
        self.touch_target_text(target, span);
        let mut statements = Vec::new();
        for clause in self.split_update_clauses(clauses_text) {
            let clause = clause.trim().trim_start_matches("and").trim();
            let Some((marker_start, marker_end)) = find_top_level_word(clause, "is")
                .or_else(|| find_top_level_word(clause, "becomes"))
            else {
                return Err(self.error(
                    span,
                    SemanticErrorKind::Lowering,
                    format!("update clause `{clause}` must use `its <field> is <value>`"),
                ));
            };
            let field_text = clause[..marker_start].trim();
            let value_text = clause[marker_end..].trim();
            let field_text = field_text.strip_prefix("its ").map_or_else(
                || {
                    field_text
                        .strip_prefix("the ")
                        .unwrap_or(field_text)
                        .to_string()
                },
                ToString::to_string,
            );
            let field = field_text
                .rsplit("'s")
                .next()
                .map_or(field_text.as_str(), str::trim);
            if field.is_empty() || value_text.is_empty() {
                return Err(self.error(
                    span,
                    SemanticErrorKind::Lowering,
                    format!("update clause `{clause}` is missing a field or value"),
                ));
            }
            let field = canonical_bare_target(field);
            let value = self.parse_value(value_text, span)?;
            let target_text = format!("{target}'s {field}");
            statements.extend(self.assign_text(&target_text, value, span, false)?);
        }
        self.touch_target_text(target, span);
        Ok(Some(statements))
    }
}
