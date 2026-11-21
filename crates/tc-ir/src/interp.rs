//! Byte-accurate reference interpreter for Core-7 IR.

use crate::ir::{BinOp, CallTarget, Expr, Function, Module, Stmt, UnOp};
use crate::types::Type;
use crate::verify::verify_module;

const RODATA_BASE: u64 = 0x1000_0000;
const DATA_BASE: u64 = 0x2000_0000;
const STACK_BASE: u64 = 0x4000_0000;
const HEAP_BASE: u64 = 0x6000_0000;
const FN_BASE: u64 = 0x8000_0000;
const PAGE_SIZE: usize = 4096;
const STACK_LIMIT: usize = 64 * 1024 * 1024;
const HEAP_LIMIT: usize = 64 * 1024 * 1024;
const MAX_CALL_DEPTH: usize = 4096;

/// Result of running a module in the reference interpreter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterpOutput {
    /// Process exit status, already masked to eight bits.
    pub exit_code: i32,
    /// Bytes written to file descriptor 1.
    pub stdout: Vec<u8>,
    /// Bytes written to file descriptor 2.
    pub stderr: Vec<u8>,
}

/// Reference interpreter failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterpError {
    /// The module failed static verification.
    InvalidModule(Vec<String>),
    /// The instruction budget was exhausted.
    StepLimit(usize),
    /// Execution reached `Stmt::Trap`.
    Trap,
    /// A memory access was outside an allocated region.
    MemoryFault(u64),
    /// Integer division or remainder by zero.
    DivisionByZero,
    /// An indirect call used an unknown function address.
    BadFunctionPointer(u64),
    /// A value or operand had an unexpected type.
    TypeError(String),
    /// The virtual stack limit was exceeded.
    StackOverflow,
    /// The virtual heap limit was exceeded.
    HeapExhausted,
    /// The call depth limit was exceeded.
    CallDepth,
    /// An internal interpreter runtime failure occurred.
    Internal(String),
}

#[derive(Debug, Clone)]
struct Value {
    ty: Type,
    bits: u64,
}

impl Value {
    fn new(ty: Type, bits: u64) -> Self {
        Self { ty, bits }
    }

    fn zero(ty: &Type) -> Self {
        Self {
            ty: ty.clone(),
            bits: 0,
        }
    }

    fn int(ty: &Type, raw: u64) -> Self {
        Self {
            ty: ty.clone(),
            bits: normalize_int(ty, raw),
        }
    }

    fn bool(value: bool) -> Self {
        Self {
            ty: Type::Bool,
            bits: u64::from(value),
        }
    }

    fn as_f32(&self) -> f32 {
        f32::from_bits(self.bits as u32)
    }

    fn as_f64(&self) -> f64 {
        match self.ty {
            Type::F32 => f64::from(self.as_f32()),
            Type::F64 => f64::from_bits(self.bits),
            _ => 0.0,
        }
    }

