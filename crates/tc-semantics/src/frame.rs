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

/// Live transfer/mutation operations that share place + amount lowering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransferOp {
    /// Add one or a supplied amount.
    Increment,
    /// Subtract one or a supplied amount.
    Decrement,
    /// Adjust by a supplied amount.
    Adjust,
}

/// Finite frame IR produced by the normalizer and consumed by `lower_frame`.
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
        /// Destination place.
        dest: PlaceSpec,
    },
    /// Swap/exchange two places.
    Swap {
        /// Left place.
        lhs: PlaceSpec,
        /// Right place.
        rhs: PlaceSpec,
    },
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
    pub(crate) fn transfer(op: TransferOp, amount: Option<ValueSpec>, dest: PlaceSpec) -> Self {
        Self::Transfer { op, amount, dest }
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
            SemanticFrame::Transfer { op, amount, dest } => {
                self.lower_transfer_frame(op, amount, dest, span)
            }
            SemanticFrame::Swap { lhs, rhs } => self.lower_swap_places(lhs, rhs, span),
        }
    }

    pub(crate) fn lower_emit_value(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        let value = self.parse_value(text, span)?;
        let ty = self.shallow_type_opt(&value);
        let mut statements = Vec::new();
        let statement = if let Some((pointer, length)) = self.text_bytes(&value) {
            AstExpr::Call {
                module: None,
                name: "print_bytes".to_string(),
                type_args: Vec::new(),
                args: vec![pointer, length],
            }
        } else if let Some(tcstring) = ty.clone().filter(|ty| {
            matches!(
                ty,
                tc_typeck::AstType::Named { name, .. } if name.eq_ignore_ascii_case("tcstring")
            )
        }) {
            let temporary = self.next_temp("emit_text");
            self.declare_temp(&temporary, tcstring.clone());
            statements.push(AstStmt::Let {
                name: temporary.clone(),
                ty: Some(tcstring),
                init: value.clone(),
                span: self.span(span),
            });
            AstExpr::Call {
                module: None,
                name: "print_str".to_string(),
                type_args: Vec::new(),
                args: vec![AstExpr::AddrOfVar(temporary)],
            }
        } else {
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
        statements.push(AstStmt::Expr(statement, self.span(span)));
        if let Some((tracker, maximum)) = self.active_print_tracker()
            && !matches!(value, AstExpr::BytesLit(_))
            && matches!(
                ty,
                None | Some(
                    tc_typeck::AstType::I8
                        | tc_typeck::AstType::I16
                        | tc_typeck::AstType::I32
                        | tc_typeck::AstType::I64
                )
            )
        {
            statements.push(AstStmt::If {
                cond: AstExpr::Binary {
                    op: if maximum {
                        tc_ir::BinOp::Gt
                    } else {
                        tc_ir::BinOp::Lt
                    },
                    lhs: Box::new(value.clone()),
                    rhs: Box::new(AstExpr::Var(tracker.clone())),
                },
                then_body: vec![AstStmt::Assign {
                    name: tracker,
                    val: value,
                    span: self.span(span),
                }],
                else_body: Vec::new(),
                span: self.span(span),
            });
        }
        Ok(statements)
    }

    pub(crate) fn lower_transfer_frame(
        &mut self,
        op: TransferOp,
        amount: Option<ValueSpec>,
        dest: PlaceSpec,
        span: Span,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        let amount = amount
            .as_ref()
            .map_or(Ok(AstExpr::ConstInt(1, None)), |value| {
                self.parse_value(&value.text, span)
            })?;
        let operator = match op {
            TransferOp::Increment | TransferOp::Adjust => tc_ir::BinOp::Add,
            TransferOp::Decrement => tc_ir::BinOp::Sub,
        };
        self.write_arithmetic(&dest.render(), operator, amount, span)
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
            let object = strip_trailing_out(object);
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
                    | SemanticConcept::Triple
                    | SemanticConcept::Negate
            )
        ) {
            let target = self.semantic_place(rest, span)?;
            let rendered = target.render();
            let numeric = self
                .resolve_target(rest, span)
                .ok()
                .and_then(|(_, target)| self.shallow_type_opt(&target.read()))
                .is_some_and(|ty| matches!(ty, tc_typeck::AstType::F32 | tc_typeck::AstType::F64));
            let two = if numeric { "2.0" } else { "2" };
            let three = if numeric { "3.0" } else { "3" };
            let value = match resolution.concept {
                Some(SemanticConcept::Square) => format!("{rendered} times {rendered}"),
                Some(SemanticConcept::Cube) => {
                    format!("{rendered} times {rendered} times {rendered}")
                }
                Some(SemanticConcept::Double) => format!("{rendered} times {two}"),
                Some(SemanticConcept::Halve) => format!("{rendered} divided by {two}"),
                Some(SemanticConcept::Triple) => format!("{rendered} times {three}"),
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
                    place,
                )
            }
            Some(SemanticConcept::Reset) => {
                let (target, amount) = if let Some((start, end)) = find_top_level_word(rest, "to") {
                    (
                        rest[..start].trim(),
                        ValueSpec::classify(rest[end..].trim()),
                    )
                } else {
                    (rest.trim(), ValueSpec::classify("0"))
                };
                return self
                    .lower_frame(
                        SemanticFrame::assign(self.semantic_place(target, span)?, amount, true),
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
                                place,
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

fn strip_trailing_out(text: &str) -> &str {
    let trimmed = text.trim_end_matches(['.', ',']).trim_end();
    if let Some(prefix) = trimmed.strip_suffix(" out") {
        let prefix = prefix.trim_end();
        if !prefix.is_empty() {
            return prefix;
        }
    }
    trimmed
}
