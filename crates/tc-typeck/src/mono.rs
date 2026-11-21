//! Demand-driven compile-time monomorphization.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::ast::{Span, TypeError, TypeErrorKind};
use crate::check::resolve_type_in;
use crate::check::{CheckedProgram, TExpr, TExprKind, TStmt};
use crate::resolve::{ConcreteType, sanitize};

const MAX_GENERIC_DEPTH: usize = 64;
const MAX_INSTANTIATIONS: usize = 4096;

/// A concrete function specialization key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct FnKey {
    pub module: usize,
    pub index: usize,
    pub type_args: Vec<ConcreteType>,
}

/// A concrete struct specialization key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct StructKey {
    pub module: usize,
    pub index: usize,
    pub type_args: Vec<ConcreteType>,
}

/// A concrete union specialization key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct UnionKey {
    pub module: usize,
    pub index: usize,
    pub type_args: Vec<ConcreteType>,
}

/// A monomorphized function.
#[derive(Debug, Clone)]
pub(crate) struct MonoFn {
    pub key: FnKey,
    pub name: String,
    pub params: Vec<(String, ConcreteType)>,
    pub ret: ConcreteType,
    pub body: Vec<TStmt>,
    pub span: Span,
}

/// A monomorphized struct.
#[derive(Debug, Clone)]
pub(crate) struct MonoStruct {
    pub key: StructKey,
    pub name: String,
    pub source_name: String,
    pub fields: Vec<(String, ConcreteType)>,
}

/// A monomorphized union.
#[derive(Debug, Clone)]
pub(crate) struct MonoUnion {
    pub key: UnionKey,
    pub name: String,
    pub source_name: String,
    pub fields: Vec<(String, ConcreteType)>,
}

/// A monomorphized global.
#[derive(Debug, Clone)]
pub(crate) struct MonoGlobal {
    pub name: String,
    pub ty: ConcreteType,
    pub init: Option<Vec<u8>>,
    pub span: Span,
}

/// The complete monomorphized program.
#[derive(Debug)]
pub(crate) struct MonoProgram {
    pub entry: usize,
    pub functions: Vec<MonoFn>,
    pub structs: Vec<MonoStruct>,
    pub unions: Vec<MonoUnion>,
    pub globals: Vec<MonoGlobal>,
    pub module_by_name: HashMap<String, usize>,
    pub fn_ids: HashMap<FnKey, usize>,
    pub global_ids: HashMap<(usize, usize), usize>,
}

struct MonoCtx<'a, 'p> {
    checked: &'a CheckedProgram<'p>,
    fn_queue: VecDeque<FnKey>,
    fn_seen: HashSet<FnKey>,
    functions: Vec<MonoFn>,
    struct_seen: HashSet<StructKey>,
    structs: Vec<MonoStruct>,
    union_seen: HashSet<UnionKey>,
    unions: Vec<MonoUnion>,
}

