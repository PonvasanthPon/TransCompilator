//! Monomorphized typed AST lowering into verified Core-7 IR.

use std::collections::HashMap;

use tc_ir::{
    CallTarget, Expr as IrExpr, FnId, Function, GlobalDef, GlobalId, LocalDef, LocalId, Module,
    RodataDef, RodataId, Stmt as IrStmt, StructDef, StructId, Type as IrType, UnionDef, UnionId,
    verify_module,
};

use crate::ast::{Span, TypeError, TypeErrorKind};
use crate::check::{TExpr, TExprKind, TStmt};
use crate::mono::{FnKey, MonoFn, MonoProgram, MonoStruct, MonoUnion};
use crate::resolve::ConcreteType;

/// Lowers a monomorphized program into a verified Core-7 module.
pub(crate) fn lower_program(mono: &MonoProgram) -> Result<Module, Vec<TypeError>> {
    let mut errors = Vec::new();

    let mut structs = Vec::with_capacity(mono.structs.len());
    for item in &mono.structs {
        match lower_struct(mono, item) {
            Ok(def) => structs.push(def),
            Err(error) => errors.push(error),
        }
    }
    let mut unions = Vec::with_capacity(mono.unions.len());
    for item in &mono.unions {
        match lower_union(mono, item) {
            Ok(def) => unions.push(def),
            Err(error) => errors.push(error),
        }
    }

    let mut globals = Vec::with_capacity(mono.globals.len());
    for global in &mono.globals {
        match convert_type(mono, &global.ty, &global.span) {
            Ok(ty) => globals.push(GlobalDef {
                name: global.name.clone(),
                ty,
                init: global.init.clone(),
            }),
            Err(error) => errors.push(error),
        }
    }

    let mut rodata = RodataTable::default();
    let mut functions = Vec::with_capacity(mono.functions.len());
    for function in &mono.functions {
        match lower_function(mono, function, &mut rodata) {
            Ok(lowered) => functions.push(lowered),
            Err(error) => errors.push(error),
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let module = Module {
        structs,
        unions,
        rodata: rodata.defs,
        globals,
        functions,
        entry: FnId(mono.entry),
    };
    if let Err(verifier_errors) = verify_module(&module) {
        let span = mono
            .functions
            .first()
            .map_or_else(Span::default, |function| function.span.clone());
        return Err(verifier_errors
            .into_iter()
            .map(|error| {
                TypeError::new(
                    span.clone(),
                    TypeErrorKind::Internal,
                    format!("lowered IR failed verification: {error:?}"),
                )
            })
            .collect());
    }
    Ok(module)
}

fn lower_struct(mono: &MonoProgram, item: &MonoStruct) -> Result<StructDef, TypeError> {
    let mut fields = Vec::with_capacity(item.fields.len());
    for (_, ty) in &item.fields {
        fields.push(convert_type(mono, ty, &Span::default())?);
    }
    Ok(StructDef {
        name: item.name.clone(),
        fields,
    })
}

fn lower_union(mono: &MonoProgram, item: &MonoUnion) -> Result<UnionDef, TypeError> {
    let mut fields = Vec::with_capacity(item.fields.len());
    for (_, ty) in &item.fields {
        fields.push(convert_type(mono, ty, &Span::default())?);
    }
    Ok(UnionDef {
        name: item.name.clone(),
        fields,
    })
}

fn lower_function(
    mono: &MonoProgram,
    function: &MonoFn,
    rodata: &mut RodataTable,
) -> Result<Function, TypeError> {
    let ret_aggregate = function.ret.is_aggregate();
    let ir_ret_high = convert_type(mono, &function.ret, &function.span)?;
    let ir_ret = if ret_aggregate {
        IrType::Void
    } else {
        ir_ret_high.clone()
    };

    let mut ir_params = Vec::with_capacity(function.params.len() + 1);
    let sret_param = if ret_aggregate {
        ir_params.push(LocalDef {
            name: "__sret".to_string(),
            ty: IrType::Ptr(Box::new(ir_ret_high)),
        });
        Some(LocalId(0))
    } else {
        None
    };
    let first_param = ir_params.len();
    for (name, ty) in &function.params {
        let ir_ty = convert_type(mono, ty, &function.span)?;
        let abi_ty = if ty.is_aggregate() {
            IrType::Ptr(Box::new(ir_ty))
        } else {
            ir_ty
        };
        ir_params.push(LocalDef {
            name: name.clone(),
            ty: abi_ty,
        });
    }

    let mut lowerer = FnLowerer {
        mono,
        rodata,
        param_count: ir_params.len(),
        locals: Vec::new(),
        stmts: Vec::new(),
        scopes: vec![HashMap::new()],
        sret: sret_param,
        ret: function.ret.clone(),
        temp_counter: 0,
    };

    for (index, (name, ty)) in function.params.iter().enumerate() {
        let value_local = lowerer.new_local(name.clone(), ty.clone(), &function.span)?;
        let parameter = LocalId(first_param + index);
        if ty.is_aggregate() {
            let ir_ty = convert_type(mono, ty, &function.span)?;
            lowerer.emit(IrStmt::MemCopy {
                dst: IrExpr::AddrOfLocal(value_local),
                src: IrExpr::Local(parameter),
                size: IrExpr::SizeOf(ir_ty),
            });
        } else {
            lowerer.emit(IrStmt::Assign {
                local: value_local,
                val: IrExpr::Local(parameter),
            });
        }
        lowerer.declare(name.clone(), value_local, ty.clone());
    }

    lowerer.lower_stmts(&function.body, &function.span)?;

    Ok(Function {
        name: function.name.clone(),
        params: ir_params,
        ret: ir_ret,
        locals: lowerer.locals,
        body: lowerer.stmts,
    })
}

#[derive(Default)]
struct RodataTable {
    defs: Vec<RodataDef>,
    ids: HashMap<Vec<u8>, RodataId>,
}

impl RodataTable {
    fn intern(&mut self, bytes: &[u8]) -> RodataId {
        if let Some(id) = self.ids.get(bytes) {
            return *id;
        }
        let id = RodataId(self.defs.len());
        self.defs.push(RodataDef {
            name: format!("rodata_{}", id.0),
            bytes: bytes.to_vec(),
        });
        self.ids.insert(bytes.to_vec(), id);
        id
    }
}

#[derive(Debug)]
enum Lowered {
    Unit,
    Scalar(ScalarValue),
    Place { local: LocalId },
}

#[derive(Debug)]
struct ScalarValue {
    ty: ConcreteType,
    expr: IrExpr,
}

struct FnLowerer<'a> {
    mono: &'a MonoProgram,
    rodata: &'a mut RodataTable,
    param_count: usize,
    locals: Vec<LocalDef>,
    stmts: Vec<IrStmt>,
    scopes: Vec<HashMap<String, (LocalId, ConcreteType)>>,
    sret: Option<LocalId>,
    ret: ConcreteType,
    temp_counter: usize,
}

impl FnLowerer<'_> {
    fn new_local(
        &mut self,
        name: String,
        ty: ConcreteType,
        span: &Span,
    ) -> Result<LocalId, TypeError> {
        let ir_ty = convert_type(self.mono, &ty, span)?;
        let id = LocalId(self.param_count + self.locals.len());
        self.locals.push(LocalDef { name, ty: ir_ty });
        Ok(id)
    }

    fn new_temp(&mut self, ty: ConcreteType, span: &Span) -> Result<LocalId, TypeError> {
        let name = format!("__tmp{}", self.temp_counter);
        self.temp_counter += 1;
        self.new_local(name, ty, span)
    }

    fn emit(&mut self, stmt: IrStmt) {
        self.stmts.push(stmt);
    }

    fn declare(&mut self, name: String, local: LocalId, ty: ConcreteType) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, (local, ty));
        }
    }

    fn lookup(&self, name: &str) -> Option<(LocalId, ConcreteType)> {
        for scope in self.scopes.iter().rev() {
            if let Some((local, ty)) = scope.get(name) {
                return Some((*local, ty.clone()));
            }
        }
        None
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn lower_stmts(&mut self, stmts: &[TStmt], span: &Span) -> Result<(), TypeError> {
        for stmt in stmts {
            self.lower_stmt(stmt, span)?;
        }
        Ok(())
    }

    fn lower_branch(&mut self, stmts: &[TStmt], span: &Span) -> Result<Vec<IrStmt>, TypeError> {
        let saved = std::mem::take(&mut self.stmts);
        self.push_scope();
        let result = self.lower_stmts(stmts, span);
        self.pop_scope();
        let body = std::mem::replace(&mut self.stmts, saved);
        result?;
        Ok(body)
    }

    fn lower_branch_expr(
        &mut self,
        expr: &TExpr,
        span: &Span,
    ) -> Result<(Vec<IrStmt>, ScalarValue), TypeError> {
        let saved = std::mem::take(&mut self.stmts);
        let value = self.lower_expr(expr, span);
        let body = std::mem::replace(&mut self.stmts, saved);
        let value = self.expect_scalar(value?, span)?;
        Ok((body, value))
    }

    fn lower_stmt(&mut self, stmt: &TStmt, _span: &Span) -> Result<(), TypeError> {
        match stmt {
            TStmt::Let {
                name,
                ty,
                init,
                span,
            } => {
                let value = self.lower_expr(init, span)?;
                let local = self.new_local(name.clone(), ty.clone(), span)?;
                self.copy_into_local(local, ty, value, span)?;
                self.declare(name.clone(), local, ty.clone());
                Ok(())
            }
            TStmt::Assign { name, val, span } => {
                let Some((local, ty)) = self.lookup(name) else {
                    return Err(internal(
                        span,
                        format!("variable `{name}` vanished during lowering"),
                    ));
                };
                let value = self.lower_expr(val, span)?;
                self.copy_into_local(local, &ty, value, span)
            }
            TStmt::Store { ptr, val, span } => {
                let ptr = self.lower_scalar(ptr, span)?;
                let pointee = ptr
                    .ty
                    .pointee()
                    .cloned()
                    .ok_or_else(|| internal(span, "store destination is not a pointer"))?;
                let value = self.lower_expr(val, span)?;
                if pointee.is_aggregate() {
                    let place = self.expect_place(value, span)?;
                    let ir_ty = convert_type(self.mono, &pointee, span)?;
                    self.emit(IrStmt::MemCopy {
                        dst: ptr.expr,
                        src: IrExpr::AddrOfLocal(place),
                        size: IrExpr::SizeOf(ir_ty),
                    });
                } else {
                    let value = self.expect_scalar(value, span)?;
                    let ty = convert_type(self.mono, &pointee, span)?;
                    self.emit(IrStmt::Store {
                        ptr: ptr.expr,
                        ty,
                        val: value.expr,
                    });
                }
                Ok(())
            }
            TStmt::MemCopy {
                dst,
                src,
                size,
                span,
            } => {
                let dst = self.lower_scalar(dst, span)?;
                let src = self.lower_scalar(src, span)?;
                let size = self.lower_scalar(size, span)?;
                self.emit(IrStmt::MemCopy {
                    dst: dst.expr,
                    src: src.expr,
                    size: size.expr,
                });
                Ok(())
            }
            TStmt::Expr(expr, span) => {
                self.lower_expr(expr, span)?;
                Ok(())
            }
            TStmt::Block(body, span) => {
                self.push_scope();
                let result = self.lower_stmts(body, span);
                self.pop_scope();
                result
            }
            TStmt::If {
                cond,
                then_body,
                else_body,
                span,
            } => {
                let cond = self.lower_scalar(cond, span)?;
                let then_body = self.lower_branch(then_body, span)?;
                let else_body = self.lower_branch(else_body, span)?;
                self.emit(IrStmt::If {
                    cond: cond.expr,
                    then_body,
                    else_body,
                });
                Ok(())
            }
            TStmt::Loop { body, span } => {
                let body = self.lower_branch(body, span)?;
                self.emit(IrStmt::Loop { body });
                Ok(())
            }
            TStmt::Break(span) => {
                self.emit(IrStmt::Break);
                let _ = span;
                Ok(())
            }
            TStmt::Continue(span) => {
                self.emit(IrStmt::Continue);
                let _ = span;
                Ok(())
            }
            TStmt::Return(value, span) => {
                match value {
                    None => self.emit(IrStmt::Return(None)),
                    Some(value) => {
                        let value = self.lower_expr(value, span)?;
                        if self.ret.is_aggregate() {
                            let place = self.expect_place(value, span)?;
                            let sret = self
                                .sret
                                .ok_or_else(|| internal(span, "missing sret parameter"))?;
                            let ir_ty = convert_type(self.mono, &self.ret, span)?;
                            self.emit(IrStmt::MemCopy {
                                dst: IrExpr::Local(sret),
                                src: IrExpr::AddrOfLocal(place),
                                size: IrExpr::SizeOf(ir_ty),
                            });
                            self.emit(IrStmt::Return(None));
                        } else {
                            let value = self.expect_scalar(value, span)?;
                            self.emit(IrStmt::Return(Some(value.expr)));
                        }
                    }
                }
                Ok(())
            }
            TStmt::Trap(_) => {
                self.emit(IrStmt::Trap);
                Ok(())
            }
        }
    }

    fn copy_into_local(
        &mut self,
        local: LocalId,
        ty: &ConcreteType,
        value: Lowered,
        span: &Span,
    ) -> Result<(), TypeError> {
        if ty.is_aggregate() {
            let place = self.expect_place(value, span)?;
            let ir_ty = convert_type(self.mono, ty, span)?;
            self.emit(IrStmt::MemCopy {
                dst: IrExpr::AddrOfLocal(local),
                src: IrExpr::AddrOfLocal(place),
                size: IrExpr::SizeOf(ir_ty),
            });
        } else {
            let value = self.expect_scalar(value, span)?;
            self.emit(IrStmt::Assign {
                local,
                val: value.expr,
            });
        }
        Ok(())
    }

    fn lower_scalar(&mut self, expr: &TExpr, span: &Span) -> Result<ScalarValue, TypeError> {
        let value = self.lower_expr(expr, span)?;
        self.expect_scalar(value, span)
    }

    fn expect_scalar(&self, value: Lowered, span: &Span) -> Result<ScalarValue, TypeError> {
        match value {
            Lowered::Scalar(value) => Ok(value),
            Lowered::Place { .. } | Lowered::Unit => {
                Err(internal(span, "expected a scalar value during lowering"))
            }
        }
    }

    fn expect_place(&self, value: Lowered, span: &Span) -> Result<LocalId, TypeError> {
        match value {
            Lowered::Place { local } => Ok(local),
            Lowered::Scalar { .. } | Lowered::Unit => Err(internal(
                span,
                "expected an aggregate value during lowering",
            )),
        }
    }

    fn materialize(
        &mut self,
        ty: &ConcreteType,
        expr: IrExpr,
        span: &Span,
    ) -> Result<Lowered, TypeError> {
        let local = self.new_temp(ty.clone(), span)?;
        self.emit(IrStmt::Assign { local, val: expr });
        Ok(Lowered::Scalar(ScalarValue {
            ty: ty.clone(),
            expr: IrExpr::Local(local),
        }))
    }

    fn lower_expr(&mut self, expr: &TExpr, span: &Span) -> Result<Lowered, TypeError> {
        match &expr.kind {
            TExprKind::ConstInt(value) => {
                let ty = convert_type(self.mono, &expr.ty, span)?;
                self.materialize(&expr.ty, IrExpr::ConstInt(*value, ty), span)
            }
            TExprKind::ConstUint(value) => {
                let ty = convert_type(self.mono, &expr.ty, span)?;
                self.materialize(&expr.ty, IrExpr::ConstUint(*value, ty), span)
            }
            TExprKind::ConstFloat(value) => {
                let ty = convert_type(self.mono, &expr.ty, span)?;
                self.materialize(&expr.ty, IrExpr::ConstFloat(*value, ty), span)
            }
            TExprKind::ConstBool(value) => {
                self.materialize(&expr.ty, IrExpr::ConstBool(*value), span)
            }
            TExprKind::ConstNull => {
                let ty = convert_type(self.mono, &expr.ty, span)?;
                self.materialize(&expr.ty, IrExpr::ConstNull(ty), span)
            }
            TExprKind::BytesLit(bytes) => {
                let id = self.rodata.intern(bytes);
                self.materialize(&expr.ty, IrExpr::RodataPtr(id), span)
            }
            TExprKind::BytesLen(bytes) => {
                let id = self.rodata.intern(bytes);
                self.materialize(&expr.ty, IrExpr::RodataLen(id), span)
            }
            TExprKind::Zero(ty) => {
                if ty.is_aggregate() {
                    let (size, _) = concrete_layout(self.mono, ty).ok_or_else(|| {
                        internal(span, "aggregate has no finite layout (recursive type?)")
                    })?;
                    let blob = vec![0_u8; size.max(1)];
                    let id = self.rodata.intern(&blob);
                    let temp = self.new_temp(ty.clone(), span)?;
                    let ir_ty = convert_type(self.mono, ty, span)?;
                    self.emit(IrStmt::MemCopy {
                        dst: IrExpr::AddrOfLocal(temp),
                        src: IrExpr::RodataPtr(id),
                        size: IrExpr::SizeOf(ir_ty),
                    });
                    Ok(Lowered::Place { local: temp })
                } else {
                    let ir_ty = convert_type(self.mono, ty, span)?;
                    let value = match ty {
                        ConcreteType::Bool => IrExpr::ConstBool(false),
                        ConcreteType::F32 | ConcreteType::F64 => IrExpr::ConstFloat(0.0, ir_ty),
                        ConcreteType::Ptr(_) => IrExpr::ConstNull(ir_ty),
                        ConcreteType::FnPtr { .. } => IrExpr::Cast {
                            val: Box::new(IrExpr::ConstUint(0, IrType::U64)),
                            to: ir_ty,
                        },
                        _ if ty.is_signed_int() => IrExpr::ConstInt(0, ir_ty),
                        _ => IrExpr::ConstUint(0, ir_ty),
                    };
                    self.materialize(ty, value, span)
                }
            }
            TExprKind::Var(name) => {
                let (local, ty) = self
                    .lookup(name)
                    .ok_or_else(|| internal(span, format!("variable `{name}` vanished")))?;
                if ty.is_aggregate() {
                    let temp = self.new_temp(ty.clone(), span)?;
                    let ir_ty = convert_type(self.mono, &ty, span)?;
                    self.emit(IrStmt::MemCopy {
                        dst: IrExpr::AddrOfLocal(temp),
                        src: IrExpr::AddrOfLocal(local),
                        size: IrExpr::SizeOf(ir_ty),
                    });
                    Ok(Lowered::Place { local: temp })
                } else {
                    self.materialize(&ty, IrExpr::Local(local), span)
                }
            }
            TExprKind::GlobalAddr { module, index } => {
                let id = *self
                    .mono
                    .global_ids
                    .get(&(*module, *index))
                    .ok_or_else(|| internal(span, "missing global during lowering"))?;
                self.materialize(&expr.ty, IrExpr::GlobalAddr(GlobalId(id)), span)
            }
            TExprKind::AddrOfVar(name) => {
                let (local, _) = self
                    .lookup(name)
                    .ok_or_else(|| internal(span, format!("variable `{name}` vanished")))?;
                self.materialize(&expr.ty, IrExpr::AddrOfLocal(local), span)
            }
            TExprKind::FieldAddr { base, field_idx } => {
                let base = self.lower_scalar(base, span)?;
                let pointee = base
                    .ty
                    .pointee()
                    .cloned()
                    .ok_or_else(|| internal(span, "field base is not a pointer"))?;
                let ir_expr = match &pointee {
                    ConcreteType::Struct { module, name, args } => {
                        let id = struct_id(self.mono, module, name, args.as_slice(), span)?;
                        IrExpr::FieldAddr {
                            base_ptr: Box::new(base.expr),
                            struct_id: StructId(id),
                            field_idx: *field_idx,
                        }
                    }
                    ConcreteType::Union { module, name, args } => {
                        let id = union_id(self.mono, module, name, args.as_slice(), span)?;
                        IrExpr::UnionFieldAddr {
                            base_ptr: Box::new(base.expr),
                            union_id: UnionId(id),
                            field_idx: *field_idx,
                        }
                    }
                    _ => {
                        return Err(internal(
                            span,
                            "field base is not a struct or union pointer",
                        ));
                    }
                };
                self.materialize(&expr.ty, ir_expr, span)
            }
            TExprKind::IndexAddr { base, index } => {
                let base = self.lower_scalar(base, span)?;
                let index = self.lower_scalar(index, span)?;
                let elem = expr
                    .ty
                    .pointee()
                    .cloned()
                    .ok_or_else(|| internal(span, "index result is not a pointer"))?;
                let ir_elem = convert_type(self.mono, &elem, span)?;
                let target = ConcreteType::Ptr(Box::new(elem.clone()));
                let base_expr = if base.ty == target {
                    base.expr
                } else {
                    IrExpr::Cast {
                        val: Box::new(base.expr),
                        to: IrType::Ptr(Box::new(ir_elem.clone())),
                    }
                };
                self.materialize(
                    &expr.ty,
                    IrExpr::IndexAddr {
                        base_ptr: Box::new(base_expr),
                        elem_ty: ir_elem,
                        index: Box::new(index.expr),
                    },
                    span,
                )
            }
            TExprKind::Load(ptr) => {
                let ptr = self.lower_scalar(ptr, span)?;
                if expr.ty.is_aggregate() {
                    let temp = self.new_temp(expr.ty.clone(), span)?;
                    let ir_ty = convert_type(self.mono, &expr.ty, span)?;
                    self.emit(IrStmt::MemCopy {
                        dst: IrExpr::AddrOfLocal(temp),
                        src: ptr.expr,
                        size: IrExpr::SizeOf(ir_ty),
                    });
                    Ok(Lowered::Place { local: temp })
                } else {
                    let ty = convert_type(self.mono, &expr.ty, span)?;
                    self.materialize(
                        &expr.ty,
                        IrExpr::Load {
                            ptr: Box::new(ptr.expr),
                            ty,
                        },
                        span,
                    )
                }
            }
            TExprKind::SizeOf(ty) => {
                let ir_ty = convert_type(self.mono, ty, span)?;
                self.materialize(&expr.ty, IrExpr::SizeOf(ir_ty), span)
            }
            TExprKind::AlignOf(ty) => {
                let ir_ty = convert_type(self.mono, ty, span)?;
                self.materialize(&expr.ty, IrExpr::AlignOf(ir_ty), span)
            }
            TExprKind::Unary { op, val } => {
                let val = self.lower_scalar(val, span)?;
                self.materialize(
                    &expr.ty,
                    IrExpr::Unary {
                        op: *op,
                        val: Box::new(val.expr),
                    },
                    span,
                )
            }
            TExprKind::Binary { op, lhs, rhs } => {
                if matches!(op, tc_ir::BinOp::LogAnd | tc_ir::BinOp::LogOr) {
                    let lhs = self.lower_scalar(lhs, span)?;
                    let result = self.new_temp(ConcreteType::Bool, span)?;
                    self.emit(IrStmt::Assign {
                        local: result,
                        val: lhs.expr.clone(),
                    });
                    let (mut rhs_body, rhs_value) = self.lower_branch_expr(rhs, span)?;
                    rhs_body.push(IrStmt::Assign {
                        local: result,
                        val: rhs_value.expr,
                    });
                    let (then_body, else_body) = if *op == tc_ir::BinOp::LogAnd {
                        (rhs_body, Vec::new())
                    } else {
                        (Vec::new(), rhs_body)
                    };
                    self.emit(IrStmt::If {
                        cond: lhs.expr,
                        then_body,
                        else_body,
                    });
                    Ok(Lowered::Scalar(ScalarValue {
                        ty: ConcreteType::Bool,
                        expr: IrExpr::Local(result),
                    }))
                } else {
                    let lhs = self.lower_scalar(lhs, span)?;
                    let rhs = self.lower_scalar(rhs, span)?;
                    self.materialize(
                        &expr.ty,
                        IrExpr::Binary {
                            op: *op,
                            lhs: Box::new(lhs.expr),
                            rhs: Box::new(rhs.expr),
                        },
                        span,
                    )
                }
            }
            TExprKind::Cast { val } => {
                let val = self.lower_scalar(val, span)?;
                let to = convert_type(self.mono, &expr.ty, span)?;
                self.materialize(
                    &expr.ty,
                    IrExpr::Cast {
                        val: Box::new(val.expr),
                        to,
                    },
                    span,
                )
            }
            TExprKind::FnAddr {
                module,
                index,
                type_args,
            } => {
                let key = FnKey {
                    module: *module,
                    index: *index,
                    type_args: type_args.clone(),
                };
                let id = *self
                    .mono
                    .fn_ids
                    .get(&key)
                    .ok_or_else(|| internal(span, "missing function specialization"))?;
                self.materialize(&expr.ty, IrExpr::FnAddr(FnId(id)), span)
            }
            TExprKind::Call {
                module,
                index,
                type_args,
                args,
            } => self.lower_direct_call(*module, *index, type_args, args, &expr.ty, span),
            TExprKind::IndirectCall { callee, args } => {
                self.lower_indirect_call(callee, args, &expr.ty, span)
            }
            TExprKind::Syscall { num, args } => {
                let num = self.lower_scalar(num, span)?;
                let mut ir_args = Vec::with_capacity(args.len());
                for arg in args {
                    ir_args.push(self.lower_scalar(arg, span)?.expr);
                }
                self.materialize(
                    &expr.ty,
                    IrExpr::Syscall {
                        num: Box::new(num.expr),
                        args: ir_args,
                    },
                    span,
                )
            }
        }
    }

    fn lower_direct_call(
        &mut self,
        module: usize,
        index: usize,
        type_args: &[ConcreteType],
        args: &[TExpr],
        ret_ty: &ConcreteType,
        span: &Span,
    ) -> Result<Lowered, TypeError> {
        let key = FnKey {
            module,
            index,
            type_args: type_args.to_vec(),
        };
        let id = *self
            .mono
            .fn_ids
            .get(&key)
            .ok_or_else(|| internal(span, "missing function specialization"))?;
        self.lower_call_with_target(CallTarget::Direct(FnId(id)), args, ret_ty, span)
    }

    fn lower_indirect_call(
        &mut self,
        callee: &TExpr,
        args: &[TExpr],
        ret_ty: &ConcreteType,
        span: &Span,
    ) -> Result<Lowered, TypeError> {
        let callee = self.lower_scalar(callee, span)?;
        self.lower_call_with_target(
            CallTarget::Indirect(Box::new(callee.expr)),
            args,
            ret_ty,
            span,
        )
    }

    fn lower_call_with_target(
        &mut self,
        target: CallTarget,
        args: &[TExpr],
        ret_ty: &ConcreteType,
        span: &Span,
    ) -> Result<Lowered, TypeError> {
        let evaluated = args
            .iter()
            .map(|arg| self.lower_expr(arg, span))
            .collect::<Result<Vec<_>, _>>()?;
        let ret_aggregate = ret_ty.is_aggregate();
        let mut ir_args = Vec::with_capacity(evaluated.len() + usize::from(ret_aggregate));
        let result_local = if ret_aggregate {
            let local = self.new_temp(ret_ty.clone(), span)?;
            ir_args.push(IrExpr::AddrOfLocal(local));
            Some(local)
        } else {
            None
        };
        for (value, arg) in evaluated.into_iter().zip(args) {
            if arg.ty.is_aggregate() {
                ir_args.push(IrExpr::AddrOfLocal(self.expect_place(value, span)?));
            } else {
                ir_args.push(self.expect_scalar(value, span)?.expr);
            }
        }
        let call = IrExpr::Call {
            target,
            args: ir_args,
        };
        if let Some(local) = result_local {
            self.emit(IrStmt::Expr(call));
            Ok(Lowered::Place { local })
        } else if *ret_ty == ConcreteType::Void {
            self.emit(IrStmt::Expr(call));
            Ok(Lowered::Unit)
        } else {
            self.materialize(ret_ty, call, span)
        }
    }
}

