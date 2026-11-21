//! Deterministic English expression parser used by the unified lowerer.
//!
//! The parser is deliberately finite and total: a successful parse consumes
//! every token, so prose cannot silently become an expression.

use std::collections::HashMap;

use tc_ir::{BinOp, UnOp};
use tc_typeck::AstExpr;
use tc_typeck::AstType;

use crate::token::{Token, TokenKind, decode_string_token, tokenize};

/// Name-resolution environment for expression parsing.
#[derive(Debug, Clone, Default)]
pub(crate) struct ExprEnv {
    /// Locals visible at the current point.
    pub locals: HashMap<String, AstType>,
    /// Module-level globals.
    pub globals: HashMap<String, AstType>,
    /// Imported module names and aliases.
    pub modules: Vec<String>,
}

impl ExprEnv {
    fn type_of(&self, name: &str) -> Option<&AstType> {
        self.locals.get(name).or_else(|| self.globals.get(name))
    }

    fn is_global(&self, name: &str) -> bool {
        self.globals.contains_key(name)
    }
}

struct Parser<'a> {
    tokens: &'a [Token],
    position: usize,
    env: &'a ExprEnv,
}

type Result<T> = std::result::Result<T, String>;

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.position)
    }

    fn at_end(&self) -> bool {
        self.position >= self.tokens.len()
    }

    fn eat_symbol(&mut self, symbol: &str) -> bool {
        if self.peek().is_some_and(|token| token.text == symbol) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn eat_word(&mut self, word: &str) -> bool {
        if self
            .peek()
            .is_some_and(|token| token.kind == TokenKind::Word && token.is(word))
        {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn expect_symbol(&mut self, symbol: &str) -> Result<()> {
        if self.eat_symbol(symbol) {
            Ok(())
        } else {
            Err(format!("expected `{symbol}`"))
        }
    }

    fn expect_word(&mut self, word: &str) -> Result<()> {
        if self.eat_word(word) {
            Ok(())
        } else {
            Err(format!("expected `{word}`"))
        }
    }

    fn parse_expression(&mut self) -> Result<AstExpr> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<AstExpr> {
        let mut left = self.parse_and()?;
        while self.eat_word("or") || self.eat_symbol("||") {
            let right = self.parse_and()?;
            left = binary(BinOp::LogOr, left, right);
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<AstExpr> {
        let mut left = self.parse_not()?;
        while self.eat_word("and") || self.eat_symbol("&&") {
            let right = self.parse_not()?;
            left = binary(BinOp::LogAnd, left, right);
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> Result<AstExpr> {
        if self.eat_word("not") || self.eat_symbol("!") {
            return Ok(AstExpr::Unary {
                op: UnOp::LogNot,
                val: Box::new(self.parse_not()?),
            });
        }
        self.parse_comparison()
    }

    fn comparison_operator(&mut self) -> Option<BinOp> {
        if self.eat_symbol("==") {
            return Some(BinOp::Eq);
        }
        if self.eat_symbol("!=") {
            return Some(BinOp::Ne);
        }
        if self.eat_symbol("<=") {
            return Some(BinOp::Le);
        }
        if self.eat_symbol(">=") {
            return Some(BinOp::Ge);
        }
        if self.eat_symbol("<") {
            return Some(BinOp::Lt);
        }
        if self.eat_symbol(">") {
            return Some(BinOp::Gt);
        }
        if self.eat_word("equals") || self.eat_word("equal") {
            let _ = self.eat_word("to");
            return Some(BinOp::Eq);
        }
        if self.eat_word("more") || self.eat_word("greater") || self.eat_word("bigger") {
            self.expect_word("than").ok()?;
            return Some(BinOp::Gt);
        }
        if self.eat_word("less") || self.eat_word("smaller") {
            self.expect_word("than").ok()?;
            return Some(BinOp::Lt);
        }
        if self.eat_word("at") {
            if self.eat_word("least") {
                return Some(BinOp::Ge);
            }
            if self.eat_word("most") {
                return Some(BinOp::Le);
            }
            return None;
        }
        if self.eat_word("not") {
            let _ = self.eat_word("equal");
            let _ = self.eat_word("to");
            return Some(BinOp::Ne);
        }
        None
    }

    fn parse_between(&mut self, left: AstExpr) -> Result<AstExpr> {
        self.expect_word("between")?;
        let low = self.parse_additive()?;
        self.expect_word("and")?;
        let high = self.parse_additive()?;
        let lower = binary(BinOp::Ge, left.clone(), low);
        let upper = binary(BinOp::Le, left, high);
        Ok(binary(BinOp::LogAnd, lower, upper))
    }

    fn parse_comparison(&mut self) -> Result<AstExpr> {
        let mut left = self.parse_bitwise()?;
        loop {
            if self.eat_word("is") {
                if self.eat_word("between") {
                    self.position -= 1;
                    left = self.parse_between(left)?;
                    continue;
                }
                if let Some(operator) = self.comparison_operator() {
                    let right = self.parse_bitwise()?;
                    left = binary(operator, left, right);
                    continue;
                }
                let right = self.parse_bitwise()?;
                left = binary(BinOp::Eq, left, right);
                continue;
            }
            if self.eat_word("between") {
                self.position -= 1;
                left = self.parse_between(left)?;
                continue;
            }
            if let Some(operator) = self.comparison_operator() {
                let right = self.parse_bitwise()?;
                left = binary(operator, left, right);
                continue;
            }
            break;
        }
        Ok(left)
    }

    fn parse_bitwise(&mut self) -> Result<AstExpr> {
        let mut left = self.parse_additive()?;
        loop {
            let operator = if self.eat_word("bitand") {
                Some(BinOp::BitAnd)
            } else if self.eat_word("bitor") {
                Some(BinOp::BitOr)
            } else if self.eat_word("bitxor") {
                Some(BinOp::BitXor)
            } else if self.eat_word("shl") {
                Some(BinOp::Shl)
            } else if self.eat_word("shr") {
                Some(BinOp::Shr)
            } else {
                None
            };
            let Some(operator) = operator else {
                break;
            };
            let right = self.parse_additive()?;
            left = binary(operator, left, right);
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<AstExpr> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let operator = if self.eat_symbol("+") || self.eat_word("plus") {
                Some(BinOp::Add)
            } else if self.eat_symbol("-") || self.eat_word("minus") {
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

    fn parse_multiplicative(&mut self) -> Result<AstExpr> {
        let mut left = self.parse_unary()?;
        loop {
            let mut percent = false;
            let mut operator = if self.eat_symbol("*") || self.eat_word("times") {
                Some(BinOp::Mul)
            } else if self.eat_symbol("/") || self.eat_word("over") || self.eat_word("mod") {
                if self.tokens[self.position - 1].is("mod") {
                    Some(BinOp::Mod)
                } else {
                    Some(BinOp::Div)
                }
            } else if self.eat_symbol("%") {
                Some(BinOp::Mod)
            } else if self.eat_word("multiplied") || self.eat_word("divided") {
                let divided = self.tokens[self.position - 1].is("divided");
                self.expect_word("by")?;
                Some(if divided { BinOp::Div } else { BinOp::Mul })
            } else if self.eat_word("percent") || self.eat_word("per") {
                if self.tokens[self.position - 1].is("per") {
                    self.expect_word("cent")?;
                }
                self.expect_word("of")?;
                percent = true;
                None
            } else {
                None
            };
            if percent {
                let right = self.parse_unary()?;
                let product = binary(BinOp::Mul, left, right);
                left = binary(BinOp::Div, product, AstExpr::ConstInt(100, None));
                continue;
            }
            let Some(op) = operator.take() else {
                break;
            };
            let right = self.parse_unary()?;
            left = binary(op, left, right);
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<AstExpr> {
        if self.eat_symbol("-") {
            return Ok(AstExpr::Unary {
                op: UnOp::Neg,
                val: Box::new(self.parse_unary()?),
            });
        }
        if self.eat_symbol("+") {
            return self.parse_unary();
        }
        if self.eat_symbol("&") || self.eat_word("address") {
            if self.tokens[self.position - 1].is("address") {
                self.expect_word("of")?;
            }
            let value = self.parse_unary()?;
            return explicit_address(&value, self.env)
                .ok_or_else(|| "expression is not addressable".to_string());
        }
        if self.eat_symbol("*") || self.eat_word("deref") {
            return Ok(AstExpr::Load(Box::new(self.parse_unary()?)));
        }
        if self.eat_word("bitnot") {
            return Ok(AstExpr::Unary {
                op: UnOp::BitNot,
                val: Box::new(self.parse_unary()?),
            });
        }
        if self.eat_word("size") || self.eat_word("align") {
            let alignment = self.tokens[self.position - 1].is("align");
            self.expect_word("of")?;
            let ty = self.parse_type()?;
            return Ok(if alignment {
                AstExpr::AlignOf(ty)
            } else {
                AstExpr::SizeOf(ty)
            });
        }
        if self.eat_word("twice") {
            let value = self.parse_unary()?;
            return Ok(binary(BinOp::Mul, value, AstExpr::ConstInt(2, None)));
        }
        if self.eat_word("half") {
            self.expect_word("of")?;
            let value = self.parse_unary()?;
            return Ok(binary(BinOp::Div, value, AstExpr::ConstInt(2, None)));
        }
        if self.eat_word("square") {
            self.expect_word("of")?;
            let value = self.parse_unary()?;
            return Ok(binary(BinOp::Mul, value.clone(), value));
        }
        let (value, _) = self.parse_postfix()?;
        Ok(value)
    }

    fn parse_postfix(&mut self) -> Result<(AstExpr, Option<AstExpr>)> {
        let (mut value, mut place) = self.parse_primary()?;
        loop {
            if self.eat_symbol(".") || self.eat_symbol("'s") {
                if let AstExpr::BytesLit(bytes) = &value {
                    if self.eat_word("len") {
                        value = AstExpr::BytesLen(bytes.clone());
                        place = None;
                        continue;
                    }
                    if self.eat_word("ptr") {
                        continue;
                    }
                }
                let field = self
                    .peek()
                    .filter(|token| token.is_word())
                    .map(|token| token.lower.clone())
                    .ok_or_else(|| "expected field name after `.`".to_string())?;
                self.position += 1;
                if let AstExpr::Var(module) = &value {
                    if self.env.modules.contains(module) {
                        if self.peek().is_some_and(|t| t.text == "(") {
                            let args = self.parse_call_arguments()?;
                            value = AstExpr::Call {
                                module: Some(module.clone()),
                                name: field,
                                type_args: Vec::new(),
                                args,
                            };
                            place = None;
                            continue;
                        }
                        let address = AstExpr::GlobalAddr {
                            module: Some(module.clone()),
                            name: field,
                        };
                        value = AstExpr::Load(Box::new(address.clone()));
                        place = Some(address);
                        continue;
                    }
                }
                let base = storage_address(&value, self.env)
                    .ok_or_else(|| "field access requires an addressable base".to_string())?;
                let address = AstExpr::FieldAddr {
                    base_ptr: Box::new(base),
                    field,
                };
                value = AstExpr::Load(Box::new(address.clone()));
                place = Some(address);
                continue;
            }
            if self.eat_symbol("[") {
                let index = self.parse_expression()?;
                self.expect_symbol("]")?;
                let base = storage_address(&value, self.env)
                    .ok_or_else(|| "indexing requires an addressable base".to_string())?;
                let address = AstExpr::IndexAddr {
                    base_ptr: Box::new(base),
                    index: Box::new(index),
                };
                value = AstExpr::Load(Box::new(address.clone()));
                place = Some(address);
                continue;
            }
            if self.eat_symbol("(") {
                let args = self.parse_arguments_after_open()?;
                value = match &value {
                    AstExpr::Var(name) if self.env.type_of(name).is_none() => AstExpr::Call {
                        module: None,
                        name: name.clone(),
                        type_args: Vec::new(),
                        args,
                    },
                    _ => AstExpr::IndirectCall {
                        callee: Box::new(value),
                        args,
                    },
                };
                place = None;
                continue;
            }
            if self.eat_word("as") {
                let ty = self.parse_type()?;
                value = AstExpr::Cast {
                    val: Box::new(value),
                    to: ty,
                };
                place = None;
                continue;
            }
            break;
        }
        Ok((value, place))
    }

    fn parse_call_arguments(&mut self) -> Result<Vec<AstExpr>> {
        self.expect_symbol("(")?;
        self.parse_arguments_after_open()
    }

    fn parse_arguments_after_open(&mut self) -> Result<Vec<AstExpr>> {
        let mut args = Vec::new();
        if self.eat_symbol(")") {
            return Ok(args);
        }
        loop {
            args.push(self.parse_expression()?);
            if self.eat_symbol(")") {
                break;
            }
            self.expect_symbol(",")?;
        }
        Ok(args)
    }

    fn parse_primary(&mut self) -> Result<(AstExpr, Option<AstExpr>)> {
        let token = self
            .peek()
            .cloned()
            .ok_or_else(|| "expected expression".to_string())?;
        match token.kind {
            TokenKind::Number => {
                self.position += 1;
                if token.text.contains('.') || token.text.contains('e') || token.text.contains('E')
                {
                    let value = token
                        .text
                        .parse::<f64>()
                        .map_err(|_| format!("invalid float `{}`", token.text))?;
                    return Ok((AstExpr::ConstFloat(value, None), None));
                }
                if let Ok(value) = token.text.parse::<i64>() {
                    return Ok((AstExpr::ConstInt(value, None), None));
                }
                let value = token
                    .text
                    .parse::<u64>()
                    .map_err(|_| format!("invalid integer `{}`", token.text))?;
                Ok((AstExpr::ConstUint(value, None), None))
            }
            TokenKind::String => {
                self.position += 1;
                let bytes = decode_string_token(&token.text)
                    .ok_or_else(|| "malformed string literal".to_string())?;
                Ok((AstExpr::BytesLit(bytes), None))
            }
            TokenKind::Symbol => {
                if self.eat_symbol("(") {
                    let value = self.parse_expression()?;
                    self.expect_symbol(")")?;
                    return Ok((value, None));
                }
                Err(format!("unexpected symbol `{}`", token.text))
            }
            TokenKind::Word => {
                let word = token.lower.clone();
                if Self::special_call(&word) {
                    return self.parse_special_call(&token);
                }
                self.position += 1;
                if self.peek().is_some_and(|candidate| candidate.is("of"))
                    && !matches!(
                        word.as_str(),
                        "size"
                            | "align"
                            | "half"
                            | "square"
                            | "twice"
                            | "sum"
                            | "product"
                            | "difference"
                    )
                {
                    self.expect_word("of")?;
                    let base = self.parse_unary()?;
                    let base_ptr = storage_address(&base, self.env)
                        .ok_or_else(|| "field access requires an addressable base".to_string())?;
                    let address = AstExpr::FieldAddr {
                        base_ptr: Box::new(base_ptr),
                        field: word,
                    };
                    return Ok((AstExpr::Load(Box::new(address.clone())), Some(address)));
                }
                match word.as_str() {
                    "true" => Ok((AstExpr::ConstBool(true), None)),
                    "false" => Ok((AstExpr::ConstBool(false), None)),
                    "null" => Ok((AstExpr::ConstNull(AstType::ptr(AstType::U8)), None)),
                    "sum" if self.peek().is_some_and(|token| token.is("of")) => {
                        self.parse_aggregate_expression("sum")
                    }
                    "product" if self.peek().is_some_and(|token| token.is("of")) => {
                        self.parse_aggregate_expression("product")
                    }
                    "difference" if self.peek().is_some_and(|token| token.is("between")) => {
                        self.parse_aggregate_expression("difference")
                    }
                    "function" => {
                        let name = self
                            .peek()
                            .filter(|candidate| candidate.is_word())
                            .map(|candidate| candidate.lower.clone())
                            .ok_or_else(|| "expected function name".to_string())?;
                        self.position += 1;
                        Ok((
                            AstExpr::FnAddr {
                                module: None,
                                name,
                                type_args: Vec::new(),
                            },
                            None,
                        ))
                    }
                    _ => self.parse_identifier(&word),
                }
            }
        }
    }

    fn special_call(word: &str) -> bool {
        matches!(word, "bytes_ptr" | "bytes_len" | "syscall")
    }

    fn parse_special_call(&mut self, token: &Token) -> Result<(AstExpr, Option<AstExpr>)> {
        self.position += 1;
        self.expect_symbol("(")?;
        match token.lower.as_str() {
            "bytes_ptr" | "bytes_len" => {
                let string = self
                    .peek()
                    .filter(|value| value.kind == TokenKind::String)
                    .cloned()
                    .ok_or_else(|| format!("`{}` expects one string literal", token.lower))?;
                self.position += 1;
                self.expect_symbol(")")?;
                let bytes = decode_string_token(&string.text)
                    .ok_or_else(|| "malformed string literal".to_string())?;
                if token.lower == "bytes_ptr" {
                    Ok((AstExpr::BytesLit(bytes), None))
                } else {
                    Ok((AstExpr::BytesLen(bytes), None))
                }
            }
            "syscall" => {
                let num = self.parse_expression()?;
                let mut args = Vec::new();
                while self.eat_symbol(",") {
                    args.push(self.parse_expression()?);
                }
                self.expect_symbol(")")?;
                Ok((
                    AstExpr::Syscall {
                        num: Box::new(num),
                        args,
                    },
                    None,
                ))
            }
            _ => Err(format!("unsupported special call `{}`", token.lower)),
        }
    }

    fn parse_aggregate_expression(&mut self, kind: &str) -> Result<(AstExpr, Option<AstExpr>)> {
        if kind == "difference" {
            self.expect_word("between")?;
            let left = self.parse_multiplicative()?;
            self.expect_word("and")?;
            let right = self.parse_multiplicative()?;
            return Ok((binary(BinOp::Sub, left, right), None));
        }
        self.expect_word("of")?;
        let operator = if kind == "sum" {
            BinOp::Add
        } else {
            BinOp::Mul
        };
        let mut value = self.parse_multiplicative()?;
        while self.eat_word("and") || self.eat_symbol(",") {
            let next = self.parse_multiplicative()?;
            value = binary(operator, value, next);
        }
        Ok((value, None))
    }

    fn parse_identifier(&mut self, word: &str) -> Result<(AstExpr, Option<AstExpr>)> {
        if self.env.is_global(word) {
            let address = AstExpr::GlobalAddr {
                module: None,
                name: word.to_string(),
            };
            let value = AstExpr::Load(Box::new(address));
            return Ok((value, None));
        }
        Ok((AstExpr::Var(word.to_string()), None))
    }

    fn parse_type(&mut self) -> Result<AstType> {
        let token = self
            .peek()
            .cloned()
            .ok_or_else(|| "expected type".to_string())?;
        if token.kind != TokenKind::Word {
            return Err("expected type name".to_string());
        }
        self.position += 1;
        match token.lower.as_str() {
            "void" => Ok(AstType::Void),
            "bool" => Ok(AstType::Bool),
            "i8" => Ok(AstType::I8),
            "i16" => Ok(AstType::I16),
            "i32" => Ok(AstType::I32),
            "i64" => Ok(AstType::I64),
            "u8" => Ok(AstType::U8),
            "u16" => Ok(AstType::U16),
            "u32" => Ok(AstType::U32),
            "u64" => Ok(AstType::U64),
            "f32" => Ok(AstType::F32),
            "f64" => Ok(AstType::F64),
            "ptr" => {
                self.expect_symbol("<")?;
                let inner = self.parse_type()?;
                self.expect_symbol(">")?;
                Ok(AstType::ptr(inner))
            }
            "array" => {
                if self.eat_word("of") {
                    let count = self
                        .peek()
                        .and_then(|value| value.text.parse::<usize>().ok())
                        .ok_or_else(|| "expected array length".to_string())?;
                    self.position += 1;
                    let inner = self.parse_type()?;
                    return Ok(AstType::array(inner, count));
                }
                self.expect_symbol("<")?;
                let inner = self.parse_type()?;
                self.expect_symbol(",")?;
                let count = self
                    .peek()
                    .and_then(|value| value.text.parse::<usize>().ok())
                    .ok_or_else(|| "expected array length".to_string())?;
                self.position += 1;
                self.expect_symbol(">")?;
                Ok(AstType::array(inner, count))
            }
            name => {
                let mut type_args = Vec::new();
                if self.eat_symbol("<") {
                    loop {
                        type_args.push(self.parse_type()?);
                        if self.eat_symbol(">") {
                            break;
                        }
                        self.expect_symbol(",")?;
                    }
                }
                Ok(AstType::Named {
                    module: None,
                    name: name.to_string(),
                    type_args,
                })
            }
        }
    }
}

fn binary(op: BinOp, lhs: AstExpr, rhs: AstExpr) -> AstExpr {
    AstExpr::Binary {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
    }
}

fn storage_address(value: &AstExpr, env: &ExprEnv) -> Option<AstExpr> {
    match value {
        AstExpr::Var(name) => {
            if env.is_global(name) {
                Some(AstExpr::GlobalAddr {
                    module: None,
                    name: name.clone(),
                })
            } else if matches!(env.type_of(name), Some(AstType::Ptr(_))) {
                Some(AstExpr::Var(name.clone()))
            } else {
                Some(AstExpr::AddrOfVar(name.clone()))
            }
        }
        AstExpr::Load(inner) => Some((**inner).clone()),
        AstExpr::FieldAddr { .. } | AstExpr::IndexAddr { .. } => Some(value.clone()),
        _ => None,
    }
}

fn explicit_address(value: &AstExpr, env: &ExprEnv) -> Option<AstExpr> {
    match value {
        AstExpr::Var(name) => {
            if env.is_global(name) {
                Some(AstExpr::GlobalAddr {
                    module: None,
                    name: name.clone(),
                })
            } else {
                Some(AstExpr::AddrOfVar(name.clone()))
            }
        }
        AstExpr::Load(inner) => Some((**inner).clone()),
        AstExpr::FieldAddr { .. } | AstExpr::IndexAddr { .. } => Some(value.clone()),
        _ => None,
    }
}

/// Parses a complete expression, requiring all tokens to be consumed.
pub(crate) fn parse_expression(text: &str, env: &ExprEnv) -> Result<AstExpr> {
    let tokens = tokenize(text);
    let mut parser = Parser {
        tokens: &tokens,
        position: 0,
        env,
    };
    let value = parser.parse_expression()?;
    if !parser.at_end() {
        let leftover = parser
            .tokens
            .get(parser.position)
            .map_or_else(String::new, |token| token.text.clone());
        return Err(format!("unexpected trailing token `{leftover}`"));
    }
    Ok(value)
}

/// Parses an expression as an addressable place and returns its address.
pub(crate) fn parse_address(text: &str, env: &ExprEnv) -> Result<AstExpr> {
    let tokens = tokenize(text);
    if tokens.is_empty() {
        return Err("empty assignment target".to_string());
    }
    if tokens[0].text == "&" || tokens[0].is("address") {
        let offset = if tokens[0].is("address") { 2 } else { 1 };
        let rest = tokens
            .get(offset..)
            .ok_or_else(|| "expected address operand".to_string())?;
        let mut parser = Parser {
            tokens: rest,
            position: 0,
            env,
        };
        let value = parser.parse_expression()?;
        if !parser.at_end() {
            return Err("unexpected trailing token after address operand".to_string());
        }
        return explicit_address(&value, env)
            .ok_or_else(|| "expression is not addressable".to_string());
    }
    if tokens[0].text == "*" || tokens[0].is("deref") {
        let rest = &tokens[1..];
        let mut parser = Parser {
            tokens: rest,
            position: 0,
            env,
        };
        let value = parser.parse_expression()?;
        if !parser.at_end() {
            return Err("unexpected trailing token after dereference".to_string());
        }
        return Ok(value);
    }
    let mut parser = Parser {
        tokens: &tokens,
        position: 0,
        env,
    };
    let value = parser.parse_expression()?;
    if !parser.at_end() {
        return Err("unexpected trailing token after assignment target".to_string());
    }
    storage_address(&value, env).ok_or_else(|| "assignment target is not addressable".to_string())
}

/// Parses a standalone type.
pub(crate) fn parse_type_text(text: &str) -> Result<AstType> {
    let tokens = tokenize(text);
    let env = ExprEnv::default();
    let mut parser = Parser {
        tokens: &tokens,
        position: 0,
        env: &env,
    };
    let ty = parser.parse_type()?;
    if !parser.at_end() {
        return Err("unexpected trailing token in type".to_string());
    }
    Ok(ty)
}

#[cfg(test)]
mod tests {
    use super::{ExprEnv, parse_address, parse_expression};

    #[test]
    fn arithmetic_and_comparisons() {
        let env = ExprEnv::default();
        assert!(parse_expression("sum of 1 and 2 and 3", &env).is_ok());
        assert!(parse_expression("10 percent of 50", &env).is_ok());
        assert!(parse_expression("x more than 5", &env).is_ok());
        assert!(parse_expression("1 + 2 * 3", &env).is_ok());
    }

    #[test]
    fn addresses_require_places() {
        let env = ExprEnv::default();
        assert!(parse_address("x", &env).is_ok());
        assert!(parse_address("1 + 2", &env).is_err());
    }
}
