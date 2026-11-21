//! ACE-inspired boolean condition parsing and precedence rendering.
//!
//! Coordinator binding order, from loosest to tightest:
//! `, or` < `, and` < `or` < `and` < `not` < atom.
//!
//! The parser is deliberately token based: atoms remain source text and are
//! parsed later with the unified Core-7 expression parser.

use tc_ir::{BinOp, UnOp};
use tc_typeck::{AstExpr, AstStmt, AstType};

use crate::quantifier::{LocalQuantSpec, parse_local_quantifier};
use crate::token::{Token, TokenKind, tokenize};

const MAX_CONDITION_DEPTH: usize = 32;

/// One condition lowered to Core-7 with any required prelude statements.
#[derive(Debug, Clone)]
pub(crate) struct LoweredCond {
    /// Statements that must execute immediately before `expr`.
    pub prelude: Vec<AstStmt>,
    /// Pure boolean expression.
    pub expr: AstExpr,
    /// Explicitly grouped canonical rendering.
    pub paraphrase: String,
}

/// Parsed boolean condition tree.
#[derive(Debug, Clone)]
pub(crate) enum CondAst {
    /// Atomic comparison or boolean expression, stored as source text.
    Atom(String),
    /// Unary negation.
    Not(Box<CondAst>),
    /// Short-circuit conjunction.
    And(Box<CondAst>, Box<CondAst>),
    /// Short-circuit disjunction.
    Or(Box<CondAst>, Box<CondAst>),
    /// Local `every` / `some` / `no` quantifier.
    Quantifier(LocalQuantSpec),
}

#[derive(Debug, Clone, Copy)]
enum SplitKind {
    CommaAnd,
    CommaOr,
    WordAnd,
    WordOr,
}

fn token_is_word(token: &Token, word: &str) -> bool {
    token.kind == TokenKind::Word && token.is(word)
}

fn next_is_word(tokens: &[Token], index: usize, word: &str) -> bool {
    tokens
        .get(index + 1)
        .is_some_and(|token| token_is_word(token, word))
}

fn starts_quantifier(tokens: &[Token], start: usize) -> bool {
    tokens.get(start).is_some_and(|token| {
        token.kind == TokenKind::Word
            && matches!(token.lower.as_str(), "every" | "each" | "some" | "no")
    })
}