    fn truthy(&self) -> bool {
        self.bits != 0
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Region {
    Rodata,
    Data,
    Stack,
    Heap,
}

#[derive(Debug, Clone)]
struct Memory {
    rodata: Vec<u8>,
    data: Vec<u8>,
    stack: Vec<u8>,
    stack_bump: usize,
    heap: Vec<u8>,
    heap_bump: usize,
    heap_freed: Vec<(u64, u64)>,
}

impl Memory {
    fn new(rodata_len: usize, data_len: usize) -> Self {
        Self {
            rodata: vec![0; rodata_len],
            data: vec![0; data_len],
            stack: Vec::new(),
            stack_bump: 0,
            heap: Vec::new(),
            heap_bump: 0,
            heap_freed: Vec::new(),
        }
    }

    fn locate(&self, addr: u64, len: usize) -> Option<(Region, usize)> {
        let end = addr.checked_add(u64::try_from(len).ok()?)?;
        let candidates = [
            (Region::Rodata, RODATA_BASE, self.rodata.len()),
            (Region::Data, DATA_BASE, self.data.len()),
            (Region::Stack, STACK_BASE, self.stack.len()),
            (Region::Heap, HEAP_BASE, self.heap.len()),
        ];
        for (region, base, size) in candidates {
            let region_end = base.checked_add(u64::try_from(size).ok()?)?;
            if addr >= base && end <= region_end {
                if region == Region::Heap
                    && self
                        .heap_freed
                        .iter()
                        .any(|(start, stop)| addr < *stop && *start < end)
                {
                    return None;
                }
                let offset = usize::try_from(addr - base).ok()?;
                return Some((region, offset));
            }
        }
        None
    }

    fn unmap_heap(&mut self, address: u64, length: usize) {
        let end = address.saturating_add(u64::try_from(length).unwrap_or(u64::MAX));
        self.heap_freed.push((address, end));
    }

    fn read(&self, addr: u64, len: usize) -> Result<Vec<u8>, InterpError> {
        let (region, offset) = self
            .locate(addr, len)
            .ok_or(InterpError::MemoryFault(addr))?;
        let bytes = match region {
            Region::Rodata => &self.rodata[offset..offset + len],
            Region::Data => &self.data[offset..offset + len],
            Region::Stack => &self.stack[offset..offset + len],
            Region::Heap => &self.heap[offset..offset + len],
        };
        Ok(bytes.to_vec())
    }

    fn write(&mut self, addr: u64, bytes: &[u8]) -> Result<(), InterpError> {
        let (region, offset) = self
            .locate(addr, bytes.len())
            .ok_or(InterpError::MemoryFault(addr))?;
        if region == Region::Rodata {
            return Err(InterpError::MemoryFault(addr));
        }
        let destination = match region {
            Region::Rodata => unreachable!(),
            Region::Data => &mut self.data[offset..offset + bytes.len()],
            Region::Stack => &mut self.stack[offset..offset + bytes.len()],
            Region::Heap => &mut self.heap[offset..offset + bytes.len()],
        };
        destination.copy_from_slice(bytes);
        Ok(())
    }

    fn read_u8(&self, addr: u64) -> Result<u8, InterpError> {
        Ok(self.read(addr, 1)?[0])
    }

    fn read_uint(&self, addr: u64, width: usize) -> Result<u64, InterpError> {
        let bytes = self.read(addr, width)?;
        let mut buffer = [0u8; 8];
        buffer[..width].copy_from_slice(&bytes);
        Ok(u64::from_le_bytes(buffer))
    }

    fn write_uint(&mut self, addr: u64, width: usize, value: u64) -> Result<(), InterpError> {
        let bytes = value.to_le_bytes();
        self.write(addr, &bytes[..width])
    }

    fn alloc_stack(&mut self, size: usize, align: usize) -> Result<u64, InterpError> {
        let size = size.max(1);
        let offset = round_up(self.stack_bump, align);
        let end = offset.checked_add(size).ok_or(InterpError::StackOverflow)?;
        if end > STACK_LIMIT {
            return Err(InterpError::StackOverflow);
        }
        if self.stack.len() < end {
            self.stack.resize(end, 0);
        }
        self.stack[offset..end].fill(0);
        self.stack_bump = end;
        Ok(STACK_BASE + u64::try_from(offset).expect("stack offset fits in u64"))
    }

    fn alloc_heap(&mut self, size: usize) -> Result<u64, InterpError> {
        let offset = round_up(self.heap_bump, PAGE_SIZE);
        let end = offset.checked_add(size).ok_or(InterpError::HeapExhausted)?;
        if end > HEAP_LIMIT {
            return Err(InterpError::HeapExhausted);
        }
        if self.heap.len() < end {
            self.heap.resize(end, 0);
        }
        self.heap[offset..end].fill(0);
        self.heap_bump = end;
        Ok(HEAP_BASE + u64::try_from(offset).expect("heap offset fits in u64"))
    }
}

const fn round_up(value: usize, align: usize) -> usize {
    value.div_ceil(align) * align
}

#[derive(Debug)]
enum Flow {
    Normal,
    Break,
    Continue,
    Return(Option<Value>),
}

#[derive(Debug)]
enum FlowError {
    Exit(i32),
    Error(InterpError),
}

impl From<InterpError> for FlowError {
    fn from(value: InterpError) -> Self {
        Self::Error(value)
    }
}

type FlowResult<T> = Result<T, FlowError>;

#[derive(Debug)]
struct Frame {
    locals: Vec<u64>,
}

#[derive(Debug)]
struct Vm<'a> {
    memory: Memory,
    rodata_offsets: Vec<u64>,
    global_offsets: Vec<u64>,
    fn_addresses: Vec<u64>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    stdin: &'a [u8],
    stdin_cursor: usize,
    steps: usize,
    max_steps: usize,
    call_depth: usize,
}

impl<'a> Vm<'a> {
    fn new(module: &Module, stdin: &'a [u8], max_steps: usize) -> Self {
        let mut rodata_offsets = Vec::with_capacity(module.rodata.len());
        let mut rodata_len = 0usize;
        for blob in &module.rodata {
            rodata_offsets.push(u64::try_from(rodata_len).expect("rodata offset fits"));
            rodata_len += blob.bytes.len();
        }

        let mut global_offsets = Vec::with_capacity(module.globals.len());
        let mut data_len = 0usize;
        for global in &module.globals {
            let align = module.align_of(&global.ty).max(1);
            data_len = round_up(data_len, align);
            global_offsets.push(u64::try_from(data_len).expect("data offset fits"));
            data_len += module.size_of(&global.ty).max(1);
        }

        let mut memory = Memory::new(rodata_len, data_len);
        let mut cursor = 0usize;
        for blob in &module.rodata {
            memory.rodata[cursor..cursor + blob.bytes.len()].copy_from_slice(&blob.bytes);
            cursor += blob.bytes.len();
        }
        for (index, global) in module.globals.iter().enumerate() {
            if let Some(init) = &global.init {
                let offset = usize::try_from(global_offsets[index]).expect("offset fits");
                memory.data[offset..offset + init.len()].copy_from_slice(init);
            }
        }

        let fn_addresses = (0..module.functions.len())
            .map(|index| FN_BASE + u64::try_from(index).expect("index fits") * 16)
            .collect();

        Self {
            memory,
            rodata_offsets,
            global_offsets,
            fn_addresses,
            stdout: Vec::new(),
            stderr: Vec::new(),
            stdin,
            stdin_cursor: 0,
            steps: 0,
            max_steps,
            call_depth: 0,
        }
    }

