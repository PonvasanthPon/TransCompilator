//! Deterministic English expression parser used by the unified lowerer.
//!
//! The parser is deliberately finite and total: a successful parse consumes
//! every token, so prose cannot silently become an expression.

use std::collections::HashMap;

use tc_ir::{BinOp, UnOp};
use tc_typeck::AstExpr;
use tc_typeck::AstType;

use crate::token::{Token, TokenKind, decode_string_token, tokenize};

/// Field metadata for one struct or union.
#[derive(Debug, Clone, Default)]
pub(crate) struct AggregateInfo {
    /// Generic parameter names in declaration order.
    pub type_params: Vec<String>,
    /// Field name to declared field type.
    pub fields: HashMap<String, AstType>,
}

/// Name-resolution environment for expression parsing.
#[derive(Debug, Clone, Default)]
pub(crate) struct ExprEnv {
    /// Locals visible at the current point.
    pub locals: HashMap<String, AstType>,
    /// Module-level globals.
    pub globals: HashMap<String, AstType>,
    /// Aggregate field metadata for place typing.
    pub aggregates: HashMap<String, AggregateInfo>,
    /// Imported module names and aliases.
    pub modules: Vec<String>,
}

impl ExprEnv {
    fn type_of(&self, name: &str) -> Option<&AstType> {
        self.locals.get(name).or_else(|| self.globals.get(name))
    }

    fn is_global(&self, name: &str) -> bool {
        !self.locals.contains_key(name) && self.globals.contains_key(name)
    }

    fn expr_type(&self, expr: &AstExpr) -> Option<AstType> {
        match expr {
            AstExpr::ConstInt(_, Some(ty)) => Some(ty.clone()),
            AstExpr::ConstInt(..) => Some(AstType::I64),
            AstExpr::ConstUint(_, Some(ty)) => Some(ty.clone()),
            AstExpr::ConstUint(..) => Some(AstType::U64),
            AstExpr::ConstFloat(_, Some(ty)) => Some(ty.clone()),
            AstExpr::ConstFloat(..) => Some(AstType::F64),
            AstExpr::ConstBool(_) => Some(AstType::Bool),
            AstExpr::ConstNull(ty) => Some(ty.clone()),
            AstExpr::BytesLit(_) => Some(AstType::ptr(AstType::U8)),
            AstExpr::BytesLen(_) => Some(AstType::U64),
            AstExpr::Var(name) => self.type_of(name).cloned(),
            AstExpr::GlobalAddr { name, .. } => self.type_of(name).cloned().map(AstType::ptr),
            AstExpr::AddrOfVar(name) => self.type_of(name).cloned().map(AstType::ptr),
            AstExpr::Load(pointer) => self.expr_type(pointer).and_then(|ty| match ty {
                AstType::Ptr(inner) => Some(*inner),
                _ => None,
            }),
            AstExpr::IndexAddr { base_ptr, .. } => {
                let base = self.expr_type(base_ptr)?;
                let pointee = match base {
                    AstType::Ptr(inner) => *inner,
                    _ => return None,
                };
                let element = match pointee {
                    AstType::Array(inner, _) => *inner,
                    other => other,
                };
                Some(AstType::ptr(element))
            }
            AstExpr::FieldAddr { base_ptr, field } => {
                let base = self.expr_type(base_ptr)?;
                let pointee = match base {
                    AstType::Ptr(inner) => *inner,
                    _ => return None,
                };
                let AstType::Named {
                    name, type_args, ..
                } = pointee
                else {
                    return None;
                };
                let info = self.aggregates.get(&name)?;
                let substitutions = info
                    .type_params
                    .iter()
                    .cloned()
                    .zip(type_args)
                    .collect::<HashMap<_, _>>();
                info.fields
                    .get(field)
                    .map(|ty| AstType::ptr(substitute_type(ty, &substitutions)))
            }
            AstExpr::Cast { to, .. } => Some(to.clone()),
            AstExpr::SizeOf(_) | AstExpr::AlignOf(_) => Some(AstType::U64),
            AstExpr::Unary { op, val } => match op {
                tc_ir::UnOp::LogNot => Some(AstType::Bool),
                _ => self.expr_type(val),
            },
            AstExpr::Binary { op, lhs, .. } => {
                if matches!(
                    op,
                    tc_ir::BinOp::Eq
                        | tc_ir::BinOp::Ne
                        | tc_ir::BinOp::Lt
                        | tc_ir::BinOp::Le
                        | tc_ir::BinOp::Gt
                        | tc_ir::BinOp::Ge
                        | tc_ir::BinOp::LogAnd
                        | tc_ir::BinOp::LogOr
                ) {
                    Some(AstType::Bool)
                } else {
                    self.expr_type(lhs)
                }
            }
            _ => None,
        }
    }
}