/// Splits a condition on one coordinator level.
fn split_condition(text: &str, kind: SplitKind) -> Vec<String> {
    let tokens = tokenize(text);
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut depth = 0_i32;
    let mut between_pending = false;
    let mut protect_rest = false;
    let mut chunk_is_quantifier = starts_quantifier(&tokens, 0);
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if token.kind == TokenKind::Symbol {
            if token.text == ","
                && depth == 0
                && !protect_rest
                && !between_pending
                && matches!(kind, SplitKind::CommaAnd | SplitKind::CommaOr)
            {
                let matched = match kind {
                    SplitKind::CommaAnd => next_is_word(&tokens, index, "and"),
                    SplitKind::CommaOr => next_is_word(&tokens, index, "or"),
                    SplitKind::WordAnd | SplitKind::WordOr => false,
                };
                if matched {
                    let separator_end = tokens[index + 1].end;
                    let piece = text[start..token.start].trim();
                    if !piece.is_empty() {
                        pieces.push(piece.to_string());
                    }
                    start = separator_end;
                    index += 2;
                    between_pending = false;
                    chunk_is_quantifier = starts_quantifier(&tokens, index);
                    continue;
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
        if depth > 0 {
            index += 1;
            continue;
        }
        if token.kind == TokenKind::Word {
            if token.is("between") {
                between_pending = true;
                index += 1;
                continue;
            }
            if chunk_is_quantifier && (token.is("satisfies") || token.is("where")) {
                protect_rest = true;
                index += 1;
                continue;
            }
            if chunk_is_quantifier && token.is("such") && next_is_word(&tokens, index, "that") {
                protect_rest = true;
                index += 2;
                continue;
            }
        }
        if protect_rest {
            index += 1;
            continue;
        }
        if token_is_word(token, "and") && between_pending {
            between_pending = false;
            index += 1;
            continue;
        }
        let separator_end = match kind {
            SplitKind::WordAnd => {
                if token_is_word(token, "and") {
                    if next_is_word(&tokens, index, "that") {
                        None
                    } else if between_pending {
                        between_pending = false;
                        None
                    } else {
                        Some(token.end)
                    }
                } else {
                    None
                }
            }
            SplitKind::WordOr => {
                if token_is_word(token, "or") && !next_is_word(&tokens, index, "that") {
                    Some(token.end)
                } else {
                    None
                }
            }
            SplitKind::CommaAnd | SplitKind::CommaOr => None,
        };
        if let Some(end) = separator_end {
            let piece = text[start..token.start].trim();
            if !piece.is_empty() {
                pieces.push(piece.to_string());
            }
            start = end;
            index += 1;
            between_pending = false;
            chunk_is_quantifier = starts_quantifier(&tokens, index);
            continue;
        }
        index += 1;
    }
    let tail = text[start..].trim();
    if !tail.is_empty() {
        pieces.push(tail.to_string());
    }
    pieces
}

fn fold_or(mut nodes: Vec<CondAst>) -> CondAst {
    let mut node = nodes.remove(0);
    for next in nodes {
        node = CondAst::Or(Box::new(node), Box::new(next));
    }
    node
}

fn fold_and(mut nodes: Vec<CondAst>) -> CondAst {
    let mut node = nodes.remove(0);
    for next in nodes {
        node = CondAst::And(Box::new(node), Box::new(next));
    }
    node
}

fn parse_comma_or(text: &str, depth: usize) -> Result<CondAst, String> {
    if depth > MAX_CONDITION_DEPTH {
        return Err("condition nesting exceeded the 32-level guard".to_string());
    }
    let pieces = split_condition(text, SplitKind::CommaOr);
    if pieces.len() > 1 {
        let nodes = pieces
            .iter()
            .map(|piece| parse_comma_and(piece, depth + 1))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(fold_or(nodes));
    }
    parse_comma_and(text, depth + 1)
}

fn parse_comma_and(text: &str, depth: usize) -> Result<CondAst, String> {
    if depth > MAX_CONDITION_DEPTH {
        return Err("condition nesting exceeded the 32-level guard".to_string());
    }
    let pieces = split_condition(text, SplitKind::CommaAnd);
    if pieces.len() > 1 {
        let nodes = pieces
            .iter()
            .map(|piece| parse_bare_or(piece, depth + 1))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(fold_and(nodes));
    }
    parse_bare_or(text, depth + 1)
}

fn parse_bare_or(text: &str, depth: usize) -> Result<CondAst, String> {
    if depth > MAX_CONDITION_DEPTH {
        return Err("condition nesting exceeded the 32-level guard".to_string());
    }
    let pieces = split_condition(text, SplitKind::WordOr);
    if pieces.len() > 1 {
        let nodes = pieces
            .iter()
            .map(|piece| parse_bare_and(piece, depth + 1))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(fold_or(nodes));
    }
    parse_bare_and(text, depth + 1)
}

fn parse_bare_and(text: &str, depth: usize) -> Result<CondAst, String> {
    if depth > MAX_CONDITION_DEPTH {
        return Err("condition nesting exceeded the 32-level guard".to_string());
    }
    let pieces = split_condition(text, SplitKind::WordAnd);
    if pieces.len() > 1 {
        let nodes = pieces
            .iter()
            .map(|piece| parse_unary_not(piece, depth + 1))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(fold_and(nodes));
    }
    parse_unary_not(text, depth + 1)
}

/// Splits the tail of `it is false that ...` on repeated `and that` / `or that`.
fn split_negated_chain(text: &str) -> (String, Vec<(bool, String)>) {
    let tokens = tokenize(text);
    let mut first: Option<String> = None;
    let mut rest = Vec::new();
    let mut pending_and = true;
    let mut start = 0;
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
            index += 1;
            continue;
        }
        if depth == 0
            && token.kind == TokenKind::Word
            && (token.is("and") || token.is("or"))
            && next_is_word(&tokens, index, "that")
        {
            let piece = text[start..token.start].trim().to_string();
            if first.is_none() {
                first = Some(piece);
            } else {
                rest.push((pending_and, piece));
            }
            pending_and = token.is("and");
            start = tokens[index + 1].end;
            index += 2;
            continue;
        }
        index += 1;
    }
    let tail = text[start..].trim().to_string();
    if first.is_none() {
        first = Some(tail);
    } else if !tail.is_empty() {
        rest.push((pending_and, tail));
    }
    (first.unwrap_or_default(), rest)
}