fn convert_type(mono: &MonoProgram, ty: &ConcreteType, span: &Span) -> Result<IrType, TypeError> {
    match ty {
        ConcreteType::Void => Ok(IrType::Void),
        ConcreteType::Bool => Ok(IrType::Bool),
        ConcreteType::I8 => Ok(IrType::I8),
        ConcreteType::I16 => Ok(IrType::I16),
        ConcreteType::I32 => Ok(IrType::I32),
        ConcreteType::I64 => Ok(IrType::I64),
        ConcreteType::U8 => Ok(IrType::U8),
        ConcreteType::U16 => Ok(IrType::U16),
        ConcreteType::U32 => Ok(IrType::U32),
        ConcreteType::U64 => Ok(IrType::U64),
        ConcreteType::F32 => Ok(IrType::F32),
        ConcreteType::F64 => Ok(IrType::F64),
        ConcreteType::Ptr(inner) => Ok(IrType::Ptr(Box::new(convert_type(mono, inner, span)?))),
        ConcreteType::Array(inner, count) => Ok(IrType::Array(
            Box::new(convert_type(mono, inner, span)?),
            *count,
        )),
        ConcreteType::Struct { module, name, args } => {
            let id = struct_id(mono, module, name, args, span)?;
            Ok(IrType::Struct(StructId(id)))
        }
        ConcreteType::Union { module, name, args } => {
            let id = union_id(mono, module, name, args, span)?;
            Ok(IrType::Union(UnionId(id)))
        }
        ConcreteType::FnPtr { params, ret } => {
            let aggregate_ret = ret.is_aggregate();
            let ir_ret_high = convert_type(mono, ret, span)?;
            let mut ir_params = Vec::with_capacity(params.len() + usize::from(aggregate_ret));
            if aggregate_ret {
                ir_params.push(IrType::Ptr(Box::new(ir_ret_high.clone())));
            }
            for param in params {
                let ir_param = convert_type(mono, param, span)?;
                ir_params.push(if param.is_aggregate() {
                    IrType::Ptr(Box::new(ir_param))
                } else {
                    ir_param
                });
            }
            Ok(IrType::FnPtr {
                params: ir_params,
                ret: Box::new(if aggregate_ret {
                    IrType::Void
                } else {
                    ir_ret_high
                }),
            })
        }
        ConcreteType::Generic(name) => Err(internal(
            span,
            format!("unmonomorphized generic type `{name}` reached lowering"),
        )),
    }
}

