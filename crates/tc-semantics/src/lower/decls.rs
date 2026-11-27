//! Declaration, header, phrase, and normalization parsing.

use super::{
    AggregateHeader, AstFn, AstGlobal, AstModule, AstStruct, AstType, AstUnion, BTreeMap,
    FunctionJob, HashMap, HashSet, Import, ModuleJob, PhraseBinding, PhrasePart, PhrasePattern,
    RawItem, SemanticError, SemanticErrorKind, SourceFile, Span, Token, TokenKind, Visibility,
    classify_verb, decode_string_token, find_top_level_symbol, find_top_level_word, first_word,
    is_simple_name, parse_type_text, segment_source, semantic_error, split_actions,
    split_header_condition, split_type_list, starts_control, tokenize, type_span,
};

pub(super) fn rewrite_bound_names(text: &str, renames: &[(String, String)]) -> String {
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

fn normalize_action_phrases(text: &str) -> String {
    text.replace(", then stop the loop", ", break")
        .replace(", and then stop", ", break")
        .replace(", then stop", ", break")
        .replace(", and stop", ", break")
}

pub(super) fn find_top_level_phrase(text: &str, phrase: &str) -> Option<(String, usize)> {
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

pub(super) fn split_condition_comparison(text: &str) -> Option<(String, tc_ir::BinOp, String)> {
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

pub(super) fn lower_name(name: &str) -> String {
    name.to_ascii_lowercase()
}

pub(super) fn declaration_kind(text: &str) -> Option<&'static str> {
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

pub(super) fn parse_visibility(text: &str) -> Visibility {
    let lower = text.to_ascii_lowercase();
    if lower.split_whitespace().any(|word| word == "public") {
        Visibility::Public
    } else {
        Visibility::Private
    }
}

pub(super) fn parse_import(text: &str, module: &str, span: Span) -> Result<Import, SemanticError> {
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

pub(super) fn top_level_word_count(text: &str, word: &str) -> usize {
    let mut count = 0;
    let mut rest = text;
    while let Some((_, end)) = find_top_level_word(rest, word) {
        count += 1;
        rest = &rest[end..];
    }
    count
}

pub(super) fn split_as(text: &str) -> Option<(String, String)> {
    let (start, end) = find_top_level_word(text, "as")?;
    let left = text[..start].trim();
    let right = text[end..].trim();
    if left.is_empty() || right.is_empty() {
        return None;
    }
    Some((left.to_string(), right.to_string()))
}

pub(super) fn canonical_name_phrase(text: &str) -> String {
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

pub(super) fn original_name_phrase(text: &str) -> String {
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

pub(super) fn split_parenthetical_type(text: &str) -> Option<(String, String)> {
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

pub(super) fn validate_field_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

pub(super) fn parse_field_specs(
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

pub(super) fn parse_fields(
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

pub(super) fn parse_type_params(
    text: &str,
    module: &str,
    span: Span,
) -> Result<Vec<String>, SemanticError> {
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

pub(super) fn parse_struct_header(
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

pub(super) fn extract_phrase_lines(
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

pub(super) fn parse_phrase_pattern(
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

pub(super) fn parse_function_header(
    text: &str,
    module: &str,
    span: Span,
) -> Result<AstFn, SemanticError> {
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

pub(super) fn natural_function_header(text: &str) -> bool {
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

pub(super) fn natural_visibility(tokens: &[tc_syntax::Token]) -> &'static str {
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

pub(super) fn split_natural_parameters(
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

pub(super) fn compact_natural_function_type(ty: &str) -> Option<String> {
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

pub(super) fn v2_type_indicator(word: &str, type_params: &[String]) -> bool {
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

pub(super) fn v2_type_word(word: &str, type_params: &[String]) -> bool {
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

pub(super) fn normalize_type_text_words(text: &str) -> String {
    text.replace("numbers", "number")
        .replace("decimals", "decimal")
        .replace("bytes ", "byte ")
        .trim_end_matches(" bytes")
        .to_string()
}

pub(super) fn extract_embedded_parameters(
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
pub(super) struct ParentheticalParam {
    canonical_names: Vec<String>,
    original_names: Vec<String>,
    ty: String,
    replace_start: usize,
    close_index: usize,
    plural: bool,
}

pub(super) fn is_parameter_run_stop(word: &str) -> bool {
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

pub(super) fn type_parameter_span(tokens: &[Token], to_index: usize) -> Option<(usize, usize)> {
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

pub(super) fn outer_return_position(tokens: &[Token]) -> Option<usize> {
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

pub(super) fn parenthetical_parameters(text: &str) -> Vec<ParentheticalParam> {
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

pub(super) fn derive_function_name(text: &str, params: &[ParentheticalParam]) -> Option<String> {
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

pub(super) fn normalize_function_header(
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

pub(super) fn quoted_values(line: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut parts = line.split('"');
    parts.next();
    while let Some(value) = parts.next() {
        values.push(value.to_string());
        parts.next();
    }
    values
}

pub(super) fn normalize_natural_declaration(trimmed: &str) -> Option<String> {
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

pub(super) fn normalize_natural_line(trimmed: &str) -> Option<String> {
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
pub(super) fn v2_action_start(text: &str) -> bool {
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

pub(super) fn split_inline_function_body(text: &str) -> Option<(String, String)> {
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

pub(super) fn sentence_terminator_before(tokens: &[Token], index: usize) -> bool {
    tokens.iter().take(index).rev().any(|token| {
        token.kind == TokenKind::Symbol && matches!(token.text.as_str(), "." | ";" | "!" | "?")
    })
}

pub(super) fn trailing_conditional_marker(
    tokens: &[Token],
    index: usize,
) -> Option<(usize, usize)> {
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

pub(super) fn split_trailing_if(text: &str) -> Option<(String, String, Option<String>)> {
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

pub(super) fn split_call_it(text: &str) -> Option<(String, String)> {
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

pub(super) fn split_tracking_directive(text: &str) -> Option<(String, String)> {
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

pub(super) fn normalize_prose_line(text: &str, indent: usize) -> Vec<String> {
    let expanded = crate::normalize::expand_contractions(text);
    let text = expanded.as_str();
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

pub(super) fn pattern_parts_to_text(parts: &[PhrasePart]) -> String {
    parts
        .iter()
        .map(|part| match part {
            PhrasePart::Word(word) => word.clone(),
            PhrasePart::Param(name) => format!("<{name}>"),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn auto_phrase_has_safe_prefix(parts: &[PhrasePart]) -> bool {
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

pub(super) fn auto_phrase_patterns(text: &str, canonical_header: &str) -> Vec<String> {
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
pub(super) struct NamePhrases {
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
        let mut binding = None;
        for operator in ["be", "starts", "start", "begins", "begin"] {
            if let Some((name_end, _)) = find_top_level_word(rest, operator) {
                binding = Some(name_end);
                break;
            }
        }
        let Some(name_end) = binding else {
            return;
        };
        let name = rest[..name_end].trim();
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

pub(super) fn top_level_declaration(text: &str) -> bool {
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

fn trailing_control_header(text: &str, allow_colon: bool) -> Option<String> {
    let pieces = tc_syntax::split_sentences(text);
    if pieces.len() < 2 {
        return None;
    }
    let last = pieces.last().map(|(_, piece)| piece.trim())?;
    if !last.ends_with(':') {
        return None;
    }
    paragraph_control_header(last, allow_colon)
}

pub(super) fn paragraph_control_header(text: &str, allow_colon: bool) -> Option<String> {
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

pub(super) fn next_nonblank_indent(lines: &[&str], index: usize) -> Option<usize> {
    lines
        .iter()
        .skip(index + 1)
        .find(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
}

pub(super) fn normalize_natural_source(text: &str) -> (String, Vec<u32>) {
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
        let action_normalized = normalize_action_phrases(&rewritten);
        let trimmed = action_normalized.as_str();
        if let Some(header) = trailing_control_header(trimmed, allow_colon) {
            let base_indent = if let Some(&control_indent) = control_blocks.last() {
                control_indent + 4
            } else if let Some(function_indent) = natural_function {
                function_indent + 4
            } else {
                indent
            };
            let pieces = tc_syntax::split_sentences(trimmed);
            let prose_indent = if control_blocks.is_empty() && natural_function.is_none() {
                base_indent
            } else {
                base_indent + indent
            };
            for (_, piece) in &pieces[..pieces.len().saturating_sub(1)] {
                for line in normalize_prose_line(piece, prose_indent) {
                    push_normalized_line(&mut output, &mut line_map, &line, source_line);
                }
            }
            push_normalized_line(
                &mut output,
                &mut line_map,
                &format!("{}{}:", " ".repeat(base_indent), header),
                source_line,
            );
            control_blocks.push(base_indent);
            continue;
        }
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

pub(super) fn push_normalized_line(
    output: &mut String,
    line_map: &mut Vec<u32>,
    line: &str,
    source_line: u32,
) {
    output.push_str(line);
    output.push('\n');
    line_map.push(source_line);
}

pub(super) fn remap_document_lines(document: &mut crate::RawDocument, line_map: &[u32]) {
    remap_item_lines(&mut document.items, line_map);
}

pub(super) fn remap_item_lines(items: &mut [RawItem], line_map: &[u32]) {
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

pub(super) fn remap_span_line(span: &mut Span, line_map: &[u32]) {
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

pub(super) fn reserved_declaration_errors(module: &str, ast: &AstModule) -> Vec<SemanticError> {
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

pub(super) fn parse_global(
    text: &str,
    module: &str,
    span: Span,
) -> Result<AstGlobal, SemanticError> {
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
