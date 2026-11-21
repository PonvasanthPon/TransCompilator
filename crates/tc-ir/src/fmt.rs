//! Deterministic textual IR formatting.

use std::fmt::Write as _;

use crate::ir::{BinOp, CallTarget, Expr, Function, Module, Stmt, UnOp};
use crate::types::Type;

/// Formats a module as deterministic, human-readable text.
#[must_use]
pub fn format_module(module: &Module) -> String {
    let mut out = String::new();
    writeln!(&mut out, "module entry=tc_fn_{} {{", module.entry.0)
        .expect("writing to String cannot fail");
    for (index, def) in module.structs.iter().enumerate() {
        let fields = def
            .fields
            .iter()
            .map(format_type)
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(
            &mut out,
            "  struct #{index} \"{}\" {{ {fields} }}",
            def.name
        )
        .expect("writing to String cannot fail");
    }
    for (index, def) in module.unions.iter().enumerate() {
        let fields = def
            .fields
            .iter()
            .map(format_type)
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(&mut out, "  union #{index} \"{}\" {{ {fields} }}", def.name)
            .expect("writing to String cannot fail");
    }
    for (index, blob) in module.rodata.iter().enumerate() {
        writeln!(
            &mut out,
            "  rodata #{index} \"{}\" len={}",
            blob.name,
            blob.bytes.len()
        )
        .expect("writing to String cannot fail");
    }
    for (index, global) in module.globals.iter().enumerate() {
        let storage = if global.init.is_some() { "data" } else { "bss" };
        writeln!(
            &mut out,
            "  global #{index} \"{}\" {} {storage}",
            global.name,
            format_type(&global.ty)
        )
        .expect("writing to String cannot fail");
    }
    for (index, function) in module.functions.iter().enumerate() {
        format_function(&mut out, index, function);
    }
    out.push_str("}\n");
    out
}

fn format_function(out: &mut String, index: usize, function: &Function) {
    let params = function
        .params
        .iter()
        .map(|param| format!("{}: {}", param.name, format_type(&param.ty)))
        .collect::<Vec<_>>()
        .join(", ");
    writeln!(
        out,
        "  fn #{index} \"{}\"({params}) -> {} {{",
        function.name,
        format_type(&function.ret)
    )
    .expect("writing to String cannot fail");
    for (local_index, local) in function.locals.iter().enumerate() {
        let id = function.params.len() + local_index;
        writeln!(
            out,
            "    local %{id} \"{}\": {}",
            local.name,
            format_type(&local.ty)
        )
        .expect("writing to String cannot fail");
    }
    for stmt in &function.body {
        format_stmt(out, stmt, 2);
    }
    out.push_str("  }\n");
}

fn indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

fn format_stmt(out: &mut String, stmt: &Stmt, level: usize) {
    indent(out, level);
    match stmt {
        Stmt::Assign { local, val } => {
            writeln!(out, "%{} = {}", local.0, format_expr(val))
                .expect("writing to String cannot fail");
        }
        Stmt::Store { ptr, ty, val } => {
            writeln!(
                out,
                "store[{}] {} <- {}",
                format_type(ty),
                format_expr(ptr),
                format_expr(val)
            )
            .expect("writing to String cannot fail");
        }
        Stmt::MemCopy { dst, src, size } => {
            writeln!(
                out,
                "memcopy {}, {}, {}",
                format_expr(dst),
                format_expr(src),
                format_expr(size)
            )
            .expect("writing to String cannot fail");
        }
        Stmt::Expr(expr) => {
            writeln!(out, "eval {}", format_expr(expr)).expect("writing to String cannot fail");
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            writeln!(out, "if {} {{", format_expr(cond)).expect("writing to String cannot fail");
            for nested in then_body {
                format_stmt(out, nested, level + 1);
            }
            indent(out, level);
            out.push_str("} else {\n");
            for nested in else_body {
                format_stmt(out, nested, level + 1);
            }
            indent(out, level);
            out.push_str("}\n");
        }
        Stmt::Loop { body } => {
            out.push_str("loop {\n");
            for nested in body {
                format_stmt(out, nested, level + 1);
            }
            indent(out, level);
            out.push_str("}\n");
        }
        Stmt::Break => out.push_str("break\n"),
        Stmt::Continue => out.push_str("continue\n"),
        Stmt::Return(None) => out.push_str("return\n"),
        Stmt::Return(Some(expr)) => {
            writeln!(out, "return {}", format_expr(expr)).expect("writing to String cannot fail");
        }
        Stmt::Trap => out.push_str("trap\n"),
    }
}

