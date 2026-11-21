//! Exhaustive static IR verification.

use crate::ir::{BinOp, CallTarget, Expr, Function, Module, Stmt, UnOp};
use crate::types::Type;

/// A structured IR verification diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    /// The entry function identifier is out of bounds.
    EntryOutOfBounds(usize),
    /// The entry function has an unsupported signature.
    InvalidEntrySignature(String),
    /// An identifier from one of the module tables is out of bounds.
    IdOutOfBounds {
        /// Which identifier family.
        family: &'static str,
        /// The invalid index.
        index: usize,
    },
    /// A struct or union is recursively embedded without a pointer.
    RecursiveType {
        /// `struct` or `union`.
        family: &'static str,
        /// The recursive table index.
        index: usize,
    },
    /// Two operand types do not match.
    TypeMismatch {
        /// Human-readable operation context.
        context: String,
        /// Type required by the operation.
        expected: Type,
        /// Type found in the IR.
        found: Type,
    },
    /// An operation received a type class it does not support.
    InvalidType {
        /// Human-readable operation context.
        context: String,
        /// The unsupported type.
        ty: Type,
    },
    /// An operation was otherwise malformed.
    InvalidOperation {
        /// Human-readable diagnostic.
        context: String,
    },
    /// `Break` appeared outside a loop.
    BreakOutsideLoop(String),
    /// `Continue` appeared outside a loop.
    ContinueOutsideLoop(String),
    /// A syscall has more than six arguments.
    SyscallArity {
        /// Number of arguments found.
        found: usize,
    },
    /// A global initializer length does not match its type size.
    InvalidInitializer {
        /// Global index.
        global: usize,
        /// Expected byte count.
        expected: usize,
        /// Actual byte count.
        found: usize,
    },
    /// A function signature uses aggregate types by value, which Phase 0 does
    /// not support.
    UnsupportedAbiType {
        /// Function name.
        function: String,
        /// Offending type.
        ty: Type,
    },
}

fn id_error(family: &'static str, index: usize) -> VerifyError {
    VerifyError::IdOutOfBounds { family, index }
}

fn mismatch(context: impl Into<String>, expected: Type, found: Type) -> VerifyError {
    VerifyError::TypeMismatch {
        context: context.into(),
        expected,
        found,
    }
}

fn invalid_type(context: impl Into<String>, ty: Type) -> VerifyError {
    VerifyError::InvalidType {
        context: context.into(),
        ty,
    }
}

