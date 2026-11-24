//! Recursive compositional English value and noun-phrase parsing.

use tc_ir::BinOp;
use tc_typeck::AstExpr;

use crate::phrases::PhraseMatch;
use crate::token::{Token, TokenKind, tokenize};
use crate::{SemanticError, Span};

/// Coarse intent of a value expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValueKind {
    /// Numeric/string/boolean literal.
    Literal,
    /// Variable or field reference.
    Place,
    /// Function or phrase call.
    Call,
    /// Arithmetic/bitwise/pointer expression.
    Expression,
}

/// One normalized value before typed lowering.
#[derive(Debug, Clone)]
pub(crate) struct ValueSpec {
    /// Original/canonical source text.
    pub text: String,
    /// Coarse expression kind.
    pub kind: ValueKind,
}

impl ValueSpec {
    /// Classifies a value without losing its source text.
    pub(crate) fn classify(text: &str) -> Self {
        let trimmed = text.trim();
        let lower = trimmed.to_ascii_lowercase();
        let kind = if trimmed.starts_with('"')
            || trimmed.starts_with('\'')
            || lower == "true"
            || lower == "false"
            || lower == "no address"
            || lower == "null"
            || trimmed
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_digit() || ch == '-' || ch == '.')
        {
            ValueKind::Literal
        } else if trimmed.contains('(') || lower.starts_with("create ") {
            ValueKind::Call
        } else if trimmed.contains(" of ")
            || trimmed.contains('\'')
            || trimmed.contains('.')
            || trimmed.contains('[')
        {
            ValueKind::Place
        } else if trimmed.contains(' ')
            || trimmed.contains('+')
            || trimmed.contains('-')
            || trimmed.contains('*')
            || trimmed.contains('/')
            || trimmed.contains('%')
            || trimmed.contains('<')
            || trimmed.contains('>')
        {
            ValueKind::Expression
        } else {
            ValueKind::Place
        };
        Self {
            text: trimmed.to_string(),
            kind,
        }
    }
}

