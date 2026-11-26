//! List, sequence, range, tracker, and state-simulation lowering.

use super::{
    ActiveTracker, AdjacentContext, AstExpr, AstStmt, AstType, GlobalQuantifier, LargestTracker,
    LoweredCond, Lowerer, QuantDomainSpec, RawItem, RawSentence, ScopeKind, SemanticError,
    SemanticErrorKind, SequenceGeneration, SequenceIteration, SequenceRecurrence, SequenceSeed,
    Span, canonical_assignment_target, find_top_level_phrase, is_simple_name, list_type,
    lower_name, named_counter_noun, named_counter_query_noun, render_expr, split_list_elements,
    starts_with_word, strip_leading_word, strip_outer_parentheses, tokenize,
};

impl Lowerer<'_> {
    pub(crate) fn active_move_floor(&self) -> Option<String> {
        self.move_floor.clone()
    }

    pub(crate) fn active_print_tracker(&self) -> Option<(String, bool)> {
        self.print_tracker
            .as_ref()
            .map(|tracker| (tracker.name.clone(), tracker.maximum))
    }

    pub(super) fn ensure_largest_tracker(
        &mut self,
        spec: LargestTracker,
        span: Span,
    ) -> Vec<AstStmt> {
        if self.largest_tracker.is_some() {
            return Vec::new();
        }
        let name = "__tc_extreme_printed".to_string();
        self.declare(&name, AstType::I64);
        self.introduce_name(&name, &AstType::I64, span);
        self.largest_tracker = Some(ActiveTracker {
            name: name.clone(),
            maximum: spec.maximum,
        });
        let init = if spec.maximum {
            AstExpr::ConstInt(i64::MIN, None)
        } else {
            AstExpr::ConstInt(i64::MAX, None)
        };
        vec![AstStmt::Let {
            name,
            ty: Some(AstType::I64),
            init,
            span: self.span(span),
        }]
    }

    pub(super) fn ensure_running_total(&mut self, span: Span) -> Vec<AstStmt> {
        if self.running_total.is_some() {
            return Vec::new();
        }
        let name = "running_total".to_string();
        if self.lookup(&name).is_none() {
            self.declare(&name, AstType::I64);
            self.introduce_name(&name, &AstType::I64, span);
            self.running_total = Some(name.clone());
            return vec![AstStmt::Let {
                name,
                ty: Some(AstType::I64),
                init: AstExpr::ConstInt(0, None),
                span: self.span(span),
            }];
        }
        self.running_total = Some(name);
        Vec::new()
    }

    pub(super) fn ensure_named_counter(&mut self, noun: &str, span: Span) -> Vec<AstStmt> {
        if self.named_counters.contains_key(noun) {
            return Vec::new();
        }
        let name = format!("__tc_count_{noun}");
        self.named_counters.insert(noun.to_string(), name.clone());
        if self.lookup(&name).is_none() {
            self.declare(&name, AstType::I64);
            self.introduce_name(&name, &AstType::I64, span);
            return vec![AstStmt::Let {
                name,
                ty: Some(AstType::I64),
                init: AstExpr::ConstInt(0, None),
                span: self.span(span),
            }];
        }
        Vec::new()
    }

    pub(super) fn scan_accumulators(&mut self, items: &[RawItem], prelude: &mut Vec<AstStmt>) {
        for item in items {
            match item {
                RawItem::Sentence(sentence) => {
                    let lower = sentence.text.to_ascii_lowercase();
                    if lower.contains("running total")
                        || lower.contains("running sum")
                        || lower.contains("running tally")
                    {
                        prelude.extend(self.ensure_running_total(sentence.span));
                    }
                    if let Some(noun) = named_counter_noun(&sentence.text) {
                        prelude.extend(self.ensure_named_counter(&noun, sentence.span));
                    }
                }
                RawItem::Block(block) => {
                    if let Some(header) = &block.header
                        && let Some(noun) = named_counter_noun(&header.text)
                    {
                        prelude.extend(self.ensure_named_counter(&noun, header.span));
                    }
                    self.scan_accumulators(&block.items, prelude);
                }
            }
        }
    }

    pub(super) fn loop_element_type(
        &mut self,
        global: &GlobalQuantifier,
        span: Span,
    ) -> Result<AstType, SemanticError> {
        match &global.domain {
            QuantDomainSpec::Range { start, .. } => {
                let value = self.parse_expr(start, span)?;
                Ok(self.shallow_type(&value))
            }
            QuantDomainSpec::Array { arr } => match self.lookup(arr).cloned() {
                Some(AstType::Array(element, _)) => Ok(*element),
                Some(AstType::Named {
                    name, type_args, ..
                }) if name.eq_ignore_ascii_case("list") => {
                    Ok(type_args.first().cloned().unwrap_or(AstType::I64))
                }
                Some(AstType::Ptr(inner)) => match inner.as_ref() {
                    AstType::Named {
                        name, type_args, ..
                    } if name.eq_ignore_ascii_case("list") => {
                        Ok(type_args.first().cloned().unwrap_or(AstType::I64))
                    }
                    _ => Ok(AstType::I64),
                },
                _ => Ok(AstType::I64),
            },
        }
    }

    pub(super) fn list_element_type(&self, name: &str) -> Option<AstType> {
        match self.lookup(name) {
            Some(AstType::Array(element, _)) => Some((**element).clone()),
            Some(AstType::Named {
                name, type_args, ..
            }) if name.eq_ignore_ascii_case("list") => type_args.first().cloned(),
            Some(AstType::Ptr(inner)) => match inner.as_ref() {
                AstType::Named {
                    name, type_args, ..
                } if name.eq_ignore_ascii_case("list") => type_args.first().cloned(),
                AstType::Array(element, _) => Some((**element).clone()),
                _ => None,
            },
            _ => None,
        }
    }

    pub(super) fn is_list_variable(&self, text: &str) -> bool {
        let trimmed = text.trim();
        is_simple_name(trimmed) && self.list_element_type(&lower_name(trimmed)).is_some()
    }

    pub(super) fn try_swap_adjacent(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<Vec<AstStmt>>, SemanticError> {
        let lower = text
            .trim()
            .trim_end_matches(['.', ','])
            .trim()
            .to_ascii_lowercase();
        if !lower.starts_with("swap ") {
            return Ok(None);
        }
        if !(lower.contains("their positions")
            || lower.contains("their places")
            || lower.contains("the two")
            || lower.contains("the 2")
            || lower.contains(" two")
            || lower.contains(" 2")
            || lower.contains("them")
            || lower.contains("adjacent"))
        {
            return Ok(None);
        }
        let Some(context) = self.adjacent.clone() else {
            return Ok(None);
        };
        let data = self.parse_expr(&format!("{}'s data", context.list), span)?;
        let index = AstExpr::Var(context.index.clone());
        let next = AstExpr::Binary {
            op: tc_ir::BinOp::Add,
            lhs: Box::new(index.clone()),
            rhs: Box::new(AstExpr::ConstInt(1, None)),
        };
        let temporary = self.next_temp("swap_value");
        let first = AstExpr::Load(Box::new(AstExpr::IndexAddr {
            base_ptr: Box::new(data.clone()),
            index: Box::new(index.clone()),
        }));
        let second = AstExpr::Load(Box::new(AstExpr::IndexAddr {
            base_ptr: Box::new(data.clone()),
            index: Box::new(next.clone()),
        }));
        Ok(Some(vec![
            AstStmt::Let {
                name: temporary.clone(),
                ty: Some(context.element.clone()),
                init: first,
                span: self.span(span),
            },
            AstStmt::Store {
                ptr: AstExpr::IndexAddr {
                    base_ptr: Box::new(data.clone()),
                    index: Box::new(index),
                },
                val: second,
                span: self.span(span),
            },
            AstStmt::Store {
                ptr: AstExpr::IndexAddr {
                    base_ptr: Box::new(data),
                    index: Box::new(next),
                },
                val: AstExpr::Var(temporary),
                span: self.span(span),
            },
        ]))
    }

    pub(super) fn emit_list_elements(
        &mut self,
        list: &str,
        span: Span,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        let Some(element) = self.list_element_type(list) else {
            return Ok(Vec::new());
        };
        let data = self.parse_expr(&format!("{list}'s data"), span)?;
        let len = self.parse_expr(&format!("{list}'s len"), span)?;
        let index = self.next_temp("print_index");
        let value = self.next_temp("print_value");
        self.declare_temp(&index, AstType::U64);
        self.declare_temp(&value, element.clone());
        let mut loop_body = vec![
            AstStmt::If {
                cond: AstExpr::Binary {
                    op: tc_ir::BinOp::Ge,
                    lhs: Box::new(AstExpr::Var(index.clone())),
                    rhs: Box::new(len.clone()),
                },
                then_body: vec![AstStmt::Break(self.span(span))],
                else_body: Vec::new(),
                span: self.span(span),
            },
            AstStmt::Assign {
                name: value.clone(),
                val: AstExpr::Load(Box::new(AstExpr::IndexAddr {
                    base_ptr: Box::new(data.clone()),
                    index: Box::new(AstExpr::Var(index.clone())),
                })),
                span: self.span(span),
            },
        ];
        let print_name = match element {
            AstType::U8 | AstType::U16 | AstType::U32 | AstType::U64 => "print_u64",
            AstType::F32 | AstType::F64 => "print_f64",
            AstType::Bool => "print_bool",
            _ => "print_i64",
        };
        loop_body.push(AstStmt::Expr(
            AstExpr::Call {
                module: None,
                name: print_name.to_string(),
                type_args: Vec::new(),
                args: vec![AstExpr::Var(value.clone())],
            },
            self.span(span),
        ));
        loop_body.push(AstStmt::Assign {
            name: index.clone(),
            val: AstExpr::Binary {
                op: tc_ir::BinOp::Add,
                lhs: Box::new(AstExpr::Var(index.clone())),
                rhs: Box::new(AstExpr::ConstInt(1, None)),
            },
            span: self.span(span),
        });
        Ok(vec![AstStmt::Block(
            vec![
                AstStmt::Let {
                    name: index,
                    ty: Some(AstType::U64),
                    init: AstExpr::ConstInt(0, None),
                    span: self.span(span),
                },
                AstStmt::Let {
                    name: value,
                    ty: Some(element),
                    init: AstExpr::ConstInt(0, None),
                    span: self.span(span),
                },
                AstStmt::Loop {
                    body: loop_body,
                    span: self.span(span),
                },
            ],
            self.span(span),
        )])
    }

    pub(super) fn lower_list_cleanup(
        &mut self,
        list: &str,
        predicate: &str,
        span: Span,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        let Some(element) = self.list_element_type(list) else {
            return Ok(Vec::new());
        };
        let data = self.parse_expr(&format!("{list}'s data"), span)?;
        let len = self.parse_expr(&format!("{list}'s len"), span)?;
        let read = self.next_temp("read");
        let write = self.next_temp("write");
        let value = self.next_temp("filter_value");
        self.declare_temp(&read, AstType::U64);
        self.declare_temp(&write, AstType::U64);
        self.declare_temp(&value, element.clone());
        let condition = self.lower_condition(&format!("{value} is not {predicate}"), span)?;
        let mut loop_body = vec![
            AstStmt::If {
                cond: AstExpr::Binary {
                    op: tc_ir::BinOp::Ge,
                    lhs: Box::new(AstExpr::Var(read.clone())),
                    rhs: Box::new(len.clone()),
                },
                then_body: vec![AstStmt::Break(self.span(span))],
                else_body: Vec::new(),
                span: self.span(span),
            },
            AstStmt::Assign {
                name: value.clone(),
                val: AstExpr::Load(Box::new(AstExpr::IndexAddr {
                    base_ptr: Box::new(data.clone()),
                    index: Box::new(AstExpr::Var(read.clone())),
                })),
                span: self.span(span),
            },
        ];
        loop_body.extend(condition.prelude);
        loop_body.push(AstStmt::If {
            cond: condition.expr,
            then_body: vec![
                AstStmt::Store {
                    ptr: AstExpr::IndexAddr {
                        base_ptr: Box::new(data),
                        index: Box::new(AstExpr::Var(write.clone())),
                    },
                    val: AstExpr::Var(value.clone()),
                    span: self.span(span),
                },
                AstStmt::Assign {
                    name: write.clone(),
                    val: AstExpr::Binary {
                        op: tc_ir::BinOp::Add,
                        lhs: Box::new(AstExpr::Var(write.clone())),
                        rhs: Box::new(AstExpr::ConstInt(1, None)),
                    },
                    span: self.span(span),
                },
            ],
            else_body: Vec::new(),
            span: self.span(span),
        });
        loop_body.push(AstStmt::Assign {
            name: read.clone(),
            val: AstExpr::Binary {
                op: tc_ir::BinOp::Add,
                lhs: Box::new(AstExpr::Var(read.clone())),
                rhs: Box::new(AstExpr::ConstInt(1, None)),
            },
            span: self.span(span),
        });
        let mut statements = vec![
            AstStmt::Let {
                name: read,
                ty: Some(AstType::U64),
                init: AstExpr::ConstInt(0, None),
                span: self.span(span),
            },
            AstStmt::Let {
                name: write.clone(),
                ty: Some(AstType::U64),
                init: AstExpr::ConstInt(0, None),
                span: self.span(span),
            },
            AstStmt::Let {
                name: value,
                ty: Some(element),
                init: AstExpr::ConstInt(0, None),
                span: self.span(span),
            },
            AstStmt::Loop {
                body: loop_body,
                span: self.span(span),
            },
        ];
        statements.extend(self.assign_text(
            &format!("{list}'s len"),
            AstExpr::Var(write),
            span,
            false,
        )?);
        Ok(statements)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_adjacent_pass(
        &mut self,
        list: &str,
        body_items: Vec<RawItem>,
        cleanup: Option<String>,
        closer: Option<(String, Span)>,
        span: Span,
        depth: usize,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        let Some(element) = self.list_element_type(list) else {
            return Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                format!("`{list}` is not a list"),
            ));
        };
        let data = self.parse_expr(&format!("{list}'s data"), span)?;
        let len = self.parse_expr(&format!("{list}'s len"), span)?;
        let index = self.next_temp("pass_index");
        let left = self.next_temp("left");
        let right = self.next_temp("right");
        self.declare_temp(&index, AstType::U64);
        self.declare_temp(&left, element.clone());
        self.declare_temp(&right, element.clone());
        let mut statements = vec![
            AstStmt::Let {
                name: index.clone(),
                ty: Some(AstType::U64),
                init: AstExpr::ConstInt(0, None),
                span: self.span(span),
            },
            AstStmt::Let {
                name: left.clone(),
                ty: Some(element.clone()),
                init: AstExpr::ConstInt(0, None),
                span: self.span(span),
            },
            AstStmt::Let {
                name: right.clone(),
                ty: Some(element.clone()),
                init: AstExpr::ConstInt(0, None),
                span: self.span(span),
            },
        ];
        self.scan_accumulators(&body_items, &mut statements);
        let previous_adjacent = self.adjacent.clone();
        self.adjacent = Some(AdjacentContext {
            list: list.to_string(),
            index: index.clone(),
            left: left.clone(),
            right: right.clone(),
            element: element.clone(),
        });
        self.last_list = Some(list.to_string());
        let body = self.lower_items(&body_items, depth + 1)?;
        self.adjacent = previous_adjacent;
        let next = AstExpr::Binary {
            op: tc_ir::BinOp::Add,
            lhs: Box::new(AstExpr::Var(index.clone())),
            rhs: Box::new(AstExpr::ConstInt(1, None)),
        };
        let mut loop_body = vec![
            AstStmt::If {
                cond: AstExpr::Binary {
                    op: tc_ir::BinOp::Ge,
                    lhs: Box::new(next.clone()),
                    rhs: Box::new(len),
                },
                then_body: vec![AstStmt::Break(self.span(span))],
                else_body: Vec::new(),
                span: self.span(span),
            },
            AstStmt::Assign {
                name: left.clone(),
                val: AstExpr::Load(Box::new(AstExpr::IndexAddr {
                    base_ptr: Box::new(data.clone()),
                    index: Box::new(AstExpr::Var(index.clone())),
                })),
                span: self.span(span),
            },
            AstStmt::Assign {
                name: right.clone(),
                val: AstExpr::Load(Box::new(AstExpr::IndexAddr {
                    base_ptr: Box::new(data),
                    index: Box::new(next),
                })),
                span: self.span(span),
            },
            AstStmt::Block(body, self.span(span)),
            AstStmt::Assign {
                name: index.clone(),
                val: AstExpr::Binary {
                    op: tc_ir::BinOp::Add,
                    lhs: Box::new(AstExpr::Var(index)),
                    rhs: Box::new(AstExpr::ConstInt(1, None)),
                },
                span: self.span(span),
            },
        ];
        loop_body
            .retain(|statement| !matches!(statement, AstStmt::Block(items, _) if items.is_empty()));
        statements.push(AstStmt::Loop {
            body: loop_body,
            span: self.span(span),
        });
        if let Some(predicate) = cleanup {
            statements.extend(self.lower_list_cleanup(list, &predicate, span)?);
        }
        if let Some((rest, closer_span)) = closer {
            statements.extend(self.lower_core(&rest, closer_span, None, None, depth + 1)?);
        }
        Ok(statements)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_sequence(
        &mut self,
        seed: SequenceSeed,
        generation: SequenceGeneration,
        iteration: SequenceIteration,
        mut body_items: Vec<RawItem>,
        closer: Option<(String, Span)>,
        span: Span,
        depth: usize,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        if seed.values.len() != 2 {
            return Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                "a sum-of-previous-two sequence needs exactly two starting values",
            ));
        }
        if generation.recurrence != SequenceRecurrence::SumPreviousTwo {
            return Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                "unsupported sequence recurrence",
            ));
        }
        if let Some(text) = iteration.body {
            body_items.insert(
                0,
                RawItem::Sentence(RawSentence {
                    text: text.clone(),
                    raw: text,
                    span,
                    fillers: Vec::new(),
                }),
            );
        }
        let mut prelude = Vec::new();
        self.scan_accumulators(&body_items, &mut prelude);
        let first = self.parse_expr(&seed.values[0], span)?;
        let second = self.parse_expr(&seed.values[1], span)?;
        let ty = self.shallow_type(&first);
        if !Self::is_integer_type(&ty) {
            return Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                "sequence starting values must be integers",
            ));
        }
        let bound = self.parse_expr(&generation.bound, span)?;
        let prev2 = self.next_temp("seq_prev2");
        let prev1 = self.next_temp("seq_prev1");
        let next = self.next_temp("seq_next");
        self.declare_temp(&prev2, ty.clone());
        self.declare_temp(&prev1, ty.clone());
        self.declare_temp(&next, ty.clone());
        self.declare(&iteration.var, ty.clone());
        self.introduce_name(&iteration.var, &ty, span);
        prelude.push(AstStmt::Let {
            name: prev2.clone(),
            ty: Some(ty.clone()),
            init: first.clone(),
            span: self.span(span),
        });
        prelude.push(AstStmt::Let {
            name: prev1.clone(),
            ty: Some(ty.clone()),
            init: second.clone(),
            span: self.span(span),
        });
        prelude.push(AstStmt::Let {
            name: next.clone(),
            ty: Some(ty.clone()),
            init: AstExpr::ConstInt(0, None),
            span: self.span(span),
        });
        prelude.push(AstStmt::Let {
            name: iteration.var.clone(),
            ty: Some(ty.clone()),
            init: first.clone(),
            span: self.span(span),
        });
        self.push_scope(ScopeKind::QuantifierBody);
        self.declare(&iteration.var, ty.clone());
        let previous_loop_var = self.active_loop_var.replace(iteration.var.clone());
        let body = self.lower_items(&body_items, depth + 1)?;
        self.active_loop_var = previous_loop_var;
        self.pop_scope();
        let body_block = || AstStmt::Block(body.clone(), self.span(span));
        let mut statements = prelude;
        statements.push(AstStmt::Assign {
            name: iteration.var.clone(),
            val: first,
            span: self.span(span),
        });
        statements.push(body_block());
        statements.push(AstStmt::Assign {
            name: iteration.var.clone(),
            val: second,
            span: self.span(span),
        });
        statements.push(body_block());
        let comparison = if generation.inclusive {
            tc_ir::BinOp::Le
        } else {
            tc_ir::BinOp::Lt
        };
        let mut loop_body = vec![
            AstStmt::Assign {
                name: next.clone(),
                val: AstExpr::Binary {
                    op: tc_ir::BinOp::Add,
                    lhs: Box::new(AstExpr::Var(prev2.clone())),
                    rhs: Box::new(AstExpr::Var(prev1.clone())),
                },
                span: self.span(span),
            },
            AstStmt::If {
                cond: AstExpr::Unary {
                    op: tc_ir::UnOp::LogNot,
                    val: Box::new(AstExpr::Binary {
                        op: comparison,
                        lhs: Box::new(AstExpr::Var(next.clone())),
                        rhs: Box::new(bound),
                    }),
                },
                then_body: vec![AstStmt::Break(self.span(span))],
                else_body: Vec::new(),
                span: self.span(span),
            },
            AstStmt::Assign {
                name: iteration.var.clone(),
                val: AstExpr::Var(next.clone()),
                span: self.span(span),
            },
            body_block(),
            AstStmt::Assign {
                name: prev2.clone(),
                val: AstExpr::Var(prev1.clone()),
                span: self.span(span),
            },
            AstStmt::Assign {
                name: prev1.clone(),
                val: AstExpr::Var(next.clone()),
                span: self.span(span),
            },
        ];
        loop_body
            .retain(|statement| !matches!(statement, AstStmt::Block(items, _) if items.is_empty()));
        statements.push(AstStmt::Loop {
            body: loop_body,
            span: self.span(span),
        });
        self.last_iteration_var = Some(iteration.var.clone());
        if let Some((rest, closer_span)) = closer {
            statements.extend(self.lower_core(&rest, closer_span, None, None, depth + 1)?);
        }
        Ok(statements)
    }

    pub(super) fn ensure_implicit_counter(
        &mut self,
        span: Span,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        if self.implicit_counter.is_some() {
            return Ok(Vec::new());
        }
        let name = "__tc_implicit_count".to_string();
        self.declare(&name, AstType::I64);
        self.implicit_counter = Some(name.clone());
        Ok(vec![AstStmt::Let {
            name,
            ty: Some(AstType::I64),
            init: AstExpr::ConstInt(0, None),
            span: self.span(span),
        }])
    }

    pub(super) fn try_implicit_count(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<Vec<AstStmt>>, SemanticError> {
        let rest = strip_leading_word(text, "count").trim();
        let lower = rest.to_ascii_lowercase();
        if rest.is_empty()
            || starts_with_word(&lower, "of")
            || starts_with_word(&lower, "the number")
            || starts_with_word(&lower, "how")
            || lower.starts_with('=')
            || lower.starts_with("+=")
            || lower.starts_with("-=")
            || lower.starts_with("*=")
            || lower.starts_with("/=")
        {
            return Ok(None);
        }
        let object_word = tokenize(rest)
            .into_iter()
            .find(|token| token.is_word())
            .map(|token| token.lower);
        if object_word.as_ref().is_some_and(|word| {
            matches!(
                word.as_str(),
                "it" | "them" | "that" | "this" | "those" | "these"
            ) || self.lookup(word).is_some()
                || self.globals.contains_key(word)
                || self.active_loop_var.as_ref().is_some_and(|var| var == word)
        }) {
            let mut statements = self.ensure_implicit_counter(span)?;
            let Some(counter) = self.implicit_counter.clone() else {
                return Ok(None);
            };
            statements.push(AstStmt::Assign {
                name: counter.clone(),
                val: AstExpr::Binary {
                    op: tc_ir::BinOp::Add,
                    lhs: Box::new(AstExpr::Var(counter)),
                    rhs: Box::new(AstExpr::ConstInt(1, None)),
                },
                span: self.span(span),
            });
            return Ok(Some(statements));
        }
        if let Some(noun) = named_counter_noun(text) {
            let mut statements = self.ensure_named_counter(&noun, span);
            let Some(counter) = self.named_counters.get(&noun).cloned() else {
                return Ok(None);
            };
            statements.push(AstStmt::Assign {
                name: counter.clone(),
                val: AstExpr::Binary {
                    op: tc_ir::BinOp::Add,
                    lhs: Box::new(AstExpr::Var(counter)),
                    rhs: Box::new(AstExpr::ConstInt(1, None)),
                },
                span: self.span(span),
            });
            return Ok(Some(statements));
        }
        let mut statements = self.ensure_implicit_counter(span)?;
        let Some(counter) = self.implicit_counter.clone() else {
            return Ok(None);
        };
        statements.push(AstStmt::Assign {
            name: counter.clone(),
            val: AstExpr::Binary {
                op: tc_ir::BinOp::Add,
                lhs: Box::new(AstExpr::Var(counter)),
                rhs: Box::new(AstExpr::ConstInt(1, None)),
            },
            span: self.span(span),
        });
        Ok(Some(statements))
    }

    pub(super) fn count_query(
        &self,
        text: &str,
        span: Span,
    ) -> Result<Option<AstExpr>, SemanticError> {
        let lower = text
            .trim()
            .trim_end_matches(['.', ',', ':'])
            .trim()
            .to_ascii_lowercase();
        let query = (lower.starts_with("how many ")
            && (lower.contains("were found")
                || lower.contains("were counted")
                || lower.contains("matched")
                || lower.contains("there were")
                || lower.contains("there are")))
            || matches!(
                lower.as_str(),
                "the total found"
                    | "total found"
                    | "the total counted"
                    | "total counted"
                    | "how many were found"
            );
        if !query {
            let Some(noun) = named_counter_query_noun(&lower) else {
                return Ok(None);
            };
            let Some(counter) = self.named_counters.get(&noun) else {
                return Err(self.error(
                    span,
                    SemanticErrorKind::Lowering,
                    format!(
                        "UNRESOLVED_ANAPHOR: count query `{noun}` has no preceding `count {noun}`"
                    ),
                ));
            };
            return Ok(Some(AstExpr::Var(counter.clone())));
        }
        let Some(counter) = &self.implicit_counter else {
            return Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                "UNRESOLVED_ANAPHOR: count query has no preceding implicit `count`",
            ));
        };
        Ok(Some(AstExpr::Var(counter.clone())))
    }

    pub(super) fn parse_natural_membership(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<LoweredCond>, SemanticError> {
        for (phrase, kind, negated) in [
            ("does not start with", "starts", true),
            ("does not end with", "ends", true),
            ("does not contain", "contains", true),
            ("starts with", "starts", false),
            ("ends with", "ends", false),
            ("contains", "contains", false),
        ] {
            let Some((left, end)) = find_top_level_phrase(text, phrase) else {
                continue;
            };
            let right = text[end..].trim();
            if left.trim().is_empty() || right.is_empty() {
                continue;
            }
            let left_value = self.parse_value(left.trim(), span)?;
            let right_value = self.parse_value(right, span)?;
            let mut prelude = Vec::new();
            let expression = if kind == "contains" {
                if self.is_list_value(&left_value) {
                    let pointer = self.list_pointer(left_value, span)?;
                    AstExpr::Call {
                        module: None,
                        name: "list_contains_i64".to_string(),
                        type_args: Vec::new(),
                        args: vec![pointer, right_value],
                    }
                } else {
                    let lhs = self.string_pointer(left_value, span, &mut prelude)?;
                    let rhs = self.string_pointer(right_value, span, &mut prelude)?;
                    AstExpr::Call {
                        module: None,
                        name: "str_contains".to_string(),
                        type_args: Vec::new(),
                        args: vec![lhs, rhs],
                    }
                }
            } else {
                let lhs = self.string_pointer(left_value, span, &mut prelude)?;
                let rhs = self.string_pointer(right_value, span, &mut prelude)?;
                let name = if kind == "starts" {
                    "str_starts_with"
                } else {
                    "str_ends_with"
                };
                AstExpr::Call {
                    module: None,
                    name: name.to_string(),
                    type_args: Vec::new(),
                    args: vec![lhs, rhs],
                }
            };
            let expression = if negated {
                AstExpr::Unary {
                    op: tc_ir::UnOp::LogNot,
                    val: Box::new(expression),
                }
            } else {
                expression
            };
            let paraphrase = strip_outer_parentheses(&render_expr(&expression));
            return Ok(Some(LoweredCond {
                prelude,
                expr: expression,
                paraphrase,
            }));
        }
        Ok(None)
    }

    pub(super) fn string_pointer(
        &mut self,
        value: AstExpr,
        span: Span,
        prelude: &mut Vec<AstStmt>,
    ) -> Result<AstExpr, SemanticError> {
        let ty = self.shallow_type_opt(&value);
        if matches!(ty, Some(AstType::Ptr(ref inner)) if self.is_tcstring_type(inner)) {
            return Ok(value);
        }
        if matches!(ty, Some(AstType::Named { ref name, .. }) if name.eq_ignore_ascii_case("tcstring"))
        {
            return match value {
                AstExpr::Var(name) => Ok(AstExpr::AddrOfVar(name)),
                other => self.materialize_string(other, span, prelude),
            };
        }
        if let Some((pointer, length)) = self.text_bytes(&value) {
            let string = AstExpr::Call {
                module: None,
                name: "str_from_bytes".to_string(),
                type_args: Vec::new(),
                args: vec![pointer, length],
            };
            return self.materialize_string(string, span, prelude);
        }
        Err(self.error(
            span,
            SemanticErrorKind::Lowering,
            "text predicate requires a text value",
        ))
    }

    pub(super) fn materialize_string(
        &mut self,
        value: AstExpr,
        span: Span,
        prelude: &mut Vec<AstStmt>,
    ) -> Result<AstExpr, SemanticError> {
        let temp = self.next_temp("str");
        prelude.push(AstStmt::Let {
            name: temp.clone(),
            ty: Some(self.tcstring_type()),
            init: value,
            span: self.span(span),
        });
        Ok(AstExpr::AddrOfVar(temp))
    }

    pub(super) fn is_tcstring_type(&self, ty: &AstType) -> bool {
        matches!(ty, AstType::Named { name, .. } if name.eq_ignore_ascii_case("tcstring"))
    }

    pub(super) fn tcstring_type(&self) -> AstType {
        AstType::Named {
            module: None,
            name: "tcstring".to_string(),
            type_args: Vec::new(),
        }
    }

    pub(super) fn is_list_value(&self, value: &AstExpr) -> bool {
        match self.shallow_type_opt(value) {
            Some(AstType::Named { name, .. }) => name.eq_ignore_ascii_case("list"),
            Some(AstType::Ptr(inner)) => {
                matches!(inner.as_ref(), AstType::Named { name, .. } if name.eq_ignore_ascii_case("list"))
            }
            _ => false,
        }
    }

    pub(super) fn list_pointer(
        &mut self,
        value: AstExpr,
        span: Span,
    ) -> Result<AstExpr, SemanticError> {
        match value {
            AstExpr::Var(name) => Ok(AstExpr::AddrOfVar(name)),
            other if matches!(self.shallow_type_opt(&other), Some(AstType::Ptr(_))) => Ok(other),
            _ => Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                "contains on a list requires a list variable",
            )),
        }
    }

    pub(super) fn parse_inline_list(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<(AstType, Vec<AstExpr>)>, SemanticError> {
        let lower = text.trim().to_ascii_lowercase();
        let empty = lower.starts_with("an empty list of ")
            || lower.starts_with("a empty list of ")
            || lower.starts_with("empty list of ");
        if empty {
            let type_text = text
                .trim()
                .split_once(" of ")
                .map_or("", |(_, rest)| rest)
                .trim()
                .to_ascii_lowercase();
            let element = if type_text.contains("decimal") || type_text.contains("real") {
                AstType::F64
            } else if type_text.contains("text")
                || type_text.contains("string")
                || type_text.contains("word")
            {
                AstType::Named {
                    module: None,
                    name: "tcstring".to_string(),
                    type_args: Vec::new(),
                }
            } else {
                AstType::I64
            };
            return Ok(Some((list_type(element), Vec::new())));
        }
        let Some(rest) = text
            .trim()
            .strip_prefix("a list of ")
            .or_else(|| text.trim().strip_prefix("A list of "))
        else {
            return Ok(None);
        };
        let parts = split_list_elements(rest);
        if parts.is_empty() {
            return Err(self.error(span, SemanticErrorKind::Lowering, "empty inline list"));
        }
        let first_text = parts[0].trim();
        let text_elements = first_text.starts_with('"') || first_text.starts_with('\'');
        let first = self.parse_value(first_text, span)?;
        let element_ty = if text_elements {
            AstType::Named {
                module: None,
                name: "tcstring".to_string(),
                type_args: Vec::new(),
            }
        } else {
            self.shallow_type(&first)
        };
        let mut elements = Vec::new();
        for part in parts {
            let value = self.parse_value(&part, span)?;
            if matches!(
                element_ty,
                AstType::Named { ref name, .. } if name.eq_ignore_ascii_case("tcstring")
            ) {
                elements.push(match value {
                    AstExpr::BytesLit(bytes) => AstExpr::Call {
                        module: None,
                        name: "str_from_bytes".to_string(),
                        type_args: Vec::new(),
                        args: vec![AstExpr::BytesLit(bytes.clone()), AstExpr::BytesLen(bytes)],
                    },
                    other => other,
                });
            } else {
                elements.push(value);
            }
        }
        Ok(Some((list_type(element_ty), elements)))
    }

    pub(super) fn lower_inline_list(
        &mut self,
        target: &str,
        ty: AstType,
        elements: Vec<AstExpr>,
        span: Span,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        let canonical = canonical_assignment_target(target);
        if !is_simple_name(&canonical) {
            return Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                "inline list target must be a simple variable",
            ));
        }
        let name = lower_name(&canonical);
        let mut statements = self.assign_text(&name, AstExpr::Zero(ty), span, true)?;
        let pointer = if self.globals.contains_key(&name) && !self.current_scope_contains(&name) {
            AstExpr::GlobalAddr {
                module: None,
                name: name.clone(),
            }
        } else {
            AstExpr::AddrOfVar(name)
        };
        for element in elements {
            statements.push(AstStmt::Expr(
                AstExpr::Call {
                    module: None,
                    name: "list_push".to_string(),
                    type_args: Vec::new(),
                    args: vec![element, pointer.clone()],
                },
                self.span(span),
            ));
        }
        Ok(statements)
    }
}
