//! Direct Intel-syntax x86_64 assembly emission.

use std::fmt::Write as _;

use tc_ir::{BinOp, CallTarget, Expr, Function, Module, Stmt, Type, UnOp, infer_expr_type};

use crate::CodegenError;

const MAX_SCRATCH_SLOTS: usize = 64;
const MAX_TEMP_SLOTS: usize = 64;

const INT_ARG_REGS: [&str; 6] = ["rdi", "rsi", "rdx", "rcx", "r8", "r9"];
const SSE_ARG_REGS: [&str; 8] = [
    "xmm0", "xmm1", "xmm2", "xmm3", "xmm4", "xmm5", "xmm6", "xmm7",
];

/// Emits a full assembly translation unit for a verified module.
pub(crate) fn emit_module(module: &Module) -> Result<String, CodegenError> {
    let mut emitter = Emitter {
        module,
        out: String::new(),
        next_label: 0,
    };
    emitter.emit()
}

struct Emitter<'a> {
    module: &'a Module,
    out: String,
    next_label: usize,
}

impl Emitter<'_> {
    fn emit(&mut self) -> Result<String, CodegenError> {
        writeln!(&mut self.out, "    .intel_syntax noprefix")
            .expect("writing to String cannot fail");

        self.emit_rodata();
        self.emit_globals();
        self.emit_text()?;
        writeln!(&mut self.out, "    .section .note.GNU-stack,\"\",@progbits")
            .expect("writing to String cannot fail");
        Ok(std::mem::take(&mut self.out))
    }

    fn emit_rodata(&mut self) {
        if self.module.rodata.is_empty() {
            return;
        }
        writeln!(&mut self.out, "    .section .rodata").expect("writing to String cannot fail");
        for (index, blob) in self.module.rodata.iter().enumerate() {
            writeln!(&mut self.out, "tc_rodata_{index}:").expect("writing to String cannot fail");
            emit_bytes(&mut self.out, &blob.bytes);
        }
    }

    fn emit_globals(&mut self) {
        let initialized = self
            .module
            .globals
            .iter()
            .enumerate()
            .filter(|(_, global)| global.init.is_some())
            .collect::<Vec<_>>();
        if !initialized.is_empty() {
            writeln!(&mut self.out, "    .section .data").expect("writing to String cannot fail");
            for (index, global) in initialized {
                let align = self.module.align_of(&global.ty).max(1);
                writeln!(&mut self.out, "    .balign {align}")
                    .expect("writing to String cannot fail");
                writeln!(&mut self.out, "tc_global_{index}:")
                    .expect("writing to String cannot fail");
                if let Some(bytes) = &global.init {
                    if bytes.is_empty() {
                        writeln!(&mut self.out, "    .byte 0")
                            .expect("writing to String cannot fail");
                    } else {
                        emit_bytes(&mut self.out, bytes);
                    }
                }
            }
        }

        let zeroed = self
            .module
            .globals
            .iter()
            .enumerate()
            .filter(|(_, global)| global.init.is_none())
            .collect::<Vec<_>>();
        if !zeroed.is_empty() {
            writeln!(&mut self.out, "    .section .bss").expect("writing to String cannot fail");
            for (index, global) in zeroed {
                let align = self.module.align_of(&global.ty).max(1);
                writeln!(&mut self.out, "    .balign {align}")
                    .expect("writing to String cannot fail");
                writeln!(&mut self.out, "tc_global_{index}:")
                    .expect("writing to String cannot fail");
                writeln!(
                    &mut self.out,
                    "    .zero {}",
                    self.module.size_of(&global.ty).max(1)
                )
                .expect("writing to String cannot fail");
            }
        }
    }

    fn emit_text(&mut self) -> Result<(), CodegenError> {
        writeln!(&mut self.out, "    .section .text").expect("writing to String cannot fail");
        self.emit_start();
        for index in 0..self.module.functions.len() {
            let mut function_emitter =
                FunctionEmitter::new(self.module, index, &mut self.out, &mut self.next_label)?;
            function_emitter.emit()?;
        }
        Ok(())
    }

    fn emit_start(&mut self) {
        let entry = &self.module.functions[self.module.entry.0];
        writeln!(&mut self.out, "    .globl _start").expect("writing to String cannot fail");
        writeln!(&mut self.out, "_start:").expect("writing to String cannot fail");
        if !entry.params.is_empty() {
            writeln!(&mut self.out, "    mov rax, qword ptr [rsp]")
                .expect("writing to String cannot fail");
            writeln!(&mut self.out, "    lea rsi, [rsp + 8]")
                .expect("writing to String cannot fail");
            writeln!(&mut self.out, "    lea rdx, [rsi + rax*8 + 8]")
                .expect("writing to String cannot fail");
            writeln!(&mut self.out, "    mov rdi, rax").expect("writing to String cannot fail");
        }
        writeln!(&mut self.out, "    and rsp, -16").expect("writing to String cannot fail");
        writeln!(&mut self.out, "    call tc_fn_{}", self.module.entry.0)
            .expect("writing to String cannot fail");
        match entry.ret {
            Type::Void => {
                writeln!(&mut self.out, "    xor edi, edi").expect("writing to String cannot fail");
            }
            Type::I32 => {
                writeln!(&mut self.out, "    mov edi, eax").expect("writing to String cannot fail");
            }
            _ => {
                writeln!(&mut self.out, "    mov rdi, rax").expect("writing to String cannot fail");
            }
        }
        writeln!(&mut self.out, "    mov eax, 60").expect("writing to String cannot fail");
        writeln!(&mut self.out, "    syscall").expect("writing to String cannot fail");
    }
}

struct FunctionEmitter<'a, 'b> {
    module: &'a Module,
    function: &'a Function,
    function_id: usize,
    out: &'b mut String,
    next_label: &'b mut usize,
    frame_size: usize,
    local_offsets: Vec<usize>,
    scratch_base: usize,
    temp_base: usize,
    scratch_next: usize,
    temp_next: usize,
    loop_stack: Vec<(String, String)>,
    epilogue: String,
}