/// Formats a type in the textual IR syntax.
#[must_use]
pub fn format_type(ty: &Type) -> String {
    match ty {
        Type::Void => "void".to_string(),
        Type::Bool => "bool".to_string(),
        Type::I8 => "i8".to_string(),
        Type::I16 => "i16".to_string(),
        Type::I32 => "i32".to_string(),
        Type::I64 => "i64".to_string(),
        Type::U8 => "u8".to_string(),
        Type::U16 => "u16".to_string(),
        Type::U32 => "u32".to_string(),
        Type::U64 => "u64".to_string(),
        Type::F32 => "f32".to_string(),
        Type::F64 => "f64".to_string(),
        Type::Ptr(inner) => format!("ptr<{}>", format_type(inner)),
        Type::Array(inner, count) => format!("array<{}, {count}>", format_type(inner)),
        Type::Struct(id) => format!("struct#{}", id.0),
        Type::Union(id) => format!("union#{}", id.0),
        Type::FnPtr { params, ret } => {
            let params = params
                .iter()
                .map(format_type)
                .collect::<Vec<_>>()
                .join(", ");
            format!("fnptr({params}) -> {}", format_type(ret))
        }
    }
}

/// Formats an expression in the textual IR syntax.
#[must_use]
pub fn format_expr(expr: &Expr) -> String {
    match expr {
        Expr::ConstInt(value, ty) => format!("const {value} : {}", format_type(ty)),
        Expr::ConstUint(value, ty) => format!("const {value}u : {}", format_type(ty)),
        Expr::ConstFloat(value, ty) => format!("const {value:?} : {}", format_type(ty)),
        Expr::ConstBool(value) => format!("const {value} : bool"),
        Expr::ConstNull(ty) => format!("null : {}", format_type(ty)),
        Expr::RodataPtr(id) => format!("rodata_ptr #{id}"),
        Expr::RodataLen(id) => format!("rodata_len #{id}"),
        Expr::Local(id) => format!("%{id}"),
        Expr::AddrOfLocal(id) => format!("addr %{id}"),
        Expr::GlobalAddr(id) => format!("global_addr #{id}"),
        Expr::FnAddr(id) => format!("fn_addr #{id}"),
        Expr::SizeOf(ty) => format!("sizeof({})", format_type(ty)),
        Expr::AlignOf(ty) => format!("alignof({})", format_type(ty)),
        Expr::Load { ptr, ty } => {
            format!("load[{}] {}", format_type(ty), format_expr(ptr))
        }
        Expr::FieldAddr {
            base_ptr,
            struct_id,
            field_idx,
        } => format!(
            "field_addr struct#{}.{field_idx} {}",
            struct_id.0,
            format_expr(base_ptr)
        ),
        Expr::UnionFieldAddr {
            base_ptr,
            union_id,
            field_idx,
        } => format!(
            "union_field_addr union#{}.{field_idx} {}",
            union_id.0,
            format_expr(base_ptr)
        ),
        Expr::IndexAddr {
            base_ptr,
            elem_ty,
            index,
        } => format!(
            "index_addr[{}] {}, {}",
            format_type(elem_ty),
            format_expr(base_ptr),
            format_expr(index)
        ),
        Expr::Unary { op, val } => {
            let op = match op {
                UnOp::Neg => "neg",
                UnOp::BitNot => "bitnot",
                UnOp::LogNot => "lognot",
            };
            format!("{op}({})", format_expr(val))
        }
        Expr::Binary { op, lhs, rhs } => {
            let op = match op {
                BinOp::Add => "add",
                BinOp::Sub => "sub",
                BinOp::Mul => "mul",
                BinOp::Div => "div",
                BinOp::Mod => "mod",
                BinOp::BitAnd => "and",
                BinOp::BitOr => "or",
                BinOp::BitXor => "xor",
                BinOp::Shl => "shl",
                BinOp::Shr => "shr",
                BinOp::Eq => "eq",
                BinOp::Ne => "ne",
                BinOp::Lt => "lt",
                BinOp::Le => "le",
                BinOp::Gt => "gt",
                BinOp::Ge => "ge",
                BinOp::LogAnd => "logand",
                BinOp::LogOr => "logor",
            };
            format!("{op}({}, {})", format_expr(lhs), format_expr(rhs))
        }
        Expr::Cast { val, to } => format!("cast[{}]({})", format_type(to), format_expr(val)),
        Expr::Call { target, args } => {
            let target = match target {
                CallTarget::Direct(id) => format!("#{id}"),
                CallTarget::Indirect(expr) => format!("*{}", format_expr(expr)),
            };
            let args = args.iter().map(format_expr).collect::<Vec<_>>().join(", ");
            format!("call {target}({args})")
        }
        Expr::Syscall { num, args } => {
            let args = args.iter().map(format_expr).collect::<Vec<_>>().join(", ");
            format!("syscall({}, {args})", format_expr(num))
        }
    }
}