/// Callbacks required by the shared value parser.
pub(crate) trait ValueContext {
    /// Match a declarative phrase value.
    fn match_value_phrase(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<PhraseMatch>, SemanticError>;

    /// Lower a matched phrase call.
    fn value_phrase_expression(
        &mut self,
        phrase: &PhraseMatch,
        span: Span,
    ) -> Result<AstExpr, SemanticError>;

    /// Parse a Core-7 expression after anaphor/name resolution.
    fn value_expression(&mut self, text: &str, span: Span) -> Result<AstExpr, SemanticError>;

    /// Parse one recursive atom, allowing `their ...` and `the result`.
    fn value_atom(&mut self, text: &str, span: Span) -> Result<AstExpr, SemanticError>;

    /// Build a lowering diagnostic for malformed NP text.
    fn value_error(&self, span: Span, message: &str) -> SemanticError;

    /// Active plural referent names for `their ...`.
    fn plural_names(&self) -> Vec<String>;

    /// Most recent assignment target for `the result`.
    fn result_name(&self) -> Option<String>;

    /// Records a plural or result anaphor resolution for paraphrase/hover.
    fn note_anaphor(&mut self, annotation: String);

    /// Builds a list/text length expression when possible.
    fn length_expression(
        &mut self,
        value: AstExpr,
        span: Span,
    ) -> Result<Option<AstExpr>, SemanticError>;
}

/// The one value parser used by every value position.
pub(crate) fn parse_value<C: ValueContext>(
    context: &mut C,
    text: &str,
    span: Span,
) -> Result<AstExpr, SemanticError> {
    let normalized = crate::number_words::normalize_hyphens(text);
    let trimmed = normalized.trim();
    let lower = trimmed.to_ascii_lowercase();
    if let Some(operation) = lower.strip_prefix("their ") {
        let names = context.plural_names();
        context.note_anaphor(format!("[their {operation} -> {}]", names.join(" + ")));
        return plural_value(&names, operation.trim()).ok_or_else(|| {
            context.value_error(
                span,
                "their ... requires two or more accessible variables from a compound binding",
            )
        });
    }
    if lower == "the result" {
        if let Some(name) = context.result_name() {
            context.note_anaphor(format!("[the result -> {name}]"));
            return Ok(AstExpr::Var(name));
        }
    }
    for prefix in [
        "the number of items in ",
        "number of items in ",
        "the number of characters in ",
        "number of characters in ",
        "the length of ",
        "length of ",
        "the size of ",
        "size of ",
        "the count of ",
        "count of ",
    ] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let rest = trimmed[trimmed.len() - rest.len()..].trim();
            let simple_count = !prefix.contains("count")
                || rest
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
                || (rest.starts_with('"') && rest.ends_with('"'));
            if !simple_count {
                continue;
            }
            if let Some((left, operator, right)) = split_length_arithmetic(rest) {
                let value = parse_value(context, left, span)?;
                if let Some(length) = context.length_expression(value, span)? {
                    let right = parse_value(context, right, span)?;
                    return Ok(binary(operator, length, right));
                }
            }
            if let Ok(value) = parse_value(context, rest, span)
                && let Some(length) = context.length_expression(value, span)?
            {
                return Ok(length);
            }
        }
    }
    for prefix in ["the value of ", "value of "] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let rest = trimmed[trimmed.len() - rest.len()..].trim();
            return parse_value(context, rest, span);
        }
    }
    for prefix in ["the number ", "number "] {
        if lower.starts_with(prefix)
            && !lower.starts_with(&format!("{prefix}of "))
            && lower.len() > prefix.len()
        {
            let rest = trimmed[prefix.len()..].trim();
            let first = rest
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            let starts_operator = matches!(
                first.as_str(),
                "times"
                    | "plus"
                    | "minus"
                    | "divided"
                    | "over"
                    | "modulo"
                    | "mod"
                    | "and"
                    | "or"
                    | "is"
                    | "are"
                    | "equals"
            );
            if !starts_operator {
                return parse_value(context, rest, span);
            }
        }
    }
    let specification = ValueSpec::classify(trimmed);
    let _ = specification.kind;
    let phrase_text = specification
        .text
        .strip_prefix("the ")
        .or_else(|| specification.text.strip_prefix("The "))
        .unwrap_or(&specification.text)
        .to_ascii_lowercase();
    for operation in ["sum", "total", "average", "minimum", "maximum", "count"] {
        if let Some(rest) = phrase_text.strip_prefix(&format!("{operation} of ")) {
            let simple = rest
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_');
            let countable = operation != "count" || simple;
            if !rest.contains(" and ")
                && !rest.contains(',')
                && !rest.starts_with("address of ")
                && !rest.is_empty()
                && countable
            {
                let candidate = format!("{operation} of address of {rest}");
                if let Some(phrase) = context.match_value_phrase(&candidate, span)? {
                    return context.value_phrase_expression(&phrase, span);
                }
            }
        }
    }
    if let Some(phrase) = context.match_value_phrase(&specification.text, span)? {
        return context.value_phrase_expression(&phrase, span);
    }
    if let Some(rest) = specification
        .text
        .strip_prefix("the ")
        .or_else(|| specification.text.strip_prefix("The "))
    {
        if let Some(phrase) = context.match_value_phrase(rest.trim(), span)? {
            return context.value_phrase_expression(&phrase, span);
        }
    }
    if is_np_expression(&specification.text)
        && let Some((left, operator, right)) = split_symbolic_expression(&specification.text)
    {
        let lhs = parse_value(context, left.trim(), span)?;
        let rhs = parse_value(context, right.trim(), span)?;
        return Ok(binary(operator, lhs, rhs));
    }
    if is_np_expression(&specification.text) {
        let mut parser = NpParser {
            text: &specification.text,
            tokens: tokenize(&specification.text),
            position: 0,
            context,
            span,
            group_depth: 0,
        };
        let value = parser.parse_all()?;
        return Ok(value);
    }
    context.value_expression(&specification.text, span)
}