impl<'a, 'b> FunctionEmitter<'a, 'b> {
    fn new(
        module: &'a Module,
        function_id: usize,
        out: &'b mut String,
        next_label: &'b mut usize,
    ) -> Result<Self, CodegenError> {
        let function = &module.functions[function_id];
        let layout = module.local_layout(function);
        let local_offsets = layout.slots.iter().map(|slot| slot.offset).collect();
        let locals_end = round_up(layout.size, 8);
        let scratch_base = locals_end;
        let temp_base = scratch_base + MAX_SCRATCH_SLOTS * 8;
        let frame_size = round_up(temp_base + MAX_TEMP_SLOTS * 8, 16);
        Ok(Self {
            module,
            function,
            function_id,
            out,
            next_label,
            frame_size,
            local_offsets,
            scratch_base,
            temp_base,
            scratch_next: 0,
            temp_next: 0,
            loop_stack: Vec::new(),
            epilogue: format!(".Lfn_epilogue_{function_id}"),
        })
    }

    fn label(&mut self, prefix: &str) -> String {
        let label = format!(".L{prefix}_{}", *self.next_label);
        *self.next_label += 1;
        label
    }

    fn local_disp(&self, index: usize) -> usize {
        self.frame_size - self.local_offsets[index]
    }

    fn scratch_disp(&self, index: usize) -> usize {
        self.frame_size - (self.scratch_base + index * 8)
    }

    fn temp_disp(&self, index: usize) -> usize {
        self.frame_size - (self.temp_base + index * 8)
    }

    fn alloc_scratch(&mut self, count: usize) -> Result<usize, CodegenError> {
        let base = self.scratch_next;
        let next = base + count;
        if next > MAX_SCRATCH_SLOTS {
            return Err(CodegenError::Emit("scratch slot pool exhausted".into()));
        }
        self.scratch_next = next;
        Ok(base)
    }

    fn free_scratch(&mut self, base: usize) {
        self.scratch_next = base;
    }

    fn alloc_temp(&mut self) -> Result<usize, CodegenError> {
        let slot = self.temp_next;
        if slot >= MAX_TEMP_SLOTS {
            return Err(CodegenError::Emit("temporary slot pool exhausted".into()));
        }
        self.temp_next += 1;
        Ok(slot)
    }

    fn free_temp(&mut self, slot: usize) {
        self.temp_next = slot;
    }

    fn emit(&mut self) -> Result<(), CodegenError> {
        writeln!(self.out, "    .type tc_fn_{}, @function", self.function_id)
            .expect("writing to String cannot fail");
        writeln!(self.out, "tc_fn_{}:", self.function_id).expect("writing to String cannot fail");
        writeln!(self.out, "    push rbp").expect("writing to String cannot fail");
        writeln!(self.out, "    mov rbp, rsp").expect("writing to String cannot fail");
        writeln!(self.out, "    sub rsp, {}", self.frame_size)
            .expect("writing to String cannot fail");
        self.zero_frame();
        self.store_parameters();
        for stmt in &self.function.body {
            self.emit_stmt(stmt)?;
        }
        if self.function.ret == Type::Void {
            writeln!(self.out, "    jmp {}", self.epilogue).expect("writing to String cannot fail");
        } else if self.function.ret.is_float() {
            writeln!(self.out, "    pxor xmm0, xmm0").expect("writing to String cannot fail");
        } else {
            writeln!(self.out, "    xor eax, eax").expect("writing to String cannot fail");
        }
        writeln!(self.out, "{}:", self.epilogue).expect("writing to String cannot fail");
        writeln!(self.out, "    mov rsp, rbp").expect("writing to String cannot fail");
        writeln!(self.out, "    pop rbp").expect("writing to String cannot fail");
        writeln!(self.out, "    ret").expect("writing to String cannot fail");
        Ok(())
    }

    fn zero_frame(&mut self) {
        let mut offset = 0usize;
        while offset < self.frame_size {
            let disp = self.frame_size - offset;
            writeln!(self.out, "    mov qword ptr [rbp - {disp}], 0")
                .expect("writing to String cannot fail");
            offset += 8;
        }
    }

    fn store_parameters(&mut self) {
        let mut int_index = 0usize;
        let mut sse_index = 0usize;
        let mut stack_index = 0usize;
        for (param_index, param) in self.function.params.iter().enumerate() {
            let disp = self.local_disp(param_index);
            let destination = format!("[rbp - {disp}]");
            if param.ty.is_float() {
                if sse_index < SSE_ARG_REGS.len() {
                    let register = SSE_ARG_REGS[sse_index];
                    sse_index += 1;
                    if param.ty == Type::F32 {
                        writeln!(self.out, "    movss dword ptr {destination}, {register}")
                            .expect("writing to String cannot fail");
                    } else {
                        writeln!(self.out, "    movsd qword ptr {destination}, {register}")
                            .expect("writing to String cannot fail");
                    }
                } else {
                    let source_disp = 16 + stack_index * 8;
                    stack_index += 1;
                    if param.ty == Type::F32 {
                        writeln!(self.out, "    movss xmm0, dword ptr [rbp + {source_disp}]")
                            .expect("writing to String cannot fail");
                        writeln!(self.out, "    movss dword ptr {destination}, xmm0")
                            .expect("writing to String cannot fail");
                    } else {
                        writeln!(self.out, "    movsd xmm0, qword ptr [rbp + {source_disp}]")
                            .expect("writing to String cannot fail");
                        writeln!(self.out, "    movsd qword ptr {destination}, xmm0")
                            .expect("writing to String cannot fail");
                    }
                }
            } else if int_index < INT_ARG_REGS.len() {
                let register = INT_ARG_REGS[int_index];
                int_index += 1;
                writeln!(self.out, "    mov rax, {register}")
                    .expect("writing to String cannot fail");
                self.store_int_at(&destination, &param.ty);
            } else {
                let source_disp = 16 + stack_index * 8;
                stack_index += 1;
                writeln!(self.out, "    mov rax, qword ptr [rbp + {source_disp}]")
                    .expect("writing to String cannot fail");
                self.store_int_at(&destination, &param.ty);
            }
        }
    }

    fn store_int_at(&mut self, destination: &str, ty: &Type) {
        let instruction = match ty {
            Type::Bool | Type::I8 | Type::U8 => "mov byte ptr",
            Type::I16 | Type::U16 => "mov word ptr",
            Type::I32 | Type::U32 => "mov dword ptr",
            _ => "mov qword ptr",
        };
        let source = match ty {
            Type::Bool | Type::I8 | Type::U8 => "al",
            Type::I16 | Type::U16 => "ax",
            Type::I32 | Type::U32 => "eax",
            _ => "rax",
        };
        writeln!(self.out, "    {instruction} {destination}, {source}")
            .expect("writing to String cannot fail");
    }

