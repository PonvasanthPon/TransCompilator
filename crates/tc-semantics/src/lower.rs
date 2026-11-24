//! Declaration parsing and the single unified Core-7 statement lowerer.

#![allow(clippy::redundant_closure_for_method_calls)]

use std::collections::{BTreeMap, HashMap, HashSet};

use tc_lexicon::{SemanticConcept, VerbPredicate, classify_verb, prep_role};
use tc_typeck::{
    AstExpr, AstFn, AstGlobal, AstModule, AstStmt, AstStruct, AstType, AstUnion, Import,
    PhrasePart, PhrasePattern, Visibility,
};

use crate::clause_graph::{self, ClauseGraph};
use crate::cond::{
    CondAst, LoweredCond, parse_condition, render_cond_child_text, render_expr,
    split_header_condition, strip_outer_parentheses,
};
use crate::discourse::{
    DiscourseStack, ReferentKind, ResolvedAnaphor, ResolvedFieldAnaphor, ScopeKind,
};
use crate::discourse_scope::{
    CheckDomain, CheckKind, GridDirection, GridMove, GridStart, GridTurn, LargestTracker,
    SequenceGeneration, SequenceIteration, SequenceRecurrence, SequenceSeed, continuation_rest,
    is_grid_report, is_word_count_request, is_word_output_request, is_word_split_request,
    parse_adjacent_pass, parse_check, parse_grid_moves, parse_grid_start, parse_largest_tracker,
    parse_loop_filter, parse_loop_introduction, parse_move_guard, parse_named_list_start,
    parse_pass_cleanup, parse_sentence_literal, parse_sequence_generation,
    parse_sequence_iteration, parse_sequence_seed, parse_state_declaration, parse_stop_condition,
    parse_word_filter, post_loop_rest,
};
use crate::expr::{
    AggregateInfo, ExprEnv, parse_address, parse_expression, parse_store_address, parse_type_text,
};
use crate::intent::{IntentVerdict, classify};
use crate::paraphrase::{ParaphraseDoc, ParaphraseEntry, canonicalize_statement};
use crate::phrases::{PhraseBinding, PhraseMatch, match_all};
use crate::place::{canonical_assignment_target, parse_place};
use crate::quantifier::{
    GlobalQuantifier, LocalQuantKind, LocalQuantSpec, PredSpec, QuantDomainSpec,
    parse_global as parse_global_quantifier,
};
use crate::suggest::hint_for;
use crate::textutil::{
    find_top_level_symbol, find_top_level_word, first_word, is_lvalue_text, split_top_level,
    split_type_list, split_where, starts_with_word,
};
use crate::token::{Token, TokenKind, decode_string_token, tokenize};
use crate::value::ValueContext;
use crate::{
    ClauseTree, RawBlock, RawItem, RawSentence, SemanticError, SemanticErrorKind, SentenceTrace,
    SourceFile, Span, Voice, analyze_clause, segment_source,
};

const MAX_DEPTH: usize = 32;

type AggregateHeader = (String, Vec<String>, Vec<(String, AstType)>);

/// One function whose body still has to be lowered.
#[derive(Debug)]
pub(crate) struct FunctionJob {
    /// Index in the module function table.
    pub index: usize,
    /// Body items in source order.
    pub body: Vec<RawItem>,
}

/// Result of the declaration pass for one module.
#[derive(Debug)]
pub(crate) struct ModuleJob {
    /// Partially built AST module (function bodies empty).
    pub module: AstModule,
    /// Bodies awaiting lowering.
    pub functions: Vec<FunctionJob>,
    /// Top-level executable items.
    pub executable: Vec<RawItem>,
}

fn semantic_error(
    module: &str,
    span: Span,
    kind: SemanticErrorKind,
    message: impl Into<String>,
) -> SemanticError {
    SemanticError {
        module: module.to_string(),
        line: span.line,
        col: span.col,
        kind,
        message: message.into(),
    }
}

fn type_span(module: &str, span: Span) -> tc_typeck::Span {
    tc_typeck::Span::new(module, span.line, span.col)
}

fn rewrite_bound_names(text: &str, renames: &[(String, String)]) -> String {
    if renames.is_empty() {
        return text.to_string();
    }
    let tokens = tokenize(text);
    let mut replacements = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if !token.is_word() {
            index += 1;
            continue;
        }
        let mut matched = false;
        for (name, replacement) in renames {
            let words = name.split('_').collect::<Vec<_>>();
            if words.is_empty() || index + words.len() > tokens.len() {
                continue;
            }
            let same = words.iter().enumerate().all(|(offset, word)| {
                tokens[index + offset].is_word() && tokens[index + offset].is(word)
            });
            if same {
                replacements.push((
                    token.start,
                    tokens[index + words.len() - 1].end,
                    replacement.clone(),
                ));
                index += words.len();
                matched = true;
                break;
            }
        }
        if !matched {
            index += 1;
        }
    }
    if replacements.is_empty() {
        return text.to_string();
    }
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;
    for (start, end, replacement) in replacements {
        if start < cursor {
            continue;
        }
        output.push_str(&text[cursor..start]);
        output.push_str(&replacement);
        cursor = end;
    }
    output.push_str(&text[cursor..]);
    output
}

fn find_top_level_phrase(text: &str, phrase: &str) -> Option<(String, usize)> {
    let words = phrase.split_whitespace().collect::<Vec<_>>();
    let tokens = tokenize(text);
    let mut depth = 0_i32;
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if token.kind == TokenKind::Symbol {
            match token.text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                _ => {}
            }
        }
        if depth == 0 {
            let matched = words.iter().enumerate().all(|(offset, word)| {
                tokens.get(index + offset).is_some_and(|candidate| {
                    candidate.is_word() && candidate.is(&word.to_ascii_lowercase())
                })
            });
            if matched {
                let end = tokens[index + words.len() - 1].end;
                return Some((text[..token.start].to_string(), end));
            }
        }
        index += 1;
    }
    None
}

fn split_condition_comparison(text: &str) -> Option<(String, tc_ir::BinOp, String)> {
    let tokens = tokenize(text);
    let mut depth = 0_i32;
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if token.kind == TokenKind::Symbol {
            if depth == 0 {
                let operator = match token.text.as_str() {
                    ">=" => Some(tc_ir::BinOp::Ge),
                    "<=" => Some(tc_ir::BinOp::Le),
                    ">" => Some(tc_ir::BinOp::Gt),
                    "<" => Some(tc_ir::BinOp::Lt),
                    "==" => Some(tc_ir::BinOp::Eq),
                    "!=" => Some(tc_ir::BinOp::Ne),
                    _ => None,
                };
                if let Some(operator) = operator {
                    return Some((
                        text[..token.start].trim().to_string(),
                        operator,
                        text[token.end..].trim().to_string(),
                    ));
                }
            }
            match token.text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                _ => {}
            }
            index += 1;
            continue;
        }
        if depth != 0 || !token.is_word() {
            index += 1;
            continue;
        }
        let following = |offset: usize| {
            tokens
                .get(index + offset)
                .filter(|token| token.is_word())
                .map(|token| token.lower.as_str())
        };
        let (operator, words) = if token.is("is") {
            match (following(1), following(2)) {
                (Some("greater" | "more"), Some("than")) => (Some(tc_ir::BinOp::Gt), 2),
                (Some("less"), Some("than")) => (Some(tc_ir::BinOp::Lt), 2),
                (Some("at"), Some("least")) => (Some(tc_ir::BinOp::Ge), 2),
                (Some("at"), Some("most")) => (Some(tc_ir::BinOp::Le), 2),
                (Some("not"), Some("equal")) => (Some(tc_ir::BinOp::Ne), 3),
                (Some("equal"), Some("to")) => (Some(tc_ir::BinOp::Eq), 2),
                _ => (Some(tc_ir::BinOp::Eq), 0),
            }
        } else if token.is("equals") {
            (Some(tc_ir::BinOp::Eq), 0)
        } else if token.is("does") && following(1) == Some("not") && following(2) == Some("equal") {
            (Some(tc_ir::BinOp::Ne), 3)
        } else {
            (None, 0)
        };
        if let Some(operator) = operator {
            let mut right_start = index + words + 1;
            if token.is("is") && words == 3 && following(2) == Some("equal") {
                right_start += 1;
            }
            let end = tokens
                .get(right_start.saturating_sub(1))
                .map_or(token.end, |token| token.end);
            return Some((
                text[..token.start].trim().to_string(),
                operator,
                text[end..].trim().to_string(),
            ));
        }
        index += 1;
    }
    None
}

fn lower_name(name: &str) -> String {
    name.to_ascii_lowercase()
}

fn declaration_kind(text: &str) -> Option<&'static str> {
    let tokens = tokenize(text);
    let mut words = tokens
        .iter()
        .filter(|token| token.kind == TokenKind::Word)
        .map(|token| token.lower.as_str());
    let mut first = words.next()?;
    if first == "define" {
        first = words.next()?;
    }
    if matches!(first, "public" | "private") {
        first = words.next()?;
    }
    match first {
        "struct" => Some("struct"),
        "union" => Some("union"),
        "global" => Some("global"),
        "function" => Some("function"),
        "import" | "use" => Some("import"),
        _ => None,
    }
}

fn parse_visibility(text: &str) -> Visibility {
    let lower = text.to_ascii_lowercase();
    if lower.split_whitespace().any(|word| word == "public") {
        Visibility::Public
    } else {
        Visibility::Private
    }
}

fn parse_import(text: &str, module: &str, span: Span) -> Result<Import, SemanticError> {
    let tokens = tokenize(text);
    let mut index = 1;
    let module_name = tokens
        .get(index)
        .filter(|token| token.is_word())
        .map(|token| token.lower.clone())
        .ok_or_else(|| {
            semantic_error(
                module,
                span,
                SemanticErrorKind::Declaration,
                "import requires a module name",
            )
        })?;
    index += 1;
    let alias = if tokens.get(index).is_some_and(|token| token.is("as")) {
        index += 1;
        let alias = tokens
            .get(index)
            .filter(|token| token.is_word())
            .map(|token| token.lower.clone())
            .ok_or_else(|| {
                semantic_error(
                    module,
                    span,
                    SemanticErrorKind::Declaration,
                    "import alias is missing",
                )
            })?;
        Some(alias)
    } else {
        None
    };
    Ok(Import {
        module: module_name,
        alias,
        span: type_span(module, span),
    })
}

fn top_level_word_count(text: &str, word: &str) -> usize {
    let mut count = 0;
    let mut rest = text;
    while let Some((_, end)) = find_top_level_word(rest, word) {
        count += 1;
        rest = &rest[end..];
    }
    count
}

fn split_as(text: &str) -> Option<(String, String)> {
    let (start, end) = find_top_level_word(text, "as")?;
    let left = text[..start].trim();
    let right = text[end..].trim();
    if left.is_empty() || right.is_empty() {
        return None;
    }
    Some((left.to_string(), right.to_string()))
}

fn canonical_name_phrase(text: &str) -> String {
    let mut words = tokenize(text)
        .into_iter()
        .filter(|token| token.is_word())
        .map(|token| token.lower)
        .collect::<Vec<_>>();
    if words.len() > 1 && matches!(words.first().map(String::as_str), Some("a" | "an" | "the")) {
        words.remove(0);
    }
    words.join("_")
}

fn original_name_phrase(text: &str) -> String {
    let mut words = tokenize(text)
        .into_iter()
        .filter(|token| token.is_word())
        .map(|token| token.lower)
        .collect::<Vec<_>>();
    if words.len() > 1 && matches!(words.first().map(String::as_str), Some("a" | "an" | "the")) {
        words.remove(0);
    }
    words.join(" ")
}

fn split_parenthetical_type(text: &str) -> Option<(String, String)> {
    let trimmed = text.trim();
    if !trimmed.ends_with(')') {
        return None;
    }
    let tokens = tokenize(trimmed);
    let close = tokens.iter().rposition(|token| token.text == ")")?;
    if close + 1 != tokens.len() {
        return None;
    }
    let mut depth = 0_i32;
    let mut open = None;
    for index in (0..=close).rev() {
        match tokens[index].text.as_str() {
            ")" => depth += 1,
            "(" => {
                depth -= 1;
                if depth == 0 {
                    open = Some(index);
                    break;
                }
            }
            _ => {}
        }
    }
    let open = open?;
    let name = trimmed[tokens.first()?.start..tokens[open].start]
        .trim()
        .to_string();
    let ty = trimmed[tokens[open].end..tokens[close].start]
        .trim()
        .to_string();
    if name.is_empty() || ty.is_empty() {
        None
    } else {
        Some((name, ty))
    }
}

fn validate_field_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn parse_field_specs(
    text: &str,
    module: &str,
    span: Span,
) -> Result<Vec<(String, String)>, SemanticError> {
    let mut specs = Vec::new();
    for comma_piece in split_type_list(text, ",") {
        let piece = comma_piece
            .trim()
            .trim_start_matches("and ")
            .trim()
            .trim_end_matches('.')
            .trim();
        if piece.is_empty() {
            continue;
        }
        let parenthetical_groups = tokenize(piece)
            .iter()
            .filter(|token| token.text == "(")
            .count();
        if top_level_word_count(piece, "as") > 1 || parenthetical_groups > 1 {
            for and_piece in split_type_list(piece, "and") {
                for inner in split_type_list(&and_piece, "or") {
                    let inner = inner.trim();
                    if inner.is_empty() {
                        continue;
                    }
                    let Some((name_text, ty)) =
                        split_as(inner).or_else(|| split_parenthetical_type(inner))
                    else {
                        return Err(semantic_error(
                            module,
                            span,
                            SemanticErrorKind::Declaration,
                            format!(
                                "field declaration `{inner}` must use `<name> as <type>` or `<name> (<type>)`"
                            ),
                        ));
                    };
                    let name = tokenize(&name_text)
                        .into_iter()
                        .filter(|token| token.is_word())
                        .find(|token| !matches!(token.lower.as_str(), "a" | "an" | "the"))
                        .map_or_else(|| canonical_name_phrase(&name_text), |token| token.lower);
                    specs.push((name, ty.trim().to_string()));
                }
            }
            continue;
        }
        if let Some((name_text, ty_text)) = split_as(piece) {
            let name = tokenize(&name_text)
                .into_iter()
                .filter(|token| token.is_word())
                .find(|token| !matches!(token.lower.as_str(), "a" | "an" | "the"))
                .map_or_else(|| canonical_name_phrase(&name_text), |token| token.lower);
            if !validate_field_name(&name) {
                return Err(semantic_error(
                    module,
                    span,
                    SemanticErrorKind::Declaration,
                    format!("invalid field name `{name}`"),
                ));
            }
            specs.push((name, ty_text.trim().to_string()));
            continue;
        }
        let Some((name_text, ty_text)) = split_parenthetical_type(piece) else {
            return Err(semantic_error(
                module,
                span,
                SemanticErrorKind::Declaration,
                format!(
                    "field declaration `{piece}` must use `<name> as <type>` or `<name> (<type>)`"
                ),
            ));
        };
        let names = if top_level_word_count(&name_text, "and") > 0 {
            split_type_list(&name_text, "and")
        } else {
            vec![name_text]
        };
        for raw_name in names {
            let name = canonical_name_phrase(raw_name.trim());
            if !validate_field_name(&name) {
                return Err(semantic_error(
                    module,
                    span,
                    SemanticErrorKind::Declaration,
                    format!("invalid field name `{name}`"),
                ));
            }
            specs.push((name, ty_text.trim().to_string()));
        }
    }
    Ok(specs)
}

fn parse_fields(
    text: &str,
    module: &str,
    span: Span,
) -> Result<Vec<(String, AstType)>, SemanticError> {
    let mut fields = Vec::new();
    for (name, ty_text) in parse_field_specs(text, module, span)? {
        let ty_text = normalize_type_text_words(&ty_text);
        let ty = parse_type_text(&ty_text).map_err(|message| {
            semantic_error(module, span, SemanticErrorKind::Declaration, message)
        })?;
        fields.push((name, ty));
    }
    Ok(fields)
}

fn parse_type_params(text: &str, module: &str, span: Span) -> Result<Vec<String>, SemanticError> {
    let mut params = Vec::new();
    for piece in split_type_list(text, "and") {
        for inner in split_type_list(&piece, ",") {
            let value = inner.trim();
            if value.is_empty() {
                continue;
            }
            if !value
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
            {
                return Err(semantic_error(
                    module,
                    span,
                    SemanticErrorKind::Declaration,
                    format!("invalid type parameter `{value}`"),
                ));
            }
            params.push(lower_name(value));
        }
    }
    Ok(params)
}

fn parse_struct_header(
    text: &str,
    module: &str,
    span: Span,
    is_union: bool,
) -> Result<AggregateHeader, SemanticError> {
    let tokens = tokenize(text);
    let mut index = 0;
    while tokens
        .get(index)
        .is_some_and(|token| token.is("define") || token.is("public") || token.is("private"))
    {
        index += 1;
    }
    let keyword = if is_union { "union" } else { "struct" };
    if !tokens.get(index).is_some_and(|token| token.is(keyword)) {
        return Err(semantic_error(
            module,
            span,
            SemanticErrorKind::Declaration,
            format!("expected `{keyword}`"),
        ));
    }
    index += 1;
    let name = tokens
        .get(index)
        .filter(|token| token.is_word())
        .map(|token| lower_name(&token.text))
        .ok_or_else(|| {
            semantic_error(
                module,
                span,
                SemanticErrorKind::Declaration,
                "aggregate name is missing",
            )
        })?;
    index += 1;
    let rest_start = tokens.get(index).map_or(text.len(), |token| token.start);
    let rest = &text[rest_start..];
    let (type_text, field_text) = if let Some((start, end)) = find_top_level_word(rest, "with") {
        (&rest[..start], Some(&rest[end..]))
    } else {
        (rest, None)
    };
    let type_params = if let Some((_, end)) = find_top_level_word(type_text, "of") {
        parse_type_params(type_text[end..].trim(), module, span)?
    } else {
        Vec::new()
    };
    let fields = if let Some(field_text) = field_text {
        parse_fields(field_text, module, span)?
    } else {
        Vec::new()
    };
    Ok((name, type_params, fields))
}