fn parse_unary_not(text: &str, depth: usize) -> Result<CondAst, String> {
    if depth > MAX_CONDITION_DEPTH {
        return Err("condition nesting exceeded the 32-level guard".to_string());
    }
    let trimmed = text.trim();
    let tokens = tokenize(trimmed);
    if tokens
        .first()
        .is_some_and(|token| token.kind == TokenKind::Word && token.is("not"))
    {
        let rest = trimmed[tokens[0].end..].trim();
        return Ok(CondAst::Not(Box::new(parse_unary_not(rest, depth + 1)?)));
    }
    let phrase = ["it", "is", "false", "that"];
    if tokens.len() >= phrase.len()
        && tokens
            .iter()
            .zip(phrase.iter())
            .all(|(token, word)| token_is_word(token, word))
    {
        let rest = trimmed[tokens[phrase.len() - 1].end..].trim();
        let (first, chain) = split_negated_chain(rest);
        let mut node = parse_atom(&first, depth + 1)?;
        for (conjunction, piece) in chain {
            let rhs = parse_atom(&piece, depth + 1)?;
            node = if conjunction {
                CondAst::And(Box::new(node), Box::new(rhs))
            } else {
                CondAst::Or(Box::new(node), Box::new(rhs))
            };
        }
        return Ok(CondAst::Not(Box::new(node)));
    }
    parse_atom(trimmed, depth + 1)
}

fn strip_condition_parentheses(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    if !trimmed.starts_with('(') || !trimmed.ends_with(')') {
        return None;
    }
    let mut depth = 0_i32;
    for (index, character) in trimmed.char_indices() {
        if character == '(' {
            depth += 1;
        } else if character == ')' {
            depth -= 1;
            if depth == 0 {
                if index + 1 == trimmed.len() {
                    return Some(trimmed[1..trimmed.len() - 1].trim());
                }
                return None;
            }
        }
    }
    None
}

fn parse_atom(text: &str, depth: usize) -> Result<CondAst, String> {
    if depth > MAX_CONDITION_DEPTH {
        return Err("condition nesting exceeded the 32-level guard".to_string());
    }
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("empty condition".to_string());
    }
    if let Some(inner) = strip_condition_parentheses(trimmed) {
        return parse_comma_or(inner, depth + 1);
    }
    let tokens = tokenize(trimmed);
    if starts_quantifier(&tokens, 0) {
        return Ok(CondAst::Quantifier(parse_local_quantifier(trimmed)?));
    }
    Ok(CondAst::Atom(trimmed.to_string()))
}

/// Parses a complete boolean condition.
pub(crate) fn parse_condition(text: &str) -> Result<CondAst, String> {
    parse_comma_or(text.trim(), 0)
}

/// Splits the text after `If` / `Unless` / `While` into condition and body.
///
/// A top-level comma followed by `and` or `or` stays inside the condition.
/// A top-level `then` or any other top-level comma ends the condition.
pub(crate) fn split_header_condition(text: &str) -> (String, String) {
    let tokens = tokenize(text);
    let mut depth = 0_i32;
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if token.kind == TokenKind::Symbol {
            if token.text == "," && depth == 0 {
                let next = tokens.get(index + 1);
                if next.is_some_and(|value| token_is_word(value, "then")) {
                    let condition = text[..token.start].trim();
                    return (
                        condition.to_string(),
                        text[next.map_or(token.end, |value| value.end)..]
                            .trim()
                            .to_string(),
                    );
                }
                if next
                    .is_some_and(|value| token_is_word(value, "and") || token_is_word(value, "or"))
                {
                    index += 2;
                    continue;
                }
                let condition = text[..token.start].trim();
                return (condition.to_string(), text[token.end..].trim().to_string());
            }
            match token.text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                _ => {}
            }
            index += 1;
            continue;
        }
        if depth > 0 {
            index += 1;
            continue;
        }
        if token_is_word(token, "then") {
            let condition = text[..token.start].trim().trim_end_matches(',').trim();
            return (condition.to_string(), text[token.end..].trim().to_string());
        }
        index += 1;
    }
    (text.trim().to_string(), String::new())
}