    fn emit_stmt(&mut self, stmt: &Stmt) -> Result<(), CodegenError> {
        match stmt {
            Stmt::Assign { local, val } => {
                let ty = self
                    .function
                    .local_ty(*local)
                    .ok_or_else(|| CodegenError::InvalidIr("bad local".into()))?
                    .clone();
                self.emit_expr(val)?;
                let disp = self
                    .local_offsets
                    .get(local.0)
                    .map(|offset| self.frame_size - offset)
                    .ok_or_else(|| CodegenError::InvalidIr("bad local".into()))?;
                let destination = format!("[rbp - {disp}]");
                self.store_value_at(&destination, &ty);
                Ok(())
            }
            Stmt::Store { ptr, ty, val } => {
                self.emit_expr(ptr)?;
                let slot = self.alloc_temp()?;
                self.spill_rax(slot);
                self.emit_expr(val)?;
                let pointer_disp = self.temp_disp(slot);
                writeln!(self.out, "    mov rcx, qword ptr [rbp - {pointer_disp}]")
                    .expect("writing to String cannot fail");
                self.store_value_at("[rcx]", ty);
                self.free_temp(slot);
                Ok(())
            }
            Stmt::MemCopy { dst, src, size } => {
                self.emit_expr(dst)?;
                let dst_slot = self.alloc_temp()?;
                self.spill_rax(dst_slot);
                self.emit_expr(src)?;
                let src_slot = self.alloc_temp()?;
                self.spill_rax(src_slot);
                self.emit_expr(size)?;
                writeln!(self.out, "    mov rcx, rax").expect("writing to String cannot fail");
                let dst_disp = self.temp_disp(dst_slot);
                let src_disp = self.temp_disp(src_slot);
                writeln!(self.out, "    mov rdi, qword ptr [rbp - {dst_disp}]")
                    .expect("writing to String cannot fail");
                writeln!(self.out, "    mov rsi, qword ptr [rbp - {src_disp}]")
                    .expect("writing to String cannot fail");
                writeln!(self.out, "    rep movsb").expect("writing to String cannot fail");
                self.free_temp(dst_slot);
                Ok(())
            }
            Stmt::Expr(expr) => {
                self.emit_expr(expr)?;
                Ok(())
            }
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let else_label = self.label("else");
                let end_label = self.label("endif");
                self.emit_expr(cond)?;
                writeln!(self.out, "    test eax, eax").expect("writing to String cannot fail");
                writeln!(self.out, "    je {else_label}").expect("writing to String cannot fail");
                for nested in then_body {
                    self.emit_stmt(nested)?;
                }
                writeln!(self.out, "    jmp {end_label}").expect("writing to String cannot fail");
                writeln!(self.out, "{else_label}:").expect("writing to String cannot fail");
                for nested in else_body {
                    self.emit_stmt(nested)?;
                }
                writeln!(self.out, "{end_label}:").expect("writing to String cannot fail");
                Ok(())
            }
            Stmt::Loop { body } => {
                let start = self.label("loop");
                let end = self.label("endloop");
                self.loop_stack.push((end.clone(), start.clone()));
                writeln!(self.out, "{start}:").expect("writing to String cannot fail");
                for nested in body {
                    self.emit_stmt(nested)?;
                }
                writeln!(self.out, "    jmp {start}").expect("writing to String cannot fail");
                writeln!(self.out, "{end}:").expect("writing to String cannot fail");
                self.loop_stack.pop();
                Ok(())
            }
            Stmt::Break => {
                let label = self
                    .loop_stack
                    .last()
                    .map(|(end, _)| end.clone())
                    .ok_or_else(|| CodegenError::InvalidIr("break outside loop".into()))?;
                writeln!(self.out, "    jmp {label}").expect("writing to String cannot fail");
                Ok(())
            }
            Stmt::Continue => {
                let label = self
                    .loop_stack
                    .last()
                    .map(|(_, start)| start.clone())
                    .ok_or_else(|| CodegenError::InvalidIr("continue outside loop".into()))?;
                writeln!(self.out, "    jmp {label}").expect("writing to String cannot fail");
                Ok(())
            }
            Stmt::Return(value) => {
                if let Some(expr) = value {
                    self.emit_expr(expr)?;
                }
                writeln!(self.out, "    jmp {}", self.epilogue)
                    .expect("writing to String cannot fail");
                Ok(())
            }
            Stmt::Trap => {
                writeln!(self.out, "    ud2").expect("writing to String cannot fail");
                Ok(())
            }
        }
    }

    fn emit_expr(&mut self, expr: &Expr) -> Result<Type, CodegenError> {
        let ty = infer_expr_type(self.module, self.function, expr)
            .map_err(|error| CodegenError::InvalidIr(format!("{error:?}")))?;
        match expr {
            Expr::ConstInt(value, ty) => {
                let bits = normalize_int(ty, *value as u64);
                writeln!(self.out, "    mov rax, {}", bits as i64)
                    .expect("writing to String cannot fail");
            }
            Expr::ConstUint(value, ty) => {
                let bits = normalize_int(ty, *value);
                writeln!(self.out, "    mov rax, {}", bits as i64)
                    .expect("writing to String cannot fail");
            }
            Expr::ConstFloat(value, Type::F32) => {
                let bits = (*value as f32).to_bits();
                writeln!(self.out, "    mov eax, {bits}").expect("writing to String cannot fail");
                writeln!(self.out, "    movd xmm0, eax").expect("writing to String cannot fail");
            }
            Expr::ConstFloat(value, _) => {
                writeln!(self.out, "    mov rax, {}", value.to_bits() as i64)
                    .expect("writing to String cannot fail");
                writeln!(self.out, "    movq xmm0, rax").expect("writing to String cannot fail");
            }
            Expr::ConstBool(value) => {
                writeln!(self.out, "    mov eax, {}", u8::from(*value))
                    .expect("writing to String cannot fail");
            }
            Expr::ConstNull(_) => {
                writeln!(self.out, "    xor eax, eax").expect("writing to String cannot fail");
            }
            Expr::RodataPtr(id) => {
                writeln!(self.out, "    lea rax, [rip + tc_rodata_{}]", id.0)
                    .expect("writing to String cannot fail");
            }
            Expr::RodataLen(id) => {
                let length = self.module.rodata[id.0].bytes.len();
                writeln!(self.out, "    mov rax, {length}").expect("writing to String cannot fail");
            }
            Expr::Local(id) => {
                let local_ty = self
                    .function
                    .local_ty(*id)
                    .ok_or_else(|| CodegenError::InvalidIr("bad local".into()))?;
                let disp = self
                    .local_offsets
                    .get(id.0)
                    .map(|offset| self.frame_size - offset)
                    .ok_or_else(|| CodegenError::InvalidIr("bad local".into()))?;
                self.load_value_at(&format!("[rbp - {disp}]"), local_ty);
            }
            Expr::AddrOfLocal(id) => {
                let disp = self
                    .local_offsets
                    .get(id.0)
                    .map(|offset| self.frame_size - offset)
                    .ok_or_else(|| CodegenError::InvalidIr("bad local".into()))?;
                writeln!(self.out, "    lea rax, [rbp - {disp}]")
                    .expect("writing to String cannot fail");
            }
            Expr::GlobalAddr(id) => {
                writeln!(self.out, "    lea rax, [rip + tc_global_{}]", id.0)
                    .expect("writing to String cannot fail");
            }
            Expr::FnAddr(id) => {
                writeln!(self.out, "    lea rax, [rip + tc_fn_{}]", id.0)
                    .expect("writing to String cannot fail");
            }
            Expr::SizeOf(type_) => {
                writeln!(self.out, "    mov rax, {}", self.module.size_of(type_))
                    .expect("writing to String cannot fail");
            }
            Expr::AlignOf(type_) => {
                writeln!(self.out, "    mov rax, {}", self.module.align_of(type_))
                    .expect("writing to String cannot fail");
            }
            Expr::Load { ptr, ty } => {
                self.emit_expr(ptr)?;
                self.load_value_at("[rax]", ty);
            }
            Expr::FieldAddr {
                base_ptr,
                struct_id,
                field_idx,
            } => {
                self.emit_expr(base_ptr)?;
                let offset = self
                    .module
                    .try_field_offset(*struct_id, *field_idx)
                    .ok_or_else(|| CodegenError::InvalidIr("bad struct field".into()))?;
                if offset != 0 {
                    writeln!(self.out, "    add rax, {offset}")
                        .expect("writing to String cannot fail");
                }
            }
            Expr::UnionFieldAddr { base_ptr, .. } => {
                self.emit_expr(base_ptr)?;
            }
            Expr::IndexAddr {
                base_ptr,
                elem_ty,
                index,
            } => {
                self.emit_expr(base_ptr)?;
                let slot = self.alloc_temp()?;
                self.spill_rax(slot);
                self.emit_expr(index)?;
                let stride = self.module.size_of(elem_ty);
                if stride != 0 {
                    writeln!(self.out, "    imul rax, rax, {stride}")
                        .expect("writing to String cannot fail");
                } else {
                    writeln!(self.out, "    xor eax, eax").expect("writing to String cannot fail");
                }
                let base_disp = self.temp_disp(slot);
                writeln!(self.out, "    mov rcx, qword ptr [rbp - {base_disp}]")
                    .expect("writing to String cannot fail");
                writeln!(self.out, "    add rax, rcx").expect("writing to String cannot fail");
                self.free_temp(slot);
            }
            Expr::Unary { op, val } => {
                let operand_ty = self.emit_expr(val)?;
                match op {
                    UnOp::Neg if operand_ty.is_float() => {
                        if operand_ty == Type::F32 {
                            writeln!(self.out, "    mov eax, -2147483648")
                                .expect("writing to String cannot fail");
                            writeln!(self.out, "    movd xmm1, eax")
                                .expect("writing to String cannot fail");
                            writeln!(self.out, "    xorps xmm0, xmm1")
                                .expect("writing to String cannot fail");
                        } else {
                            writeln!(self.out, "    mov rax, -9223372036854775808")
                                .expect("writing to String cannot fail");
                            writeln!(self.out, "    movq xmm1, rax")
                                .expect("writing to String cannot fail");
                            writeln!(self.out, "    xorpd xmm0, xmm1")
                                .expect("writing to String cannot fail");
                        }
                    }
                    UnOp::Neg => {
                        writeln!(self.out, "    neg rax").expect("writing to String cannot fail");
                        self.normalize_rax(&operand_ty);
                    }
                    UnOp::BitNot => {
                        writeln!(self.out, "    not rax").expect("writing to String cannot fail");
                        self.normalize_rax(&operand_ty);
                    }
                    UnOp::LogNot => {
                        writeln!(self.out, "    xor eax, 1")
                            .expect("writing to String cannot fail");
                    }
                }
            }
            Expr::Binary { op, lhs, rhs } => {
                self.emit_binary(*op, lhs, rhs)?;
            }
            Expr::Cast { val, to } => {
                let from = self.emit_expr(val)?;
                self.emit_cast(&from, to)?;
            }
            Expr::Call { target, args } => {
                self.emit_call(target, args)?;
            }
            Expr::Syscall { num, args } => {
                self.emit_syscall(num, args)?;
            }
        }
        Ok(ty)
    }

    fn emit_binary(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr) -> Result<(), CodegenError> {
        let left_ty = infer_expr_type(self.module, self.function, lhs)
            .map_err(|error| CodegenError::InvalidIr(format!("{error:?}")))?;
        if matches!(op, BinOp::LogAnd | BinOp::LogOr) {
            let short_label = self.label("logic_short");
            let end_label = self.label("logic_end");
            self.emit_expr(lhs)?;
            writeln!(self.out, "    test eax, eax").expect("writing to String cannot fail");
            if op == BinOp::LogAnd {
                writeln!(self.out, "    je {short_label}").expect("writing to String cannot fail");
                self.emit_expr(rhs)?;
                writeln!(self.out, "    jmp {end_label}").expect("writing to String cannot fail");
                writeln!(self.out, "{short_label}:").expect("writing to String cannot fail");
                writeln!(self.out, "    xor eax, eax").expect("writing to String cannot fail");
            } else {
                writeln!(self.out, "    jne {short_label}").expect("writing to String cannot fail");
                self.emit_expr(rhs)?;
                writeln!(self.out, "    jmp {end_label}").expect("writing to String cannot fail");
                writeln!(self.out, "{short_label}:").expect("writing to String cannot fail");
                writeln!(self.out, "    mov eax, 1").expect("writing to String cannot fail");
            }
            writeln!(self.out, "{end_label}:").expect("writing to String cannot fail");
            return Ok(());
        }

        self.emit_expr(lhs)?;
        let slot = self.alloc_temp()?;
        if left_ty.is_float() {
            self.spill_xmm(slot, &left_ty);
        } else {
            self.spill_rax(slot);
        }
        self.emit_expr(rhs)?;
        let left_disp = self.temp_disp(slot);

        if left_ty.is_float() {
            if left_ty == Type::F32 {
                writeln!(self.out, "    movss xmm1, dword ptr [rbp - {left_disp}]")
                    .expect("writing to String cannot fail");
            } else {
                writeln!(self.out, "    movsd xmm1, qword ptr [rbp - {left_disp}]")
                    .expect("writing to String cannot fail");
            }
            let suffix = if left_ty == Type::F32 { "ss" } else { "sd" };
            match op {
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                    let instruction = match op {
                        BinOp::Add => "add",
                        BinOp::Sub => "sub",
                        BinOp::Mul => "mul",
                        _ => "div",
                    };
                    writeln!(self.out, "    {instruction}{suffix} xmm1, xmm0")
                        .expect("writing to String cannot fail");
                    writeln!(self.out, "    mov{suffix} xmm0, xmm1")
                        .expect("writing to String cannot fail");
                }
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                    let compare = if left_ty == Type::F32 {
                        "ucomiss"
                    } else {
                        "ucomisd"
                    };
                    writeln!(self.out, "    {compare} xmm1, xmm0")
                        .expect("writing to String cannot fail");
                    match op {
                        BinOp::Eq => {
                            writeln!(self.out, "    sete al")
                                .expect("writing to String cannot fail");
                            writeln!(self.out, "    setnp cl")
                                .expect("writing to String cannot fail");
                            writeln!(self.out, "    and al, cl")
                                .expect("writing to String cannot fail");
                        }
                        BinOp::Ne => {
                            writeln!(self.out, "    setne al")
                                .expect("writing to String cannot fail");
                            writeln!(self.out, "    setp cl")
                                .expect("writing to String cannot fail");
                            writeln!(self.out, "    or al, cl")
                                .expect("writing to String cannot fail");
                        }
                        BinOp::Lt => {
                            writeln!(self.out, "    setb al")
                                .expect("writing to String cannot fail");
                            writeln!(self.out, "    setnp cl")
                                .expect("writing to String cannot fail");
                            writeln!(self.out, "    and al, cl")
                                .expect("writing to String cannot fail");
                        }
                        BinOp::Le => {
                            writeln!(self.out, "    setbe al")
                                .expect("writing to String cannot fail");
                            writeln!(self.out, "    setnp cl")
                                .expect("writing to String cannot fail");
                            writeln!(self.out, "    and al, cl")
                                .expect("writing to String cannot fail");
                        }
                        BinOp::Gt => {
                            writeln!(self.out, "    seta al")
                                .expect("writing to String cannot fail");
                        }
                        BinOp::Ge => {
                            writeln!(self.out, "    setae al")
                                .expect("writing to String cannot fail");
                        }
                        _ => unreachable!(),
                    }
                    writeln!(self.out, "    movzx eax, al").expect("writing to String cannot fail");
                }
                _ => {
                    return Err(CodegenError::Emit(
                        "unsupported floating binary operator".into(),
                    ));
                }
            }
            self.free_temp(slot);
            return Ok(());
        }

        writeln!(self.out, "    mov r8, rax").expect("writing to String cannot fail");
        writeln!(self.out, "    mov rax, qword ptr [rbp - {left_disp}]")
            .expect("writing to String cannot fail");
        let signed = left_ty.is_signed_int();
        match op {
            BinOp::Add => {
                writeln!(self.out, "    add rax, r8").expect("writing to String cannot fail");
                self.normalize_rax(&left_ty);
            }
            BinOp::Sub => {
                writeln!(self.out, "    sub rax, r8").expect("writing to String cannot fail");
                self.normalize_rax(&left_ty);
            }
            BinOp::Mul => {
                writeln!(self.out, "    imul rax, r8").expect("writing to String cannot fail");
                self.normalize_rax(&left_ty);
            }
            BinOp::Div | BinOp::Mod => {
                if signed {
                    if left_ty == Type::I64 {
                        let normal = self.label("idiv_normal");
                        let done = self.label("idiv_done");
                        writeln!(self.out, "    cmp r8, -1")
                            .expect("writing to String cannot fail");
                        writeln!(self.out, "    jne {normal}")
                            .expect("writing to String cannot fail");
                        writeln!(self.out, "    mov rcx, -9223372036854775808")
                            .expect("writing to String cannot fail");
                        writeln!(self.out, "    cmp rax, rcx")
                            .expect("writing to String cannot fail");
                        writeln!(self.out, "    jne {normal}")
                            .expect("writing to String cannot fail");
                        if op == BinOp::Mod {
                            writeln!(self.out, "    xor eax, eax")
                                .expect("writing to String cannot fail");
                        }
                        writeln!(self.out, "    jmp {done}")
                            .expect("writing to String cannot fail");
                        writeln!(self.out, "{normal}:").expect("writing to String cannot fail");
                        writeln!(self.out, "    cqo").expect("writing to String cannot fail");
                        writeln!(self.out, "    idiv r8").expect("writing to String cannot fail");
                        if op == BinOp::Mod {
                            writeln!(self.out, "    mov rax, rdx")
                                .expect("writing to String cannot fail");
                        }
                        writeln!(self.out, "{done}:").expect("writing to String cannot fail");
                    } else {
                        writeln!(self.out, "    cqo").expect("writing to String cannot fail");
                        writeln!(self.out, "    idiv r8").expect("writing to String cannot fail");
                        if op == BinOp::Mod {
                            writeln!(self.out, "    mov rax, rdx")
                                .expect("writing to String cannot fail");
                        }
                    }
                } else {
                    writeln!(self.out, "    xor edx, edx").expect("writing to String cannot fail");
                    writeln!(self.out, "    div r8").expect("writing to String cannot fail");
                    if op == BinOp::Mod {
                        writeln!(self.out, "    mov rax, rdx")
                            .expect("writing to String cannot fail");
                    }
                }
                self.normalize_rax(&left_ty);
            }
            BinOp::BitAnd => {
                writeln!(self.out, "    and rax, r8").expect("writing to String cannot fail");
                self.normalize_rax(&left_ty);
            }
            BinOp::BitOr => {
                writeln!(self.out, "    or rax, r8").expect("writing to String cannot fail");
                self.normalize_rax(&left_ty);
            }
            BinOp::BitXor => {
                writeln!(self.out, "    xor rax, r8").expect("writing to String cannot fail");
                self.normalize_rax(&left_ty);
            }
            BinOp::Shl | BinOp::Shr => {
                writeln!(self.out, "    mov rcx, r8").expect("writing to String cannot fail");
                let instruction = if op == BinOp::Shl {
                    "shl"
                } else if signed {
                    "sar"
                } else {
                    "shr"
                };
                writeln!(self.out, "    {instruction} rax, cl")
                    .expect("writing to String cannot fail");
                self.normalize_rax(&left_ty);
            }
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                writeln!(self.out, "    cmp rax, r8").expect("writing to String cannot fail");
                let condition = match op {
                    BinOp::Eq => "e",
                    BinOp::Ne => "ne",
                    BinOp::Lt if signed => "l",
                    BinOp::Lt => "b",
                    BinOp::Le if signed => "le",
                    BinOp::Le => "be",
                    BinOp::Gt if signed => "g",
                    BinOp::Gt => "a",
                    BinOp::Ge if signed => "ge",
                    BinOp::Ge => "ae",
                    _ => unreachable!(),
                };
                writeln!(self.out, "    set{condition} al").expect("writing to String cannot fail");
                writeln!(self.out, "    movzx eax, al").expect("writing to String cannot fail");
            }
            _ => {
                return Err(CodegenError::Emit(
                    "unsupported integer binary operator".into(),
                ));
            }
        }
        self.free_temp(slot);
        Ok(())
    }

    fn emit_cast(&mut self, from: &Type, to: &Type) -> Result<(), CodegenError> {
        if from.is_integer() && to.is_integer() {
            self.normalize_rax(to);
            return Ok(());
        }
        if from.is_integer() && to.is_float() {
            let unsigned_wide = *from == Type::U64;
            if unsigned_wide {
                let low = self.label("u64_to_float_low");
                let done = self.label("u64_to_float_done");
                writeln!(self.out, "    test rax, rax").expect("writing to String cannot fail");
                writeln!(self.out, "    jns {low}").expect("writing to String cannot fail");
                writeln!(self.out, "    mov rcx, rax").expect("writing to String cannot fail");
                writeln!(self.out, "    and eax, 1").expect("writing to String cannot fail");
                writeln!(self.out, "    shr rcx, 1").expect("writing to String cannot fail");
                writeln!(self.out, "    or rcx, rax").expect("writing to String cannot fail");
                if *to == Type::F32 {
                    writeln!(self.out, "    cvtsi2ss xmm0, rcx")
                        .expect("writing to String cannot fail");
                    writeln!(self.out, "    addss xmm0, xmm0")
                        .expect("writing to String cannot fail");
                } else {
                    writeln!(self.out, "    cvtsi2sd xmm0, rcx")
                        .expect("writing to String cannot fail");
                    writeln!(self.out, "    addsd xmm0, xmm0")
                        .expect("writing to String cannot fail");
                }
                writeln!(self.out, "    jmp {done}").expect("writing to String cannot fail");
                writeln!(self.out, "{low}:").expect("writing to String cannot fail");
                if *to == Type::F32 {
                    writeln!(self.out, "    cvtsi2ss xmm0, rax")
                        .expect("writing to String cannot fail");
                } else {
                    writeln!(self.out, "    cvtsi2sd xmm0, rax")
                        .expect("writing to String cannot fail");
                }
                writeln!(self.out, "{done}:").expect("writing to String cannot fail");
            } else if *to == Type::F32 {
                writeln!(self.out, "    cvtsi2ss xmm0, rax")
                    .expect("writing to String cannot fail");
            } else {
                writeln!(self.out, "    cvtsi2sd xmm0, rax")
                    .expect("writing to String cannot fail");
            }
            return Ok(());
        }
        if from.is_float() && to.is_integer() {
            // The reference interpreter uses Rust's saturating float-to-int
            // semantics.  x86 `cvtt*2si` instead yields INT64_MIN for NaN and
            // out-of-range values, so clamp explicitly before converting.
            if *from == Type::F32 {
                writeln!(self.out, "    cvtss2sd xmm0, xmm0")
                    .expect("writing to String cannot fail");
            }
            let nan = self.label("f2i_nan");
            let low = self.label("f2i_low");
            let high = self.label("f2i_high");
            let done = self.label("f2i_done");

            // NaN -> 0.
            writeln!(self.out, "    ucomisd xmm0, xmm0").expect("writing to String cannot fail");
            writeln!(self.out, "    jp {nan}").expect("writing to String cannot fail");

            let (lower, upper, lower_bits, upper_bits) = match to {
                Type::I8 => (-128.0_f64, 128.0_f64, (-128_i64) as u64, 127_u64),
                Type::I16 => (-32768.0_f64, 32768.0_f64, (-32768_i64) as u64, 32767_u64),
                Type::I32 => (
                    -2_147_483_648.0_f64,
                    2_147_483_648.0_f64,
                    (-2_147_483_648_i64) as u64,
                    2_147_483_647_u64,
                ),
                Type::I64 => (
                    -9_223_372_036_854_775_808.0_f64,
                    9_223_372_036_854_775_808.0_f64,
                    i64::MIN as u64,
                    i64::MAX as u64,
                ),
                Type::U8 => (0.0_f64, 256.0_f64, 0, 255_u64),
                Type::U16 => (0.0_f64, 65536.0_f64, 0, 65535_u64),
                Type::U32 => (0.0_f64, 4_294_967_296.0_f64, 0, 4_294_967_295_u64),
                Type::U64 => (0.0_f64, 18_446_744_073_709_551_616.0_f64, 0, u64::MAX),
                _ => {
                    return Err(CodegenError::Emit(format!(
                        "unsupported float cast target {to:?}"
                    )));
                }
            };

            writeln!(self.out, "    mov rax, {}", lower.to_bits())
                .expect("writing to String cannot fail");
            writeln!(self.out, "    movq xmm1, rax").expect("writing to String cannot fail");
            writeln!(self.out, "    ucomisd xmm0, xmm1").expect("writing to String cannot fail");
            writeln!(self.out, "    jb {low}").expect("writing to String cannot fail");

            writeln!(self.out, "    mov rax, {}", upper.to_bits())
                .expect("writing to String cannot fail");
            writeln!(self.out, "    movq xmm1, rax").expect("writing to String cannot fail");
            writeln!(self.out, "    ucomisd xmm0, xmm1").expect("writing to String cannot fail");
            writeln!(self.out, "    jae {high}").expect("writing to String cannot fail");

            writeln!(self.out, "    cvttsd2si rax, xmm0").expect("writing to String cannot fail");
            self.normalize_rax(to);
            writeln!(self.out, "    jmp {done}").expect("writing to String cannot fail");

            writeln!(self.out, "{nan}:").expect("writing to String cannot fail");
            writeln!(self.out, "    xor eax, eax").expect("writing to String cannot fail");
            writeln!(self.out, "    jmp {done}").expect("writing to String cannot fail");
            writeln!(self.out, "{low}:").expect("writing to String cannot fail");
            writeln!(self.out, "    mov rax, {lower_bits}").expect("writing to String cannot fail");
            self.normalize_rax(to);
            writeln!(self.out, "    jmp {done}").expect("writing to String cannot fail");
            writeln!(self.out, "{high}:").expect("writing to String cannot fail");
            writeln!(self.out, "    mov rax, {upper_bits}").expect("writing to String cannot fail");
            self.normalize_rax(to);
            writeln!(self.out, "{done}:").expect("writing to String cannot fail");
            return Ok(());
        }
        match (from, to) {
            (Type::F32, Type::F64) => {
                writeln!(self.out, "    cvtss2sd xmm0, xmm0")
                    .expect("writing to String cannot fail");
            }
            (Type::F64, Type::F32) => {
                writeln!(self.out, "    cvtsd2ss xmm0, xmm0")
                    .expect("writing to String cannot fail");
            }
            (Type::Ptr(_), Type::Ptr(_))
            | (Type::Ptr(_), Type::U64)
            | (Type::U64, Type::Ptr(_))
            | (Type::FnPtr { .. }, Type::U64)
            | (Type::U64, Type::FnPtr { .. }) => {}
            _ => {
                return Err(CodegenError::Emit(format!(
                    "unsupported cast {from:?} -> {to:?}"
                )));
            }
        }
        Ok(())
    }

    fn emit_call(&mut self, target: &CallTarget, args: &[Expr]) -> Result<(), CodegenError> {
        let base = self.alloc_scratch(args.len() + 1)?;
        let target_slot = base + args.len();
        if let CallTarget::Indirect(expr) = target {
            self.emit_expr(expr)?;
            let disp = self.scratch_disp(target_slot);
            writeln!(self.out, "    mov qword ptr [rbp - {disp}], rax")
                .expect("writing to String cannot fail");
        }

        let mut arg_types = Vec::with_capacity(args.len());
        for (index, arg) in args.iter().enumerate() {
            let ty = self.emit_expr(arg)?;
            let disp = self.scratch_disp(base + index);
            if ty.is_float() {
                writeln!(self.out, "    mov qword ptr [rbp - {disp}], 0")
                    .expect("writing to String cannot fail");
                if ty == Type::F32 {
                    writeln!(self.out, "    movss dword ptr [rbp - {disp}], xmm0")
                        .expect("writing to String cannot fail");
                } else {
                    writeln!(self.out, "    movsd qword ptr [rbp - {disp}], xmm0")
                        .expect("writing to String cannot fail");
                }
            } else {
                writeln!(self.out, "    mov qword ptr [rbp - {disp}], rax")
                    .expect("writing to String cannot fail");
            }
            arg_types.push(ty);
        }

        let mut int_index = 0usize;
        let mut sse_index = 0usize;
        let mut stack_args = Vec::new();
        for (index, ty) in arg_types.iter().enumerate() {
            if ty.is_float() {
                if sse_index < SSE_ARG_REGS.len() {
                    sse_index += 1;
                } else {
                    stack_args.push(index);
                }
            } else if int_index < INT_ARG_REGS.len() {
                int_index += 1;
            } else {
                stack_args.push(index);
            }
        }

        let stack_bytes = stack_args.len() * 8;
        let stack_total = round_up(stack_bytes, 16);
        if stack_total > 0 {
            writeln!(self.out, "    sub rsp, {stack_total}")
                .expect("writing to String cannot fail");
            for (position, index) in stack_args.iter().enumerate() {
                let disp = self.scratch_disp(base + index);
                writeln!(self.out, "    mov rax, qword ptr [rbp - {disp}]")
                    .expect("writing to String cannot fail");
                writeln!(self.out, "    mov qword ptr [rsp + {}], rax", position * 8)
                    .expect("writing to String cannot fail");
            }
        }

        int_index = 0;
        sse_index = 0;
        for (index, ty) in arg_types.iter().enumerate() {
            let disp = self.scratch_disp(base + index);
            if ty.is_float() {
                if sse_index < SSE_ARG_REGS.len() {
                    let register = SSE_ARG_REGS[sse_index];
                    sse_index += 1;
                    if *ty == Type::F32 {
                        writeln!(self.out, "    movss {register}, dword ptr [rbp - {disp}]")
                            .expect("writing to String cannot fail");
                    } else {
                        writeln!(self.out, "    movsd {register}, qword ptr [rbp - {disp}]")
                            .expect("writing to String cannot fail");
                    }
                }
            } else if int_index < INT_ARG_REGS.len() {
                let register = INT_ARG_REGS[int_index];
                int_index += 1;
                writeln!(self.out, "    mov {register}, qword ptr [rbp - {disp}]")
                    .expect("writing to String cannot fail");
            }
        }

        if matches!(target, CallTarget::Indirect(_)) {
            let disp = self.scratch_disp(target_slot);
            writeln!(self.out, "    mov r11, qword ptr [rbp - {disp}]")
                .expect("writing to String cannot fail");
            writeln!(self.out, "    call r11").expect("writing to String cannot fail");
        } else if let CallTarget::Direct(id) = target {
            writeln!(self.out, "    call tc_fn_{}", id.0).expect("writing to String cannot fail");
        }
        if stack_total > 0 {
            writeln!(self.out, "    add rsp, {stack_total}")
                .expect("writing to String cannot fail");
        }
        self.free_scratch(base);
        Ok(())
    }

    fn emit_syscall(&mut self, num: &Expr, args: &[Expr]) -> Result<(), CodegenError> {
        let base = self.alloc_scratch(args.len() + 1)?;
        self.emit_expr(num)?;
        let num_disp = self.scratch_disp(base);
        writeln!(self.out, "    mov qword ptr [rbp - {num_disp}], rax")
            .expect("writing to String cannot fail");
        for (index, arg) in args.iter().enumerate() {
            self.emit_expr(arg)?;
            let disp = self.scratch_disp(base + 1 + index);
            writeln!(self.out, "    mov qword ptr [rbp - {disp}], rax")
                .expect("writing to String cannot fail");
        }
        writeln!(self.out, "    mov rax, qword ptr [rbp - {num_disp}]")
            .expect("writing to String cannot fail");
        let registers = ["rdi", "rsi", "rdx", "r10", "r8", "r9"];
        for (index, register) in registers.iter().enumerate().take(args.len()) {
            let disp = self.scratch_disp(base + 1 + index);
            writeln!(self.out, "    mov {register}, qword ptr [rbp - {disp}]")
                .expect("writing to String cannot fail");
        }
        writeln!(self.out, "    syscall").expect("writing to String cannot fail");
        self.free_scratch(base);
        Ok(())
    }

    fn spill_rax(&mut self, slot: usize) {
        let disp = self.temp_disp(slot);
        writeln!(self.out, "    mov qword ptr [rbp - {disp}], rax")
            .expect("writing to String cannot fail");
    }

    fn spill_xmm(&mut self, slot: usize, ty: &Type) {
        let disp = self.temp_disp(slot);
        if *ty == Type::F32 {
            writeln!(self.out, "    movss dword ptr [rbp - {disp}], xmm0")
                .expect("writing to String cannot fail");
        } else {
            writeln!(self.out, "    movsd qword ptr [rbp - {disp}], xmm0")
                .expect("writing to String cannot fail");
        }
    }

    fn load_value_at(&mut self, address: &str, ty: &Type) {
        match ty {
            Type::Bool => {
                writeln!(self.out, "    movzx eax, byte ptr {address}")
                    .expect("writing to String cannot fail");
                writeln!(self.out, "    test eax, eax").expect("writing to String cannot fail");
                writeln!(self.out, "    setne al").expect("writing to String cannot fail");
                writeln!(self.out, "    movzx eax, al").expect("writing to String cannot fail");
            }
            Type::U8 => {
                writeln!(self.out, "    movzx eax, byte ptr {address}")
                    .expect("writing to String cannot fail");
            }
            Type::I8 => {
                writeln!(self.out, "    movsx rax, byte ptr {address}")
                    .expect("writing to String cannot fail");
            }
            Type::I16 => {
                writeln!(self.out, "    movsx rax, word ptr {address}")
                    .expect("writing to String cannot fail");
            }
            Type::U16 => {
                writeln!(self.out, "    movzx eax, word ptr {address}")
                    .expect("writing to String cannot fail");
            }
            Type::I32 => {
                writeln!(self.out, "    movsxd rax, dword ptr {address}")
                    .expect("writing to String cannot fail");
            }
            Type::U32 => {
                writeln!(self.out, "    mov eax, dword ptr {address}")
                    .expect("writing to String cannot fail");
            }
            Type::I64 | Type::U64 | Type::Ptr(_) | Type::FnPtr { .. } => {
                writeln!(self.out, "    mov rax, qword ptr {address}")
                    .expect("writing to String cannot fail");
            }
            Type::F32 => {
                writeln!(self.out, "    movss xmm0, dword ptr {address}")
                    .expect("writing to String cannot fail");
            }
            Type::F64 => {
                writeln!(self.out, "    movsd xmm0, qword ptr {address}")
                    .expect("writing to String cannot fail");
            }
            _ => {
                // Aggregate loads are rejected by the verifier.
            }
        }
    }

    fn store_value_at(&mut self, destination: &str, ty: &Type) {
        match ty {
            Type::Bool | Type::I8 | Type::U8 => {
                writeln!(self.out, "    mov byte ptr {destination}, al")
                    .expect("writing to String cannot fail");
            }
            Type::I16 | Type::U16 => {
                writeln!(self.out, "    mov word ptr {destination}, ax")
                    .expect("writing to String cannot fail");
            }
            Type::I32 | Type::U32 => {
                writeln!(self.out, "    mov dword ptr {destination}, eax")
                    .expect("writing to String cannot fail");
            }
            Type::I64 | Type::U64 | Type::Ptr(_) | Type::FnPtr { .. } => {
                writeln!(self.out, "    mov qword ptr {destination}, rax")
                    .expect("writing to String cannot fail");
            }
            Type::F32 => {
                writeln!(self.out, "    movss dword ptr {destination}, xmm0")
                    .expect("writing to String cannot fail");
            }
            Type::F64 => {
                writeln!(self.out, "    movsd qword ptr {destination}, xmm0")
                    .expect("writing to String cannot fail");
            }
            _ => {}
        }
    }

    fn normalize_rax(&mut self, ty: &Type) {
        match ty {
            Type::I8 => {
                writeln!(self.out, "    movsx rax, al").expect("writing to String cannot fail");
            }
            Type::I16 => {
                writeln!(self.out, "    movsx rax, ax").expect("writing to String cannot fail");
            }
            Type::I32 => {
                writeln!(self.out, "    movsxd rax, eax").expect("writing to String cannot fail");
            }
            Type::U8 | Type::Bool => {
                writeln!(self.out, "    movzx eax, al").expect("writing to String cannot fail");
            }
            Type::U16 => {
                writeln!(self.out, "    movzx eax, ax").expect("writing to String cannot fail");
            }
            Type::U32 => {
                writeln!(self.out, "    mov eax, eax").expect("writing to String cannot fail");
            }
            _ => {}
        }
    }
}

fn normalize_int(ty: &Type, raw: u64) -> u64 {
    match ty {
        Type::Bool => u64::from(raw != 0),
        Type::I8 => (raw as u8 as i8) as i64 as u64,
        Type::I16 => (raw as u16 as i16) as i64 as u64,
        Type::I32 => (raw as u32 as i32) as i64 as u64,
        Type::U8 => u64::from(raw as u8),
        Type::U16 => u64::from(raw as u16),
        Type::U32 => u64::from(raw as u32),
        _ => raw,
    }
}

const fn round_up(value: usize, align: usize) -> usize {
    value.div_ceil(align) * align
}

fn emit_bytes(out: &mut String, bytes: &[u8]) {
    for chunk in bytes.chunks(16) {
        let values = chunk
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(out, "    .byte {values}").expect("writing to String cannot fail");
    }
}
