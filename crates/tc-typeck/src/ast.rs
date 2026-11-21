//! High-level typed AST, spans, and diagnostics for the Phase 1 front end.

use tc_ir::{BinOp, UnOp};

/// Source location used in diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Span {
    /// Source module name.
    pub module: String,
    /// One-based line number.
    pub line: u32,
    /// One-based column number.
    pub col: u32,
}

impl Span {
    /// Creates a new source span.
    #[must_use]
    pub fn new(module: impl Into<String>, line: u32, col: u32) -> Self {
        Self {
            module: module.into(),
            line,
            col,
        }
    }
}

impl std::fmt::Display for Span {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}:{}:{}", self.module, self.line, self.col)
    }
}

/// Item visibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Visibility {
    /// Accessible from other modules.
    Public,
    /// Restricted to the defining module.
    Private,
}

/// One part of a declarative English phrase pattern.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PhrasePart {
    /// A canonical lowercase keyword or preposition.
    Word(String),
    /// A function parameter name.
    Param(String),
}

/// A declarative English phrase signature attached to a function.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PhrasePattern {
    /// Ordered phrase parts.
    pub parts: Vec<PhrasePart>,
}

/// A type as written in the high-level AST.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AstType {
    /// No value.
    Void,
    /// Boolean.
    Bool,
    /// Signed 8-bit integer.
    I8,
    /// Signed 16-bit integer.
    I16,
    /// Signed 32-bit integer.
    I32,
    /// Signed 64-bit integer.
    I64,
    /// Unsigned 8-bit integer.
    U8,
    /// Unsigned 16-bit integer.
    U16,
    /// Unsigned 32-bit integer.
    U32,
    /// Unsigned 64-bit integer.
    U64,
    /// 32-bit IEEE-754 float.
    F32,
    /// 64-bit IEEE-754 float.
    F64,
    /// Pointer type.
    Ptr(Box<AstType>),
    /// Fixed-size array type.
    Array(Box<AstType>, usize),
    /// Named generic parameter or user-defined type.
    Named {
        /// Optional module qualifier or alias.
        module: Option<String>,
        /// Type name.
        name: String,
        /// Generic type arguments.
        type_args: Vec<AstType>,
    },
    /// Function pointer type.
    FnPtr {
        /// Parameter types.
        params: Vec<AstType>,
        /// Return type.
        ret: Box<AstType>,
    },
}

impl AstType {
    /// Creates a pointer type.
    #[must_use]
    pub fn ptr(inner: AstType) -> Self {
        Self::Ptr(Box::new(inner))
    }

    /// Creates an array type.
    #[must_use]
    pub fn array(inner: AstType, count: usize) -> Self {
        Self::Array(Box::new(inner), count)
    }

    /// Creates an unqualified named type.
    #[must_use]
    pub fn named(name: impl Into<String>) -> Self {
        Self::Named {
            module: None,
            name: name.into(),
            type_args: Vec::new(),
        }
    }
}

/// A named struct declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstStruct {
    /// Visibility.
    pub vis: Visibility,
    /// Struct name.
    pub name: String,
    /// Type parameter names.
    pub type_params: Vec<String>,
    /// Fields in declaration order.
    pub fields: Vec<(String, AstType)>,
    /// Declaration span.
    pub span: Span,
}

/// A named union declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstUnion {
    /// Visibility.
    pub vis: Visibility,
    /// Union name.
    pub name: String,
    /// Type parameter names.
    pub type_params: Vec<String>,
    /// Members in declaration order.
    pub fields: Vec<(String, AstType)>,
    /// Declaration span.
    pub span: Span,
}

/// A static global declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstGlobal {
    /// Visibility.
    pub vis: Visibility,
    /// Global name.
    pub name: String,
    /// Global type.
    pub ty: AstType,
    /// Exact initialized bytes, or `None` for zero-initialized storage.
    pub init: Option<Vec<u8>>,
    /// Declaration span.
    pub span: Span,
}

/// A function declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct AstFn {
    /// Visibility.
    pub vis: Visibility,
    /// Function name.
    pub name: String,
    /// Generic type parameter names.
    pub type_params: Vec<String>,
    /// Parameters in declaration order.
    pub params: Vec<(String, AstType)>,
    /// Return type.
    pub ret: AstType,
    /// Declarative English phrase signatures.
    pub phrases: Vec<PhrasePattern>,
    /// Function body.
    pub body: Vec<AstStmt>,
    /// Declaration span.
    pub span: Span,
}