fn extract_phrase_lines(
    items: &mut Vec<RawItem>,
    module: &str,
    span: Span,
) -> Result<Vec<PhrasePattern>, SemanticError> {
    let mut patterns = Vec::new();
    let mut remaining = Vec::new();
    for item in std::mem::take(items) {
        match item {
            RawItem::Sentence(sentence)
                if sentence.text.to_ascii_lowercase().starts_with("phrase:") =>
            {
                patterns.push(parse_phrase_pattern(&sentence.text, module, span)?);
            }
            other => remaining.push(other),
        }
    }
    *items = remaining;
    let mut seen = HashSet::new();
    patterns.retain(|pattern| {
        let key = pattern
            .parts
            .iter()
            .map(|part| match part {
                PhrasePart::Word(word) => word.clone(),
                PhrasePart::Param(param) => format!("<{param}>"),
            })
            .collect::<Vec<_>>()
            .join(" ");
        seen.insert(key)
    });
    Ok(patterns)
}

fn parse_phrase_pattern(
    text: &str,
    module: &str,
    span: Span,
) -> Result<PhrasePattern, SemanticError> {
    let tokens = tokenize(text);
    let literal = tokens
        .iter()
        .find(|token| token.kind == TokenKind::String)
        .map(|token| token.text.clone())
        .ok_or_else(|| {
            semantic_error(
                module,
                span,
                SemanticErrorKind::Declaration,
                "Phrase: requires a quoted pattern",
            )
        })?;
    let decoded = decode_string_token(&literal).ok_or_else(|| {
        semantic_error(
            module,
            span,
            SemanticErrorKind::Declaration,
            "malformed phrase string",
        )
    })?;
    let pattern_text = String::from_utf8(decoded).map_err(|_| {
        semantic_error(
            module,
            span,
            SemanticErrorKind::Declaration,
            "phrase pattern must be UTF-8",
        )
    })?;
    let mut parts = Vec::new();
    for word in pattern_text.split_whitespace() {
        if let Some(param) = word
            .strip_prefix('<')
            .and_then(|value| value.strip_suffix('>'))
        {
            let param = lower_name(param);
            if param.is_empty() {
                return Err(semantic_error(
                    module,
                    span,
                    SemanticErrorKind::Declaration,
                    "empty phrase parameter",
                ));
            }
            parts.push(PhrasePart::Param(param));
        } else {
            parts.push(PhrasePart::Word(lower_name(word)));
        }
    }
    if parts.is_empty() {
        return Err(semantic_error(
            module,
            span,
            SemanticErrorKind::Declaration,
            "empty phrase pattern",
        ));
    }
    Ok(PhrasePattern { parts })
}

fn parse_function_header(text: &str, module: &str, span: Span) -> Result<AstFn, SemanticError> {
    let tokens = tokenize(text);
    let mut index = 0;
    while tokens
        .get(index)
        .is_some_and(|token| token.is("define") || token.is("public") || token.is("private"))
    {
        index += 1;
    }
    if !tokens.get(index).is_some_and(|token| token.is("function")) {
        return Err(semantic_error(
            module,
            span,
            SemanticErrorKind::Declaration,
            "expected `function`",
        ));
    }
    index += 1;
    let name = tokens
        .get(index)
        .filter(|token| token.is_word())
        .map(|token| lower_name(&token.text))
        .ok_or_else(|| {
            semantic_error(
                module,
                span,
                SemanticErrorKind::Declaration,
                "function name is missing",
            )
        })?;
    index += 1;
    let mut type_params = Vec::new();
    let mut params = Vec::new();
    let mut ret = AstType::Void;
    while index < tokens.len() {
        if tokens[index].is("of") {
            index += 1;
            let start = tokens.get(index).map_or(text.len(), |token| token.start);
            let mut end = text.len();
            while index < tokens.len() {
                if tokens[index].is("taking") || tokens[index].is("returning") {
                    end = tokens[index].start;
                    break;
                }
                index += 1;
            }
            type_params = parse_type_params(&text[start..end], module, span)?;
            continue;
        }
        if tokens[index].is("taking") {
            index += 1;
            let start = tokens.get(index).map_or(text.len(), |token| token.start);
            let mut end = text.len();
            while index < tokens.len() {
                if tokens[index].is("returning") {
                    end = tokens[index].start;
                    break;
                }
                index += 1;
            }
            let param_text = &text[start..end];
            for comma_piece in split_type_list(param_text, ",") {
                if top_level_word_count(&comma_piece, "as") == 1 {
                    let Some((param_name, ty_text)) = split_as(&comma_piece) else {
                        continue;
                    };
                    let ty = parse_type_text(ty_text.trim()).map_err(|message| {
                        semantic_error(module, span, SemanticErrorKind::Declaration, message)
                    })?;
                    params.push((lower_name(param_name.trim()), ty));
                    continue;
                }
                for inner in split_type_list(&comma_piece, "and") {
                    if inner.trim().is_empty() {
                        continue;
                    }
                    let Some((param_name, ty_text)) = split_as(&inner) else {
                        return Err(semantic_error(
                            module,
                            span,
                            SemanticErrorKind::Declaration,
                            format!("parameter `{inner}` must use `<name> as <type>`"),
                        ));
                    };
                    let ty = parse_type_text(ty_text.trim()).map_err(|message| {
                        semantic_error(module, span, SemanticErrorKind::Declaration, message)
                    })?;
                    params.push((lower_name(param_name.trim()), ty));
                }
            }
            continue;
        }
        if tokens[index].is("returning") {
            index += 1;
            let start = tokens.get(index).map_or(text.len(), |token| token.start);
            let ty_text = text[start..].trim();
            ret = parse_type_text(ty_text).map_err(|message| {
                semantic_error(module, span, SemanticErrorKind::Declaration, message)
            })?;
            break;
        }
        index += 1;
    }
    Ok(AstFn {
        vis: parse_visibility(text),
        name,
        type_params,
        params,
        ret,
        phrases: Vec::new(),
        body: Vec::new(),
        span: type_span(module, span),
    })
}

fn natural_function_header(text: &str) -> bool {
    let tokens = tokenize(text);
    let mut index = 0;
    while tokens.get(index).is_some_and(|token| {
        matches!(
            token.lower.as_str(),
            "publicly" | "public" | "private" | "," | "the" | "a" | "an"
        )
    }) {
        index += 1;
    }
    tokens.get(index).is_some_and(|token| token.is("to"))
        && tokens.get(index + 1).is_some_and(|token| token.is_word())
}

fn natural_visibility(tokens: &[tc_syntax::Token]) -> &'static str {
    if tokens
        .iter()
        .take_while(|token| !token.is("to"))
        .any(|token| token.is("public") || token.is("publicly"))
    {
        "Public "
    } else if tokens
        .iter()
        .take_while(|token| !token.is("to"))
        .any(|token| token.is("private"))
    {
        "Private "
    } else {
        ""
    }
}

fn split_natural_parameters(
    text: &str,
    module: &str,
    span: Span,
) -> Result<Vec<(String, String)>, SemanticError> {
    let mut params = Vec::new();
    for comma_piece in split_type_list(text, ",") {
        if let Some((name, ty)) = split_as(&comma_piece) {
            params.push((lower_name(name.trim()), ty.trim().to_string()));
            continue;
        }
        for piece in split_type_list(&comma_piece, "and") {
            let piece = piece.trim();
            if piece.is_empty() {
                continue;
            }
            let tokens = tokenize(piece);
            let Some(name_token) = tokens.last() else {
                continue;
            };
            if !name_token.is_word() {
                return Err(semantic_error(
                    module,
                    span,
                    SemanticErrorKind::Declaration,
                    format!("parameter `{piece}` is missing a name"),
                ));
            }
            let name = lower_name(&name_token.text);
            let type_text = piece[..name_token.start].trim();
            let ty = if type_text.is_empty() {
                "number".to_string()
            } else {
                type_text.to_string()
            };
            params.push((name, ty));
        }
    }
    Ok(params)
}

fn compact_natural_function_type(ty: &str) -> Option<String> {
    let ty = ty.trim();
    let ty = ty
        .strip_prefix("a ")
        .or_else(|| ty.strip_prefix("an "))
        .unwrap_or(ty);
    let rest = ty.strip_prefix("function taking ")?;
    let (start, end) = find_top_level_word(rest, "returning")?;
    let params = split_type_list(rest[..start].trim(), "and")
        .into_iter()
        .flat_map(|piece| split_type_list(&piece, ","))
        .map(|piece| piece.trim().to_string())
        .filter(|piece| !piece.is_empty())
        .collect::<Vec<_>>();
    Some(format!(
        "fn({}) -> {}",
        params.join(", "),
        rest[end..].trim()
    ))
}

fn v2_type_indicator(word: &str, type_params: &[String]) -> bool {
    matches!(
        word,
        "whole"
            | "signed"
            | "unsigned"
            | "number"
            | "numbers"
            | "integer"
            | "count"
            | "small"
            | "short"
            | "byte"
            | "decimal"
            | "decimals"
            | "real"
            | "truth"
            | "flag"
            | "boolean"
            | "pointer"
            | "address"
            | "block"
            | "fixed"
            | "nothing"
            | "function"
    ) || type_params.iter().any(|param| param == word)
        || word.chars().next().is_some_and(char::is_uppercase)
}

fn v2_type_word(word: &str, type_params: &[String]) -> bool {
    matches!(
        word,
        "a" | "an"
            | "the"
            | "whole"
            | "signed"
            | "unsigned"
            | "number"
            | "numbers"
            | "integer"
            | "count"
            | "small"
            | "short"
            | "byte"
            | "bytes"
            | "decimal"
            | "decimals"
            | "real"
            | "truth"
            | "value"
            | "flag"
            | "boolean"
            | "pointer"
            | "address"
            | "of"
            | "to"
            | "block"
            | "fixed"
            | "nothing"
            | "function"
            | "taking"
            | "returning"
    ) || type_params.iter().any(|param| param == word)
        || word.chars().next().is_some_and(char::is_uppercase)
}

fn normalize_type_text_words(text: &str) -> String {
    text.replace("numbers", "number")
        .replace("decimals", "decimal")
        .replace("bytes ", "byte ")
        .trim_end_matches(" bytes")
        .to_string()
}

fn extract_embedded_parameters(
    tokens: &[tc_syntax::Token],
    type_params: &[String],
) -> Vec<(String, String)> {
    let mut params: Vec<(String, String)> = Vec::new();
    let mut last_type: Option<String> = None;
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if token.is("and") {
            if let Some(next) = tokens.get(index + 1) {
                if next.is_word() && !v2_type_word(&next.lower, type_params) {
                    if let Some(ty) = &last_type {
                        if !params.iter().any(|(name, _)| name == &next.lower) {
                            params.push((next.lower.clone(), ty.clone()));
                        }
                    }
                    index += 2;
                    continue;
                }
            }
            index += 1;
            continue;
        }
        let starts_type = if token.is("a") || token.is("an") {
            tokens.get(index + 1).is_some_and(|next| {
                v2_type_indicator(&next.lower, type_params)
                    || next.text.chars().next().is_some_and(char::is_uppercase)
            })
        } else {
            v2_type_indicator(&token.lower, type_params)
                || token.text.chars().next().is_some_and(char::is_uppercase)
        };
        if !token.is_word() || !starts_type {
            index += 1;
            continue;
        }
        let start = index;
        while index < tokens.len() {
            let candidate = &tokens[index];
            if candidate.kind != TokenKind::Word
                || (!v2_type_word(&candidate.lower, type_params)
                    && !candidate
                        .text
                        .chars()
                        .next()
                        .is_some_and(char::is_uppercase))
                || candidate.is("is")
                || (index != start && matches!(candidate.lower.as_str(), "a" | "an" | "the"))
            {
                break;
            }
            index += 1;
        }
        let type_text = tokens[start..index]
            .iter()
            .map(|token| token.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        if type_text.trim().is_empty() {
            continue;
        }
        let Some(name_token) = tokens.get(index) else {
            continue;
        };
        if !name_token.is_word()
            || v2_type_indicator(&name_token.lower, type_params)
            || matches!(
                name_token.lower.as_str(),
                "is" | "are"
                    | "for"
                    | "if"
                    | "otherwise"
                    | "returning"
                    | "given"
                    | "and"
                    | "or"
                    | "into"
                    | "from"
                    | "to"
                    | "with"
                    | "at"
                    | "in"
                    | "on"
                    | "by"
                    | "between"
            )
        {
            continue;
        }
        let name = name_token.lower.clone();
        if !params.iter().any(|(existing, _)| existing == &name) {
            let ty = normalize_type_text_words(&type_text);
            params.push((name, ty.clone()));
            last_type = Some(ty);
        }
        index += 1;
    }
    params
}

#[derive(Debug, Clone)]
struct ParentheticalParam {
    canonical_names: Vec<String>,
    original_names: Vec<String>,
    ty: String,
    replace_start: usize,
    close_index: usize,
    plural: bool,
}

fn is_parameter_run_stop(word: &str) -> bool {
    matches!(
        word,
        "of" | "from"
            | "for"
            | "to"
            | "with"
            | "at"
            | "in"
            | "on"
            | "by"
            | "into"
            | "as"
            | "given"
            | "bytes"
            | "text"
            | "file"
            | "named"
            | "called"
            | "giving"
            | "returning"
            | "if"
            | "unless"
            | "while"
    )
}

fn type_parameter_span(tokens: &[Token], to_index: usize) -> Option<(usize, usize)> {
    let mut index = to_index + 1;
    while index + 1 < tokens.len() {
        if matches!(
            tokens[index].lower.as_str(),
            "given" | "giving" | "returning" | "named" | "called"
        ) {
            break;
        }
        if tokens[index].is("for") || tokens[index].is("of") {
            let next = tokens.get(index + 1)?;
            if next.is_word() && next.text.chars().next().is_some_and(char::is_uppercase) {
                let start = index;
                let mut end = index + 1;
                while end < tokens.len() {
                    let token = &tokens[end];
                    if token.is("and")
                        || token.text == ","
                        || (token.is_word()
                            && token.text.chars().next().is_some_and(char::is_uppercase))
                    {
                        end += 1;
                    } else {
                        break;
                    }
                }
                return Some((start, end));
            }
        }
        index += 1;
    }
    None
}

fn outer_return_position(tokens: &[Token]) -> Option<usize> {
    for index in (0..tokens.len()).rev() {
        if !(tokens[index].is("returning") || tokens[index].is("giving")) {
            continue;
        }
        let segment_start = tokens[..index]
            .iter()
            .rposition(|token| token.text == "," || token.is("given"))
            .map_or(0, |position| position + 1);
        let inside_function_type = tokens[segment_start..index]
            .iter()
            .any(|token| token.is("taking"));
        if !inside_function_type {
            return Some(index);
        }
    }
    None
}