/// Builds a plural anaphor expression from a referent group.
#[must_use]
pub(crate) fn plural_value(names: &[String], operation: &str) -> Option<AstExpr> {
    if names.is_empty() {
        return None;
    }
    let expressions = names
        .iter()
        .map(|name| AstExpr::Var(name.clone()))
        .collect::<Vec<_>>();
    match operation {
        "sum" | "total" => Some(fold(expressions, BinOp::Add)),
        "product" => Some(fold(expressions, BinOp::Mul)),
        "difference" if expressions.len() == 2 => Some(binary(
            BinOp::Sub,
            expressions[0].clone(),
            expressions[1].clone(),
        )),
        "quotient" if expressions.len() == 2 => Some(binary(
            BinOp::Div,
            expressions[0].clone(),
            expressions[1].clone(),
        )),
        "average" | "mean" => {
            let total = fold(expressions, BinOp::Add);
            Some(binary(
                BinOp::Div,
                total,
                AstExpr::ConstInt(i64::try_from(names.len()).ok()?, None),
            ))
        }
        "minimum" | "min" | "least" => Some(fold_by(expressions, numeric_min)),
        "maximum" | "max" | "greatest" => Some(fold_by(expressions, numeric_max)),
        _ => None,
    }
}

fn fold(values: Vec<AstExpr>, operator: BinOp) -> AstExpr {
    let mut values = values.into_iter();
    let Some(first) = values.next() else {
        return AstExpr::ConstInt(0, None);
    };
    values.fold(first, |left, right| binary(operator, left, right))
}

fn fold_by(values: Vec<AstExpr>, operation: fn(AstExpr, AstExpr) -> AstExpr) -> AstExpr {
    let mut values = values.into_iter();
    let Some(first) = values.next() else {
        return AstExpr::ConstInt(0, None);
    };
    values.fold(first, operation)
}

fn binary(operator: BinOp, lhs: AstExpr, rhs: AstExpr) -> AstExpr {
    AstExpr::Binary {
        op: operator,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
    }
}

fn numeric_min(left: AstExpr, right: AstExpr) -> AstExpr {
    let difference = binary(BinOp::Sub, left, right.clone());
    let sign = binary(BinOp::Shr, difference.clone(), AstExpr::ConstInt(63, None));
    binary(BinOp::Add, right, binary(BinOp::BitAnd, difference, sign))
}

fn numeric_max(left: AstExpr, right: AstExpr) -> AstExpr {
    let difference = binary(BinOp::Sub, left.clone(), right);
    let sign = binary(BinOp::Shr, difference.clone(), AstExpr::ConstInt(63, None));
    binary(BinOp::Sub, left, binary(BinOp::BitAnd, difference, sign))
}

fn numeric_abs(value: AstExpr) -> AstExpr {
    let sign = binary(BinOp::Shr, value.clone(), AstExpr::ConstInt(63, None));
    binary(BinOp::Sub, binary(BinOp::BitXor, value, sign.clone()), sign)
}

fn split_symbolic_expression(text: &str) -> Option<(&str, BinOp, &str)> {
    let tokens = tokenize(text);
    let mut depth = 0_i32;
    let mut additive = None;
    let mut multiplicative = None;
    for (index, token) in tokens.iter().enumerate() {
        if token.kind == TokenKind::Symbol {
            match token.text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                _ => {}
            }
            if depth == 0 && index > 0 {
                match token.text.as_str() {
                    "+" => additive = Some((index, BinOp::Add)),
                    "-" => additive = Some((index, BinOp::Sub)),
                    "*" => multiplicative = Some((index, BinOp::Mul)),
                    "/" => multiplicative = Some((index, BinOp::Div)),
                    "%" => multiplicative = Some((index, BinOp::Mod)),
                    _ => {}
                }
            }
        }
    }
    let (index, operator) = additive.or(multiplicative)?;
    let token = &tokens[index];
    Some((&text[..token.start], operator, &text[token.end..]))
}

fn split_length_arithmetic(text: &str) -> Option<(&str, BinOp, &str)> {
    let tokens = tokenize(text);
    let mut depth = 0_i32;
    for (index, token) in tokens.iter().enumerate() {
        if token.kind == TokenKind::Symbol {
            match token.text.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                "+" | "-" | "*" | "/" if depth == 0 && index > 0 => {
                    let operator = match token.text.as_str() {
                        "+" => BinOp::Add,
                        "-" => BinOp::Sub,
                        "*" => BinOp::Mul,
                        _ => BinOp::Div,
                    };
                    return Some((
                        text[..token.start].trim(),
                        operator,
                        text[token.end..].trim(),
                    ));
                }
                _ => {}
            }
            continue;
        }
        if depth != 0 || index == 0 {
            continue;
        }
        let operator = match token.lower.as_str() {
            "plus" => Some(BinOp::Add),
            "minus" => Some(BinOp::Sub),
            "times" => Some(BinOp::Mul),
            _ => None,
        };
        if let Some(operator) = operator {
            return Some((
                text[..token.start].trim(),
                operator,
                text[token.end..].trim(),
            ));
        }
        if token.is("divided")
            && tokens.get(index + 1).is_some_and(|next| next.is("by"))
            && let Some(by) = tokens.get(index + 1)
        {
            return Some((
                text[..token.start].trim(),
                BinOp::Div,
                text[by.end..].trim(),
            ));
        }
    }
    None
}