/// Infers the result type of an expression.
///
/// # Errors
///
/// Returns a [`VerifyError`] when the expression is ill-typed or references an
/// out-of-bounds identifier.
pub fn infer_expr_type(
    module: &Module,
    function: &Function,
    expr: &Expr,
) -> Result<Type, VerifyError> {
    match expr {
        Expr::ConstInt(_, ty) => {
            if ty.is_signed_int() {
                Ok(ty.clone())
            } else {
                Err(invalid_type("ConstInt", ty.clone()))
            }
        }
        Expr::ConstUint(_, ty) => {
            if ty.is_unsigned_int() {
                Ok(ty.clone())
            } else {
                Err(invalid_type("ConstUint", ty.clone()))
            }
        }
        Expr::ConstFloat(_, ty) => {
            if ty.is_float() {
                Ok(ty.clone())
            } else {
                Err(invalid_type("ConstFloat", ty.clone()))
            }
        }
        Expr::ConstBool(_) => Ok(Type::Bool),
        Expr::ConstNull(ty) => {
            if matches!(ty, Type::Ptr(_)) {
                validate_type(module, ty).map_err(|found| invalid_type("ConstNull", found))?;
                Ok(ty.clone())
            } else {
                Err(invalid_type("ConstNull", ty.clone()))
            }
        }
        Expr::RodataPtr(id) => {
            if id.0 < module.rodata.len() {
                Ok(Type::Ptr(Box::new(Type::U8)))
            } else {
                Err(id_error("rodata", id.0))
            }
        }
        Expr::RodataLen(id) => {
            if id.0 < module.rodata.len() {
                Ok(Type::U64)
            } else {
                Err(id_error("rodata", id.0))
            }
        }
        Expr::Local(id) => {
            let ty = function
                .local_ty(*id)
                .ok_or_else(|| id_error("local", id.0))?
                .clone();
            if ty.is_scalar() {
                Ok(ty)
            } else {
                Err(invalid_type(
                    format!("Local({}) requires a scalar local", id.0),
                    ty,
                ))
            }
        }
        Expr::AddrOfLocal(id) => {
            let ty = function
                .local_ty(*id)
                .ok_or_else(|| id_error("local", id.0))?
                .clone();
            Ok(Type::Ptr(Box::new(ty)))
        }
        Expr::GlobalAddr(id) => {
            let global = module
                .globals
                .get(id.0)
                .ok_or_else(|| id_error("global", id.0))?;
            Ok(Type::Ptr(Box::new(global.ty.clone())))
        }
        Expr::FnAddr(id) => {
            let target = module
                .functions
                .get(id.0)
                .ok_or_else(|| id_error("function", id.0))?;
            Ok(Type::FnPtr {
                params: target.params.iter().map(|p| p.ty.clone()).collect(),
                ret: Box::new(target.ret.clone()),
            })
        }
        Expr::SizeOf(ty) | Expr::AlignOf(ty) => {
            validate_type(module, ty).map_err(|found| invalid_type("SizeOf/AlignOf", found))?;
            Ok(Type::U64)
        }
        Expr::Load { ptr, ty } => {
            let ptr_ty = infer_expr_type(module, function, ptr)?;
            let Type::Ptr(pointee) = ptr_ty else {
                return Err(invalid_type("Load pointer", ptr_ty));
            };
            if *pointee != *ty {
                return Err(mismatch("Load pointee", (*pointee).clone(), ty.clone()));
            }
            if !ty.is_scalar() {
                return Err(invalid_type("Load", ty.clone()));
            }
            validate_type(module, ty).map_err(|found| invalid_type("Load", found))?;
            Ok(ty.clone())
        }
        Expr::FieldAddr {
            base_ptr,
            struct_id,
            field_idx,
        } => {
            let def = module
                .structs
                .get(struct_id.0)
                .ok_or_else(|| id_error("struct", struct_id.0))?;
            let field = def
                .fields
                .get(*field_idx)
                .ok_or_else(|| id_error("struct field", *field_idx))?;
            let base_ty = infer_expr_type(module, function, base_ptr)?;
            let expected = Type::Ptr(Box::new(Type::Struct(*struct_id)));
            if base_ty != expected {
                return Err(mismatch("FieldAddr base", expected, base_ty));
            }
            validate_type(module, field).map_err(|found| invalid_type("FieldAddr field", found))?;
            Ok(Type::Ptr(Box::new(field.clone())))
        }
        Expr::UnionFieldAddr {
            base_ptr,
            union_id,
            field_idx,
        } => {
            let def = module
                .unions
                .get(union_id.0)
                .ok_or_else(|| id_error("union", union_id.0))?;
            let field = def
                .fields
                .get(*field_idx)
                .ok_or_else(|| id_error("union field", *field_idx))?;
            let base_ty = infer_expr_type(module, function, base_ptr)?;
            let expected = Type::Ptr(Box::new(Type::Union(*union_id)));
            if base_ty != expected {
                return Err(mismatch("UnionFieldAddr base", expected, base_ty));
            }
            validate_type(module, field)
                .map_err(|found| invalid_type("UnionFieldAddr field", found))?;
            Ok(Type::Ptr(Box::new(field.clone())))
        }
        Expr::IndexAddr {
            base_ptr,
            elem_ty,
            index,
        } => {
            let base_ty = infer_expr_type(module, function, base_ptr)?;
            let expected = Type::Ptr(Box::new(elem_ty.clone()));
            if base_ty != expected {
                return Err(mismatch("IndexAddr base", expected, base_ty));
            }
            let index_ty = infer_expr_type(module, function, index)?;
            if !index_ty.is_integer() {
                return Err(invalid_type("IndexAddr index", index_ty));
            }
            validate_type(module, elem_ty)
                .map_err(|found| invalid_type("IndexAddr element", found))?;
            Ok(Type::Ptr(Box::new(elem_ty.clone())))
        }
        Expr::Unary { op, val } => {
            let ty = infer_expr_type(module, function, val)?;
            match op {
                UnOp::Neg => {
                    if ty.is_signed_int() || ty.is_unsigned_int() || ty.is_float() {
                        Ok(ty)
                    } else {
                        Err(invalid_type("Unary Neg", ty))
                    }
                }
                UnOp::BitNot => {
                    if ty.is_integer() {
                        Ok(ty)
                    } else {
                        Err(invalid_type("Unary BitNot", ty))
                    }
                }
                UnOp::LogNot => {
                    if ty.is_bool() {
                        Ok(Type::Bool)
                    } else {
                        Err(invalid_type("Unary LogNot", ty))
                    }
                }
            }
        }
        Expr::Binary { op, lhs, rhs } => {
            let left = infer_expr_type(module, function, lhs)?;
            let right = infer_expr_type(module, function, rhs)?;
            if left != right {
                return Err(mismatch("Binary operands", left, right));
            }
            infer_binary_result(*op, left)
        }
        Expr::Cast { val, to } => {
            let from = infer_expr_type(module, function, val)?;
            validate_type(module, to).map_err(|found| invalid_type("Cast target", found))?;
            if cast_allowed(&from, to) {
                Ok(to.clone())
            } else {
                Err(invalid_type(format!("Cast from {from:?}"), to.clone()))
            }
        }
        Expr::Call { target, args } => {
            let arg_types = args
                .iter()
                .map(|arg| infer_expr_type(module, function, arg))
                .collect::<Result<Vec<_>, _>>()?;
            match target {
                CallTarget::Direct(id) => {
                    let callee = module
                        .functions
                        .get(id.0)
                        .ok_or_else(|| id_error("function", id.0))?;
                    check_call_signature(
                        &format!("direct call to {}", callee.name),
                        &callee
                            .params
                            .iter()
                            .map(|p| p.ty.clone())
                            .collect::<Vec<_>>(),
                        &callee.ret,
                        &arg_types,
                    )?;
                    Ok(callee.ret.clone())
                }
                CallTarget::Indirect(callee_expr) => {
                    let callee_ty = infer_expr_type(module, function, callee_expr)?;
                    let Type::FnPtr { params, ret } = callee_ty else {
                        return Err(invalid_type("indirect call target", callee_ty));
                    };
                    check_call_signature("indirect call", &params, &ret, &arg_types)?;
                    Ok(*ret)
                }
            }
        }
        Expr::Syscall { num, args } => {
            let num_ty = infer_expr_type(module, function, num)?;
            if !is_syscall_word(&num_ty) {
                return Err(invalid_type("syscall number", num_ty));
            }
            if args.len() > 6 {
                return Err(VerifyError::SyscallArity { found: args.len() });
            }
            for arg in args {
                let ty = infer_expr_type(module, function, arg)?;
                if !is_syscall_word(&ty) {
                    return Err(invalid_type("syscall argument", ty));
                }
            }
            Ok(Type::I64)
        }
    }
}