/// Instantiates only the functions, structs, unions, and globals reachable from
/// the program entry point.
pub(crate) fn monomorphize(checked: &CheckedProgram<'_>) -> Result<MonoProgram, Vec<TypeError>> {
    let entry_fn = &checked.functions[checked.entry_function];
    if !entry_fn.type_params.is_empty() {
        return Err(vec![TypeError::new(
            entry_fn.span.clone(),
            TypeErrorKind::InvalidEntrySignature,
            "entry function must not be generic".to_string(),
        )]);
    }
    let entry_key = FnKey {
        module: entry_fn.module,
        index: entry_fn.index,
        type_args: Vec::new(),
    };

    let mut ctx = MonoCtx {
        checked,
        fn_queue: VecDeque::new(),
        fn_seen: HashSet::new(),
        functions: Vec::new(),
        struct_seen: HashSet::new(),
        structs: Vec::new(),
        union_seen: HashSet::new(),
        unions: Vec::new(),
    };

    let mut errors = Vec::new();

    let mut globals = Vec::new();
    let mut global_ids = HashMap::new();
    for global in &checked.globals {
        match ctx.collect_type(&global.ty) {
            Ok(()) => {}
            Err(error) => errors.push(error),
        }
        let module_name = checked.resolved.module_name(global.module);
        global_ids.insert((global.module, global.index), globals.len());
        globals.push(MonoGlobal {
            name: format!("{}__{}", sanitize(module_name), sanitize(&global.name)),
            ty: global.ty.clone(),
            init: global.init.clone(),
            span: global.span.clone(),
        });
    }

    if let Err(error) = ctx.enqueue_fn(entry_key.clone(), &entry_fn.span) {
        errors.push(error);
    }

    while let Some(key) = ctx.fn_queue.pop_front() {
        if let Err(error) = ctx.instantiate_fn(&key) {
            errors.push(error);
            break;
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    disambiguate_overload_names(&mut ctx.functions);
    let fn_ids = ctx
        .functions
        .iter()
        .enumerate()
        .map(|(index, function)| (function.key.clone(), index))
        .collect::<HashMap<_, _>>();
    let entry = *fn_ids
        .get(&entry_key)
        .expect("entry specialization must be instantiated");
    let module_by_name = checked.resolved.module_by_name.clone();

    Ok(MonoProgram {
        entry,
        functions: ctx.functions,
        structs: ctx.structs,
        unions: ctx.unions,
        globals,
        module_by_name,
        fn_ids,
        global_ids,
    })
}

impl MonoCtx<'_, '_> {
    fn enqueue_fn(&mut self, key: FnKey, span: &Span) -> Result<(), TypeError> {
        if key.type_args.iter().any(ConcreteType::contains_generic) {
            return Err(TypeError::new(
                span.clone(),
                TypeErrorKind::Internal,
                "unsubstituted generic argument reached monomorphization".to_string(),
            ));
        }
        if key
            .type_args
            .iter()
            .any(|arg| concrete_depth(arg) > MAX_GENERIC_DEPTH)
        {
            return Err(TypeError::new(
                span.clone(),
                TypeErrorKind::InfiniteGenericInstantiation,
                "generic instantiation exceeded the maximum type depth".to_string(),
            ));
        }
        if self.fn_seen.contains(&key) {
            return Ok(());
        }
        let Some(function) = self.checked.checked_fn(key.module, key.index) else {
            return Err(TypeError::new(
                span.clone(),
                TypeErrorKind::Internal,
                "resolved function has no checked body".to_string(),
            ));
        };
        if function.type_params.len() != key.type_args.len() {
            return Err(TypeError::new(
                span.clone(),
                TypeErrorKind::WrongGenericArity,
                format!(
                    "function `{}` expects {} type arguments, found {}",
                    function.name,
                    function.type_params.len(),
                    key.type_args.len()
                ),
            ));
        }
        if self.fn_seen.len() >= MAX_INSTANTIATIONS {
            return Err(TypeError::new(
                span.clone(),
                TypeErrorKind::InfiniteGenericInstantiation,
                "generic instantiation count exceeded the safety limit".to_string(),
            ));
        }
        self.fn_seen.insert(key.clone());
        self.fn_queue.push_back(key);
        Ok(())
    }

    fn instantiate_fn(&mut self, key: &FnKey) -> Result<(), TypeError> {
        let function = self
            .checked
            .checked_fn(key.module, key.index)
            .cloned()
            .ok_or_else(|| {
                TypeError::new(
                    Span::default(),
                    TypeErrorKind::Internal,
                    "missing checked function during monomorphization".to_string(),
                )
            })?;
        let type_map = function
            .type_params
            .iter()
            .zip(&key.type_args)
            .map(|(param, arg)| (param.clone(), arg.clone()))
            .collect::<HashMap<_, _>>();

        let mut params = Vec::with_capacity(function.params.len());
        for (name, ty) in &function.params {
            let ty = ty.substitute(&type_map);
            if ty.contains_generic() {
                return Err(TypeError::new(
                    function.span.clone(),
                    TypeErrorKind::Internal,
                    format!("unsubstituted type in parameter `{name}`"),
                ));
            }
            params.push((name.clone(), ty));
        }
        let ret = function.ret.substitute(&type_map);
        if ret.contains_generic() {
            return Err(TypeError::new(
                function.span.clone(),
                TypeErrorKind::Internal,
                "unsubstituted return type".to_string(),
            ));
        }
        let body = function
            .body
            .iter()
            .map(|stmt| substitute_stmt(stmt, &type_map))
            .collect::<Vec<_>>();

        let module_name = self.checked.resolved.module_name(key.module);
        let mut name = format!("{}__{}", sanitize(module_name), sanitize(&function.name));
        for arg in &key.type_args {
            name.push_str("__");
            name.push_str(&arg.mangle());
        }

        self.collect_type(&ret)?;
        for (_, ty) in &params {
            self.collect_type(ty)?;
        }
        for stmt in &body {
            self.visit_stmt(stmt, &function.span)?;
        }

        self.functions.push(MonoFn {
            key: key.clone(),
            name,
            params,
            ret,
            body,
            span: function.span.clone(),
        });
        Ok(())
    }

    fn collect_type(&mut self, ty: &ConcreteType) -> Result<(), TypeError> {
        if ty.contains_generic() {
            return Err(TypeError::new(
                Span::default(),
                TypeErrorKind::Internal,
                "generic type reached monomorphization".to_string(),
            ));
        }
        if concrete_depth(ty) > MAX_GENERIC_DEPTH {
            return Err(TypeError::new(
                Span::default(),
                TypeErrorKind::InfiniteGenericInstantiation,
                "type nesting exceeded the maximum depth".to_string(),
            ));
        }
        match ty {
            ConcreteType::Ptr(inner) | ConcreteType::Array(inner, _) => self.collect_type(inner),
            ConcreteType::FnPtr { params, ret } => {
                for param in params {
                    self.collect_type(param)?;
                }
                self.collect_type(ret)
            }
            ConcreteType::Struct { module, name, args } => {
                let module_index = *self
                    .checked
                    .resolved
                    .module_by_name
                    .get(module)
                    .ok_or_else(|| {
                        TypeError::new(
                            Span::default(),
                            TypeErrorKind::UndefinedType,
                            format!("module `{module}` not found"),
                        )
                    })?;
                let index = *self.checked.resolved.modules[module_index]
                    .structs
                    .get(name)
                    .ok_or_else(|| {
                        TypeError::new(
                            Span::default(),
                            TypeErrorKind::UndefinedType,
                            format!("struct `{name}` not found"),
                        )
                    })?;
                let key = StructKey {
                    module: module_index,
                    index,
                    type_args: args.clone(),
                };
                if self.struct_seen.contains(&key) {
                    return Ok(());
                }
                if args
                    .iter()
                    .any(|arg| concrete_depth(arg) > MAX_GENERIC_DEPTH)
                {
                    return Err(TypeError::new(
                        Span::default(),
                        TypeErrorKind::InfiniteGenericInstantiation,
                        "generic struct argument exceeded the maximum depth".to_string(),
                    ));
                }
                if self.struct_seen.len() + self.union_seen.len() >= MAX_INSTANTIATIONS {
                    return Err(TypeError::new(
                        Span::default(),
                        TypeErrorKind::InfiniteGenericInstantiation,
                        "aggregate specialization count exceeded the safety limit".to_string(),
                    ));
                }
                self.struct_seen.insert(key.clone());
                let def = self.checked.resolved.struct_ast(module_index, index);
                if def.type_params.len() != args.len() {
                    return Err(TypeError::new(
                        def.span.clone(),
                        TypeErrorKind::WrongGenericArity,
                        format!(
                            "struct `{name}` expects {} type arguments, found {}",
                            def.type_params.len(),
                            args.len()
                        ),
                    ));
                }
                let type_map = def
                    .type_params
                    .iter()
                    .zip(args)
                    .map(|(param, arg)| (param.clone(), arg.clone()))
                    .collect::<HashMap<_, _>>();
                let mut fields = Vec::with_capacity(def.fields.len());
                for (field_name, field_ty) in &def.fields {
                    let field_ty = resolve_type_in(
                        &self.checked.resolved,
                        module_index,
                        &type_map,
                        field_ty,
                        &def.span,
                    )?;
                    self.collect_type(&field_ty)?;
                    fields.push((field_name.clone(), field_ty));
                }
                let mut mangled = format!(
                    "{}__{}",
                    sanitize(self.checked.resolved.module_name(module_index)),
                    sanitize(name)
                );
                for arg in args {
                    mangled.push_str("__");
                    mangled.push_str(&arg.mangle());
                }
                self.structs.push(MonoStruct {
                    key,
                    name: mangled,
                    source_name: name.clone(),
                    fields,
                });
                Ok(())
            }
            ConcreteType::Union { module, name, args } => {
                let module_index = *self
                    .checked
                    .resolved
                    .module_by_name
                    .get(module)
                    .ok_or_else(|| {
                        TypeError::new(
                            Span::default(),
                            TypeErrorKind::UndefinedType,
                            format!("module `{module}` not found"),
                        )
                    })?;
                let index = *self.checked.resolved.modules[module_index]
                    .unions
                    .get(name)
                    .ok_or_else(|| {
                        TypeError::new(
                            Span::default(),
                            TypeErrorKind::UndefinedType,
                            format!("union `{name}` not found"),
                        )
                    })?;
                let key = UnionKey {
                    module: module_index,
                    index,
                    type_args: args.clone(),
                };
                if self.union_seen.contains(&key) {
                    return Ok(());
                }
                if args
                    .iter()
                    .any(|arg| concrete_depth(arg) > MAX_GENERIC_DEPTH)
                {
                    return Err(TypeError::new(
                        Span::default(),
                        TypeErrorKind::InfiniteGenericInstantiation,
                        "generic union argument exceeded the maximum depth".to_string(),
                    ));
                }
                if self.struct_seen.len() + self.union_seen.len() >= MAX_INSTANTIATIONS {
                    return Err(TypeError::new(
                        Span::default(),
                        TypeErrorKind::InfiniteGenericInstantiation,
                        "aggregate specialization count exceeded the safety limit".to_string(),
                    ));
                }
                self.union_seen.insert(key.clone());
                let def = self.checked.resolved.union_ast(module_index, index);
                if def.type_params.len() != args.len() {
                    return Err(TypeError::new(
                        def.span.clone(),
                        TypeErrorKind::WrongGenericArity,
                        format!(
                            "union `{name}` expects {} type arguments, found {}",
                            def.type_params.len(),
                            args.len()
                        ),
                    ));
                }
                let type_map = def
                    .type_params
                    .iter()
                    .zip(args)
                    .map(|(param, arg)| (param.clone(), arg.clone()))
                    .collect::<HashMap<_, _>>();
                let mut fields = Vec::with_capacity(def.fields.len());
                for (field_name, field_ty) in &def.fields {
                    let field_ty = resolve_type_in(
                        &self.checked.resolved,
                        module_index,
                        &type_map,
                        field_ty,
                        &def.span,
                    )?;
                    self.collect_type(&field_ty)?;
                    fields.push((field_name.clone(), field_ty));
                }
                let mut mangled = format!(
                    "{}__{}",
                    sanitize(self.checked.resolved.module_name(module_index)),
                    sanitize(name)
                );
                for arg in args {
                    mangled.push_str("__");
                    mangled.push_str(&arg.mangle());
                }
                self.unions.push(MonoUnion {
                    key,
                    name: mangled,
                    source_name: name.clone(),
                    fields,
                });
                Ok(())
            }
            ConcreteType::Generic(_) => unreachable!(),
            _ => Ok(()),
        }
    }

    fn visit_stmt(&mut self, stmt: &TStmt, span: &Span) -> Result<(), TypeError> {
        match stmt {
            TStmt::Let { ty, init, .. } => {
                self.collect_type(ty)?;
                self.visit_expr(init, span)
            }
            TStmt::Assign { val, .. } | TStmt::Expr(val, _) => self.visit_expr(val, span),
            TStmt::Store { ptr, val, .. } => {
                self.visit_expr(ptr, span)?;
                self.visit_expr(val, span)
            }
            TStmt::MemCopy { dst, src, size, .. } => {
                self.visit_expr(dst, span)?;
                self.visit_expr(src, span)?;
                self.visit_expr(size, span)
            }
            TStmt::Block(body, _) | TStmt::Loop { body, .. } => {
                for nested in body {
                    self.visit_stmt(nested, span)?;
                }
                Ok(())
            }
            TStmt::If {
                cond,
                then_body,
                else_body,
                ..
            } => {
                self.visit_expr(cond, span)?;
                for nested in then_body {
                    self.visit_stmt(nested, span)?;
                }
                for nested in else_body {
                    self.visit_stmt(nested, span)?;
                }
                Ok(())
            }
            TStmt::Return(Some(value), _) => self.visit_expr(value, span),
            TStmt::Break(_) | TStmt::Continue(_) | TStmt::Return(None, _) | TStmt::Trap(_) => {
                Ok(())
            }
        }
    }

    fn visit_expr(&mut self, expr: &TExpr, span: &Span) -> Result<(), TypeError> {
        self.collect_type(&expr.ty)?;
        match &expr.kind {
            TExprKind::FieldAddr { base, .. } => self.visit_expr(base, span),
            TExprKind::IndexAddr { base, index } => {
                self.visit_expr(base, span)?;
                self.visit_expr(index, span)
            }
            TExprKind::Load(ptr) => self.visit_expr(ptr, span),
            TExprKind::SizeOf(ty) | TExprKind::AlignOf(ty) => self.collect_type(ty),
            TExprKind::Unary { val, .. } | TExprKind::Cast { val } => self.visit_expr(val, span),
            TExprKind::Binary { lhs, rhs, .. } => {
                self.visit_expr(lhs, span)?;
                self.visit_expr(rhs, span)
            }
            TExprKind::FnAddr {
                module,
                index,
                type_args,
            } => {
                for arg in type_args {
                    self.collect_type(arg)?;
                }
                self.enqueue_fn(
                    FnKey {
                        module: *module,
                        index: *index,
                        type_args: type_args.clone(),
                    },
                    span,
                )
            }
            TExprKind::Call {
                module,
                index,
                type_args,
                args,
            } => {
                for arg in args {
                    self.visit_expr(arg, span)?;
                }
                for arg in type_args {
                    self.collect_type(arg)?;
                }
                self.enqueue_fn(
                    FnKey {
                        module: *module,
                        index: *index,
                        type_args: type_args.clone(),
                    },
                    span,
                )
            }
            TExprKind::IndirectCall { callee, args } => {
                self.visit_expr(callee, span)?;
                for arg in args {
                    self.visit_expr(arg, span)?;
                }
                Ok(())
            }
            TExprKind::Syscall { num, args } => {
                self.visit_expr(num, span)?;
                for arg in args {
                    self.visit_expr(arg, span)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

fn disambiguate_overload_names(functions: &mut [MonoFn]) {
    let mut counts = HashMap::new();
    for function in functions.iter() {
        *counts.entry(function.name.clone()).or_insert(0usize) += 1;
    }
    for function in functions {
        if counts.get(&function.name).copied().unwrap_or(0) > 1 {
            if function.params.is_empty() {
                function.name.push_str("__unit");
            } else {
                for (_, ty) in &function.params {
                    function.name.push_str("__");
                    function.name.push_str(&ty.mangle());
                }
            }
        }
    }
}

fn concrete_depth(ty: &ConcreteType) -> usize {
    match ty {
        ConcreteType::Ptr(inner) | ConcreteType::Array(inner, _) => 1 + concrete_depth(inner),
        ConcreteType::Struct { args, .. } | ConcreteType::Union { args, .. } => {
            1 + args.iter().map(concrete_depth).max().unwrap_or(0)
        }
        ConcreteType::FnPtr { params, ret } => {
            1 + params
                .iter()
                .map(concrete_depth)
                .chain(std::iter::once(concrete_depth(ret)))
                .max()
                .unwrap_or(0)
        }
        _ => 0,
    }
}

fn substitute_expr(expr: &TExpr, map: &HashMap<String, ConcreteType>) -> TExpr {
    let ty = expr.ty.substitute(map);
    let kind = match &expr.kind {
        TExprKind::ConstInt(value) => TExprKind::ConstInt(*value),
        TExprKind::ConstUint(value) => TExprKind::ConstUint(*value),
        TExprKind::ConstFloat(value) => TExprKind::ConstFloat(*value),
        TExprKind::ConstBool(value) => TExprKind::ConstBool(*value),
        TExprKind::ConstNull => TExprKind::ConstNull,
        TExprKind::BytesLit(bytes) => TExprKind::BytesLit(bytes.clone()),
        TExprKind::BytesLen(bytes) => TExprKind::BytesLen(bytes.clone()),
        TExprKind::Var(name) => TExprKind::Var(name.clone()),
        TExprKind::GlobalAddr { module, index } => TExprKind::GlobalAddr {
            module: *module,
            index: *index,
        },
        TExprKind::AddrOfVar(name) => TExprKind::AddrOfVar(name.clone()),
        TExprKind::FieldAddr { base, field_idx } => TExprKind::FieldAddr {
            base: Box::new(substitute_expr(base, map)),
            field_idx: *field_idx,
        },
        TExprKind::IndexAddr { base, index } => TExprKind::IndexAddr {
            base: Box::new(substitute_expr(base, map)),
            index: Box::new(substitute_expr(index, map)),
        },
        TExprKind::Load(ptr) => TExprKind::Load(Box::new(substitute_expr(ptr, map))),
        TExprKind::SizeOf(type_) => TExprKind::SizeOf(type_.substitute(map)),
        TExprKind::AlignOf(type_) => TExprKind::AlignOf(type_.substitute(map)),
        TExprKind::Unary { op, val } => TExprKind::Unary {
            op: *op,
            val: Box::new(substitute_expr(val, map)),
        },
        TExprKind::Binary { op, lhs, rhs } => TExprKind::Binary {
            op: *op,
            lhs: Box::new(substitute_expr(lhs, map)),
            rhs: Box::new(substitute_expr(rhs, map)),
        },
        TExprKind::Cast { val } => TExprKind::Cast {
            val: Box::new(substitute_expr(val, map)),
        },
        TExprKind::FnAddr {
            module,
            index,
            type_args,
        } => TExprKind::FnAddr {
            module: *module,
            index: *index,
            type_args: type_args.iter().map(|arg| arg.substitute(map)).collect(),
        },
        TExprKind::Call {
            module,
            index,
            type_args,
            args,
        } => TExprKind::Call {
            module: *module,
            index: *index,
            type_args: type_args.iter().map(|arg| arg.substitute(map)).collect(),
            args: args.iter().map(|arg| substitute_expr(arg, map)).collect(),
        },
        TExprKind::IndirectCall { callee, args } => TExprKind::IndirectCall {
            callee: Box::new(substitute_expr(callee, map)),
            args: args.iter().map(|arg| substitute_expr(arg, map)).collect(),
        },
        TExprKind::Syscall { num, args } => TExprKind::Syscall {
            num: Box::new(substitute_expr(num, map)),
            args: args.iter().map(|arg| substitute_expr(arg, map)).collect(),
        },
    };
    TExpr { ty, kind }
}

fn substitute_stmt(stmt: &TStmt, map: &HashMap<String, ConcreteType>) -> TStmt {
    match stmt {
        TStmt::Let {
            name,
            ty,
            init,
            span,
        } => TStmt::Let {
            name: name.clone(),
            ty: ty.substitute(map),
            init: substitute_expr(init, map),
            span: span.clone(),
        },
        TStmt::Assign { name, val, span } => TStmt::Assign {
            name: name.clone(),
            val: substitute_expr(val, map),
            span: span.clone(),
        },
        TStmt::Store { ptr, val, span } => TStmt::Store {
            ptr: substitute_expr(ptr, map),
            val: substitute_expr(val, map),
            span: span.clone(),
        },
        TStmt::MemCopy {
            dst,
            src,
            size,
            span,
        } => TStmt::MemCopy {
            dst: substitute_expr(dst, map),
            src: substitute_expr(src, map),
            size: substitute_expr(size, map),
            span: span.clone(),
        },
        TStmt::Expr(expr, span) => TStmt::Expr(substitute_expr(expr, map), span.clone()),
        TStmt::Block(body, span) => TStmt::Block(
            body.iter().map(|stmt| substitute_stmt(stmt, map)).collect(),
            span.clone(),
        ),
        TStmt::If {
            cond,
            then_body,
            else_body,
            span,
        } => TStmt::If {
            cond: substitute_expr(cond, map),
            then_body: then_body
                .iter()
                .map(|stmt| substitute_stmt(stmt, map))
                .collect(),
            else_body: else_body
                .iter()
                .map(|stmt| substitute_stmt(stmt, map))
                .collect(),
            span: span.clone(),
        },
        TStmt::Loop { body, span } => TStmt::Loop {
            body: body.iter().map(|stmt| substitute_stmt(stmt, map)).collect(),
            span: span.clone(),
        },
        TStmt::Break(span) => TStmt::Break(span.clone()),
        TStmt::Continue(span) => TStmt::Continue(span.clone()),
        TStmt::Return(value, span) => TStmt::Return(
            value.as_ref().map(|value| substitute_expr(value, map)),
            span.clone(),
        ),
        TStmt::Trap(span) => TStmt::Trap(span.clone()),
    }
}
