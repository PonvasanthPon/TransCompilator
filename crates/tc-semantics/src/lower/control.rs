//! Control flow, conditions, loops, quantifiers, and clause graphs.

use super::{
    AstExpr, AstStmt, AstType, CheckDomain, CheckKind, CheckRecord, ClauseGraph, ClauseTree,
    CondAst, GlobalQuantifier, HashSet, IntentVerdict, LocalQuantKind, LocalQuantSpec, LoweredCond,
    Lowerer, MAX_DEPTH, PredSpec, QuantDomainSpec, RawItem, RawSentence, ScopeKind, SemanticError,
    SemanticErrorKind, Span, Token, TokenKind, ValueContext, VerbPredicate, analyze_clause,
    apply_loop_filters, canonical_assignment_target, canonicalize_statement, classify,
    classify_verb, clause_graph, closer_needs_last_number, continuation_rest,
    extract_largest_trackers, find_top_level_phrase, find_top_level_symbol, find_top_level_word,
    first_word, is_lvalue_text, lower_name, parse_adjacent_pass, parse_check, parse_condition,
    parse_loop_introduction, parse_named_list_start, parse_pass_cleanup, parse_sequence_generation,
    parse_sequence_iteration, parse_sequence_seed, parse_state_declaration, parse_type_text,
    post_loop_rest, prepare_loop_rules, raw_item_end_line, raw_item_start_line,
    render_cond_child_text, render_expr, rewrite_bound_names, semantic_error, split_actions,
    split_condition_comparison, split_header_condition, split_top_level, split_where,
    starts_control, starts_with_word, strip_leading_word, strip_outer_parentheses, tokenize,
};