    fn tick(&mut self) -> FlowResult<()> {
        self.steps += 1;
        if self.steps > self.max_steps {
            Err(FlowError::Error(InterpError::StepLimit(self.max_steps)))
        } else {
            Ok(())
        }
    }

    fn read_scalar(&self, addr: u64, ty: &Type) -> FlowResult<Value> {
        let bits = match ty {
            Type::Bool => u64::from(self.memory.read_u8(addr)? != 0),
            Type::I8 => (self.memory.read_u8(addr)? as i8) as i64 as u64,
            Type::U8 => u64::from(self.memory.read_u8(addr)?),
            Type::I16 => (self.memory.read_uint(addr, 2)? as u16 as i16) as i64 as u64,
            Type::U16 => self.memory.read_uint(addr, 2)?,
            Type::I32 => (self.memory.read_uint(addr, 4)? as u32 as i32) as i64 as u64,
            Type::U32 => self.memory.read_uint(addr, 4)?,
            Type::I64 | Type::U64 | Type::Ptr(_) | Type::FnPtr { .. } => {
                self.memory.read_uint(addr, 8)?
            }
            Type::F32 => self.memory.read_uint(addr, 4)?,
            Type::F64 => self.memory.read_uint(addr, 8)?,
            _ => {
                return Err(FlowError::Error(InterpError::TypeError(format!(
                    "cannot load {ty:?}"
                ))));
            }
        };
        Ok(Value::new(ty.clone(), bits))
    }

    fn write_scalar(&mut self, addr: u64, ty: &Type, value: &Value) -> FlowResult<()> {
        match ty {
            Type::Bool | Type::I8 | Type::U8 => self.memory.write_uint(addr, 1, value.bits)?,
            Type::I16 | Type::U16 => self.memory.write_uint(addr, 2, value.bits)?,
            Type::I32 | Type::U32 | Type::F32 => self.memory.write_uint(addr, 4, value.bits)?,
            Type::I64 | Type::U64 | Type::F64 | Type::Ptr(_) | Type::FnPtr { .. } => {
                self.memory.write_uint(addr, 8, value.bits)?;
            }
            _ => {
                return Err(FlowError::Error(InterpError::TypeError(format!(
                    "cannot store {ty:?}"
                ))));
            }
        }
        Ok(())
    }

    fn call_function(
        &mut self,
        module: &Module,
        function_id: usize,
        args: &[Value],
    ) -> FlowResult<Option<Value>> {
        let function = module
            .functions
            .get(function_id)
            .ok_or_else(|| FlowError::Error(InterpError::TypeError("bad function id".into())))?;
        if args.len() != function.params.len() {
            return Err(FlowError::Error(InterpError::TypeError(
                "argument count mismatch".into(),
            )));
        }
        self.call_depth += 1;
        if self.call_depth > MAX_CALL_DEPTH {
            self.call_depth -= 1;
            return Err(FlowError::Error(InterpError::CallDepth));
        }

        let mark = self.memory.stack_bump;
        let layout = module.local_layout(function);
        let frame_size = round_up(layout.size.max(1), 16);
        let frame_base = match self.memory.alloc_stack(frame_size, 16) {
            Ok(address) => address,
            Err(error) => {
                self.call_depth -= 1;
                return Err(FlowError::Error(error));
            }
        };
        let locals = layout
            .slots
            .iter()
            .map(|slot| frame_base + u64::try_from(slot.offset).expect("slot fits"))
            .collect::<Vec<_>>();
        let frame = Frame { locals };

        for (index, (param, value)) in function.params.iter().zip(args).enumerate() {
            if let Some(address) = frame.locals.get(index).copied() {
                self.write_scalar(address, &param.ty, value)?;
            }
        }

        let returned = match self.exec_stmts(module, function, &frame, &function.body) {
            Ok(Flow::Return(value)) => Ok(value),
            Ok(_) => Ok(if function.ret == Type::Void {
                None
            } else {
                Some(Value::zero(&function.ret))
            }),
            Err(error) => Err(error),
        };
        self.memory.stack_bump = mark;
        self.call_depth -= 1;
        returned
    }