fn struct_id(
    mono: &MonoProgram,
    module: &str,
    name: &str,
    args: &[ConcreteType],
    span: &Span,
) -> Result<usize, TypeError> {
    let module_index = *mono
        .module_by_name
        .get(module)
        .ok_or_else(|| internal(span, format!("unknown module `{module}`")))?;
    mono.structs
        .iter()
        .position(|item| {
            item.key.module == module_index
                && item.source_name == name
                && item.key.type_args.as_slice() == args
        })
        .ok_or_else(|| {
            internal(
                span,
                format!("monomorphized struct `{module}::{name}` not found"),
            )
        })
}

fn union_id(
    mono: &MonoProgram,
    module: &str,
    name: &str,
    args: &[ConcreteType],
    span: &Span,
) -> Result<usize, TypeError> {
    let module_index = *mono
        .module_by_name
        .get(module)
        .ok_or_else(|| internal(span, format!("unknown module `{module}`")))?;
    mono.unions
        .iter()
        .position(|item| {
            item.key.module == module_index
                && item.source_name == name
                && item.key.type_args.as_slice() == args
        })
        .ok_or_else(|| {
            internal(
                span,
                format!("monomorphized union `{module}::{name}` not found"),
            )
        })
}

fn concrete_layout(mono: &MonoProgram, ty: &ConcreteType) -> Option<(usize, usize)> {
    concrete_layout_depth(mono, ty, 0)
}