/// A module import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    /// Imported module name.
    pub module: String,
    /// Optional local alias.
    pub alias: Option<String>,
    /// Import span.
    pub span: Span,
}

/// A source module.
#[derive(Debug, Clone, PartialEq)]
pub struct AstModule {
    /// Module name.
    pub name: String,
    /// Imports.
    pub imports: Vec<Import>,
    /// Structs.
    pub structs: Vec<AstStruct>,
    /// Unions.
    pub unions: Vec<AstUnion>,
    /// Globals.
    pub globals: Vec<AstGlobal>,
    /// Functions.
    pub functions: Vec<AstFn>,
}

/// A complete multi-file program.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    /// All source modules.
    pub modules: Vec<AstModule>,
    /// Entry module name.
    pub entry_module: String,
    /// Entry function name.
    pub entry_fn: String,
}

/// A validated declarative phrase signature exported for Phase 3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhraseEntry {
    /// Defining module.
    pub module: String,
    /// Function name.
    pub function: String,
    /// Validated phrase pattern.
    pub pattern: PhrasePattern,
}

/// A statement in the high-level AST.
#[derive(Debug, Clone, PartialEq)]
pub enum AstStmt {
    /// Introduce a local variable.
    Let {
        /// Variable name.
        name: String,
        /// Optional declared type.
        ty: Option<AstType>,
        /// Initializer.
        init: AstExpr,
        /// Source span.
        span: Span,
    },
    /// Assign to an existing variable.
    Assign {
        /// Variable name.
        name: String,
        /// New value.
        val: AstExpr,
        /// Source span.
        span: Span,
    },
    /// Store a value through a pointer.
    Store {
        /// Destination pointer.
        ptr: AstExpr,
        /// Value.
        val: AstExpr,
        /// Source span.
        span: Span,
    },
    /// Byte copy.
    MemCopy {
        /// Destination pointer.
        dst: AstExpr,
        /// Source pointer.
        src: AstExpr,
        /// Byte count.
        size: AstExpr,
        /// Source span.
        span: Span,
    },
    /// Expression evaluated for side effects.
    Expr(AstExpr, Span),
    /// Nested lexical block.
    Block(Vec<AstStmt>, Span),
    /// Conditional.
    If {
        /// Boolean condition.
        cond: AstExpr,
        /// True branch.
        then_body: Vec<AstStmt>,
        /// False branch.
        else_body: Vec<AstStmt>,
        /// Source span.
        span: Span,
    },
    /// Unconditional loop.
    Loop {
        /// Loop body.
        body: Vec<AstStmt>,
        /// Source span.
        span: Span,
    },
    /// Break out of the innermost loop.
    Break(Span),
    /// Continue the innermost loop.
    Continue(Span),
    /// Return from the function.
    Return(Option<AstExpr>, Span),
    /// Trap.
    Trap(Span),
}