fn type_text(ty: &AstType) -> String {
    match ty {
        AstType::Void => "void".to_string(),
        AstType::Bool => "bool".to_string(),
        AstType::I8 => "i8".to_string(),
        AstType::I16 => "i16".to_string(),
        AstType::I32 => "i32".to_string(),
        AstType::I64 => "i64".to_string(),
        AstType::U8 => "u8".to_string(),
        AstType::U16 => "u16".to_string(),
        AstType::U32 => "u32".to_string(),
        AstType::U64 => "u64".to_string(),
        AstType::F32 => "f32".to_string(),
        AstType::F64 => "f64".to_string(),
        AstType::Ptr(inner) => format!("ptr<{}>", type_text(inner)),
        AstType::Array(inner, count) => format!("array<{}, {count}>", type_text(inner)),
        AstType::Named {
            module,
            name,
            type_args,
        } => {
            let base = module
                .as_ref()
                .map_or_else(|| name.clone(), |module| format!("{module}.{name}"));
            if type_args.is_empty() {
                base
            } else {
                format!(
                    "{base}<{}>",
                    type_args
                        .iter()
                        .map(type_text)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
        AstType::FnPtr { params, ret } => format!(
            "fn({}) -> {}",
            params.iter().map(type_text).collect::<Vec<_>>().join(", "),
            type_text(ret)
        ),
    }
}

fn binary_symbol(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::BitAnd => "bitand",
        BinOp::BitOr => "bitor",
        BinOp::BitXor => "bitxor",
        BinOp::Shl => "shl",
        BinOp::Shr => "shr",
        BinOp::LogAnd => "and",
        BinOp::LogOr => "or",
    }
}

/// Removes one balanced outer parenthesis pair from a rendered atom.
pub(crate) fn strip_outer_parentheses(text: &str) -> String {
    let trimmed = text.trim();
    if !trimmed.starts_with('(') || !trimmed.ends_with(')') {
        return trimmed.to_string();
    }
    let mut depth = 0_i32;
    for (index, character) in trimmed.char_indices() {
        if character == '(' {
            depth += 1;
        } else if character == ')' {
            depth -= 1;
            if depth == 0 && index + 1 < trimmed.len() {
                return trimmed.to_string();
            }
        }
    }
    trimmed[1..trimmed.len() - 1].trim().to_string()
}

/// Wraps a rendered condition child in parentheses when it is grouped.
pub(crate) fn render_cond_child_text(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        format!("({})", &trimmed[1..trimmed.len() - 1])
    } else {
        trimmed.to_string()
    }
}

/// Renders an expression as deterministic canonical English-like text.
pub(crate) fn render_expr(expr: &AstExpr) -> String {
    match expr {
        AstExpr::ConstInt(value, _) => value.to_string(),
        AstExpr::ConstUint(value, _) => value.to_string(),
        AstExpr::ConstFloat(value, _) => {
            let mut text = format!("{value}");
            if !text.contains('.') && !text.contains('e') {
                text.push_str(".0");
            }
            text
        }
        AstExpr::ConstBool(value) => value.to_string(),
        AstExpr::ConstNull(ty) => format!("null as {}", type_text(ty)),
        AstExpr::BytesLit(bytes) => format!("bytes_ptr({:?})", String::from_utf8_lossy(bytes)),
        AstExpr::BytesLen(bytes) => format!("bytes_len({:?})", String::from_utf8_lossy(bytes)),
        AstExpr::Zero(ty) => format!("zero {}", type_text(ty)),
        AstExpr::Var(name) => name.clone(),
        AstExpr::GlobalAddr { module, name } => module
            .as_ref()
            .map_or_else(|| name.clone(), |module| format!("{module}.{name}")),
        AstExpr::AddrOfVar(name) => format!("&{name}"),
        AstExpr::FieldAddr { base_ptr, field } => {
            format!("{}.{field}", render_expr(base_ptr))
        }
        AstExpr::IndexAddr { base_ptr, index } => {
            format!("{}[{}]", render_expr(base_ptr), render_expr(index))
        }
        AstExpr::Load(pointer) => format!("*{}", render_expr(pointer)),
        AstExpr::SizeOf(ty) => format!("size of {}", type_text(ty)),
        AstExpr::AlignOf(ty) => format!("align of {}", type_text(ty)),
        AstExpr::Unary { op, val } => match op {
            UnOp::Neg => format!("-{}", render_expr(val)),
            UnOp::LogNot => format!("!{}", render_expr(val)),
            UnOp::BitNot => format!("bitnot {}", render_expr(val)),
        },
        AstExpr::Binary { op, lhs, rhs } => format!(
            "({} {} {})",
            render_expr(lhs),
            binary_symbol(*op),
            render_expr(rhs)
        ),
        AstExpr::Cast { val, to } => format!("{} as {}", render_expr(val), type_text(to)),
        AstExpr::FnAddr { module, name, .. } => module.as_ref().map_or_else(
            || format!("function {name}"),
            |module| format!("function {module}.{name}"),
        ),
        AstExpr::Call {
            module, name, args, ..
        } => {
            let callee = module
                .as_ref()
                .map_or_else(|| name.clone(), |module| format!("{module}.{name}"));
            format!(
                "{callee}({})",
                args.iter().map(render_expr).collect::<Vec<_>>().join(", ")
            )
        }
        AstExpr::IndirectCall { callee, args } => format!(
            "{}({})",
            render_expr(callee),
            args.iter().map(render_expr).collect::<Vec<_>>().join(", ")
        ),
        AstExpr::Syscall { num, args } => {
            let mut all = vec![render_expr(num)];
            all.extend(args.iter().map(render_expr));
            format!("syscall({})", all.join(", "))
        }
    }
}