impl Lowerer<'_> {
    pub(super) fn lower_items(
        &mut self,
        items: &[RawItem],
        depth: usize,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        if depth > MAX_DEPTH {
            return Err(semantic_error(
                &self.module,
                Span::new(1, 1),
                SemanticErrorKind::Lowering,
                "statement nesting exceeded the 32-level guard",
            ));
        }
        let mut statements = Vec::new();
        let mut index = 0;
        while index < items.len() {
            if let RawItem::Sentence(sentence) = &items[index]
                && let Some(start) = parse_named_list_start(&sentence.text)
            {
                let declaration = format!("Let {} be a list of {}.", start.name, start.elements);
                let mut lowered =
                    self.lower_core(&declaration, sentence.span, None, None, depth + 1)?;
                self.last_list = Some(start.name.clone());
                let mut cursor = index + 1;
                let has_pass = items.get(cursor).is_some_and(|item| {
                    matches!(
                        item,
                        RawItem::Sentence(pass)
                            if parse_adjacent_pass(&pass.text).is_some()
                    )
                });
                if has_pass {
                    let pass_item = &items[cursor];
                    let pass_span = match pass_item {
                        RawItem::Sentence(pass) => pass.span,
                        RawItem::Block(block) => block.span,
                    };
                    cursor += 1;
                    let mut body = Vec::new();
                    let mut cleanup = None;
                    let mut closer = None;
                    let mut last_line = raw_item_end_line(pass_item);
                    while cursor < items.len() {
                        let start_line = raw_item_start_line(&items[cursor]);
                        if start_line > last_line.saturating_add(1) {
                            break;
                        }
                        if let RawItem::Sentence(next) = &items[cursor] {
                            if cleanup.is_none()
                                && let Some(predicate) = parse_pass_cleanup(&next.text)
                            {
                                cleanup = Some(predicate);
                                last_line = raw_item_end_line(&items[cursor]);
                                cursor += 1;
                                continue;
                            }
                            let closer_text = if next.fillers.is_empty() {
                                next.text.clone()
                            } else {
                                format!("{}, {}", next.fillers.join(" "), next.text)
                            };
                            if let Some(rest) = post_loop_rest(&closer_text) {
                                closer = Some((rest, next.span));
                                cursor += 1;
                                break;
                            }
                        }
                        body.push(items[cursor].clone());
                        last_line = raw_item_end_line(&items[cursor]);
                        cursor += 1;
                    }
                    lowered.extend(self.lower_adjacent_pass(
                        &start.name,
                        body,
                        cleanup,
                        closer,
                        pass_span,
                        depth + 1,
                    )?);
                    statements.extend(lowered);
                    index = cursor;
                    continue;
                }
                statements.extend(lowered);
                index += 1;
                continue;
            }
            if let RawItem::Sentence(sentence) = &items[index]
                && let Some(declarations) = parse_state_declaration(&sentence.text)
            {
                for (name, value_text) in declarations {
                    let value = self.parse_expr(&value_text, sentence.span)?;
                    let ty = self.shallow_type(&value);
                    if self.lookup(&name).is_some() || self.globals.contains_key(&name) {
                        statements.extend(self.assign_text(&name, value, sentence.span, false)?);
                    } else {
                        self.declare(&name, ty.clone());
                        self.introduce_name(&name, &ty, sentence.span);
                        statements.push(AstStmt::Let {
                            name,
                            ty: Some(ty),
                            init: value,
                            span: self.span(sentence.span),
                        });
                    }
                }
                index += 1;
                continue;
            }
            if let RawItem::Sentence(sentence) = &items[index]
                && let Some(seed) = parse_sequence_seed(&sentence.text)
                && let Some(RawItem::Sentence(generation_sentence)) = items.get(index + 1)
                && let Some(generation) = parse_sequence_generation(&generation_sentence.text)
                && let Some(RawItem::Sentence(iteration_sentence)) = items.get(index + 2)
                && let Some(iteration) = parse_sequence_iteration(&iteration_sentence.text)
            {
                let mut body = Vec::new();
                let mut cursor = index + 3;
                let mut last_line = raw_item_end_line(&items[index + 2]);
                let mut closer = None;
                while cursor < items.len() {
                    let start_line = raw_item_start_line(&items[cursor]);
                    if start_line > last_line.saturating_add(1) {
                        break;
                    }
                    if let RawItem::Sentence(next) = &items[cursor] {
                        let closer_text = if next.fillers.is_empty() {
                            next.text.clone()
                        } else {
                            format!("{}, {}", next.fillers.join(" "), next.text)
                        };
                        if let Some(rest) = post_loop_rest(&closer_text) {
                            closer = Some((rest, next.span));
                            cursor += 1;
                            break;
                        }
                    }
                    body.push(items[cursor].clone());
                    last_line = raw_item_end_line(&items[cursor]);
                    cursor += 1;
                }
                let lowered = self.lower_sequence(
                    seed,
                    generation,
                    iteration,
                    body,
                    closer,
                    sentence.span,
                    depth + 1,
                )?;
                statements.extend(lowered);
                index = cursor;
                continue;
            }
            if let RawItem::Sentence(sentence) = &items[index]
                && let Some(intro) = parse_loop_introduction(&sentence.text)
            {
                let mut body = Vec::new();
                let mut cursor = index + 1;
                let mut last_line = raw_item_end_line(&items[index]);
                let mut closer = None;
                while cursor < items.len() {
                    let start_line = raw_item_start_line(&items[cursor]);
                    if start_line > last_line.saturating_add(1) {
                        break;
                    }
                    if let RawItem::Sentence(next) = &items[cursor] {
                        let closer_text = if next.fillers.is_empty() {
                            next.text.clone()
                        } else {
                            format!("{}, {}", next.fillers.join(" "), next.text)
                        };
                        if let Some(rest) = post_loop_rest(&closer_text) {
                            closer = Some((rest, next.span));
                            cursor += 1;
                            break;
                        }
                        if let Some(rest) = continuation_rest(&next.text, &intro.quantifier.var) {
                            body.push(RawItem::Sentence(RawSentence {
                                text: rest.clone(),
                                raw: rest,
                                span: next.span,
                                fillers: Vec::new(),
                            }));
                            last_line = raw_item_end_line(&items[cursor]);
                            cursor += 1;
                            continue;
                        }
                    }
                    body.push(items[cursor].clone());
                    last_line = raw_item_end_line(&items[cursor]);
                    cursor += 1;
                }
                let mut lowered = self.ensure_implicit_counter(sentence.span)?;
                let (body, move_floor) = prepare_loop_rules(body);
                let (body, tracker_spec) = extract_largest_trackers(body);
                if let Some(spec) = tracker_spec {
                    lowered.extend(self.ensure_largest_tracker(spec, sentence.span));
                }
                let previous_print_tracker = self.print_tracker.clone();
                if let Some(tracker) = self.largest_tracker.clone() {
                    self.print_tracker = Some(tracker);
                }
                let previous_move_floor = self.move_floor.clone();
                if let Some(floor) = move_floor {
                    self.move_floor = Some(floor);
                }
                let var = intro.quantifier.var.clone();
                let mut body = body;
                let stopping_number = if closer
                    .as_ref()
                    .is_some_and(|(rest, _)| closer_needs_last_number(rest))
                {
                    let ty = self.loop_element_type(&intro.quantifier, sentence.span)?;
                    let name = self.next_temp("last_number");
                    lowered.push(self.declare_local(&name, ty, sentence.span));
                    body.insert(
                        0,
                        RawItem::Sentence(RawSentence {
                            text: format!("Set {name} to {var}"),
                            raw: String::new(),
                            span: sentence.span,
                            fillers: Vec::new(),
                        }),
                    );
                    Some(name)
                } else {
                    None
                };
                let body = apply_loop_filters(body, &var);
                let loop_result = self.lower_global_quantifier(
                    intro.quantifier,
                    Some(&body),
                    sentence.span,
                    depth + 1,
                );
                self.print_tracker = previous_print_tracker;
                self.move_floor = previous_move_floor;
                self.last_iteration_var = stopping_number;
                lowered.append(&mut loop_result?);
                if let Some((rest, closer_span)) = closer {
                    lowered.extend(self.lower_core(&rest, closer_span, None, None, depth + 1)?);
                }
                statements.extend(lowered);
                index = cursor;
                continue;
            }
            match &items[index] {
                RawItem::Sentence(sentence) => {
                    if sentence.text.to_ascii_lowercase().starts_with("phrase:") {
                        index += 1;
                        continue;
                    }
                    let effective_sentence = RawSentence {
                        text: crate::normalize::sentence(&sentence.text),
                        raw: sentence.raw.clone(),
                        span: sentence.span,
                        fillers: sentence.fillers.clone(),
                    };
                    let sentence = &effective_sentence;
                    let sentence_lower = sentence.text.to_ascii_lowercase();
                    let first_token = first_word(&sentence_lower);
                    if first_token == "otherwise" || first_token == "else" {
                        let keyword = if first_token == "otherwise" {
                            "otherwise"
                        } else {
                            "else"
                        };
                        let body = strip_leading_word(&sentence.text, keyword)
                            .trim()
                            .trim_start_matches(',')
                            .trim();
                        if body.is_empty() {
                            self.errors.push(self.error(
                                sentence.span,
                                SemanticErrorKind::Lowering,
                                "`Otherwise` requires an inline statement",
                            ));
                            index += 1;
                            continue;
                        }
                        let clause = analyze_clause(self.parser, sentence);
                        let verdict = IntentVerdict::Exec;
                        let _ = self.trace_sentence(sentence, &clause, None, verdict);
                        self.push_paraphrase(
                            sentence,
                            IntentVerdict::Exec,
                            canonicalize_statement(body),
                        );
                        self.push_scope(ScopeKind::IfElse);
                        let lowered = self.lower_actions_capturing(body, sentence.span, depth + 1);
                        self.pop_scope();
                        match lowered {
                            Ok((else_body, _)) => {
                                let target =
                                    statements.iter_mut().rev().find_map(
                                        |statement| match statement {
                                            AstStmt::If { else_body, .. } => Some(else_body),
                                            _ => None,
                                        },
                                    );
                                if let Some(existing) = target {
                                    existing.extend(else_body);
                                } else {
                                    self.errors.push(self.error(
                                        sentence.span,
                                        SemanticErrorKind::Lowering,
                                        "`Otherwise`/`Else` must follow an `If` statement",
                                    ));
                                }
                            }
                            Err(error) => self.errors.push(error),
                        }
                        index += 1;
                        continue;
                    }
                    let clause = analyze_clause(self.parser, sentence);
                    let phrase = self
                        .match_phrase_typed(&sentence.text, sentence.span)?
                        .filter(|phrase| !self.emit_phrase_defers(phrase, &sentence.text));
                    let graph = if phrase.is_none() {
                        clause_graph::decompose(&sentence.text)
                    } else {
                        None
                    };
                    let base = classify(&sentence.text, &clause, phrase.as_ref());
                    let mut verdict = match self.discourse_intent_override(sentence, &clause, base)
                    {
                        Ok(verdict) => verdict,
                        Err(error) => {
                            self.errors.push(error);
                            index += 1;
                            continue;
                        }
                    };
                    let first_text = tokenize(&sentence.text)
                        .into_iter()
                        .find(|token| token.is_word())
                        .map(|token| token.lower)
                        .unwrap_or_default();
                    let narrative_pronoun = matches!(
                        first_text.as_str(),
                        "i" | "we" | "he" | "she" | "it" | "they" | "you" | "my" | "our" | "their"
                    );
                    if graph.is_some()
                        && !(narrative_pronoun
                            && clause.voice == tc_syntax::Voice::Active
                            && clause.verb.as_ref().is_some_and(|verb| verb.is_past))
                        && !verdict.is_exec()
                    {
                        verdict = IntentVerdict::Exec;
                    }
                    let _ =
                        self.trace_sentence(sentence, &clause, phrase.as_ref(), verdict.clone());
                    if verdict.is_exec() {
                        let canonical = canonicalize_statement(&sentence.text);
                        self.push_paraphrase(sentence, verdict.clone(), canonical);
                        if let Some(phrase) = phrase {
                            self.add_annotation(format!("[phrase: \"{}\"]", phrase.pattern));
                            match self.lower_phrase_call(&phrase, sentence.span) {
                                Ok(statement) => {
                                    statements.push(statement);
                                    if let Some(update) =
                                        self.track_phrase_emit(&phrase, sentence.span)
                                    {
                                        statements.push(update);
                                    }
                                }
                                Err(error) => self.errors.push(error),
                            }
                        } else if let Some(graph) = graph {
                            match self.lower_clause_graph(
                                graph,
                                sentence.span,
                                depth + 1,
                                None,
                                None,
                            ) {
                                Ok(mut lowered) => statements.append(&mut lowered),
                                Err(error) => self.errors.push(error),
                            }
                        } else {
                            match self.lower_stmt(sentence, None, None, depth + 1) {
                                Ok(mut lowered) => statements.append(&mut lowered),
                                Err(error) => self.errors.push(error),
                            }
                        }
                    } else {
                        self.push_paraphrase(sentence, verdict.clone(), String::new());
                    }
                    index += 1;
                }
                RawItem::Block(block) => {
                    let Some(header) = &block.header else {
                        index += 1;
                        continue;
                    };
                    if header.text.to_ascii_lowercase().starts_with("phrase:") {
                        index += 1;
                        continue;
                    }
                    let effective_header = RawSentence {
                        text: crate::normalize::sentence(&header.text),
                        raw: header.raw.clone(),
                        span: header.span,
                        fillers: header.fillers.clone(),
                    };
                    let header = &effective_header;
                    let clause = analyze_clause(self.parser, header);
                    let phrase = self
                        .match_phrase_typed(&header.text, header.span)?
                        .filter(|phrase| !self.emit_phrase_defers(phrase, &header.text));
                    let base = classify(&header.text, &clause, phrase.as_ref());
                    let verdict = match self.discourse_intent_override(header, &clause, base) {
                        Ok(verdict) => verdict,
                        Err(error) => {
                            self.errors.push(error);
                            index += 1;
                            continue;
                        }
                    };
                    let _ = self.trace_sentence(header, &clause, phrase.as_ref(), verdict.clone());
                    if !verdict.is_exec() {
                        self.push_paraphrase(header, verdict.clone(), String::new());
                        index += 1;
                        continue;
                    }
                    self.push_paraphrase(
                        header,
                        verdict.clone(),
                        canonicalize_statement(&header.text),
                    );
                    let lower = header.text.to_ascii_lowercase();
                    let mut else_items = None;
                    if starts_with_word(&lower, "if") {
                        if let Some(RawItem::Block(next)) = items.get(index + 1) {
                            if let Some(next_header) = &next.header {
                                let next_lower = next_header.text.to_ascii_lowercase();
                                if starts_with_word(&next_lower, "else")
                                    || starts_with_word(&next_lower, "otherwise")
                                {
                                    let next_clause = analyze_clause(self.parser, next_header);
                                    let next_verdict =
                                        classify(&next_header.text, &next_clause, None);
                                    let _ = self.trace_sentence(
                                        next_header,
                                        &next_clause,
                                        None,
                                        next_verdict,
                                    );
                                    else_items = Some(next.items.as_slice());
                                    index += 1;
                                }
                            }
                        }
                    }
                    let is_control = ["if", "unless", "while", "repeat", "loop", "for"]
                        .iter()
                        .any(|keyword| starts_with_word(&lower, keyword));
                    if !is_control {
                        return Err(self.error(
                            header.span,
                            SemanticErrorKind::Lowering,
                            format!("unsupported block header `{}`", header.text),
                        ));
                    }
                    let is_loop = ["while", "until", "for", "repeat", "loop"]
                        .iter()
                        .any(|keyword| starts_with_word(&lower, keyword));
                    if is_loop && block.items.is_empty() {
                        self.errors.push(self.error(
                            header.span,
                            SemanticErrorKind::Lowering,
                            format!(
                                "empty loop body after '{}' — place the loop statements directly below with no blank line, or indent them",
                                header.text
                            ),
                        ));
                        index += 1;
                        continue;
                    }
                    match self.lower_stmt(header, Some(&block.items), else_items, depth + 1) {
                        Ok(mut lowered) => statements.append(&mut lowered),
                        Err(error) => self.errors.push(error),
                    }
                    index += 1;
                }
            }
        }
        Ok(statements)
    }

    pub(super) fn contains_anaphor_tokens(&self, tokens: &[Token]) -> bool {
        let mut index = 0;
        while index < tokens.len() {
            let token = &tokens[index];
            if token.kind == TokenKind::Word {
                if token.is("it") || token.is("its") {
                    return true;
                }
                if token.is("the") {
                    if let Some(next) = tokens.get(index + 1) {
                        if next.is_word() && self.is_declared_type(&next.lower) {
                            return true;
                        }
                    }
                }
            }
            index += 1;
        }
        false
    }

    pub(super) fn rhs_is_pure_expression(&self, text: &str) -> bool {
        let tokens = tokenize(text);
        if tokens.is_empty() {
            return false;
        }
        let mut has_value = false;
        let mut index = 0;
        while index < tokens.len() {
            let token = &tokens[index];
            match token.kind {
                TokenKind::Number | TokenKind::String => has_value = true,
                TokenKind::Word => {
                    let allowed = matches!(
                        token.lower.as_str(),
                        "it" | "its"
                            | "true"
                            | "false"
                            | "null"
                            | "as"
                            | "function"
                            | "size"
                            | "align"
                            | "of"
                            | "address"
                            | "deref"
                            | "bitnot"
                            | "twice"
                            | "half"
                            | "square"
                            | "sum"
                            | "product"
                            | "difference"
                            | "between"
                            | "and"
                            | "or"
                            | "not"
                            | "plus"
                            | "minus"
                            | "times"
                            | "over"
                            | "mod"
                            | "percent"
                            | "per"
                            | "cent"
                            | "multiplied"
                            | "divided"
                            | "by"
                            | "bytes_ptr"
                            | "bytes_len"
                            | "syscall"
                            | "bitand"
                            | "bitor"
                            | "bitxor"
                            | "shl"
                            | "shr"
                            | "i8"
                            | "i16"
                            | "i32"
                            | "i64"
                            | "u8"
                            | "u16"
                            | "u32"
                            | "u64"
                            | "f32"
                            | "f64"
                            | "bool"
                            | "ptr"
                            | "fn"
                            | "array"
                    );
                    let is_variable = self.lookup(&token.lower).is_some()
                        || self.globals.contains_key(&token.lower);
                    let is_call = tokens.get(index + 1).is_some_and(|next| next.text == "(");
                    if !allowed && !is_variable && !is_call {
                        return false;
                    }
                    if is_variable || is_call || matches!(token.lower.as_str(), "true" | "false") {
                        has_value = true;
                    }
                }
                TokenKind::Symbol => {
                    if !matches!(
                        token.text.as_str(),
                        "(" | ")"
                            | "["
                            | "]"
                            | "."
                            | "'s"
                            | ","
                            | "+"
                            | "-"
                            | "*"
                            | "/"
                            | "%"
                            | "<"
                            | ">"
                            | "<="
                            | ">="
                            | "=="
                            | "!="
                            | "&&"
                            | "||"
                            | "&"
                            | "!"
                            | "->"
                    ) {
                        return false;
                    }
                }
            }
            index += 1;
        }
        has_value
    }

    pub(super) fn is_create_declaration(&self, tokens: &[Token]) -> bool {
        let words = tokens
            .iter()
            .filter(|token| token.is_word())
            .collect::<Vec<_>>();
        if words.len() < 4 {
            return false;
        }
        let mut index = 1;
        if words[index].is("a") || words[index].is("an") {
            index += 1;
        }
        if index >= words.len() {
            return false;
        }
        let type_word = &words[index].lower;
        let known_type = self.types.contains(type_word) || parse_type_text(type_word).is_ok();
        if !known_type {
            return false;
        }
        index += 1;
        if index < words.len() && (words[index].is("called") || words[index].is("named")) {
            index += 1;
        }
        index < words.len()
    }

    pub(super) fn discourse_intent_override(
        &mut self,
        sentence: &RawSentence,
        clause: &ClauseTree,
        base: IntentVerdict,
    ) -> Result<IntentVerdict, SemanticError> {
        let text = sentence.text.trim();
        let tokens = tokenize(text);
        let first = tokens
            .first()
            .filter(|token| token.is_word())
            .map(|token| token.lower.clone())
            .unwrap_or_default();
        if !base.is_exec() && self.is_natural_list_operation(text) {
            return Ok(IntentVerdict::Exec);
        }
        if base.is_exec() {
            if matches!(first.as_str(), "that" | "this" | "there") {
                let second = tokens
                    .get(1)
                    .map(|token| token.lower.clone())
                    .unwrap_or_default();
                if matches!(
                    second.as_str(),
                    "is" | "was" | "were" | "has" | "have" | "had" | "seems" | "looks"
                ) {
                    let rhs = text[tokens[1].end..].trim();
                    if !self.rhs_is_pure_expression(rhs) {
                        return Ok(IntentVerdict::CommentOmitted {
                            reason: "demonstrative prose without computational directive"
                                .to_string(),
                        });
                    }
                }
            }
            return Ok(base);
        }
        if classify_verb(&first, None)
            .is_some_and(|resolution| resolution.predicate == Some(VerbPredicate::Create))
            && self.is_create_declaration(&tokens)
        {
            return Ok(IntentVerdict::Exec);
        }
        if first == "it" {
            let second = tokens
                .get(1)
                .map(|token| token.lower.clone())
                .unwrap_or_default();
            if matches!(
                second.as_str(),
                "was"
                    | "were"
                    | "had"
                    | "been"
                    | "seemed"
                    | "looked"
                    | "might"
                    | "could"
                    | "would"
                    | "should"
                    | "may"
                    | "must"
                    | "has"
                    | "have"
            ) {
                return Ok(IntentVerdict::CommentOmitted {
                    reason: "expletive prose without computational directive".to_string(),
                });
            }
            let negation = tokens.get(1).is_some_and(|token| token.is("is"))
                && tokens.get(2).is_some_and(|token| token.is("false"))
                && tokens.get(3).is_some_and(|token| token.is("that"));
            if negation {
                return Ok(IntentVerdict::Exec);
            }
            if second == "is" {
                if self.discourse.resolve_it(sentence.span).is_err() {
                    return Ok(base);
                }
                let rhs = text[tokens[1].end..].trim();
                if !self.rhs_is_pure_expression(rhs) {
                    return Ok(IntentVerdict::CommentOmitted {
                        reason: "expletive prose without computational directive".to_string(),
                    });
                }
                return Ok(IntentVerdict::Exec);
            }
            if matches!(
                second.as_str(),
                "becomes" | "equals" | "=" | "+=" | "-=" | "*=" | "/="
            ) {
                if self.discourse.resolve_it(sentence.span).is_ok() {
                    return Ok(IntentVerdict::Exec);
                }
                return Ok(base);
            }
            if (second == "goes" || second == "go")
                && self.discourse.resolve_it(sentence.span).is_ok()
            {
                return Ok(IntentVerdict::Exec);
            }
            return Ok(base);
        }
        if matches!(first.as_str(), "that" | "this" | "there") {
            let second = tokens
                .get(1)
                .map(|token| token.lower.clone())
                .unwrap_or_default();
            if matches!(
                second.as_str(),
                "is" | "was" | "were" | "has" | "have" | "had" | "seems" | "looks"
            ) {
                let rhs = text[tokens[1].end..].trim();
                if !self.rhs_is_pure_expression(rhs) {
                    return Ok(IntentVerdict::CommentOmitted {
                        reason: "demonstrative prose without computational directive".to_string(),
                    });
                }
            }
        }
        for keyword in ["is", "becomes", "equals"] {
            if let Some((start, end)) = find_top_level_word(text, keyword) {
                let left = canonical_assignment_target(text[..start].trim());
                let right = text[end..].trim();
                if is_lvalue_text(&left)
                    && !right.is_empty()
                    && (crate::intent::is_call_expression(right)
                        || self.match_phrase_typed(right, sentence.span)?.is_some())
                {
                    return Ok(IntentVerdict::Exec);
                }
            }
        }
        if let Some((start, end)) = find_top_level_symbol(text, "=") {
            let left = text[..start].trim();
            let right = text[end..].trim();
            let left = canonical_assignment_target(left);
            if is_lvalue_text(&left)
                && !right.is_empty()
                && (crate::intent::is_call_expression(right)
                    || self.match_phrase_typed(right, sentence.span)?.is_some())
            {
                return Ok(IntentVerdict::Exec);
            }
        }
        if first == "its" {
            return Ok(IntentVerdict::Exec);
        }
        if first == "the"
            && tokens
                .get(1)
                .is_some_and(|token| token.is_word() && self.is_declared_type(&token.lower))
        {
            return Ok(IntentVerdict::Exec);
        }
        if base.is_unsupported() || self.contains_anaphor_tokens(&tokens) {
            let Ok(resolved) = self.resolve_anaphors(text, sentence.span) else {
                return Ok(base);
            };
            let resolved_sentence = RawSentence {
                text: resolved.clone(),
                raw: String::new(),
                span: sentence.span,
                fillers: Vec::new(),
            };
            let resolved_clause = analyze_clause(self.parser, &resolved_sentence);
            let verdict = classify(&resolved, &resolved_clause, None);
            if verdict.is_exec() {
                return Ok(IntentVerdict::Exec);
            }
        }
        let _ = clause;
        Ok(base)
    }

    pub(super) fn lower_repeat_count(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<AstExpr, SemanticError> {
        let rest = strip_leading_word(text, "repeat");
        let count_text = if let Some((start, _)) = find_top_level_word(rest, "times") {
            rest[..start].trim()
        } else {
            rest
        };
        self.parse_expr(count_text, span)
    }

    pub(super) fn is_integer_type(ty: &AstType) -> bool {
        matches!(
            ty,
            AstType::I8
                | AstType::I16
                | AstType::I32
                | AstType::I64
                | AstType::U8
                | AstType::U16
                | AstType::U32
                | AstType::U64
        )
    }

    pub(super) fn constant_i64(expr: &AstExpr) -> Option<i64> {
        match expr {
            AstExpr::ConstInt(value, _) => Some(*value),
            AstExpr::ConstUint(value, _) => i64::try_from(*value).ok(),
            _ => None,
        }
    }

    pub(super) fn lower_condition(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<LoweredCond, SemanticError> {
        let resolved = self.replace_value_referent_phrases(text, span)?;
        if let Some(total) = self.running_total.clone()
            && resolved.contains(&total)
        {
            self.touch_name(&total, span);
        }
        let (condition, where_clause) = split_where(&resolved);
        if where_clause.is_some() {
            return Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                "`where` bindings on a control header must be lowered by the enclosing statement",
            ));
        }
        let condition = condition
            .strip_prefix(',')
            .map_or(condition.as_str(), str::trim);
        if let Some((left, operator, right)) = split_condition_comparison(condition)
            && crate::value::is_np_expression(&left)
        {
            let lhs = self.parse_value(&left, span)?;
            let rhs = self.parse_value(&right, span)?;
            let expr = AstExpr::Binary {
                op: operator,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
            let paraphrase = strip_outer_parentheses(&render_expr(&expr));
            return Ok(LoweredCond {
                prelude: Vec::new(),
                expr,
                paraphrase,
            });
        }
        let parsed = parse_condition(condition)
            .map_err(|message| self.error(span, SemanticErrorKind::Lowering, message))?;
        self.lower_cond_ast(&parsed, span)
    }

    #[allow(clippy::single_match_else)]
    pub(super) fn lower_check_whether(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<Vec<AstStmt>>, SemanticError> {
        let Some(spec) = parse_check(text) else {
            return Ok(None);
        };
        let matched = self.next_temp("check_matched");
        self.declare(&matched, AstType::Bool);
        let initial = matches!(spec.kind, CheckKind::Every | CheckKind::No);
        let mut statements = vec![AstStmt::Let {
            name: matched.clone(),
            ty: Some(AstType::Bool),
            init: AstExpr::ConstBool(initial),
            span: self.span(span),
        }];
        let inner = self.next_temp(&format!("check_{}", spec.noun));
        match &spec.domain {
            CheckDomain::List { name } => {
                let Some(ty) = self.lookup(name).cloned() else {
                    return Err(self.error(
                        span,
                        SemanticErrorKind::Lowering,
                        format!("check list `{name}` is not in scope"),
                    ));
                };
                let (element, data) = match ty {
                    AstType::Named {
                        name: list_name,
                        type_args,
                        ..
                    } if list_name.eq_ignore_ascii_case("list") => (
                        type_args.first().cloned().unwrap_or(AstType::I64),
                        self.parse_expr(&format!("{name}'s data"), span)?,
                    ),
                    AstType::Ptr(inner)
                        if matches!(
                            inner.as_ref(),
                            AstType::Named { name: list_name, .. }
                                if list_name.eq_ignore_ascii_case("list")
                        ) =>
                    {
                        let AstType::Named { type_args, .. } = inner.as_ref() else {
                            unreachable!()
                        };
                        (
                            type_args.first().cloned().unwrap_or(AstType::I64),
                            self.parse_expr(&format!("{name}'s data"), span)?,
                        )
                    }
                    AstType::Array(element, _) => (
                        *element,
                        self.parse_expr(&format!("address of item 0 of {name}"), span)?,
                    ),
                    _ => {
                        return Err(self.error(
                            span,
                            SemanticErrorKind::Lowering,
                            format!("check domain `{name}` is not a list or array"),
                        ));
                    }
                };
                let index = self.next_temp("check_index");
                let length = self.next_temp("check_len");
                let length_expr = if matches!(self.lookup(name), Some(AstType::Array(_, _))) {
                    AstExpr::ConstInt(
                        match self.lookup(name) {
                            Some(AstType::Array(_, count)) => {
                                i64::try_from(*count).unwrap_or(i64::MAX)
                            }
                            _ => 0,
                        },
                        None,
                    )
                } else {
                    self.parse_expr(&format!("{name}'s len"), span)?
                };
                statements.push(AstStmt::Let {
                    name: index.clone(),
                    ty: Some(AstType::U64),
                    init: AstExpr::ConstUint(0, None),
                    span: self.span(span),
                });
                statements.push(AstStmt::Let {
                    name: length.clone(),
                    ty: None,
                    init: length_expr,
                    span: self.span(span),
                });
                let (predicate_prelude, predicate) = self.check_predicate(&spec, &inner, span)?;
                let mut body = vec![AstStmt::If {
                    cond: AstExpr::Binary {
                        op: tc_ir::BinOp::Ge,
                        lhs: Box::new(AstExpr::Var(index.clone())),
                        rhs: Box::new(AstExpr::Var(length.clone())),
                    },
                    then_body: vec![AstStmt::Break(self.span(span))],
                    else_body: Vec::new(),
                    span: self.span(span),
                }];
                body.push(AstStmt::Let {
                    name: inner.clone(),
                    ty: Some(element),
                    init: AstExpr::Load(Box::new(AstExpr::IndexAddr {
                        base_ptr: Box::new(data),
                        index: Box::new(AstExpr::Var(index.clone())),
                    })),
                    span: self.span(span),
                });
                body.extend(predicate_prelude);
                body.push(self.check_branch(spec.kind, predicate, &matched, span));
                body.push(AstStmt::Assign {
                    name: index.clone(),
                    val: AstExpr::Binary {
                        op: tc_ir::BinOp::Add,
                        lhs: Box::new(AstExpr::Var(index)),
                        rhs: Box::new(AstExpr::ConstInt(1, None)),
                    },
                    span: self.span(span),
                });
                statements.push(AstStmt::Loop {
                    body,
                    span: self.span(span),
                });
            }
            _ => {
                let (start, end, var_ty) = match &spec.domain {
                    CheckDomain::Range { start, end } => {
                        let start = self.parse_expr(start, span)?;
                        let var_ty = self.shallow_type(&start);
                        (start, self.parse_expr(end, span)?, var_ty)
                    }
                    CheckDomain::Smaller { start } => {
                        let outer = self.active_loop_var.clone().ok_or_else(|| {
                            self.error(
                                span,
                                SemanticErrorKind::Lowering,
                                "`smaller` check requires an active discourse loop",
                            )
                        })?;
                        let start = self.parse_expr(start, span)?;
                        (
                            start.clone(),
                            AstExpr::Binary {
                                op: tc_ir::BinOp::Sub,
                                lhs: Box::new(AstExpr::Var(outer)),
                                rhs: Box::new(AstExpr::ConstInt(1, None)),
                            },
                            self.shallow_type(&start),
                        )
                    }
                    CheckDomain::Larger { end } => {
                        let outer = self.active_loop_var.clone().ok_or_else(|| {
                            self.error(
                                span,
                                SemanticErrorKind::Lowering,
                                "`larger` check requires an active discourse loop",
                            )
                        })?;
                        (
                            AstExpr::Binary {
                                op: tc_ir::BinOp::Add,
                                lhs: Box::new(AstExpr::Var(outer)),
                                rhs: Box::new(AstExpr::ConstInt(1, None)),
                            },
                            self.parse_expr(end, span)?,
                            AstType::I64,
                        )
                    }
                    CheckDomain::List { .. } => unreachable!(),
                };
                let current = self.next_temp("check_current");
                let limit = self.next_temp("check_limit");
                statements.push(AstStmt::Let {
                    name: current.clone(),
                    ty: Some(var_ty.clone()),
                    init: start,
                    span: self.span(span),
                });
                statements.push(AstStmt::Let {
                    name: limit.clone(),
                    ty: None,
                    init: end,
                    span: self.span(span),
                });
                let (predicate_prelude, predicate) = self.check_predicate(&spec, &inner, span)?;
                let mut body = vec![
                    AstStmt::If {
                        cond: AstExpr::Binary {
                            op: tc_ir::BinOp::Gt,
                            lhs: Box::new(AstExpr::Var(current.clone())),
                            rhs: Box::new(AstExpr::Var(limit.clone())),
                        },
                        then_body: vec![AstStmt::Break(self.span(span))],
                        else_body: Vec::new(),
                        span: self.span(span),
                    },
                    AstStmt::Let {
                        name: inner.clone(),
                        ty: Some(var_ty),
                        init: AstExpr::Var(current.clone()),
                        span: self.span(span),
                    },
                ];
                body.extend(predicate_prelude);
                body.push(self.check_branch(spec.kind, predicate, &matched, span));
                body.push(AstStmt::Assign {
                    name: current.clone(),
                    val: AstExpr::Binary {
                        op: tc_ir::BinOp::Add,
                        lhs: Box::new(AstExpr::Var(current)),
                        rhs: Box::new(AstExpr::ConstInt(1, None)),
                    },
                    span: self.span(span),
                });
                statements.push(AstStmt::Loop {
                    body,
                    span: self.span(span),
                });
            }
        }
        self.last_check = Some(CheckRecord {
            matched_var: matched,
            kind: spec.kind,
        });
        Ok(Some(statements))
    }

    pub(super) fn check_predicate(
        &mut self,
        spec: &crate::discourse_scope::CheckSpec,
        inner: &str,
        span: Span,
    ) -> Result<(Vec<AstStmt>, AstExpr), SemanticError> {
        let predicate = spec.predicate.trim();
        if starts_with_word(predicate, "divides") {
            let tokens = tokenize(predicate);
            let start = tokens.first().map_or(predicate.len(), |token| token.end);
            let rest = predicate[start..].trim();
            let rest = strip_leading_word(rest, "into").trim();
            let rest = rest.trim_end_matches("evenly").trim();
            let target = rest.trim();
            let target_value = if matches!(target.to_ascii_lowercase().as_str(), "it" | "them") {
                let outer = self.active_loop_var.clone().ok_or_else(|| {
                    self.error(
                        span,
                        SemanticErrorKind::Lowering,
                        "`divides into it` requires an active discourse loop",
                    )
                })?;
                AstExpr::Var(outer)
            } else {
                self.parse_value(target, span)?
            };
            let modulo = AstExpr::Binary {
                op: tc_ir::BinOp::Mod,
                lhs: Box::new(target_value),
                rhs: Box::new(AstExpr::Var(inner.to_string())),
            };
            return Ok((
                Vec::new(),
                AstExpr::Binary {
                    op: tc_ir::BinOp::Eq,
                    lhs: Box::new(modulo),
                    rhs: Box::new(AstExpr::ConstInt(0, None)),
                },
            ));
        }
        let text = format!("{inner} {predicate}");
        let condition = self.lower_condition(&text, span)?;
        Ok((condition.prelude, condition.expr))
    }

    pub(super) fn check_branch(
        &self,
        kind: CheckKind,
        predicate: AstExpr,
        matched: &str,
        span: Span,
    ) -> AstStmt {
        let (condition, value) = match kind {
            CheckKind::Any => (predicate, true),
            CheckKind::Every | CheckKind::No => (
                AstExpr::Unary {
                    op: tc_ir::UnOp::LogNot,
                    val: Box::new(predicate),
                },
                false,
            ),
        };
        let condition = if matches!(kind, CheckKind::No) {
            AstExpr::Unary {
                op: tc_ir::UnOp::LogNot,
                val: Box::new(condition),
            }
        } else {
            condition
        };
        AstStmt::If {
            cond: condition,
            then_body: vec![
                AstStmt::Assign {
                    name: matched.to_string(),
                    val: AstExpr::ConstBool(value),
                    span: self.span(span),
                },
                AstStmt::Break(self.span(span)),
            ],
            else_body: Vec::new(),
            span: self.span(span),
        }
    }

    pub(super) fn check_ellipsis(
        &self,
        text: &str,
        span: Span,
    ) -> Result<Option<AstExpr>, SemanticError> {
        let lower = text
            .trim()
            .trim_end_matches(['.', ',', ':'])
            .trim()
            .to_ascii_lowercase();
        let negative = lower == "none do"
            || lower.starts_with("none of them do")
            || lower.starts_with("neither of them does")
            || lower.starts_with("no such ")
            || lower.ends_with("does not")
            || lower.ends_with("do not");
        let positive = lower.starts_with("any of them do")
            || lower.starts_with("one of them does")
            || lower.starts_with("some of them do")
            || lower.starts_with("all of them do")
            || lower == "it does"
            || lower == "they do"
            || lower == "it do";
        if !negative && !positive {
            return Ok(None);
        }
        let Some(check) = &self.last_check else {
            return Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                "UNRESOLVED_ANAPHOR: ellipsis `none/any of them do` has no preceding `check whether`",
            ));
        };
        let base = AstExpr::Var(check.matched_var.clone());
        let expression = match check.kind {
            CheckKind::No => {
                if negative {
                    base
                } else {
                    AstExpr::Unary {
                        op: tc_ir::UnOp::LogNot,
                        val: Box::new(base),
                    }
                }
            }
            CheckKind::Any | CheckKind::Every => {
                if negative {
                    AstExpr::Unary {
                        op: tc_ir::UnOp::LogNot,
                        val: Box::new(base),
                    }
                } else {
                    base
                }
            }
        };
        Ok(Some(expression))
    }

    pub(super) fn lower_cond_ast(
        &mut self,
        node: &CondAst,
        span: Span,
    ) -> Result<LoweredCond, SemanticError> {
        match node {
            CondAst::Atom(text) => {
                if let Some(expr) = self.check_ellipsis(text, span)? {
                    let paraphrase = strip_outer_parentheses(&render_expr(&expr));
                    return Ok(LoweredCond {
                        prelude: Vec::new(),
                        expr,
                        paraphrase,
                    });
                }
                if let Some(condition) = self.parse_natural_membership(text, span)? {
                    return Ok(condition);
                }
                if let Some(expr) = self.parse_natural_predicate(text, span)? {
                    let paraphrase = strip_outer_parentheses(&render_expr(&expr));
                    return Ok(LoweredCond {
                        prelude: Vec::new(),
                        expr,
                        paraphrase,
                    });
                }
                if crate::value::is_np_expression(text)
                    && let Some((left, operator, right)) = split_condition_comparison(text)
                {
                    let lhs = self.parse_value(&left, span)?;
                    let rhs = self.parse_value(&right, span)?;
                    let expr = AstExpr::Binary {
                        op: operator,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    };
                    let paraphrase = strip_outer_parentheses(&render_expr(&expr));
                    return Ok(LoweredCond {
                        prelude: Vec::new(),
                        expr,
                        paraphrase,
                    });
                }
                let resolved = self.resolve_anaphors(text, span)?;
                let expr = self.parse_expr_raw(&resolved, span)?;
                let paraphrase = strip_outer_parentheses(&render_expr(&expr));
                Ok(LoweredCond {
                    prelude: Vec::new(),
                    expr,
                    paraphrase,
                })
            }
            CondAst::Not(inner) => {
                let inner = self.lower_cond_ast(inner, span)?;
                Ok(LoweredCond {
                    prelude: inner.prelude,
                    expr: AstExpr::Unary {
                        op: tc_ir::UnOp::LogNot,
                        val: Box::new(inner.expr),
                    },
                    paraphrase: format!("not {{{}}}", inner.paraphrase),
                })
            }
            CondAst::And(lhs, rhs) => self.lower_cond_pair(lhs, rhs, span, true),
            CondAst::Or(lhs, rhs) => self.lower_cond_pair(lhs, rhs, span, false),
            CondAst::Quantifier(spec) => self.lower_local_quantifier(spec, span),
        }
    }

    pub(super) fn lower_cond_pair(
        &mut self,
        lhs: &CondAst,
        rhs: &CondAst,
        span: Span,
        conjunction: bool,
    ) -> Result<LoweredCond, SemanticError> {
        let lhs = self.lower_cond_ast(lhs, span)?;
        let rhs = self.lower_cond_ast(rhs, span)?;
        let operator = if conjunction { "and" } else { "or" };
        let paraphrase = format!(
            "{{{0} {operator} {1}}}",
            render_cond_child_text(&lhs.paraphrase),
            render_cond_child_text(&rhs.paraphrase)
        );
        if rhs.prelude.is_empty() {
            return Ok(LoweredCond {
                prelude: lhs.prelude,
                expr: AstExpr::Binary {
                    op: if conjunction {
                        tc_ir::BinOp::LogAnd
                    } else {
                        tc_ir::BinOp::LogOr
                    },
                    lhs: Box::new(lhs.expr),
                    rhs: Box::new(rhs.expr),
                },
                paraphrase,
            });
        }
        // Preserve short-circuiting: the right quantifier prelude only runs
        // when the left operand does not already decide the result.
        let temp = self.next_temp("sc");
        self.declare_temp(&temp, AstType::Bool);
        let mut prelude = lhs.prelude;
        prelude.push(AstStmt::Let {
            name: temp.clone(),
            ty: Some(AstType::Bool),
            init: AstExpr::ConstBool(!conjunction),
            span: self.span(span),
        });
        let mut then_body = rhs.prelude;
        then_body.push(AstStmt::Assign {
            name: temp.clone(),
            val: rhs.expr,
            span: self.span(span),
        });
        let condition = if conjunction {
            lhs.expr
        } else {
            AstExpr::Unary {
                op: tc_ir::UnOp::LogNot,
                val: Box::new(lhs.expr),
            }
        };
        prelude.push(AstStmt::If {
            cond: condition,
            then_body,
            else_body: Vec::new(),
            span: self.span(span),
        });
        Ok(LoweredCond {
            prelude,
            expr: AstExpr::Var(temp),
            paraphrase,
        })
    }

    pub(super) fn lower_local_quantifier(
        &mut self,
        spec: &LocalQuantSpec,
        span: Span,
    ) -> Result<LoweredCond, SemanticError> {
        match &spec.domain {
            QuantDomainSpec::Range { start, end, step } => {
                self.lower_local_range(spec, start, end, step.as_deref(), span)
            }
            QuantDomainSpec::Array { arr } => self.lower_local_array(spec, arr, span),
        }
    }

    pub(super) fn lower_local_range(
        &mut self,
        spec: &LocalQuantSpec,
        start_text: &str,
        end_text: &str,
        step_text: Option<&str>,
        span: Span,
    ) -> Result<LoweredCond, SemanticError> {
        let start = self.parse_expr(start_text, span)?;
        let end = self.parse_expr(end_text, span)?;
        let step = if let Some(text) = step_text {
            self.parse_expr(text, span)?
        } else {
            AstExpr::ConstInt(1, None)
        };
        let start_ty = self.shallow_type(&start);
        if !Self::is_integer_type(&start_ty) {
            return Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                "range quantifier bounds must be integers",
            ));
        }
        if Self::constant_i64(&step) == Some(0) {
            return Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                "quantifier step cannot be 0",
            ));
        }
        let res = self.next_temp("q_res");
        let cur = self.next_temp("q_cur");
        let end_temp = self.next_temp("q_end");
        let step_temp = self.next_temp("q_step");
        let first = self.next_temp("q_first");
        self.declare_temp(&res, AstType::Bool);
        self.declare_temp(&cur, start_ty.clone());
        self.declare_temp(&end_temp, start_ty.clone());
        self.declare_temp(&step_temp, start_ty.clone());
        self.declare_temp(&first, AstType::Bool);
        let mut prelude = vec![
            AstStmt::Let {
                name: res.clone(),
                ty: Some(AstType::Bool),
                init: AstExpr::ConstBool(matches!(
                    spec.kind,
                    LocalQuantKind::Every | LocalQuantKind::No
                )),
                span: self.span(span),
            },
            AstStmt::Let {
                name: cur.clone(),
                ty: Some(start_ty.clone()),
                init: start,
                span: self.span(span),
            },
            AstStmt::Let {
                name: end_temp.clone(),
                ty: Some(start_ty.clone()),
                init: end,
                span: self.span(span),
            },
            AstStmt::Let {
                name: step_temp.clone(),
                ty: Some(start_ty.clone()),
                init: step.clone(),
                span: self.span(span),
            },
            AstStmt::Let {
                name: first.clone(),
                ty: Some(AstType::Bool),
                init: AstExpr::ConstBool(true),
                span: self.span(span),
            },
        ];
        if Self::constant_i64(&step).is_none() {
            prelude.push(AstStmt::If {
                cond: AstExpr::Binary {
                    op: tc_ir::BinOp::Eq,
                    lhs: Box::new(AstExpr::Var(step_temp.clone())),
                    rhs: Box::new(AstExpr::ConstInt(0, None)),
                },
                then_body: vec![AstStmt::Trap(self.span(span))],
                else_body: Vec::new(),
                span: self.span(span),
            });
        }
        self.push_scope(ScopeKind::QuantifierBody);
        self.declare(&spec.var, start_ty.clone());
        self.introduce_name(&spec.var, &start_ty, span);
        let predicate_text = match &spec.pred {
            PredSpec::Implicit(text) => format!("{} {text}", spec.var),
            PredSpec::Explicit(text) => text.clone(),
        };
        let parsed_predicate = parse_condition(&predicate_text)
            .map_err(|message| self.error(span, SemanticErrorKind::Lowering, message));
        let predicate = match parsed_predicate {
            Ok(value) => self.lower_cond_ast(&value, span),
            Err(error) => {
                self.pop_scope();
                return Err(error);
            }
        };
        self.pop_scope();
        let predicate = predicate?;
        let termination = Self::range_termination(&cur, &end_temp, &step_temp, &step);
        let mut loop_body = vec![
            AstStmt::If {
                cond: AstExpr::Unary {
                    op: tc_ir::UnOp::LogNot,
                    val: Box::new(AstExpr::Var(first.clone())),
                },
                then_body: vec![AstStmt::Assign {
                    name: cur.clone(),
                    val: AstExpr::Binary {
                        op: tc_ir::BinOp::Add,
                        lhs: Box::new(AstExpr::Var(cur.clone())),
                        rhs: Box::new(AstExpr::Var(step_temp.clone())),
                    },
                    span: self.span(span),
                }],
                else_body: Vec::new(),
                span: self.span(span),
            },
            AstStmt::Assign {
                name: first,
                val: AstExpr::ConstBool(false),
                span: self.span(span),
            },
            AstStmt::If {
                cond: termination,
                then_body: vec![AstStmt::Break(self.span(span))],
                else_body: Vec::new(),
                span: self.span(span),
            },
            AstStmt::Let {
                name: spec.var.clone(),
                ty: Some(start_ty),
                init: AstExpr::Var(cur),
                span: self.span(span),
            },
        ];
        loop_body.extend(predicate.prelude);
        let (check, result) = match spec.kind {
            LocalQuantKind::Every => (
                AstExpr::Unary {
                    op: tc_ir::UnOp::LogNot,
                    val: Box::new(predicate.expr),
                },
                false,
            ),
            LocalQuantKind::Some => (predicate.expr, true),
            LocalQuantKind::No => (predicate.expr, false),
        };
        loop_body.push(AstStmt::If {
            cond: check,
            then_body: vec![
                AstStmt::Assign {
                    name: res.clone(),
                    val: AstExpr::ConstBool(result),
                    span: self.span(span),
                },
                AstStmt::Break(self.span(span)),
            ],
            else_body: Vec::new(),
            span: self.span(span),
        });
        prelude.push(AstStmt::Loop {
            body: loop_body,
            span: self.span(span),
        });
        Ok(LoweredCond {
            prelude,
            expr: AstExpr::Var(res),
            paraphrase: spec.display.clone(),
        })
    }

    pub(super) fn range_termination(
        cur: &str,
        end: &str,
        step: &str,
        step_expr: &AstExpr,
    ) -> AstExpr {
        match Self::constant_i64(step_expr) {
            Some(value) if value > 0 => AstExpr::Binary {
                op: tc_ir::BinOp::Gt,
                lhs: Box::new(AstExpr::Var(cur.to_string())),
                rhs: Box::new(AstExpr::Var(end.to_string())),
            },
            Some(_) => AstExpr::Binary {
                op: tc_ir::BinOp::Lt,
                lhs: Box::new(AstExpr::Var(cur.to_string())),
                rhs: Box::new(AstExpr::Var(end.to_string())),
            },
            None => AstExpr::Binary {
                op: tc_ir::BinOp::LogOr,
                lhs: Box::new(AstExpr::Binary {
                    op: tc_ir::BinOp::LogAnd,
                    lhs: Box::new(AstExpr::Binary {
                        op: tc_ir::BinOp::Gt,
                        lhs: Box::new(AstExpr::Var(step.to_string())),
                        rhs: Box::new(AstExpr::ConstInt(0, None)),
                    }),
                    rhs: Box::new(AstExpr::Binary {
                        op: tc_ir::BinOp::Gt,
                        lhs: Box::new(AstExpr::Var(cur.to_string())),
                        rhs: Box::new(AstExpr::Var(end.to_string())),
                    }),
                }),
                rhs: Box::new(AstExpr::Binary {
                    op: tc_ir::BinOp::LogAnd,
                    lhs: Box::new(AstExpr::Binary {
                        op: tc_ir::BinOp::Lt,
                        lhs: Box::new(AstExpr::Var(step.to_string())),
                        rhs: Box::new(AstExpr::ConstInt(0, None)),
                    }),
                    rhs: Box::new(AstExpr::Binary {
                        op: tc_ir::BinOp::Lt,
                        lhs: Box::new(AstExpr::Var(cur.to_string())),
                        rhs: Box::new(AstExpr::Var(end.to_string())),
                    }),
                }),
            },
        }
    }

    pub(super) fn lower_local_array(
        &mut self,
        spec: &LocalQuantSpec,
        arr: &str,
        span: Span,
    ) -> Result<LoweredCond, SemanticError> {
        let Some(ty) = self.lookup(arr).cloned() else {
            return Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                format!("quantifier array `{arr}` is not in scope"),
            ));
        };
        let AstType::Array(element, len) = ty else {
            return Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                format!("quantifier domain `{arr}` is not a fixed-size array"),
            ));
        };
        let res = self.next_temp("q_res");
        let idx = self.next_temp("q_idx");
        let first = self.next_temp("q_first");
        self.declare_temp(&res, AstType::Bool);
        self.declare_temp(&idx, AstType::I64);
        self.declare_temp(&first, AstType::Bool);
        let base = if self.globals.contains_key(arr)
            && !self.scopes.iter().any(|scope| scope.vars.contains_key(arr))
        {
            AstExpr::GlobalAddr {
                module: None,
                name: arr.to_string(),
            }
        } else {
            AstExpr::AddrOfVar(arr.to_string())
        };
        let mut prelude = vec![
            AstStmt::Let {
                name: res.clone(),
                ty: Some(AstType::Bool),
                init: AstExpr::ConstBool(matches!(
                    spec.kind,
                    LocalQuantKind::Every | LocalQuantKind::No
                )),
                span: self.span(span),
            },
            AstStmt::Let {
                name: idx.clone(),
                ty: Some(AstType::I64),
                init: AstExpr::ConstInt(0, None),
                span: self.span(span),
            },
            AstStmt::Let {
                name: first.clone(),
                ty: Some(AstType::Bool),
                init: AstExpr::ConstBool(true),
                span: self.span(span),
            },
        ];
        self.push_scope(ScopeKind::QuantifierBody);
        self.declare(&spec.var, (*element).clone());
        self.introduce_name(&spec.var, &element, span);
        let predicate_text = match &spec.pred {
            PredSpec::Implicit(text) => format!("{} {text}", spec.var),
            PredSpec::Explicit(text) => text.clone(),
        };
        let predicate = match parse_condition(&predicate_text) {
            Ok(value) => self.lower_cond_ast(&value, span),
            Err(message) => Err(self.error(span, SemanticErrorKind::Lowering, message)),
        };
        self.pop_scope();
        let predicate = predicate?;
        let mut loop_body = vec![
            AstStmt::If {
                cond: AstExpr::Unary {
                    op: tc_ir::UnOp::LogNot,
                    val: Box::new(AstExpr::Var(first.clone())),
                },
                then_body: vec![AstStmt::Assign {
                    name: idx.clone(),
                    val: AstExpr::Binary {
                        op: tc_ir::BinOp::Add,
                        lhs: Box::new(AstExpr::Var(idx.clone())),
                        rhs: Box::new(AstExpr::ConstInt(1, None)),
                    },
                    span: self.span(span),
                }],
                else_body: Vec::new(),
                span: self.span(span),
            },
            AstStmt::Assign {
                name: first,
                val: AstExpr::ConstBool(false),
                span: self.span(span),
            },
            AstStmt::If {
                cond: AstExpr::Binary {
                    op: tc_ir::BinOp::Ge,
                    lhs: Box::new(AstExpr::Var(idx.clone())),
                    rhs: Box::new(AstExpr::ConstInt(
                        i64::try_from(len).unwrap_or(i64::MAX),
                        None,
                    )),
                },
                then_body: vec![AstStmt::Break(self.span(span))],
                else_body: Vec::new(),
                span: self.span(span),
            },
            AstStmt::Let {
                name: spec.var.clone(),
                ty: Some((*element).clone()),
                init: AstExpr::Load(Box::new(AstExpr::IndexAddr {
                    base_ptr: Box::new(base),
                    index: Box::new(AstExpr::Var(idx)),
                })),
                span: self.span(span),
            },
        ];
        loop_body.extend(predicate.prelude);
        let (check, result) = match spec.kind {
            LocalQuantKind::Every => (
                AstExpr::Unary {
                    op: tc_ir::UnOp::LogNot,
                    val: Box::new(predicate.expr),
                },
                false,
            ),
            LocalQuantKind::Some => (predicate.expr, true),
            LocalQuantKind::No => (predicate.expr, false),
        };
        loop_body.push(AstStmt::If {
            cond: check,
            then_body: vec![
                AstStmt::Assign {
                    name: res.clone(),
                    val: AstExpr::ConstBool(result),
                    span: self.span(span),
                },
                AstStmt::Break(self.span(span)),
            ],
            else_body: Vec::new(),
            span: self.span(span),
        });
        prelude.push(AstStmt::Loop {
            body: loop_body,
            span: self.span(span),
        });
        Ok(LoweredCond {
            prelude,
            expr: AstExpr::Var(res),
            paraphrase: spec.display.clone(),
        })
    }

    pub(super) fn lower_clause_graph(
        &mut self,
        graph: ClauseGraph,
        span: Span,
        depth: usize,
        then_items: Option<&[RawItem]>,
        else_items: Option<&[RawItem]>,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        if graph.scoped {
            self.push_scope(ScopeKind::WhereClause);
            let existing_vars = self
                .scopes
                .last()
                .map(|scope| scope.vars.keys().cloned().collect::<HashSet<_>>())
                .unwrap_or_default();
            let referent_checkpoint = self.discourse.current_referent_count();
            let mut block = Vec::new();
            let mut names = Vec::new();
            let mut renames = Vec::new();
            for binding in &graph.bindings {
                if let Some((name, expression)) = clause_graph::binding_parts(binding) {
                    let outer_exists = self.lookup(&name).is_some();
                    let lowered_name = if outer_exists {
                        self.next_temp("where")
                    } else {
                        name.clone()
                    };
                    let rewritten = rewrite_bound_names(&expression, &renames);
                    block.extend(self.lower_binding(&lowered_name, &rewritten, span)?);
                    if lowered_name != name {
                        renames.push((name, lowered_name.clone()));
                    }
                    names.push(lowered_name);
                }
            }
            if names.len() >= 2 {
                self.discourse.set_plural(names);
            }
            for clause in &graph.clauses {
                if clause.trim().is_empty() {
                    continue;
                }
                let clause = rewrite_bound_names(clause, &renames);
                let clause = crate::normalize::sentence(&clause);
                block.extend(self.lower_core(&clause, span, then_items, else_items, depth + 1)?);
            }
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
            let mut outer = Vec::new();
            for statement in &mut block {
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
            self.pop_scope();
            if let Some(parent) = self.scopes.last_mut() {
                for (name, ty) in moved_vars {
                    parent.vars.insert(name, ty);
                }
            }
            self.discourse.append_referents(moved_referents);
            outer.push(AstStmt::Block(block, self.span(span)));
            return Ok(outer);
        }
        let mut statements = Vec::new();
        let mut names = graph.compound_names.clone();
        for binding in &graph.bindings {
            if let Some((name, expression)) = clause_graph::binding_parts(binding) {
                names.push(name.clone());
                statements.extend(self.lower_binding(&name, &expression, span)?);
            }
        }
        if names.len() >= 2 {
            self.discourse.set_plural(names);
        }
        for clause in &graph.clauses {
            if clause.trim().is_empty() {
                continue;
            }
            if let Some(bindings) = clause_graph::split_binding_sequence(clause)
                && bindings.len() > 1
            {
                let mut binding_names = Vec::new();
                for binding in bindings {
                    if let Some((name, expression)) = clause_graph::binding_parts(&binding) {
                        binding_names.push(name.clone());
                        statements.extend(self.lower_binding(&name, &expression, span)?);
                    }
                }
                if binding_names.len() >= 2 {
                    self.discourse.set_plural(binding_names);
                }
                continue;
            }
            if let Some((parts, compound_names)) = clause_graph::split_compound_declaration(clause)
            {
                if compound_names.len() >= 2 {
                    self.discourse.set_plural(compound_names);
                }
                for part in parts {
                    statements.extend(self.lower_core(&part, span, None, None, depth + 1)?);
                }
            } else {
                statements.extend(self.lower_core(
                    clause,
                    span,
                    then_items,
                    else_items,
                    depth + 1,
                )?);
            }
        }
        Ok(statements)
    }

    pub(super) fn lower_binding(
        &mut self,
        name: &str,
        expression: &str,
        span: Span,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        let value = self.parse_value(expression, span)?;
        if let AstExpr::BytesLit(bytes) = &value {
            self.bytes_lengths.insert(name.to_string(), bytes.len());
        } else {
            self.bytes_lengths.remove(name);
        }
        let ty = self.shallow_type(&value);
        self.declare(name, ty.clone());
        self.introduce_name(name, &ty, span);
        Ok(vec![AstStmt::Let {
            name: name.to_string(),
            ty: None,
            init: value,
            span: self.span(span),
        }])
    }

    pub(super) fn lower_where_bindings(
        &mut self,
        bindings: &str,
        span: Span,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        let mut statements = Vec::new();
        let mut pieces = Vec::new();
        for comma in split_top_level(bindings, ",") {
            pieces.extend(split_top_level(&comma, "and"));
        }
        for piece in pieces {
            let binding = piece.trim();
            if binding.is_empty() {
                continue;
            }
            let Some((start, end)) =
                find_top_level_word(binding, "is").or_else(|| find_top_level_symbol(binding, "="))
            else {
                return Err(self.error(
                    span,
                    SemanticErrorKind::Lowering,
                    format!("`where` binding `{binding}` must use `<name> is <expression>`"),
                ));
            };
            let name = lower_name(binding[..start].trim());
            let value_text = binding[end..].trim();
            let value = self.parse_expr(value_text, span)?;
            let ty = self.shallow_type(&value);
            self.declare(&name, ty.clone());
            self.introduce_name(&name, &ty, span);
            statements.push(AstStmt::Let {
                name,
                ty: None,
                init: value,
                span: self.span(span),
            });
        }
        Ok(statements)
    }

    pub(super) fn lower_global_quantifier(
        &mut self,
        global: GlobalQuantifier,
        block_items: Option<&[RawItem]>,
        span: Span,
        depth: usize,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        enum Domain {
            Range {
                start: AstExpr,
                end: AstExpr,
                step: AstExpr,
                var_ty: AstType,
                step_constant: Option<i64>,
            },
            Array {
                element: AstType,
                len: usize,
                base: AstExpr,
            },
            List {
                element: AstType,
                len: AstExpr,
                data: AstExpr,
            },
        }
        let domain = match &global.domain {
            QuantDomainSpec::Range { start, end, step } => {
                let mut start = self.parse_expr(start, span)?;
                let mut end = self.parse_expr(end, span)?;
                let mut step = if let Some(text) = step {
                    self.parse_expr(text, span)?
                } else {
                    AstExpr::ConstInt(1, None)
                };
                let start_ty = self.shallow_type_opt(&start);
                let end_ty = self.shallow_type_opt(&end);
                let signed = |ty: &Option<AstType>| {
                    matches!(
                        ty,
                        Some(AstType::I8 | AstType::I16 | AstType::I32 | AstType::I64)
                    )
                };
                let unsigned = |ty: &Option<AstType>| {
                    matches!(
                        ty,
                        Some(AstType::U8 | AstType::U16 | AstType::U32 | AstType::U64)
                    )
                };
                if signed(&start_ty) && unsigned(&end_ty) {
                    end = AstExpr::Cast {
                        val: Box::new(end),
                        to: AstType::I64,
                    };
                } else if unsigned(&start_ty) && signed(&end_ty) {
                    start = AstExpr::Cast {
                        val: Box::new(start),
                        to: AstType::I64,
                    };
                }
                let var_ty = self.shallow_type(&start);
                if signed(&Some(var_ty.clone())) && unsigned(&self.shallow_type_opt(&step)) {
                    step = AstExpr::Cast {
                        val: Box::new(step),
                        to: var_ty.clone(),
                    };
                }
                if !Self::is_integer_type(&var_ty) {
                    return Err(self.error(
                        span,
                        SemanticErrorKind::Lowering,
                        "range quantifier bounds must be integers",
                    ));
                }
                let step_constant = Self::constant_i64(&step);
                if step_constant == Some(0) {
                    return Err(self.error(
                        span,
                        SemanticErrorKind::Lowering,
                        "quantifier step cannot be 0",
                    ));
                }
                Domain::Range {
                    start,
                    end,
                    step,
                    var_ty,
                    step_constant,
                }
            }
            QuantDomainSpec::Array { arr } => {
                let Some(ty) = self.lookup(arr).cloned() else {
                    return Err(self.error(
                        span,
                        SemanticErrorKind::Lowering,
                        format!("quantifier array `{arr}` is not in scope"),
                    ));
                };
                if let AstType::Array(element, len) = ty {
                    let base = if self.globals.contains_key(arr)
                        && !self.scopes.iter().any(|scope| scope.vars.contains_key(arr))
                    {
                        AstExpr::GlobalAddr {
                            module: None,
                            name: arr.clone(),
                        }
                    } else {
                        AstExpr::AddrOfVar(arr.clone())
                    };
                    Domain::Array {
                        element: *element,
                        len,
                        base,
                    }
                } else {
                    let list = match &ty {
                        AstType::Named {
                            name, type_args, ..
                        } if name.eq_ignore_ascii_case("list") => type_args.first().cloned(),
                        AstType::Ptr(inner) => match inner.as_ref() {
                            AstType::Named {
                                name, type_args, ..
                            } if name.eq_ignore_ascii_case("list") => type_args.first().cloned(),
                            _ => None,
                        },
                        _ => None,
                    };
                    let Some(element) = list else {
                        return Err(self.error(
                            span,
                            SemanticErrorKind::Lowering,
                            format!("quantifier domain `{arr}` is not a fixed-size array or List"),
                        ));
                    };
                    let len = self.parse_expr(&format!("{arr}'s len"), span)?;
                    let data = self.parse_expr(&format!("{arr}'s data"), span)?;
                    Domain::List { element, len, data }
                }
            }
        };
        let var_ty = match &domain {
            Domain::Range { var_ty, .. } => var_ty.clone(),
            Domain::Array { element, .. } | Domain::List { element, .. } => element.clone(),
        };
        let mut prelude = Vec::new();
        let (counter, end_temp, step_temp, first) = match &domain {
            Domain::Range { .. } => (
                self.next_temp("q_cur"),
                self.next_temp("q_end"),
                self.next_temp("q_step"),
                self.next_temp("q_first"),
            ),
            Domain::Array { .. } | Domain::List { .. } => (
                self.next_temp("q_idx"),
                String::new(),
                String::new(),
                self.next_temp("q_first"),
            ),
        };
        match &domain {
            Domain::Range {
                start,
                end,
                step,
                var_ty,
                step_constant,
            } => {
                self.declare_temp(&counter, var_ty.clone());
                self.declare_temp(&end_temp, var_ty.clone());
                self.declare_temp(&step_temp, var_ty.clone());
                self.declare_temp(&first, AstType::Bool);
                prelude.push(AstStmt::Let {
                    name: counter.clone(),
                    ty: Some(var_ty.clone()),
                    init: start.clone(),
                    span: self.span(span),
                });
                prelude.push(AstStmt::Let {
                    name: end_temp.clone(),
                    ty: Some(var_ty.clone()),
                    init: end.clone(),
                    span: self.span(span),
                });
                prelude.push(AstStmt::Let {
                    name: step_temp.clone(),
                    ty: Some(var_ty.clone()),
                    init: step.clone(),
                    span: self.span(span),
                });
                prelude.push(AstStmt::Let {
                    name: first.clone(),
                    ty: Some(AstType::Bool),
                    init: AstExpr::ConstBool(true),
                    span: self.span(span),
                });
                if step_constant.is_none() {
                    prelude.push(AstStmt::If {
                        cond: AstExpr::Binary {
                            op: tc_ir::BinOp::Eq,
                            lhs: Box::new(AstExpr::Var(step_temp.clone())),
                            rhs: Box::new(AstExpr::ConstInt(0, None)),
                        },
                        then_body: vec![AstStmt::Trap(self.span(span))],
                        else_body: Vec::new(),
                        span: self.span(span),
                    });
                }
            }
            Domain::Array { .. } | Domain::List { .. } => {
                let index_ty = if matches!(&domain, Domain::List { .. }) {
                    AstType::U64
                } else {
                    AstType::I64
                };
                self.declare_temp(&counter, index_ty.clone());
                self.declare_temp(&first, AstType::Bool);
                prelude.push(AstStmt::Let {
                    name: counter.clone(),
                    ty: Some(index_ty),
                    init: AstExpr::ConstInt(0, None),
                    span: self.span(span),
                });
                prelude.push(AstStmt::Let {
                    name: first.clone(),
                    ty: Some(AstType::Bool),
                    init: AstExpr::ConstBool(true),
                    span: self.span(span),
                });
            }
        }
        let termination = match &domain {
            Domain::Range { step, .. } => {
                Self::range_termination(&counter, &end_temp, &step_temp, step)
            }
            Domain::Array { len, .. } => AstExpr::Binary {
                op: tc_ir::BinOp::Ge,
                lhs: Box::new(AstExpr::Var(counter.clone())),
                rhs: Box::new(AstExpr::ConstInt(
                    i64::try_from(*len).unwrap_or(i64::MAX),
                    None,
                )),
            },
            Domain::List { len, .. } => AstExpr::Binary {
                op: tc_ir::BinOp::Ge,
                lhs: Box::new(AstExpr::Var(counter.clone())),
                rhs: Box::new(len.clone()),
            },
        };
        let mut loop_body = vec![
            AstStmt::If {
                cond: AstExpr::Unary {
                    op: tc_ir::UnOp::LogNot,
                    val: Box::new(AstExpr::Var(first.clone())),
                },
                then_body: vec![AstStmt::Assign {
                    name: counter.clone(),
                    val: match &domain {
                        Domain::Range { .. } => AstExpr::Binary {
                            op: tc_ir::BinOp::Add,
                            lhs: Box::new(AstExpr::Var(counter.clone())),
                            rhs: Box::new(AstExpr::Var(step_temp.clone())),
                        },
                        Domain::Array { .. } | Domain::List { .. } => AstExpr::Binary {
                            op: tc_ir::BinOp::Add,
                            lhs: Box::new(AstExpr::Var(counter.clone())),
                            rhs: Box::new(AstExpr::ConstInt(1, None)),
                        },
                    },
                    span: self.span(span),
                }],
                else_body: Vec::new(),
                span: self.span(span),
            },
            AstStmt::Assign {
                name: first,
                val: AstExpr::ConstBool(false),
                span: self.span(span),
            },
            AstStmt::If {
                cond: termination,
                then_body: vec![AstStmt::Break(self.span(span))],
                else_body: Vec::new(),
                span: self.span(span),
            },
        ];
        let init = match &domain {
            Domain::Range { .. } => AstExpr::Var(counter.clone()),
            Domain::Array { base, .. } => AstExpr::Load(Box::new(AstExpr::IndexAddr {
                base_ptr: Box::new(base.clone()),
                index: Box::new(AstExpr::Var(counter.clone())),
            })),
            Domain::List { data, .. } => AstExpr::Load(Box::new(AstExpr::IndexAddr {
                base_ptr: Box::new(data.clone()),
                index: Box::new(AstExpr::Var(counter.clone())),
            })),
        };
        loop_body.push(AstStmt::Let {
            name: global.var.clone(),
            ty: Some(var_ty.clone()),
            init,
            span: self.span(span),
        });
        self.push_scope(ScopeKind::QuantifierBody);
        self.declare(&global.var, var_ty.clone());
        self.introduce_name(&global.var, &var_ty, span);
        self.loop_depth += 1;
        let inline_body = global
            .body
            .as_deref()
            .filter(|text| !text.trim().is_empty());
        let previous_loop_var = self.active_loop_var.replace(global.var.clone());
        let body_result = match (block_items, inline_body) {
            (Some(items), _) if !items.is_empty() => self
                .lower_items(items, depth + 1)
                .map(|body| (body, Vec::new())),
            (_, Some(text)) => self.lower_actions_capturing(text, span, depth),
            _ => Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                "`For every` requires an inline body or an indented block",
            )),
        };
        self.active_loop_var = previous_loop_var;
        self.loop_depth -= 1;
        self.pop_scope();
        let (body, body_lines) = body_result?;
        let mut canonical = format!("{}:", global.display);
        for line in body_lines {
            canonical.push_str("\n       ");
            canonical.push_str(&line);
        }
        self.set_canonical(canonical);
        loop_body.extend(body);
        prelude.push(AstStmt::Loop {
            body: loop_body,
            span: self.span(span),
        });
        Ok(vec![AstStmt::Block(prelude, self.span(span))])
    }

    pub(super) fn split_condition_actions(
        &self,
        text: &str,
        span: Span,
    ) -> Result<(String, String), SemanticError> {
        let (condition, action) = split_header_condition(text.trim());
        if condition.is_empty() || action.is_empty() {
            return Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                format!("cannot separate condition from action in `{}`", text.trim()),
            ));
        }
        Ok((condition, action))
    }

    pub(super) fn lower_actions_capturing(
        &mut self,
        text: &str,
        span: Span,
        depth: usize,
    ) -> Result<(Vec<AstStmt>, Vec<String>), SemanticError> {
        self.synthetic_depth += 1;
        self.push_scope(ScopeKind::Block);
        let mut statements = Vec::new();
        let mut canonical = Vec::new();
        let mut error = None;
        for action in split_actions(text) {
            let control = starts_control(&action);
            let resolved = if control {
                action
            } else {
                match self.resolve_anaphors(&action, span) {
                    Ok(value) => value,
                    Err(failure) => {
                        error = Some(failure);
                        break;
                    }
                }
            };
            let sentence = RawSentence {
                text: resolved.clone(),
                raw: String::new(),
                span,
                fillers: Vec::new(),
            };
            match self.lower_stmt(&sentence, None, None, depth + 1) {
                Ok(mut lowered) => {
                    statements.append(&mut lowered);
                    canonical.push(canonicalize_statement(&resolved));
                }
                Err(failure) => {
                    error = Some(failure);
                    break;
                }
            }
        }
        self.pop_scope();
        self.synthetic_depth -= 1;
        match error {
            Some(failure) => Err(failure),
            None => Ok((statements, canonical)),
        }
    }

    pub(super) fn parse_natural_predicate(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<AstExpr>, SemanticError> {
        for (phrase, negated) in [
            ("is not an exact multiple of", true),
            ("is an exact multiple of", false),
            ("is not a multiple of", true),
            ("is a multiple of", false),
            ("is not divisible by", true),
            ("is divisible by", false),
            ("is not a factor of", true),
            ("is a factor of", false),
        ] {
            if let Some((left, end)) = find_top_level_phrase(text, phrase) {
                let right = text[end..].trim();
                if !left.trim().is_empty() && !right.is_empty() {
                    let lhs = self.parse_value(left.trim(), span)?;
                    let rhs = self.parse_value(right, span)?;
                    let modulo = AstExpr::Binary {
                        op: tc_ir::BinOp::Mod,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    };
                    let operator = if negated {
                        tc_ir::BinOp::Ne
                    } else {
                        tc_ir::BinOp::Eq
                    };
                    return Ok(Some(AstExpr::Binary {
                        op: operator,
                        lhs: Box::new(modulo),
                        rhs: Box::new(AstExpr::ConstInt(0, None)),
                    }));
                }
            }
        }
        for (phrase, negated) in [("is not between", true), ("is between", false)] {
            if let Some((left, end)) = find_top_level_phrase(text, phrase) {
                let rest = text[end..].trim();
                if left.trim().is_empty() {
                    continue;
                }
                if let Some((low, high_end)) = find_top_level_phrase(rest, "and") {
                    let low = low.trim();
                    let high = rest[high_end..].trim();
                    let value = self.parse_value(left.trim(), span)?;
                    let low = self.parse_value(low, span)?;
                    let high = self.parse_value(high, span)?;
                    let lower = AstExpr::Binary {
                        op: tc_ir::BinOp::Ge,
                        lhs: Box::new(value.clone()),
                        rhs: Box::new(low.clone()),
                    };
                    let upper = AstExpr::Binary {
                        op: tc_ir::BinOp::Le,
                        lhs: Box::new(value.clone()),
                        rhs: Box::new(high.clone()),
                    };
                    return Ok(Some(if negated {
                        AstExpr::Binary {
                            op: tc_ir::BinOp::LogOr,
                            lhs: Box::new(AstExpr::Binary {
                                op: tc_ir::BinOp::Lt,
                                lhs: Box::new(value.clone()),
                                rhs: Box::new(low),
                            }),
                            rhs: Box::new(AstExpr::Binary {
                                op: tc_ir::BinOp::Gt,
                                lhs: Box::new(value),
                                rhs: Box::new(high),
                            }),
                        }
                    } else {
                        AstExpr::Binary {
                            op: tc_ir::BinOp::LogAnd,
                            lhs: Box::new(lower),
                            rhs: Box::new(upper),
                        }
                    }));
                }
            }
        }
        for (phrase, kind, negated) in [
            ("is not even", "even", true),
            ("is even", "even", false),
            ("is not odd", "odd", true),
            ("is odd", "odd", false),
            ("is not positive", "positive", true),
            ("is positive", "positive", false),
            ("is not negative", "negative", true),
            ("is negative", "negative", false),
            ("is not zero", "zero", true),
            ("is nonzero", "zero", true),
            ("is zero", "zero", false),
            ("is not empty", "empty", true),
            ("is empty", "empty", false),
        ] {
            if let Some((left, end)) = find_top_level_phrase(text, phrase) {
                if !text[end..].trim().is_empty() || left.trim().is_empty() {
                    continue;
                }
                let value = self.parse_value(left.trim(), span)?;
                let zero = AstExpr::ConstInt(0, None);
                let expression = match kind {
                    "even" | "odd" => {
                        let modulo = AstExpr::Binary {
                            op: tc_ir::BinOp::Mod,
                            lhs: Box::new(value),
                            rhs: Box::new(AstExpr::ConstInt(2, None)),
                        };
                        let equal = kind == "even";
                        AstExpr::Binary {
                            op: if equal == negated {
                                tc_ir::BinOp::Ne
                            } else {
                                tc_ir::BinOp::Eq
                            },
                            lhs: Box::new(modulo),
                            rhs: Box::new(zero),
                        }
                    }
                    "positive" => AstExpr::Binary {
                        op: if negated {
                            tc_ir::BinOp::Le
                        } else {
                            tc_ir::BinOp::Gt
                        },
                        lhs: Box::new(value),
                        rhs: Box::new(zero),
                    },
                    "negative" => AstExpr::Binary {
                        op: if negated {
                            tc_ir::BinOp::Ge
                        } else {
                            tc_ir::BinOp::Lt
                        },
                        lhs: Box::new(value),
                        rhs: Box::new(zero),
                    },
                    "zero" => AstExpr::Binary {
                        op: if negated {
                            tc_ir::BinOp::Ne
                        } else {
                            tc_ir::BinOp::Eq
                        },
                        lhs: Box::new(value),
                        rhs: Box::new(zero),
                    },
                    _ => {
                        if let Some(length) = self.length_expression(value.clone(), span)? {
                            AstExpr::Binary {
                                op: if negated {
                                    tc_ir::BinOp::Ne
                                } else {
                                    tc_ir::BinOp::Eq
                                },
                                lhs: Box::new(length),
                                rhs: Box::new(zero),
                            }
                        } else if let Some(ty) = self.shallow_type_opt(&value)
                            && matches!(ty, AstType::Ptr(_))
                        {
                            AstExpr::Binary {
                                op: if negated {
                                    tc_ir::BinOp::Ne
                                } else {
                                    tc_ir::BinOp::Eq
                                },
                                lhs: Box::new(value),
                                rhs: Box::new(AstExpr::ConstNull(ty)),
                            }
                        } else {
                            return Err(self.error(
                                span,
                                SemanticErrorKind::Lowering,
                                "`is empty` requires a list, text, or pointer value",
                            ));
                        }
                    }
                };
                return Ok(Some(expression));
            }
        }
        for (phrase, operator) in [
            ("is no less than", tc_ir::BinOp::Ge),
            ("no less than", tc_ir::BinOp::Ge),
            ("is at least", tc_ir::BinOp::Ge),
            ("at least", tc_ir::BinOp::Ge),
            ("is no more than", tc_ir::BinOp::Le),
            ("no more than", tc_ir::BinOp::Le),
            ("is at most", tc_ir::BinOp::Le),
            ("at most", tc_ir::BinOp::Le),
            ("is more than", tc_ir::BinOp::Gt),
            ("is above", tc_ir::BinOp::Gt),
            ("is over", tc_ir::BinOp::Gt),
            ("exceeds", tc_ir::BinOp::Gt),
            ("is less than", tc_ir::BinOp::Lt),
            ("is fewer than", tc_ir::BinOp::Lt),
            ("is below", tc_ir::BinOp::Lt),
            ("is under", tc_ir::BinOp::Lt),
            ("is not equal to", tc_ir::BinOp::Ne),
            ("differs from", tc_ir::BinOp::Ne),
            ("is different from", tc_ir::BinOp::Ne),
            ("is equal to", tc_ir::BinOp::Eq),
            ("is the same as", tc_ir::BinOp::Eq),
            ("equals", tc_ir::BinOp::Eq),
        ] {
            if let Some((left, end)) = find_top_level_phrase(text, phrase) {
                let right = text[end..].trim();
                if left.trim().is_empty() || right.is_empty() {
                    continue;
                }
                let lhs = self.parse_value(left.trim(), span)?;
                let rhs = self.parse_value(right, span)?;
                return Ok(Some(AstExpr::Binary {
                    op: operator,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                }));
            }
        }
        Ok(None)
    }
}