    fn exec_stmts(
        &mut self,
        module: &Module,
        function: &Function,
        frame: &Frame,
        body: &[Stmt],
    ) -> FlowResult<Flow> {
        for stmt in body {
            let flow = self.exec_stmt(module, function, frame, stmt)?;
            if !matches!(flow, Flow::Normal) {
                return Ok(flow);
            }
        }
        Ok(Flow::Normal)
    }

    fn exec_stmt(
        &mut self,
        module: &Module,
        function: &Function,
        frame: &Frame,
        stmt: &Stmt,
    ) -> FlowResult<Flow> {
        self.tick()?;
        match stmt {
            Stmt::Assign { local, val } => {
                let value = self.eval(module, function, frame, val)?;
                let ty = function
                    .local_ty(*local)
                    .ok_or_else(|| FlowError::Error(InterpError::TypeError("bad local".into())))?;
                let address = *frame
                    .locals
                    .get(local.0)
                    .ok_or(FlowError::Error(InterpError::TypeError("bad local".into())))?;
                self.write_scalar(address, ty, &value)?;
                Ok(Flow::Normal)
            }
            Stmt::Store { ptr, ty, val } => {
                let address = self.eval(module, function, frame, ptr)?;
                let value = self.eval(module, function, frame, val)?;
                self.write_scalar(address.bits, ty, &value)?;
                Ok(Flow::Normal)
            }
            Stmt::MemCopy { dst, src, size } => {
                let dst = self.eval(module, function, frame, dst)?;
                let src = self.eval(module, function, frame, src)?;
                let size = self.eval(module, function, frame, size)?;
                let length = usize::try_from(size.bits)
                    .map_err(|_| FlowError::Error(InterpError::MemoryFault(dst.bits)))?;
                // Match the backend's DF=0 `rep movsb`: copy forward one byte
                // at a time, including its behavior for overlapping ranges.
                for offset in 0..length {
                    let delta = u64::try_from(offset).expect("offset fits");
                    let byte = self.memory.read_u8(src.bits.wrapping_add(delta))?;
                    self.memory.write(dst.bits.wrapping_add(delta), &[byte])?;
                }
                Ok(Flow::Normal)
            }
            Stmt::Expr(expr) => {
                self.eval(module, function, frame, expr)?;
                Ok(Flow::Normal)
            }
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let condition = self.eval(module, function, frame, cond)?;
                if condition.truthy() {
                    self.exec_stmts(module, function, frame, then_body)
                } else {
                    self.exec_stmts(module, function, frame, else_body)
                }
            }
            Stmt::Loop { body } => loop {
                self.tick()?;
                match self.exec_stmts(module, function, frame, body)? {
                    Flow::Normal | Flow::Continue => {}
                    Flow::Break => return Ok(Flow::Normal),
                    Flow::Return(value) => return Ok(Flow::Return(value)),
                }
            },
            Stmt::Break => Ok(Flow::Break),
            Stmt::Continue => Ok(Flow::Continue),
            Stmt::Return(value) => {
                let value = match value {
                    Some(expr) => Some(self.eval(module, function, frame, expr)?),
                    None => None,
                };
                Ok(Flow::Return(value))
            }
            Stmt::Trap => Err(FlowError::Error(InterpError::Trap)),
        }
    }