fn infer_binary_result(op: BinOp, ty: Type) -> Result<Type, VerifyError> {
    match op {
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
            if ty.is_integer() || ty.is_float() {
                Ok(ty)
            } else {
                Err(invalid_type("float/integer binary arithmetic", ty))
            }
        }
        BinOp::Mod => {
            if ty.is_integer() {
                Ok(ty)
            } else {
                Err(invalid_type("Mod", ty))
            }
        }
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr => {
            if ty.is_integer() {
                Ok(ty)
            } else {
                Err(invalid_type("bitwise/shift", ty))
            }
        }
        BinOp::Eq | BinOp::Ne => {
            if ty.is_scalar() {
                Ok(Type::Bool)
            } else {
                Err(invalid_type("equality comparison", ty))
            }
        }
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            if ty.is_integer() || ty.is_float() {
                Ok(Type::Bool)
            } else {
                Err(invalid_type("ordered comparison", ty))
            }
        }
        BinOp::LogAnd | BinOp::LogOr => {
            if ty.is_bool() {
                Ok(Type::Bool)
            } else {
                Err(invalid_type("logical operator", ty))
            }
        }
    }
}

fn is_syscall_word(ty: &Type) -> bool {
    matches!(ty, Type::I64 | Type::U64 | Type::Ptr(_))
}

fn cast_allowed(from: &Type, to: &Type) -> bool {
    match (from, to) {
        (a, b) if a.is_integer() && b.is_integer() => true,
        (a, Type::F32 | Type::F64) if a.is_integer() => true,
        (Type::F32 | Type::F64, b) if b.is_integer() => true,
        (Type::F32, Type::F64) | (Type::F64, Type::F32) => true,
        (Type::Ptr(_), Type::U64) | (Type::U64, Type::Ptr(_)) => true,
        (Type::Ptr(_), Type::Ptr(_)) => true,
        (Type::FnPtr { .. }, Type::U64) | (Type::U64, Type::FnPtr { .. }) => true,
        _ => false,
    }
}

