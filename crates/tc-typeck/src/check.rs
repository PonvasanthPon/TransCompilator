//! Type checking, lexical scopes, overload resolution, and typed AST output.

use std::collections::{HashMap, HashSet};

use tc_ir::{BinOp, UnOp};

use crate::ast::{
    AstExpr, AstFn, AstStmt, AstType, PhraseEntry, PhrasePart, Span, TypeError, TypeErrorKind,
};
use crate::resolve::{ConcreteType, FnItem, ResolvedModule, ResolvedProgram, TypeItem};

/// A typed expression.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TExpr {
    pub ty: ConcreteType,
    pub kind: TExprKind,
}

/// Typed expression forms.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TExprKind {
    ConstInt(i64),
    ConstUint(u64),
    ConstFloat(f64),
    ConstBool(bool),
    ConstNull,
    BytesLit(Vec<u8>),
    BytesLen(Vec<u8>),
    Var(String),
    GlobalAddr {
        module: usize,
        index: usize,
    },
    AddrOfVar(String),
    FieldAddr {
        base: Box<TExpr>,
        field_idx: usize,
    },
    IndexAddr {
        base: Box<TExpr>,
        index: Box<TExpr>,
    },
    Load(Box<TExpr>),
    SizeOf(ConcreteType),
    AlignOf(ConcreteType),
    Unary {
        op: UnOp,
        val: Box<TExpr>,
    },
    Binary {
        op: BinOp,
        lhs: Box<TExpr>,
        rhs: Box<TExpr>,
    },
    Cast {
        val: Box<TExpr>,
    },
    FnAddr {
        module: usize,
        index: usize,
        type_args: Vec<ConcreteType>,
    },
    Call {
        module: usize,
        index: usize,
        type_args: Vec<ConcreteType>,
        args: Vec<TExpr>,
    },
    IndirectCall {
        callee: Box<TExpr>,
        args: Vec<TExpr>,
    },
    Syscall {
        num: Box<TExpr>,
        args: Vec<TExpr>,
    },
}

/// A typed statement.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TStmt {
    Let {
        name: String,
        ty: ConcreteType,
        init: TExpr,
        span: Span,
    },
    Assign {
        name: String,
        val: TExpr,
        span: Span,
    },
    Store {
        ptr: TExpr,
        val: TExpr,
        span: Span,
    },
    MemCopy {
        dst: TExpr,
        src: TExpr,
        size: TExpr,
        span: Span,
    },
    Expr(TExpr, Span),
    Block(Vec<TStmt>, Span),
    If {
        cond: TExpr,
        then_body: Vec<TStmt>,
        else_body: Vec<TStmt>,
        span: Span,
    },
    Loop {
        body: Vec<TStmt>,
        span: Span,
    },
    Break(Span),
    Continue(Span),
    Return(Option<TExpr>, Span),
    Trap(Span),
}

/// A fully checked function.
#[derive(Debug, Clone)]
pub(crate) struct CheckedFn {
    pub module: usize,
    pub index: usize,
    pub name: String,
    pub type_params: Vec<String>,
    pub params: Vec<(String, ConcreteType)>,
    pub ret: ConcreteType,
    pub body: Vec<TStmt>,
    pub span: Span,
}

/// A fully checked global.
#[derive(Debug, Clone)]
pub(crate) struct CheckedGlobal {
    pub module: usize,
    pub index: usize,
    pub name: String,
    pub ty: ConcreteType,
    pub init: Option<Vec<u8>>,
    pub span: Span,
}

/// The complete typed program.
#[derive(Debug)]
pub(crate) struct CheckedProgram<'a> {
    pub resolved: ResolvedProgram<'a>,
    pub functions: Vec<CheckedFn>,
    pub globals: Vec<CheckedGlobal>,
    pub function_lookup: HashMap<(usize, usize), usize>,
    pub entry_function: usize,
}

impl CheckedProgram<'_> {
    pub(crate) fn checked_fn(&self, module: usize, index: usize) -> Option<&CheckedFn> {
        self.function_lookup
            .get(&(module, index))
            .and_then(|checked| self.functions.get(*checked))
    }
}