pub(crate) fn is_np_expression(text: &str) -> bool {
    let tokens = tokenize(text);
    if tokens
        .iter()
        .any(|token| token.is_word() && matches!(token.lower.as_str(), "squared" | "cubed"))
    {
        return true;
    }
    let mut index = 0;
    if tokens
        .first()
        .is_some_and(|token| token.is("the") || token.is("a") || token.is("an"))
    {
        index = 1;
    }
    let first = tokens
        .get(index)
        .filter(|token| token.is_word())
        .map(|token| token.lower.as_str());
    let second = tokens
        .get(index + 1)
        .filter(|token| token.is_word())
        .map(|token| token.lower.as_str());
    if index == 1
        && matches!(
            first,
            Some("plus" | "minus" | "times" | "divided" | "over" | "modulo" | "mod")
        )
    {
        return false;
    }
    match first {
        Some(
            "sum" | "total" | "product" | "average" | "mean" | "minimum" | "smallest" | "least"
            | "maximum" | "largest" | "greatest" | "quotient" | "ratio" | "remainder",
        ) => second == Some("of"),
        Some("difference") => matches!(second, Some("between" | "of")),
        Some("square" | "cube") => second == Some("of"),
        Some("absolute") => second == Some("value"),
        Some("magnitude" | "opposite") => second == Some("of"),
        Some(
            "twice" | "double" | "triple" | "half" | "quarter" | "third" | "negative" | "minus",
        ) => tokens
            .get(index + 1)
            .is_some_and(|token| !token.is("and") && token.text != ","),
        _ => false,
    }
}

struct NpParser<'a, 'c, C> {
    text: &'a str,
    tokens: Vec<Token>,
    position: usize,
    context: &'c mut C,
    span: Span,
    group_depth: usize,
}

