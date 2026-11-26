//! Shared free helpers used across the lowering submodules.

use super::{
    AstExpr, AstType, LargestTracker, RawBlock, RawItem, RawSentence, SemanticConcept, Token,
    TokenKind, VerbPredicate, classify_verb, first_word, parse_largest_tracker, parse_loop_filter,
    parse_move_guard, parse_stop_condition, split_top_level, starts_with_word, tokenize,
};

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

pub(super) fn list_type(element: AstType) -> AstType {
    AstType::Named {
        module: None,
        name: "list".to_string(),
        type_args: vec![element],
    }
}

pub(super) fn split_list_elements(text: &str) -> Vec<String> {
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

pub(super) fn zero_like(expr: &AstExpr) -> AstExpr {
    match expr {
        AstExpr::ConstFloat(..) => AstExpr::ConstFloat(0.0, None),
        _ => AstExpr::ConstInt(0, None),
    }
}

pub(super) fn transform_operator(
    concept: Option<SemanticConcept>,
    predicate: VerbPredicate,
) -> Option<tc_ir::BinOp> {
    match concept {
        Some(SemanticConcept::Mul) => Some(tc_ir::BinOp::Mul),
        Some(SemanticConcept::Div) => Some(tc_ir::BinOp::Div),
        Some(SemanticConcept::Mod) => Some(tc_ir::BinOp::Mod),
        Some(SemanticConcept::Add | SemanticConcept::Increment) => Some(tc_ir::BinOp::Add),
        Some(SemanticConcept::Sub | SemanticConcept::Decrement) => Some(tc_ir::BinOp::Sub),
        _ if predicate == VerbPredicate::Combine => Some(tc_ir::BinOp::Add),
        _ => Some(tc_ir::BinOp::Mul),
    }
}

pub(super) fn is_direct_call_statement(text: &str) -> bool {
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

pub(super) fn is_simple_name(text: &str) -> bool {
    let tokens = tokenize(text);
    tokens.len() == 1
        && tokens[0].kind == TokenKind::Word
        && tokens[0]
            .text
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

pub(super) fn after_verb_start_text<'t>(
    tokens: &[Token],
    verb_index: usize,
    text: &'t str,
) -> &'t str {
    text[tokens[verb_index + 1].start..].trim()
}

pub(super) fn join_tokens(tokens: &[Token]) -> String {
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

pub(super) fn split_actions(text: &str) -> Vec<String> {
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

pub(super) fn contains_nary_operator(text: &str) -> bool {
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

pub(super) fn singular_noun(word: &str) -> String {
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

pub(super) fn named_counter_noun(text: &str) -> Option<String> {
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

pub(super) fn named_counter_query_noun(lower: &str) -> Option<String> {
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

pub(super) fn extract_largest_trackers(
    items: Vec<RawItem>,
) -> (Vec<RawItem>, Option<LargestTracker>) {
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

pub(super) fn closer_needs_last_number(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("where you stopped")
        || lower.contains("where it stopped")
        || lower.contains("stopping number")
        || lower.contains("number at which you stopped")
        || lower.contains("number you stopped at")
}

pub(super) fn prepare_loop_rules(items: Vec<RawItem>) -> (Vec<RawItem>, Option<String>) {
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

pub(super) fn apply_loop_filters(items: Vec<RawItem>, var: &str) -> Vec<RawItem> {
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

pub(super) fn raw_item_start_line(item: &RawItem) -> u32 {
    match item {
        RawItem::Sentence(sentence) => sentence.span.line,
        RawItem::Block(block) => block.span.line,
    }
}

pub(super) fn raw_item_end_line(item: &RawItem) -> u32 {
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

pub(super) fn starts_control(text: &str) -> bool {
    matches!(
        first_word(text).as_str(),
        "if" | "unless" | "while" | "repeat" | "loop" | "for"
    )
}

pub(super) fn starts_action(text: &str) -> bool {
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