    fn eval(
        &mut self,
        module: &Module,
        function: &Function,
        frame: &Frame,
        expr: &Expr,
    ) -> FlowResult<Value> {
        self.tick()?;
        match expr {
            Expr::ConstInt(raw, ty) => Ok(Value::int(ty, *raw as u64)),
            Expr::ConstUint(raw, ty) => Ok(Value::int(ty, *raw)),
            Expr::ConstFloat(value, ty) => match ty {
                Type::F32 => Ok(Value::new(Type::F32, u64::from((*value as f32).to_bits()))),
                Type::F64 => Ok(Value::new(Type::F64, value.to_bits())),
                _ => Err(FlowError::Error(InterpError::TypeError(
                    "bad float type".into(),
                ))),
            },
            Expr::ConstBool(value) => Ok(Value::bool(*value)),
            Expr::ConstNull(ty) => Ok(Value::new(ty.clone(), 0)),
            Expr::RodataPtr(id) => {
                let offset = *self.rodata_offsets.get(id.0).ok_or_else(|| {
                    FlowError::Error(InterpError::TypeError("bad rodata id".into()))
                })?;
                Ok(Value::new(
                    Type::Ptr(Box::new(Type::U8)),
                    RODATA_BASE + offset,
                ))
            }
            Expr::RodataLen(id) => {
                let blob = module.rodata.get(id.0).ok_or_else(|| {
                    FlowError::Error(InterpError::TypeError("bad rodata id".into()))
                })?;
                Ok(Value::new(
                    Type::U64,
                    u64::try_from(blob.bytes.len()).expect("length fits"),
                ))
            }
            Expr::Local(id) => {
                let ty = function
                    .local_ty(*id)
                    .ok_or_else(|| FlowError::Error(InterpError::TypeError("bad local".into())))?;
                let address = *frame
                    .locals
                    .get(id.0)
                    .ok_or_else(|| FlowError::Error(InterpError::TypeError("bad local".into())))?;
                self.read_scalar(address, ty)
            }
            Expr::AddrOfLocal(id) => {
                let ty = function
                    .local_ty(*id)
                    .ok_or_else(|| FlowError::Error(InterpError::TypeError("bad local".into())))?;
                let address = *frame
                    .locals
                    .get(id.0)
                    .ok_or_else(|| FlowError::Error(InterpError::TypeError("bad local".into())))?;
                Ok(Value::new(Type::Ptr(Box::new(ty.clone())), address))
            }
            Expr::GlobalAddr(id) => {
                let global = module
                    .globals
                    .get(id.0)
                    .ok_or_else(|| FlowError::Error(InterpError::TypeError("bad global".into())))?;
                let offset = *self
                    .global_offsets
                    .get(id.0)
                    .ok_or_else(|| FlowError::Error(InterpError::TypeError("bad global".into())))?;
                Ok(Value::new(
                    Type::Ptr(Box::new(global.ty.clone())),
                    DATA_BASE + offset,
                ))
            }
            Expr::FnAddr(id) => {
                let target = module.functions.get(id.0).ok_or_else(|| {
                    FlowError::Error(InterpError::TypeError("bad function id".into()))
                })?;
                let address = *self.fn_addresses.get(id.0).ok_or_else(|| {
                    FlowError::Error(InterpError::TypeError("bad function id".into()))
                })?;
                Ok(Value::new(
                    Type::FnPtr {
                        params: target.params.iter().map(|p| p.ty.clone()).collect(),
                        ret: Box::new(target.ret.clone()),
                    },
                    address,
                ))
            }
            Expr::SizeOf(ty) => Ok(Value::new(
                Type::U64,
                u64::try_from(module.size_of(ty)).expect("size fits"),
            )),
            Expr::AlignOf(ty) => Ok(Value::new(
                Type::U64,
                u64::try_from(module.align_of(ty)).expect("alignment fits"),
            )),
            Expr::Load { ptr, ty } => {
                let address = self.eval(module, function, frame, ptr)?;
                self.read_scalar(address.bits, ty)
            }
            Expr::FieldAddr {
                base_ptr,
                struct_id,
                field_idx,
            } => {
                let base = self.eval(module, function, frame, base_ptr)?;
                let offset = module
                    .try_field_offset(*struct_id, *field_idx)
                    .ok_or_else(|| {
                        FlowError::Error(InterpError::TypeError("bad struct field".into()))
                    })?;
                let field = module.structs[struct_id.0].fields[*field_idx].clone();
                Ok(Value::new(
                    Type::Ptr(Box::new(field)),
                    base.bits
                        .wrapping_add(u64::try_from(offset).expect("offset fits")),
                ))
            }
            Expr::UnionFieldAddr {
                base_ptr,
                union_id,
                field_idx,
            } => {
                let base = self.eval(module, function, frame, base_ptr)?;
                let field = module
                    .unions
                    .get(union_id.0)
                    .and_then(|def| def.fields.get(*field_idx))
                    .ok_or_else(|| {
                        FlowError::Error(InterpError::TypeError("bad union field".into()))
                    })?
                    .clone();
                Ok(Value::new(Type::Ptr(Box::new(field)), base.bits))
            }
            Expr::IndexAddr {
                base_ptr,
                elem_ty,
                index,
            } => {
                let base = self.eval(module, function, frame, base_ptr)?;
                let index = self.eval(module, function, frame, index)?;
                let stride = u64::try_from(module.size_of(elem_ty)).expect("size fits");
                Ok(Value::new(
                    Type::Ptr(Box::new(elem_ty.clone())),
                    base.bits.wrapping_add(index.bits.wrapping_mul(stride)),
                ))
            }
            Expr::Unary { op, val } => {
                let value = self.eval(module, function, frame, val)?;
                self.unary(*op, value)
            }
            Expr::Binary { op, lhs, rhs } => {
                let left = self.eval(module, function, frame, lhs)?;
                if *op == BinOp::LogAnd {
                    if !left.truthy() {
                        return Ok(Value::bool(false));
                    }
                    let right = self.eval(module, function, frame, rhs)?;
                    return Ok(Value::bool(right.truthy()));
                }
                if *op == BinOp::LogOr {
                    if left.truthy() {
                        return Ok(Value::bool(true));
                    }
                    let right = self.eval(module, function, frame, rhs)?;
                    return Ok(Value::bool(right.truthy()));
                }
                let right = self.eval(module, function, frame, rhs)?;
                self.binary(*op, left, right)
            }
            Expr::Cast { val, to } => {
                let value = self.eval(module, function, frame, val)?;
                self.cast(value, to)
            }
            Expr::Call { target, args } => {
                let callee = match target {
                    CallTarget::Direct(id) => *id,
                    CallTarget::Indirect(expr) => {
                        let target = self.eval(module, function, frame, expr)?;
                        let index = self
                            .fn_addresses
                            .iter()
                            .position(|address| *address == target.bits)
                            .ok_or(FlowError::Error(InterpError::BadFunctionPointer(
                                target.bits,
                            )))?;
                        crate::ir::FnId(index)
                    }
                };
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(self.eval(module, function, frame, arg)?);
                }
                let result = self.call_function(module, callee.0, &values)?;
                let ret = module
                    .functions
                    .get(callee.0)
                    .map_or(Type::Void, |f| f.ret.clone());
                Ok(result.unwrap_or_else(|| Value::zero(&ret)))
            }
            Expr::Syscall { num, args } => {
                let number = self.eval(module, function, frame, num)?;
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(self.eval(module, function, frame, arg)?);
                }
                self.syscall(number.bits, &values)
            }
        }
    }

    fn unary(&self, op: UnOp, value: Value) -> FlowResult<Value> {
        match op {
            UnOp::Neg => match value.ty {
                Type::F32 => Ok(Value::new(
                    Type::F32,
                    u64::from((-value.as_f32()).to_bits()),
                )),
                Type::F64 => Ok(Value::new(
                    Type::F64,
                    (-f64::from_bits(value.bits)).to_bits(),
                )),
                _ => Ok(Value::int(&value.ty, value.bits.wrapping_neg())),
            },
            UnOp::BitNot => Ok(Value::int(&value.ty, !value.bits)),
            UnOp::LogNot => Ok(Value::bool(!value.truthy())),
        }
    }

    fn binary(&self, op: BinOp, left: Value, right: Value) -> FlowResult<Value> {
        if left.ty.is_float() {
            if left.ty == Type::F32 {
                let lhs = left.as_f32();
                let rhs = right.as_f32();
                let result = match op {
                    BinOp::Add => lhs + rhs,
                    BinOp::Sub => lhs - rhs,
                    BinOp::Mul => lhs * rhs,
                    BinOp::Div => lhs / rhs,
                    _ => {
                        let truth = match op {
                            BinOp::Eq => lhs == rhs,
                            BinOp::Ne => lhs != rhs,
                            BinOp::Lt => lhs < rhs,
                            BinOp::Le => lhs <= rhs,
                            BinOp::Gt => lhs > rhs,
                            BinOp::Ge => lhs >= rhs,
                            _ => {
                                return Err(FlowError::Error(InterpError::TypeError(
                                    "unsupported f32 operation".into(),
                                )));
                            }
                        };
                        return Ok(Value::bool(truth));
                    }
                };
                return Ok(Value::new(Type::F32, u64::from(result.to_bits())));
            }
            let lhs = f64::from_bits(left.bits);
            let rhs = f64::from_bits(right.bits);
            let result = match op {
                BinOp::Add => lhs + rhs,
                BinOp::Sub => lhs - rhs,
                BinOp::Mul => lhs * rhs,
                BinOp::Div => lhs / rhs,
                _ => {
                    let truth = match op {
                        BinOp::Eq => lhs == rhs,
                        BinOp::Ne => lhs != rhs,
                        BinOp::Lt => lhs < rhs,
                        BinOp::Le => lhs <= rhs,
                        BinOp::Gt => lhs > rhs,
                        BinOp::Ge => lhs >= rhs,
                        _ => {
                            return Err(FlowError::Error(InterpError::TypeError(
                                "unsupported f64 operation".into(),
                            )));
                        }
                    };
                    return Ok(Value::bool(truth));
                }
            };
            return Ok(Value::new(Type::F64, result.to_bits()));
        }

        let ty = &left.ty;
        if matches!(
            op,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
        ) {
            let truth = compare_ints(op, ty, left.bits, right.bits);
            return Ok(Value::bool(truth));
        }

        let signed = ty.is_signed_int();
        let result = match op {
            BinOp::Add => left.bits.wrapping_add(right.bits),
            BinOp::Sub => left.bits.wrapping_sub(right.bits),
            BinOp::Mul => left.bits.wrapping_mul(right.bits),
            BinOp::Div => {
                if right.bits == 0 {
                    return Err(FlowError::Error(InterpError::DivisionByZero));
                }
                if signed {
                    (left.bits as i64).wrapping_div(right.bits as i64) as u64
                } else {
                    left.bits / right.bits
                }
            }
            BinOp::Mod => {
                if right.bits == 0 {
                    return Err(FlowError::Error(InterpError::DivisionByZero));
                }
                if signed {
                    (left.bits as i64).wrapping_rem(right.bits as i64) as u64
                } else {
                    left.bits % right.bits
                }
            }
            BinOp::BitAnd => left.bits & right.bits,
            BinOp::BitOr => left.bits | right.bits,
            BinOp::BitXor => left.bits ^ right.bits,
            BinOp::Shl => left.bits.wrapping_shl((right.bits & 63) as u32),
            BinOp::Shr => {
                if signed {
                    ((left.bits as i64) >> (right.bits & 63)) as u64
                } else {
                    left.bits >> (right.bits & 63)
                }
            }
            _ => {
                return Err(FlowError::Error(InterpError::TypeError(
                    "unsupported integer operation".into(),
                )));
            }
        };
        Ok(Value::int(ty, result))
    }

    fn cast(&self, value: Value, to: &Type) -> FlowResult<Value> {
        let from = &value.ty;
        if from.is_integer() && to.is_integer() {
            return Ok(Value::int(to, value.bits));
        }
        if from.is_integer() && to.is_float() {
            let as_float = if from.is_signed_int() {
                (value.bits as i64) as f64
            } else {
                value.bits as f64
            };
            return Ok(match to {
                Type::F32 => Value::new(Type::F32, u64::from((as_float as f32).to_bits())),
                _ => Value::new(Type::F64, as_float.to_bits()),
            });
        }
        if from.is_float() && to.is_integer() {
            // Match Rust's saturating float-to-integer semantics at the
            // target width.  Converting through i64 first would truncate
            // rather than saturate for i8/i16/i32/u8/u16/u32.
            let float = value.as_f64();
            let raw = match to {
                Type::I8 => (float as i8) as u64,
                Type::I16 => (float as i16) as u64,
                Type::I32 => (float as i32) as u64,
                Type::I64 => (float as i64) as u64,
                Type::U8 => (float as u8) as u64,
                Type::U16 => (float as u16) as u64,
                Type::U32 => (float as u32) as u64,
                Type::U64 => float as u64,
                _ => 0,
            };
            return Ok(Value::int(to, raw));
        }
        match (from, to) {
            (Type::F32, Type::F64) => {
                let widened = f64::from(value.as_f32());
                Ok(Value::new(Type::F64, widened.to_bits()))
            }
            (Type::F64, Type::F32) => {
                let narrowed = f64::from_bits(value.bits) as f32;
                Ok(Value::new(Type::F32, u64::from(narrowed.to_bits())))
            }
            (Type::Ptr(_), Type::Ptr(_))
            | (Type::Ptr(_), Type::U64)
            | (Type::U64, Type::Ptr(_))
            | (Type::FnPtr { .. }, Type::U64)
            | (Type::U64, Type::FnPtr { .. }) => Ok(Value::new(to.clone(), value.bits)),
            _ => Err(FlowError::Error(InterpError::TypeError(format!(
                "unsupported cast {from:?} -> {to:?}"
            )))),
        }
    }

    fn syscall(&mut self, number: u64, args: &[Value]) -> FlowResult<Value> {
        let arg = |index: usize| args.get(index).map_or(0, |value| value.bits);
        match number {
            0 => {
                let fd = arg(0);
                if fd != 0 {
                    return Ok(Value::new(Type::I64, (-9i64) as u64));
                }
                let address = arg(1);
                let count = usize::try_from(arg(2)).unwrap_or(usize::MAX);
                let available = self.stdin.len().saturating_sub(self.stdin_cursor);
                let read = count.min(available);
                if read != 0 {
                    let bytes = self.stdin[self.stdin_cursor..self.stdin_cursor + read].to_vec();
                    self.memory.write(address, &bytes)?;
                    self.stdin_cursor += read;
                }
                Ok(Value::new(Type::I64, read as u64))
            }
            1 => {
                let fd = arg(0);
                if fd != 1 && fd != 2 {
                    return Ok(Value::new(Type::I64, (-9i64) as u64));
                }
                let address = arg(1);
                let count = usize::try_from(arg(2)).unwrap_or(usize::MAX);
                let bytes = self.memory.read(address, count)?;
                if fd == 1 {
                    self.stdout.extend_from_slice(&bytes);
                } else {
                    self.stderr.extend_from_slice(&bytes);
                }
                Ok(Value::new(Type::I64, count as u64))
            }
            9 => {
                let length = usize::try_from(arg(1)).unwrap_or(usize::MAX);
                if length == 0 || length > HEAP_LIMIT {
                    return Ok(Value::new(Type::I64, (-12i64) as u64));
                }
                let padded = round_up(length, PAGE_SIZE);
                match self.memory.alloc_heap(padded) {
                    Ok(address) => Ok(Value::new(Type::I64, address)),
                    Err(_) => Ok(Value::new(Type::I64, (-12i64) as u64)),
                }
            }
            11 => {
                let length = usize::try_from(arg(1)).unwrap_or(usize::MAX);
                self.memory.unmap_heap(arg(0), length);
                Ok(Value::new(Type::I64, 0))
            }
            60 => Err(FlowError::Exit((arg(0) & 0xff) as i32)),
            _ => Ok(Value::new(Type::I64, (-38i64) as u64)),
        }
    }
}