fn concrete_layout_depth(
    mono: &MonoProgram,
    ty: &ConcreteType,
    depth: usize,
) -> Option<(usize, usize)> {
    if depth > 64 {
        return None;
    }
    Some(match ty {
        ConcreteType::Void => (1, 1),
        ConcreteType::Bool | ConcreteType::U8 | ConcreteType::I8 => (1, 1),
        ConcreteType::U16 | ConcreteType::I16 => (2, 2),
        ConcreteType::U32 | ConcreteType::I32 | ConcreteType::F32 => (4, 4),
        ConcreteType::U64
        | ConcreteType::I64
        | ConcreteType::F64
        | ConcreteType::Ptr(_)
        | ConcreteType::FnPtr { .. } => (8, 8),
        ConcreteType::Array(element, count) => {
            let (size, align) = concrete_layout_depth(mono, element, depth + 1)?;
            (size.checked_mul(*count)?, align)
        }
        ConcreteType::Struct { module, name, args } => {
            let module_index = *mono.module_by_name.get(module)?;
            let item = mono.structs.iter().find(|item| {
                item.key.module == module_index
                    && item.source_name == *name
                    && item.key.type_args == *args
            })?;
            let mut offset = 0_usize;
            let mut align = 1_usize;
            for (_, field) in &item.fields {
                let (field_size, field_align) = concrete_layout_depth(mono, field, depth + 1)?;
                offset = round_up(offset, field_align).checked_add(field_size)?;
                align = align.max(field_align);
            }
            (round_up(offset, align).max(1), align)
        }
        ConcreteType::Union { module, name, args } => {
            let module_index = *mono.module_by_name.get(module)?;
            let item = mono.unions.iter().find(|item| {
                item.key.module == module_index
                    && item.source_name == *name
                    && item.key.type_args == *args
            })?;
            let mut size = 0_usize;
            let mut align = 1_usize;
            for (_, field) in &item.fields {
                let (field_size, field_align) = concrete_layout_depth(mono, field, depth + 1)?;
                size = size.max(field_size);
                align = align.max(field_align);
            }
            (round_up(size, align).max(1), align)
        }
        ConcreteType::Generic(_) => return None,
    })
}

fn round_up(value: usize, align: usize) -> usize {
    value.div_ceil(align.max(1)) * align.max(1)
}

fn internal(span: &Span, message: impl Into<String>) -> TypeError {
    TypeError::new(span.clone(), TypeErrorKind::Internal, message)
}