struct Parser<'a> {
    tokens: &'a [Token],
    position: usize,
    env: &'a ExprEnv,
    budget: usize,
    type_depth: usize,
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

    fn charge(&mut self) -> Result<()> {
        if self.budget == 0 {
            return Err("expression exceeds the Core-7 complexity limit".to_string());
        }
        self.budget -= 1;
        Ok(())
    }

    fn parse_expression(&mut self) -> Result<AstExpr> {
        self.charge()?;
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<AstExpr> {
        let mut left = self.parse_and()?;
        while self.eat_word("or") || self.eat_symbol("||") {
            self.charge()?;
            let right = self.parse_and()?;
            left = binary(BinOp::LogOr, left, right);
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<AstExpr> {
        let mut left = self.parse_not()?;
        while self.eat_word("and") || self.eat_symbol("&&") {
            self.charge()?;
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
            self.charge()?;
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
            self.charge()?;
            let right = self.parse_unary()?;
            left = binary(op, left, right);
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<AstExpr> {
        self.charge()?;
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
        if self
            .peek()
            .is_some_and(|token| token.is("size") || token.is("align"))
            && self
                .tokens
                .get(self.position + 1)
                .is_some_and(|token| token.is("of"))
        {
            let alignment = self.tokens[self.position].is("align");
            self.position += 1;
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
        if self
            .peek()
            .is_some_and(|token| token.is("half") || token.is("square"))
            && self
                .tokens
                .get(self.position + 1)
                .is_some_and(|token| token.is("of"))
        {
            let square = self.tokens[self.position].is("square");
            self.position += 1;
            self.expect_word("of")?;
            let value = self.parse_unary()?;
            return Ok(if square {
                binary(BinOp::Mul, value.clone(), value)
            } else {
                binary(BinOp::Div, value, AstExpr::ConstInt(2, None))
            });
        }
        let (value, _) = self.parse_postfix()?;
        Ok(value)
    }

    fn parse_postfix(&mut self) -> Result<(AstExpr, Option<AstExpr>)> {
        let (mut value, mut place) = self.parse_primary()?;
        loop {
            self.charge()?;
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
                let base = deref_address(&value, self.env)
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
                let base = deref_address(&value, self.env)
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
        self.charge()?;
        let token = self
            .peek()
            .cloned()
            .ok_or_else(|| "expected expression".to_string())?;
        match token.kind {
            TokenKind::Number => {
                self.position += 1;
                let normalized = token.text.replace('_', "");
                let radix = if normalized.starts_with("0x") || normalized.starts_with("0X") {
                    Some((16, &normalized[2..]))
                } else if normalized.starts_with("0b") || normalized.starts_with("0B") {
                    Some((2, &normalized[2..]))
                } else if normalized.starts_with("0o") || normalized.starts_with("0O") {
                    Some((8, &normalized[2..]))
                } else {
                    None
                };
                if let Some((radix, digits)) = radix {
                    let value = u64::from_str_radix(digits, radix)
                        .map_err(|_| format!("invalid integer `{}`", token.text))?;
                    return Ok(if let Ok(value) = i64::try_from(value) {
                        (AstExpr::ConstInt(value, None), None)
                    } else {
                        (AstExpr::ConstUint(value, None), None)
                    });
                }
                if normalized.contains('.') || normalized.contains('e') || normalized.contains('E')
                {
                    let value = normalized
                        .parse::<f64>()
                        .map_err(|_| format!("invalid float `{}`", token.text))?;
                    return Ok((AstExpr::ConstFloat(value, None), None));
                }
                if let Ok(value) = normalized.parse::<i64>() {
                    return Ok((AstExpr::ConstInt(value, None), None));
                }
                let value = normalized
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
                    let base_ptr = deref_address(&base, self.env)
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
        if self.type_depth >= 64 {
            return Err("type nesting exceeds the Core-7 limit".to_string());
        }
        self.type_depth += 1;
        let result = self.parse_type_inner();
        self.type_depth -= 1;
        result
    }

    fn parse_type_inner(&mut self) -> Result<AstType> {
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
            "fn" | "function" => {
                self.expect_symbol("(")?;
                let mut params = Vec::new();
                if !self.eat_symbol(")") {
                    loop {
                        params.push(self.parse_type()?);
                        if self.eat_symbol(")") {
                            break;
                        }
                        self.expect_symbol(",")?;
                    }
                }
                let ret = if self.eat_symbol("->") || self.eat_word("returning") {
                    self.parse_type()?
                } else {
                    AstType::Void
                };
                Ok(AstType::FnPtr {
                    params,
                    ret: Box::new(ret),
                })
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
                let mut module = None;
                let mut type_name = name.to_string();
                if self.eat_symbol(".") {
                    let next = self
                        .peek()
                        .filter(|token| token.is_word())
                        .map(|token| token.lower.clone())
                        .ok_or_else(|| "expected type name after module qualifier".to_string())?;
                    self.position += 1;
                    module = Some(name.to_string());
                    type_name = next;
                }
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
                    module,
                    name: type_name,
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

fn substitute_type(ty: &AstType, substitutions: &HashMap<String, AstType>) -> AstType {
    match ty {
        AstType::Named {
            module,
            name,
            type_args,
        } => {
            if module.is_none() {
                if let Some(replacement) = substitutions.get(name) {
                    return replacement.clone();
                }
            }
            AstType::Named {
                module: module.clone(),
                name: name.clone(),
                type_args: type_args
                    .iter()
                    .map(|arg| substitute_type(arg, substitutions))
                    .collect(),
            }
        }
        AstType::Ptr(inner) => AstType::Ptr(Box::new(substitute_type(inner, substitutions))),
        AstType::Array(inner, count) => {
            AstType::Array(Box::new(substitute_type(inner, substitutions)), *count)
        }
        AstType::FnPtr { params, ret } => AstType::FnPtr {
            params: params
                .iter()
                .map(|param| substitute_type(param, substitutions))
                .collect(),
            ret: Box::new(substitute_type(ret, substitutions)),
        },
        other => other.clone(),
    }
}

fn place_address(value: &AstExpr, env: &ExprEnv) -> Option<AstExpr> {
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

fn deref_address(value: &AstExpr, env: &ExprEnv) -> Option<AstExpr> {
    if matches!(env.expr_type(value), Some(AstType::Ptr(_))) {
        Some(value.clone())
    } else {
        place_address(value, env)
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
        AstExpr::Load(inner) => {
            if let AstExpr::GlobalAddr { name, .. } = inner.as_ref() {
                if matches!(env.type_of(name), Some(AstType::Ptr(_))) {
                    return Some(value.clone());
                }
            }
            Some((**inner).clone())
        }
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
        budget: 1024,
        type_depth: 0,
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
            budget: 1024,
            type_depth: 0,
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
            budget: 1024,
            type_depth: 0,
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
        budget: 1024,
        type_depth: 0,
    };
    let value = parser.parse_expression()?;
    if !parser.at_end() {
        return Err("unexpected trailing token after assignment target".to_string());
    }
    place_address(&value, env).ok_or_else(|| "assignment target is not addressable".to_string())
}

/// Parses an expression used as a store address.  A pointer value is used
/// directly; a non-pointer place yields its address.
pub(crate) fn parse_store_address(text: &str, env: &ExprEnv) -> Result<AstExpr> {
    let value = parse_expression(text, env)?;
    deref_address(&value, env).ok_or_else(|| "store target is not addressable".to_string())
}

/// Parses a standalone type.
pub(crate) fn parse_type_text(text: &str) -> Result<AstType> {
    let tokens = tokenize(text);
    let env = ExprEnv::default();
    let mut parser = Parser {
        tokens: &tokens,
        position: 0,
        env: &env,
        budget: 1024,
        type_depth: 0,
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