/// Checks every module and function, returning the typed program.
pub(crate) fn check_program(
    resolved: ResolvedProgram<'_>,
) -> Result<CheckedProgram<'_>, Vec<TypeError>> {
    let mut errors = Vec::new();

    validate_aggregate_declarations(&resolved, &mut errors);

    let mut globals = Vec::new();
    let mut global_types = HashMap::new();
    for module in &resolved.modules {
        let type_map = HashMap::new();
        for (index, global) in module.ast.globals.iter().enumerate() {
            match resolve_type_in(&resolved, module.index, &type_map, &global.ty, &global.span) {
                Ok(ty) => {
                    global_types.insert((module.index, index), ty.clone());
                    globals.push(CheckedGlobal {
                        module: module.index,
                        index,
                        name: global.name.clone(),
                        ty,
                        init: global.init.clone(),
                        span: global.span.clone(),
                    });
                }
                Err(error) => errors.push(error),
            }
        }
    }

    let phrase_entries = match validate_phrases(&resolved) {
        Ok(entries) => entries,
        Err(phrase_errors) => {
            errors.extend(phrase_errors);
            Vec::new()
        }
    };
    let _ = phrase_entries;

    let mut functions = Vec::new();
    let mut function_lookup = HashMap::new();
    for module in &resolved.modules {
        for (index, function) in module.ast.functions.iter().enumerate() {
            match check_function(&resolved, module, index, function, &global_types) {
                Ok(checked) => {
                    function_lookup.insert((module.index, index), functions.len());
                    functions.push(checked);
                }
                Err(error) => errors.push(error),
            }
        }
    }

    if let Some(entry_index) = function_lookup.get(&(resolved.entry_module, resolved.entry_fn)) {
        let entry = &functions[*entry_index];
        let valid_return = matches!(
            entry.ret,
            ConcreteType::Void | ConcreteType::I32 | ConcreteType::I64
        );
        let valid_params = entry.params.is_empty()
            || (entry.params.len() == 3
                && entry.params[0].1 == ConcreteType::I64
                && entry.params[1].1
                    == ConcreteType::Ptr(Box::new(ConcreteType::Ptr(Box::new(ConcreteType::U8))))
                && entry.params[2].1
                    == ConcreteType::Ptr(Box::new(ConcreteType::Ptr(Box::new(ConcreteType::U8)))));
        if !valid_return || !valid_params {
            errors.push(TypeError::new(
                entry.span.clone(),
                TypeErrorKind::InvalidEntrySignature,
                "entry must be main() or main(i64, ptr<ptr<u8>>, ptr<ptr<u8>>) returning void, i32, or i64"
                    .to_string(),
            ));
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let entry_function = *function_lookup
        .get(&(resolved.entry_module, resolved.entry_fn))
        .expect("resolved entry function must be checked");

    Ok(CheckedProgram {
        resolved,
        functions,
        globals,
        function_lookup,
        entry_function,
    })
}

fn validate_aggregate_declarations(resolved: &ResolvedProgram<'_>, errors: &mut Vec<TypeError>) {
    for module in &resolved.modules {
        for def in &module.ast.structs {
            let type_map = def
                .type_params
                .iter()
                .map(|param| (param.clone(), ConcreteType::Generic(param.clone())))
                .collect::<HashMap<_, _>>();
            for (_, field_ty) in &def.fields {
                if let Err(error) =
                    resolve_type_in(resolved, module.index, &type_map, field_ty, &def.span)
                {
                    errors.push(error);
                }
            }
        }
        for def in &module.ast.unions {
            let type_map = def
                .type_params
                .iter()
                .map(|param| (param.clone(), ConcreteType::Generic(param.clone())))
                .collect::<HashMap<_, _>>();
            for (_, field_ty) in &def.fields {
                if let Err(error) =
                    resolve_type_in(resolved, module.index, &type_map, field_ty, &def.span)
                {
                    errors.push(error);
                }
            }
        }
    }
}

pub(crate) fn validate_phrases(
    resolved: &ResolvedProgram<'_>,
) -> Result<Vec<PhraseEntry>, Vec<TypeError>> {
    let mut entries = Vec::new();
    let mut errors = Vec::new();
    for module in &resolved.modules {
        for function in &module.ast.functions {
            let params = function
                .params
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<HashSet<_>>();
            for pattern in &function.phrases {
                let mut bound = HashSet::new();
                for part in &pattern.parts {
                    if let PhrasePart::Param(name) = part {
                        if !params.contains(name.as_str()) {
                            errors.push(TypeError::new(
                                function.span.clone(),
                                TypeErrorKind::InvalidPhraseParam,
                                format!(
                                    "phrase parameter `{name}` does not name a parameter of `{}`",
                                    function.name
                                ),
                            ));
                        }
                        if !bound.insert(name.clone()) {
                            errors.push(TypeError::new(
                                function.span.clone(),
                                TypeErrorKind::DuplicatePhraseParam,
                                format!(
                                    "phrase for `{}` binds parameter `{name}` more than once",
                                    function.name
                                ),
                            ));
                        }
                    }
                }
                entries.push(PhraseEntry {
                    module: module.ast.name.clone(),
                    function: function.name.clone(),
                    pattern: pattern.clone(),
                });
            }
        }
    }
    if errors.is_empty() {
        Ok(entries)
    } else {
        Err(errors)
    }
}

fn check_function(
    resolved: &ResolvedProgram<'_>,
    module: &ResolvedModule<'_>,
    index: usize,
    function: &AstFn,
    _global_types: &HashMap<(usize, usize), ConcreteType>,
) -> Result<CheckedFn, TypeError> {
    let type_map = function
        .type_params
        .iter()
        .map(|param| (param.clone(), ConcreteType::Generic(param.clone())))
        .collect::<HashMap<_, _>>();

    let mut params = Vec::with_capacity(function.params.len());
    for (name, ty) in &function.params {
        let ty = resolve_type_in(resolved, module.index, &type_map, ty, &function.span)?;
        params.push((name.clone(), ty));
    }
    let ret = resolve_type_in(
        resolved,
        module.index,
        &type_map,
        &function.ret,
        &function.span,
    )?;

    let mut checker = Checker {
        resolved,
        module: module.index,
        type_params: type_map,
        scopes: vec![
            params
                .iter()
                .map(|(name, ty)| (name.clone(), ty.clone()))
                .collect(),
        ],
        loop_depth: 0,
        ret: ret.clone(),
        fresh_counter: 0,
    };
    let body = checker.check_stmts(&function.body, &function.span)?;

    Ok(CheckedFn {
        module: module.index,
        index,
        name: function.name.clone(),
        type_params: function.type_params.clone(),
        params,
        ret,
        body,
        span: function.span.clone(),
    })
}

type InstMatch = (
    Vec<ConcreteType>,
    Vec<ConcreteType>,
    ConcreteType,
    bool,
    bool,
);
type LiteralMatch = Option<(Vec<ConcreteType>, Vec<TExpr>, ConcreteType)>;

struct Checker<'a, 'p> {
    resolved: &'a ResolvedProgram<'p>,
    module: usize,
    type_params: HashMap<String, ConcreteType>,
    scopes: Vec<HashMap<String, ConcreteType>>,
    loop_depth: usize,
    ret: ConcreteType,
    fresh_counter: usize,
}

impl Checker<'_, '_> {
    fn lookup_var(&self, name: &str) -> Option<&ConcreteType> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }

    fn declare(&mut self, name: String, ty: ConcreteType) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, ty);
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn check_stmts(&mut self, stmts: &[AstStmt], span: &Span) -> Result<Vec<TStmt>, TypeError> {
        let mut result = Vec::with_capacity(stmts.len());
        for stmt in stmts {
            result.push(self.check_stmt(stmt, span)?);
        }
        Ok(result)
    }

    fn check_scoped_stmts(
        &mut self,
        stmts: &[AstStmt],
        span: &Span,
    ) -> Result<Vec<TStmt>, TypeError> {
        self.push_scope();
        let result = self.check_stmts(stmts, span);
        self.pop_scope();
        result
    }

    fn check_stmt(&mut self, stmt: &AstStmt, _fallback_span: &Span) -> Result<TStmt, TypeError> {
        match stmt {
            AstStmt::Let {
                name,
                ty,
                init,
                span,
            } => {
                let declared = ty
                    .as_ref()
                    .map(|ty| {
                        resolve_type_in(self.resolved, self.module, &self.type_params, ty, span)
                    })
                    .transpose()?;
                let init = self.check_expr(init, declared.as_ref(), span)?;
                let ty = declared.unwrap_or_else(|| init.ty.clone());
                if init.ty != ty {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::TypeMismatch,
                        format!(
                            "initializer for `{name}` has type {:?}, expected {:?}",
                            init.ty, ty
                        ),
                    ));
                }
                self.declare(name.clone(), ty.clone());
                Ok(TStmt::Let {
                    name: name.clone(),
                    ty,
                    init,
                    span: span.clone(),
                })
            }
            AstStmt::Assign { name, val, span } => {
                let expected = self.lookup_var(name).cloned().ok_or_else(|| {
                    TypeError::new(
                        span.clone(),
                        TypeErrorKind::UndefinedVariable,
                        format!("variable `{name}` is not in scope"),
                    )
                })?;
                let val = self.check_expr(val, Some(&expected), span)?;
                Ok(TStmt::Assign {
                    name: name.clone(),
                    val,
                    span: span.clone(),
                })
            }
            AstStmt::Store { ptr, val, span } => {
                let ptr = self.infer_expr(ptr, span)?;
                let Some(pointee) = ptr.ty.pointee() else {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::TypeMismatch,
                        "store destination is not a pointer".to_string(),
                    ));
                };
                let val = self.check_expr(val, Some(pointee), span)?;
                Ok(TStmt::Store {
                    ptr,
                    val,
                    span: span.clone(),
                })
            }
            AstStmt::MemCopy {
                dst,
                src,
                size,
                span,
            } => {
                let dst = self.infer_expr(dst, span)?;
                let src = self.infer_expr(src, span)?;
                if !matches!(dst.ty, ConcreteType::Ptr(_))
                    || !matches!(src.ty, ConcreteType::Ptr(_))
                {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::TypeMismatch,
                        "memcopy operands must be pointers".to_string(),
                    ));
                }
                let size = self.infer_expr(size, span)?;
                if !size.ty.is_integer() {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::TypeMismatch,
                        "memcopy size must be an integer".to_string(),
                    ));
                }
                Ok(TStmt::MemCopy {
                    dst,
                    src,
                    size,
                    span: span.clone(),
                })
            }
            AstStmt::Expr(expr, span) => {
                Ok(TStmt::Expr(self.infer_expr(expr, span)?, span.clone()))
            }
            AstStmt::Block(body, span) => Ok(TStmt::Block(
                self.check_scoped_stmts(body, span)?,
                span.clone(),
            )),
            AstStmt::If {
                cond,
                then_body,
                else_body,
                span,
            } => {
                let cond = self.check_expr(cond, Some(&ConcreteType::Bool), span)?;
                let then_body = self.check_scoped_stmts(then_body, span)?;
                let else_body = self.check_scoped_stmts(else_body, span)?;
                Ok(TStmt::If {
                    cond,
                    then_body,
                    else_body,
                    span: span.clone(),
                })
            }
            AstStmt::Loop { body, span } => {
                self.loop_depth += 1;
                self.push_scope();
                let body_result = self.check_stmts(body, span);
                self.pop_scope();
                self.loop_depth -= 1;
                Ok(TStmt::Loop {
                    body: body_result?,
                    span: span.clone(),
                })
            }
            AstStmt::Break(span) => {
                if self.loop_depth == 0 {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::InvalidBreak,
                        "break outside of a loop".to_string(),
                    ));
                }
                Ok(TStmt::Break(span.clone()))
            }
            AstStmt::Continue(span) => {
                if self.loop_depth == 0 {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::InvalidContinue,
                        "continue outside of a loop".to_string(),
                    ));
                }
                Ok(TStmt::Continue(span.clone()))
            }
            AstStmt::Return(value, span) => {
                if self.ret == ConcreteType::Void {
                    if value.is_some() {
                        return Err(TypeError::new(
                            span.clone(),
                            TypeErrorKind::InvalidReturn,
                            "void function cannot return a value".to_string(),
                        ));
                    }
                    Ok(TStmt::Return(None, span.clone()))
                } else {
                    let ret = self.ret.clone();
                    let Some(value) = value else {
                        return Err(TypeError::new(
                            span.clone(),
                            TypeErrorKind::InvalidReturn,
                            format!("function must return {ret:?}"),
                        ));
                    };
                    let value = self.check_expr(value, Some(&ret), span)?;
                    Ok(TStmt::Return(Some(value), span.clone()))
                }
            }
            AstStmt::Trap(span) => Ok(TStmt::Trap(span.clone())),
        }
    }

    fn infer_expr(&mut self, expr: &AstExpr, span: &Span) -> Result<TExpr, TypeError> {
        self.check_expr(expr, None, span)
    }

    fn check_expr(
        &mut self,
        expr: &AstExpr,
        expected: Option<&ConcreteType>,
        span: &Span,
    ) -> Result<TExpr, TypeError> {
        let typed = match expr {
            AstExpr::ConstInt(value, annotation) => {
                let ty = self.literal_int_type(*value, annotation.as_ref(), expected, span)?;
                let kind = if ty.is_unsigned_int() {
                    if *value < 0 {
                        return Err(TypeError::new(
                            span.clone(),
                            TypeErrorKind::TypeMismatch,
                            "negative literal cannot be used with an unsigned type".to_string(),
                        ));
                    }
                    TExprKind::ConstUint(*value as u64)
                } else {
                    TExprKind::ConstInt(*value)
                };
                TExpr { ty, kind }
            }
            AstExpr::ConstUint(value, annotation) => {
                let ty = self.literal_uint_type(*value, annotation.as_ref(), expected, span)?;
                let kind = if ty.is_signed_int() {
                    if *value > i64::MAX as u64 {
                        return Err(TypeError::new(
                            span.clone(),
                            TypeErrorKind::TypeMismatch,
                            "unsigned literal does not fit in a signed type".to_string(),
                        ));
                    }
                    TExprKind::ConstInt(*value as i64)
                } else {
                    TExprKind::ConstUint(*value)
                };
                TExpr { ty, kind }
            }
            AstExpr::ConstFloat(value, annotation) => {
                let ty = self.literal_float_type(annotation.as_ref(), expected, span)?;
                TExpr {
                    ty,
                    kind: TExprKind::ConstFloat(*value),
                }
            }
            AstExpr::ConstBool(value) => TExpr {
                ty: ConcreteType::Bool,
                kind: TExprKind::ConstBool(*value),
            },
            AstExpr::ConstNull(annotation) => {
                let ty = resolve_type_in(
                    self.resolved,
                    self.module,
                    &self.type_params,
                    annotation,
                    span,
                )?;
                if !matches!(ty, ConcreteType::Ptr(_)) {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::TypeMismatch,
                        "ConstNull requires a pointer type".to_string(),
                    ));
                }
                TExpr {
                    ty,
                    kind: TExprKind::ConstNull,
                }
            }
            AstExpr::BytesLit(bytes) => TExpr {
                ty: ConcreteType::Ptr(Box::new(ConcreteType::U8)),
                kind: TExprKind::BytesLit(bytes.clone()),
            },
            AstExpr::BytesLen(bytes) => TExpr {
                ty: ConcreteType::U64,
                kind: TExprKind::BytesLen(bytes.clone()),
            },
            AstExpr::Var(name) => {
                let ty = self.lookup_var(name).cloned().ok_or_else(|| {
                    TypeError::new(
                        span.clone(),
                        TypeErrorKind::UndefinedVariable,
                        format!("variable `{name}` is not in scope"),
                    )
                })?;
                TExpr {
                    ty,
                    kind: TExprKind::Var(name.clone()),
                }
            }
            AstExpr::GlobalAddr { module, name } => {
                let item =
                    self.resolved
                        .lookup_global(self.module, module.as_deref(), name, span)?;
                let global = &self.resolved.modules[item.module].ast.globals[item.index];
                let ty = resolve_type_in(
                    self.resolved,
                    item.module,
                    &HashMap::new(),
                    &global.ty,
                    &global.span,
                )?;
                TExpr {
                    ty: ConcreteType::Ptr(Box::new(ty)),
                    kind: TExprKind::GlobalAddr {
                        module: item.module,
                        index: item.index,
                    },
                }
            }
            AstExpr::AddrOfVar(name) => {
                let ty = self.lookup_var(name).cloned().ok_or_else(|| {
                    TypeError::new(
                        span.clone(),
                        TypeErrorKind::UndefinedVariable,
                        format!("variable `{name}` is not in scope"),
                    )
                })?;
                TExpr {
                    ty: ConcreteType::Ptr(Box::new(ty)),
                    kind: TExprKind::AddrOfVar(name.clone()),
                }
            }
            AstExpr::FieldAddr { base_ptr, field } => {
                let base = self.infer_expr(base_ptr, span)?;
                let Some(pointee) = base.ty.pointee() else {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::TypeMismatch,
                        "field access requires a pointer to a struct or union".to_string(),
                    ));
                };
                let (field_idx, field_ty) =
                    resolve_field_type(self.resolved, pointee, field, span)?;
                TExpr {
                    ty: ConcreteType::Ptr(Box::new(field_ty)),
                    kind: TExprKind::FieldAddr {
                        base: Box::new(base),
                        field_idx,
                    },
                }
            }
            AstExpr::IndexAddr { base_ptr, index } => {
                let base = self.infer_expr(base_ptr, span)?;
                let Some(pointee) = base.ty.pointee() else {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::TypeMismatch,
                        "indexing requires a pointer".to_string(),
                    ));
                };
                let elem = match pointee {
                    ConcreteType::Array(elem, _) => (**elem).clone(),
                    other => other.clone(),
                };
                let index = self.infer_expr(index, span)?;
                if !index.ty.is_integer() {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::TypeMismatch,
                        "array index must be an integer".to_string(),
                    ));
                }
                TExpr {
                    ty: ConcreteType::Ptr(Box::new(elem)),
                    kind: TExprKind::IndexAddr {
                        base: Box::new(base),
                        index: Box::new(index),
                    },
                }
            }
            AstExpr::Load(ptr) => {
                let ptr = self.infer_expr(ptr, span)?;
                let Some(pointee) = ptr.ty.pointee() else {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::TypeMismatch,
                        "load requires a pointer".to_string(),
                    ));
                };
                if *pointee == ConcreteType::Void {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::TypeMismatch,
                        "cannot load void".to_string(),
                    ));
                }
                TExpr {
                    ty: pointee.clone(),
                    kind: TExprKind::Load(Box::new(ptr)),
                }
            }
            AstExpr::SizeOf(ty) => TExpr {
                ty: ConcreteType::U64,
                kind: TExprKind::SizeOf(resolve_type_in(
                    self.resolved,
                    self.module,
                    &self.type_params,
                    ty,
                    span,
                )?),
            },
            AstExpr::AlignOf(ty) => TExpr {
                ty: ConcreteType::U64,
                kind: TExprKind::AlignOf(resolve_type_in(
                    self.resolved,
                    self.module,
                    &self.type_params,
                    ty,
                    span,
                )?),
            },
            AstExpr::Unary { op, val } => self.check_unary(*op, val, expected, span)?,
            AstExpr::Binary { op, lhs, rhs } => self.check_binary(*op, lhs, rhs, expected, span)?,
            AstExpr::Cast { val, to } => {
                let target =
                    resolve_type_in(self.resolved, self.module, &self.type_params, to, span)?;
                let val = self.infer_expr(val, span)?;
                if !cast_allowed(&val.ty, &target) {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::TypeMismatch,
                        format!("cannot cast {:?} to {:?}", val.ty, target),
                    ));
                }
                TExpr {
                    ty: target,
                    kind: TExprKind::Cast { val: Box::new(val) },
                }
            }
            AstExpr::FnAddr {
                module,
                name,
                type_args,
            } => {
                let (item, args, signature) =
                    self.resolve_fn_addr(module.as_deref(), name, type_args, expected, span)?;
                TExpr {
                    ty: signature,
                    kind: TExprKind::FnAddr {
                        module: item.module,
                        index: item.index,
                        type_args: args,
                    },
                }
            }
            AstExpr::Call {
                module,
                name,
                type_args,
                args,
            } => {
                let mut typed_args = Vec::with_capacity(args.len());
                for arg in args {
                    typed_args.push(self.infer_expr(arg, span)?);
                }
                let (item, resolved_args, coerced_args, ret) = self.resolve_call(
                    module.as_deref(),
                    name,
                    type_args,
                    args,
                    &typed_args,
                    expected,
                    span,
                )?;
                TExpr {
                    ty: ret,
                    kind: TExprKind::Call {
                        module: item.module,
                        index: item.index,
                        type_args: resolved_args,
                        args: coerced_args,
                    },
                }
            }
            AstExpr::IndirectCall { callee, args } => {
                let callee = self.infer_expr(callee, span)?;
                let ConcreteType::FnPtr { params, ret } = &callee.ty else {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::TypeMismatch,
                        "indirect call target is not a function pointer".to_string(),
                    ));
                };
                if params.len() != args.len() {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::NoMatchingOverload,
                        format!(
                            "indirect call expected {} arguments, found {}",
                            params.len(),
                            args.len()
                        ),
                    ));
                }
                let mut typed_args = Vec::with_capacity(args.len());
                for (arg, param) in args.iter().zip(params) {
                    typed_args.push(self.check_expr(arg, Some(param), span)?);
                }
                TExpr {
                    ty: (**ret).clone(),
                    kind: TExprKind::IndirectCall {
                        callee: Box::new(callee),
                        args: typed_args,
                    },
                }
            }
            AstExpr::Syscall { num, args } => {
                let num = self.infer_expr(num, span)?;
                if !is_syscall_word(&num.ty) {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::TypeMismatch,
                        "syscall number must be i64, u64, or a pointer".to_string(),
                    ));
                }
                if args.len() > 6 {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::InvalidOperation,
                        "syscall supports at most six arguments".to_string(),
                    ));
                }
                let mut typed_args = Vec::with_capacity(args.len());
                for arg in args {
                    let typed = self.infer_expr(arg, span)?;
                    if !is_syscall_word(&typed.ty) {
                        return Err(TypeError::new(
                            span.clone(),
                            TypeErrorKind::TypeMismatch,
                            "syscall arguments must be i64, u64, or pointers".to_string(),
                        ));
                    }
                    typed_args.push(typed);
                }
                TExpr {
                    ty: ConcreteType::I64,
                    kind: TExprKind::Syscall {
                        num: Box::new(num),
                        args: typed_args,
                    },
                }
            }
        };

        if let Some(expected) = expected {
            if typed.ty != *expected {
                return Err(TypeError::new(
                    span.clone(),
                    TypeErrorKind::TypeMismatch,
                    format!("expected {:?}, found {:?}", expected, typed.ty),
                ));
            }
        }
        Ok(typed)
    }

    fn literal_int_type(
        &self,
        value: i64,
        annotation: Option<&AstType>,
        expected: Option<&ConcreteType>,
        span: &Span,
    ) -> Result<ConcreteType, TypeError> {
        if let Some(annotation) = annotation {
            let ty = resolve_type_in(
                self.resolved,
                self.module,
                &self.type_params,
                annotation,
                span,
            )?;
            if !ty.is_signed_int() {
                return Err(TypeError::new(
                    span.clone(),
                    TypeErrorKind::TypeMismatch,
                    "signed integer literal requires a signed integer type".to_string(),
                ));
            }
            return Ok(ty);
        }
        if let Some(expected) = expected {
            if expected.is_integer() {
                if expected.is_unsigned_int() && value < 0 {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::TypeMismatch,
                        "negative literal cannot use an unsigned type".to_string(),
                    ));
                }
                return Ok(expected.clone());
            }
        }
        Ok(ConcreteType::I64)
    }

    fn literal_uint_type(
        &self,
        value: u64,
        annotation: Option<&AstType>,
        expected: Option<&ConcreteType>,
        span: &Span,
    ) -> Result<ConcreteType, TypeError> {
        if let Some(annotation) = annotation {
            let ty = resolve_type_in(
                self.resolved,
                self.module,
                &self.type_params,
                annotation,
                span,
            )?;
            if !ty.is_unsigned_int() {
                return Err(TypeError::new(
                    span.clone(),
                    TypeErrorKind::TypeMismatch,
                    "unsigned literal requires an unsigned integer type".to_string(),
                ));
            }
            return Ok(ty);
        }
        if let Some(expected) = expected {
            if expected.is_signed_int() && value > i64::MAX as u64 {
                return Err(TypeError::new(
                    span.clone(),
                    TypeErrorKind::TypeMismatch,
                    "unsigned literal does not fit in a signed type".to_string(),
                ));
            }
            if expected.is_integer() {
                return Ok(expected.clone());
            }
        }
        Ok(ConcreteType::U64)
    }

    fn literal_float_type(
        &self,
        annotation: Option<&AstType>,
        expected: Option<&ConcreteType>,
        span: &Span,
    ) -> Result<ConcreteType, TypeError> {
        if let Some(annotation) = annotation {
            let ty = resolve_type_in(
                self.resolved,
                self.module,
                &self.type_params,
                annotation,
                span,
            )?;
            if !ty.is_float() {
                return Err(TypeError::new(
                    span.clone(),
                    TypeErrorKind::TypeMismatch,
                    "float literal requires f32 or f64".to_string(),
                ));
            }
            return Ok(ty);
        }
        if let Some(expected) = expected {
            if expected.is_float() {
                return Ok(expected.clone());
            }
        }
        Ok(ConcreteType::F64)
    }

    fn check_unary(
        &mut self,
        op: UnOp,
        val: &AstExpr,
        expected: Option<&ConcreteType>,
        span: &Span,
    ) -> Result<TExpr, TypeError> {
        let operand_ty = match op {
            UnOp::LogNot => ConcreteType::Bool,
            UnOp::Neg | UnOp::BitNot => {
                if let Some(expected) = expected {
                    if expected.is_integer() || expected.is_float() {
                        expected.clone()
                    } else {
                        self.infer_expr(val, span)?.ty
                    }
                } else {
                    self.infer_expr(val, span)?.ty
                }
            }
        };
        let val = self.check_expr(val, Some(&operand_ty), span)?;
        match op {
            UnOp::Neg => {
                if !(operand_ty.is_integer() || operand_ty.is_float()) {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::TypeMismatch,
                        "negation requires an integer or float".to_string(),
                    ));
                }
            }
            UnOp::BitNot => {
                if !operand_ty.is_integer() {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::TypeMismatch,
                        "bitwise complement requires an integer".to_string(),
                    ));
                }
            }
            UnOp::LogNot => {}
        }
        Ok(TExpr {
            ty: operand_ty,
            kind: TExprKind::Unary {
                op,
                val: Box::new(val),
            },
        })
    }

    fn check_binary(
        &mut self,
        op: BinOp,
        lhs: &AstExpr,
        rhs: &AstExpr,
        expected: Option<&ConcreteType>,
        span: &Span,
    ) -> Result<TExpr, TypeError> {
        if matches!(op, BinOp::LogAnd | BinOp::LogOr) {
            let lhs = self.check_expr(lhs, Some(&ConcreteType::Bool), span)?;
            let rhs = self.check_expr(rhs, Some(&ConcreteType::Bool), span)?;
            return Ok(TExpr {
                ty: ConcreteType::Bool,
                kind: TExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            });
        }

        let needs_value_type = matches!(
            op,
            BinOp::Add
                | BinOp::Sub
                | BinOp::Mul
                | BinOp::Div
                | BinOp::Mod
                | BinOp::BitAnd
                | BinOp::BitOr
                | BinOp::BitXor
                | BinOp::Shl
                | BinOp::Shr
        );
        let operand_ty = if needs_value_type {
            if let Some(expected) = expected {
                if expected.is_integer() || expected.is_float() {
                    expected.clone()
                } else {
                    self.operand_type(lhs, rhs, span)?
                }
            } else {
                self.operand_type(lhs, rhs, span)?
            }
        } else {
            self.operand_type(lhs, rhs, span)?
        };

        let lhs = self.check_expr(lhs, Some(&operand_ty), span)?;
        let rhs = self.check_expr(rhs, Some(&operand_ty), span)?;
        let result_ty = match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                if !(operand_ty.is_integer() || operand_ty.is_float()) {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::TypeMismatch,
                        "arithmetic requires integers or floats".to_string(),
                    ));
                }
                operand_ty.clone()
            }
            BinOp::Mod => {
                if !operand_ty.is_integer() {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::TypeMismatch,
                        "remainder requires integers".to_string(),
                    ));
                }
                operand_ty.clone()
            }
            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr => {
                if !operand_ty.is_integer() {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::TypeMismatch,
                        "bitwise and shift operations require integers".to_string(),
                    ));
                }
                operand_ty.clone()
            }
            BinOp::Eq | BinOp::Ne => {
                if !operand_ty.is_scalar() {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::TypeMismatch,
                        "equality requires scalar operands".to_string(),
                    ));
                }
                ConcreteType::Bool
            }
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                if !(operand_ty.is_integer() || operand_ty.is_float()) {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::TypeMismatch,
                        "ordered comparison requires integers or floats".to_string(),
                    ));
                }
                ConcreteType::Bool
            }
            BinOp::LogAnd | BinOp::LogOr => unreachable!(),
        };
        Ok(TExpr {
            ty: result_ty,
            kind: TExprKind::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
        })
    }

    fn operand_type(
        &mut self,
        lhs: &AstExpr,
        rhs: &AstExpr,
        span: &Span,
    ) -> Result<ConcreteType, TypeError> {
        if is_untyped_literal(lhs) && !is_untyped_literal(rhs) {
            return Ok(self.infer_expr(rhs, span)?.ty);
        }
        Ok(self.infer_expr(lhs, span)?.ty)
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_call(
        &mut self,
        module: Option<&str>,
        name: &str,
        explicit_args: &[AstType],
        args_ast: &[AstExpr],
        args: &[TExpr],
        _expected: Option<&ConcreteType>,
        span: &Span,
    ) -> Result<(FnItem, Vec<ConcreteType>, Vec<TExpr>, ConcreteType), TypeError> {
        let candidates = self
            .resolved
            .function_candidates(self.module, module, name, span)?;
        let mut viable = Vec::new();
        let mut unconstrained = false;
        let mut explicit_arity_match = explicit_args.is_empty();

        for candidate in candidates.iter().copied() {
            let function = &self.resolved.modules[candidate.module].ast.functions[candidate.index];
            if !explicit_args.is_empty() && explicit_args.len() == function.type_params.len() {
                explicit_arity_match = true;
            }
            let (type_args, _params, ret, matched, unbound) =
                self.instantiate_candidate(candidate, explicit_args, args, span)?;
            if matched {
                viable.push((candidate, type_args, ret, args.to_vec()));
            }
            if unbound {
                unconstrained = true;
            }
        }

        if viable.is_empty() {
            let mut literal_viable = Vec::new();
            for candidate in candidates.iter().copied() {
                if let Some((type_args, coerced_args, ret)) = self
                    .instantiate_candidate_with_literals(
                        candidate,
                        explicit_args,
                        args_ast,
                        args,
                        span,
                    )?
                {
                    literal_viable.push((candidate, type_args, ret, coerced_args));
                }
            }
            if !literal_viable.is_empty() {
                viable = literal_viable;
            }
        }

        match viable.len() {
            1 => {
                let (candidate, type_args, ret, args) = viable.pop().expect("one viable candidate");
                Ok((candidate, type_args, args, ret))
            }
            0 if unconstrained => Err(TypeError::new(
                span.clone(),
                TypeErrorKind::UnconstrainedGeneric,
                format!("cannot infer all generic type parameters of `{name}`"),
            )),
            0 if !explicit_args.is_empty() && !explicit_arity_match => Err(TypeError::new(
                span.clone(),
                TypeErrorKind::WrongGenericArity,
                format!(
                    "no overload of `{name}` accepts {} generic arguments",
                    explicit_args.len()
                ),
            )),
            0 => Err(TypeError::new(
                span.clone(),
                TypeErrorKind::NoMatchingOverload,
                format!(
                    "no overload of `{name}` matches arguments ({})",
                    args.iter()
                        .map(|arg| format!("{:?}", arg.ty))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )),
            _ => Err(TypeError::new(
                span.clone(),
                TypeErrorKind::AmbiguousCall,
                format!("call to `{name}` matches {} overloads", viable.len()),
            )),
        }
    }

    fn instantiate_candidate_with_literals(
        &mut self,
        candidate: FnItem,
        explicit_args: &[AstType],
        args_ast: &[AstExpr],
        args: &[TExpr],
        span: &Span,
    ) -> Result<LiteralMatch, TypeError> {
        let function =
            self.resolved.modules[candidate.module].ast.functions[candidate.index].clone();
        if args_ast.len() != args.len() {
            return Ok(None);
        }
        let (type_args, type_map) = if !explicit_args.is_empty() {
            if explicit_args.len() != function.type_params.len() {
                return Ok(None);
            }
            let mut type_args = Vec::with_capacity(explicit_args.len());
            for arg in explicit_args {
                type_args.push(resolve_type_in(
                    self.resolved,
                    self.module,
                    &self.type_params,
                    arg,
                    span,
                )?);
            }
            let type_map = function
                .type_params
                .iter()
                .zip(&type_args)
                .map(|(param, arg)| (param.clone(), arg.clone()))
                .collect::<HashMap<_, _>>();
            (type_args, type_map)
        } else if function.type_params.is_empty() {
            (Vec::new(), HashMap::new())
        } else {
            let mut fresh = Vec::with_capacity(function.type_params.len());
            let mut fresh_map = HashMap::new();
            for (index, param) in function.type_params.iter().enumerate() {
                let name = format!("$lit{}_{index}", self.fresh_counter);
                self.fresh_counter += 1;
                fresh_map.insert(param.clone(), ConcreteType::Generic(name.clone()));
                fresh.push(name);
            }
            let (params, _ret) = self.candidate_signature(candidate, &fresh_map, span)?;
            if params.len() != args.len() {
                return Ok(None);
            }
            let mut bindings = HashMap::new();
            for ((param, ast), arg) in params.iter().zip(args_ast).zip(args) {
                if is_untyped_literal(ast) {
                    continue;
                }
                if !unify(param, &arg.ty, &mut bindings) {
                    return Ok(None);
                }
            }
            if fresh.iter().any(|name| !bindings.contains_key(name)) {
                return Ok(None);
            }
            let type_args = fresh
                .iter()
                .map(|name| bindings[name].clone())
                .collect::<Vec<_>>();
            let type_map = function
                .type_params
                .iter()
                .zip(&type_args)
                .map(|(param, arg)| (param.clone(), arg.clone()))
                .collect::<HashMap<_, _>>();
            (type_args, type_map)
        };

        let (params, ret) = self.candidate_signature(candidate, &type_map, span)?;
        if params.len() != args.len() {
            return Ok(None);
        }
        let mut coerced = Vec::with_capacity(args.len());
        for ((param, ast), typed) in params.iter().zip(args_ast).zip(args) {
            if is_untyped_literal(ast) && !param.contains_generic() {
                match self.check_expr(ast, Some(param), span) {
                    Ok(value) if value.ty == *param => coerced.push(value),
                    _ => return Ok(None),
                }
            } else if typed.ty == *param {
                coerced.push(typed.clone());
            } else {
                return Ok(None);
            }
        }
        Ok(Some((type_args, coerced, ret)))
    }

    fn resolve_fn_addr(
        &mut self,
        module: Option<&str>,
        name: &str,
        explicit_args: &[AstType],
        expected: Option<&ConcreteType>,
        span: &Span,
    ) -> Result<(FnItem, Vec<ConcreteType>, ConcreteType), TypeError> {
        let candidates = self
            .resolved
            .function_candidates(self.module, module, name, span)?;
        let mut viable = Vec::new();
        let mut unconstrained = false;
        let mut explicit_arity_match = explicit_args.is_empty();
        for candidate in candidates {
            let function = &self.resolved.modules[candidate.module].ast.functions[candidate.index];
            if !explicit_args.is_empty() && explicit_args.len() == function.type_params.len() {
                explicit_arity_match = true;
            }
            if explicit_args.is_empty() && !function.type_params.is_empty() {
                if let Some(expected @ ConcreteType::FnPtr { .. }) = expected {
                    let mut type_map = HashMap::new();
                    let mut fresh = Vec::new();
                    for (index, param) in function.type_params.iter().enumerate() {
                        let name = format!("$fnaddr{}_{index}", self.fresh_counter);
                        self.fresh_counter += 1;
                        type_map.insert(param.clone(), ConcreteType::Generic(name.clone()));
                        fresh.push(name);
                    }
                    let (params, ret) = self.candidate_signature(candidate, &type_map, span)?;
                    let signature = ConcreteType::FnPtr {
                        params,
                        ret: Box::new(ret),
                    };
                    let mut bindings = HashMap::new();
                    if unify(&signature, expected, &mut bindings)
                        && fresh.iter().all(|name| bindings.contains_key(name))
                    {
                        let type_args = fresh
                            .iter()
                            .map(|name| bindings[name].clone())
                            .collect::<Vec<_>>();
                        let mut substitution = HashMap::new();
                        for (param, arg) in function.type_params.iter().zip(&type_args) {
                            substitution.insert(param.clone(), arg.clone());
                        }
                        let (params, ret) =
                            self.candidate_signature(candidate, &substitution, span)?;
                        let signature = ConcreteType::FnPtr {
                            params,
                            ret: Box::new(ret),
                        };
                        if signature == *expected {
                            viable.push((candidate, type_args, signature));
                        }
                    } else {
                        unconstrained = true;
                    }
                    continue;
                }
                unconstrained = true;
                continue;
            }
            let (type_args, _params, _ret, matched, unbound) =
                self.instantiate_fn_addr(candidate, explicit_args, span)?;
            if matched {
                let function =
                    &self.resolved.modules[candidate.module].ast.functions[candidate.index];
                let type_map = function
                    .type_params
                    .iter()
                    .zip(&type_args)
                    .map(|(param, arg)| (param.clone(), arg.clone()))
                    .collect::<HashMap<_, _>>();
                let (params, ret) = self.candidate_signature(candidate, &type_map, span)?;
                let signature = ConcreteType::FnPtr {
                    params,
                    ret: Box::new(ret),
                };
                if let Some(expected) = expected {
                    if signature != *expected {
                        continue;
                    }
                }
                viable.push((candidate, type_args, signature));
            }
            if unbound {
                unconstrained = true;
            }
        }

        match viable.len() {
            1 => {
                let (candidate, type_args, signature) = viable.pop().expect("one viable candidate");
                Ok((candidate, type_args, signature))
            }
            0 if unconstrained => Err(TypeError::new(
                span.clone(),
                TypeErrorKind::UnconstrainedGeneric,
                format!("cannot infer generic type parameters of `{name}`"),
            )),
            0 if !explicit_args.is_empty() && !explicit_arity_match => Err(TypeError::new(
                span.clone(),
                TypeErrorKind::WrongGenericArity,
                format!(
                    "no overload of `{name}` accepts {} generic arguments",
                    explicit_args.len()
                ),
            )),
            0 => Err(TypeError::new(
                span.clone(),
                TypeErrorKind::NoMatchingOverload,
                format!("no function address `{name}` matches the context"),
            )),
            _ => Err(TypeError::new(
                span.clone(),
                TypeErrorKind::AmbiguousCall,
                format!("function address `{name}` is ambiguous"),
            )),
        }
    }

    fn instantiate_fn_addr(
        &mut self,
        candidate: FnItem,
        explicit_args: &[AstType],
        span: &Span,
    ) -> Result<InstMatch, TypeError> {
        let function = &self.resolved.modules[candidate.module].ast.functions[candidate.index];
        if !explicit_args.is_empty() {
            if explicit_args.len() != function.type_params.len() {
                return Ok((Vec::new(), Vec::new(), ConcreteType::Void, false, false));
            }
            let mut args = Vec::with_capacity(explicit_args.len());
            for arg in explicit_args {
                args.push(resolve_type_in(
                    self.resolved,
                    self.module,
                    &self.type_params,
                    arg,
                    span,
                )?);
            }
            let type_map = function
                .type_params
                .iter()
                .zip(&args)
                .map(|(param, arg)| (param.clone(), arg.clone()))
                .collect::<HashMap<_, _>>();
            let (params, ret) = self.candidate_signature(candidate, &type_map, span)?;
            return Ok((args, params, ret, true, false));
        }
        if !function.type_params.is_empty() {
            return Ok((Vec::new(), Vec::new(), ConcreteType::Void, false, true));
        }
        let (params, ret) = self.candidate_signature(candidate, &HashMap::new(), span)?;
        Ok((Vec::new(), params, ret, true, false))
    }

    fn instantiate_candidate(
        &mut self,
        candidate: FnItem,
        explicit_args: &[AstType],
        args: &[TExpr],
        span: &Span,
    ) -> Result<InstMatch, TypeError> {
        let function = &self.resolved.modules[candidate.module].ast.functions[candidate.index];
        if !explicit_args.is_empty() {
            if explicit_args.len() != function.type_params.len() {
                return Ok((Vec::new(), Vec::new(), ConcreteType::Void, false, false));
            }
            let mut concrete_args = Vec::with_capacity(explicit_args.len());
            for arg in explicit_args {
                concrete_args.push(resolve_type_in(
                    self.resolved,
                    self.module,
                    &self.type_params,
                    arg,
                    span,
                )?);
            }
            let type_map = function
                .type_params
                .iter()
                .zip(&concrete_args)
                .map(|(param, arg)| (param.clone(), arg.clone()))
                .collect::<HashMap<_, _>>();
            let (params, ret) = self.candidate_signature(candidate, &type_map, span)?;
            if params.len() != args.len() {
                return Ok((concrete_args, params, ret, false, false));
            }
            let matched = params.iter().zip(args).all(|(param, arg)| *param == arg.ty);
            return Ok((concrete_args, params, ret, matched, false));
        }

        if function.type_params.is_empty() {
            let (params, ret) = self.candidate_signature(candidate, &HashMap::new(), span)?;
            if params.len() != args.len() {
                return Ok((Vec::new(), params, ret, false, false));
            }
            let matched = params.iter().zip(args).all(|(param, arg)| *param == arg.ty);
            return Ok((Vec::new(), params, ret, matched, false));
        }

        let mut fresh = Vec::with_capacity(function.type_params.len());
        let mut fresh_map = HashMap::new();
        for (index, param) in function.type_params.iter().enumerate() {
            let name = format!("$call{}_{index}", self.fresh_counter);
            self.fresh_counter += 1;
            fresh_map.insert(param.clone(), ConcreteType::Generic(name.clone()));
            fresh.push(name);
        }
        let (params, ret) = self.candidate_signature(candidate, &fresh_map, span)?;
        if params.len() != args.len() {
            return Ok((Vec::new(), params, ret, false, false));
        }
        let mut bindings = HashMap::new();
        for (param, arg) in params.iter().zip(args) {
            if !unify(param, &arg.ty, &mut bindings) {
                return Ok((Vec::new(), params, ret, false, false));
            }
        }
        if fresh.iter().any(|name| !bindings.contains_key(name)) {
            return Ok((Vec::new(), params, ret, false, true));
        }
        let type_args = fresh
            .iter()
            .map(|name| bindings[name].clone())
            .collect::<Vec<_>>();
        let substitution = function
            .type_params
            .iter()
            .zip(&type_args)
            .map(|(param, arg)| (param.clone(), arg.clone()))
            .collect::<HashMap<_, _>>();
        let (params, ret) = self.candidate_signature(candidate, &substitution, span)?;
        let matched = params.iter().zip(args).all(|(param, arg)| *param == arg.ty);
        Ok((type_args, params, ret, matched, false))
    }

    fn candidate_signature(
        &self,
        candidate: FnItem,
        type_map: &HashMap<String, ConcreteType>,
        span: &Span,
    ) -> Result<(Vec<ConcreteType>, ConcreteType), TypeError> {
        let function = &self.resolved.modules[candidate.module].ast.functions[candidate.index];
        let mut params = Vec::with_capacity(function.params.len());
        for (_, ty) in &function.params {
            params.push(resolve_type_in(
                self.resolved,
                candidate.module,
                type_map,
                ty,
                span,
            )?);
        }
        let ret = resolve_type_in(
            self.resolved,
            candidate.module,
            type_map,
            &function.ret,
            span,
        )?;
        Ok((params, ret))
    }
}

fn is_untyped_literal(expr: &AstExpr) -> bool {
    matches!(
        expr,
        AstExpr::ConstInt(_, None) | AstExpr::ConstUint(_, None) | AstExpr::ConstFloat(_, None)
    )
}

fn is_syscall_word(ty: &ConcreteType) -> bool {
    matches!(
        ty,
        ConcreteType::I64 | ConcreteType::U64 | ConcreteType::Ptr(_)
    )
}

fn cast_allowed(from: &ConcreteType, to: &ConcreteType) -> bool {
    if from == to {
        return true;
    }
    match (from, to) {
        (a, b) if a.is_integer() && b.is_integer() => true,
        (a, ConcreteType::F32 | ConcreteType::F64) if a.is_integer() => true,
        (ConcreteType::F32 | ConcreteType::F64, b) if b.is_integer() => true,
        (ConcreteType::F32, ConcreteType::F64) | (ConcreteType::F64, ConcreteType::F32) => true,
        (ConcreteType::Ptr(_), ConcreteType::U64)
        | (ConcreteType::U64, ConcreteType::Ptr(_))
        | (ConcreteType::Ptr(_), ConcreteType::Ptr(_))
        | (ConcreteType::FnPtr { .. }, ConcreteType::U64)
        | (ConcreteType::U64, ConcreteType::FnPtr { .. }) => true,
        _ => false,
    }
}

fn unify(
    expected: &ConcreteType,
    actual: &ConcreteType,
    bindings: &mut HashMap<String, ConcreteType>,
) -> bool {
    if let ConcreteType::Generic(name) = expected {
        if let Some(bound) = bindings.get(name) {
            return bound == actual;
        }
        bindings.insert(name.clone(), actual.clone());
        return true;
    }
    match (expected, actual) {
        (ConcreteType::Ptr(a), ConcreteType::Ptr(b)) => unify(a, b, bindings),
        (ConcreteType::Array(a, ac), ConcreteType::Array(b, bc)) => {
            ac == bc && unify(a, b, bindings)
        }
        (
            ConcreteType::Struct {
                module: am,
                name: an,
                args: aa,
            },
            ConcreteType::Struct {
                module: bm,
                name: bn,
                args: ba,
            },
        )
        | (
            ConcreteType::Union {
                module: am,
                name: an,
                args: aa,
            },
            ConcreteType::Union {
                module: bm,
                name: bn,
                args: ba,
            },
        ) => {
            am == bm
                && an == bn
                && aa.len() == ba.len()
                && aa.iter().zip(ba).all(|(a, b)| unify(a, b, bindings))
        }
        (
            ConcreteType::FnPtr {
                params: ap,
                ret: ar,
            },
            ConcreteType::FnPtr {
                params: bp,
                ret: br,
            },
        ) => {
            ap.len() == bp.len()
                && ap.iter().zip(bp).all(|(a, b)| unify(a, b, bindings))
                && unify(ar, br, bindings)
        }
        _ => expected == actual,
    }
}

/// Resolves an AST type in a module and type-parameter environment.
pub(crate) fn resolve_type_in(
    resolved: &ResolvedProgram<'_>,
    module: usize,
    type_params: &HashMap<String, ConcreteType>,
    ty: &AstType,
    span: &Span,
) -> Result<ConcreteType, TypeError> {
    match ty {
        AstType::Void => Ok(ConcreteType::Void),
        AstType::Bool => Ok(ConcreteType::Bool),
        AstType::I8 => Ok(ConcreteType::I8),
        AstType::I16 => Ok(ConcreteType::I16),
        AstType::I32 => Ok(ConcreteType::I32),
        AstType::I64 => Ok(ConcreteType::I64),
        AstType::U8 => Ok(ConcreteType::U8),
        AstType::U16 => Ok(ConcreteType::U16),
        AstType::U32 => Ok(ConcreteType::U32),
        AstType::U64 => Ok(ConcreteType::U64),
        AstType::F32 => Ok(ConcreteType::F32),
        AstType::F64 => Ok(ConcreteType::F64),
        AstType::Ptr(inner) => Ok(ConcreteType::Ptr(Box::new(resolve_type_in(
            resolved,
            module,
            type_params,
            inner,
            span,
        )?))),
        AstType::Array(inner, count) => Ok(ConcreteType::Array(
            Box::new(resolve_type_in(resolved, module, type_params, inner, span)?),
            *count,
        )),
        AstType::Named {
            module: module_ref,
            name,
            type_args,
        } => {
            if module_ref.is_none() {
                if let Some(param) = type_params.get(name) {
                    if !type_args.is_empty() {
                        return Err(TypeError::new(
                            span.clone(),
                            TypeErrorKind::WrongGenericArity,
                            format!("generic parameter `{name}` does not take type arguments"),
                        ));
                    }
                    return Ok(param.clone());
                }
            }
            let item = resolved.lookup_type(module, module_ref.as_deref(), name, span)?;
            let (defining_module, expected_args, item_name) = match item {
                TypeItem::Struct { module, index } => {
                    let def = resolved.struct_ast(module, index);
                    (module, def.type_params.len(), def.name.clone())
                }
                TypeItem::Union { module, index } => {
                    let def = resolved.union_ast(module, index);
                    (module, def.type_params.len(), def.name.clone())
                }
            };
            if type_args.len() != expected_args {
                return Err(TypeError::new(
                    span.clone(),
                    TypeErrorKind::WrongGenericArity,
                    format!(
                        "type `{item_name}` expects {expected_args} type arguments, found {}",
                        type_args.len()
                    ),
                ));
            }
            let mut args = Vec::with_capacity(type_args.len());
            for arg in type_args {
                args.push(resolve_type_in(resolved, module, type_params, arg, span)?);
            }
            let module_name = resolved.module_name(defining_module).to_string();
            Ok(match item {
                TypeItem::Struct { .. } => ConcreteType::Struct {
                    module: module_name,
                    name: item_name,
                    args,
                },
                TypeItem::Union { .. } => ConcreteType::Union {
                    module: module_name,
                    name: item_name,
                    args,
                },
            })
        }
        AstType::FnPtr { params, ret } => {
            let mut resolved_params = Vec::with_capacity(params.len());
            for param in params {
                resolved_params.push(resolve_type_in(resolved, module, type_params, param, span)?);
            }
            let resolved_ret = resolve_type_in(resolved, module, type_params, ret, span)?;
            Ok(ConcreteType::FnPtr {
                params: resolved_params,
                ret: Box::new(resolved_ret),
            })
        }
    }
}

fn resolve_field_type(
    resolved: &ResolvedProgram<'_>,
    aggregate: &ConcreteType,
    field: &str,
    span: &Span,
) -> Result<(usize, ConcreteType), TypeError> {
    match aggregate {
        ConcreteType::Struct { module, name, args } => {
            let module_index = *resolved.module_by_name.get(module).ok_or_else(|| {
                TypeError::new(
                    span.clone(),
                    TypeErrorKind::UndefinedType,
                    format!("module `{module}` not found"),
                )
            })?;
            let index = *resolved.modules[module_index]
                .structs
                .get(name)
                .ok_or_else(|| {
                    TypeError::new(
                        span.clone(),
                        TypeErrorKind::UndefinedType,
                        format!("struct `{name}` not found"),
                    )
                })?;
            let def = resolved.struct_ast(module_index, index);
            if def.type_params.len() != args.len() {
                return Err(TypeError::new(
                    span.clone(),
                    TypeErrorKind::WrongGenericArity,
                    format!("type `{name}` expects {} arguments", def.type_params.len()),
                ));
            }
            let (field_idx, (_, field_ty)) = def
                .fields
                .iter()
                .enumerate()
                .find(|(_, (field_name, _))| field_name == field)
                .ok_or_else(|| {
                    TypeError::new(
                        span.clone(),
                        TypeErrorKind::UndefinedField,
                        format!("struct `{name}` has no field `{field}`"),
                    )
                })?;
            let type_map = def
                .type_params
                .iter()
                .zip(args)
                .map(|(param, arg)| (param.clone(), arg.clone()))
                .collect::<HashMap<_, _>>();
            let resolved_ty = resolve_type_in(resolved, module_index, &type_map, field_ty, span)?;
            Ok((field_idx, resolved_ty))
        }
        ConcreteType::Union { module, name, args } => {
            let module_index = *resolved.module_by_name.get(module).ok_or_else(|| {
                TypeError::new(
                    span.clone(),
                    TypeErrorKind::UndefinedType,
                    format!("module `{module}` not found"),
                )
            })?;
            let index = *resolved.modules[module_index]
                .unions
                .get(name)
                .ok_or_else(|| {
                    TypeError::new(
                        span.clone(),
                        TypeErrorKind::UndefinedType,
                        format!("union `{name}` not found"),
                    )
                })?;
            let def = resolved.union_ast(module_index, index);
            if def.type_params.len() != args.len() {
                return Err(TypeError::new(
                    span.clone(),
                    TypeErrorKind::WrongGenericArity,
                    format!("type `{name}` expects {} arguments", def.type_params.len()),
                ));
            }
            let (field_idx, (_, field_ty)) = def
                .fields
                .iter()
                .enumerate()
                .find(|(_, (field_name, _))| field_name == field)
                .ok_or_else(|| {
                    TypeError::new(
                        span.clone(),
                        TypeErrorKind::UndefinedField,
                        format!("union `{name}` has no member `{field}`"),
                    )
                })?;
            let type_map = def
                .type_params
                .iter()
                .zip(args)
                .map(|(param, arg)| (param.clone(), arg.clone()))
                .collect::<HashMap<_, _>>();
            let resolved_ty = resolve_type_in(resolved, module_index, &type_map, field_ty, span)?;
            Ok((field_idx, resolved_ty))
        }
        _ => Err(TypeError::new(
            span.clone(),
            TypeErrorKind::TypeMismatch,
            "field access requires a struct or union pointer".to_string(),
        )),
    }
}
