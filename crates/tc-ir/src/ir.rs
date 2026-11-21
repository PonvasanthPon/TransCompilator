//! Core-7 typed intermediate representation nodes.

use crate::types::Type;

macro_rules! define_id {
    ($name:ident) => {
        /// Typed index into the corresponding module table.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub usize);

        impl $name {
            /// Creates an identifier from a table index.
            #[must_use]
            pub const fn new(index: usize) -> Self {
                Self(index)
            }

            /// Returns the underlying table index.
            #[must_use]
            pub const fn index(self) -> usize {
                self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "{}", self.0)
            }
        }
    };
}

define_id!(FnId);
define_id!(LocalId);
define_id!(GlobalId);
define_id!(RodataId);
define_id!(StructId);
define_id!(UnionId);

/// A named struct field layout definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructDef {
    /// Human-readable name used by the textual formatter.
    pub name: String,
    /// Field types in declaration order.
    pub fields: Vec<Type>,
}

/// A named union layout definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnionDef {
    /// Human-readable name used by the textual formatter.
    pub name: String,
    /// Member types, all placed at offset zero.
    pub fields: Vec<Type>,
}

/// A local variable or function parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalDef {
    /// Human-readable name used by the textual formatter.
    pub name: String,
    /// Storage type of the local.
    pub ty: Type,
}

/// Read-only bytes emitted into `.rodata`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RodataDef {
    /// Human-readable name used by the textual formatter.
    pub name: String,
    /// Exact byte contents.
    pub bytes: Vec<u8>,
}

/// Static storage emitted into `.data` (initialized) or `.bss` (zeroed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalDef {
    /// Human-readable name used by the textual formatter.
    pub name: String,
    /// Type of the global object.
    pub ty: Type,
    /// Exact initialized bytes, or `None` for zero-initialized `.bss`.
    pub init: Option<Vec<u8>>,
}

/// A function definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    /// Human-readable name used by the textual formatter.
    pub name: String,
    /// Parameters in declaration order.
    pub params: Vec<LocalDef>,
    /// Return type, or [`Type::Void`].
    pub ret: Type,
    /// Non-parameter locals.
    pub locals: Vec<LocalDef>,
    /// Function body.
    pub body: Vec<Stmt>,
}

impl Function {
    /// Returns the total number of addressable locals, including parameters.
    #[must_use]
    pub fn local_count(&self) -> usize {
        self.params.len() + self.locals.len()
    }

    /// Returns a local definition by combined parameter/local index.
    #[must_use]
    pub fn local_def(&self, id: LocalId) -> Option<&LocalDef> {
        if id.0 < self.params.len() {
            self.params.get(id.0)
        } else {
            self.locals.get(id.0 - self.params.len())
        }
    }

    /// Returns the type of a local by combined parameter/local index.
    #[must_use]
    pub fn local_ty(&self, id: LocalId) -> Option<&Type> {
        self.local_def(id).map(|local| &local.ty)
    }

    /// Iterates over parameters followed by ordinary locals.
    pub fn local_defs(&self) -> impl Iterator<Item = &LocalDef> + '_ {
        self.params.iter().chain(self.locals.iter())
    }
}

/// A complete Core-7 module.
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    /// Struct layout table.
    pub structs: Vec<StructDef>,
    /// Union layout table.
    pub unions: Vec<UnionDef>,
    /// Read-only static byte blobs.
    pub rodata: Vec<RodataDef>,
    /// Static mutable storage.
    pub globals: Vec<GlobalDef>,
    /// Function definitions.
    pub functions: Vec<Function>,
    /// Entry function (the program's `main`).
    pub entry: FnId,
}

impl Module {
    /// Creates an empty module with the given entry function.
    #[must_use]
    pub fn new(entry: FnId) -> Self {
        Self {
            structs: Vec::new(),
            unions: Vec::new(),
            rodata: Vec::new(),
            globals: Vec::new(),
            functions: Vec::new(),
            entry,
        }
    }
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    /// Arithmetic negation.
    Neg,
    /// Bitwise complement.
    BitNot,
    /// Logical negation (boolean only).
    LogNot,
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    /// Addition.
    Add,
    /// Subtraction.
    Sub,
    /// Multiplication.
    Mul,
    /// Division.
    Div,
    /// Remainder.
    Mod,
    /// Bitwise AND.
    BitAnd,
    /// Bitwise OR.
    BitOr,
    /// Bitwise XOR.
    BitXor,
    /// Left shift.
    Shl,
    /// Right shift.
    Shr,
    /// Equality comparison.
    Eq,
    /// Inequality comparison.
    Ne,
    /// Less-than comparison.
    Lt,
    /// Less-than-or-equal comparison.
    Le,
    /// Greater-than comparison.
    Gt,
    /// Greater-than-or-equal comparison.
    Ge,
    /// Short-circuiting logical AND.
    LogAnd,
    /// Short-circuiting logical OR.
    LogOr,
}