fn compare_ints(op: BinOp, ty: &Type, left: u64, right: u64) -> bool {
    let signed = ty.is_signed_int();
    match op {
        BinOp::Eq => left == right,
        BinOp::Ne => left != right,
        BinOp::Lt => {
            if signed {
                (left as i64) < (right as i64)
            } else {
                left < right
            }
        }
        BinOp::Le => {
            if signed {
                (left as i64) <= (right as i64)
            } else {
                left <= right
            }
        }
        BinOp::Gt => {
            if signed {
                (left as i64) > (right as i64)
            } else {
                left > right
            }
        }
        BinOp::Ge => {
            if signed {
                (left as i64) >= (right as i64)
            } else {
                left >= right
            }
        }
        _ => false,
    }
}

fn entry_arguments(vm: &mut Vm<'_>, args: &[String]) -> Result<Vec<Value>, InterpError> {
    let mut pointers = Vec::with_capacity(args.len());
    for arg in args {
        let mut bytes = arg.as_bytes().to_vec();
        bytes.push(0);
        let address = vm.memory.alloc_stack(bytes.len(), 1)?;
        vm.memory.write(address, &bytes)?;
        pointers.push(address);
    }

    let argv_address = vm.memory.alloc_stack((pointers.len() + 1) * 8, 8)?;
    for (index, pointer) in pointers.iter().enumerate() {
        vm.memory.write_uint(
            argv_address + u64::try_from(index).expect("index fits") * 8,
            8,
            *pointer,
        )?;
    }
    vm.memory.write_uint(
        argv_address + u64::try_from(pointers.len()).expect("length fits") * 8,
        8,
        0,
    )?;

    let envp_address = vm.memory.alloc_stack(8, 8)?;
    vm.memory.write_uint(envp_address, 8, 0)?;

    Ok(vec![
        Value::new(Type::I64, u64::try_from(args.len()).expect("argc fits")),
        Value::new(
            Type::Ptr(Box::new(Type::Ptr(Box::new(Type::U8)))),
            argv_address,
        ),
        Value::new(
            Type::Ptr(Box::new(Type::Ptr(Box::new(Type::U8)))),
            envp_address,
        ),
    ])
}