fn parenthetical_parameters(text: &str) -> Vec<ParentheticalParam> {
    let tokens = tokenize(text);
    let Some(to_index) = tokens.iter().position(|token| token.is("to")) else {
        return Vec::new();
    };
    let minimum_start = to_index + 2;
    let mut params = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        if tokens[index].text == "(" {
            {
                let open = index;
                let mut close = None;
                let mut inner_depth = 0_i32;
                for (cursor, token) in tokens.iter().enumerate().skip(open) {
                    match token.text.as_str() {
                        "(" => inner_depth += 1,
                        ")" => {
                            inner_depth -= 1;
                            if inner_depth == 0 {
                                close = Some(cursor);
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                let Some(close) = close else {
                    break;
                };
                let mut run_start = open;
                while run_start > minimum_start {
                    let previous = &tokens[run_start - 1];
                    if previous.kind != TokenKind::Word || is_parameter_run_stop(&previous.lower) {
                        break;
                    }
                    run_start -= 1;
                }
                if run_start < open {
                    let mut words = tokens[run_start..open]
                        .iter()
                        .map(|token| token.lower.clone())
                        .collect::<Vec<_>>();
                    if words.first().is_some_and(|word| word == "and") {
                        words.remove(0);
                    }
                    let type_text = normalize_type_text_words(
                        text[tokens[open].end..tokens[close].start].trim(),
                    );
                    let plural = words.iter().any(|word| word == "and");
                    let multi_word = !plural && run_start == minimum_start && words.len() >= 2;
                    let (canonical_names, original_names, replace_start) = if plural {
                        let mut names = Vec::new();
                        let mut originals = Vec::new();
                        for piece in split_type_list(&words.join(" "), "and") {
                            let original = original_name_phrase(&piece);
                            let canonical = canonical_name_phrase(&piece);
                            if !canonical.is_empty() {
                                originals.push(original);
                                names.push(canonical);
                            }
                        }
                        (names, originals, run_start)
                    } else if multi_word {
                        let original = original_name_phrase(&words.join(" "));
                        let canonical = canonical_name_phrase(&words.join(" "));
                        (vec![canonical], vec![original], run_start)
                    } else {
                        let original = words.last().cloned().unwrap_or_default();
                        let canonical = canonical_name_phrase(&original);
                        (vec![canonical], vec![original], open - 1)
                    };
                    if canonical_names.iter().all(|name| !name.is_empty()) {
                        params.push(ParentheticalParam {
                            canonical_names,
                            original_names,
                            ty: type_text,
                            replace_start,
                            close_index: close,
                            plural,
                        });
                    }
                }
                index = close + 1;
                continue;
            }
        }
        index += 1;
    }
    params
}

fn derive_function_name(text: &str, params: &[ParentheticalParam]) -> Option<String> {
    let tokens = tokenize(text);
    let to_index = tokens.iter().position(|token| token.is("to"))?;
    let end = tokens
        .iter()
        .position(|token| {
            token.is("giving") || token.is("returning") || token.is("named") || token.is("called")
        })
        .unwrap_or(tokens.len());
    let mut skip = vec![false; tokens.len()];
    for param in params {
        for slot in &mut skip[param.replace_start..=param.close_index] {
            *slot = true;
        }
    }
    if let Some((start, span_end)) = type_parameter_span(&tokens, to_index) {
        for slot in &mut skip[start..span_end.min(end)] {
            *slot = true;
        }
    }
    let mut words = Vec::new();
    for (index, token) in tokens.iter().enumerate().take(end).skip(to_index + 1) {
        if skip[index] || (!token.is_word() && token.kind != TokenKind::Number) {
            continue;
        }
        if matches!(
            token.lower.as_str(),
            "a" | "an"
                | "the"
                | "and"
                | "or"
                | "given"
                | "as"
                | "named"
                | "called"
                | "giving"
                | "returning"
        ) {
            continue;
        }
        words.push(token.lower.clone());
    }
    while words.last().is_some_and(|word| {
        matches!(
            word.as_str(),
            "of" | "for" | "from" | "to" | "with" | "at" | "in" | "on" | "by" | "into"
        )
    }) {
        words.pop();
    }
    if words.is_empty() {
        return tokens
            .get(to_index + 1)
            .filter(|token| token.is_word())
            .map(|token| token.lower.clone());
    }
    Some(words.join("_"))
}

fn normalize_function_header(
    text: &str,
    module: &str,
    span: Span,
) -> Result<String, SemanticError> {
    let mut clean = text.trim().trim_end_matches('.').to_string();
    let mut canonical_name = None;
    for keyword in ["named", "called"] {
        if let Some((start, end)) = find_top_level_word(&clean, keyword) {
            let rest = &clean[end..];
            if let Some(name_token) = tokenize(rest).into_iter().find(|token| token.is_word()) {
                canonical_name = Some(lower_name(&name_token.text));
                let mut remove_start = start;
                if let Some(comma) = clean[..start].rfind(',') {
                    remove_start = comma;
                }
                let remove_end = end + name_token.end;
                clean.replace_range(remove_start..remove_end, "");
            }
            break;
        }
    }
    let text = clean.as_str();
    let tokens = tokenize(text);
    let Some(to_index) = tokens.iter().position(|token| token.is("to")) else {
        return Err(semantic_error(
            module,
            span,
            SemanticErrorKind::Declaration,
            "natural function header is missing `To`",
        ));
    };
    let Some(name_token) = tokens.get(to_index + 1).filter(|token| token.is_word()) else {
        return Err(semantic_error(
            module,
            span,
            SemanticErrorKind::Declaration,
            "natural function name is missing",
        ));
    };
    let parenthetical = parenthetical_parameters(text);
    let name = canonical_name.unwrap_or_else(|| {
        if parenthetical.is_empty() {
            lower_name(&name_token.text)
        } else {
            derive_function_name(text, &parenthetical)
                .unwrap_or_else(|| lower_name(&name_token.text))
        }
    });
    let mut type_params = Vec::new();
    if let Some((start_index, end_index)) = type_parameter_span(&tokens, to_index) {
        let start = tokens
            .get(start_index)
            .map_or(text.len(), |token| token.end);
        let end = tokens
            .get(end_index)
            .map_or(text.len(), |token| token.start);
        type_params = parse_type_params(&text[start..end], module, span)?;
    }
    let mut params = Vec::new();
    let outer_returning = outer_return_position(&tokens);
    if !parenthetical.is_empty() {
        for param in &parenthetical {
            for canonical in &param.canonical_names {
                params.push((canonical.clone(), param.ty.clone()));
            }
        }
    }
    if params.is_empty() {
        if let Some(given_index) = tokens.iter().position(|token| token.is("given")) {
            let start = tokens
                .get(given_index + 1)
                .map_or(text.len(), |token| token.start);
            let end = outer_returning
                .and_then(|position| tokens.get(position))
                .map_or(text.len(), |token| token.start);
            params = split_natural_parameters(&text[start..end], module, span)?;
        }
    }
    if params.is_empty() {
        let phrase_start = tokens
            .get(to_index + 2)
            .map_or(text.len(), |token| token.start);
        let phrase_end = outer_returning
            .and_then(|position| tokens.get(position))
            .map_or(text.len(), |token| token.start);
        let phrase_tokens = tokenize(&text[phrase_start..phrase_end]);
        params = extract_embedded_parameters(&phrase_tokens, &type_params);
    }
    let mut ret = "nothing".to_string();
    if let Some(position) = outer_returning {
        let start = tokens
            .get(position + 1)
            .map_or(text.len(), |token| token.start);
        ret = text[start..]
            .trim()
            .trim_end_matches(':')
            .trim_end_matches('.')
            .trim()
            .to_string();
    }
    let mut header = String::new();
    header.push_str(natural_visibility(&tokens));
    header.push_str("function ");
    header.push_str(&name);
    if !type_params.is_empty() {
        header.push_str(" of ");
        header.push_str(&type_params.join(" and "));
    }
    if !params.is_empty() {
        header.push_str(" taking ");
        header.push_str(
            &params
                .into_iter()
                .map(|(name, ty)| {
                    let ty = compact_natural_function_type(&ty).unwrap_or(ty);
                    format!("{name} as {ty}")
                })
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    header.push_str(" returning ");
    header.push_str(&ret);
    header.push(':');
    Ok(header)
}

fn quoted_values(line: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut parts = line.split('"');
    parts.next();
    while let Some(value) = parts.next() {
        values.push(value.to_string());
        parts.next();
    }
    values
}

fn normalize_natural_declaration(trimmed: &str) -> Option<String> {
    let tokens = tokenize(trimmed);
    let mut index = 0;
    while tokens.get(index).is_some_and(|token| {
        matches!(
            token.lower.as_str(),
            "a" | "an" | "the" | "public" | "private" | ","
        )
    }) {
        index += 1;
    }
    if tokens.get(index).is_some_and(|token| token.is("use")) {
        let rest = trimmed[tokens[index].end..].trim().trim_end_matches('.');
        let mut modules = Vec::new();
        for comma_piece in split_type_list(rest, ",") {
            for piece in split_type_list(&comma_piece, "and") {
                let value = piece.trim().trim_end_matches('.');
                if !value.is_empty() {
                    modules.push(value.to_string());
                }
            }
        }
        if modules.is_empty() {
            return None;
        }
        return Some(
            modules
                .into_iter()
                .map(|module| format!("Import {module}."))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    let record_keyword = tokens.get(index).map(|token| token.lower.as_str());
    if matches!(record_keyword, Some("record" | "union")) {
        let is_union = record_keyword == Some("union");
        let keyword_index = index;
        let prefix = &tokens[..keyword_index];
        let public = prefix.iter().any(|token| token.is("public"));
        let name_token = tokens
            .get(keyword_index + 1)
            .filter(|token| token.is_word())?;
        let rest_start = tokens
            .get(keyword_index + 2)
            .map_or(trimmed.len(), |token| token.start);
        let rest = &trimmed[rest_start..];
        let (before, after) = find_top_level_word(rest, "has")
            .or_else(|| find_top_level_word(rest, "holds"))
            .or_else(|| find_top_level_word(rest, "with"))?;
        let keyword = if is_union { "union" } else { "struct" };
        return Some(
            format!(
                "{} {keyword} {} {} with {}",
                if public { "Public" } else { "Private" },
                lower_name(&name_token.text),
                rest[..before].trim(),
                rest[after..].trim()
            )
            .replace("  ", " "),
        );
    }
    if tokens.get(index).is_some_and(|token| token.is("keep")) {
        let mut cursor = index + 1;
        while tokens.get(cursor).is_some_and(|token| {
            matches!(
                token.lower.as_str(),
                "a" | "an" | "the" | "public" | "private"
            )
        }) {
            cursor += 1;
        }
        if !tokens
            .get(cursor)
            .is_some_and(|token| token.is("shared") || token.is("global"))
        {
            return None;
        }
        cursor += 1;
        let name = tokens
            .get(cursor)
            .filter(|token| token.is_word())?
            .text
            .clone();
        let public = tokens[..cursor].iter().any(|token| token.is("public"));
        let as_index = tokens
            .iter()
            .position(|token| token.is("as"))
            .or_else(|| tokens.iter().position(|token| token.text == "="))?;
        let start = tokens
            .get(as_index + 1)
            .map_or(trimmed.len(), |token| token.start);
        return Some(format!(
            "{} global {} as {}",
            if public { "Public" } else { "Private" },
            lower_name(&name),
            trimmed[start..].trim()
        ));
    }
    None
}

fn normalize_natural_line(trimmed: &str) -> Option<String> {
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("say ") {
        let tokens = tokenize(trimmed);
        if let [verb, literal] = tokens.as_slice()
            && verb.is("say")
            && literal.kind == TokenKind::String
        {
            let literal = literal.text.as_str();
            return Some(format!(
                "Say bytes bytes_ptr({literal}) of length bytes_len({literal})."
            ));
        }
    }
    if lower.starts_with("give back") {
        let tokens = tokenize(trimmed);
        let start = tokens.get(2).map_or(trimmed.len(), |token| token.start);
        let rest = trimmed[start..].trim();
        return Some(if rest.is_empty() {
            "Return.".to_string()
        } else {
            format!("Return {rest}")
        });
    }
    if lower.starts_with("stop and return") {
        return Some("Return.".to_string());
    }
    if lower.starts_with("call this as") || lower.starts_with("you can call this as") {
        let values = quoted_values(trimmed);
        if values.is_empty() {
            return None;
        }
        return Some(
            values
                .into_iter()
                .map(|value| format!("Phrase: \"{value}\"."))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    normalize_natural_declaration(trimmed)
}

/// Rewrites natural-English declarations into the stable declaration shorthands.
fn v2_action_start(text: &str) -> bool {
    let word = first_word(text);
    if word.is_empty()
        || matches!(
            word.as_str(),
            "returning" | "giving" | "given" | "named" | "called" | "taking" | "of"
        )
    {
        return false;
    }
    if matches!(
        word.as_str(),
        "give"
            | "return"
            | "put"
            | "set"
            | "make"
            | "move"
            | "add"
            | "remove"
            | "stop"
            | "break"
            | "continue"
            | "let"
            | "start"
            | "claim"
            | "clear"
            | "say"
            | "ask"
            | "save"
            | "display"
            | "delete"
            | "open"
            | "close"
            | "push"
            | "append"
            | "pop"
            | "sort"
            | "free"
            | "request"
            | "find"
            | "round"
            | "square"
            | "fill"
            | "split"
            | "resize"
            | "format"
            | "print"
            | "read"
            | "join"
            | "create"
    ) {
        return true;
    }
    classify_verb(&word, None).is_some()
}

fn split_inline_function_body(text: &str) -> Option<(String, String)> {
    let tokens = tokenize(text);
    let mut depth = 0_i32;
    for token in &tokens {
        if token.kind == TokenKind::Symbol {
            match token.text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                "," if depth == 0 => {
                    let header = text[..token.start].trim();
                    let body = text[token.end..].trim();
                    let header_lower = header.to_ascii_lowercase();
                    let pending_given = (header_lower.contains(" given ")
                        || header_lower.contains(" giving "))
                        && !header_lower.contains("returning");
                    if !header.is_empty() && !pending_given && v2_action_start(body) {
                        return Some((header.to_string(), body.to_string()));
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn sentence_terminator_before(tokens: &[Token], index: usize) -> bool {
    tokens.iter().take(index).rev().any(|token| {
        token.kind == TokenKind::Symbol && matches!(token.text.as_str(), "." | ";" | "!" | "?")
    })
}

fn trailing_conditional_marker(tokens: &[Token], index: usize) -> Option<(usize, usize)> {
    let token = tokens.get(index)?;
    if token.kind != TokenKind::Word {
        return None;
    }
    if token.is("if") {
        let previous = index
            .checked_sub(1)
            .and_then(|previous| tokens.get(previous));
        if previous.is_some_and(|value| value.is("only")) {
            return previous.map(|value| (value.start, token.end));
        }
        return Some((token.start, token.end));
    }
    if token.is("provided") && tokens.get(index + 1).is_some_and(|value| value.is("that")) {
        return Some((token.start, tokens[index + 1].end));
    }
    if token.is("as")
        && tokens.get(index + 1).is_some_and(|value| value.is("long"))
        && tokens.get(index + 2).is_some_and(|value| value.is("as"))
    {
        return Some((token.start, tokens[index + 2].end));
    }
    None
}

fn split_trailing_if(text: &str) -> Option<(String, String, Option<String>)> {
    let tokens = tokenize(text);
    let mut depth = 0_i32;
    for (index, token) in tokens.iter().enumerate() {
        if token.kind == TokenKind::Symbol {
            match token.text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                _ => {}
            }
            continue;
        }
        if depth != 0 || index == 0 {
            continue;
        }
        let Some((action_end, condition_start)) = trailing_conditional_marker(&tokens, index)
        else {
            continue;
        };
        if sentence_terminator_before(&tokens, index) {
            continue;
        }
        let action = text[..action_end].trim();
        if action.is_empty() || !v2_action_start(action) {
            continue;
        }
        let suffix = &text[condition_start..];
        let suffix_tokens = tokenize(suffix);
        let mut suffix_depth = 0_i32;
        let mut otherwise = None;
        for (inner, part) in suffix_tokens.iter().enumerate() {
            if part.kind == TokenKind::Symbol {
                match part.text.as_str() {
                    "(" | "[" | "{" => suffix_depth += 1,
                    ")" | "]" | "}" => suffix_depth -= 1,
                    "," if suffix_depth == 0 => {
                        let next = suffix_tokens.get(inner + 1);
                        if next.is_some_and(|value| value.is("otherwise") || value.is("else")) {
                            otherwise = next.map(|value| (part.start, value.end));
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
        if let Some((comma, end_word)) = otherwise {
            let condition = suffix[..comma].trim();
            let alternative = suffix[end_word..].trim();
            if !condition.is_empty() && !alternative.is_empty() {
                return Some((
                    action.to_string(),
                    condition.to_string(),
                    Some(alternative.to_string()),
                ));
            }
        } else if !suffix.trim().is_empty() {
            return Some((action.to_string(), suffix.trim().to_string(), None));
        }
    }
    None
}

fn split_call_it(text: &str) -> Option<(String, String)> {
    let lower = text.to_ascii_lowercase();
    let marker = " and call it ";
    let position = lower.rfind(marker)?;
    let expression = text[..position].trim();
    let name = text[position + marker.len()..].trim();
    if expression.is_empty() || name.is_empty() || !is_simple_name(name) {
        return None;
    }
    let expression = expression
        .strip_prefix("Start at ")
        .or_else(|| expression.strip_prefix("start at "))
        .unwrap_or(expression)
        .trim()
        .to_string();
    Some((name.to_string(), expression))
}

fn split_tracking_directive(text: &str) -> Option<(String, String)> {
    let tokens = tokenize(text);
    let mut depth = 0_i32;
    for token in &tokens {
        if token.kind == TokenKind::Symbol {
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
        let directive = text[token.end..].trim();
        if crate::discourse_scope::parse_largest_tracker(directive).is_none() {
            continue;
        }
        let main = text[..token.start].trim().trim_end_matches(',').trim();
        if !main.is_empty() {
            return Some((main.to_string(), directive.to_string()));
        }
    }
    None
}

fn normalize_prose_line(text: &str, indent: usize) -> Vec<String> {
    let pieces = tc_syntax::split_sentences(text);
    if pieces.len() > 1 {
        let mut lines = Vec::new();
        for (_, piece) in pieces {
            lines.extend(normalize_prose_line(&piece, indent));
        }
        return lines;
    }
    if let Some((main, directive)) = split_tracking_directive(text) {
        let mut lines = normalize_prose_line(&main, indent);
        lines.extend(normalize_prose_line(&directive, indent));
        return lines;
    }
    let spaces = " ".repeat(indent);
    let trimmed = text.trim().trim_end_matches(['.', ',']).trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("phrase:") {
        return vec![format!("{spaces}{trimmed}.")];
    }
    if lower.starts_with("keep generating ")
        || lower.starts_with("keep going ")
        || lower.starts_with("continue generating ")
        || lower.starts_with("keep building ")
        || lower.starts_with("generate ")
        || lower.starts_with("continue ")
    {
        return vec![format!("{spaces}{trimmed}.")];
    }
    if crate::clause_graph::decompose(trimmed).is_some() {
        return vec![format!("{spaces}{trimmed}.")];
    }
    if lower.starts_with("call this as") || lower.starts_with("you can call this as") {
        return normalize_natural_line(trimmed)
            .into_iter()
            .flat_map(|value| value.lines().map(ToString::to_string).collect::<Vec<_>>())
            .map(|line| format!("{spaces}{line}"))
            .collect();
    }
    if trimmed.ends_with(':') {
        return vec![format!("{spaces}{trimmed}")];
    }
    if !lower.starts_with("until ")
        && !lower.starts_with("while ")
        && !lower.starts_with("if ")
        && !lower.starts_with("for ")
    {
        if let Some((start, end)) = find_top_level_word(trimmed, "becomes") {
            let target = trimmed[..start].trim();
            let value = trimmed[end..].trim();
            let target_lower = target.to_ascii_lowercase();
            if !target.is_empty()
                && !value.is_empty()
                && (target_lower.starts_with("its ") || target.contains("'s"))
            {
                return normalize_prose_line(&format!("Set {target} to {value}"), indent);
            }
        }
    }
    if let Some(rest) = trimmed
        .strip_prefix("Until ")
        .or_else(|| trimmed.strip_prefix("until "))
    {
        let (condition, body) = split_header_condition(rest);
        let condition = condition.trim();
        let body = body.trim();
        if !body.is_empty() {
            let mut lines = vec![format!("{spaces}While not {condition}:")];
            for action in split_actions(body) {
                lines.extend(normalize_prose_line(&action, indent + 4));
            }
            return lines;
        }
    }
    if let Some(rest) = trimmed
        .strip_prefix("While ")
        .or_else(|| trimmed.strip_prefix("while "))
    {
        let (condition, body) = split_header_condition(rest);
        let condition = condition.trim();
        let body = body.trim();
        if !body.is_empty() {
            let mut lines = vec![format!("{spaces}While {condition}:")];
            for action in split_actions(body) {
                lines.extend(normalize_prose_line(&action, indent + 4));
            }
            return lines;
        }
    }
    if let Some(rest) = trimmed
        .strip_prefix("If ")
        .or_else(|| trimmed.strip_prefix("if "))
    {
        let (condition, body) = split_header_condition(rest);
        let condition = condition.trim();
        let body = body.trim();
        if v2_action_start(body) {
            let body_tokens = tokenize(body);
            let mut otherwise = None;
            let mut depth = 0_i32;
            for (index, token) in body_tokens.iter().enumerate() {
                if token.kind == TokenKind::Symbol {
                    match token.text.as_str() {
                        "(" | "[" | "{" => depth += 1,
                        ")" | "]" | "}" => depth -= 1,
                        _ => {}
                    }
                    continue;
                }
                if depth == 0 && (token.is("otherwise") || token.is("else")) {
                    otherwise = Some((token.start, token.end));
                    break;
                }
                let _ = index;
            }
            let (then_text, else_text) = if let Some((comma, word_end)) = otherwise {
                (&body[..comma], Some(body[word_end..].trim()))
            } else {
                (body, None)
            };
            let mut lines = vec![format!("{spaces}If {condition}:")];
            for action in split_actions(then_text) {
                lines.extend(normalize_prose_line(&action, indent + 4));
            }
            if let Some(else_text) = else_text {
                lines.push(format!("{spaces}Otherwise:"));
                for action in split_actions(else_text) {
                    lines.extend(normalize_prose_line(&action, indent + 4));
                }
            }
            return lines;
        }
    }
    if let Some((action, condition, alternative)) = split_trailing_if(trimmed) {
        let mut lines = vec![format!("{spaces}If {condition}:")];
        lines.extend(normalize_prose_line(&action, indent + 4));
        if let Some(alternative) = alternative {
            lines.push(format!("{spaces}Otherwise:"));
            lines.extend(normalize_prose_line(&alternative, indent + 4));
        }
        return lines;
    }
    if let Some(rest) = trimmed
        .strip_prefix("Let ")
        .or_else(|| trimmed.strip_prefix("let "))
    {
        if let Some((name, value)) = rest.split_once(" be ") {
            let value_lower = value.to_ascii_lowercase();
            if value_lower.starts_with("block of")
                || value_lower.starts_with("fixed ")
                || value_lower.starts_with("array ")
            {
                return vec![format!("{spaces}Define local {name} as {value}.")];
            }
        }
    }
    if let Some((name, expression)) = split_call_it(trimmed) {
        return vec![format!("{spaces}Let {name} be {expression}.")];
    }
    if let Some(rest) = trimmed
        .strip_prefix("Make ")
        .or_else(|| trimmed.strip_prefix("make "))
    {
        let rest = rest.strip_prefix("be ").unwrap_or(rest);
        let tokens = tokenize(rest);
        let value_start = tokens.iter().skip(1).find_map(|token| {
            let starts_value = token.kind == TokenKind::Number
                || token.kind == TokenKind::String
                || token.text == "("
                || matches!(
                    token.lower.as_str(),
                    "it" | "true"
                        | "false"
                        | "no"
                        | "null"
                        | "square"
                        | "root"
                        | "sum"
                        | "product"
                        | "difference"
                        | "create"
                        | "address"
                );
            starts_value.then_some(token.start)
        });
        let split = value_start.or_else(|| {
            tokens
                .get(1)
                .map(|token| token.start)
                .filter(|_| tokens.len() > 1)
        });
        if let Some(split) = split {
            let target = rest[..split].trim();
            let value = rest[split..].trim();
            if !target.is_empty() && !value.is_empty() {
                return vec![format!("{spaces}Set {target} to {value}.")];
            }
        }
    }
    if let Some(rest) = trimmed
        .strip_prefix("Move ")
        .or_else(|| trimmed.strip_prefix("move "))
    {
        if let Some((start, end)) = find_top_level_word(rest, "to") {
            let target = rest[..start].trim();
            let value = rest[end..].trim();
            if is_simple_name(target) && !target.to_ascii_lowercase().contains(" from ") {
                return vec![format!("{spaces}Set {target} to {value}.")];
            }
        }
    }
    if lower.starts_with("stop with error ") {
        if let Some(message) = quoted_values(trimmed).first() {
            return vec![format!(
                "{spaces}panic(\"{}\", {}).",
                message,
                message.len()
            )];
        }
    }
    if let Some(normalized) = normalize_natural_line(trimmed) {
        return normalized
            .lines()
            .map(|line| format!("{spaces}{line}"))
            .collect();
    }
    let may_split = matches!(
        first_word(trimmed).as_str(),
        "add"
            | "remove"
            | "put"
            | "set"
            | "give"
            | "return"
            | "clear"
            | "say"
            | "display"
            | "save"
            | "claim"
            | "round"
            | "find"
            | "request"
            | "push"
            | "append"
            | "pop"
            | "sort"
            | "free"
            | "open"
            | "close"
            | "delete"
            | "start"
            | "split"
            | "resize"
            | "format"
            | "print"
            | "read"
            | "join"
            | "create"
    );
    let actions = split_actions(trimmed);
    if may_split && v2_action_start(trimmed) && actions.len() > 1 && !starts_control(trimmed) {
        let mut lines = Vec::new();
        for action in actions {
            lines.extend(normalize_prose_line(&action, indent));
        }
        return lines;
    }
    vec![format!("{spaces}{trimmed}.")]
}

#[allow(dead_code)]
fn pattern_parts_to_text(parts: &[PhrasePart]) -> String {
    parts
        .iter()
        .map(|part| match part {
            PhrasePart::Word(word) => word.clone(),
            PhrasePart::Param(name) => format!("<{name}>"),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn auto_phrase_has_safe_prefix(parts: &[PhrasePart]) -> bool {
    let literals = parts
        .iter()
        .filter_map(|part| match part {
            PhrasePart::Word(word) => Some(word.as_str()),
            PhrasePart::Param(_) => None,
        })
        .collect::<Vec<_>>();
    let Some(first) = literals.first() else {
        return false;
    };
    if matches!(
        *first,
        "set"
            | "let"
            | "put"
            | "add"
            | "remove"
            | "if"
            | "unless"
            | "while"
            | "until"
            | "repeat"
            | "for"
            | "return"
            | "give"
            | "update"
            | "make"
            | "move"
            | "start"
            | "stop"
            | "break"
            | "continue"
            | "clear"
            | "copy"
            | "ask"
    ) {
        return false;
    }
    literals.len() >= 2 || !matches!(*first, "it" | "its" | "the" | "a" | "an")
}

fn auto_phrase_patterns(text: &str, canonical_header: &str) -> Vec<String> {
    let params = parenthetical_parameters(text);
    let tokens = tokenize(text);
    let Some(to_index) = tokens.iter().position(|token| token.is("to")) else {
        return Vec::new();
    };
    if params.is_empty() && canonical_header.contains(" taking ") {
        return Vec::new();
    }
    let end = tokens
        .iter()
        .position(|token| {
            token.is("named") || token.is("called") || token.is("giving") || token.is("returning")
        })
        .unwrap_or(tokens.len());
    let mut by_start = std::collections::HashMap::new();
    for param in &params {
        by_start.insert(param.replace_start, param);
    }
    let type_span = type_parameter_span(&tokens, to_index);
    let mut parts = Vec::new();
    let mut index = to_index + 1;
    while index < end {
        if let Some((start, span_end)) = type_span {
            if index >= start && index < span_end {
                index = span_end;
                continue;
            }
        }
        if let Some(param) = by_start.get(&index) {
            if param.plural {
                for (position, name) in param.canonical_names.iter().enumerate() {
                    if position > 0 {
                        parts.push(PhrasePart::Word("and".to_string()));
                    }
                    parts.push(PhrasePart::Param(name.clone()));
                }
            } else {
                parts.push(PhrasePart::Param(param.canonical_names[0].clone()));
            }
            index = param.close_index + 1;
            continue;
        }
        let token = &tokens[index];
        if token.is_word() {
            if matches!(
                token.lower.as_str(),
                "given" | "as" | "named" | "called" | "giving" | "returning"
            ) {
                index += 1;
                continue;
            }
            parts.push(PhrasePart::Word(token.lower.clone()));
        } else if token.kind == TokenKind::Number {
            parts.push(PhrasePart::Word(token.lower.clone()));
        }
        index += 1;
    }
    if !auto_phrase_has_safe_prefix(&parts) {
        return Vec::new();
    }
    let mut patterns = vec![pattern_parts_to_text(&parts)];
    if parts
        .first()
        .is_some_and(|part| matches!(part, PhrasePart::Word(word) if word == "find"))
    {
        let drop = if matches!(parts.get(1), Some(PhrasePart::Word(word)) if word == "the") {
            2
        } else {
            1
        };
        if drop < parts.len()
            && parts[drop..]
                .iter()
                .any(|part| matches!(part, PhrasePart::Param(_)))
        {
            let alias = pattern_parts_to_text(&parts[drop..]);
            if alias != patterns[0] {
                patterns.push(alias);
            }
        }
    }
    patterns
}

#[derive(Debug, Default, Clone)]
struct NamePhrases {
    global: BTreeMap<Vec<String>, String>,
    local: BTreeMap<Vec<String>, String>,
}

impl NamePhrases {
    fn words(text: &str) -> Vec<String> {
        let mut words = tokenize(text)
            .into_iter()
            .filter(|token| token.is_word())
            .map(|token| token.lower)
            .collect::<Vec<_>>();
        if words.len() > 1 && matches!(words.first().map(String::as_str), Some("a" | "an" | "the"))
        {
            words.remove(0);
        }
        words
    }

    fn register(&mut self, text: &str, global: bool) -> Option<String> {
        let words = Self::words(text);
        if words.is_empty() {
            return None;
        }
        let canonical = words.join("_");
        if (2..=4).contains(&words.len()) {
            if global {
                self.global.insert(words, canonical.clone());
            } else {
                self.local.insert(words, canonical.clone());
            }
        }
        Some(canonical)
    }

    fn clear_local(&mut self) {
        self.local.clear();
    }

    fn rewrite(&self, text: &str) -> String {
        let tokens = tokenize(text);
        if tokens.is_empty() {
            return text.to_string();
        }
        let mut output = String::new();
        let mut cursor = 0;
        let mut index = 0;
        while index < tokens.len() {
            let token = &tokens[index];
            if token.kind != TokenKind::Word {
                index += 1;
                continue;
            }
            let mut best: Option<(usize, String)> = None;
            for length in (2..=4).rev() {
                if index + length > tokens.len() {
                    continue;
                }
                let slice = &tokens[index..index + length];
                if !slice.iter().all(|token| token.kind == TokenKind::Word) {
                    continue;
                }
                if !slice
                    .windows(2)
                    .all(|pair| text[pair[0].end..pair[1].start].trim().is_empty())
                {
                    continue;
                }
                let words = slice
                    .iter()
                    .map(|token| token.lower.clone())
                    .collect::<Vec<_>>();
                if let Some(canonical) = self.local.get(&words).or_else(|| self.global.get(&words))
                {
                    best = Some((length, canonical.clone()));
                    break;
                }
            }
            if let Some((length, canonical)) = best {
                output.push_str(&text[cursor..token.start]);
                output.push_str(&canonical);
                cursor = tokens[index + length - 1].end;
                index += length;
            } else {
                index += 1;
            }
        }
        output.push_str(&text[cursor..]);
        output
    }

    fn register_record_fields(&mut self, line: &str) {
        let tokens = tokenize(line);
        let Some(keyword) = tokens
            .iter()
            .position(|token| token.is("record") || token.is("struct") || token.is("union"))
        else {
            return;
        };
        let rest_start = tokens
            .get(keyword + 2)
            .map_or(line.len(), |token| token.start);
        let rest = &line[rest_start..];
        let Some((_, after)) = find_top_level_word(rest, "has")
            .or_else(|| find_top_level_word(rest, "holds"))
            .or_else(|| find_top_level_word(rest, "with"))
        else {
            return;
        };
        let fields = &rest[after..];
        for comma_piece in split_type_list(fields, ",") {
            let piece = comma_piece
                .trim()
                .trim_start_matches("and ")
                .trim()
                .trim_end_matches('.')
                .trim();
            if piece.is_empty() {
                continue;
            }
            let groups = tokenize(piece)
                .iter()
                .filter(|token| token.text == "(")
                .count();
            if groups > 1 || top_level_word_count(piece, "as") > 1 {
                for inner in split_type_list(piece, "and") {
                    let inner = inner.trim();
                    if inner.is_empty() {
                        continue;
                    }
                    if let Some((name, _)) = split_parenthetical_type(inner) {
                        let names = if top_level_word_count(&name, "and") > 0 {
                            split_type_list(&name, "and")
                        } else {
                            vec![name]
                        };
                        for name in names {
                            self.register(&name, true);
                        }
                    }
                }
                continue;
            }
            let Some((name, _)) = split_parenthetical_type(piece) else {
                continue;
            };
            let names = if top_level_word_count(&name, "and") > 0 {
                split_type_list(&name, "and")
            } else {
                vec![name]
            };
            for name in names {
                self.register(&name, true);
            }
        }
    }

    fn register_shared_global(&mut self, line: &str) {
        let tokens = tokenize(line);
        let Some(shared) = tokens
            .iter()
            .position(|token| token.is("shared") || token.is("global"))
        else {
            return;
        };
        let start = tokens
            .get(shared + 1)
            .map_or(line.len(), |token| token.start);
        let Some((as_start, _)) = find_top_level_word(&line[start..], "as") else {
            return;
        };
        let name = line[start..start + as_start].trim();
        if !name.is_empty() {
            self.register(name, true);
        }
    }

    fn register_let(&mut self, line: &str) {
        let trimmed = line.trim_start();
        let lower = trimmed.to_ascii_lowercase();
        if !lower.starts_with("let ") {
            return;
        }
        let rest = &trimmed[4..];
        let Some((be_start, _)) = find_top_level_word(rest, "be") else {
            return;
        };
        let name = rest[..be_start].trim();
        if !name.is_empty() {
            self.register(name, false);
        }
    }

    fn register_created(&mut self, line: &str) {
        let tokens = tokenize(line);
        let Some(marker) = tokens
            .iter()
            .position(|token| token.is("called") || token.is("named"))
        else {
            return;
        };
        let start = tokens
            .get(marker + 1)
            .map_or(line.len(), |token| token.start);
        let name = line[start..].trim().trim_end_matches('.').trim();
        if !name.is_empty() {
            self.register(name, false);
        }
    }

    fn register_function_header(&mut self, line: &str) {
        for param in parenthetical_parameters(line) {
            for original in param.original_names {
                self.register(&original, false);
            }
        }
    }

    fn register_declarations(&mut self, line: &str) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            return;
        }
        if natural_function_header(trimmed) {
            self.register_function_header(trimmed);
            return;
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("a record ")
            || lower.starts_with("a public record ")
            || lower.starts_with("a union ")
            || lower.starts_with("a public union ")
            || lower.starts_with("public record ")
            || lower.starts_with("record ")
            || lower.starts_with("public union ")
            || lower.starts_with("union ")
        {
            self.register_record_fields(trimmed);
            return;
        }
        if lower.starts_with("keep ") {
            self.register_shared_global(trimmed);
            return;
        }
        if lower.starts_with("let ") {
            self.register_let(trimmed);
            return;
        }
        if lower.starts_with("create ") || lower.starts_with("make a ") {
            self.register_created(trimmed);
        }
    }
}

fn top_level_declaration(text: &str) -> bool {
    let lower = text.trim().to_ascii_lowercase();
    lower.starts_with("use ")
        || lower.starts_with("import ")
        || lower.starts_with("a record ")
        || lower.starts_with("a public record ")
        || lower.starts_with("a union ")
        || lower.starts_with("a public union ")
        || lower.starts_with("keep ")
        || lower.starts_with("define global ")
        || lower.starts_with("define struct ")
        || lower.starts_with("publicly, to ")
        || lower.starts_with("publicly to ")
        || lower.starts_with("to ")
}

fn paragraph_control_header(text: &str, allow_colon: bool) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.ends_with(':') && !allow_colon {
        return None;
    }
    let lower = trimmed
        .trim_end_matches(['.', ':'])
        .trim()
        .to_ascii_lowercase();
    if find_top_level_symbol(trimmed, ",").is_some() {
        return None;
    }
    for prefix in ["repeat while ", "repeat until "] {
        if lower.starts_with(prefix) {
            let rest = trimmed
                .trim_end_matches(['.', ':'])
                .trim()
                .get(prefix.len()..)?
                .trim();
            return Some(if prefix == "repeat while " {
                format!("While {rest}")
            } else {
                format!("While not {rest}")
            });
        }
    }
    if lower.starts_with("until ") {
        let rest = trimmed
            .trim_end_matches(['.', ':'])
            .trim()
            .get("until ".len()..)?
            .trim();
        return Some(format!("While not {rest}"));
    }
    (lower.starts_with("while ")
        || lower.starts_with("if ")
        || lower.starts_with("unless ")
        || lower.starts_with("for "))
    .then(|| trimmed.trim_end_matches(['.', ':']).trim().to_string())
}

fn next_nonblank_indent(lines: &[&str], index: usize) -> Option<usize> {
    lines
        .iter()
        .skip(index + 1)
        .find(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
}

fn normalize_natural_source(text: &str) -> (String, Vec<u32>) {
    let mut output = String::new();
    let mut line_map = Vec::new();
    let mut natural_function: Option<usize> = None;
    let mut control_blocks: Vec<usize> = Vec::new();
    let mut names = NamePhrases::default();
    let lines = text.lines().collect::<Vec<_>>();
    for (index, raw_line) in lines.iter().copied().enumerate() {
        let source_line = u32::try_from(index + 1).unwrap_or(u32::MAX);
        let indent = raw_line.len() - raw_line.trim_start().len();
        let trimmed = raw_line.trim_start();
        let allow_colon = next_nonblank_indent(&lines, index).is_none_or(|next| next <= indent);
        if trimmed.is_empty() {
            control_blocks.clear();
            natural_function = None;
            names.clear_local();
            output.push('\n');
            line_map.push(source_line);
            continue;
        }
        if trimmed
            .trim_end_matches('.')
            .trim()
            .eq_ignore_ascii_case("done")
            || trimmed
                .trim_end_matches('.')
                .trim()
                .eq_ignore_ascii_case("end")
        {
            control_blocks.clear();
            natural_function = None;
            names.clear_local();
            continue;
        }
        if matches!(
            trimmed
                .trim_end_matches('.')
                .trim()
                .to_ascii_lowercase()
                .as_str(),
            "end the loop" | "end the condition"
        ) && !control_blocks.is_empty()
        {
            control_blocks.pop();
            continue;
        }
        if natural_function.is_some() && top_level_declaration(trimmed) {
            natural_function = None;
            control_blocks.clear();
            names.clear_local();
        }
        if natural_function_header(trimmed) {
            names.clear_local();
        }
        names.register_declarations(trimmed);
        let rewritten = names.rewrite(trimmed);
        let trimmed = rewritten.as_str();
        if let Some(&control_indent) = control_blocks.last() {
            if let Some(header) = paragraph_control_header(trimmed, allow_colon) {
                push_normalized_line(
                    &mut output,
                    &mut line_map,
                    &format!("{}{}:", " ".repeat(control_indent + 4), header),
                    source_line,
                );
                control_blocks.push(control_indent + 4);
                continue;
            }
            let effective = control_indent + 4 + indent;
            for line in normalize_prose_line(trimmed, effective) {
                push_normalized_line(&mut output, &mut line_map, &line, source_line);
            }
            continue;
        }
        if let Some(header_indent) = natural_function {
            if let Some(header) = paragraph_control_header(trimmed, allow_colon) {
                push_normalized_line(
                    &mut output,
                    &mut line_map,
                    &format!("{}{}:", " ".repeat(header_indent + 4), header),
                    source_line,
                );
                control_blocks.push(header_indent + 4);
                continue;
            }
            let effective = header_indent + 4 + indent;
            for line in normalize_prose_line(trimmed, effective) {
                push_normalized_line(&mut output, &mut line_map, &line, source_line);
            }
            continue;
        }
        if natural_function_header(trimmed) {
            let (header_text, inline_body) = split_inline_function_body(trimmed)
                .map_or((trimmed.to_string(), None), |(header, body)| {
                    (header, Some(body))
                });
            if let Ok(header) = normalize_function_header(&header_text, "main", Span::default()) {
                push_normalized_line(
                    &mut output,
                    &mut line_map,
                    &format!("{}{}", " ".repeat(indent), header),
                    source_line,
                );
                for pattern in auto_phrase_patterns(&header_text, &header) {
                    push_normalized_line(
                        &mut output,
                        &mut line_map,
                        &format!("{}Phrase: \"{}\".", " ".repeat(indent + 4), pattern),
                        source_line,
                    );
                }
                if let Some(body) = inline_body {
                    for line in normalize_prose_line(&body, indent + 4) {
                        push_normalized_line(&mut output, &mut line_map, &line, source_line);
                    }
                } else {
                    natural_function = Some(indent);
                }
                continue;
            }
        }
        if let Some(header) = paragraph_control_header(trimmed, allow_colon) {
            push_normalized_line(
                &mut output,
                &mut line_map,
                &format!("{}{}:", " ".repeat(indent), header),
                source_line,
            );
            control_blocks.push(indent);
            continue;
        }
        for line in normalize_prose_line(trimmed, indent) {
            push_normalized_line(&mut output, &mut line_map, &line, source_line);
        }
    }
    (output, line_map)
}

fn push_normalized_line(
    output: &mut String,
    line_map: &mut Vec<u32>,
    line: &str,
    source_line: u32,
) {
    output.push_str(line);
    output.push('\n');
    line_map.push(source_line);
}

fn remap_document_lines(document: &mut crate::RawDocument, line_map: &[u32]) {
    remap_item_lines(&mut document.items, line_map);
}

fn remap_item_lines(items: &mut [RawItem], line_map: &[u32]) {
    for item in items {
        match item {
            RawItem::Sentence(sentence) => remap_span_line(&mut sentence.span, line_map),
            RawItem::Block(block) => {
                if let Some(header) = &mut block.header {
                    remap_span_line(&mut header.span, line_map);
                }
                remap_span_line(&mut block.span, line_map);
                remap_item_lines(&mut block.items, line_map);
            }
        }
    }
}

fn remap_span_line(span: &mut Span, line_map: &[u32]) {
    if let Some(line) = span
        .line
        .checked_sub(1)
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| line_map.get(index))
    {
        span.line = *line;
    }
}

/// Runs the declaration pass over one segmented module.
pub(crate) fn parse_module(source: &SourceFile) -> Result<ModuleJob, Vec<SemanticError>> {
    let module_name = source.module_name();
    let (normalized_source, line_map) = normalize_natural_source(&source.text);
    let normalized = SourceFile::new(source.name.clone(), normalized_source);
    let mut document = segment_source(&normalized);
    remap_document_lines(&mut document, &line_map);
    let mut ast = AstModule {
        name: module_name.clone(),
        imports: Vec::new(),
        structs: Vec::new(),
        unions: Vec::new(),
        globals: Vec::new(),
        functions: Vec::new(),
    };
    let mut functions = Vec::new();
    let mut executable = Vec::new();
    let mut errors = Vec::new();
    for item in document.items {
        match item {
            RawItem::Sentence(sentence) => {
                let Some(kind) = declaration_kind(&sentence.text) else {
                    executable.push(RawItem::Sentence(sentence));
                    continue;
                };
                let span = sentence.span;
                match kind {
                    "import" => match parse_import(&sentence.text, &module_name, span) {
                        Ok(value) => ast.imports.push(value),
                        Err(error) => errors.push(error),
                    },
                    "global" => match parse_global(&sentence.text, &module_name, span) {
                        Ok(value) => ast.globals.push(value),
                        Err(error) => errors.push(error),
                    },
                    "struct" | "union" => {
                        let is_union = kind == "union";
                        match parse_struct_header(&sentence.text, &module_name, span, is_union) {
                            Ok((name, type_params, fields)) => {
                                if is_union {
                                    ast.unions.push(AstUnion {
                                        vis: parse_visibility(&sentence.text),
                                        name,
                                        type_params,
                                        fields,
                                        span: type_span(&module_name, span),
                                    });
                                } else {
                                    ast.structs.push(AstStruct {
                                        vis: parse_visibility(&sentence.text),
                                        name,
                                        type_params,
                                        fields,
                                        span: type_span(&module_name, span),
                                    });
                                }
                            }
                            Err(error) => errors.push(error),
                        }
                    }
                    "function" => match parse_function_header(&sentence.text, &module_name, span) {
                        Ok(function) => {
                            ast.functions.push(function);
                            functions.push(FunctionJob {
                                index: ast.functions.len() - 1,
                                body: Vec::new(),
                            });
                        }
                        Err(error) => errors.push(error),
                    },
                    _ => executable.push(RawItem::Sentence(sentence)),
                }
            }
            RawItem::Block(block) => {
                let Some(header) = &block.header else {
                    executable.push(RawItem::Block(block));
                    continue;
                };
                let Some(kind) = declaration_kind(&header.text) else {
                    executable.push(RawItem::Block(block));
                    continue;
                };
                let span = header.span;
                match kind {
                    "struct" | "union" => {
                        let is_union = kind == "union";
                        match parse_struct_header(&header.text, &module_name, span, is_union) {
                            Ok((name, type_params, mut fields)) => {
                                for item in &block.items {
                                    if let RawItem::Sentence(field) = item {
                                        match parse_fields(&field.text, &module_name, field.span) {
                                            Ok(mut more) => fields.append(&mut more),
                                            Err(error) => errors.push(error),
                                        }
                                    }
                                }
                                if is_union {
                                    ast.unions.push(AstUnion {
                                        vis: parse_visibility(&header.text),
                                        name,
                                        type_params,
                                        fields,
                                        span: type_span(&module_name, span),
                                    });
                                } else {
                                    ast.structs.push(AstStruct {
                                        vis: parse_visibility(&header.text),
                                        name,
                                        type_params,
                                        fields,
                                        span: type_span(&module_name, span),
                                    });
                                }
                            }
                            Err(error) => errors.push(error),
                        }
                    }
                    "function" => match parse_function_header(&header.text, &module_name, span) {
                        Ok(mut function) => {
                            let mut body = block.items;
                            match extract_phrase_lines(&mut body, &module_name, span) {
                                Ok(patterns) => function.phrases = patterns,
                                Err(error) => errors.push(error),
                            }
                            ast.functions.push(function);
                            functions.push(FunctionJob {
                                index: ast.functions.len() - 1,
                                body,
                            });
                        }
                        Err(error) => errors.push(error),
                    },
                    "global" => match parse_global(&header.text, &module_name, span) {
                        Ok(value) => ast.globals.push(value),
                        Err(error) => errors.push(error),
                    },
                    _ => executable.push(RawItem::Block(block)),
                }
            }
        }
    }
    errors.extend(reserved_declaration_errors(&module_name, &ast));
    if errors.is_empty() {
        Ok(ModuleJob {
            module: ast,
            functions,
            executable,
        })
    } else {
        Err(errors)
    }
}

fn reserved_declaration_errors(module: &str, ast: &AstModule) -> Vec<SemanticError> {
    let mut errors = Vec::new();
    let mut check = |span: &tc_typeck::Span, name: &str, label: &str, example: &str| {
        let lower = name.to_ascii_lowercase();
        if crate::suggest::is_reserved_control_word(&lower) {
            errors.push(semantic_error(
                module,
                Span::new(span.line, span.col),
                SemanticErrorKind::Declaration,
                format!(
                    "'{lower}' is a reserved control word — choose a different {label} name (for example, '{example}')"
                ),
            ));
        }
    };
    for definition in &ast.structs {
        check(&definition.span, &definition.name, "record", "Waypoint");
        for (field, _) in &definition.fields {
            check(&definition.span, field, "field", "distance");
        }
    }
    for definition in &ast.unions {
        check(&definition.span, &definition.name, "union", "Payload");
        for (field, _) in &definition.fields {
            check(&definition.span, field, "field", "distance");
        }
    }
    for global in &ast.globals {
        check(&global.span, &global.name, "variable", "counter");
    }
    errors
}

fn parse_global(text: &str, module: &str, span: Span) -> Result<AstGlobal, SemanticError> {
    let tokens = tokenize(text);
    let mut index = 0;
    while tokens
        .get(index)
        .is_some_and(|token| token.is("define") || token.is("public") || token.is("private"))
    {
        index += 1;
    }
    if !tokens.get(index).is_some_and(|token| token.is("global")) {
        return Err(semantic_error(
            module,
            span,
            SemanticErrorKind::Declaration,
            "expected `global`",
        ));
    }
    index += 1;
    let name = tokens
        .get(index)
        .filter(|token| token.is_word())
        .map(|token| lower_name(&token.text))
        .ok_or_else(|| {
            semantic_error(
                module,
                span,
                SemanticErrorKind::Declaration,
                "global name is missing",
            )
        })?;
    index += 1;
    if !tokens.get(index).is_some_and(|token| token.is("as")) {
        return Err(semantic_error(
            module,
            span,
            SemanticErrorKind::Declaration,
            "global requires `as <type>`",
        ));
    }
    index += 1;
    let start = tokens.get(index).map_or(text.len(), |token| token.start);
    let ty = parse_type_text(&text[start..])
        .map_err(|message| semantic_error(module, span, SemanticErrorKind::Declaration, message))?;
    Ok(AstGlobal {
        vis: parse_visibility(text),
        name,
        ty,
        init: None,
        span: type_span(module, span),
    })
}

/// Builds the per-module phrase scope: own functions plus imported public ones.
#[must_use]
pub(crate) fn collect_phrase_bindings(modules: &[ModuleJob]) -> Vec<Vec<PhraseBinding>> {
    let by_name = modules
        .iter()
        .enumerate()
        .map(|(index, module)| (module.module.name.clone(), index))
        .collect::<HashMap<_, _>>();
    let mut result = Vec::with_capacity(modules.len());
    for module in modules {
        let mut bindings = Vec::new();
        for function in &module.module.functions {
            for pattern in &function.phrases {
                bindings.push(PhraseBinding {
                    module: module.module.name.clone(),
                    function: function.name.clone(),
                    vis: function.vis,
                    pattern: pattern.clone(),
                    param_names: function
                        .params
                        .iter()
                        .map(|(name, _)| name.clone())
                        .collect(),
                    params: function.params.iter().map(|(_, ty)| ty.clone()).collect(),
                    type_params: function.type_params.clone(),
                });
            }
        }
        for import in &module.module.imports {
            let Some(index) = by_name.get(&import.module) else {
                continue;
            };
            for function in &modules[*index].module.functions {
                if function.vis != Visibility::Public {
                    continue;
                }
                for pattern in &function.phrases {
                    bindings.push(PhraseBinding {
                        module: modules[*index].module.name.clone(),
                        function: function.name.clone(),
                        vis: Visibility::Public,
                        pattern: pattern.clone(),
                        param_names: function
                            .params
                            .iter()
                            .map(|(name, _)| name.clone())
                            .collect(),
                        params: function.params.iter().map(|(_, ty)| ty.clone()).collect(),
                        type_params: function.type_params.clone(),
                    });
                }
            }
        }
        result.push(bindings);
    }
    result
}

#[derive(Debug, Clone)]
struct Scope {
    vars: HashMap<String, AstType>,
}

impl Scope {
    fn new() -> Self {
        Self {
            vars: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Target {
    Local(String),
    Global(String),
    Memory(AstExpr),
}

impl Target {
    pub(crate) fn read(&self) -> AstExpr {
        match self {
            Self::Local(name) => AstExpr::Var(name.clone()),
            Self::Global(name) => AstExpr::Load(Box::new(AstExpr::GlobalAddr {
                module: None,
                name: name.clone(),
            })),
            Self::Memory(address) => AstExpr::Load(Box::new(address.clone())),
        }
    }
}

#[derive(Debug, Clone)]
struct CheckRecord {
    matched_var: String,
    kind: CheckKind,
}

#[derive(Debug, Clone)]
struct ActiveTracker {
    name: String,
    maximum: bool,
}

#[derive(Debug, Clone)]
struct AdjacentContext {
    list: String,
    index: String,
    left: String,
    right: String,
    element: AstType,
}

pub(crate) struct Lowerer<'a> {
    parser: &'a crate::LinkGrammarParser,
    module: String,
    globals: HashMap<String, AstType>,
    types: HashSet<String>,
    aggregates: HashMap<String, AggregateInfo>,
    module_names: Vec<String>,
    phrases: Vec<PhraseBinding>,
    function_returns: HashMap<String, Option<AstType>>,
    scopes: Vec<Scope>,
    discourse: DiscourseStack,
    paraphrase_entries: Vec<ParaphraseEntry>,
    active_entry: Option<usize>,
    synthetic_depth: usize,
    trace: Vec<SentenceTrace>,
    errors: Vec<SemanticError>,
    temp_counter: usize,
    loop_depth: usize,
    bytes_lengths: HashMap<String, usize>,
    active_loop_var: Option<String>,
    last_check: Option<CheckRecord>,
    implicit_counter: Option<String>,
    largest_tracker: Option<ActiveTracker>,
    print_tracker: Option<ActiveTracker>,
    running_total: Option<String>,
    named_counters: HashMap<String, String>,
    move_floor: Option<String>,
    last_iteration_var: Option<String>,
    last_list: Option<String>,
    adjacent: Option<AdjacentContext>,
}

impl<'a> Lowerer<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        parser: &'a crate::LinkGrammarParser,
        module: &str,
        globals: HashMap<String, AstType>,
        types: HashSet<String>,
        aggregates: HashMap<String, AggregateInfo>,
        module_names: Vec<String>,
        phrases: Vec<PhraseBinding>,
        function_returns: HashMap<String, Option<AstType>>,
    ) -> Self {
        Self {
            parser,
            module: module.to_string(),
            globals,
            types,
            aggregates,
            module_names,
            phrases,
            function_returns,
            scopes: vec![Scope::new()],
            discourse: DiscourseStack::new(),
            paraphrase_entries: Vec::new(),
            active_entry: None,
            synthetic_depth: 0,
            trace: Vec::new(),
            errors: Vec::new(),
            temp_counter: 0,
            loop_depth: 0,
            bytes_lengths: HashMap::new(),
            active_loop_var: None,
            last_check: None,
            implicit_counter: None,
            largest_tracker: None,
            print_tracker: None,
            running_total: None,
            named_counters: HashMap::new(),
            move_floor: None,
            last_iteration_var: None,
            last_list: None,
            adjacent: None,
        }
    }

    pub(crate) fn span(&self, span: Span) -> tc_typeck::Span {
        type_span(&self.module, span)
    }

    pub(crate) fn error(
        &self,
        span: Span,
        kind: SemanticErrorKind,
        message: impl Into<String>,
    ) -> SemanticError {
        semantic_error(&self.module, span, kind, message.into())
    }

    fn push_scope(&mut self, kind: ScopeKind) {
        self.scopes.push(Scope::new());
        self.discourse.push_scope(kind);
    }

    fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
        self.discourse.pop_scope();
    }

    fn declare(&mut self, name: &str, ty: AstType) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.vars.insert(name.to_string(), ty);
        }
    }

    pub(crate) fn declare_temp(&mut self, name: &str, ty: AstType) {
        self.declare(name, ty);
    }

    fn referent_meta(&self, ty: &AstType) -> (ReferentKind, Option<String>, bool) {
        match ty {
            AstType::Named { name, .. } if self.types.contains(name) => {
                (ReferentKind::Aggregate, Some(name.clone()), false)
            }
            AstType::Ptr(inner) => match inner.as_ref() {
                AstType::Named { name, .. } if self.types.contains(name) => {
                    (ReferentKind::Aggregate, Some(name.clone()), true)
                }
                _ => (ReferentKind::Scalar, None, false),
            },
            AstType::Array(..) => (ReferentKind::Array, None, false),
            _ => (ReferentKind::Scalar, None, false),
        }
    }

    fn introduce_name(&mut self, name: &str, ty: &AstType, span: Span) {
        let (kind, type_name, is_ptr) = self.referent_meta(ty);
        self.discourse
            .introduce(name, Some(ty.clone()), type_name, is_ptr, kind, span);
    }

    fn touch_name(&mut self, name: &str, span: Span) {
        if self.discourse.touch(name) {
            return;
        }
        if let Some(ty) = self.globals.get(name).cloned() {
            self.introduce_name(name, &ty, span);
        }
    }

    pub(crate) fn touch_target_text(&mut self, text: &str, span: Span) {
        let canonical = canonical_assignment_target(text);
        if let Some(token) = tokenize(&canonical)
            .into_iter()
            .find(|token| token.kind == TokenKind::Word)
        {
            let name = token.lower.clone();
            if self.lookup(&name).is_some() || self.globals.contains_key(&name) {
                self.touch_name(&name, span);
            }
        }
    }

    fn push_paraphrase(
        &mut self,
        sentence: &RawSentence,
        verdict: IntentVerdict,
        canonical: String,
    ) {
        let entry = ParaphraseEntry {
            span: sentence.span,
            original: sentence.text.clone(),
            verdict,
            canonical,
            resolutions: Vec::new(),
        };
        self.paraphrase_entries.push(entry);
        self.active_entry = Some(self.paraphrase_entries.len() - 1);
    }

    fn add_annotation(&mut self, annotation: String) {
        if let Some(index) = self.active_entry {
            if let Some(entry) = self.paraphrase_entries.get_mut(index) {
                if !entry.resolutions.contains(&annotation) {
                    entry.resolutions.push(annotation);
                }
            }
        }
    }

    fn set_canonical(&mut self, canonical: String) {
        if self.synthetic_depth > 0 {
            return;
        }
        if let Some(index) = self.active_entry {
            if let Some(entry) = self.paraphrase_entries.get_mut(index) {
                entry.canonical = canonical;
            }
        }
    }

    fn anaphor_error(&self, error: &crate::discourse::AnaphorError, span: Span) -> SemanticError {
        let _ = span;
        self.error(span, SemanticErrorKind::Lowering, error.to_string())
    }

    fn resolve_it(&mut self, span: Span) -> Result<ResolvedAnaphor, SemanticError> {
        match self.discourse.resolve_it(span) {
            Ok(resolved) => {
                self.add_annotation(resolved.annotation.clone());
                Ok(resolved)
            }
            Err(error) => Err(self.anaphor_error(&error, span)),
        }
    }

    fn resolve_its_field(
        &mut self,
        field: &str,
        span: Span,
    ) -> Result<ResolvedFieldAnaphor, SemanticError> {
        match self
            .discourse
            .resolve_its_field(field, &self.aggregates, span)
        {
            Ok(resolved) => {
                self.add_annotation(resolved.annotation.clone());
                Ok(resolved)
            }
            Err(error) => Err(self.anaphor_error(&error, span)),
        }
    }

    fn resolve_definite(
        &mut self,
        type_word: &str,
        span: Span,
    ) -> Result<ResolvedAnaphor, SemanticError> {
        match self.discourse.resolve_definite_type(type_word, span) {
            Ok(resolved) => {
                self.add_annotation(resolved.annotation.clone());
                Ok(resolved)
            }
            Err(error) => Err(self.anaphor_error(&error, span)),
        }
    }

    fn resolve_appositive(
        &mut self,
        type_word: &str,
        var_name: &str,
        span: Span,
    ) -> Result<ResolvedAnaphor, SemanticError> {
        match self.discourse.resolve_appositive(type_word, var_name, span) {
            Ok(resolved) => {
                self.add_annotation(resolved.annotation.clone());
                Ok(resolved)
            }
            Err(error) => Err(self.anaphor_error(&error, span)),
        }
    }

    fn is_declared_type(&self, word: &str) -> bool {
        self.types.contains(word)
    }

    fn token_at(tokens: &[Token], index: usize) -> Option<&Token> {
        tokens.get(index)
    }

    /// Rewrites `it`, `its <field>`, `the <type>` and appositive noun phrases
    /// into canonical variable references, recording each resolution.
    fn rewrite_known_names(&self, text: &str) -> String {
        let mut names = self
            .scopes
            .iter()
            .flat_map(|scope| scope.vars.keys())
            .chain(self.globals.keys())
            .filter(|name| name.contains('_'))
            .cloned()
            .collect::<Vec<_>>();
        names.sort_by(|left, right| {
            right
                .matches('_')
                .count()
                .cmp(&left.matches('_').count())
                .then_with(|| right.len().cmp(&left.len()))
        });
        names.dedup();
        if names.is_empty() {
            return text.to_string();
        }
        let mappings = names
            .into_iter()
            .filter_map(|name| {
                let words = name.split('_').map(ToString::to_string).collect::<Vec<_>>();
                (2..=4).contains(&words.len()).then_some((words, name))
            })
            .collect::<Vec<_>>();
        if mappings.is_empty() {
            return text.to_string();
        }
        let tokens = tokenize(text);
        let mut output = String::new();
        let mut cursor = 0;
        let mut index = 0;
        while index < tokens.len() {
            let token = &tokens[index];
            if token.kind != TokenKind::Word {
                index += 1;
                continue;
            }
            let mut best: Option<(usize, String)> = None;
            for (words, canonical) in &mappings {
                let length = words.len();
                if index + length > tokens.len() {
                    continue;
                }
                let slice = &tokens[index..index + length];
                if !slice
                    .iter()
                    .all(|candidate| candidate.kind == TokenKind::Word)
                    || !slice
                        .windows(2)
                        .all(|pair| text[pair[0].end..pair[1].start].trim().is_empty())
                {
                    continue;
                }
                if slice
                    .iter()
                    .map(|candidate| candidate.lower.as_str())
                    .eq(words.iter().map(String::as_str))
                {
                    best = Some((length, canonical.clone()));
                    break;
                }
            }
            if let Some((length, canonical)) = best {
                output.push_str(&text[cursor..token.start]);
                output.push_str(&canonical);
                cursor = tokens[index + length - 1].end;
                index += length;
            } else {
                index += 1;
            }
        }
        output.push_str(&text[cursor..]);
        output
    }

    fn replace_value_referent_phrases(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<String, SemanticError> {
        let mut current = text.to_string();
        loop {
            let mut replaced = false;
            if let Some(context) = self.adjacent.clone() {
                for (phrase, value) in [
                    ("the left number", &context.left),
                    ("the left one", &context.left),
                    ("the left value", &context.left),
                    ("the right number", &context.right),
                    ("the right one", &context.right),
                    ("the right value", &context.right),
                ] {
                    if let Some((start, end)) =
                        crate::textutil::find_top_level_phrase(&current, phrase)
                    {
                        current = format!("{}{}{}", &current[..start], value, &current[end..]);
                        self.touch_name(value, span);
                        replaced = true;
                        break;
                    }
                }
            }
            if replaced {
                continue;
            }
            if let Some(list) = self.last_list.clone() {
                for phrase in [
                    "the remaining numbers in order",
                    "the remaining items in order",
                    "the remaining values in order",
                    "the numbers in order",
                    "the items in order",
                    "the values in order",
                    "the remaining numbers",
                    "the remaining items",
                    "the remaining values",
                    "the numbers that remain",
                    "the items that remain",
                ] {
                    if let Some((start, end)) =
                        crate::textutil::find_top_level_phrase(&current, phrase)
                    {
                        current = format!("{}{}{}", &current[..start], list, &current[end..]);
                        self.touch_name(&list, span);
                        replaced = true;
                        break;
                    }
                }
            }
            if replaced {
                continue;
            }
            if let Some(var) = self.last_iteration_var.clone() {
                for phrase in [
                    "the stopping number",
                    "that stopping number",
                    "the stop number",
                    "the number where you stopped",
                    "the number at which you stopped",
                ] {
                    if let Some((start, end)) =
                        crate::textutil::find_top_level_phrase(&current, phrase)
                    {
                        current = format!("{}{}{}", &current[..start], var, &current[end..]);
                        self.touch_name(&var, span);
                        replaced = true;
                        break;
                    }
                }
            }
            if replaced {
                continue;
            }
            if let Some(total) = self.running_total.clone() {
                for phrase in [
                    "the final running total",
                    "the running total",
                    "a running total",
                    "that running total",
                    "the final total",
                    "that total",
                    "the running sum",
                    "a running sum",
                ] {
                    if let Some((start, end)) =
                        crate::textutil::find_top_level_phrase(&current, phrase)
                    {
                        current = format!("{}{}{}", &current[..start], total, &current[end..]);
                        replaced = true;
                        break;
                    }
                }
            }
            if replaced {
                continue;
            }
            for phrase in [
                "that largest value",
                "the largest value",
                "the largest one",
                "that maximum value",
                "the maximum value",
                "the biggest value",
                "that biggest value",
                "that smallest value",
                "the smallest value",
                "the minimum value",
                "that minimum value",
            ] {
                if let Some((start, end)) = crate::textutil::find_top_level_phrase(&current, phrase)
                {
                    let Some(tracker) = self.largest_tracker.clone() else {
                        return Err(self.error(
                            span,
                            SemanticErrorKind::Lowering,
                            format!(
                                "UNRESOLVED_ANAPHOR: '{phrase}' has no preceding `keep track of the largest`"
                            ),
                        ));
                    };
                    current = format!("{}{}{}", &current[..start], tracker.name, &current[end..]);
                    self.touch_name(&tracker.name, span);
                    replaced = true;
                    break;
                }
            }
            if replaced {
                continue;
            }
            for phrase in [
                "the final value",
                "the resulting value",
                "the new value",
                "the transformed value",
                "the updated value",
                "the current value",
                "the final number",
                "the final result",
            ] {
                if let Some((start, end)) = crate::textutil::find_top_level_phrase(&current, phrase)
                {
                    let Some(target) = self
                        .active_loop_var
                        .clone()
                        .or_else(|| self.discourse.last_result())
                    else {
                        return Err(self.error(
                            span,
                            SemanticErrorKind::Lowering,
                            format!(
                                "UNRESOLVED_ANAPHOR: '{phrase}' has no active loop value in scope"
                            ),
                        ));
                    };
                    current = format!("{}{}{}", &current[..start], target, &current[end..]);
                    self.touch_name(&target, span);
                    replaced = true;
                    break;
                }
            }
            if !replaced {
                break;
            }
        }
        Ok(current)
    }

    fn resolve_anaphors(&mut self, text: &str, span: Span) -> Result<String, SemanticError> {
        let rewritten = self.replace_value_referent_phrases(text, span)?;
        let rewritten = self.rewrite_known_names(&rewritten);
        let text = rewritten.as_str();
        let tokens = tokenize(text);
        let mut replacements: Vec<(usize, usize, String)> = Vec::new();
        let mut index = 0;
        while index < tokens.len() {
            let token = &tokens[index];
            if token.kind != TokenKind::Word {
                index += 1;
                continue;
            }
            if token.is("it") {
                let negation = Self::token_at(&tokens, index + 1)
                    .is_some_and(|value| value.is("is"))
                    && Self::token_at(&tokens, index + 2).is_some_and(|value| value.is("false"))
                    && Self::token_at(&tokens, index + 3).is_some_and(|value| value.is("that"));
                if negation {
                    index += 1;
                    continue;
                }
                let resolved = self.resolve_it(span)?;
                replacements.push((token.start, token.end, resolved.target_var));
                index += 1;
                continue;
            }
            if token.is("its") {
                let Some(field) =
                    Self::token_at(&tokens, index + 1).filter(|value| value.is_word())
                else {
                    return Err(self.error(
                        span,
                        SemanticErrorKind::Lowering,
                        "UNRESOLVED_ANAPHOR: 'its' requires a field name (e.g. `its x`)",
                    ));
                };
                let resolved = self.resolve_its_field(&field.lower, span)?;
                let _ = resolved.is_ptr;
                replacements.push((
                    token.start,
                    field.end,
                    format!("{}.{}", resolved.base_var, resolved.field),
                ));
                index += 2;
                continue;
            }
            if token.is("the") {
                let Some(next) = Self::token_at(&tokens, index + 1) else {
                    index += 1;
                    continue;
                };
                if next.kind != TokenKind::Word {
                    index += 1;
                    continue;
                }
                let type_word = next.lower.clone();
                if type_word == "result"
                    && self.lookup("result").is_none()
                    && !self.globals.contains_key("result")
                    && let Some(last) = self.discourse.last_result()
                {
                    replacements.push((token.start, next.end, last));
                    index += 2;
                    continue;
                }
                if self.is_declared_type(&type_word) {
                    let after = Self::token_at(&tokens, index + 2);
                    let appositive_name = after
                        .filter(|value| value.is_word())
                        .map(|value| value.lower.clone());
                    let appositive_followed_by_field = appositive_name.as_ref().is_some_and(|_| {
                        Self::token_at(&tokens, index + 3)
                            .is_some_and(|value| value.text == "'s" || value.text == ".")
                    });
                    let appositive_at_end = appositive_name.is_some()
                        && Self::token_at(&tokens, index + 3).is_none_or(|value| {
                            value.kind == TokenKind::Symbol
                                && matches!(value.text.as_str(), "," | "." | ":" | ";")
                        });
                    if appositive_followed_by_field || appositive_at_end {
                        let var_name = appositive_name.unwrap_or_default();
                        let resolved = self.resolve_appositive(&type_word, &var_name, span)?;
                        replacements.push((
                            token.start,
                            after.map_or(next.end, |value| value.end),
                            resolved.target_var,
                        ));
                        index += 3;
                        continue;
                    }
                    let resolved = self.resolve_definite(&type_word, span)?;
                    replacements.push((token.start, next.end, resolved.target_var));
                    index += 2;
                    continue;
                }
                if Self::token_at(&tokens, index + 2)
                    .is_some_and(|value| value.text == "'s" || value.text == ".")
                {
                    let resolved = self.resolve_definite(&type_word, span)?;
                    replacements.push((token.start, next.end, resolved.target_var));
                    index += 2;
                    continue;
                }
                // A definite article before an ordinary identifier is simply
                // dropped: `the salary` denotes the variable `salary`.
                replacements.push((token.start, token.end, String::new()));
                index += 1;
                continue;
            }
            index += 1;
        }
        if replacements.is_empty() {
            return Ok(text.trim().to_string());
        }
        replacements.sort_by_key(|(start, _, _)| *start);
        let mut output = String::with_capacity(text.len());
        let mut cursor = 0;
        for (start, end, replacement) in replacements {
            if start < cursor {
                continue;
            }
            output.push_str(&text[cursor..start]);
            output.push_str(&replacement);
            cursor = end;
        }
        output.push_str(&text[cursor..]);
        Ok(output.trim().to_string())
    }

    fn lookup(&self, name: &str) -> Option<&AstType> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.vars.get(name))
            .or_else(|| self.globals.get(name))
    }

    fn current_scope_contains(&self, name: &str) -> bool {
        self.scopes
            .last()
            .is_some_and(|scope| scope.vars.contains_key(name))
    }

    fn expr_env(&self) -> ExprEnv {
        let mut locals = HashMap::new();
        for scope in &self.scopes {
            for (name, ty) in &scope.vars {
                locals.insert(name.clone(), ty.clone());
            }
        }
        ExprEnv {
            locals,
            globals: self.globals.clone(),
            aggregates: self.aggregates.clone(),
            modules: self.module_names.clone(),
        }
    }

    fn rewrite_number_words(&self, text: &str) -> String {
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

    fn parse_expr(&mut self, text: &str, span: Span) -> Result<AstExpr, SemanticError> {
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

    fn expression_name_hint(&self, text: &str) -> String {
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

    fn parse_expr_raw(&self, text: &str, span: Span) -> Result<AstExpr, SemanticError> {
        let rewritten = self.rewrite_number_words(&self.rewrite_known_names(text));
        parse_expression(&rewritten, &self.expr_env())
            .map_err(|message| self.error(span, SemanticErrorKind::Lowering, message))
    }

    fn phrase_expression(
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

    pub(crate) fn parse_value(&mut self, text: &str, span: Span) -> Result<AstExpr, SemanticError> {
        let normalized = crate::number_words::normalize_hyphens(text);
        let trimmed = normalized.trim();
        let lower = trimmed.to_ascii_lowercase();
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
        crate::value::parse_value(self, text, span)
    }

    fn lower_square_root(&mut self, text: &str, span: Span) -> Result<AstExpr, SemanticError> {
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

    fn lower_trim_value(&mut self, text: &str, span: Span) -> Result<AstExpr, SemanticError> {
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

    fn text_bytes(&self, value: &AstExpr) -> Option<(AstExpr, AstExpr)> {
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

    fn structural_compatibility(param: &AstType, actual: &AstType, type_params: &[String]) -> i8 {
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

    fn type_compatibility(
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

    fn match_phrase_typed(
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

    fn emit_phrase_defers(&self, phrase: &PhraseMatch, source: &str) -> bool {
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
        if let AstExpr::Call { name, .. } = expr {
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

    fn trace_sentence(
        &mut self,
        sentence: &RawSentence,
        clause: &ClauseTree,
        phrase: Option<&PhraseMatch>,
        verdict: IntentVerdict,
    ) -> IntentVerdict {
        let _ = phrase;
        let mut predicate = None;
        let mut concept = None;
        let mut class_id = None;
        let mut synset_id = None;
        let mut lemma = clause.verb.as_ref().map(|verb| verb.lemma.clone());
        if let Some(verb) = &clause.verb {
            let prep = clause.prep_phrases.first().map(|value| value.prep.as_str());
            if let Some(resolution) = classify_verb(&verb.raw, prep) {
                lemma = Some(resolution.lemma);
                predicate = resolution.predicate;
                concept = resolution.concept;
                class_id = resolution.class_id;
                synset_id = resolution.synset_id;
            }
        }
        self.trace.push(SentenceTrace {
            module: self.module.clone(),
            line: sentence.span.line,
            col: sentence.span.col,
            text: sentence.text.clone(),
            fillers: sentence.fillers.clone(),
            intent: verdict.clone(),
            lg_used: clause.lg_used,
            lg_words: clause.lg_words,
            lg_links: clause.lg_links,
            voice: clause.voice,
            verb: clause.verb.as_ref().map(|verb| verb.raw.clone()),
            lemma,
            predicate,
            concept,
            class_id,
            synset_id,
        });
        verdict
    }

    fn lower_items(
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
                && let Some(start) = parse_grid_start(&sentence.text)
                && let Some(RawItem::Sentence(moves_sentence)) = items.get(index + 1)
                && let Some(moves) = parse_grid_moves(&moves_sentence.text)
            {
                let mut cursor = index + 2;
                let mut report = false;
                while cursor < items.len() {
                    let RawItem::Sentence(next) = &items[cursor] else {
                        break;
                    };
                    if is_grid_report(&next.text) {
                        report = true;
                        cursor += 1;
                        break;
                    }
                    let lower = next.text.to_ascii_lowercase();
                    if lower.contains("one step at a time")
                        || lower.contains("already visited")
                        || lower.contains("starting spot")
                    {
                        cursor += 1;
                        continue;
                    }
                    break;
                }
                if report {
                    statements.extend(self.lower_grid_walk(start, moves, sentence.span)?);
                    index = cursor;
                    continue;
                }
            }
            if let RawItem::Sentence(sentence) = &items[index]
                && let Some(literal) = parse_sentence_literal(&sentence.text)
                && let Some(RawItem::Sentence(split)) = items.get(index + 1)
                && is_word_split_request(&split.text)
            {
                let mut cursor = index + 2;
                if let Some(RawItem::Sentence(next)) = items.get(cursor)
                    && is_word_count_request(&next.text)
                {
                    cursor += 1;
                }
                let mut filter = None;
                let mut output = false;
                while cursor < items.len() {
                    let RawItem::Sentence(next) = &items[cursor] else {
                        break;
                    };
                    if filter.is_none()
                        && let Some(parsed) = parse_word_filter(&next.text)
                    {
                        filter = Some(parsed);
                        cursor += 1;
                        continue;
                    }
                    if is_word_output_request(&next.text) {
                        output = true;
                        cursor += 1;
                        break;
                    }
                    break;
                }
                if output {
                    statements.extend(self.lower_word_frequency(
                        &literal,
                        filter.unwrap_or((0, false)),
                        sentence.span,
                    )?);
                    index = cursor;
                    continue;
                }
            }
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

    fn contains_anaphor_tokens(&self, tokens: &[Token]) -> bool {
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

    fn rhs_is_pure_expression(&self, text: &str) -> bool {
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

    fn is_create_declaration(&self, tokens: &[Token]) -> bool {
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

    fn discourse_intent_override(
        &mut self,
        sentence: &RawSentence,
        clause: &ClauseTree,
        base: IntentVerdict,
    ) -> Result<IntentVerdict, SemanticError> {
        if base.is_exec() {
            return Ok(base);
        }
        let text = sentence.text.trim();
        let tokens = tokenize(text);
        let first = tokens
            .first()
            .filter(|token| token.is_word())
            .map(|token| token.lower.clone())
            .unwrap_or_default();
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
            let resolved = self.resolve_anaphors(text, sentence.span)?;
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

    pub(crate) fn next_temp(&mut self, label: &str) -> String {
        self.temp_counter += 1;
        format!("__{label}_{}", self.temp_counter)
    }

    fn lower_repeat_count(&mut self, text: &str, span: Span) -> Result<AstExpr, SemanticError> {
        let rest = strip_leading_word(text, "repeat");
        let count_text = if let Some((start, _)) = find_top_level_word(rest, "times") {
            rest[..start].trim()
        } else {
            rest
        };
        self.parse_expr(count_text, span)
    }

    fn is_integer_type(ty: &AstType) -> bool {
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

    fn constant_i64(expr: &AstExpr) -> Option<i64> {
        match expr {
            AstExpr::ConstInt(value, _) => Some(*value),
            AstExpr::ConstUint(value, _) => i64::try_from(*value).ok(),
            _ => None,
        }
    }

    fn lower_condition(&mut self, text: &str, span: Span) -> Result<LoweredCond, SemanticError> {
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
    fn lower_check_whether(
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

    fn check_predicate(
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

    fn check_branch(
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

    fn check_ellipsis(&self, text: &str, span: Span) -> Result<Option<AstExpr>, SemanticError> {
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

    pub(crate) fn active_move_floor(&self) -> Option<String> {
        self.move_floor.clone()
    }

    pub(crate) fn active_print_tracker(&self) -> Option<(String, bool)> {
        self.print_tracker
            .as_ref()
            .map(|tracker| (tracker.name.clone(), tracker.maximum))
    }

    fn ensure_largest_tracker(&mut self, spec: LargestTracker, span: Span) -> Vec<AstStmt> {
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

    fn ensure_running_total(&mut self, span: Span) -> Vec<AstStmt> {
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

    fn ensure_named_counter(&mut self, noun: &str, span: Span) -> Vec<AstStmt> {
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

    fn scan_accumulators(&mut self, items: &[RawItem], prelude: &mut Vec<AstStmt>) {
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

    fn loop_element_type(
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

    fn lower_grid_walk(
        &mut self,
        start: GridStart,
        moves: Vec<GridMove>,
        span: Span,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        let mut x = start.x;
        let mut y = start.y;
        let mut facing = start.facing;
        let mut visited: HashSet<(i64, i64)> = HashSet::new();
        visited.insert((0, 0));
        let mut crossings = 0_i64;
        for grid_move in moves {
            if let Some(turn) = grid_move.turn {
                facing = match (facing, turn) {
                    (GridDirection::North, GridTurn::Right) => GridDirection::East,
                    (GridDirection::North, GridTurn::Left) => GridDirection::West,
                    (GridDirection::East, GridTurn::Right) => GridDirection::South,
                    (GridDirection::East, GridTurn::Left) => GridDirection::North,
                    (GridDirection::South, GridTurn::Right) => GridDirection::West,
                    (GridDirection::South, GridTurn::Left) => GridDirection::East,
                    (GridDirection::West, GridTurn::Right) => GridDirection::North,
                    (GridDirection::West, GridTurn::Left) => GridDirection::South,
                    (GridDirection::North, GridTurn::Around) => GridDirection::South,
                    (GridDirection::South, GridTurn::Around) => GridDirection::North,
                    (GridDirection::East, GridTurn::Around) => GridDirection::West,
                    (GridDirection::West, GridTurn::Around) => GridDirection::East,
                };
            }
            for _ in 0..grid_move.steps.max(0) {
                match facing {
                    GridDirection::North => y += 1,
                    GridDirection::East => x += 1,
                    GridDirection::South => y -= 1,
                    GridDirection::West => x -= 1,
                }
                if !visited.insert((x, y)) {
                    crossings += 1;
                }
            }
        }
        let distance = x.abs() + y.abs();
        let mut statements = Vec::new();
        statements.extend(self.lower_emit_parts(&[crossings.to_string()], span)?);
        statements.extend(self.lower_emit_parts(&[distance.to_string()], span)?);
        Ok(statements)
    }

    fn lower_word_frequency(
        &mut self,
        source: &str,
        filter: (usize, bool),
        span: Span,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        let (minimum_letters, keep_repeated) = filter;
        let words = source
            .split_whitespace()
            .map(|word| {
                word.trim_matches(|character: char| {
                    !(character.is_alphanumeric() || character == '\'')
                })
                .to_ascii_lowercase()
            })
            .filter(|word| !word.is_empty())
            .collect::<Vec<_>>();
        let mut order = Vec::new();
        let mut counts: HashMap<String, usize> = HashMap::new();
        for word in &words {
            if !counts.contains_key(word) {
                order.push(word.clone());
            }
            *counts.entry(word.clone()).or_insert(0) += 1;
        }
        let kept = order
            .into_iter()
            .filter_map(|word| {
                let count = *counts.get(&word)?;
                let letters = word
                    .chars()
                    .filter(|character| character.is_alphabetic())
                    .count();
                (letters >= minimum_letters || (keep_repeated && count > 1))
                    .then_some((word, count))
            })
            .collect::<Vec<_>>();
        let mut statements = Vec::new();
        for (word, count) in &kept {
            statements.extend(self.lower_emit_parts(
                &[quote_word(word), "\" \"".to_string(), count.to_string()],
                span,
            )?);
        }
        if let Some((_, (word, _))) = kept
            .iter()
            .enumerate()
            .max_by_key(|(position, (_, count))| (*count, usize::MAX - *position))
        {
            statements.extend(self.lower_emit_parts(&[quote_word(word)], span)?);
        }
        Ok(statements)
    }

    fn list_element_type(&self, name: &str) -> Option<AstType> {
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

    fn is_list_variable(&self, text: &str) -> bool {
        let trimmed = text.trim();
        is_simple_name(trimmed) && self.list_element_type(&lower_name(trimmed)).is_some()
    }

    fn try_swap_adjacent(
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

    fn emit_list_elements(
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

    fn lower_list_cleanup(
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
    fn lower_adjacent_pass(
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
    fn lower_sequence(
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

    fn ensure_implicit_counter(&mut self, span: Span) -> Result<Vec<AstStmt>, SemanticError> {
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

    fn try_implicit_count(
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

    fn count_query(&self, text: &str, span: Span) -> Result<Option<AstExpr>, SemanticError> {
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

    fn parse_natural_membership(
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

    fn string_pointer(
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

    fn materialize_string(
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

    fn is_tcstring_type(&self, ty: &AstType) -> bool {
        matches!(ty, AstType::Named { name, .. } if name.eq_ignore_ascii_case("tcstring"))
    }

    fn tcstring_type(&self) -> AstType {
        AstType::Named {
            module: None,
            name: "tcstring".to_string(),
            type_args: Vec::new(),
        }
    }

    fn is_list_value(&self, value: &AstExpr) -> bool {
        match self.shallow_type_opt(value) {
            Some(AstType::Named { name, .. }) => name.eq_ignore_ascii_case("list"),
            Some(AstType::Ptr(inner)) => {
                matches!(inner.as_ref(), AstType::Named { name, .. } if name.eq_ignore_ascii_case("list"))
            }
            _ => false,
        }
    }

    fn list_pointer(&mut self, value: AstExpr, span: Span) -> Result<AstExpr, SemanticError> {
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

    fn parse_natural_predicate(
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

    fn lower_cond_ast(&mut self, node: &CondAst, span: Span) -> Result<LoweredCond, SemanticError> {
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

    fn lower_cond_pair(
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

    fn lower_local_quantifier(
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

    fn lower_local_range(
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

    fn range_termination(cur: &str, end: &str, step: &str, step_expr: &AstExpr) -> AstExpr {
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

    fn lower_local_array(
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

    fn lower_phrase_call(
        &mut self,
        phrase: &PhraseMatch,
        span: Span,
    ) -> Result<AstStmt, SemanticError> {
        Ok(AstStmt::Expr(
            self.phrase_expression(phrase, span)?,
            self.span(span),
        ))
    }

    fn split_multi_emit(&mut self, text: &str, span: Span) -> Option<Vec<String>> {
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

    fn track_phrase_emit(&mut self, phrase: &PhraseMatch, span: Span) -> Option<AstStmt> {
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

    fn lower_stmt(
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

    fn lower_clause_graph(
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

    fn lower_binding(
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

    fn lower_where_bindings(
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

    fn lower_core(
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
            let global = parse_global_quantifier(original)
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
        let trimmed = resolved_text.as_str();
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
            } else {
                self.lower_actions_capturing(
                    &inline.as_ref().expect("inline repeat").1,
                    span,
                    depth,
                )
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

    fn parse_create_type(&self, text: &str) -> Option<AstType> {
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

    fn try_create_declaration(
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

    fn try_discourse_mutation(
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

    fn lower_global_quantifier(
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
                let start = self.parse_expr(start, span)?;
                let end = self.parse_expr(end, span)?;
                let step = if let Some(text) = step {
                    self.parse_expr(text, span)?
                } else {
                    AstExpr::ConstInt(1, None)
                };
                let var_ty = self.shallow_type(&start);
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

    fn try_natural_bit_update(
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

    fn try_write_at(&mut self, text: &str, span: Span) -> Result<Option<AstStmt>, SemanticError> {
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

    fn try_set_item(&mut self, text: &str, span: Span) -> Result<Option<AstStmt>, SemanticError> {
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

    fn try_memcopy(&mut self, text: &str, span: Span) -> Result<Option<AstStmt>, SemanticError> {
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

    fn try_syscall_with(
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

    fn split_condition_actions(
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

    fn lower_actions_capturing(
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

    fn declaration_type(&self, text: &str) -> Option<AstType> {
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

    fn declare_local(&mut self, name: &str, ty: AstType, span: Span) -> AstStmt {
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

    fn try_local_declaration(&mut self, text: &str, span: Span) -> Option<Vec<AstStmt>> {
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

    fn lower_quantifier_value(
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

    fn parse_inline_list(
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

    fn lower_inline_list(
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

    fn split_output_parts(&mut self, text: &str, span: Span) -> Option<Vec<String>> {
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

    fn try_assignment(
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

    fn try_computational(
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
                let operator = if predicate == VerbPredicate::Combine {
                    tc_ir::BinOp::Add
                } else {
                    operator
                };
                Ok(Some(self.write_arithmetic(target, operator, value, span)?))
            }
            VerbPredicate::Create
            | VerbPredicate::Destroy
            | VerbPredicate::Emit
            | VerbPredicate::Receive => Ok(None),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_transfer(
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

pub(crate) fn type_contains_params(ty: &AstType, params: &[String]) -> bool {
    match ty {
        AstType::Named {
            name, type_args, ..
        } => {
            params.iter().any(|param| param == name)
                || type_args
                    .iter()
                    .any(|arg| type_contains_params(arg, params))
        }
        AstType::Ptr(inner) | AstType::Array(inner, _) => type_contains_params(inner, params),
        AstType::FnPtr {
            params: arguments,
            ret,
        } => {
            arguments
                .iter()
                .any(|argument| type_contains_params(argument, params))
                || type_contains_params(ret, params)
        }
        _ => false,
    }
}

fn list_type(element: AstType) -> AstType {
    AstType::Named {
        module: None,
        name: "list".to_string(),
        type_args: vec![element],
    }
}

fn split_list_elements(text: &str) -> Vec<String> {
    let mut parts = split_top_level(text, ",");
    for part in &mut parts {
        let trimmed = part.trim();
        if starts_with_word(trimmed, "and") {
            *part = strip_leading_word(trimmed, "and").trim().to_string();
        } else {
            *part = trimmed.to_string();
        }
    }
    if parts.len() == 1 && !text.contains(',') {
        parts = split_top_level(text, "and")
            .into_iter()
            .map(|part| part.trim().to_string())
            .collect();
    }
    parts.retain(|part| !part.is_empty());
    parts
}

fn zero_like(expr: &AstExpr) -> AstExpr {
    match expr {
        AstExpr::ConstFloat(..) => AstExpr::ConstFloat(0.0, None),
        _ => AstExpr::ConstInt(0, None),
    }
}

fn transform_operator(
    concept: Option<SemanticConcept>,
    predicate: VerbPredicate,
) -> Option<tc_ir::BinOp> {
    match concept {
        Some(SemanticConcept::Mul) => Some(tc_ir::BinOp::Mul),
        Some(SemanticConcept::Div) => Some(tc_ir::BinOp::Div),
        Some(SemanticConcept::Mod) => Some(tc_ir::BinOp::Mod),
        _ if predicate == VerbPredicate::Combine => Some(tc_ir::BinOp::Add),
        _ => Some(tc_ir::BinOp::Mul),
    }
}

fn is_direct_call_statement(text: &str) -> bool {
    let tokens = tokenize(text);
    let Some(first) = tokens.first() else {
        return false;
    };
    if !first.is_word() {
        return false;
    }
    let open = if tokens.get(1).is_some_and(|token| token.text == "(") {
        1
    } else if tokens.get(1).is_some_and(|token| token.text == ".")
        && tokens.get(2).is_some_and(|token| token.is_word())
        && tokens.get(3).is_some_and(|token| token.text == "(")
    {
        3
    } else {
        return false;
    };
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
    if depth != 0 {
        return false;
    }
    let last = tokens
        .iter()
        .rposition(|token| !matches!(token.text.as_str(), "." | ","))
        .unwrap_or(0);
    tokens[last].text == ")"
}

fn is_simple_name(text: &str) -> bool {
    let tokens = tokenize(text);
    tokens.len() == 1
        && tokens[0].kind == TokenKind::Word
        && tokens[0]
            .text
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn after_verb_start_text<'t>(tokens: &[Token], verb_index: usize, text: &'t str) -> &'t str {
    text[tokens[verb_index + 1].start..].trim()
}

fn join_tokens(tokens: &[Token]) -> String {
    tokens
        .iter()
        .map(|token| token.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn strip_leading_word<'t>(text: &'t str, word: &str) -> &'t str {
    let trimmed = text.trim_start();
    if trimmed.len() >= word.len() && trimmed[..word.len()].eq_ignore_ascii_case(word) {
        trimmed[word.len()..].trim_start()
    } else {
        trimmed
    }
}

fn split_actions(text: &str) -> Vec<String> {
    let tokens = tokenize(text);
    let mut result = Vec::new();
    let mut start = 0;
    let mut depth = 0_i32;
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if token.kind == TokenKind::Symbol {
            match token.text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                "," if depth == 0 => {
                    let action = text[start..token.start].trim();
                    if starts_control(action) {
                        result.push(text[start..].trim().to_string());
                        return result;
                    }
                    let rest = text[token.end..].trim();
                    if !action.is_empty() && !rest.is_empty() && starts_action(rest) {
                        result.push(action.to_string());
                        start = token.end;
                    }
                }
                _ => {}
            }
            index += 1;
            continue;
        }
        if depth == 0 && token.is_word() && token.is("and") {
            let action = text[start..token.start].trim();
            let rest = text[token.end..].trim();
            if starts_control(action) {
                result.push(text[start..].trim().to_string());
                return result;
            }
            if !action.is_empty() && !rest.is_empty() && starts_action(rest) {
                result.push(action.to_string());
                start = token.end;
            }
        }
        index += 1;
    }
    let tail = text[start..].trim();
    if !tail.is_empty() {
        result.push(tail.to_string());
    }
    result
}

fn contains_nary_operator(text: &str) -> bool {
    [
        "sum of",
        "total of",
        "product of",
        "average of",
        "mean of",
        "minimum of",
        "maximum of",
        "difference between",
        "difference of",
        "quotient of",
        "ratio of",
        "remainder of",
        "square of",
        "cube of",
        "absolute value of",
        "magnitude of",
        "opposite of",
    ]
    .iter()
    .any(|phrase| crate::textutil::find_top_level_phrase(text, phrase).is_some())
}

fn quote_word(word: &str) -> String {
    let mut quoted = String::from("\"");
    for character in word.chars() {
        match character {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            _ => quoted.push(character),
        }
    }
    quoted.push('"');
    quoted
}

fn singular_noun(word: &str) -> String {
    if word.ends_with("ies") && word.len() > 3 {
        format!("{}y", &word[..word.len() - 3])
    } else if (word.ends_with("ses")
        || word.ends_with("xes")
        || word.ends_with("zes")
        || word.ends_with("ches")
        || word.ends_with("shes"))
        && word.len() > 3
    {
        word[..word.len() - 2].to_string()
    } else if word.ends_with('s') && !word.ends_with("ss") && word.len() > 1 {
        word[..word.len() - 1].to_string()
    } else {
        word.to_string()
    }
}

fn named_counter_noun(text: &str) -> Option<String> {
    let tokens = tokenize(text);
    for (index, token) in tokens.iter().enumerate() {
        if !token.is("count") {
            continue;
        }
        let mut words = Vec::new();
        for next in &tokens[index + 1..] {
            if next.kind == TokenKind::Symbol {
                if matches!(next.text.as_str(), "," | ";" | ".") {
                    break;
                }
                continue;
            }
            if next.is("and") {
                break;
            }
            words.push(next.lower.clone());
        }
        let Some(first) = words.first() else {
            continue;
        };
        if matches!(
            first.as_str(),
            "it" | "them" | "that" | "this" | "those" | "these" | "of" | "how"
        ) {
            continue;
        }
        if first == "the" && words.get(1).is_some_and(|word| word == "number") {
            continue;
        }
        let noun = words.iter().find(|word| {
            !matches!(
                word.as_str(),
                "a" | "an" | "the" | "any" | "every" | "each" | "one" | "another"
            )
        })?;
        return Some(singular_noun(noun));
    }
    None
}

fn named_counter_query_noun(lower: &str) -> Option<String> {
    let rest = if let Some(rest) = lower.strip_prefix("how many ") {
        rest
    } else if let Some(rest) = lower
        .strip_prefix("the total number of ")
        .or_else(|| lower.strip_prefix("total number of "))
    {
        rest
    } else if let Some(rest) = lower
        .strip_prefix("the number of ")
        .or_else(|| lower.strip_prefix("number of "))
    {
        rest
    } else {
        lower
            .strip_prefix("the count of ")
            .or_else(|| lower.strip_prefix("count of "))?
    };
    let words = rest
        .trim_end_matches(['.', ',', ':'])
        .split_whitespace()
        .collect::<Vec<_>>();
    let event = words
        .iter()
        .position(|word| matches!(*word, "occurred" | "happened" | "there" | "made"));
    let event = event.or_else(|| {
        words.iter().enumerate().find_map(|(index, word)| {
            (*word == "were"
                && words
                    .get(index + 1)
                    .is_some_and(|next| matches!(*next, "counted" | "found" | "recorded")))
            .then_some(index)
        })
    })?;
    let noun_words = &words[..event];
    let noun = noun_words.iter().find(|word| {
        !matches!(
            **word,
            "a" | "an"
                | "the"
                | "any"
                | "every"
                | "each"
                | "one"
                | "another"
                | "total"
                | "number"
                | "count"
                | "of"
                | "that"
                | "which"
        )
    })?;
    Some(singular_noun(noun))
}

fn extract_largest_trackers(items: Vec<RawItem>) -> (Vec<RawItem>, Option<LargestTracker>) {
    let mut remaining = Vec::new();
    let mut spec = None;
    for item in items {
        if let RawItem::Sentence(sentence) = &item
            && let Some(parsed) = parse_largest_tracker(&sentence.text)
        {
            spec = Some(parsed);
            continue;
        }
        remaining.push(item);
    }
    (remaining, spec)
}

fn closer_needs_last_number(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("where you stopped")
        || lower.contains("where it stopped")
        || lower.contains("stopping number")
        || lower.contains("number at which you stopped")
        || lower.contains("number you stopped at")
}

fn prepare_loop_rules(items: Vec<RawItem>) -> (Vec<RawItem>, Option<String>) {
    let mut remaining = Vec::new();
    let mut move_floor = None;
    let mut stop = None;
    for item in items {
        if let RawItem::Sentence(sentence) = &item {
            if move_floor.is_none()
                && let Some(floor) = parse_move_guard(&sentence.text)
            {
                move_floor = Some(floor);
                continue;
            }
            if stop.is_none()
                && let Some(condition) = parse_stop_condition(&sentence.text)
            {
                stop = Some((condition, sentence.span));
                continue;
            }
        }
        remaining.push(item);
    }
    if let Some((condition, span)) = stop {
        remaining.push(RawItem::Sentence(RawSentence {
            text: format!("If {condition}, break"),
            raw: String::new(),
            span,
            fillers: Vec::new(),
        }));
    }
    (remaining, move_floor)
}

fn apply_loop_filters(items: Vec<RawItem>, var: &str) -> Vec<RawItem> {
    let mut result = Vec::new();
    let mut index = 0;
    while index < items.len() {
        if let RawItem::Sentence(sentence) = &items[index]
            && let Some(guard) = parse_loop_filter(&sentence.text, var)
        {
            let header = RawSentence {
                text: format!("If {guard}"),
                raw: sentence.raw.clone(),
                span: sentence.span,
                fillers: Vec::new(),
            };
            let block = RawBlock {
                header: Some(header),
                items: apply_loop_filters(items[index + 1..].to_vec(), var),
                span: sentence.span,
            };
            result.push(RawItem::Block(block));
            return result;
        }
        result.push(items[index].clone());
        index += 1;
    }
    result
}

fn raw_item_start_line(item: &RawItem) -> u32 {
    match item {
        RawItem::Sentence(sentence) => sentence.span.line,
        RawItem::Block(block) => block.span.line,
    }
}

fn raw_item_end_line(item: &RawItem) -> u32 {
    match item {
        RawItem::Sentence(sentence) => sentence.span.line,
        RawItem::Block(block) => block
            .items
            .iter()
            .map(raw_item_end_line)
            .max()
            .unwrap_or(block.span.line)
            .max(block.span.line),
    }
}

fn starts_control(text: &str) -> bool {
    matches!(
        first_word(text).as_str(),
        "if" | "unless" | "while" | "repeat" | "loop" | "for"
    )
}

fn starts_action(text: &str) -> bool {
    if !text
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic())
    {
        return false;
    }
    let word = first_word(text);
    if word.is_empty() {
        return false;
    }
    if matches!(
        word.as_str(),
        "let"
            | "return"
            | "break"
            | "continue"
            | "trap"
            | "syscall"
            | "if"
            | "unless"
            | "while"
            | "repeat"
            | "loop"
    ) {
        return true;
    }
    classify_verb(&word, None).is_some()
}

/// Runs the declaration and lowering passes for one module.
#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_module(
    module: ModuleJob,
    parser: &crate::LinkGrammarParser,
    module_names: Vec<String>,
    phrases: Vec<PhraseBinding>,
    external_returns: HashMap<String, Option<AstType>>,
    external_types: HashSet<String>,
    external_aggregates: HashMap<String, AggregateInfo>,
    is_entry: bool,
) -> (
    AstModule,
    Vec<SentenceTrace>,
    Vec<SemanticError>,
    ParaphraseDoc,
) {
    let mut module_ast = module.module;
    let globals = module_ast
        .globals
        .iter()
        .map(|global| (global.name.clone(), global.ty.clone()))
        .collect::<HashMap<_, _>>();
    let mut types = external_types;
    types.extend(
        module_ast
            .structs
            .iter()
            .map(|value| value.name.clone())
            .chain(module_ast.unions.iter().map(|value| value.name.clone())),
    );
    let mut aggregates = external_aggregates;
    for definition in &module_ast.structs {
        aggregates.insert(
            definition.name.clone(),
            AggregateInfo {
                type_params: definition.type_params.clone(),
                fields: definition.fields.iter().cloned().collect(),
            },
        );
    }
    for definition in &module_ast.unions {
        aggregates.insert(
            definition.name.clone(),
            AggregateInfo {
                type_params: definition.type_params.clone(),
                fields: definition.fields.iter().cloned().collect(),
            },
        );
    }
    let mut function_returns = external_returns;
    for function in &module_ast.functions {
        function_returns.insert(
            function.name.clone(),
            (!type_contains_params(&function.ret, &function.type_params))
                .then(|| function.ret.clone()),
        );
    }
    let mut lowerer = Lowerer::new(
        parser,
        &module_ast.name,
        globals,
        types,
        aggregates,
        module_names,
        phrases,
        function_returns,
    );
    for job in module.functions {
        let function = &module_ast.functions[job.index];
        lowerer.scopes = vec![Scope::new()];
        lowerer.bytes_lengths.clear();
        lowerer.discourse = DiscourseStack::new();
        lowerer.push_scope(ScopeKind::Function);
        for (name, ty) in &function.params {
            lowerer.declare(name, ty.clone());
            lowerer.introduce_name(name, ty, Span::new(function.span.line, function.span.col));
        }
        let body = lowerer.lower_items(&job.body, 0).unwrap_or_else(|error| {
            lowerer.errors.push(error);
            Vec::new()
        });
        lowerer.pop_scope();
        module_ast.functions[job.index].body = body;
    }
    let has_main = module_ast
        .functions
        .iter()
        .any(|function| function.name == "main");
    if !module.executable.is_empty() {
        if has_main {
            lowerer.errors.push(semantic_error(
                &module_ast.name,
                Span::new(1, 1),
                SemanticErrorKind::Lowering,
                "top-level statements cannot be combined with an explicit `main` function",
            ));
        } else {
            lowerer.scopes = vec![Scope::new()];
            lowerer.discourse = DiscourseStack::new();
            lowerer.push_scope(ScopeKind::ModuleTopLevel);
            let mut body = lowerer
                .lower_items(&module.executable, 0)
                .unwrap_or_else(|error| {
                    lowerer.errors.push(error);
                    Vec::new()
                });
            lowerer.pop_scope();
            if !body
                .iter()
                .any(|statement| matches!(statement, AstStmt::Return(..)))
            {
                body.push(AstStmt::Return(
                    Some(AstExpr::ConstInt(0, None)),
                    tc_typeck::Span::new(&module_ast.name, 1, 1),
                ));
            }
            module_ast.functions.push(AstFn {
                vis: Visibility::Private,
                name: "main".to_string(),
                type_params: Vec::new(),
                params: Vec::new(),
                ret: AstType::I64,
                phrases: Vec::new(),
                body,
                span: tc_typeck::Span::new(&module_ast.name, 1, 1),
            });
        }
    } else if is_entry && !has_main {
        module_ast.functions.push(AstFn {
            vis: Visibility::Private,
            name: "main".to_string(),
            type_params: Vec::new(),
            params: Vec::new(),
            ret: AstType::I64,
            phrases: Vec::new(),
            body: vec![AstStmt::Return(
                Some(AstExpr::ConstInt(0, None)),
                tc_typeck::Span::new(&module_ast.name, 1, 1),
            )],
            span: tc_typeck::Span::new(&module_ast.name, 1, 1),
        });
    }
    let doc = ParaphraseDoc {
        module: module_ast.name.clone(),
        entries: lowerer.paraphrase_entries,
    };
    (module_ast, lowerer.trace, lowerer.errors, doc)
}

impl ValueContext for Lowerer<'_> {
    fn match_value_phrase(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<PhraseMatch>, SemanticError> {
        self.match_phrase_typed(text, span)
    }

    fn value_phrase_expression(
        &mut self,
        phrase: &PhraseMatch,
        span: Span,
    ) -> Result<AstExpr, SemanticError> {
        self.phrase_expression(phrase, span)
    }

    fn value_expression(&mut self, text: &str, span: Span) -> Result<AstExpr, SemanticError> {
        self.parse_expr(text, span)
    }

    fn value_atom(&mut self, text: &str, span: Span) -> Result<AstExpr, SemanticError> {
        self.parse_value(text, span)
    }

    fn length_expression(
        &mut self,
        value: AstExpr,
        span: Span,
    ) -> Result<Option<AstExpr>, SemanticError> {
        if let AstExpr::BytesLit(bytes) = &value {
            return Ok(Some(AstExpr::ConstInt(
                i64::try_from(bytes.len()).unwrap_or(i64::MAX),
                None,
            )));
        }
        if let AstExpr::Var(name) = &value {
            if let Some(length) = self.bytes_lengths.get(name) {
                return Ok(Some(AstExpr::ConstInt(
                    i64::try_from(*length).unwrap_or(i64::MAX),
                    None,
                )));
            }
        }
        let Some(ty) = self.shallow_type_opt(&value) else {
            return Ok(None);
        };
        let aggregate = match &ty {
            AstType::Named { name, .. } if name.eq_ignore_ascii_case("list") => true,
            AstType::Named { name, .. } if name.eq_ignore_ascii_case("tcstring") => true,
            AstType::Ptr(inner) => match inner.as_ref() {
                AstType::Named { name, .. } => {
                    name.eq_ignore_ascii_case("list") || name.eq_ignore_ascii_case("tcstring")
                }
                _ => false,
            },
            _ => false,
        };
        if !aggregate {
            return Ok(None);
        }
        match &value {
            AstExpr::Var(name) => {
                let field = format!("{name}'s len");
                let expression = self.parse_expr(&field, span)?;
                Ok(Some(expression))
            }
            _ => Ok(None),
        }
    }

    fn value_error(&self, span: Span, message: &str) -> SemanticError {
        self.error(span, SemanticErrorKind::Lowering, message)
    }

    fn plural_names(&self) -> Vec<String> {
        self.discourse.plural_names()
    }

    fn result_name(&self) -> Option<String> {
        if self.lookup("result").is_some() || self.globals.contains_key("result") {
            None
        } else {
            self.discourse.last_result()
        }
    }

    fn note_anaphor(&mut self, annotation: String) {
        self.add_annotation(annotation);
    }
}

#[cfg(test)]
mod version2_tests {
    use super::{Span, normalize_function_header, normalize_natural_source};

    #[test]
    fn natural_header_with_given_parameters() {
        let header = normalize_function_header(
            "Publicly, to twice given value as a whole number, named twice, returning a whole number.",
            "main",
            Span::default(),
        )
        .expect("header");
        assert!(header.contains("function twice"));
        assert!(header.contains("taking value as a whole number"));
        assert!(header.contains("returning a whole number"));
    }

    #[test]
    fn paragraph_function_ends_at_blank_line() {
        let (normalized, _) = normalize_natural_source(
            "Use core.\nTo twice given value as a whole number, named twice, returning a whole number.\nGive back value plus value.\n\nReturn twice 5.\n",
        );
        assert!(normalized.contains("function twice"));
        assert!(normalized.contains("Return value plus value"));
        assert!(normalized.contains("Return twice 5"));
    }

    #[test]
    fn trailing_if_becomes_control_block() {
        let (normalized, _) = normalize_natural_source(
            "To smaller given a as a whole number, b as a whole number, named smaller, returning a whole number.\nGive back a if a is less than b, otherwise give back b.\n",
        );
        assert!(normalized.contains("If a is less than b:"));
        assert!(normalized.contains("Return a"));
        assert!(normalized.contains("Otherwise:"));
        assert!(normalized.contains("Return b"));
    }

    #[test]
    fn inline_naming_becomes_let() {
        let (normalized, _) = normalize_natural_source(
            "To make given value as a whole number, named make, returning nothing.\nvalue plus 1 and call it answer.\n",
        );
        assert!(normalized.contains("Let answer be value plus 1."));
    }
}