/// A call target.
#[derive(Debug, Clone, PartialEq)]
pub enum CallTarget {
    /// Direct call to a known function.
    Direct(FnId),
    /// Indirect call through a function pointer expression.
    Indirect(Box<Expr>),
}

/// A Core-7 expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Signed integer constant.
    ConstInt(i64, Type),
    /// Unsigned integer constant.
    ConstUint(u64, Type),
    /// Floating-point constant.
    ConstFloat(f64, Type),
    /// Boolean constant.
    ConstBool(bool),
    /// Typed null pointer.
    ConstNull(Type),
    /// Address of a read-only byte blob.
    RodataPtr(RodataId),
    /// Length of a read-only byte blob.
    RodataLen(RodataId),
    /// Read a scalar, pointer, or function-pointer local.
    Local(LocalId),
    /// Address of any local, including aggregates.
    AddrOfLocal(LocalId),
    /// Address of a global.
    GlobalAddr(GlobalId),
    /// Address of a function as a function pointer.
    FnAddr(FnId),
    /// Size of a type in bytes.
    SizeOf(Type),
    /// Alignment of a type in bytes.
    AlignOf(Type),
    /// Load a scalar, pointer, or function pointer from memory.
    Load {
        /// Address expression.
        ptr: Box<Expr>,
        /// Loaded type.
        ty: Type,
    },
    /// Address of a struct field.
    FieldAddr {
        /// Pointer to the struct.
        base_ptr: Box<Expr>,
        /// Struct type identifier.
        struct_id: StructId,
        /// Field index.
        field_idx: usize,
    },
    /// Address of a union member.
    UnionFieldAddr {
        /// Pointer to the union.
        base_ptr: Box<Expr>,
        /// Union type identifier.
        union_id: UnionId,
        /// Member index.
        field_idx: usize,
    },
    /// Address of an array element.
    IndexAddr {
        /// Pointer to the first element.
        base_ptr: Box<Expr>,
        /// Element type.
        elem_ty: Type,
        /// Element index.
        index: Box<Expr>,
    },
    /// Unary operation.
    Unary {
        /// Operator.
        op: UnOp,
        /// Operand.
        val: Box<Expr>,
    },
    /// Binary operation.
    Binary {
        /// Operator.
        op: BinOp,
        /// Left operand.
        lhs: Box<Expr>,
        /// Right operand.
        rhs: Box<Expr>,
    },
    /// Type conversion.
    Cast {
        /// Source expression.
        val: Box<Expr>,
        /// Destination type.
        to: Type,
    },
    /// Function call.
    Call {
        /// Direct or indirect target.
        target: CallTarget,
        /// Arguments in source order.
        args: Vec<Expr>,
    },
    /// Raw Linux system call.
    Syscall {
        /// Syscall number.
        num: Box<Expr>,
        /// Syscall arguments (0 through 6).
        args: Vec<Expr>,
    },
}

/// A Core-7 statement.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// Assign a scalar, pointer, or function pointer to a local.
    Assign {
        /// Destination local.
        local: LocalId,
        /// Value expression.
        val: Expr,
    },
    /// Store a scalar, pointer, or function pointer through a pointer.
    Store {
        /// Destination address.
        ptr: Expr,
        /// Stored type.
        ty: Type,
        /// Value expression.
        val: Expr,
    },
    /// Copy bytes between memory regions.
    MemCopy {
        /// Destination address.
        dst: Expr,
        /// Source address.
        src: Expr,
        /// Number of bytes.
        size: Expr,
    },
    /// Evaluate an expression for its side effects.
    Expr(Expr),
    /// Conditional branch.
    If {
        /// Boolean condition.
        cond: Expr,
        /// Body executed when true.
        then_body: Vec<Stmt>,
        /// Body executed when false.
        else_body: Vec<Stmt>,
    },
    /// Unconditional loop.
    Loop {
        /// Loop body.
        body: Vec<Stmt>,
    },
    /// Exit the innermost loop.
    Break,
    /// Continue the innermost loop.
    Continue,
    /// Return from the current function.
    Return(Option<Expr>),
    /// Emit `ud2` / halt interpretation with a trap.
    Trap,
}
