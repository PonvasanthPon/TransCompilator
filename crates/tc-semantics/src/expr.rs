//! Deterministic English expression parser used by the unified lowerer.
//!
//! The parser is deliberately finite and total: a successful parse consumes
//! every token, so prose cannot silently become an expression.

use std::collections::HashMap;

use tc_ir::{BinOp, UnOp};
use tc_typeck::AstExpr;
use tc_typeck::AstType;

use crate::token::{Token, TokenKind, decode_string_token, tokenize};

const MAX_GROUP_DEPTH: usize = 16;

fn groups_exceed_limit(tokens: &[Token]) -> bool {
    let mut depth = 0_usize;
    for token in tokens {
        if token.kind != TokenKind::Symbol {
            continue;
        }
        match token.text.as_str() {
            "(" | "[" | "{" => {
                depth += 1;
                if depth > MAX_GROUP_DEPTH {
                    return true;
                }
            }
            ")" | "]" | "}" => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    false
}

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

    pub(crate) fn expr_type(&self, expr: &AstExpr) -> Option<AstType> {
        match expr {
            AstExpr::ConstInt(_, Some(ty)) => Some(ty.clone()),
            AstExpr::ConstInt(..) => Some(AstType::I64),
            AstExpr::ConstUint(_, Some(ty)) => Some(ty.clone()),
            AstExpr::ConstUint(..) => Some(AstType::U64),
            AstExpr::ConstFloat(_, Some(ty)) => Some(ty.clone()),
            AstExpr::ConstFloat(..) => Some(AstType::F64),
            AstExpr::ConstBool(_) => Some(AstType::Bool),
            AstExpr::ConstNull(ty) => Some(ty.clone()),
            AstExpr::Zero(ty) => Some(ty.clone()),
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
            AstExpr::FnAddr { .. } => None,
            AstExpr::Call { .. } | AstExpr::IndirectCall { .. } | AstExpr::Syscall { .. } => None,
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
        }
    }
}

struct Parser<'a> {
    tokens: &'a [Token],
    position: usize,
    env: &'a ExprEnv,
    budget: usize,
    type_depth: usize,
    group_depth: usize,
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
            let saved = self.position;
            if self.eat_word("has") {
                let no_address = self.eat_word("no") && self.eat_word("address");
                let an_address = self.eat_word("an") && self.eat_word("address");
                if no_address || an_address {
                    let null = AstExpr::ConstNull(AstType::ptr(AstType::U8));
                    let address = AstExpr::Cast {
                        val: Box::new(left),
                        to: AstType::ptr(AstType::U8),
                    };
                    left = binary(
                        if no_address { BinOp::Eq } else { BinOp::Ne },
                        address,
                        null,
                    );
                    continue;
                }
                self.position = saved;
            }
            if self.eat_word("is") {
                if self.eat_word("between") {
                    self.position -= 1;
                    left = self.parse_between(left)?;
                    continue;
                }
                let null_check = if self.eat_word("empty") {
                    Some(true)
                } else {
                    let saved_not = self.position;
                    if self.eat_word("not") && self.eat_word("empty") {
                        Some(false)
                    } else {
                        self.position = saved_not;
                        None
                    }
                };
                if let Some(is_empty) = null_check {
                    let null = AstExpr::ConstNull(AstType::ptr(AstType::U8));
                    let address = AstExpr::Cast {
                        val: Box::new(left),
                        to: AstType::ptr(AstType::U8),
                    };
                    left = binary(if is_empty { BinOp::Eq } else { BinOp::Ne }, address, null);
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
            if self.words_are(&["bytes", "after"]) || self.words_are(&["bytes", "before"]) {
                let before = self.words_are(&["bytes", "before"]);
                self.position += 2;
                let pointer = self.parse_unary()?;
                let target = if self.eat_word("as") {
                    self.parse_type()?
                } else if let Some(pointer_type @ AstType::Ptr(_)) = self.env.expr_type(&pointer) {
                    pointer_type
                } else {
                    AstType::ptr(AstType::U8)
                };
                let address = AstExpr::Cast {
                    val: Box::new(pointer),
                    to: AstType::U64,
                };
                let offset = AstExpr::Cast {
                    val: Box::new(left),
                    to: AstType::U64,
                };
                let sum = binary(
                    if before { BinOp::Sub } else { BinOp::Add },
                    address,
                    offset,
                );
                left = AstExpr::Cast {
                    val: Box::new(sum),
                    to: target,
                };
                continue;
            }
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
            } else if self.eat_symbol("/")
                || self.eat_word("over")
                || self.eat_word("mod")
                || self.eat_word("modulo")
            {
                if self.tokens[self.position - 1].is("mod")
                    || self.tokens[self.position - 1].is("modulo")
                {
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
        if self.eat_word("apply") {
            let callee = self.parse_unary()?;
            self.expect_word("to")?;
            let first = self.parse_unary()?;
            self.expect_word("with")?;
            let second = self.parse_unary()?;
            return Ok(AstExpr::IndirectCall {
                callee: Box::new(callee),
                args: vec![first, second],
            });
        }
        if self.eat_word("convert") {
            let value = self.parse_unary()?;
            self.expect_word("to")?;
            let to = self.parse_type()?;
            return Ok(AstExpr::Cast {
                val: Box::new(value),
                to,
            });
        }
        if self.words_are(&["byte", "size", "of"]) {
            self.position += 3;
            let ty = self.parse_type()?;
            return Ok(AstExpr::SizeOf(ty));
        }
        if self.words_are(&["alignment", "of"]) || self.words_are(&["align", "of"]) {
            self.position += 2;
            let ty = self.parse_type()?;
            return Ok(AstExpr::AlignOf(ty));
        }
        if self.words_are(&["memory", "location", "of"]) {
            self.position += 3;
            let (value, place) = self.parse_primary()?;
            let address = explicit_address(&value, self.env)
                .or(place)
                .ok_or_else(|| "memory location requires an addressable value".to_string())?;
            if self.eat_word("as") {
                let to = self.parse_type()?;
                return Ok(AstExpr::Cast {
                    val: Box::new(address),
                    to,
                });
            }
            return Ok(address);
        }
        if self.words_are(&["value", "at"]) {
            self.position += 2;
            return Ok(AstExpr::Load(Box::new(self.parse_unary()?)));
        }
        if matches!(self.peek_lower(0), Some("item" | "element")) {
            let saved = self.position;
            self.position += 1;
            let index = if self
                .peek()
                .is_some_and(|token| token.kind == TokenKind::Number)
            {
                self.parse_unary()?
            } else if self.peek_lower(0).is_some() && self.peek_lower(1) == Some("of") {
                let name = self.peek_lower(0).unwrap_or_default().to_string();
                self.position += 1;
                AstExpr::Var(name)
            } else {
                let result = self.parse_unary();
                if result.is_err() {
                    self.position = saved;
                    return Err("item index is invalid".to_string());
                }
                result?
            };
            if self.eat_word("of") {
                let pointer = self.parse_unary()?;
                let base = deref_address(&pointer, self.env)
                    .ok_or_else(|| "item access requires a pointer".to_string())?;
                return Ok(AstExpr::Load(Box::new(AstExpr::IndexAddr {
                    base_ptr: Box::new(base),
                    index: Box::new(index),
                })));
            }
            self.position = saved;
        }
        if self.words_are(&["keep", "bits"]) {
            self.position += 2;
            let mask = self.parse_unary()?;
            self.expect_word("of")?;
            let value = self.parse_unary()?;
            let mask = if let Some(ty) = self.env.expr_type(&value) {
                AstExpr::Cast {
                    val: Box::new(mask),
                    to: ty,
                }
            } else {
                mask
            };
            return Ok(binary(BinOp::BitAnd, value, mask));
        }
        if self.words_are(&["bitwise", "and", "of"]) {
            self.position += 3;
            let left = self.parse_unary()?;
            self.expect_word("and")?;
            let right = self.parse_unary()?;
            return Ok(binary(BinOp::BitAnd, left, right));
        }
        if self.words_are(&["bitwise", "or", "of"]) {
            self.position += 3;
            let left = self.parse_unary()?;
            self.expect_word("and")?;
            let right = self.parse_unary()?;
            return Ok(binary(BinOp::BitOr, left, right));
        }
        if self.words_are(&["exclusive", "or", "of"]) {
            self.position += 3;
            let left = self.parse_unary()?;
            self.expect_word("and")?;
            let right = self.parse_unary()?;
            return Ok(binary(BinOp::BitXor, left, right));
        }
        if self.words_are(&["bitwise", "inverse", "of"]) {
            self.position += 3;
            return Ok(AstExpr::Unary {
                op: UnOp::BitNot,
                val: Box::new(self.parse_unary()?),
            });
        }
        if self.words_are(&["shift"]) {
            self.position += 1;
            let value = self.parse_unary()?;
            let right = if self.eat_word("left") {
                false
            } else if self.eat_word("right") {
                true
            } else {
                return Err("shift requires `left` or `right`".to_string());
            };
            self.expect_word("by")?;
            let amount = self.parse_unary()?;
            return Ok(binary(
                if right { BinOp::Shr } else { BinOp::Shl },
                value,
                amount,
            ));
        }
        if self.words_are(&["ask", "linux", "kernel"]) {
            self.position += 3;
            let number = self.parse_unary()?;
            self.expect_word("with")?;
            let mut args = Vec::new();
            loop {
                self.eat_word("and");
                args.push(self.parse_expression()?);
                if !self.eat_symbol(",") {
                    break;
                }
            }
            return Ok(AstExpr::Syscall {
                num: Box::new(number),
                args,
            });
        }
        if self.eat_symbol("-") {
            let value = self.parse_unary()?;
            if let AstExpr::ConstUint(integer, None) = value {
                if integer <= (1_u64 << 63) {
                    let signed = if integer == (1_u64 << 63) {
                        i64::MIN
                    } else {
                        -(integer as i64)
                    };
                    return Ok(AstExpr::ConstInt(signed, None));
                }
            }
            return Ok(AstExpr::Unary {
                op: UnOp::Neg,
                val: Box::new(value),
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
                    if self.group_depth >= MAX_GROUP_DEPTH {
                        return Err("expression nesting exceeds the Core-7 limit".to_string());
                    }
                    self.group_depth += 1;
                    let value = self.parse_expression()?;
                    self.group_depth -= 1;
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
                    "empty" | "no" if self.peek().is_some_and(|token| token.is("address")) => {
                        self.position += 1;
                        Ok((AstExpr::ConstNull(AstType::ptr(AstType::U8)), None))
                    }
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

    fn peek_lower(&self, offset: usize) -> Option<&str> {
        self.tokens
            .get(self.position + offset)
            .filter(|token| token.kind == TokenKind::Word)
            .map(|token| token.lower.as_str())
    }

    fn words_are(&self, words: &[&str]) -> bool {
        words
            .iter()
            .enumerate()
            .all(|(offset, word)| self.peek_lower(offset) == Some(*word))
    }

    fn words_are_any(&self, words: &[&str]) -> bool {
        words.iter().enumerate().all(|(offset, word)| {
            self.tokens
                .get(self.position + offset)
                .is_some_and(|token| token.lower == *word)
        })
    }

    fn skip_type_articles(&mut self) {
        while matches!(self.peek_lower(0), Some("a" | "an" | "the")) {
            self.position += 1;
        }
    }

    fn parse_natural_array(&mut self, prefix_words: usize) -> Result<AstType> {
        self.position += prefix_words;
        let count = self
            .peek()
            .filter(|token| token.kind == TokenKind::Number)
            .and_then(|value| value.text.parse::<usize>().ok())
            .ok_or_else(|| "expected array length".to_string())?;
        self.position += 1;
        let inner = self.parse_type()?;
        Ok(AstType::array(inner, count))
    }

    fn parse_type_inner(&mut self) -> Result<AstType> {
        self.skip_type_articles();
        for (words, ty) in [
            (&["32", "-", "bit", "number"][..], AstType::I32),
            (&["unsigned", "32", "-", "bit", "number"][..], AstType::U32),
            (&["16", "-", "bit", "number"][..], AstType::I16),
            (&["unsigned", "16", "-", "bit", "number"][..], AstType::U16),
            (&["32", "-", "bit", "decimal"][..], AstType::F32),
            (&["signed", "32", "-", "bit", "number"][..], AstType::I32),
            (&["signed", "16", "-", "bit", "number"][..], AstType::I16),
        ] {
            if self.words_are_any(words) {
                self.position += words.len();
                return Ok(ty);
            }
        }
        let token = self
            .peek()
            .cloned()
            .ok_or_else(|| "expected type".to_string())?;
        if token.kind != TokenKind::Word {
            return Err("expected type name".to_string());
        }
        if self.words_are(&["whole", "number"])
            || self.words_are(&["signed", "number"])
            || self.words_are(&["integer"])
            || self.words_are(&["number"])
            || self.words_are(&["signed", "whole", "number"])
        {
            let count = if self.words_are(&["signed", "whole", "number"]) {
                3
            } else if self.words_are(&["whole", "number"]) || self.words_are(&["signed", "number"])
            {
                2
            } else {
                1
            };
            self.position += count;
            return Ok(AstType::I64);
        }
        if self.words_are(&["unsigned", "number"])
            || self.words_are(&["unsigned", "whole", "number"])
            || self.words_are(&["count"])
        {
            self.position += if self.words_are(&["unsigned", "whole", "number"]) {
                3
            } else if self.words_are(&["unsigned", "number"]) {
                2
            } else {
                1
            };
            return Ok(AstType::U64);
        }
        if self.words_are(&["small", "number"])
            || self.words_are(&["signed", "32-bit", "number"])
            || self.words_are(&["32", "-", "bit", "number"])
        {
            self.position += if self.words_are(&["32", "-", "bit", "number"]) {
                4
            } else if self.words_are(&["signed", "32-bit", "number"]) {
                3
            } else {
                2
            };
            return Ok(AstType::I32);
        }
        if self.words_are(&["unsigned", "32-bit", "number"])
            || self.words_are(&["unsigned", "32", "-", "bit", "number"])
        {
            self.position += if self.words_are(&["unsigned", "32", "-", "bit", "number"]) {
                5
            } else {
                3
            };
            return Ok(AstType::U32);
        }
        if self.words_are(&["short", "number"])
            || self.words_are(&["signed", "16-bit", "number"])
            || self.words_are(&["16", "-", "bit", "number"])
        {
            self.position += if self.words_are(&["16", "-", "bit", "number"]) {
                4
            } else if self.words_are(&["signed", "16-bit", "number"]) {
                3
            } else {
                2
            };
            return Ok(AstType::I16);
        }
        if self.words_are(&["unsigned", "16-bit", "number"])
            || self.words_are(&["unsigned", "16", "-", "bit", "number"])
        {
            self.position += if self.words_are(&["unsigned", "16", "-", "bit", "number"]) {
                5
            } else {
                3
            };
            return Ok(AstType::U16);
        }
        if self.words_are(&["signed", "byte"]) {
            self.position += 2;
            return Ok(AstType::I8);
        }
        if self.words_are(&["byte"]) {
            self.position += 1;
            return Ok(AstType::U8);
        }
        if self.words_are(&["decimal", "number"])
            || self.words_are(&["real", "number"])
            || self.words_are(&["decimal"])
        {
            self.position += if self.words_are(&["decimal"]) { 1 } else { 2 };
            return Ok(AstType::F64);
        }
        if self.words_are(&["short", "decimal"])
            || self.words_are(&["32-bit", "decimal"])
            || self.words_are(&["32", "-", "bit", "decimal"])
        {
            self.position += if self.words_are(&["32", "-", "bit", "decimal"]) {
                4
            } else {
                2
            };
            return Ok(AstType::F32);
        }
        if self.words_are(&["truth", "value"]) {
            self.position += 2;
            return Ok(AstType::Bool);
        }
        if self.words_are(&["boolean"]) || self.words_are(&["flag"]) {
            self.position += 1;
            return Ok(AstType::Bool);
        }
        if self.words_are(&["nothing"]) {
            self.position += 1;
            return Ok(AstType::Void);
        }
        if self.words_are(&["pointer", "to"]) || self.words_are(&["address", "of"]) {
            self.position += 2;
            let inner = self.parse_type()?;
            return Ok(AstType::ptr(inner));
        }
        if self.words_are(&["block", "of"]) {
            return self.parse_natural_array(2);
        }
        if self.words_are(&["fixed"]) {
            return self.parse_natural_array(1);
        }
        if self.words_are(&["function", "taking"]) {
            self.position += 2;
            let mut params = Vec::new();
            loop {
                params.push(self.parse_type()?);
                if self.eat_word("and") || self.eat_symbol(",") {
                    continue;
                }
                break;
            }
            let ret = if self.eat_word("returning") {
                self.parse_type()?
            } else {
                AstType::Void
            };
            return Ok(AstType::FnPtr {
                params,
                ret: Box::new(ret),
            });
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
                } else if self.eat_word("of") {
                    loop {
                        type_args.push(self.parse_type()?);
                        if self.eat_word("and") || self.eat_symbol(",") {
                            continue;
                        }
                        break;
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
    if groups_exceed_limit(&tokens) {
        return Err("expression nesting exceeds the Core-7 limit".to_string());
    }
    let mut parser = Parser {
        tokens: &tokens,
        position: 0,
        env,
        budget: 1024,
        type_depth: 0,
        group_depth: 0,
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
            group_depth: 0,
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
            group_depth: 0,
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
        group_depth: 0,
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
        group_depth: 0,
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
    fn natural_type_phrases_parse() {
        use super::parse_type_text;
        for text in [
            "a whole number",
            "an unsigned number",
            "a truth value",
            "a pointer to byte",
            "Result of T and E",
            "Option of T",
            "List of T",
            "block of 8 byte",
            "function taking pointer to T and pointer to T returning whole number",
        ] {
            assert!(
                parse_type_text(text).is_ok(),
                "{text}: {:?}",
                parse_type_text(text)
            );
        }
    }

    #[test]
    fn addresses_require_places() {
        let env = ExprEnv::default();
        assert!(parse_address("x", &env).is_ok());
        assert!(parse_address("1 + 2", &env).is_err());
    }
}