fn check_call_signature(
    context: &str,
    params: &[Type],
    ret: &Type,
    args: &[Type],
) -> Result<(), VerifyError> {
    if params.len() != args.len() {
        return Err(VerifyError::InvalidOperation {
            context: format!(
                "{context}: expected {} arguments, found {}",
                params.len(),
                args.len()
            ),
        });
    }
    for (idx, (expected, found)) in params.iter().zip(args).enumerate() {
        if expected != found {
            return Err(mismatch(
                format!("{context} argument {idx}"),
                expected.clone(),
                found.clone(),
            ));
        }
    }
    let _ = ret;
    Ok(())
}

fn validate_type(module: &Module, ty: &Type) -> Result<(), Type> {
    match ty {
        Type::Ptr(inner) | Type::Array(inner, _) => validate_type(module, inner),
        Type::Struct(id) => {
            if id.0 < module.structs.len() {
                Ok(())
            } else {
                Err(Type::Struct(*id))
            }
        }
        Type::Union(id) => {
            if id.0 < module.unions.len() {
                Ok(())
            } else {
                Err(Type::Union(*id))
            }
        }
        Type::FnPtr { params, ret } => {
            for param in params {
                validate_type(module, param)?;
            }
            validate_type(module, ret)
        }
        _ => Ok(()),
    }
}