impl<C: ValueContext> NpParser<'_, '_, C> {
    fn parse_all(&mut self) -> Result<AstExpr, SemanticError> {
        let value = self.parse_additive()?;
        if let Some(token) = self.tokens.get(self.position) {
            return Err(self.context.value_error(
                self.span,
                &format!("unexpected trailing token `{}`", token.text),
            ));
        }
        Ok(value)
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn peek_is(&self, word: &str) -> bool {
        self.peek().is_some_and(|token| token.is(word))
    }

    fn peek_word(&self, offset: usize) -> Option<&str> {
        self.tokens
            .get(self.position + offset)
            .filter(|token| token.is_word())
            .map(|token| token.lower.as_str())
    }

    fn eat(&mut self, word: &str) -> bool {
        if self.peek_is(word) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn eat_symbol(&mut self, symbol: &str) -> bool {
        if self.peek().is_some_and(|token| token.text == symbol) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, word: &str) -> Result<(), SemanticError> {
        if self.eat(word) {
            Ok(())
        } else {
            Err(self.context.value_error(
                self.span,
                &format!("expected `{word}` in English expression"),
            ))
        }
    }

    fn parse_additive(&mut self) -> Result<AstExpr, SemanticError> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let operator = if self.eat("plus") || self.eat_symbol("+") {
                Some(BinOp::Add)
            } else if self.eat("minus") || self.eat_symbol("-") {
                Some(BinOp::Sub)
            } else {
                None
            };
            let Some(operator) = operator else {
                break;
            };
            let right = self.parse_multiplicative()?;
            left = binary(operator, left, right);
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<AstExpr, SemanticError> {
        let mut left = self.parse_prefix()?;
        loop {
            let operator = if self.eat("times") || self.eat_symbol("*") {
                Some(BinOp::Mul)
            } else if self.eat("multiplied") {
                self.expect("by")?;
                Some(BinOp::Mul)
            } else if self.eat("divided") {
                self.expect("by")?;
                Some(BinOp::Div)
            } else if self.eat("over") || self.eat_symbol("/") {
                Some(BinOp::Div)
            } else if self.eat("modulo") || self.eat("mod") || self.eat_symbol("%") {
                Some(BinOp::Mod)
            } else {
                None
            };
            let Some(operator) = operator else {
                break;
            };
            let right = self.parse_prefix()?;
            left = binary(operator, left, right);
        }
        Ok(left)
    }

    fn parse_prefix(&mut self) -> Result<AstExpr, SemanticError> {
        let checkpoint = self.position;
        let article = self.eat("the") || self.eat("a") || self.eat("an");
        if let Some(operation) = self.nary_operation() {
            return self.parse_nary(operation);
        }
        if self.eat("square") {
            self.expect("of")?;
            let value = self.parse_prefix()?;
            return Ok(binary(BinOp::Mul, value.clone(), value));
        }
        if self.eat("cube") {
            self.expect("of")?;
            let value = self.parse_prefix()?;
            return Ok(binary(
                BinOp::Mul,
                binary(BinOp::Mul, value.clone(), value.clone()),
                value,
            ));
        }
        if self.eat("absolute") {
            self.expect("value")?;
            self.expect("of")?;
            return Ok(numeric_abs(self.parse_prefix()?));
        }
        if self.eat("magnitude") {
            self.expect("of")?;
            return Ok(numeric_abs(self.parse_prefix()?));
        }
        if self.eat("twice") {
            let value = self.parse_prefix()?;
            return Ok(binary(BinOp::Mul, AstExpr::ConstInt(2, None), value));
        }
        if self.eat("double") {
            let _ = self.eat("of");
            let value = self.parse_prefix()?;
            return Ok(binary(BinOp::Mul, AstExpr::ConstInt(2, None), value));
        }
        if self.eat("triple") {
            let _ = self.eat("of");
            let value = self.parse_prefix()?;
            return Ok(binary(BinOp::Mul, AstExpr::ConstInt(3, None), value));
        }
        if self.eat("half") {
            let _ = self.eat("of");
            let value = self.parse_prefix()?;
            return Ok(binary(BinOp::Div, value, AstExpr::ConstInt(2, None)));
        }
        if self.eat("quarter") {
            let _ = self.eat("of");
            let value = self.parse_prefix()?;
            return Ok(binary(BinOp::Div, value, AstExpr::ConstInt(4, None)));
        }
        if self.eat("third") {
            let _ = self.eat("of");
            let value = self.parse_prefix()?;
            return Ok(binary(BinOp::Div, value, AstExpr::ConstInt(3, None)));
        }
        if self.eat("negative") || self.eat("minus") {
            let value = self.parse_prefix()?;
            return Ok(AstExpr::Unary {
                op: tc_ir::UnOp::Neg,
                val: Box::new(value),
            });
        }
        if self.eat("opposite") {
            self.expect("of")?;
            let value = self.parse_prefix()?;
            return Ok(AstExpr::Unary {
                op: tc_ir::UnOp::Neg,
                val: Box::new(value),
            });
        }
        if article {
            self.position = checkpoint;
        }
        self.parse_postfix()
    }

    fn nary_operation(&self) -> Option<&'static str> {
        let first = self.peek_word(0)?;
        let second = self.peek_word(1);
        let third = self.peek_word(2);
        match first {
            "sum" | "total" if second == Some("of") => Some("sum"),
            "product" if second == Some("of") => Some("product"),
            "average" | "mean" if second == Some("of") => Some("average"),
            "minimum" | "smallest" | "least" if second == Some("of") => Some("minimum"),
            "maximum" | "largest" | "greatest" if second == Some("of") => Some("maximum"),
            "difference" if matches!(second, Some("between" | "of")) => Some("difference"),
            "quotient" if second == Some("of") => Some("quotient"),
            "ratio" if second == Some("of") => Some("ratio"),
            "remainder" if second == Some("of") => Some("remainder"),
            "square" if third == Some("of") => Some("square"),
            _ => None,
        }
    }

    fn parse_nary(&mut self, operation: &str) -> Result<AstExpr, SemanticError> {
        let article = self.eat("the") || self.eat("a") || self.eat("an");
        let _ = article;
        match operation {
            "difference" => {
                let _ = self.eat("difference");
                if !self.eat("between") {
                    self.expect("of")?;
                }
                let left = self.parse_multiplicative()?;
                self.expect("and")?;
                let right = self.parse_multiplicative()?;
                Ok(binary(BinOp::Sub, left, right))
            }
            "quotient" | "ratio" | "remainder" => {
                let _ = self.eat(operation);
                self.expect("of")?;
                let left = self.parse_prefix()?;
                if !self.eat("and") && !self.eat("by") && !self.eat("to") {
                    self.expect("divided")?;
                    self.expect("by")?;
                }
                let right = self.parse_prefix()?;
                let operator = if operation == "remainder" {
                    BinOp::Mod
                } else {
                    BinOp::Div
                };
                Ok(binary(operator, left, right))
            }
            "square" => {
                let _ = self.eat("square");
                self.expect("of")?;
                let value = self.parse_prefix()?;
                Ok(binary(BinOp::Mul, value.clone(), value))
            }
            _ => {
                let _ = self.eat(operation);
                self.expect("of")?;
                let mut values = vec![self.parse_multiplicative()?];
                while self.eat_symbol(",") {
                    let _ = self.eat("and");
                    if self.tokens.get(self.position).is_none() {
                        break;
                    }
                    values.push(self.parse_multiplicative()?);
                }
                if self.eat("and") && self.tokens.get(self.position).is_some() {
                    values.push(self.parse_multiplicative()?);
                }
                let count = values.len();
                let value = match operation {
                    "product" => fold(values, BinOp::Mul),
                    "average" => binary(
                        BinOp::Div,
                        fold(values, BinOp::Add),
                        AstExpr::ConstInt(i64::try_from(count).unwrap_or(1), None),
                    ),
                    "minimum" => fold_by(values, numeric_min),
                    "maximum" => fold_by(values, numeric_max),
                    _ => fold(values, BinOp::Add),
                };
                Ok(value)
            }
        }
    }

    fn parse_postfix(&mut self) -> Result<AstExpr, SemanticError> {
        let mut value = self.parse_primary()?;
        loop {
            if self.eat("squared") {
                value = binary(BinOp::Mul, value.clone(), value);
            } else if self.eat("cubed") {
                value = binary(
                    BinOp::Mul,
                    binary(BinOp::Mul, value.clone(), value.clone()),
                    value,
                );
            } else {
                break;
            }
        }
        Ok(value)
    }

    fn parse_primary(&mut self) -> Result<AstExpr, SemanticError> {
        if self.eat_symbol("(") {
            if self.group_depth >= 16 {
                return Err(self
                    .context
                    .value_error(self.span, "expression nesting exceeds the Core-7 limit"));
            }
            self.group_depth += 1;
            let value = self.parse_additive()?;
            self.group_depth -= 1;
            if !self.eat_symbol(")") {
                return Err(self.context.value_error(self.span, "expected `)`"));
            }
            return Ok(value);
        }
        let start = self.position;
        while let Some(token) = self.peek() {
            if atom_delimiter(token) {
                let numeric_hyphen = token.text == "-"
                    && self.position > start
                    && self.tokens.get(self.position - 1).is_some_and(|previous| {
                        previous.is_word() && crate::number_words::is_word(&previous.lower)
                    })
                    && self.tokens.get(self.position + 1).is_some_and(|next| {
                        next.is_word() && crate::number_words::is_word(&next.lower)
                    });
                if numeric_hyphen {
                    self.position += 1;
                    continue;
                }
                break;
            }
            self.position += 1;
        }
        if start == self.position {
            let found = self.peek().map_or_else(
                || "end of expression".to_string(),
                |token| token.text.clone(),
            );
            return Err(self.context.value_error(
                self.span,
                &format!("expected an English value, found `{found}`"),
            ));
        }
        let first = &self.tokens[start];
        let last = &self.tokens[self.position - 1];
        let text = self.text[first.start..last.end].trim();
        self.context.value_atom(text, self.span)
    }
}

fn atom_delimiter(token: &Token) -> bool {
    if token.kind == TokenKind::Symbol {
        return matches!(
            token.text.as_str(),
            "(" | ")" | "," | "+" | "-" | "*" | "/" | "%"
        );
    }
    token.is_word()
        && matches!(
            token.lower.as_str(),
            "and"
                | "plus"
                | "minus"
                | "times"
                | "multiplied"
                | "divided"
                | "over"
                | "modulo"
                | "mod"
                | "squared"
                | "cubed"
                | "to"
                | "by"
        )
}