/// Runs `module.entry` with `args` as `argv` (including `argv[0]`).
///
/// `stdin` supplies bytes for syscall 0, and `max_steps` bounds the total
/// expression/statement budget.
///
/// # Errors
///
/// Returns [`InterpError`] when verification, memory access, trap handling, or
/// execution fails.
pub fn interpret(
    module: &Module,
    args: &[String],
    stdin: &[u8],
    max_steps: usize,
) -> Result<InterpOutput, InterpError> {
    std::thread::scope(|scope| {
        let handle = std::thread::Builder::new()
            .name("tc-ir-interp".to_string())
            .stack_size(64 * 1024 * 1024)
            .spawn_scoped(scope, || interpret_inner(module, args, stdin, max_steps))
            .map_err(|error| {
                InterpError::Internal(format!("failed to start interpreter thread: {error}"))
            })?;
        handle
            .join()
            .map_err(|_| InterpError::Internal("interpreter thread panicked".to_string()))?
    })
}

fn interpret_inner(
    module: &Module,
    args: &[String],
    stdin: &[u8],
    max_steps: usize,
) -> Result<InterpOutput, InterpError> {
    if let Err(errors) = verify_module(module) {
        return Err(InterpError::InvalidModule(
            errors
                .into_iter()
                .map(|error| format!("{error:?}"))
                .collect(),
        ));
    }
    let entry = module
        .functions
        .get(module.entry.0)
        .ok_or_else(|| InterpError::TypeError("entry function out of bounds".into()))?;
    let mut vm = Vm::new(module, stdin, max_steps);
    let arguments = if entry.params.is_empty() {
        Vec::new()
    } else {
        entry_arguments(&mut vm, args)?
    };

    let result = vm.call_function(module, module.entry.0, &arguments);
    let (exit_code, stdout, stderr) = match result {
        Ok(value) => {
            let code = value.map_or(0, |value| (value.bits & 0xff) as i32);
            (code, vm.stdout, vm.stderr)
        }
        Err(FlowError::Exit(code)) => (code, vm.stdout, vm.stderr),
        Err(FlowError::Error(error)) => return Err(error),
    };
    Ok(InterpOutput {
        exit_code,
        stdout,
        stderr,
    })
}
