//! Statement, expression, assignment, emit, transfer, and call lowering.

use super::{
    AstExpr, AstStmt, AstType, HashSet, Lowerer, MAX_DEPTH, PhraseMatch, RawItem, RawSentence,
    ScopeKind, SemanticConcept, SemanticError, SemanticErrorKind, Span, Target, Token, TokenKind,
    VerbPredicate, Voice, after_verb_start_text, analyze_clause, canonical_assignment_target,
    classify_verb, clause_graph, contains_nary_operator, find_top_level_phrase,
    find_top_level_symbol, find_top_level_word, first_word, hint_for, is_direct_call_statement,
    is_lvalue_text, is_simple_name, join_tokens, lower_name, match_all, parse_address,
    parse_condition, parse_expression, parse_global_quantifier, parse_place, parse_store_address,
    parse_type_text, prep_role, render_expr, split_header_condition, split_top_level,
    split_type_list, split_where, starts_with_word, strip_leading_word, tokenize,
    transform_operator, zero_like,
};

use crate::value::ValueContext;

impl Lowerer<'_> {
    pub(super) fn rewrite_number_words(&self, text: &str) -> String {
        let tokens = tokenize(text);
        let mut replacements = Vec::new();
        let mut index = 0;
        while index < tokens.len() {
            let token = &tokens[index];
            if !token.is_word() || !crate::number_words::is_word(&token.lower) {
                index += 1;
                continue;
            }
            let start = index;
            let mut end = index;
            while end < tokens.len() {
                let candidate = &tokens[end];
                let numeric = candidate.is_word() && crate::number_words::is_word(&candidate.lower);
                let hyphen = candidate.text == "-" && end + 1 < tokens.len();
                if numeric || hyphen {
                    end += 1;
                } else {
                    break;
                }
            }
            if end <= start {
                index += 1;
                continue;
            }
            let phrase = text[tokens[start].start..tokens[end - 1].end].trim();
            if let Some(value) = crate::number_words::parse(phrase) {
                let canonical = phrase.to_ascii_lowercase().replace([' ', '-'], "_");
                let next_is_call = tokens
                    .get(end)
                    .is_some_and(|token| token.text == "(" || token.text == ".");
                if self.lookup(&canonical).is_none()
                    && !self.globals.contains_key(&canonical)
                    && !self.function_returns.contains_key(&canonical)
                    && !next_is_call
                {
                    replacements.push((
                        tokens[start].start,
                        tokens[end - 1].end,
                        value.to_string(),
                    ));
                }
            }
            index = end;
        }
        if replacements.is_empty() {
            return text.to_string();
        }
        let mut output = String::with_capacity(text.len());
        let mut cursor = 0;
        for (start, end, replacement) in replacements {
            output.push_str(&text[cursor..start]);
            output.push_str(&replacement);
            cursor = end;
        }
        output.push_str(&text[cursor..]);
        output
    }

    pub(super) fn parse_expr(&mut self, text: &str, span: Span) -> Result<AstExpr, SemanticError> {
        let resolved = self.resolve_anaphors(text, span)?;
        let resolved = crate::number_words::normalize_hyphens(&resolved);
        if let Some(value) = crate::number_words::parse(&resolved) {
            let canonical = resolved
                .trim()
                .to_ascii_lowercase()
                .replace([' ', '-'], "_");
            if self.lookup(&canonical).is_none()
                && !self.globals.contains_key(&canonical)
                && !self.function_returns.contains_key(&canonical)
            {
                return Ok(AstExpr::ConstInt(value, None));
            }
        }
        let rewritten = self.rewrite_number_words(&resolved);
        let result = parse_expression(&rewritten, &self.expr_env());
        result.map_err(|message| {
            let hint = if message.starts_with("unexpected trailing token") {
                self.expression_name_hint(text)
            } else {
                String::new()
            };
            self.error(
                span,
                SemanticErrorKind::Lowering,
                format!("{message}{hint}"),
            )
        })
    }

    pub(super) fn expression_name_hint(&self, text: &str) -> String {
        let env = self.expr_env();
        let mut candidates = env.locals.keys().map(String::as_str).collect::<Vec<_>>();
        candidates.extend(env.globals.keys().map(String::as_str));
        candidates.extend(self.function_returns.keys().map(String::as_str));
        let hint = hint_for(text, candidates.iter().copied());
        if !hint.is_empty() {
            return hint;
        }
        let tokens = tokenize(text);
        if let Some(first) = tokens.first() {
            let rest = text[first.end..].trim();
            let rest_hint = hint_for(rest, candidates.iter().copied());
            if !rest_hint.is_empty() {
                return rest_hint;
            }
            let rest_word = first_word(rest);
            if !rest_word.is_empty() {
                return hint_for(&rest_word, candidates.iter().copied());
            }
        }
        String::new()
    }

    pub(super) fn parse_expr_raw(&self, text: &str, span: Span) -> Result<AstExpr, SemanticError> {
        let rewritten = self.rewrite_number_words(&self.rewrite_known_names(text));
        parse_expression(&rewritten, &self.expr_env())
            .map_err(|message| self.error(span, SemanticErrorKind::Lowering, message))
    }

    pub(super) fn phrase_expression(
        &mut self,
        phrase: &PhraseMatch,
        span: Span,
    ) -> Result<AstExpr, SemanticError> {
        let mut args = Vec::with_capacity(phrase.args.len());
        for argument in &phrase.args {
            args.push(self.parse_value(argument, span)?);
        }
        let module = (phrase.module != self.module).then(|| phrase.module.clone());
        Ok(AstExpr::Call {
            module,
            name: phrase.function.clone(),
            type_args: Vec::new(),
            args,
        })
    }

    pub(super) fn as_text_value(
        &mut self,
        value: AstExpr,
        span: Span,
    ) -> Result<AstExpr, SemanticError> {
        match self.shallow_type_opt(&value) {
            Some(AstType::Named { ref name, .. }) if name.eq_ignore_ascii_case("tcstring") => {
                Ok(value)
            }
            Some(AstType::Ptr(inner)) => match inner.as_ref() {
                AstType::U8 => {
                    if let AstExpr::BytesLit(bytes) = &value {
                        let length = AstExpr::BytesLen(bytes.clone());
                        return Ok(AstExpr::Call {
                            module: None,
                            name: "str_from_bytes".to_string(),
                            type_args: Vec::new(),
                            args: vec![value, length],
                        });
                    }
                    if let AstExpr::Var(name) = &value
                        && let Some(length) = self.bytes_lengths.get(name).copied()
                    {
                        return Ok(AstExpr::Call {
                            module: None,
                            name: "str_from_bytes".to_string(),
                            type_args: Vec::new(),
                            args: vec![
                                value,
                                AstExpr::ConstInt(i64::try_from(length).unwrap_or(i64::MAX), None),
                            ],
                        });
                    }
                    Err(self.error(span, SemanticErrorKind::Lowering, "text value required"))
                }
                AstType::Named { name, .. } if name.eq_ignore_ascii_case("tcstring") => {
                    Ok(AstExpr::Load(Box::new(value)))
                }
                _ => Err(self.error(span, SemanticErrorKind::Lowering, "text value required")),
            },
            _ => Err(self.error(span, SemanticErrorKind::Lowering, "text value required")),
        }
    }

    pub(crate) fn parse_value(&mut self, text: &str, span: Span) -> Result<AstExpr, SemanticError> {
        let normalized = crate::number_words::normalize_hyphens(text);
        let trimmed = normalized.trim();
        let lower = trimmed.to_ascii_lowercase();
        if let Some(rest) = lower
            .strip_prefix("the slice of ")
            .or_else(|| lower.strip_prefix("slice of "))
        {
            let rest = trimmed[trimmed.len() - rest.len()..].trim();
            if let Some((from_start, from_end)) = find_top_level_word(rest, "from") {
                let text_part = rest[..from_start].trim();
                let after = rest[from_end..].trim();
                if let Some((to_start, to_end)) = find_top_level_word(after, "to") {
                    let start_text = after[..to_start].trim();
                    let finish_text = after[to_end..].trim();
                    let text_value = self.parse_value(text_part, span)?;
                    let (pointer, length) = self.text_parts(text_value, span)?;
                    let start = self.parse_value(start_text, span)?;
                    let finish = self.parse_value(finish_text, span)?;
                    return Ok(AstExpr::Call {
                        module: None,
                        name: "str_slice".to_string(),
                        type_args: Vec::new(),
                        args: vec![pointer, length, start, finish],
                    });
                }
            }
        }
        if let Some(rest) = lower.strip_prefix("split ") {
            let rest = trimmed[trimmed.len() - rest.len()..].trim();
            if let Some((by_start, by_end)) = find_top_level_word(rest, "by") {
                let text_expr = self.parse_value(rest[..by_start].trim(), span)?;
                let delimiter_expr = self.parse_value(rest[by_end..].trim(), span)?;
                let text_expr = self.as_text_value(text_expr, span)?;
                let delimiter_expr = self.as_text_value(delimiter_expr, span)?;
                return Ok(AstExpr::Call {
                    module: None,
                    name: "str_split".to_string(),
                    type_args: Vec::new(),
                    args: vec![text_expr, delimiter_expr],
                });
            }
        }
        if let Some(rest) = lower.strip_prefix("join ") {
            let rest = trimmed[trimmed.len() - rest.len()..].trim();
            if let Some((with_start, with_end)) = find_top_level_word(rest, "with") {
                let pieces = self.parse_value(rest[..with_start].trim(), span)?;
                let delimiter_expr = self.parse_value(rest[with_end..].trim(), span)?;
                let delimiter_expr = self.as_text_value(delimiter_expr, span)?;
                return Ok(AstExpr::Call {
                    module: None,
                    name: "str_join".to_string(),
                    type_args: Vec::new(),
                    args: vec![pieces, delimiter_expr],
                });
            }
        }
        if matches!(
            lower.as_str(),
            "an empty map" | "empty map" | "an empty number map" | "empty number map"
        ) {
            let ty = parse_type_text("IntMap of whole number")
                .map_err(|message| self.error(span, SemanticErrorKind::Lowering, message))?;
            return Ok(AstExpr::Zero(ty));
        }
        if let Some(rest) = lower
            .strip_prefix("the number of entries in ")
            .or_else(|| lower.strip_prefix("number of entries in "))
        {
            let rest = trimmed[trimmed.len() - rest.len()..].trim();
            if let Some(map) = self.resolve_intmap_name(rest) {
                let pointer = self.intmap_pointer(AstExpr::Var(map), span)?;
                return Ok(AstExpr::Cast {
                    val: Box::new(AstExpr::Call {
                        module: None,
                        name: "intmap_len".to_string(),
                        type_args: Vec::new(),
                        args: vec![pointer],
                    }),
                    to: AstType::I64,
                });
            }
        }
        if let Some(rest) = lower
            .strip_prefix("entry ")
            .or_else(|| lower.strip_prefix("the entry "))
        {
            let rest = trimmed[trimmed.len() - rest.len()..].trim();
            if let Some((of_start, of_end)) = find_top_level_word(rest, "of") {
                let key_text = rest[..of_start].trim();
                let map_text = rest[of_end..].trim();
                if let Some(map) = self.resolve_intmap_name(map_text) {
                    let pointer = self.intmap_pointer(AstExpr::Var(map), span)?;
                    let key = self.parse_value(key_text, span)?;
                    return Ok(AstExpr::Call {
                        module: None,
                        name: "intmap_value".to_string(),
                        type_args: Vec::new(),
                        args: vec![pointer, key],
                    });
                }
            }
        }
        if matches!(
            lower.as_str(),
            "empty text" | "an empty text" | "empty string"
        ) {
            return Ok(AstExpr::Call {
                module: None,
                name: "str_new".to_string(),
                type_args: Vec::new(),
                args: Vec::new(),
            });
        }
        if let Some(value) = self.count_query(trimmed, span)? {
            return Ok(value);
        }
        for prefix in ["that ", "this "] {
            if let Some(rest) = lower.strip_prefix(prefix) {
                let noun = lower_name(rest.trim());
                if self.lookup(&noun).is_some() {
                    return Ok(AstExpr::Var(noun));
                }
                if self.active_loop_var.as_deref() == Some(noun.as_str()) {
                    return Ok(AstExpr::Var(noun));
                }
            }
        }
        for prefix in ["the square root of ", "square root of "] {
            if let Some(rest) = lower.strip_prefix(prefix) {
                let rest = trimmed[trimmed.len() - rest.len()..].trim();
                return self.lower_square_root(rest, span);
            }
        }
        for prefix in ["trim text ", "trimmed text ", "trim ", "trimmed "] {
            if lower.starts_with(prefix) {
                let rest = trimmed[prefix.len()..].trim();
                return self.lower_trim_value(rest, span);
            }
        }
        for (prefix, operator) in [
            ("one less than ", tc_ir::BinOp::Sub),
            ("one more than ", tc_ir::BinOp::Add),
        ] {
            if lower.starts_with(prefix) {
                let value = self.parse_value(trimmed[prefix.len()..].trim(), span)?;
                return Ok(AstExpr::Binary {
                    op: operator,
                    lhs: Box::new(value),
                    rhs: Box::new(AstExpr::ConstInt(1, None)),
                });
            }
        }
        if let Some(rest) = lower
            .strip_prefix("the midpoint of ")
            .or_else(|| lower.strip_prefix("midpoint of "))
        {
            let rest = trimmed[trimmed.len() - rest.len()..].trim();
            if let Some((and_start, and_end)) = find_top_level_word(rest, "and") {
                let first = self.parse_value(rest[..and_start].trim(), span)?;
                let second = self.parse_value(rest[and_end..].trim(), span)?;
                let difference = AstExpr::Binary {
                    op: tc_ir::BinOp::Sub,
                    lhs: Box::new(second.clone()),
                    rhs: Box::new(first.clone()),
                };
                let half = AstExpr::Binary {
                    op: tc_ir::BinOp::Div,
                    lhs: Box::new(difference),
                    rhs: Box::new(AstExpr::ConstInt(2, None)),
                };
                return Ok(AstExpr::Binary {
                    op: tc_ir::BinOp::Add,
                    lhs: Box::new(first),
                    rhs: Box::new(half),
                });
            }
        }
        for (prefix, maximum) in [
            ("the larger of ", true),
            ("larger of ", true),
            ("the greater of ", true),
            ("greater of ", true),
            ("the smaller of ", false),
            ("smaller of ", false),
            ("the lesser of ", false),
            ("lesser of ", false),
        ] {
            if let Some(rest) = lower.strip_prefix(prefix) {
                let rest = trimmed[trimmed.len() - rest.len()..].trim();
                if let Some((and_start, and_end)) = find_top_level_word(rest, "and") {
                    let first = self.parse_value(rest[..and_start].trim(), span)?;
                    let second = self.parse_value(rest[and_end..].trim(), span)?;
                    return Ok(if maximum {
                        crate::value::numeric_max(first, second)
                    } else {
                        crate::value::numeric_min(first, second)
                    });
                }
            }
        }
        if let Some(rest) = lower
            .strip_prefix("the last position of ")
            .or_else(|| lower.strip_prefix("last position of "))
        {
            let rest = trimmed[trimmed.len() - rest.len()..].trim();
            let list = self.parse_value(rest, span)?;
            if let Some(length) = self.length_expression(list, span)? {
                return Ok(AstExpr::Binary {
                    op: tc_ir::BinOp::Sub,
                    lhs: Box::new(AstExpr::Cast {
                        val: Box::new(length),
                        to: AstType::I64,
                    }),
                    rhs: Box::new(AstExpr::ConstInt(1, None)),
                });
            }
        }
        if let Some(rest) = lower
            .strip_prefix("the last item of ")
            .or_else(|| lower.strip_prefix("last item of "))
        {
            let rest = trimmed[trimmed.len() - rest.len()..].trim();
            let list = self.parse_value(rest, span)?;
            let pointer = self.list_pointer(list.clone(), span)?;
            if let Some(length) = self.length_expression(list, span)? {
                return Ok(AstExpr::Call {
                    module: None,
                    name: "list_get".to_string(),
                    type_args: Vec::new(),
                    args: vec![
                        pointer,
                        AstExpr::Cast {
                            val: Box::new(AstExpr::Binary {
                                op: tc_ir::BinOp::Sub,
                                lhs: Box::new(length),
                                rhs: Box::new(AstExpr::ConstInt(1, None)),
                            }),
                            to: AstType::U64,
                        },
                    ],
                });
            }
        }
        if let Some(rest) = lower
            .strip_prefix("the first item of ")
            .or_else(|| lower.strip_prefix("first item of "))
        {
            let rest = trimmed[trimmed.len() - rest.len()..].trim();
            let list = self.parse_value(rest, span)?;
            let pointer = self.list_pointer(list, span)?;
            return Ok(AstExpr::Call {
                module: None,
                name: "list_get".to_string(),
                type_args: Vec::new(),
                args: vec![pointer, AstExpr::ConstInt(0, None)],
            });
        }
        crate::value::parse_value(self, text, span)
    }

    pub(super) fn lower_square_root(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<AstExpr, SemanticError> {
        let value = self.parse_value(text, span)?;
        let value = if matches!(self.shallow_type_opt(&value), Some(AstType::F64)) {
            value
        } else {
            AstExpr::Cast {
                val: Box::new(value),
                to: AstType::F64,
            }
        };
        Ok(AstExpr::Call {
            module: None,
            name: "sqrt".to_string(),
            type_args: Vec::new(),
            args: vec![value],
        })
    }

    pub(super) fn lower_trim_value(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<AstExpr, SemanticError> {
        let value = self.parse_value(text, span)?;
        let ty = self.shallow_type_opt(&value);
        if matches!(ty, Some(AstType::Named { ref name, .. }) if name.eq_ignore_ascii_case("tcstring"))
        {
            return match value {
                AstExpr::Var(name) => Ok(AstExpr::Call {
                    module: None,
                    name: "str_trim".to_string(),
                    type_args: Vec::new(),
                    args: vec![AstExpr::AddrOfVar(name)],
                }),
                _ => Err(self.error(
                    span,
                    SemanticErrorKind::Lowering,
                    "trim currently requires a named text value or byte text",
                )),
            };
        }
        if matches!(ty, Some(AstType::Ptr(ref inner)) if matches!(inner.as_ref(), AstType::Named { name, .. } if name.eq_ignore_ascii_case("tcstring")))
        {
            return Ok(AstExpr::Call {
                module: None,
                name: "str_trim".to_string(),
                type_args: Vec::new(),
                args: vec![value],
            });
        }
        if let Some((pointer, length)) = self.text_bytes(&value) {
            return Ok(AstExpr::Call {
                module: None,
                name: "str_trim_bytes".to_string(),
                type_args: Vec::new(),
                args: vec![pointer, length],
            });
        }
        Err(self.error(
            span,
            SemanticErrorKind::Lowering,
            "trim requires a text value",
        ))
    }

    pub(crate) fn text_bytes(&self, value: &AstExpr) -> Option<(AstExpr, AstExpr)> {
        if let AstExpr::BytesLit(bytes) = value {
            return Some((value.clone(), AstExpr::BytesLen(bytes.clone())));
        }
        if let AstExpr::Var(name) = value
            && let Some(length) = self.bytes_lengths.get(name)
        {
            return Some((
                value.clone(),
                AstExpr::ConstUint(u64::try_from(*length).unwrap_or(u64::MAX), None),
            ));
        }
        None
    }

    pub(super) fn structural_compatibility(
        param: &AstType,
        actual: &AstType,
        type_params: &[String],
    ) -> i8 {
        if let AstType::Named { name, .. } = param {
            if type_params.iter().any(|generic| generic == name) {
                return 2;
            }
        }
        if param == actual {
            return 2;
        }
        match (param, actual) {
            (AstType::Ptr(left), AstType::Ptr(right))
            | (AstType::Array(left, _), AstType::Array(right, _)) => {
                if Self::structural_compatibility(left, right, type_params) > 0 {
                    2
                } else {
                    0
                }
            }
            (
                AstType::Named {
                    module: left_module,
                    name: left_name,
                    type_args: left_args,
                },
                AstType::Named {
                    module: right_module,
                    name: right_name,
                    type_args: right_args,
                },
            ) => {
                if left_module != right_module
                    || left_name != right_name
                    || left_args.len() != right_args.len()
                {
                    return 0;
                }
                if left_args.iter().zip(right_args).all(|(left, right)| {
                    Self::structural_compatibility(left, right, type_params) > 0
                }) {
                    2
                } else {
                    0
                }
            }
            (
                AstType::FnPtr {
                    params: left_params,
                    ret: left_ret,
                },
                AstType::FnPtr {
                    params: right_params,
                    ret: right_ret,
                },
            ) => {
                if left_params.len() != right_params.len() {
                    return 0;
                }
                let params_ok = left_params.iter().zip(right_params).all(|(left, right)| {
                    Self::structural_compatibility(left, right, type_params) > 0
                });
                if params_ok && Self::structural_compatibility(left_ret, right_ret, type_params) > 0
                {
                    2
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    pub(super) fn type_compatibility(
        &self,
        param: &AstType,
        actual: &AstType,
        type_params: &[String],
        expression: &AstExpr,
    ) -> i8 {
        let structural = Self::structural_compatibility(param, actual, type_params);
        if structural > 0 {
            return structural;
        }
        match (expression, param) {
            (
                AstExpr::ConstInt(_, None),
                AstType::I8 | AstType::I16 | AstType::I32 | AstType::I64,
            )
            | (
                AstExpr::ConstInt(_, None),
                AstType::U8 | AstType::U16 | AstType::U32 | AstType::U64,
            )
            | (
                AstExpr::ConstUint(_, None),
                AstType::I8 | AstType::I16 | AstType::I32 | AstType::I64,
            )
            | (
                AstExpr::ConstUint(_, None),
                AstType::U8 | AstType::U16 | AstType::U32 | AstType::U64,
            ) => 1,
            (AstExpr::ConstFloat(_, None), AstType::F32 | AstType::F64) => 1,
            (AstExpr::BytesLit(_), AstType::Ptr(inner)) if **inner == AstType::U8 => 1,
            _ => 0,
        }
    }

    pub(super) fn match_phrase_typed(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<PhraseMatch>, SemanticError> {
        let candidates = match_all(text, &self.phrases);
        let Some(max_score) = candidates.iter().map(|candidate| candidate.score).max() else {
            return Ok(None);
        };
        let mut candidates = candidates
            .into_iter()
            .filter(|candidate| candidate.score == max_score)
            .collect::<Vec<_>>();
        let max_params = candidates
            .iter()
            .map(|candidate| candidate.params.len())
            .max()
            .unwrap_or(0);
        candidates.retain(|candidate| candidate.params.len() == max_params);
        if candidates.len() == 1 {
            return Ok(candidates.into_iter().next());
        }
        let mut scored = candidates
            .into_iter()
            .map(|candidate| {
                let mut compatibility = 0_i8;
                let mut usable = true;
                for (argument, param) in candidate.args.iter().zip(candidate.params.iter()) {
                    let Ok(expression) = self.parse_expr(argument, span) else {
                        usable = false;
                        break;
                    };
                    let actual = self.shallow_type(&expression);
                    let score = self.type_compatibility(
                        param,
                        &actual,
                        &candidate.type_params,
                        &expression,
                    );
                    if score == 0 {
                        usable = false;
                        break;
                    }
                    compatibility += score;
                }
                (usable, compatibility, candidate)
            })
            .collect::<Vec<_>>();
        scored.retain(|(usable, _, _)| *usable);
        let Some(best_score) = scored.iter().map(|(_, score, _)| *score).max() else {
            return Ok(None);
        };
        let mut best = scored
            .into_iter()
            .filter(|(_, score, _)| *score == best_score)
            .collect::<Vec<_>>();
        best.sort_by(|left, right| {
            left.2
                .module
                .cmp(&right.2.module)
                .then_with(|| left.2.function.cmp(&right.2.function))
        });
        if best.len() > 1 {
            let names = best
                .iter()
                .map(|(_, _, candidate)| format!("{}::{}", candidate.module, candidate.function))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                format!("ambiguous Phrase dispatch (candidates: {names})"),
            ));
        }
        Ok(best.into_iter().next().map(|(_, _, candidate)| candidate))
    }

    pub(super) fn emit_phrase_defers(&self, phrase: &PhraseMatch, source: &str) -> bool {
        let first = phrase.pattern.split_whitespace().next().unwrap_or_default();
        let is_emit = classify_verb(first, None).is_some_and(|resolution| {
            resolution.predicate == Some(VerbPredicate::Emit)
                || resolution.concept == Some(SemanticConcept::Output)
        });
        let lower = source.to_ascii_lowercase();
        let wrapped_value = [
            "the length of ",
            "length of ",
            "the value of ",
            "value of ",
            "number of items in ",
            "number of characters in ",
            "the size of ",
            "the count of ",
        ]
        .iter()
        .any(|needle| lower.contains(needle));
        is_emit
            && (wrapped_value
                || crate::value::is_np_expression(source)
                || phrase
                    .args
                    .iter()
                    .any(|argument| crate::value::is_np_expression(argument)))
    }

    pub(crate) fn shallow_type_opt(&self, expr: &AstExpr) -> Option<AstType> {
        if let AstExpr::Call { name, args, .. } = expr {
            if matches!(name.as_str(), "list_get" | "list_first" | "list_last")
                && let Some(AstType::Ptr(inner)) =
                    args.first().and_then(|arg| self.shallow_type_opt(arg))
                && let AstType::Named {
                    name: type_name,
                    type_args,
                    ..
                } = inner.as_ref()
                && type_name.eq_ignore_ascii_case("list")
            {
                return type_args.first().cloned();
            }
            if let Some(return_type) = self.function_returns.get(name) {
                return return_type.clone();
            }
        }
        let env = self.expr_env();
        env.expr_type(expr)
    }

    pub(crate) fn shallow_type(&self, expr: &AstExpr) -> AstType {
        self.shallow_type_opt(expr).unwrap_or(AstType::I64)
    }

    pub(super) fn lower_phrase_call(
        &mut self,
        phrase: &PhraseMatch,
        span: Span,
    ) -> Result<AstStmt, SemanticError> {
        Ok(AstStmt::Expr(
            self.phrase_expression(phrase, span)?,
            self.span(span),
        ))
    }

    pub(super) fn split_multi_emit(&mut self, text: &str, span: Span) -> Option<Vec<String>> {
        let tokens = tokenize(text);
        let first = tokens.first()?;
        if !first.is_word()
            || !matches!(
                first.lower.as_str(),
                "print" | "say" | "show" | "display" | "emit" | "output"
            )
        {
            return None;
        }
        if crate::textutil::find_top_level_phrase(text, "with").is_some() {
            return None;
        }
        let mut rest = text[first.end..].trim().trim_end_matches('.').to_string();
        while let Some((start, end)) = crate::textutil::find_top_level_phrase(&rest, "followed by")
        {
            rest = format!("{}{}", &rest[..start], &rest[end..]);
        }
        let rest_tokens = tokenize(&rest);
        let mut segments = Vec::new();
        let mut depth = 0_i32;
        let mut start = 0;
        for token in &rest_tokens {
            if token.kind == TokenKind::Symbol {
                if depth == 0 && token.text == "," {
                    let segment = rest[start..token.start].trim();
                    if !segment.is_empty() {
                        segments.push(segment.to_string());
                    }
                    start = token.end;
                }
                match token.text.as_str() {
                    "(" | "[" | "{" => depth += 1,
                    ")" | "]" | "}" => depth -= 1,
                    _ => {}
                }
                continue;
            }
            if depth != 0 || !token.is("and") {
                continue;
            }
            let segment = rest[start..token.start].trim();
            if !segment.is_empty() {
                segments.push(segment.to_string());
            }
            start = token.end;
        }
        let tail = rest[start..].trim().trim_end_matches('.');
        if !tail.is_empty() {
            segments.push(tail.to_string());
        }
        segments.retain(|segment| !segment.is_empty() && !matches!(segment.as_str(), "and" | ","));
        if segments.len() < 2 {
            return None;
        }
        for segment in &segments {
            if contains_nary_operator(segment) {
                return None;
            }
            if !self.is_list_variable(segment) && self.parse_value(segment, span).is_err() {
                return None;
            }
        }
        Some(segments)
    }

    pub(super) fn track_phrase_emit(
        &mut self,
        phrase: &PhraseMatch,
        span: Span,
    ) -> Option<AstStmt> {
        let (tracker, maximum) = self.active_print_tracker()?;
        let emitter = phrase
            .pattern
            .split_whitespace()
            .next()
            .map(str::to_ascii_lowercase)?;
        if !matches!(
            emitter.as_str(),
            "print" | "say" | "show" | "display" | "emit" | "output"
        ) {
            return None;
        }
        let argument = phrase.args.first()?;
        let value = self.parse_value(argument, span).ok()?;
        let ty = self.shallow_type_opt(&value);
        if !matches!(
            ty,
            None | Some(AstType::I8 | AstType::I16 | AstType::I32 | AstType::I64)
        ) {
            return None;
        }
        Some(AstStmt::If {
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
        })
    }

    pub(super) fn lower_stmt(
        &mut self,
        sentence: &RawSentence,
        then_items: Option<&[RawItem]>,
        else_items: Option<&[RawItem]>,
        depth: usize,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        if depth > MAX_DEPTH {
            return Err(self.error(
                sentence.span,
                SemanticErrorKind::Lowering,
                "statement nesting exceeded the 32-level guard",
            ));
        }
        if let Some(statement) = self.try_list_operation(&sentence.text, sentence.span)? {
            return Ok(vec![statement]);
        }
        if let Some(graph) = clause_graph::decompose(&sentence.text) {
            return self.lower_clause_graph(graph, sentence.span, depth, then_items, else_items);
        }
        let (core, where_clause) = split_where(&sentence.text);
        if let Some(bindings) = where_clause {
            self.push_scope(ScopeKind::WhereClause);
            let mut block = self.lower_where_bindings(&bindings, sentence.span)?;
            let existing_vars = self
                .scopes
                .last()
                .map(|scope| scope.vars.keys().cloned().collect::<HashSet<_>>())
                .unwrap_or_default();
            let referent_checkpoint = self.discourse.current_referent_count();
            let inner = self.lower_core(&core, sentence.span, then_items, else_items, depth + 1);
            let mut moved_vars = Vec::new();
            if let Some(scope) = self.scopes.last_mut() {
                let declared = scope
                    .vars
                    .keys()
                    .filter(|name| !existing_vars.contains(*name))
                    .cloned()
                    .collect::<Vec<_>>();
                for name in declared {
                    if let Some(ty) = scope.vars.remove(&name) {
                        moved_vars.push((name, ty));
                    }
                }
            }
            let moved_referents = self.discourse.take_referents_since(referent_checkpoint);
            let transformed = inner.map(|mut statements| {
                let mut outer = Vec::new();
                for statement in &mut statements {
                    if let AstStmt::Let {
                        name,
                        ty,
                        init,
                        span,
                    } = statement.clone()
                    {
                        if existing_vars.contains(&name) {
                            continue;
                        }
                        let declared = ty.unwrap_or_else(|| self.shallow_type(&init));
                        *statement = AstStmt::Assign {
                            name: name.clone(),
                            val: init,
                            span: span.clone(),
                        };
                        outer.push(AstStmt::Let {
                            name,
                            ty: Some(declared.clone()),
                            init: AstExpr::Zero(declared),
                            span,
                        });
                    }
                }
                (outer, statements)
            });
            self.pop_scope();
            if let Some(parent) = self.scopes.last_mut() {
                for (name, ty) in moved_vars {
                    parent.vars.insert(name, ty);
                }
            }
            self.discourse.append_referents(moved_referents);
            let (outer, mut inner) = transformed?;
            block.append(&mut inner);
            let mut all = outer;
            all.push(AstStmt::Block(block, self.span(sentence.span)));
            return Ok(all);
        }
        self.lower_core(&core, sentence.span, then_items, else_items, depth + 1)
    }

    pub(super) fn lower_core(
        &mut self,
        text: &str,
        span: Span,
        then_items: Option<&[RawItem]>,
        else_items: Option<&[RawItem]>,
        depth: usize,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        let normalized = crate::normalize::sentence(text);
        let original = normalized.trim().trim_end_matches(['.', ',']).trim();
        if original.is_empty() {
            return Ok(Vec::new());
        }
        let original_lower = original.to_ascii_lowercase();
        if original_lower == "end" {
            return Ok(Vec::new());
        }
        if starts_with_word(&original_lower, "for") {
            let global_text = if let Some(list) = self.last_list.clone() {
                original
                    .replace("the list", &list)
                    .replace("The list", &list)
            } else {
                original.to_string()
            };
            let global = parse_global_quantifier(&global_text)
                .map_err(|message| self.error(span, SemanticErrorKind::Lowering, message))?;
            if let Some(global) = global {
                return self.lower_global_quantifier(global, then_items, span, depth);
            }
        }
        if starts_with_word(&original_lower, "update") {
            if let Some(statements) = self.try_update(original, span)? {
                return Ok(statements);
            }
        }
        let control_scope = starts_with_word(&original_lower, "if")
            || starts_with_word(&original_lower, "unless")
            || starts_with_word(&original_lower, "while")
            || starts_with_word(&original_lower, "repeat")
            || starts_with_word(&original_lower, "loop");
        let resolved_text = if control_scope {
            original.to_string()
        } else {
            self.resolve_anaphors(original, span)?
        };
        // A colon-headed `Repeat while`/`Repeat until` block reaches this
        // point as a structural header, bypassing the flat-prose normalizer.
        // Canonicalize it to the equivalent `While` form so indented bodies
        // lower through the same condition/body path as `While`.
        let repeat_condition = if original_lower.starts_with("repeat while ") {
            Some(format!(
                "While {}",
                original["repeat while ".len()..].trim()
            ))
        } else if original_lower.starts_with("repeat until ") {
            Some(format!(
                "While not {}",
                original["repeat until ".len()..].trim()
            ))
        } else {
            None
        };
        let trimmed = repeat_condition
            .as_deref()
            .unwrap_or(resolved_text.as_str());
        let lower = trimmed.to_ascii_lowercase();
        for prefix in [
            "check whether ",
            "see whether ",
            "test whether ",
            "find out whether ",
        ] {
            if lower.starts_with(prefix) {
                let rest = trimmed[prefix.len()..].trim();
                if let Some(statements) = self.lower_check_whether(rest, span)? {
                    return Ok(statements);
                }
            }
        }
        if starts_with_word(&lower, "count")
            && let Some(statements) = self.try_implicit_count(trimmed, span)?
        {
            return Ok(statements);
        }
        self.set_canonical(crate::paraphrase::canonicalize_statement(trimmed));
        if lower == "end" {
            return Ok(Vec::new());
        }
        for marker in [", then stop", ", and stop", " then stop"] {
            if let Some(index) = lower.rfind(marker) {
                let before = trimmed[..index].trim().trim_end_matches(',').trim();
                let mut statements = if before.is_empty() {
                    Vec::new()
                } else {
                    self.lower_core(before, span, None, None, depth + 1)?
                };
                statements.push(AstStmt::Break(self.span(span)));
                return Ok(statements);
            }
        }
        if let Some(statements) = self.try_swap_items(trimmed, span)? {
            return Ok(statements);
        }
        if let Some(statements) = self.try_swap_adjacent(trimmed, span)? {
            return Ok(statements);
        }
        if let Some(parts) = self.split_multi_emit(trimmed, span) {
            let mut statements = Vec::new();
            for part in parts {
                if self.is_list_variable(&part) {
                    statements.extend(self.emit_list_elements(&part, span)?);
                } else {
                    statements.extend(self.lower_emit_value(&part, span)?);
                }
            }
            return Ok(statements);
        }
        if let Some(statements) = self.try_map_operation(trimmed, span)? {
            return Ok(statements);
        }
        if let Some(statements) = self.try_string_operation(trimmed, span)? {
            return Ok(statements);
        }
        if let Some(statement) = self.try_list_operation(trimmed, span)? {
            return Ok(vec![statement]);
        }
        if let Some(phrase) = self
            .match_phrase_typed(trimmed, span)?
            .filter(|phrase| !self.emit_phrase_defers(phrase, trimmed))
        {
            self.add_annotation(format!("[phrase: \"{}\"]", phrase.pattern));
            let statement = self.lower_phrase_call(&phrase, span)?;
            let mut statements = vec![statement];
            if let Some(update) = self.track_phrase_emit(&phrase, span) {
                statements.push(update);
            }
            return Ok(statements);
        }
        if starts_with_word(&lower, "return") {
            let rest = strip_leading_word(trimmed, "return").trim();
            let value = if rest.is_empty() {
                None
            } else {
                Some(self.parse_value(rest, span)?)
            };
            return Ok(vec![AstStmt::Return(value, self.span(span))]);
        }
        if starts_with_word(&lower, "break") {
            return Ok(vec![AstStmt::Break(self.span(span))]);
        }
        if starts_with_word(&lower, "continue") {
            return Ok(vec![AstStmt::Continue(self.span(span))]);
        }
        if matches!(
            lower.trim_end_matches('.').trim(),
            "stop immediately" | "stop the loop" | "stop now"
        ) {
            return Ok(vec![AstStmt::Break(self.span(span))]);
        }
        if matches!(
            lower.trim_end_matches('.').trim(),
            "skip to the next one" | "skip this one" | "skip to the next item"
        ) {
            return Ok(vec![AstStmt::Continue(self.span(span))]);
        }
        if starts_with_word(&lower, "trap") {
            return Ok(vec![AstStmt::Trap(self.span(span))]);
        }
        if starts_with_word(&lower, "define") {
            if let Some(statements) = self.try_local_declaration(trimmed, span) {
                return Ok(statements);
            }
        }
        let first_word_lower = first_word(&lower);
        if classify_verb(&first_word_lower, None)
            .is_some_and(|resolution| resolution.predicate == Some(VerbPredicate::Create))
            && self.is_create_declaration(&tokenize(trimmed))
        {
            if let Some(statements) = self.try_create_declaration(trimmed, span)? {
                return Ok(statements);
            }
        }
        if starts_with_word(&lower, "syscall") {
            if let Some(statement) = self.try_syscall_with(trimmed, span)? {
                return Ok(vec![statement]);
            }
        }
        if starts_with_word(&lower, "copy") || starts_with_word(&lower, "memcopy") {
            if let Some(statement) = self.try_memcopy(trimmed, span)? {
                return Ok(vec![statement]);
            }
        }
        if starts_with_word(&lower, "if") {
            let rest = strip_leading_word(trimmed, "if");
            let (condition_text, body_text) = split_header_condition(rest);
            if let Some(items) = then_items {
                self.push_scope(ScopeKind::IfCondition);
                let condition = match self.lower_condition(&condition_text, span) {
                    Ok(value) => value,
                    Err(error) => {
                        self.pop_scope();
                        return Err(error);
                    }
                };
                self.set_canonical(format!("If {}, then:", condition.paraphrase));
                self.push_scope(ScopeKind::IfThen);
                let then_body = self.lower_items(items, depth + 1);
                self.pop_scope();
                self.pop_scope();
                let then_body = then_body?;
                let else_body = if let Some(items) = else_items {
                    self.push_scope(ScopeKind::IfElse);
                    let body = self.lower_items(items, depth + 1);
                    self.pop_scope();
                    body?
                } else {
                    Vec::new()
                };
                let mut statements = condition.prelude;
                statements.push(AstStmt::If {
                    cond: condition.expr,
                    then_body,
                    else_body,
                    span: self.span(span),
                });
                return Ok(statements);
            }
            let mut then_text = body_text;
            let mut else_text = None;
            for marker in ["otherwise", "else"] {
                if let Some((start, end)) = find_top_level_word(&then_text, marker) {
                    let before = then_text[..start].trim().to_string();
                    let after = then_text[end..].trim().to_string();
                    if !after.is_empty() && !before.is_empty() {
                        then_text = before;
                        else_text = Some(after);
                        break;
                    }
                }
            }
            if then_text.is_empty() {
                return Err(self.error(
                    span,
                    SemanticErrorKind::Lowering,
                    "`If` requires a then branch after the condition",
                ));
            }
            self.push_scope(ScopeKind::IfCondition);
            let condition = match self.lower_condition(&condition_text, span) {
                Ok(value) => value,
                Err(error) => {
                    self.pop_scope();
                    return Err(error);
                }
            };
            self.push_scope(ScopeKind::IfThen);
            let then_result = self.lower_actions_capturing(&then_text, span, depth);
            self.pop_scope();
            self.pop_scope();
            let (then_body, then_lines) = then_result?;
            let (else_body, else_lines) = if let Some(actions) = else_text {
                self.push_scope(ScopeKind::IfElse);
                let result = self.lower_actions_capturing(&actions, span, depth);
                self.pop_scope();
                result?
            } else {
                (Vec::new(), Vec::new())
            };
            let mut canonical = format!("If {}, then:", condition.paraphrase);
            for line in then_lines {
                canonical.push_str("\n       ");
                canonical.push_str(&line);
            }
            if !else_lines.is_empty() {
                canonical.push_str("\n   otherwise:");
                for line in else_lines {
                    canonical.push_str("\n       ");
                    canonical.push_str(&line);
                }
            }
            self.set_canonical(canonical);
            let mut statements = condition.prelude;
            statements.push(AstStmt::If {
                cond: condition.expr,
                then_body,
                else_body,
                span: self.span(span),
            });
            return Ok(statements);
        }
        if starts_with_word(&lower, "unless") {
            let rest = strip_leading_word(trimmed, "unless");
            let (condition_text, body_text) = split_header_condition(rest);
            if let Some(items) = then_items {
                self.push_scope(ScopeKind::IfCondition);
                let condition = match self.lower_condition(&condition_text, span) {
                    Ok(value) => value,
                    Err(error) => {
                        self.pop_scope();
                        return Err(error);
                    }
                };
                self.set_canonical(format!("Unless {}:", condition.paraphrase));
                self.push_scope(ScopeKind::IfThen);
                let body = self.lower_items(items, depth + 1);
                self.pop_scope();
                self.pop_scope();
                let mut statements = condition.prelude;
                statements.push(AstStmt::If {
                    cond: AstExpr::Unary {
                        op: tc_ir::UnOp::LogNot,
                        val: Box::new(condition.expr),
                    },
                    then_body: body?,
                    else_body: Vec::new(),
                    span: self.span(span),
                });
                return Ok(statements);
            }
            if body_text.is_empty() {
                return Err(self.error(
                    span,
                    SemanticErrorKind::Lowering,
                    "`Unless` requires a body after the condition",
                ));
            }
            self.push_scope(ScopeKind::IfCondition);
            let condition = match self.lower_condition(&condition_text, span) {
                Ok(value) => value,
                Err(error) => {
                    self.pop_scope();
                    return Err(error);
                }
            };
            self.push_scope(ScopeKind::IfThen);
            let body_result = self.lower_actions_capturing(&body_text, span, depth);
            self.pop_scope();
            self.pop_scope();
            let (body, body_lines) = body_result?;
            let mut canonical = format!("Unless {}:", condition.paraphrase);
            for line in body_lines {
                canonical.push_str("\n       ");
                canonical.push_str(&line);
            }
            self.set_canonical(canonical);
            let mut statements = condition.prelude;
            statements.push(AstStmt::If {
                cond: AstExpr::Unary {
                    op: tc_ir::UnOp::LogNot,
                    val: Box::new(condition.expr),
                },
                then_body: body,
                else_body: Vec::new(),
                span: self.span(span),
            });
            return Ok(statements);
        }
        if starts_with_word(&lower, "while") {
            let rest = strip_leading_word(trimmed, "while");
            let (condition_text, body_text) = split_header_condition(rest);
            self.push_scope(ScopeKind::IfCondition);
            let condition = match self.lower_condition(&condition_text, span) {
                Ok(value) => value,
                Err(error) => {
                    self.pop_scope();
                    return Err(error);
                }
            };
            self.pop_scope();
            self.loop_depth += 1;
            self.push_scope(ScopeKind::LoopBody);
            let body_result = if let Some(items) = then_items {
                self.lower_items(items, depth + 1)
                    .map(|body| (body, Vec::new()))
            } else if body_text.is_empty() {
                Err(self.error(
                    span,
                    SemanticErrorKind::Lowering,
                    "`While` requires a body after the condition",
                ))
            } else {
                self.lower_actions_capturing(&body_text, span, depth)
            };
            self.pop_scope();
            self.loop_depth -= 1;
            let (body, body_lines) = body_result?;
            let mut canonical = format!("While {}, do:", condition.paraphrase);
            for line in body_lines {
                canonical.push_str("\n       ");
                canonical.push_str(&line);
            }
            self.set_canonical(canonical);
            let mut loop_body = condition.prelude;
            loop_body.push(AstStmt::If {
                cond: AstExpr::Unary {
                    op: tc_ir::UnOp::LogNot,
                    val: Box::new(condition.expr),
                },
                then_body: vec![AstStmt::Break(self.span(span))],
                else_body: Vec::new(),
                span: self.span(span),
            });
            loop_body.extend(body);
            return Ok(vec![AstStmt::Loop {
                body: loop_body,
                span: self.span(span),
            }]);
        }
        if starts_with_word(&lower, "repeat") {
            let inline = if then_items.is_none() {
                let rest = strip_leading_word(trimmed, "repeat");
                if let Some((start, end)) = find_top_level_word(rest, "times") {
                    Some((
                        rest[..start].trim().to_string(),
                        rest[end..]
                            .trim()
                            .trim_start_matches(',')
                            .trim()
                            .to_string(),
                    ))
                } else {
                    Some(self.split_condition_actions(rest, span)?)
                }
            } else {
                None
            };
            let count = if let Some((count_text, _)) = &inline {
                self.parse_expr(count_text, span)?
            } else {
                self.lower_repeat_count(trimmed, span)?
            };
            let counter = self.next_temp("repeat");
            let counter_ty = self.shallow_type(&count);
            self.declare_temp(&counter, counter_ty);
            let mut loop_body = vec![AstStmt::If {
                cond: AstExpr::Unary {
                    op: tc_ir::UnOp::LogNot,
                    val: Box::new(AstExpr::Binary {
                        op: tc_ir::BinOp::Lt,
                        lhs: Box::new(AstExpr::Var(counter.clone())),
                        rhs: Box::new(count.clone()),
                    }),
                },
                then_body: vec![AstStmt::Break(self.span(span))],
                else_body: Vec::new(),
                span: self.span(span),
            }];
            self.loop_depth += 1;
            self.push_scope(ScopeKind::LoopBody);
            let body_result = if let Some(items) = then_items {
                self.lower_items(items, depth + 1)
                    .map(|body| (body, Vec::new()))
            } else if let Some((_, body)) = inline.as_ref() {
                self.lower_actions_capturing(body, span, depth)
            } else {
                Err(self.error(
                    span,
                    SemanticErrorKind::Lowering,
                    "`Repeat` requires a body",
                ))
            };
            self.pop_scope();
            self.loop_depth -= 1;
            let (body, body_lines) = body_result?;
            let mut canonical = format!("Repeat {} times:", render_expr(&count));
            for line in body_lines {
                canonical.push_str("\n       ");
                canonical.push_str(&line);
            }
            self.set_canonical(canonical);
            loop_body.extend(body);
            loop_body.push(AstStmt::Assign {
                name: counter.clone(),
                val: AstExpr::Binary {
                    op: tc_ir::BinOp::Add,
                    lhs: Box::new(AstExpr::Var(counter.clone())),
                    rhs: Box::new(AstExpr::ConstInt(1, None)),
                },
                span: self.span(span),
            });
            return Ok(vec![AstStmt::Block(
                vec![
                    AstStmt::Let {
                        name: counter,
                        ty: None,
                        init: zero_like(&count),
                        span: self.span(span),
                    },
                    AstStmt::Loop {
                        body: loop_body,
                        span: self.span(span),
                    },
                ],
                self.span(span),
            )]);
        }
        if starts_with_word(&lower, "loop") {
            let body = if let Some(items) = then_items {
                self.loop_depth += 1;
                self.push_scope(ScopeKind::LoopBody);
                let body = self.lower_items(items, depth + 1);
                self.pop_scope();
                self.loop_depth -= 1;
                body?
            } else {
                Vec::new()
            };
            self.set_canonical("Loop forever:".to_string());
            return Ok(vec![AstStmt::Loop {
                body,
                span: self.span(span),
            }]);
        }
        if starts_with_word(&lower, "else") || starts_with_word(&lower, "otherwise") {
            return Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                "`Else`/`Otherwise` must follow an `If` block",
            ));
        }
        if let Some(statements) = self.try_natural_bit_update(trimmed, span)? {
            return Ok(statements);
        }
        if let Some(statement) = self.try_write_at(trimmed, span)? {
            return Ok(vec![statement]);
        }
        if let Some(statement) = self.try_set_item(trimmed, span)? {
            return Ok(vec![statement]);
        }
        if starts_with_word(&lower, "ask") {
            let value = self.parse_value(trimmed, span)?;
            return Ok(vec![AstStmt::Expr(value, self.span(span))]);
        }
        if is_direct_call_statement(trimmed) {
            let value = self.parse_value(trimmed, span)?;
            return Ok(vec![AstStmt::Expr(value, self.span(span))]);
        }
        if let Some(statements) = self.try_discourse_mutation(trimmed, span)? {
            return Ok(statements);
        }
        if let Some(statement) = self.try_assignment(trimmed, span)? {
            return Ok(statement);
        }
        if let Some(statements) = self.try_semantic_frame(trimmed, span)? {
            return Ok(statements);
        }
        if let Some(statement) = self.try_computational(trimmed, span)? {
            return Ok(statement);
        }
        let value = self.parse_value(trimmed, span)?;
        Ok(vec![AstStmt::Expr(value, self.span(span))])
    }

    pub(super) fn parse_create_type(&self, text: &str) -> Option<AstType> {
        let trimmed = text.trim();
        if let Ok(ty) = parse_type_text(trimmed) {
            return Some(ty);
        }
        let (start, end) = find_top_level_word(trimmed, "of")?;
        let base = trimmed[..start].trim();
        let mut arguments = Vec::new();
        for comma in split_type_list(trimmed[end..].trim(), ",") {
            arguments.extend(split_type_list(&comma, "and"));
        }
        if base.is_empty() || arguments.is_empty() {
            return None;
        }
        let generic = format!("{base}<{}>", arguments.join(", "));
        parse_type_text(&generic).ok()
    }

    pub(super) fn try_create_declaration(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<Vec<AstStmt>>, SemanticError> {
        let tokens = tokenize(text);
        let words = tokens
            .iter()
            .enumerate()
            .filter(|(_, token)| token.is_word())
            .collect::<Vec<_>>();
        if words.len() < 3 {
            return Ok(None);
        }
        let mut index = 1;
        if words[index].1.is("a") || words[index].1.is("an") {
            index += 1;
        }
        if index >= words.len() {
            return Ok(None);
        }
        let type_start = words[index].1.start;
        let marker = words
            .iter()
            .position(|(_, token)| token.is("called") || token.is("named"));
        let (type_end, name_index) = if let Some(marker) = marker {
            if marker + 1 >= words.len() {
                return Ok(None);
            }
            (words[marker].1.start, marker + 1)
        } else {
            (words[words.len() - 1].1.start, words.len() - 1)
        };
        if name_index >= words.len() || type_start >= type_end {
            return Ok(None);
        }
        let type_text = text[type_start..type_end].trim();
        let Some(ty) = self.parse_create_type(type_text) else {
            return Ok(None);
        };
        let name_token = words[name_index].1;
        let name = lower_name(&name_token.text);
        if !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            return Ok(None);
        }
        Ok(Some(vec![self.declare_local(&name, ty, span)]))
    }

    pub(super) fn try_discourse_mutation(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<Vec<AstStmt>>, SemanticError> {
        let tokens = tokenize(text);
        if tokens.len() < 5 {
            return Ok(None);
        }
        if !tokens[0].is_word() {
            return Ok(None);
        }
        let target = lower_name(&tokens[0].text);
        if self.lookup(&target).is_none() && !self.globals.contains_key(&target) {
            return Ok(None);
        }
        let (go_verb, up) = if tokens[1].is("goes") || tokens[1].is("go") {
            if tokens[2].is("up") {
                (true, true)
            } else if tokens[2].is("down") {
                (true, false)
            } else {
                (false, false)
            }
        } else {
            (false, false)
        };
        if !go_verb || !tokens[3].is("by") {
            return Ok(None);
        }
        let value_text = text[tokens[3].end..].trim();
        let value = self.parse_expr(value_text, span)?;
        let operator = if up {
            tc_ir::BinOp::Add
        } else {
            tc_ir::BinOp::Sub
        };
        Ok(Some(self.write_arithmetic(&target, operator, value, span)?))
    }

    pub(super) fn try_natural_bit_update(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<Vec<AstStmt>>, SemanticError> {
        let lower = text.trim().to_ascii_lowercase();
        let (set, separator) = if starts_with_word(&lower, "clear") {
            (false, "from")
        } else if starts_with_word(&lower, "set") {
            (true, "in")
        } else {
            return Ok(None);
        };
        if !starts_with_word(
            strip_leading_word(text, if set { "set" } else { "clear" }),
            "bits",
        ) {
            return Ok(None);
        }
        let tokens = tokenize(text);
        let Some(rest_token) = tokens.get(2) else {
            return Ok(None);
        };
        let rest = &text[rest_token.start..];
        let Some((start, end)) = find_top_level_word(rest, separator) else {
            return Ok(None);
        };
        let mask_text = rest[..start].trim();
        let target = rest[end..].trim();
        if mask_text.is_empty() || !is_simple_name(target) {
            return Ok(None);
        }
        let name = lower_name(target);
        let Some(ty) = self.lookup(&name).cloned() else {
            return Ok(None);
        };
        let mask = AstExpr::Cast {
            val: Box::new(self.parse_value(mask_text, span)?),
            to: ty,
        };
        let mask = if set {
            mask
        } else {
            AstExpr::Unary {
                op: tc_ir::UnOp::BitNot,
                val: Box::new(mask),
            }
        };
        let value = AstExpr::Binary {
            op: if set {
                tc_ir::BinOp::BitOr
            } else {
                tc_ir::BinOp::BitAnd
            },
            lhs: Box::new(self.parse_value(target, span)?),
            rhs: Box::new(mask),
        };
        Ok(Some(self.assign_text(target, value, span, true)?))
    }

    pub(super) fn try_write_at(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<AstStmt>, SemanticError> {
        if !starts_with_word(text, "write") {
            return Ok(None);
        }
        let rest = strip_leading_word(text, "write");
        let Some((start, end)) = find_top_level_word(rest, "at") else {
            return Ok(None);
        };
        let value_text = rest[..start].trim();
        let pointer_text = rest[end..].trim();
        if value_text.is_empty() || pointer_text.is_empty() {
            return Ok(None);
        }
        let ptr = parse_store_address(pointer_text, &self.expr_env())
            .map_err(|message| self.error(span, SemanticErrorKind::Lowering, message))?;
        Ok(Some(AstStmt::Store {
            ptr,
            val: self.parse_value(value_text, span)?,
            span: self.span(span),
        }))
    }

    pub(super) fn try_set_item(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<AstStmt>, SemanticError> {
        if !starts_with_word(text, "set") {
            return Ok(None);
        }
        let rest = strip_leading_word(text, "set").trim();
        let rest = if starts_with_word(rest, "item") {
            strip_leading_word(rest, "item")
        } else if starts_with_word(rest, "element") {
            strip_leading_word(rest, "element")
        } else {
            return Ok(None);
        };
        let Some((of_start, of_end)) = find_top_level_word(rest, "of") else {
            return Ok(None);
        };
        let after_of = &rest[of_end..];
        let Some((to_start, to_end)) = find_top_level_word(after_of, "to") else {
            return Ok(None);
        };
        let index_text = rest[..of_start].trim();
        let pointer_text = after_of[..to_start].trim();
        let value_text = after_of[to_end..].trim();
        if index_text.is_empty() || pointer_text.is_empty() || value_text.is_empty() {
            return Ok(None);
        }
        let base_ptr = parse_store_address(pointer_text, &self.expr_env())
            .map_err(|message| self.error(span, SemanticErrorKind::Lowering, message))?;
        let address = AstExpr::IndexAddr {
            base_ptr: Box::new(base_ptr),
            index: Box::new(self.parse_value(index_text, span)?),
        };
        Ok(Some(AstStmt::Store {
            ptr: address,
            val: self.parse_value(value_text, span)?,
            span: self.span(span),
        }))
    }

    pub(super) fn try_memcopy(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<AstStmt>, SemanticError> {
        let rest = if starts_with_word(text, "memcopy") {
            strip_leading_word(text, "memcopy")
        } else {
            strip_leading_word(text, "copy")
        };
        let Some((bytes_start, bytes_end)) = find_top_level_word(rest, "bytes") else {
            return Ok(None);
        };
        let Some((_, from_end)) = find_top_level_word(&rest[bytes_end..], "from") else {
            return Ok(None);
        };
        let from_end = bytes_end + from_end;
        let Some((to_start, to_end)) = find_top_level_word(&rest[from_end..], "to")
            .or_else(|| find_top_level_word(&rest[from_end..], "into"))
        else {
            return Ok(None);
        };
        let to_start = from_end + to_start;
        let to_end = from_end + to_end;
        let size_text = rest[..bytes_start].trim();
        let src_text = rest[from_end..to_start].trim();
        let dst_text = rest[to_end..].trim();
        if size_text.is_empty() || src_text.is_empty() || dst_text.is_empty() {
            return Ok(None);
        }
        Ok(Some(AstStmt::MemCopy {
            dst: self.parse_value(dst_text, span)?,
            src: self.parse_value(src_text, span)?,
            size: self.parse_value(size_text, span)?,
            span: self.span(span),
        }))
    }

    pub(super) fn try_syscall_with(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<AstStmt>, SemanticError> {
        let rest = strip_leading_word(text, "syscall");
        let Some((start, end)) = find_top_level_word(rest, "with") else {
            return Ok(None);
        };
        let num_text = rest[..start].trim();
        let args_text = rest[end..].trim();
        let mut args = Vec::new();
        for argument in split_top_level(args_text, ",") {
            args.push(self.parse_expr(&argument, span)?);
        }
        Ok(Some(AstStmt::Expr(
            AstExpr::Syscall {
                num: Box::new(self.parse_expr(num_text, span)?),
                args,
            },
            self.span(span),
        )))
    }

    pub(super) fn declaration_type(&self, text: &str) -> Option<AstType> {
        let trimmed = text.trim();
        let stripped = trimmed
            .strip_prefix("a ")
            .or_else(|| trimmed.strip_prefix("an "))
            .unwrap_or(trimmed);
        if let Ok(ty) = parse_type_text(stripped) {
            if let AstType::Named { module, name, .. } = &ty {
                if module.is_some() || self.types.contains(name) {
                    return Some(ty);
                }
            }
        }
        let words = tokenize(trimmed)
            .into_iter()
            .filter(|token| token.is_word())
            .map(|token| token.lower)
            .collect::<Vec<_>>();
        let name = match words.as_slice() {
            [name] => name,
            [article, name] if article == "a" || article == "an" => name,
            _ => return None,
        };
        self.types.contains(name).then(|| AstType::Named {
            module: None,
            name: name.clone(),
            type_args: Vec::new(),
        })
    }

    pub(super) fn declare_local(&mut self, name: &str, ty: AstType, span: Span) -> AstStmt {
        let init = match &ty {
            AstType::Ptr(_) => AstExpr::ConstNull(ty.clone()),
            AstType::Named { .. } | AstType::Array(..) => AstExpr::Zero(ty.clone()),
            AstType::F32 | AstType::F64 => AstExpr::ConstFloat(0.0, Some(ty.clone())),
            AstType::Bool => AstExpr::ConstBool(false),
            AstType::Void => AstExpr::ConstInt(0, None),
            AstType::U8 | AstType::U16 | AstType::U32 | AstType::U64 => {
                AstExpr::ConstUint(0, Some(ty.clone()))
            }
            _ => AstExpr::ConstInt(0, Some(ty.clone())),
        };
        self.declare(name, ty.clone());
        self.introduce_name(name, &ty, span);
        AstStmt::Let {
            name: name.to_string(),
            ty: Some(ty),
            init,
            span: self.span(span),
        }
    }

    pub(super) fn try_local_declaration(&mut self, text: &str, span: Span) -> Option<Vec<AstStmt>> {
        let tokens = tokenize(text);
        let mut words = tokens.iter().filter(|token| token.is_word());
        if !words.next().is_some_and(|token| token.is("define")) {
            return None;
        }
        let mut next = words.next()?;
        if next.is("local") {
            next = words.next()?;
        }
        let name = lower_name(&next.text);
        if words.next().is_some_and(|token| token.is("as")) {
            let type_start = tokens
                .iter()
                .position(|token| token.is("as"))
                .and_then(|index| tokens.get(index + 1))
                .map_or(text.len(), |token| token.start);
            let ty_text = text[type_start..].trim().trim_end_matches('.');
            if let Ok(ty) = parse_type_text(ty_text) {
                if self.lookup(&name).is_some() {
                    return None;
                }
                return Some(vec![self.declare_local(&name, ty, span)]);
            }
        }
        None
    }

    pub(super) fn lower_quantifier_value(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<(Vec<AstStmt>, AstExpr)>, SemanticError> {
        let tokens = tokenize(text);
        if tokens.first().is_some_and(|token| token.is("no"))
            && tokens.get(1).is_some_and(|token| token.is("address"))
        {
            return Ok(None);
        }
        let has_quantifier = tokens.iter().any(|token| {
            token.kind == TokenKind::Word
                && matches!(token.lower.as_str(), "every" | "each" | "some" | "no")
        });
        if !has_quantifier {
            return Ok(None);
        }
        let parsed = parse_condition(text)
            .map_err(|message| self.error(span, SemanticErrorKind::Lowering, message))?;
        let lowered = self.lower_cond_ast(&parsed, span)?;
        Ok(Some((lowered.prelude, lowered.expr)))
    }

    pub(crate) fn split_emit_parts(&mut self, text: &str, span: Span) -> Option<Vec<String>> {
        let with = find_top_level_phrase(text, "with");
        let followed = find_top_level_phrase(text, "followed by");
        let (start, end) = match (with, followed) {
            (Some((left, end)), Some((right, followed_end))) => {
                if left.len() <= right.len() {
                    (left.len(), end)
                } else {
                    (right.len(), followed_end)
                }
            }
            (Some((left, end)), None) | (None, Some((left, end))) => (left.len(), end),
            (None, None) => return None,
        };
        let first = text[..start].trim();
        let rest = text[end..].trim();
        if first.is_empty() || rest.is_empty() {
            return None;
        }
        let mut parts = vec![first.to_string()];
        parts.extend(self.split_output_parts(rest, span)?);
        (parts.len() >= 2).then_some(parts)
    }

    pub(super) fn split_output_parts(&mut self, text: &str, span: Span) -> Option<Vec<String>> {
        let text = text.trim().trim_end_matches(',').trim();
        if text.is_empty() {
            return None;
        }
        if let Ok(value) = self.parse_value(text, span)
            && !matches!(
                value,
                AstExpr::Binary {
                    op: tc_ir::BinOp::LogAnd | tc_ir::BinOp::LogOr,
                    ..
                }
            )
        {
            return Some(vec![text.to_string()]);
        }
        let tokens = tokenize(text);
        let mut separators = Vec::new();
        let mut depth = 0_i32;
        for (index, token) in tokens.iter().enumerate() {
            if token.kind == TokenKind::Symbol {
                match token.text.as_str() {
                    "(" | "[" | "{" => depth += 1,
                    ")" | "]" | "}" => depth -= 1,
                    "," if depth == 0 => separators.push(index),
                    _ => {}
                }
            } else if depth == 0 && (token.is("and") || token.is("with")) {
                separators.push(index);
            }
        }
        for index in separators.into_iter().rev() {
            let token = &tokens[index];
            let left = text[..token.start].trim();
            let right = text[token.end..].trim();
            if left.is_empty() || right.is_empty() {
                continue;
            }
            let Some(mut left_parts) = self.split_output_parts(left, span) else {
                continue;
            };
            let Some(right_parts) = self.split_output_parts(right, span) else {
                continue;
            };
            left_parts.extend(right_parts);
            return Some(left_parts);
        }
        None
    }

    pub(crate) fn lower_emit_parts(
        &mut self,
        parts: &[String],
        span: Span,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        let mut statements = Vec::new();
        for part in parts {
            let value = self.parse_value(part, span)?;
            let statement = if let Some((pointer, length)) = self.text_bytes(&value) {
                AstExpr::Call {
                    module: None,
                    name: "write_bytes".to_string(),
                    type_args: Vec::new(),
                    args: vec![pointer, length],
                }
            } else {
                let name = match self.shallow_type_opt(&value) {
                    Some(AstType::U8 | AstType::U16 | AstType::U32 | AstType::U64) => "write_u64",
                    Some(AstType::F32 | AstType::F64) => "write_f64",
                    Some(AstType::Bool) => "write_bool",
                    _ => "write_i64",
                };
                AstExpr::Call {
                    module: None,
                    name: name.to_string(),
                    type_args: Vec::new(),
                    args: vec![value],
                }
            };
            statements.push(AstStmt::Expr(statement, self.span(span)));
        }
        statements.push(AstStmt::Expr(
            AstExpr::Call {
                module: None,
                name: "write_newline".to_string(),
                type_args: Vec::new(),
                args: Vec::new(),
            },
            self.span(span),
        ));
        Ok(statements)
    }

    pub(super) fn try_assignment(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<Vec<AstStmt>>, SemanticError> {
        for (symbol, operator) in [
            ("+=", tc_ir::BinOp::Add),
            ("-=", tc_ir::BinOp::Sub),
            ("*=", tc_ir::BinOp::Mul),
            ("/=", tc_ir::BinOp::Div),
        ] {
            if let Some((start, end)) = find_top_level_symbol(text, symbol) {
                let target_text = text[..start].trim();
                let value_text = text[end..].trim();
                let value = self.parse_expr(value_text, span)?;
                return Ok(Some(self.write_arithmetic(
                    target_text,
                    operator,
                    value,
                    span,
                )?));
            }
        }
        if starts_with_word(text, "let") {
            let rest = strip_leading_word(text, "let");
            let binding = ["be", "start at", "starts at", "begin at", "begins at"]
                .iter()
                .find_map(|operator| {
                    find_top_level_phrase(rest, operator)
                        .map(|(target, end)| (target, rest[end..].trim().to_string()))
                });
            if let Some((target, value_text)) = binding {
                let target = target.trim();
                let value_text = value_text.as_str();
                if let Some(statements) = self.try_repeated_list(target, value_text, span)? {
                    return Ok(Some(statements));
                }
                if let Some((ty, elements)) = self.parse_inline_list(value_text, span)? {
                    return Ok(Some(self.lower_inline_list(target, ty, elements, span)?));
                }
                if is_simple_name(target) {
                    let name = lower_name(target);
                    if let Some(ty) = self.declaration_type(value_text) {
                        if self.lookup(&name).is_none() {
                            return Ok(Some(vec![self.declare_local(&name, ty, span)]));
                        }
                    }
                }
                if let Some((mut prelude, value)) = self.lower_quantifier_value(value_text, span)? {
                    let mut assigned = self.assign_text(target, value, span, true)?;
                    prelude.append(&mut assigned);
                    return Ok(Some(prelude));
                }
                let value = self.parse_value(value_text, span)?;
                let assigned = self.assign_text(target, value, span, true);
                return Ok(Some(assigned?));
            }
        }
        for keyword in ["is", "becomes", "equals"] {
            if let Some((start, end)) = find_top_level_word(text, keyword) {
                let canonical_target = canonical_assignment_target(text[..start].trim());
                let target = canonical_target.as_str();
                let value_text = text[end..].trim();
                if target.is_empty() || !is_lvalue_text(target) {
                    continue;
                }
                if is_simple_name(target) {
                    let name = lower_name(target);
                    if let Some(ty) = self.declaration_type(value_text) {
                        if self.lookup(&name).is_none() {
                            return Ok(Some(vec![self.declare_local(&name, ty, span)]));
                        }
                    }
                }
                if let Some((mut prelude, value)) = self.lower_quantifier_value(value_text, span)? {
                    let mut assigned = self.assign_text(target, value, span, true)?;
                    prelude.append(&mut assigned);
                    return Ok(Some(prelude));
                }
                let Ok(value) = self.parse_value(value_text, span) else {
                    continue;
                };
                return Ok(Some(self.assign_text(target, value, span, true)?));
            }
        }
        if let Some((start, end)) = find_top_level_symbol(text, "=") {
            let canonical_target = canonical_assignment_target(text[..start].trim());
            let target = canonical_target.as_str();
            let value_text = text[end..].trim();
            if is_lvalue_text(target) && !value_text.is_empty() {
                if let Some((mut prelude, value)) = self.lower_quantifier_value(value_text, span)? {
                    let mut assigned = self.assign_text(target, value, span, true)?;
                    prelude.append(&mut assigned);
                    return Ok(Some(prelude));
                }
                let value = self.parse_value(value_text, span)?;
                let assigned = self.assign_text(target, value, span, true);
                return Ok(Some(assigned?));
            }
        }
        Ok(None)
    }

    pub(crate) fn assign_text(
        &mut self,
        target_text: &str,
        value: AstExpr,
        span: Span,
        allow_declare: bool,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        let _place = parse_place(target_text)
            .map_err(|message| self.error(span, SemanticErrorKind::Lowering, message))?;
        let canonical_target = canonical_assignment_target(target_text);
        let target_text = canonical_target.as_str();
        if is_simple_name(target_text) {
            let name = lower_name(target_text);
            if let AstExpr::BytesLit(bytes) = &value {
                self.bytes_lengths.insert(name, bytes.len());
            } else {
                self.bytes_lengths.remove(&name);
            }
        }
        if is_simple_name(target_text) {
            let name = lower_name(target_text);
            if self.current_scope_contains(&name) {
                return Ok(vec![self.write_target(&Target::Local(name), value, span)]);
            }
            if self.globals.contains_key(&name)
                && !self
                    .scopes
                    .iter()
                    .any(|scope| scope.vars.contains_key(&name))
            {
                return Ok(vec![self.write_target(&Target::Global(name), value, span)]);
            }
            if let Some(outer) = self.lookup(&name).cloned() {
                let value_ty = self.shallow_type_opt(&value);
                let same_type = value_ty.as_ref().is_none_or(|value_ty| outer == *value_ty);
                if same_type {
                    let target = if self.globals.contains_key(&name)
                        && !self
                            .scopes
                            .iter()
                            .any(|scope| scope.vars.contains_key(&name))
                    {
                        Target::Global(name)
                    } else {
                        Target::Local(name)
                    };
                    return Ok(vec![self.write_target(&target, value, span)]);
                }
                // A known type-changing assignment in a nested block is a
                // shadow, not a mutation of the outer binding.
                let value_ty = value_ty.unwrap_or(AstType::I64);
                self.declare(&name, value_ty.clone());
                self.introduce_name(&name, &value_ty, span);
                self.discourse.set_last_result(&name);
                return Ok(vec![AstStmt::Let {
                    name,
                    ty: None,
                    init: value,
                    span: self.span(span),
                }]);
            }
            if allow_declare {
                let ty = self.shallow_type(&value);
                self.declare(&name, ty.clone());
                self.introduce_name(&name, &ty, span);
                self.discourse.set_last_result(&name);
                return Ok(vec![AstStmt::Let {
                    name,
                    ty: None,
                    init: value,
                    span: self.span(span),
                }]);
            }
            return Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                format!("assignment target `{name}` is not declared"),
            ));
        }
        let address = parse_address(target_text, &self.expr_env())
            .map_err(|message| self.error(span, SemanticErrorKind::Lowering, message))?;
        self.touch_target_text(target_text, span);
        Ok(vec![AstStmt::Store {
            ptr: address,
            val: value,
            span: self.span(span),
        }])
    }

    pub(crate) fn write_arithmetic(
        &mut self,
        target_text: &str,
        operator: tc_ir::BinOp,
        value: AstExpr,
        span: Span,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        let (prelude, target) = self.resolve_target(target_text, span)?;
        let current = target.read();
        let combined = AstExpr::Binary {
            op: operator,
            lhs: Box::new(current),
            rhs: Box::new(value),
        };
        let mut statements = prelude;
        statements.push(self.write_target(&target, combined, span));
        self.touch_target_text(target_text, span);
        Ok(statements)
    }

    pub(crate) fn resolve_target(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<(Vec<AstStmt>, Target), SemanticError> {
        let _place = parse_place(text)
            .map_err(|message| self.error(span, SemanticErrorKind::Lowering, message))?;
        let canonical_target = canonical_assignment_target(text);
        let text = canonical_target.as_str();
        if is_simple_name(text) {
            let name = lower_name(text);
            if self.globals.contains_key(&name) {
                return Ok((Vec::new(), Target::Global(name)));
            }
            if self
                .scopes
                .iter()
                .any(|scope| scope.vars.contains_key(&name))
            {
                return Ok((Vec::new(), Target::Local(name)));
            }
            self.declare(&name, AstType::I64);
            self.introduce_name(&name, &AstType::I64, span);
            return Ok((
                vec![AstStmt::Let {
                    name: name.clone(),
                    ty: None,
                    init: AstExpr::ConstInt(0, None),
                    span: self.span(span),
                }],
                Target::Local(name),
            ));
        }
        let address = parse_address(text, &self.expr_env())
            .map_err(|message| self.error(span, SemanticErrorKind::Lowering, message))?;
        Ok((Vec::new(), Target::Memory(address)))
    }

    pub(crate) fn write_target(&mut self, target: &Target, value: AstExpr, span: Span) -> AstStmt {
        match target {
            Target::Local(name) => {
                self.touch_name(name, span);
                self.discourse.set_last_result(name);
                AstStmt::Assign {
                    name: name.clone(),
                    val: value,
                    span: self.span(span),
                }
            }
            Target::Global(name) => {
                self.touch_name(name, span);
                self.discourse.set_last_result(name);
                AstStmt::Store {
                    ptr: AstExpr::GlobalAddr {
                        module: None,
                        name: name.clone(),
                    },
                    val: value,
                    span: self.span(span),
                }
            }
            Target::Memory(address) => AstStmt::Store {
                ptr: address.clone(),
                val: value,
                span: self.span(span),
            },
        }
    }

    pub(super) fn try_computational(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<Vec<AstStmt>>, SemanticError> {
        let tokens = tokenize(text);
        let mut verb_index = None;
        let mut resolution = None;
        for (index, token) in tokens.iter().enumerate() {
            if token.kind != TokenKind::Word
                || matches!(
                    token.lower.as_str(),
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
            {
                continue;
            }
            if tokens.get(index + 1).is_some_and(|next| {
                next.is_word()
                    && matches!(
                        next.lower.as_str(),
                        "am" | "is" | "are" | "was" | "were" | "be" | "been" | "being"
                    )
            }) {
                continue;
            }
            let prep = tokens.iter().skip(index + 1).find_map(|candidate| {
                if candidate.kind == TokenKind::Word
                    && matches!(
                        tc_lexicon::lookup_closed_class(&candidate.lower),
                        Some(tc_lexicon::ClosedClass::Preposition)
                    )
                {
                    Some(candidate.lower.clone())
                } else {
                    None
                }
            });
            let resolved = prep
                .as_deref()
                .and_then(|prep| classify_verb(&token.lower, Some(prep)))
                .or_else(|| classify_verb(&token.lower, None));
            if let Some(resolved) = resolved {
                if resolved.predicate.is_some() || resolved.concept.is_some() {
                    verb_index = Some(index);
                    resolution = Some(resolved);
                    break;
                }
            }
        }
        let Some(verb_index) = verb_index else {
            return Ok(None);
        };
        let Some(resolution) = resolution else {
            return Ok(None);
        };
        let clause = analyze_clause(
            self.parser,
            &RawSentence {
                text: text.to_string(),
                raw: String::new(),
                span,
                fillers: Vec::new(),
            },
        );
        let mut group_depth = 0_i32;
        let prep_indices = tokens
            .iter()
            .enumerate()
            .filter_map(|(index, token)| {
                if token.kind == TokenKind::Symbol {
                    match token.text.as_str() {
                        "(" | "[" | "{" => group_depth += 1,
                        ")" | "]" | "}" => group_depth -= 1,
                        _ => {}
                    }
                    return None;
                }
                (index > verb_index
                    && group_depth == 0
                    && token.kind == TokenKind::Word
                    && matches!(
                        tc_lexicon::lookup_closed_class(&token.lower),
                        Some(tc_lexicon::ClosedClass::Preposition)
                    ))
                .then_some(index)
            })
            .collect::<Vec<_>>();
        let first_prep = prep_indices.first().copied();
        let prep_word = first_prep.map(|index| tokens[index].lower.clone());
        let role = prep_word.as_deref().and_then(prep_role);
        let mut subject_text = clause
            .subject
            .as_ref()
            .map(|subject| join_tokens(subject))
            .unwrap_or_default();
        if subject_text.is_empty() {
            let before_auxiliary = tokens
                .iter()
                .take_while(|token| {
                    !(token.is_word()
                        && matches!(
                            token.lower.as_str(),
                            "am" | "is" | "are" | "was" | "were" | "be" | "been" | "being"
                        ))
                })
                .filter(|token| token.is_word() || token.kind == TokenKind::Number)
                .cloned()
                .collect::<Vec<_>>();
            subject_text = join_tokens(&before_auxiliary);
        }
        let after_verb_start = tokens
            .get(verb_index + 1)
            .map_or(text.len(), |token| token.start);
        let before_prep_end = first_prep.map_or(text.len(), |index| tokens[index].start);
        let after_prep_start = first_prep
            .and_then(|index| tokens.get(index + 1))
            .map_or(text.len(), |token| token.start);
        let between_verb_prep = text[after_verb_start..before_prep_end].trim();
        let after_prep = text[after_prep_start..].trim();
        let be_auxiliary = verb_index > 0
            && tokens.get(verb_index - 1).is_some_and(|token| {
                token.is_word()
                    && matches!(
                        token.lower.as_str(),
                        "am" | "is" | "are" | "was" | "were" | "be" | "been" | "being"
                    )
            });
        let passive = clause.voice == Voice::Passive || be_auxiliary;
        match resolution.concept {
            Some(SemanticConcept::Double) => {
                let target = if passive {
                    subject_text.as_str()
                } else {
                    after_verb_start_text(&tokens, verb_index, text)
                };
                let value = AstExpr::ConstInt(2, None);
                return Ok(Some(self.write_arithmetic(
                    target,
                    tc_ir::BinOp::Mul,
                    value,
                    span,
                )?));
            }
            Some(SemanticConcept::Halve) => {
                let target = if passive {
                    subject_text.as_str()
                } else {
                    after_verb_start_text(&tokens, verb_index, text)
                };
                let value = AstExpr::ConstInt(2, None);
                return Ok(Some(self.write_arithmetic(
                    target,
                    tc_ir::BinOp::Div,
                    value,
                    span,
                )?));
            }
            Some(SemanticConcept::Triple) => {
                let target = if passive {
                    subject_text.as_str()
                } else {
                    after_verb_start_text(&tokens, verb_index, text)
                };
                let value = AstExpr::ConstInt(3, None);
                return Ok(Some(self.write_arithmetic(
                    target,
                    tc_ir::BinOp::Mul,
                    value,
                    span,
                )?));
            }
            Some(SemanticConcept::Negate) => {
                let target = if passive {
                    subject_text.as_str()
                } else {
                    after_verb_start_text(&tokens, verb_index, text)
                };
                let (prelude, target) = self.resolve_target(target, span)?;
                let current = target.read();
                let negated = AstExpr::Unary {
                    op: tc_ir::UnOp::Neg,
                    val: Box::new(current),
                };
                let mut statements = prelude;
                statements.push(self.write_target(&target, negated, span));
                return Ok(Some(statements));
            }
            _ => {}
        }
        let Some(predicate) = resolution.predicate else {
            return Ok(None);
        };
        match predicate {
            VerbPredicate::Assign => {
                if prep_indices.is_empty() {
                    return Ok(None);
                }
                let mut chosen = None;
                for prep_index in &prep_indices {
                    let before = text[after_verb_start..tokens[*prep_index].start].trim();
                    let after = text[tokens[*prep_index].end..].trim();
                    let canonical_before = canonical_assignment_target(before);
                    let canonical_after = canonical_assignment_target(after);
                    let chosen_role = prep_role(&tokens[*prep_index].lower);
                    let locative = matches!(
                        tokens[*prep_index].lower.as_str(),
                        "in" | "into" | "onto" | "inside"
                    );
                    if locative && is_lvalue_text(&canonical_after) {
                        if let Ok(value) = self.parse_expr(before, span) {
                            chosen = Some((canonical_after, value, chosen_role));
                            break;
                        }
                    }
                    if is_lvalue_text(&canonical_before) {
                        if let Ok(value) = self.parse_value(after, span) {
                            chosen = Some((canonical_before, value, chosen_role));
                            break;
                        }
                    }
                    if is_lvalue_text(&canonical_after) {
                        if let Ok(value) = self.parse_value(before, span) {
                            chosen = Some((canonical_after, value, chosen_role));
                            break;
                        }
                    }
                }
                let Some((target, value, chosen_role)) = chosen else {
                    return Err(self.error(
                        span,
                        SemanticErrorKind::Lowering,
                        format!(
                            "assignment verb `{}` needs a target and a value",
                            resolution.lemma
                        ),
                    ));
                };
                if chosen_role == Some(tc_lexicon::PrepRole::Location) {
                    let address =
                        parse_store_address(&target, &self.expr_env()).map_err(|message| {
                            self.error(span, SemanticErrorKind::Lowering, message)
                        })?;
                    return Ok(Some(vec![AstStmt::Store {
                        ptr: address,
                        val: value,
                        span: self.span(span),
                    }]));
                }
                Ok(Some(self.assign_text(&target, value, span, true)?))
            }
            VerbPredicate::TransferAdd => Ok(Some(self.lower_transfer(
                resolution.concept,
                text,
                &tokens,
                verb_index,
                first_prep,
                passive,
                &subject_text,
                role,
                tc_ir::BinOp::Add,
                span,
            )?)),
            VerbPredicate::TransferSub => Ok(Some(self.lower_transfer(
                resolution.concept,
                text,
                &tokens,
                verb_index,
                first_prep,
                passive,
                &subject_text,
                role,
                tc_ir::BinOp::Sub,
                span,
            )?)),
            VerbPredicate::TransferMove => {
                if prep_indices.len() < 2 {
                    return Ok(None);
                }
                let source_index = prep_indices[0];
                let destination_index = prep_indices[1];
                let source = text[tokens[source_index].end..tokens[destination_index].start].trim();
                let destination = text[tokens[destination_index].end..].trim();
                let value_text = if passive && !subject_text.is_empty() {
                    subject_text.as_str()
                } else {
                    between_verb_prep
                };
                let value = self.parse_expr(value_text, span)?;
                let mut statements =
                    self.write_arithmetic(source, tc_ir::BinOp::Sub, value.clone(), span)?;
                statements.extend(self.write_arithmetic(
                    destination,
                    tc_ir::BinOp::Add,
                    value.clone(),
                    span,
                )?);
                if let Some(floor_text) = self.active_move_floor() {
                    let floor = self.parse_value(&floor_text, span)?;
                    let available = self.parse_value(source, span)?;
                    let required = AstExpr::Binary {
                        op: tc_ir::BinOp::Add,
                        lhs: Box::new(value),
                        rhs: Box::new(floor),
                    };
                    statements = vec![AstStmt::If {
                        cond: AstExpr::Binary {
                            op: tc_ir::BinOp::Ge,
                            lhs: Box::new(available),
                            rhs: Box::new(required),
                        },
                        then_body: statements,
                        else_body: Vec::new(),
                        span: self.span(span),
                    }];
                }
                Ok(Some(statements))
            }
            VerbPredicate::Transform | VerbPredicate::Combine => {
                let Some(operator) = transform_operator(resolution.concept, predicate) else {
                    return Ok(None);
                };
                let (target, value) = if passive {
                    (subject_text.as_str(), after_prep)
                } else {
                    match role {
                        Some(tc_lexicon::PrepRole::Extent) => (between_verb_prep, after_prep),
                        Some(tc_lexicon::PrepRole::Instrument) => (between_verb_prep, after_prep),
                        Some(tc_lexicon::PrepRole::Destination) => (after_prep, between_verb_prep),
                        _ => (between_verb_prep, after_prep),
                    }
                };
                if target.is_empty() {
                    return Ok(None);
                }
                let value = if value.is_empty() {
                    AstExpr::ConstInt(1, None)
                } else {
                    self.parse_value(value, span)?
                };
                let mut operator = if predicate == VerbPredicate::Combine {
                    tc_ir::BinOp::Add
                } else {
                    operator
                };
                if role == Some(tc_lexicon::PrepRole::Extent) {
                    let lower_text = text.to_ascii_lowercase();
                    if lower_text.contains(" up by ") {
                        operator = tc_ir::BinOp::Add;
                    } else if lower_text.contains(" down by ")
                        || matches!(resolution.concept, Some(SemanticConcept::Transfer))
                    {
                        operator = tc_ir::BinOp::Sub;
                    }
                }
                Ok(Some(self.write_arithmetic(target, operator, value, span)?))
            }
            VerbPredicate::Create
            | VerbPredicate::Destroy
            | VerbPredicate::Emit
            | VerbPredicate::Receive => Ok(None),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_transfer(
        &mut self,
        concept: Option<SemanticConcept>,
        text: &str,
        tokens: &[Token],
        verb_index: usize,
        first_prep: Option<usize>,
        passive: bool,
        subject_text: &str,
        role: Option<tc_lexicon::PrepRole>,
        operator: tc_ir::BinOp,
        span: Span,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        let Some(prep_index) = first_prep else {
            let target = self
                .active_loop_var
                .clone()
                .or_else(|| self.discourse.last_result());
            let Some(target) = target else {
                return Ok(Vec::new());
            };
            let after_verb = tokens
                .get(verb_index + 1)
                .map_or(text.len(), |token| token.start);
            let amount_text = text[after_verb..].trim();
            let amount = if amount_text.is_empty() {
                AstExpr::ConstInt(1, None)
            } else {
                self.parse_value(amount_text, span)?
            };
            return self.write_arithmetic(&target, operator, amount, span);
        };
        let after_verb = tokens
            .get(verb_index + 1)
            .map_or(text.len(), |token| token.start);
        let between = text[after_verb..tokens[prep_index].start].trim();
        let after = text[tokens[prep_index].end..].trim();
        let _ = concept;
        let (target, value_text) = if passive {
            match role {
                Some(tc_lexicon::PrepRole::Extent) | Some(tc_lexicon::PrepRole::Instrument) => {
                    (subject_text, after)
                }
                _ => {
                    if after.is_empty() {
                        (subject_text, between)
                    } else {
                        (after, subject_text)
                    }
                }
            }
        } else {
            match role {
                Some(tc_lexicon::PrepRole::Destination) => (after, between),
                Some(tc_lexicon::PrepRole::Source) => (after, between),
                Some(tc_lexicon::PrepRole::Extent) => (between, after),
                Some(tc_lexicon::PrepRole::Instrument) => (between, after),
                _ => {
                    if is_lvalue_text(between) {
                        (between, after)
                    } else {
                        (after, between)
                    }
                }
            }
        };
        if target.is_empty() {
            return Ok(Vec::new());
        }
        let value = if value_text.is_empty() {
            AstExpr::ConstInt(0, None)
        } else {
            self.parse_value(value_text, span)?
        };
        self.write_arithmetic(target, operator, value, span)
    }
}