/// Verifies every invariant of a module, returning all diagnostics found.
#[must_use]
pub fn verify_module(module: &Module) -> Result<(), Vec<VerifyError>> {
    let mut errors = Vec::new();

    if module.entry.0 >= module.functions.len() {
        errors.push(VerifyError::EntryOutOfBounds(module.entry.0));
    }

    for (index, def) in module.structs.iter().enumerate() {
        for field in &def.fields {
            if let Err(found) = validate_type(module, field) {
                errors.push(invalid_type(format!("struct {index} field"), found));
            }
        }
    }
    for (index, def) in module.unions.iter().enumerate() {
        for field in &def.fields {
            if let Err(found) = validate_type(module, field) {
                errors.push(invalid_type(format!("union {index} field"), found));
            }
        }
    }

    for (index, _) in module.structs.iter().enumerate() {
        if module.has_infinite_size(&Type::Struct(crate::ir::StructId(index))) {
            errors.push(VerifyError::RecursiveType {
                family: "struct",
                index,
            });
        }
    }
    for (index, _) in module.unions.iter().enumerate() {
        if module.has_infinite_size(&Type::Union(crate::ir::UnionId(index))) {
            errors.push(VerifyError::RecursiveType {
                family: "union",
                index,
            });
        }
    }

    for (index, global) in module.globals.iter().enumerate() {
        if let Err(found) = validate_type(module, &global.ty) {
            errors.push(invalid_type(format!("global {index}"), found));
            continue;
        }
        if module.has_infinite_size(&global.ty) {
            continue;
        }
        if let Some(init) = &global.init {
            let expected = module.size_of(&global.ty);
            if init.len() != expected {
                errors.push(VerifyError::InvalidInitializer {
                    global: index,
                    expected,
                    found: init.len(),
                });
            }
        }
    }

    for (index, function) in module.functions.iter().enumerate() {
        for param in &function.params {
            if let Err(found) = validate_type(module, &param.ty) {
                errors.push(invalid_type(format!("function {index} parameter"), found));
            }
            if !param.ty.is_abi_scalar() {
                errors.push(VerifyError::UnsupportedAbiType {
                    function: function.name.clone(),
                    ty: param.ty.clone(),
                });
            }
        }
        if let Err(found) = validate_type(module, &function.ret) {
            errors.push(invalid_type(format!("function {index} return"), found));
        }
        if !function.ret.is_abi_scalar() && function.ret != Type::Void {
            errors.push(VerifyError::UnsupportedAbiType {
                function: function.name.clone(),
                ty: function.ret.clone(),
            });
        }
        for local in &function.locals {
            if let Err(found) = validate_type(module, &local.ty) {
                errors.push(invalid_type(format!("function {index} local"), found));
            }
        }
    }

    if let Some(entry) = module.functions.get(module.entry.0) {
        let valid_return = matches!(entry.ret, Type::Void | Type::I32 | Type::I64);
        let valid_params = entry.params.is_empty()
            || entry.params.iter().map(|p| p.ty.clone()).eq([
                Type::I64,
                Type::Ptr(Box::new(Type::Ptr(Box::new(Type::U8)))),
                Type::Ptr(Box::new(Type::Ptr(Box::new(Type::U8)))),
            ]);
        if !valid_return || !valid_params {
            errors.push(VerifyError::InvalidEntrySignature(format!(
                "entry `{}` must be main() or main(i64, ptr<ptr<u8>>, ptr<ptr<u8>>) returning void, i32, or i64",
                entry.name
            )));
        }
    }

    for function in &module.functions {
        verify_body(module, function, &mut errors);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn verify_body(module: &Module, function: &Function, errors: &mut Vec<VerifyError>) {
    for stmt in &function.body {
        verify_stmt(module, function, stmt, 0, errors);
    }
}

fn verify_expr(
    module: &Module,
    function: &Function,
    expr: &Expr,
    errors: &mut Vec<VerifyError>,
) -> Option<Type> {
    match infer_expr_type(module, function, expr) {
        Ok(ty) => Some(ty),
        Err(error) => {
            errors.push(error);
            None
        }
    }
}

fn require_type(
    module: &Module,
    function: &Function,
    expr: &Expr,
    expected: &Type,
    context: &str,
    errors: &mut Vec<VerifyError>,
) -> Option<Type> {
    let found = verify_expr(module, function, expr, errors)?;
    if found != *expected {
        errors.push(mismatch(context, expected.clone(), found));
        return None;
    }
    Some(found)
}

fn verify_stmt(
    module: &Module,
    function: &Function,
    stmt: &Stmt,
    loop_depth: usize,
    errors: &mut Vec<VerifyError>,
) {
    match stmt {
        Stmt::Assign { local, val } => {
            let Some(local_ty) = function.local_ty(*local).cloned() else {
                errors.push(id_error("local", local.0));
                return;
            };
            if !local_ty.is_scalar() {
                errors.push(invalid_type("Assign target", local_ty));
                return;
            }
            require_type(module, function, val, &local_ty, "Assign value", errors);
        }
        Stmt::Store { ptr, ty, val } => {
            if !ty.is_scalar() {
                errors.push(invalid_type("Store value type", ty.clone()));
                return;
            }
            let expected_ptr = Type::Ptr(Box::new(ty.clone()));
            let Some(found_ptr) = verify_expr(module, function, ptr, errors) else {
                return;
            };
            if found_ptr != expected_ptr {
                errors.push(mismatch("Store pointer", expected_ptr, found_ptr));
            }
            require_type(module, function, val, ty, "Store value", errors);
        }
        Stmt::MemCopy { dst, src, size } => {
            for (expr, label) in [(dst, "MemCopy destination"), (src, "MemCopy source")] {
                if let Some(ty) = verify_expr(module, function, expr, errors) {
                    if !matches!(ty, Type::Ptr(_)) {
                        errors.push(invalid_type(label, ty));
                    }
                }
            }
            if let Some(ty) = verify_expr(module, function, size, errors) {
                if !ty.is_integer() {
                    errors.push(invalid_type("MemCopy size", ty));
                }
            }
        }
        Stmt::Expr(expr) => {
            verify_expr(module, function, expr, errors);
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            require_type(module, function, cond, &Type::Bool, "If condition", errors);
            for nested in then_body {
                verify_stmt(module, function, nested, loop_depth, errors);
            }
            for nested in else_body {
                verify_stmt(module, function, nested, loop_depth, errors);
            }
        }
        Stmt::Loop { body } => {
            for nested in body {
                verify_stmt(module, function, nested, loop_depth + 1, errors);
            }
        }
        Stmt::Break => {
            if loop_depth == 0 {
                errors.push(VerifyError::BreakOutsideLoop(function.name.clone()));
            }
        }
        Stmt::Continue => {
            if loop_depth == 0 {
                errors.push(VerifyError::ContinueOutsideLoop(function.name.clone()));
            }
        }
        Stmt::Return(value) => match (value, &function.ret) {
            (None, Type::Void) => {}
            (None, expected) => errors.push(mismatch("Return value", expected.clone(), Type::Void)),
            (Some(_expr), Type::Void) => {
                errors.push(VerifyError::InvalidOperation {
                    context: "void function returned a value".to_string(),
                });
            }
            (Some(expr), expected) => {
                require_type(module, function, expr, expected, "Return value", errors);
            }
        },
        Stmt::Trap => {}
    }
}
