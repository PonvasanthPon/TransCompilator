//! List, sequence, range, tracker, and state-simulation lowering.

use super::{
    ActiveTracker, AdjacentContext, AstExpr, AstStmt, AstType, GlobalQuantifier, LargestTracker,
    LoweredCond, Lowerer, QuantDomainSpec, RawItem, RawSentence, ScopeKind, SemanticError,
    SemanticErrorKind, SequenceGeneration, SequenceIteration, SequenceRecurrence, SequenceSeed,
    Span, canonical_assignment_target, find_top_level_phrase, find_top_level_word, is_simple_name,
    list_type, lower_name, named_counter_noun, named_counter_query_noun, render_expr,
    split_list_elements, starts_with_word, strip_leading_word, strip_outer_parentheses, tokenize,
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

    fn is_named_list_variable(&self, name: &str) -> bool {
        match self.lookup(name) {
            Some(AstType::Named { name, .. }) => name.eq_ignore_ascii_case("list"),
            Some(AstType::Ptr(inner)) => {
                matches!(inner.as_ref(), AstType::Named { name, .. } if name.eq_ignore_ascii_case("list"))
            }
            _ => false,
        }
    }

    fn resolve_list_name(&self, text: &str) -> Option<String> {
        let trimmed = text.trim().trim_end_matches('.').trim();
        let lower = trimmed.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "the list"
                | "the list of numbers"
                | "the list of values"
                | "the list of items"
                | "the numbers"
                | "the values"
                | "the items"
        ) {
            return self
                .last_list
                .clone()
                .filter(|name| self.is_named_list_variable(name));
        }
        let name = lower_name(trimmed);
        self.is_named_list_variable(&name).then_some(name)
    }

    fn natural_list_index(&self, index: AstExpr) -> AstExpr {
        if matches!(self.shallow_type_opt(&index), Some(AstType::U64)) {
            return index;
        }
        AstExpr::Cast {
            val: Box::new(index),
            to: AstType::U64,
        }
    }

    fn list_call(&self, name: &str, args: Vec<AstExpr>, span: Span) -> AstStmt {
        AstStmt::Expr(
            AstExpr::Call {
                module: None,
                name: name.to_string(),
                type_args: Vec::new(),
                args,
            },
            self.span(span),
        )
    }

    pub(super) fn is_natural_list_operation(&self, text: &str) -> bool {
        let lower = text
            .trim()
            .trim_end_matches('.')
            .trim()
            .to_ascii_lowercase();
        lower.starts_with("set item ")
            || lower.starts_with("replace item ")
            || lower.starts_with("append ")
            || lower.starts_with("add ") && lower.contains(" to the end of ")
            || lower.starts_with("remove the last ")
            || lower.starts_with("remove item ")
            || lower.starts_with("remove every ")
            || lower.starts_with("reverse ")
            || lower.starts_with("sort ")
    }

    pub(super) fn try_list_operation(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<AstStmt>, SemanticError> {
        let trimmed = text.trim().trim_end_matches('.').trim();
        let lower = trimmed.to_ascii_lowercase();

        let assignment = if lower.starts_with("set item ") {
            Some((&trimmed["set item ".len()..], "to"))
        } else if lower.starts_with("replace item ") {
            Some((&trimmed["replace item ".len()..], "with"))
        } else {
            None
        };
        if let Some((rest, separator)) = assignment {
            let Some((of_start, of_end)) = find_top_level_word(rest, "of") else {
                return Ok(None);
            };
            let index_text = rest[..of_start].trim();
            let after = rest[of_end..].trim();
            let Some((list_end, list_end_end)) = find_top_level_word(after, separator) else {
                return Ok(None);
            };
            let list_text = after[..list_end].trim();
            let value_text = after[list_end_end..].trim();
            let Some(list) = self.resolve_list_name(list_text) else {
                return Ok(None);
            };
            let index = self.parse_expr(index_text, span)?;
            let value = self.parse_expr(value_text, span)?;
            let pointer = self.list_pointer(AstExpr::Var(list), span)?;
            return Ok(Some(self.list_call(
                "list_set",
                vec![pointer, self.natural_list_index(index), value],
                span,
            )));
        }

        if lower.starts_with("append ") {
            let rest = &trimmed["append ".len()..];
            if let Some((to_start, to_end)) = find_top_level_word(rest, "to") {
                let value_text = rest[..to_start].trim();
                let list_text = rest[to_end..].trim();
                if let Some(list) = self.resolve_list_name(list_text) {
                    let mut value = self.parse_expr(value_text, span)?;
                    if matches!(
                        self.list_element_type(&list),
                        Some(AstType::Named { ref name, .. })
                            if name.eq_ignore_ascii_case("tcstring")
                    ) {
                        value = self.as_text_value(value, span)?;
                    }
                    let pointer = self.list_pointer(AstExpr::Var(list), span)?;
                    return Ok(Some(self.list_call(
                        "list_push",
                        vec![value, pointer],
                        span,
                    )));
                }
                if let Some(item_text) = list_text.strip_prefix("item ")
                    && let Some((of_start, of_end)) = find_top_level_word(item_text, "of")
                {
                    let index_text = item_text[..of_start].trim();
                    let list_name_text = item_text[of_end..].trim();
                    if let Some(list) = self.resolve_list_name(list_name_text) {
                        let Some(element) = self.list_element_type(&list) else {
                            return Ok(None);
                        };
                        if !matches!(
                            element,
                            AstType::Named { ref name, .. }
                                if name.eq_ignore_ascii_case("tcstring")
                        ) {
                            return Ok(None);
                        }
                        let pointer = self.list_pointer(AstExpr::Var(list), span)?;
                        let index = self.parse_expr(index_text, span)?;
                        let index = self.natural_list_index(index);
                        let temporary = self.next_temp("append_item");
                        self.declare_temp(&temporary, element);
                        let mut statements = vec![AstStmt::Let {
                            name: temporary.clone(),
                            ty: None,
                            init: AstExpr::Call {
                                module: None,
                                name: "list_get".to_string(),
                                type_args: Vec::new(),
                                args: vec![pointer.clone(), index.clone()],
                            },
                            span: self.span(span),
                        }];
                        let append_text = format!("append {value_text} to {temporary}");
                        if let Some(append) = self.try_string_operation(&append_text, span)? {
                            statements.extend(append);
                        } else {
                            return Ok(None);
                        }
                        statements.push(AstStmt::Expr(
                            AstExpr::Call {
                                module: None,
                                name: "list_set".to_string(),
                                type_args: Vec::new(),
                                args: vec![pointer, index, AstExpr::Var(temporary)],
                            },
                            self.span(span),
                        ));
                        let block_span = self.span(span);
                        return Ok(Some(AstStmt::Block(statements, block_span)));
                    }
                }
            }
        }

        if lower.starts_with("add ") {
            if let Some((to_start, to_end)) = find_top_level_word(trimmed, "to") {
                let value_text = trimmed[3..to_start].trim();
                let after = trimmed[to_end..].trim();
                if let Some(list_text) = after
                    .strip_prefix("the end of ")
                    .or_else(|| after.strip_prefix("The end of "))
                {
                    if let Some(list) = self.resolve_list_name(list_text) {
                        let value = self.parse_expr(value_text, span)?;
                        let pointer = self.list_pointer(AstExpr::Var(list), span)?;
                        return Ok(Some(self.list_call(
                            "list_push",
                            vec![value, pointer],
                            span,
                        )));
                    }
                }
            }
        }

        if lower.starts_with("remove the last ") {
            let rest = trimmed["remove the last ".len()..].trim();
            for noun in ["item from ", "number from ", "element from ", "value from "] {
                if rest.to_ascii_lowercase().starts_with(noun) {
                    let list_text = rest[noun.len()..].trim();
                    if let Some(list) = self.resolve_list_name(list_text) {
                        let pointer = self.list_pointer(AstExpr::Var(list), span)?;
                        return Ok(Some(self.list_call("list_pop", vec![pointer], span)));
                    }
                }
            }
        }

        if lower.starts_with("remove item ") {
            let rest = &trimmed["remove item ".len()..];
            if let Some((from_start, from_end)) =
                find_top_level_word(rest, "from").or_else(|| find_top_level_word(rest, "of"))
            {
                let index_text = rest[..from_start].trim();
                let list_text = rest[from_end..].trim();
                if let Some(list) = self.resolve_list_name(list_text) {
                    let index = self.parse_expr(index_text, span)?;
                    let pointer = self.list_pointer(AstExpr::Var(list), span)?;
                    return Ok(Some(self.list_call(
                        "list_remove_at",
                        vec![self.natural_list_index(index), pointer],
                        span,
                    )));
                }
            }
        }

        if lower.starts_with("remove every ") {
            let rest = &trimmed["remove every ".len()..];
            if let Some((from_start, from_end)) = find_top_level_word(rest, "from") {
                let value_text = rest[..from_start].trim();
                let list_text = rest[from_end..].trim();
                if let Some(list) = self.resolve_list_name(list_text) {
                    if matches!(self.list_element_type(&list), Some(AstType::I64)) {
                        let value = self.parse_expr(value_text, span)?;
                        let pointer = self.list_pointer(AstExpr::Var(list), span)?;
                        return Ok(Some(self.list_call(
                            "list_remove_value_i64",
                            vec![value, pointer],
                            span,
                        )));
                    }
                }
            }
        }

        if lower.starts_with("reverse ") {
            let list_text = &trimmed["reverse ".len()..];
            let name = lower_name(list_text.trim());
            if self.list_element_type(&name).is_some() {
                let pointer = self.list_pointer(AstExpr::Var(name), span)?;
                return Ok(Some(self.list_call("list_reverse", vec![pointer], span)));
            }
        }

        if lower.starts_with("sort ") && !lower.contains(" using ") {
            let rest = &trimmed["sort ".len()..];
            let list_text = rest
                .strip_prefix("the numbers in ")
                .or_else(|| rest.strip_prefix("The numbers in "))
                .or_else(|| rest.strip_prefix("the list "))
                .unwrap_or(rest)
                .trim();
            if let Some(list) = self.resolve_list_name(list_text) {
                if matches!(self.list_element_type(&list), Some(AstType::I64)) {
                    let pointer = self.list_pointer(AstExpr::Var(list), span)?;
                    return Ok(Some(self.list_call("list_sort_i64", vec![pointer], span)));
                }
            }
        }

        Ok(None)
    }

    fn is_text_variable(&self, name: &str) -> bool {
        match self.lookup(name) {
            Some(AstType::Named { name, .. }) => name.eq_ignore_ascii_case("tcstring"),
            Some(AstType::Ptr(inner)) => matches!(
                inner.as_ref(),
                AstType::Named { name, .. } if name.eq_ignore_ascii_case("tcstring")
            ),
            _ => false,
        }
    }

    pub(super) fn text_pointer(
        &mut self,
        value: AstExpr,
        span: Span,
    ) -> Result<AstExpr, SemanticError> {
        match value {
            AstExpr::Var(name) => {
                if self.globals.contains_key(&name) && !self.current_scope_contains(&name) {
                    Ok(AstExpr::GlobalAddr { module: None, name })
                } else {
                    Ok(AstExpr::AddrOfVar(name))
                }
            }
            AstExpr::GlobalAddr { .. } => Ok(value),
            AstExpr::Load(inner) if matches!(inner.as_ref(), AstExpr::GlobalAddr { .. }) => {
                Ok(*inner)
            }
            other if matches!(self.shallow_type_opt(&other), Some(AstType::Ptr(_))) => Ok(other),
            _ => Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                "text operation requires a text variable",
            )),
        }
    }

    pub(super) fn text_parts(
        &mut self,
        value: AstExpr,
        span: Span,
    ) -> Result<(AstExpr, AstExpr), SemanticError> {
        let ty = self.shallow_type_opt(&value);
        let literal_bytes = if let AstExpr::BytesLit(bytes) = &value {
            Some(bytes.clone())
        } else {
            None
        };
        if let Some(bytes) = literal_bytes {
            return Ok((value, AstExpr::BytesLen(bytes)));
        }
        let base = match ty {
            Some(AstType::Named { ref name, .. }) if name.eq_ignore_ascii_case("tcstring") => {
                self.text_pointer(value, span)?
            }
            Some(AstType::Ptr(inner)) => match inner.as_ref() {
                AstType::Named { name, .. } if name.eq_ignore_ascii_case("tcstring") => value,
                AstType::U8 => {
                    let length = if let AstExpr::Var(name) = &value {
                        self.bytes_lengths.get(name).map(|length| {
                            AstExpr::ConstInt(i64::try_from(*length).unwrap_or(i64::MAX), None)
                        })
                    } else {
                        None
                    };
                    let Some(length) = length else {
                        return Err(self.error(
                            span,
                            SemanticErrorKind::Lowering,
                            "text slice needs a known byte length",
                        ));
                    };
                    return Ok((value, length));
                }
                _ => {
                    return Err(self.error(span, SemanticErrorKind::Lowering, "slice needs text"));
                }
            },
            _ => {
                return Err(self.error(span, SemanticErrorKind::Lowering, "slice needs text"));
            }
        };
        let data = AstExpr::Load(Box::new(AstExpr::FieldAddr {
            base_ptr: Box::new(base.clone()),
            field: "data".to_string(),
        }));
        let length = AstExpr::Load(Box::new(AstExpr::FieldAddr {
            base_ptr: Box::new(base),
            field: "len".to_string(),
        }));
        Ok((data, length))
    }

    fn is_intmap_variable(&self, name: &str) -> bool {
        match self.lookup(name) {
            Some(AstType::Named { name, .. }) => name.eq_ignore_ascii_case("intmap"),
            Some(AstType::Ptr(inner)) => matches!(
                inner.as_ref(),
                AstType::Named { name, .. } if name.eq_ignore_ascii_case("intmap")
            ),
            _ => false,
        }
    }

    pub(super) fn resolve_intmap_name(&self, text: &str) -> Option<String> {
        let name = lower_name(text.trim().trim_end_matches('.').trim());
        self.is_intmap_variable(&name).then_some(name)
    }

    pub(super) fn intmap_pointer(
        &mut self,
        value: AstExpr,
        span: Span,
    ) -> Result<AstExpr, SemanticError> {
        self.text_pointer(value, span).map_err(|_| {
            self.error(
                span,
                SemanticErrorKind::Lowering,
                "map operation requires a map variable",
            )
        })
    }

    pub(super) fn is_natural_map_operation(&self, text: &str) -> bool {
        let lower = text
            .trim()
            .trim_end_matches('.')
            .trim()
            .to_ascii_lowercase();
        lower.starts_with("set entry ")
            || lower.starts_with("add ") && lower.contains(" to entry ")
            || lower.starts_with("subtract ") && lower.contains(" from entry ")
    }

    pub(super) fn try_map_operation(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<Vec<AstStmt>>, SemanticError> {
        let trimmed = text.trim().trim_end_matches('.').trim();
        let lower = trimmed.to_ascii_lowercase();

        if lower.starts_with("set entry ") {
            let rest = &trimmed["set entry ".len()..];
            let Some((of_start, of_end)) = find_top_level_word(rest, "of") else {
                return Ok(None);
            };
            let key_text = rest[..of_start].trim();
            let after = rest[of_end..].trim();
            let Some((to_start, to_end)) = find_top_level_word(after, "to") else {
                return Ok(None);
            };
            let map_text = after[..to_start].trim();
            let value_text = after[to_end..].trim();
            let Some(map) = self.resolve_intmap_name(map_text) else {
                return Ok(None);
            };
            let pointer = self.intmap_pointer(AstExpr::Var(map), span)?;
            let key = self.parse_value(key_text, span)?;
            let value = self.parse_value(value_text, span)?;
            return Ok(Some(vec![AstStmt::Expr(
                AstExpr::Call {
                    module: None,
                    name: "intmap_put".to_string(),
                    type_args: Vec::new(),
                    args: vec![pointer, key, value],
                },
                self.span(span),
            )]));
        }

        for (prefix, subtract) in [("add ", false), ("subtract ", true)] {
            if lower.starts_with(prefix) {
                let rest = &trimmed[prefix.len()..];
                let separator = if subtract { "from" } else { "to" };
                let Some((to_start, to_end)) = find_top_level_word(rest, separator) else {
                    continue;
                };
                let amount_text = rest[..to_start].trim();
                let after = rest[to_end..].trim();
                let Some(entry_rest) = after.strip_prefix("entry ") else {
                    continue;
                };
                let Some((of_start, of_end)) = find_top_level_word(entry_rest, "of") else {
                    continue;
                };
                let key_text = entry_rest[..of_start].trim();
                let map_text = entry_rest[of_end..].trim();
                let Some(map) = self.resolve_intmap_name(map_text) else {
                    continue;
                };
                let pointer = self.intmap_pointer(AstExpr::Var(map), span)?;
                let key = self.parse_value(key_text, span)?;
                let mut amount = self.parse_value(amount_text, span)?;
                if subtract {
                    amount = AstExpr::Unary {
                        op: tc_ir::UnOp::Neg,
                        val: Box::new(amount),
                    };
                }
                return Ok(Some(vec![AstStmt::Expr(
                    AstExpr::Call {
                        module: None,
                        name: "intmap_add".to_string(),
                        type_args: Vec::new(),
                        args: vec![amount, key, pointer],
                    },
                    self.span(span),
                )]));
            }
        }
        Ok(None)
    }

    pub(super) fn try_repeated_list(
        &mut self,
        target: &str,
        text: &str,
        span: Span,
    ) -> Result<Option<Vec<AstStmt>>, SemanticError> {
        let trimmed = text.trim();
        let lower = trimmed.to_ascii_lowercase();
        let Some(rest) = trimmed
            .strip_prefix("a list of ")
            .or_else(|| trimmed.strip_prefix("A list of "))
        else {
            return Ok(None);
        };
        let lower_rest = lower
            .strip_prefix("a list of ")
            .or_else(|| lower.strip_prefix("a list of "))
            .unwrap_or(&lower);
        let (count_text, value) = if let Some(count) = lower_rest
            .strip_suffix(" zeros")
            .or_else(|| lower_rest.strip_suffix(" zeroes"))
        {
            (
                rest[..count.len()].trim().to_string(),
                AstExpr::ConstInt(0, None),
            )
        } else if let Some((count, value_text)) = lower_rest.split_once(" copies of ") {
            (
                rest[..count.len()].trim().to_string(),
                self.parse_value(rest[rest.len() - value_text.len()..].trim(), span)?,
            )
        } else {
            return Ok(None);
        };
        if !is_simple_name(target) {
            return Ok(None);
        }
        let name = lower_name(target);
        let element = self.shallow_type(&value);
        let ty = list_type(element.clone());
        let mut statements = self.assign_text(&name, AstExpr::Zero(ty), span, true)?;
        let pointer = if self.globals.contains_key(&name) && !self.current_scope_contains(&name) {
            AstExpr::GlobalAddr {
                module: None,
                name: name.clone(),
            }
        } else {
            AstExpr::AddrOfVar(name.clone())
        };
        let count = self.parse_value(&count_text, span)?;
        let count = AstExpr::Cast {
            val: Box::new(count),
            to: AstType::I64,
        };
        let index = self.next_temp("repeat_index");
        self.declare_temp(&index, AstType::I64);
        statements.push(AstStmt::Let {
            name: index.clone(),
            ty: Some(AstType::I64),
            init: AstExpr::ConstInt(0, None),
            span: self.span(span),
        });
        let push = AstStmt::Expr(
            AstExpr::Call {
                module: None,
                name: "list_push".to_string(),
                type_args: Vec::new(),
                args: vec![value, pointer],
            },
            self.span(span),
        );
        statements.push(AstStmt::Loop {
            body: vec![
                AstStmt::If {
                    cond: AstExpr::Binary {
                        op: tc_ir::BinOp::Ge,
                        lhs: Box::new(AstExpr::Var(index.clone())),
                        rhs: Box::new(count),
                    },
                    then_body: vec![AstStmt::Break(self.span(span))],
                    else_body: Vec::new(),
                    span: self.span(span),
                },
                push,
                AstStmt::Assign {
                    name: index.clone(),
                    val: AstExpr::Binary {
                        op: tc_ir::BinOp::Add,
                        lhs: Box::new(AstExpr::Var(index)),
                        rhs: Box::new(AstExpr::ConstInt(1, None)),
                    },
                    span: self.span(span),
                },
            ],
            span: self.span(span),
        });
        Ok(Some(statements))
    }

    pub(super) fn try_swap_items(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<Vec<AstStmt>>, SemanticError> {
        let trimmed = text.trim().trim_end_matches('.').trim();
        let lower = trimmed.to_ascii_lowercase();
        if !lower.starts_with("swap item ") {
            return Ok(None);
        }
        let rest = &trimmed["swap item ".len()..];
        let Some((of_start, of_end)) = find_top_level_word(rest, "of") else {
            return Ok(None);
        };
        let first_text = rest[..of_start].trim();
        let after = rest[of_end..].trim();
        let Some((and_start, and_end)) = find_top_level_word(after, "and") else {
            return Ok(None);
        };
        let list_text = after[..and_start].trim();
        let Some(second_part) = after[and_end..].trim().strip_prefix("item ") else {
            return Ok(None);
        };
        let second_part = second_part.trim();
        let Some((second_of_start, second_of_end)) = find_top_level_word(second_part, "of") else {
            return Ok(None);
        };
        let second_text = second_part[..second_of_start].trim();
        let second_list = second_part[second_of_end..].trim();
        let Some(list) = self.resolve_list_name(list_text) else {
            return Ok(None);
        };
        if self.resolve_list_name(second_list) != Some(list.clone()) {
            return Ok(None);
        }
        let element = self
            .list_element_type(&list)
            .ok_or_else(|| self.error(span, SemanticErrorKind::Lowering, "swap needs a list"))?;
        let pointer = self.list_pointer(AstExpr::Var(list), span)?;
        let first = self.parse_value(first_text, span)?;
        let second = self.parse_value(second_text, span)?;
        let first = self.natural_list_index(first);
        let second = self.natural_list_index(second);
        let temporary = self.next_temp("swap_item");
        self.declare_temp(&temporary, element);
        Ok(Some(vec![
            AstStmt::Let {
                name: temporary.clone(),
                ty: None,
                init: AstExpr::Call {
                    module: None,
                    name: "list_get".to_string(),
                    type_args: Vec::new(),
                    args: vec![pointer.clone(), first.clone()],
                },
                span: self.span(span),
            },
            AstStmt::Expr(
                AstExpr::Call {
                    module: None,
                    name: "list_set".to_string(),
                    type_args: Vec::new(),
                    args: vec![
                        pointer.clone(),
                        first,
                        AstExpr::Call {
                            module: None,
                            name: "list_get".to_string(),
                            type_args: Vec::new(),
                            args: vec![pointer.clone(), second.clone()],
                        },
                    ],
                },
                self.span(span),
            ),
            AstStmt::Expr(
                AstExpr::Call {
                    module: None,
                    name: "list_set".to_string(),
                    type_args: Vec::new(),
                    args: vec![pointer, second, AstExpr::Var(temporary)],
                },
                self.span(span),
            ),
        ]))
    }

    pub(super) fn try_string_operation(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<Vec<AstStmt>>, SemanticError> {
        let trimmed = text.trim().trim_end_matches('.').trim();
        let lower = trimmed.to_ascii_lowercase();
        if let Some(rest) = lower
            .strip_prefix("remove the last character from ")
            .or_else(|| lower.strip_prefix("remove last character from "))
            .or_else(|| lower.strip_prefix("remove the last byte from "))
            .or_else(|| lower.strip_prefix("remove last byte from "))
        {
            let rest = trimmed[trimmed.len() - rest.len()..].trim();
            if is_simple_name(rest) {
                let target = lower_name(rest);
                if self.is_text_variable(&target) {
                    let pointer = self.text_pointer(AstExpr::Var(target), span)?;
                    return Ok(Some(vec![AstStmt::Expr(
                        AstExpr::Call {
                            module: None,
                            name: "str_pop_byte".to_string(),
                            type_args: Vec::new(),
                            args: vec![pointer],
                        },
                        self.span(span),
                    )]));
                }
            }
        }
        if !lower.starts_with("append ") {
            return Ok(None);
        }
        let rest = &trimmed["append ".len()..];
        let Some((to_start, to_end)) = find_top_level_word(rest, "to") else {
            return Ok(None);
        };
        let value_text = rest[..to_start].trim();
        let target_text = rest[to_end..].trim();
        if !is_simple_name(target_text) {
            return Ok(None);
        }
        let target = lower_name(target_text);
        if !self.is_text_variable(&target) {
            return Ok(None);
        }
        let target_ptr = self.text_pointer(AstExpr::Var(target), span)?;
        let value = self.parse_value(value_text, span)?;
        let ty = self.shallow_type_opt(&value);
        let mut statements = Vec::new();
        if let AstExpr::BytesLit(bytes) = &value {
            statements.push(AstStmt::Expr(
                AstExpr::Call {
                    module: None,
                    name: "str_push_bytes".to_string(),
                    type_args: Vec::new(),
                    args: vec![target_ptr, value.clone(), AstExpr::BytesLen(bytes.clone())],
                },
                self.span(span),
            ));
            return Ok(Some(statements));
        }
        let name = match ty {
            Some(AstType::Named { ref name, .. }) if name.eq_ignore_ascii_case("tcstring") => {
                let source = self.string_pointer(value, span, &mut statements)?;
                statements.push(AstStmt::Expr(
                    AstExpr::Call {
                        module: None,
                        name: "str_append_text".to_string(),
                        type_args: Vec::new(),
                        args: vec![target_ptr, source],
                    },
                    self.span(span),
                ));
                return Ok(Some(statements));
            }
            Some(AstType::Ptr(inner)) if matches!(inner.as_ref(), AstType::U8) => {
                let source = value;
                let source_name = value_text.trim().to_ascii_lowercase();
                let length = self
                    .bytes_lengths
                    .get(&lower_name(&source_name))
                    .map(|len| AstExpr::ConstInt(i64::try_from(*len).unwrap_or(i64::MAX), None));
                if let Some(length) = length {
                    statements.push(AstStmt::Expr(
                        AstExpr::Call {
                            module: None,
                            name: "str_push_bytes".to_string(),
                            type_args: Vec::new(),
                            args: vec![target_ptr, source, length],
                        },
                        self.span(span),
                    ));
                    return Ok(Some(statements));
                }
                return Ok(None);
            }
            _ => "str_append_byte",
        };
        statements.push(AstStmt::Expr(
            AstExpr::Call {
                module: None,
                name: name.to_string(),
                type_args: Vec::new(),
                args: vec![
                    target_ptr,
                    AstExpr::Cast {
                        val: Box::new(value),
                        to: AstType::U8,
                    },
                ],
            },
            self.span(span),
        ));
        Ok(Some(statements))
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
            AstExpr::Var(name) => {
                if self.globals.contains_key(&name) && !self.current_scope_contains(&name) {
                    Ok(AstExpr::GlobalAddr { module: None, name })
                } else {
                    Ok(AstExpr::AddrOfVar(name))
                }
            }
            AstExpr::GlobalAddr { .. } => Ok(value),
            AstExpr::Load(inner) if matches!(inner.as_ref(), AstExpr::GlobalAddr { .. }) => {
                Ok(*inner)
            }
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