/// An expression in the high-level AST.
#[derive(Debug, Clone, PartialEq)]
pub enum AstExpr {
    /// Signed integer literal with optional type annotation.
    ConstInt(i64, Option<AstType>),
    /// Unsigned integer literal with optional type annotation.
    ConstUint(u64, Option<AstType>),
    /// Floating-point literal with optional type annotation.
    ConstFloat(f64, Option<AstType>),
    /// Boolean literal.
    ConstBool(bool),
    /// Typed null pointer.
    ConstNull(AstType),
    /// Read-only byte literal pointer.
    BytesLit(Vec<u8>),
    /// Read-only byte literal length.
    BytesLen(Vec<u8>),
    /// Variable reference.
    Var(String),
    /// Address of a global object.
    GlobalAddr {
        /// Optional module qualifier.
        module: Option<String>,
        /// Global name.
        name: String,
    },
    /// Address of a local variable.
    AddrOfVar(String),
    /// Address of a named struct/union field.
    FieldAddr {
        /// Pointer to the aggregate.
        base_ptr: Box<AstExpr>,
        /// Field or member name.
        field: String,
    },
    /// Address of an array element.
    IndexAddr {
        /// Pointer to the first element or array.
        base_ptr: Box<AstExpr>,
        /// Element index.
        index: Box<AstExpr>,
    },
    /// Dereference a pointer.
    Load(Box<AstExpr>),
    /// Size of a type.
    SizeOf(AstType),
    /// Alignment of a type.
    AlignOf(AstType),
    /// Unary operation.
    Unary {
        /// Operator.
        op: UnOp,
        /// Operand.
        val: Box<AstExpr>,
    },
    /// Binary operation.
    Binary {
        /// Operator.
        op: BinOp,
        /// Left operand.
        lhs: Box<AstExpr>,
        /// Right operand.
        rhs: Box<AstExpr>,
    },
    /// Type conversion.
    Cast {
        /// Source expression.
        val: Box<AstExpr>,
        /// Destination type.
        to: AstType,
    },
    /// Address of a function.
    FnAddr {
        /// Optional module qualifier.
        module: Option<String>,
        /// Function name.
        name: String,
        /// Explicit generic type arguments.
        type_args: Vec<AstType>,
    },
    /// Direct function call.
    Call {
        /// Optional module qualifier.
        module: Option<String>,
        /// Function name.
        name: String,
        /// Explicit generic type arguments.
        type_args: Vec<AstType>,
        /// Arguments.
        args: Vec<AstExpr>,
    },
    /// Indirect function call.
    IndirectCall {
        /// Function pointer expression.
        callee: Box<AstExpr>,
        /// Arguments.
        args: Vec<AstExpr>,
    },
    /// Raw Linux syscall.
    Syscall {
        /// Syscall number.
        num: Box<AstExpr>,
        /// Arguments.
        args: Vec<AstExpr>,
    },
}

/// Structured diagnostic categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeErrorKind {
    /// Two modules share a name.
    DuplicateModule,
    /// A module alias is declared twice.
    DuplicateModuleAlias,
    /// An imported module does not exist.
    MissingModule,
    /// The entry module does not exist.
    MissingEntryModule,
    /// The entry function does not exist.
    MissingEntryFunction,
    /// An import cycle was found.
    CircularImport,
    /// Two items in the same namespace share a name.
    DuplicateItem,
    /// A private item was accessed from another module.
    PrivateAccess,
    /// A variable is not in scope.
    UndefinedVariable,
    /// A type name could not be resolved.
    UndefinedType,
    /// A function name could not be resolved.
    UndefinedFunction,
    /// A global name could not be resolved.
    UndefinedGlobal,
    /// A struct/union field does not exist.
    UndefinedField,
    /// An unqualified item name is ambiguous across imports.
    AmbiguousItem,
    /// A generic type or function received the wrong number of arguments.
    WrongGenericArity,
    /// Operand or value types do not match.
    TypeMismatch,
    /// No overload accepts the call arguments.
    NoMatchingOverload,
    /// More than one overload matches equally well.
    AmbiguousCall,
    /// A generic parameter cannot be inferred and was not supplied.
    UnconstrainedGeneric,
    /// Generic monomorphization exceeded the depth guard.
    InfiniteGenericInstantiation,
    /// A phrase pattern refers to an unknown parameter.
    InvalidPhraseParam,
    /// A phrase pattern binds the same parameter twice.
    DuplicatePhraseParam,
    /// `break` appeared outside a loop.
    InvalidBreak,
    /// `continue` appeared outside a loop.
    InvalidContinue,
    /// A return statement does not match the function signature.
    InvalidReturn,
    /// The entry function has an invalid signature.
    InvalidEntrySignature,
    /// An operation is not valid for the supplied types.
    InvalidOperation,
    /// Internal lowering invariant failed.
    Internal,
}

/// A structured type-checking diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeError {
    /// Source location.
    pub span: Span,
    /// Diagnostic category.
    pub kind: TypeErrorKind,
    /// Human-readable explanation.
    pub message: String,
}

impl TypeError {
    /// Creates a new diagnostic.
    #[must_use]
    pub fn new(span: Span, kind: TypeErrorKind, message: impl Into<String>) -> Self {
        Self {
            span,
            kind,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for TypeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}: {:?}: {}",
            self.span, self.kind, self.message
        )
    }
}

impl std::error::Error for TypeError {}
