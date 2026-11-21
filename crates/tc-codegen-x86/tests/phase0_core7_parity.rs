//! Phase 0 differential parity tests: ELF64 vs reference IR interpreter.

use std::io::Write as _;
use std::os::unix::process::CommandExt as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use tc_codegen_x86::compile_to_elf;
use tc_ir::{
    BinOp, CallTarget, Expr, FnId, Function, GlobalDef, InterpError, LocalDef, LocalId, Module,
    RodataDef, Stmt, Type, UnOp, UnionDef, format_module, format_type, interpret, verify_module,
};
use tc_ir::{StructDef, StructId, UnionId};

static ELF_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct RunResult {
    exit_code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn unique_elf(tag: &str) -> PathBuf {
    let counter = ELF_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "tc_r1_parity_{}_{}_{tag}.elf",
        std::process::id(),
        counter
    ))
}

fn run_elf(path: &PathBuf, args: &[&str], stdin: &[u8]) -> RunResult {
    assert!(!args.is_empty(), "argv must include argv[0]");
    let mut child = Command::new(path)
        .arg0(args[0])
        .args(&args[1..])
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ELF");
    child
        .stdin
        .take()
        .expect("stdin pipe")
        .write_all(stdin)
        .expect("write stdin");
    let output = child.wait_with_output().expect("wait ELF");
    RunResult {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: output.stdout,
        stderr: output.stderr,
    }
}

fn assert_parity(
    module: &Module,
    args: &[&str],
    stdin: &[u8],
    expected_exit: i32,
    expected_stdout: &[u8],
) {
    verify_module(module).unwrap_or_else(|errors| panic!("verification failed: {errors:#?}"));
    let owned_args = args.iter().map(ToString::to_string).collect::<Vec<_>>();
    let interp = interpret(module, &owned_args, stdin, 10_000_000)
        .unwrap_or_else(|error| panic!("interpreter failed: {error:?}"));
    assert_eq!(
        interp.exit_code,
        expected_exit,
        "interpreter exit mismatch; stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&interp.stdout),
        String::from_utf8_lossy(&interp.stderr)
    );
    assert_eq!(
        interp.stdout,
        expected_stdout,
        "interpreter stdout mismatch; stderr={:?}",
        String::from_utf8_lossy(&interp.stderr)
    );

    let path = unique_elf("parity");
    compile_to_elf(module, &path).unwrap_or_else(|error| panic!("codegen failed: {error}"));
    let elf = run_elf(&path, args, stdin);
    let _ = std::fs::remove_file(&path);

    assert_eq!(
        elf.exit_code,
        interp.exit_code,
        "ELF/interpreter exit mismatch; elf stderr={:?}",
        String::from_utf8_lossy(&elf.stderr)
    );
    assert_eq!(
        elf.stdout,
        interp.stdout,
        "ELF/interpreter stdout mismatch; elf stderr={:?}",
        String::from_utf8_lossy(&elf.stderr)
    );
    assert_eq!(elf.stderr, interp.stderr, "ELF/interpreter stderr mismatch");
    assert_eq!(
        elf.exit_code,
        expected_exit,
        "ELF exit mismatch; stderr={:?}",
        String::from_utf8_lossy(&elf.stderr)
    );
    assert_eq!(elf.stdout, expected_stdout, "ELF stdout mismatch");
}

fn ptr(inner: Type) -> Type {
    Type::Ptr(Box::new(inner))
}

fn fnptr(params: Vec<Type>, ret: Type) -> Type {
    Type::FnPtr {
        params,
        ret: Box::new(ret),
    }
}

fn local_def(name: &str, ty: Type) -> LocalDef {
    LocalDef {
        name: name.to_string(),
        ty,
    }
}

fn func(
    name: &str,
    params: &[(&str, Type)],
    ret: Type,
    locals: &[(&str, Type)],
    body: Vec<Stmt>,
) -> Function {
    Function {
        name: name.to_string(),
        params: params
            .iter()
            .map(|(name, ty)| local_def(name, ty.clone()))
            .collect(),
        ret,
        locals: locals
            .iter()
            .map(|(name, ty)| local_def(name, ty.clone()))
            .collect(),
        body,
    }
}

fn module(functions: Vec<Function>) -> Module {
    Module {
        structs: Vec::new(),
        unions: Vec::new(),
        rodata: Vec::new(),
        globals: Vec::new(),
        functions,
        entry: FnId(0),
    }
}

fn c_i64(value: i64) -> Expr {
    Expr::ConstInt(value, Type::I64)
}

fn c_i32(value: i32) -> Expr {
    Expr::ConstInt(i64::from(value), Type::I32)
}

fn c_u64(value: u64) -> Expr {
    Expr::ConstUint(value, Type::U64)
}

fn c_bool(value: bool) -> Expr {
    Expr::ConstBool(value)
}

fn c_f64(value: f64) -> Expr {
    Expr::ConstFloat(value, Type::F64)
}

fn c_f32(value: f32) -> Expr {
    Expr::ConstFloat(f64::from(value), Type::F32)
}

fn c_int(value: i64, ty: Type) -> Expr {
    Expr::ConstInt(value, ty)
}

fn c_uint(value: u64, ty: Type) -> Expr {
    Expr::ConstUint(value, ty)
}

fn expect_f32(actual: Expr, expected: f32) -> Stmt {
    expect_expr_eq(actual, c_f32(expected))
}

fn local(id: usize) -> Expr {
    Expr::Local(LocalId(id))
}

fn addr_local(id: usize) -> Expr {
    Expr::AddrOfLocal(LocalId(id))
}

fn bin(op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
    Expr::Binary {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
    }
}

fn unary(op: UnOp, val: Expr) -> Expr {
    Expr::Unary {
        op,
        val: Box::new(val),
    }
}

fn cast(val: Expr, to: Type) -> Expr {
    Expr::Cast {
        val: Box::new(val),
        to,
    }
}

fn call(id: usize, args: Vec<Expr>) -> Expr {
    Expr::Call {
        target: CallTarget::Direct(FnId(id)),
        args,
    }
}

fn syscall(num: i64, args: Vec<Expr>) -> Expr {
    Expr::Syscall {
        num: Box::new(Expr::ConstInt(num, Type::I64)),
        args,
    }
}

fn assign(id: usize, val: Expr) -> Stmt {
    Stmt::Assign {
        local: LocalId(id),
        val,
    }
}

fn store(ptr: Expr, ty: Type, val: Expr) -> Stmt {
    Stmt::Store { ptr, ty, val }
}

fn expr(value: Expr) -> Stmt {
    Stmt::Expr(value)
}

fn if_(cond: Expr, then_body: Vec<Stmt>, else_body: Vec<Stmt>) -> Stmt {
    Stmt::If {
        cond,
        then_body,
        else_body,
    }
}

fn loop_(body: Vec<Stmt>) -> Stmt {
    Stmt::Loop { body }
}

fn ret(value: Expr) -> Stmt {
    Stmt::Return(Some(value))
}

fn ret_void() -> Stmt {
    Stmt::Return(None)
}

fn load(ptr: Expr, ty: Type) -> Expr {
    Expr::Load {
        ptr: Box::new(ptr),
        ty,
    }
}

fn field_addr(base: Expr, struct_id: usize, field_idx: usize) -> Expr {
    Expr::FieldAddr {
        base_ptr: Box::new(base),
        struct_id: StructId(struct_id),
        field_idx,
    }
}

fn union_field_addr(base: Expr, union_id: usize, field_idx: usize) -> Expr {
    Expr::UnionFieldAddr {
        base_ptr: Box::new(base),
        union_id: UnionId(union_id),
        field_idx,
    }
}

fn index_addr(base: Expr, elem_ty: Type, index: Expr) -> Expr {
    Expr::IndexAddr {
        base_ptr: Box::new(base),
        elem_ty,
        index: Box::new(index),
    }
}

fn trap_unless(condition: Expr) -> Stmt {
    if_(unary(UnOp::LogNot, condition), vec![Stmt::Trap], Vec::new())
}

fn expect_expr_eq(actual: Expr, expected: Expr) -> Stmt {
    trap_unless(bin(BinOp::Eq, actual, expected))
}

fn expect_i64(actual: Expr, expected: i64) -> Stmt {
    expect_expr_eq(actual, c_i64(expected))
}

fn expect_u64(actual: Expr, expected: u64) -> Stmt {
    expect_expr_eq(actual, c_u64(expected))
}

fn expect_f64(actual: Expr, expected: f64) -> Stmt {
    expect_expr_eq(actual, c_f64(expected))
}

fn hello_module() -> Module {
    let mut module = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[],
        vec![
            expr(syscall(
                1,
                vec![
                    c_i64(1),
                    Expr::RodataPtr(tc_ir::RodataId(0)),
                    Expr::RodataLen(tc_ir::RodataId(0)),
                ],
            )),
            ret(c_i64(0)),
        ],
    )]);
    module.rodata.push(RodataDef {
        name: "hello".to_string(),
        bytes: b"hello tc_r1\n".to_vec(),
    });
    module
}

#[test]
fn exit_zero() {
    let m = module(vec![func("main", &[], Type::I64, &[], vec![ret(c_i64(0))])]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn exit_42() {
    let m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[],
        vec![ret(c_i64(42))],
    )]);
    assert_parity(&m, &["prog"], &[], 42, b"");
}

#[test]
fn syscall_write_hello() {
    assert_parity(&hello_module(), &["prog"], &[], 0, b"hello tc_r1\n");
}

#[test]
fn signed_i64_arith() {
    let body = vec![
        expect_i64(bin(BinOp::Add, c_i64(40), c_i64(2)), 42),
        expect_i64(bin(BinOp::Sub, c_i64(10), c_i64(15)), -5),
        expect_i64(bin(BinOp::Mul, c_i64(-6), c_i64(7)), -42),
        expect_i64(bin(BinOp::Div, c_i64(-84), c_i64(2)), -42),
        expect_i64(bin(BinOp::Mod, c_i64(-85), c_i64(7)), -1),
        ret(c_i64(0)),
    ];
    let m = module(vec![func("main", &[], Type::I64, &[], body)]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn fn_8_int_args() {
    let params = (0..8)
        .map(|i| (format!("p{i}"), Type::I64))
        .collect::<Vec<_>>();
    let params_ref = params
        .iter()
        .map(|(name, ty)| (name.as_str(), ty.clone()))
        .collect::<Vec<_>>();
    let mut sum = local(0);
    for index in 1..8 {
        sum = bin(BinOp::Add, sum, local(index));
    }
    let helper = func("sum8", &params_ref, Type::I64, &[], vec![ret(sum)]);
    let args = (1..=8).map(c_i64).collect::<Vec<_>>();
    let body = vec![expect_i64(call(1, args), 36), ret(c_i64(0))];
    let m = module(vec![func("main", &[], Type::I64, &[], body), helper]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn fn_10_float_args() {
    let names = (0..10).map(|i| format!("f{i}")).collect::<Vec<_>>();
    let params = names
        .iter()
        .map(|name| (name.as_str(), Type::F64))
        .collect::<Vec<_>>();
    let mut sum = local(0);
    for index in 1..10 {
        sum = bin(BinOp::Add, sum, local(index));
    }
    let helper = func("sum10", &params, Type::F64, &[], vec![ret(sum)]);
    let args = (1..=10)
        .map(|value| c_f64(f64::from(value)))
        .collect::<Vec<_>>();
    let body = vec![expect_f64(call(1, args), 55.0), ret(c_i64(0))];
    let m = module(vec![func("main", &[], Type::I64, &[], body), helper]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn struct_alignment_and_offsets() {
    let mut m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[("s", Type::Struct(StructId(0)))],
        vec![
            expect_u64(Expr::SizeOf(Type::Struct(StructId(0))), 24),
            expect_u64(Expr::AlignOf(Type::Struct(StructId(0))), 8),
            expect_u64(
                bin(
                    BinOp::Sub,
                    cast(field_addr(addr_local(0), 0, 1), Type::U64),
                    cast(addr_local(0), Type::U64),
                ),
                8,
            ),
            expect_u64(
                bin(
                    BinOp::Sub,
                    cast(field_addr(addr_local(0), 0, 2), Type::U64),
                    cast(addr_local(0), Type::U64),
                ),
                16,
            ),
            expect_u64(
                bin(
                    BinOp::Sub,
                    cast(field_addr(addr_local(0), 0, 3), Type::U64),
                    cast(addr_local(0), Type::U64),
                ),
                20,
            ),
            store(field_addr(addr_local(0), 0, 1), Type::I64, c_i64(-7)),
            expect_i64(load(field_addr(addr_local(0), 0, 1), Type::I64), -7),
            ret(c_i64(0)),
        ],
    )]);
    m.structs.push(StructDef {
        name: "S".to_string(),
        fields: vec![Type::U8, Type::I64, Type::U16, Type::I32],
    });
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn union_f64_u64_bit_pun() {
    let mut m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[("u", Type::Union(UnionId(0)))],
        vec![
            store(union_field_addr(addr_local(0), 0, 0), Type::F64, c_f64(1.5)),
            expect_u64(
                load(union_field_addr(addr_local(0), 0, 1), Type::U64),
                0x3ff8_0000_0000_0000,
            ),
            ret(c_i64(0)),
        ],
    )]);
    m.unions.push(UnionDef {
        name: "U".to_string(),
        fields: vec![Type::F64, Type::U64],
    });
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn narrow_signed_i8_i16_i32() {
    let body = vec![
        expect_expr_eq(cast(c_i64(130), Type::I8), c_int(-126, Type::I8)),
        expect_expr_eq(cast(c_i64(65535), Type::I16), c_int(-1, Type::I16)),
        expect_expr_eq(
            cast(c_i64(i64::from(i32::MIN)), Type::I32),
            c_int(i64::from(i32::MIN), Type::I32),
        ),
        expect_expr_eq(
            bin(BinOp::Add, c_int(127, Type::I8), c_int(1, Type::I8)),
            c_int(-128, Type::I8),
        ),
        expect_expr_eq(
            bin(BinOp::Sub, c_int(-128, Type::I8), c_int(1, Type::I8)),
            c_int(127, Type::I8),
        ),
        expect_expr_eq(
            bin(BinOp::Add, c_int(100, Type::I8), c_int(100, Type::I8)),
            c_int(-56, Type::I8),
        ),
        expect_expr_eq(
            unary(UnOp::Neg, c_int(-128, Type::I8)),
            c_int(-128, Type::I8),
        ),
        ret(c_i64(0)),
    ];
    let m = module(vec![func("main", &[], Type::I64, &[], body)]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn unsigned_u8_u16_u32_u64() {
    let max = u64::MAX;
    let body = vec![
        expect_expr_eq(
            bin(BinOp::Add, c_uint(250, Type::U8), c_uint(10, Type::U8)),
            c_uint(4, Type::U8),
        ),
        expect_expr_eq(
            bin(BinOp::Add, c_uint(65535, Type::U16), c_uint(1, Type::U16)),
            c_uint(0, Type::U16),
        ),
        expect_expr_eq(
            bin(
                BinOp::Div,
                c_uint(u64::from(u32::MAX), Type::U32),
                c_uint(3, Type::U32),
            ),
            c_uint(1_431_655_765, Type::U32),
        ),
        expect_expr_eq(
            bin(BinOp::Div, c_uint(max, Type::U64), c_uint(2, Type::U64)),
            c_uint(9_223_372_036_854_775_807, Type::U64),
        ),
        expect_expr_eq(
            bin(BinOp::Mod, c_uint(max, Type::U64), c_uint(10, Type::U64)),
            c_uint(5, Type::U64),
        ),
        expect_expr_eq(
            bin(
                BinOp::Shr,
                c_uint(0xffff_ffff_ffff_fff8, Type::U64),
                c_uint(1, Type::U64),
            ),
            c_uint(0x7fff_ffff_ffff_fffc, Type::U64),
        ),
        expect_expr_eq(
            bin(BinOp::Shr, c_int(-8, Type::I64), c_int(1, Type::I64)),
            c_int(-4, Type::I64),
        ),
        trap_unless(bin(BinOp::Gt, c_uint(max, Type::U64), c_uint(0, Type::U64))),
        trap_unless(bin(BinOp::Lt, c_int(-1, Type::I64), c_int(0, Type::I64))),
        ret(c_i64(0)),
    ];
    let m = module(vec![func("main", &[], Type::I64, &[], body)]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn bitwise_ops() {
    let body = vec![
        expect_i64(bin(BinOp::BitAnd, c_i64(0xf0), c_i64(0xcc)), 0xc0),
        expect_i64(bin(BinOp::BitOr, c_i64(0xf0), c_i64(0xcc)), 0xfc),
        expect_i64(bin(BinOp::BitXor, c_i64(0xf0), c_i64(0xcc)), 0x3c),
        expect_expr_eq(
            unary(UnOp::BitNot, c_uint(0, Type::U64)),
            c_uint(u64::MAX, Type::U64),
        ),
        expect_u64(bin(BinOp::Shl, c_u64(1), c_u64(63)), 0x8000_0000_0000_0000),
        expect_u64(bin(BinOp::Shr, c_u64(u64::MAX), c_u64(60)), 15),
        expect_i64(bin(BinOp::BitXor, c_i64(-1), c_i64(-1)), 0),
        expect_expr_eq(
            bin(BinOp::LogAnd, c_bool(true), c_bool(false)),
            c_bool(false),
        ),
        expect_expr_eq(bin(BinOp::LogOr, c_bool(false), c_bool(true)), c_bool(true)),
        expect_expr_eq(unary(UnOp::LogNot, c_bool(false)), c_bool(true)),
        ret(c_i64(0)),
    ];
    let m = module(vec![func("main", &[], Type::I64, &[], body)]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn f64_arith_and_cmp() {
    let body = vec![
        expect_f64(bin(BinOp::Add, c_f64(1.5), c_f64(2.25)), 3.75),
        expect_f64(bin(BinOp::Sub, c_f64(5.0), c_f64(8.5)), -3.5),
        expect_f64(bin(BinOp::Mul, c_f64(2.5), c_f64(4.0)), 10.0),
        expect_f64(bin(BinOp::Div, c_f64(7.5), c_f64(2.5)), 3.0),
        expect_f64(unary(UnOp::Neg, c_f64(2.5)), -2.5),
        trap_unless(bin(BinOp::Lt, c_f64(-2.5), c_f64(0.0))),
        trap_unless(bin(BinOp::Le, c_f64(1.0), c_f64(1.0))),
        trap_unless(bin(BinOp::Gt, c_f64(2.0), c_f64(-1.0))),
        trap_unless(bin(BinOp::Ge, c_f64(2.0), c_f64(2.0))),
        trap_unless(bin(BinOp::Eq, c_f64(1.25), c_f64(1.25))),
        trap_unless(bin(BinOp::Ne, c_f64(1.25), c_f64(1.5))),
        ret(c_i64(0)),
    ];
    let m = module(vec![func("main", &[], Type::I64, &[], body)]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn f32_arith_and_casts() {
    let helper = func(
        "add_f32",
        &[("a", Type::F32), ("b", Type::F32)],
        Type::F32,
        &[],
        vec![ret(bin(BinOp::Add, local(0), local(1)))],
    );
    let body = vec![
        expect_f32(bin(BinOp::Add, c_f32(1.25), c_f32(2.5)), 3.75),
        expect_f32(bin(BinOp::Mul, c_f32(1.5), c_f32(4.0)), 6.0),
        expect_f32(bin(BinOp::Div, c_f32(7.5), c_f32(2.5)), 3.0),
        expect_f32(unary(UnOp::Neg, c_f32(2.5)), -2.5),
        expect_f32(call(1, vec![c_f32(1.25), c_f32(2.5)]), 3.75),
        expect_f64(cast(c_f32(3.75), Type::F64), 3.75),
        expect_f32(cast(c_f64(3.75), Type::F32), 3.75),
        expect_f64(cast(c_i64(-7), Type::F64), -7.0),
        expect_i64(cast(c_f64(42.9), Type::I64), 42),
        expect_expr_eq(cast(c_f32(2.5), Type::I32), c_i32(2)),
        expect_f64(
            cast(c_u64(u64::MAX), Type::F64),
            18_446_744_073_709_551_616.0,
        ),
        trap_unless(bin(BinOp::Lt, c_f32(-1.5), c_f32(0.0))),
        ret(c_i64(0)),
    ];
    let m = module(vec![func("main", &[], Type::I64, &[], body), helper]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn if_else_and_nested_branches() {
    let body = vec![
        assign(0, c_i64(0)),
        if_(
            bin(BinOp::Lt, c_i64(1), c_i64(2)),
            vec![if_(
                bin(BinOp::Gt, c_i64(3), c_i64(4)),
                vec![assign(0, c_i64(10))],
                vec![assign(0, c_i64(20))],
            )],
            vec![assign(0, c_i64(30))],
        ),
        expect_i64(local(0), 20),
        if_(
            bin(BinOp::Eq, c_i64(2), c_i64(2)),
            vec![assign(0, c_i64(7))],
            Vec::new(),
        ),
        expect_i64(local(0), 7),
        if_(
            bin(BinOp::Eq, c_i64(1), c_i64(2)),
            vec![assign(0, c_i64(99))],
            vec![if_(
                bin(BinOp::Eq, c_i64(3), c_i64(3)),
                vec![assign(0, c_i64(11))],
                Vec::new(),
            )],
        ),
        expect_i64(local(0), 11),
        ret(c_i64(0)),
    ];
    let m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[("x", Type::I64)],
        body,
    )]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn short_circuit_and_or() {
    let bump = func(
        "bump",
        &[("p", ptr(Type::I64))],
        Type::Bool,
        &[],
        vec![
            store(
                local(0),
                Type::I64,
                bin(BinOp::Add, load(local(0), Type::I64), c_i64(1)),
            ),
            ret(c_bool(true)),
        ],
    );
    let body = vec![
        assign(0, c_i64(0)),
        expr(bin(
            BinOp::LogAnd,
            c_bool(false),
            call(1, vec![addr_local(0)]),
        )),
        expect_i64(local(0), 0),
        expr(bin(
            BinOp::LogOr,
            c_bool(true),
            call(1, vec![addr_local(0)]),
        )),
        expect_i64(local(0), 0),
        expr(bin(
            BinOp::LogAnd,
            c_bool(true),
            call(1, vec![addr_local(0)]),
        )),
        expect_i64(local(0), 1),
        expr(bin(
            BinOp::LogOr,
            c_bool(false),
            call(1, vec![addr_local(0)]),
        )),
        expect_i64(local(0), 2),
        ret(c_i64(0)),
    ];
    let m = module(vec![
        func("main", &[], Type::I64, &[("count", Type::I64)], body),
        bump,
    ]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn loop_break_continue() {
    let body = vec![
        assign(0, c_i64(0)),
        assign(1, c_i64(0)),
        loop_(vec![
            assign(0, bin(BinOp::Add, local(0), c_i64(1))),
            if_(
                bin(BinOp::Gt, local(0), c_i64(10)),
                vec![Stmt::Break],
                Vec::new(),
            ),
            if_(
                bin(BinOp::Eq, local(0), c_i64(5)),
                vec![Stmt::Continue],
                Vec::new(),
            ),
            assign(1, bin(BinOp::Add, local(1), local(0))),
        ]),
        expect_i64(local(1), 50),
        ret(c_i64(0)),
    ];
    let m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[("i", Type::I64), ("sum", Type::I64)],
        body,
    )]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn nested_loops_3_deep() {
    let body = vec![
        assign(0, c_i64(0)),
        assign(1, c_i64(0)),
        assign(2, c_i64(0)),
        assign(3, c_i64(0)),
        loop_(vec![
            if_(
                bin(BinOp::Ge, local(0), c_i64(2)),
                vec![Stmt::Break],
                Vec::new(),
            ),
            assign(1, c_i64(0)),
            loop_(vec![
                if_(
                    bin(BinOp::Ge, local(1), c_i64(2)),
                    vec![Stmt::Break],
                    Vec::new(),
                ),
                assign(2, c_i64(0)),
                loop_(vec![
                    if_(
                        bin(BinOp::Ge, local(2), c_i64(2)),
                        vec![Stmt::Break],
                        Vec::new(),
                    ),
                    assign(3, bin(BinOp::Add, local(3), c_i64(1))),
                    assign(2, bin(BinOp::Add, local(2), c_i64(1))),
                ]),
                assign(1, bin(BinOp::Add, local(1), c_i64(1))),
            ]),
            assign(0, bin(BinOp::Add, local(0), c_i64(1))),
        ]),
        expect_i64(local(3), 8),
        ret(c_i64(0)),
    ];
    let m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[
            ("i", Type::I64),
            ("j", Type::I64),
            ("k", Type::I64),
            ("count", Type::I64),
        ],
        body,
    )]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn fn_mixed_int_and_float_args() {
    let mut param_names = Vec::new();
    let mut param_types = Vec::new();
    for index in 0..8 {
        param_names.push(format!("i{index}"));
        param_types.push(Type::I64);
        param_names.push(format!("f{index}"));
        param_types.push(Type::F64);
    }
    param_names.push("f8".to_string());
    param_names.push("f9".to_string());
    param_types.push(Type::F64);
    param_types.push(Type::F64);
    let params = param_names
        .iter()
        .zip(param_types.iter())
        .map(|(name, ty)| (name.as_str(), ty.clone()))
        .collect::<Vec<_>>();

    let mut int_sum = local(0);
    for pair in 1..8 {
        int_sum = bin(BinOp::Add, int_sum, local(pair * 2));
    }
    let mut float_sum = local(1);
    for pair in 1..8 {
        float_sum = bin(BinOp::Add, float_sum, local(pair * 2 + 1));
    }
    float_sum = bin(BinOp::Add, float_sum, local(16));
    float_sum = bin(BinOp::Add, float_sum, local(17));
    let helper_body = vec![ret(bin(BinOp::Add, cast(int_sum, Type::F64), float_sum))];
    let helper = func("mixed", &params, Type::F64, &[], helper_body);

    let mut args = Vec::new();
    for index in 1..=8 {
        args.push(c_i64(index));
        args.push(c_f64(f64::from(
            u8::try_from(index).expect("small integer"),
        )));
    }
    args.push(c_f64(9.0));
    args.push(c_f64(10.0));
    let body = vec![expect_f64(call(1, args), 91.0), ret(c_i64(0))];
    let m = module(vec![func("main", &[], Type::I64, &[], body), helper]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn nested_call_arg_clobber() {
    let add = func(
        "add",
        &[("a", Type::I64), ("b", Type::I64)],
        Type::I64,
        &[],
        vec![ret(bin(BinOp::Add, local(0), local(1)))],
    );
    let mul = func(
        "mul",
        &[("a", Type::I64), ("b", Type::I64)],
        Type::I64,
        &[],
        vec![ret(bin(BinOp::Mul, local(0), local(1)))],
    );
    let sub = func(
        "sub",
        &[("a", Type::I64), ("b", Type::I64)],
        Type::I64,
        &[],
        vec![ret(bin(BinOp::Sub, local(0), local(1)))],
    );
    let nested = call(
        1,
        vec![
            call(2, vec![c_i64(2), c_i64(3)]),
            call(3, vec![c_i64(20), c_i64(5)]),
        ],
    );
    let body = vec![expect_i64(nested, 21), ret(c_i64(0))];
    let m = module(vec![func("main", &[], Type::I64, &[], body), add, mul, sub]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn recursion_factorial_12() {
    let factorial = func(
        "factorial",
        &[("n", Type::I64)],
        Type::I64,
        &[],
        vec![if_(
            bin(BinOp::Le, local(0), c_i64(1)),
            vec![ret(c_i64(1))],
            vec![ret(bin(
                BinOp::Mul,
                local(0),
                call(1, vec![bin(BinOp::Sub, local(0), c_i64(1))]),
            ))],
        )],
    );
    let body = vec![
        expect_i64(call(1, vec![c_i64(12)]), 479_001_600),
        ret(c_i64(0)),
    ];
    let m = module(vec![func("main", &[], Type::I64, &[], body), factorial]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn recursion_fib_15() {
    let fib = func(
        "fib",
        &[("n", Type::I64)],
        Type::I64,
        &[],
        vec![if_(
            bin(BinOp::Lt, local(0), c_i64(2)),
            vec![ret(local(0))],
            vec![ret(bin(
                BinOp::Add,
                call(1, vec![bin(BinOp::Sub, local(0), c_i64(1))]),
                call(1, vec![bin(BinOp::Sub, local(0), c_i64(2))]),
            ))],
        )],
    );
    let body = vec![expect_i64(call(1, vec![c_i64(15)]), 610), ret(c_i64(0))];
    let m = module(vec![func("main", &[], Type::I64, &[], body), fib]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn recursion_sum_down_500() {
    let sum = func(
        "sum_down",
        &[("n", Type::I64)],
        Type::I64,
        &[],
        vec![if_(
            bin(BinOp::Eq, local(0), c_i64(0)),
            vec![ret(c_i64(0))],
            vec![ret(bin(
                BinOp::Add,
                local(0),
                call(1, vec![bin(BinOp::Sub, local(0), c_i64(1))]),
            ))],
        )],
    );
    let body = vec![
        expect_i64(call(1, vec![c_i64(500)]), 125_250),
        ret(c_i64(0)),
    ];
    let m = module(vec![func("main", &[], Type::I64, &[], body), sum]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn mutual_recursion_even_odd() {
    let even = func(
        "is_even",
        &[("n", Type::I64)],
        Type::I64,
        &[],
        vec![if_(
            bin(BinOp::Eq, local(0), c_i64(0)),
            vec![ret(c_i64(1))],
            vec![ret(call(2, vec![bin(BinOp::Sub, local(0), c_i64(1))]))],
        )],
    );
    let odd = func(
        "is_odd",
        &[("n", Type::I64)],
        Type::I64,
        &[],
        vec![if_(
            bin(BinOp::Eq, local(0), c_i64(0)),
            vec![ret(c_i64(0))],
            vec![ret(call(1, vec![bin(BinOp::Sub, local(0), c_i64(1))]))],
        )],
    );
    let body = vec![
        expect_i64(call(1, vec![c_i64(42)]), 1),
        expect_i64(call(2, vec![c_i64(41)]), 1),
        ret(c_i64(0)),
    ];
    let m = module(vec![func("main", &[], Type::I64, &[], body), even, odd]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn function_pointer_indirect_call() {
    let square = func(
        "square",
        &[("x", Type::I64)],
        Type::I64,
        &[],
        vec![ret(bin(BinOp::Mul, local(0), local(0)))],
    );
    let signature = fnptr(vec![Type::I64], Type::I64);
    let pick = func(
        "pick",
        &[],
        signature.clone(),
        &[],
        vec![ret(Expr::FnAddr(FnId(1)))],
    );
    let apply = func(
        "apply",
        &[("f", fnptr(vec![Type::I64], Type::I64)), ("x", Type::I64)],
        Type::I64,
        &[],
        vec![ret(Expr::Call {
            target: CallTarget::Indirect(Box::new(local(0))),
            args: vec![local(1)],
        })],
    );
    let body = vec![
        expect_i64(call(2, vec![Expr::FnAddr(FnId(1)), c_i64(7)]), 49),
        expect_i64(
            Expr::Call {
                target: CallTarget::Indirect(Box::new(Expr::FnAddr(FnId(1)))),
                args: vec![c_i64(9)],
            },
            81,
        ),
        assign(0, call(3, Vec::new())),
        expect_i64(
            Expr::Call {
                target: CallTarget::Indirect(Box::new(local(0))),
                args: vec![c_i64(6)],
            },
            36,
        ),
        ret(c_i64(0)),
    ];
    let m = module(vec![
        func("main", &[], Type::I64, &[("fp", signature)], body),
        square,
        apply,
        pick,
    ]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn nested_struct_and_ptr_mutation() {
    let mutate = func(
        "mutate",
        &[("p", ptr(Type::Struct(StructId(1))))],
        Type::Void,
        &[],
        vec![
            store(
                field_addr(field_addr(local(0), 1, 0), 0, 1),
                Type::I32,
                c_i32(77),
            ),
            store(field_addr(local(0), 1, 1), Type::U8, c_uint(9, Type::U8)),
            ret_void(),
        ],
    );
    let body = vec![
        store(
            field_addr(field_addr(addr_local(0), 1, 0), 0, 0),
            Type::I64,
            c_i64(5),
        ),
        store(
            field_addr(field_addr(addr_local(0), 1, 0), 0, 1),
            Type::I32,
            c_i32(6),
        ),
        store(
            field_addr(addr_local(0), 1, 1),
            Type::U8,
            c_uint(1, Type::U8),
        ),
        expr(call(1, vec![addr_local(0)])),
        expect_i64(
            load(field_addr(field_addr(addr_local(0), 1, 0), 0, 0), Type::I64),
            5,
        ),
        expect_expr_eq(
            load(field_addr(field_addr(addr_local(0), 1, 0), 0, 1), Type::I32),
            c_i32(77),
        ),
        expect_expr_eq(
            load(field_addr(addr_local(0), 1, 1), Type::U8),
            c_uint(9, Type::U8),
        ),
        ret(c_i64(0)),
    ];
    let mut m = module(vec![
        func(
            "main",
            &[],
            Type::I64,
            &[("outer", Type::Struct(StructId(1)))],
            body,
        ),
        mutate,
    ]);
    m.structs.push(StructDef {
        name: "Inner".to_string(),
        fields: vec![Type::I64, Type::I32],
    });
    m.structs.push(StructDef {
        name: "Outer".to_string(),
        fields: vec![Type::Struct(StructId(0)), Type::U8],
    });
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn fixed_array_stack_sum() {
    let arr_ty = Type::Array(Box::new(Type::I64), 5);
    let base = || cast(addr_local(0), ptr(Type::I64));
    let body = vec![
        assign(1, c_i64(0)),
        assign(2, c_i64(0)),
        loop_(vec![
            if_(
                bin(BinOp::Ge, local(1), c_i64(5)),
                vec![Stmt::Break],
                Vec::new(),
            ),
            store(
                index_addr(base(), Type::I64, local(1)),
                Type::I64,
                bin(BinOp::Add, local(1), c_i64(1)),
            ),
            assign(1, bin(BinOp::Add, local(1), c_i64(1))),
        ]),
        assign(1, c_i64(0)),
        loop_(vec![
            if_(
                bin(BinOp::Ge, local(1), c_i64(5)),
                vec![Stmt::Break],
                Vec::new(),
            ),
            assign(
                2,
                bin(
                    BinOp::Add,
                    local(2),
                    load(index_addr(base(), Type::I64, local(1)), Type::I64),
                ),
            ),
            assign(1, bin(BinOp::Add, local(1), c_i64(1))),
        ]),
        expect_i64(local(2), 15),
        ret(c_i64(0)),
    ];
    let m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[("arr", arr_ty), ("i", Type::I64), ("sum", Type::I64)],
        body,
    )]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn fixed_array_bubble_sort_8() {
    let arr_ty = Type::Array(Box::new(Type::I64), 8);
    let base = || cast(addr_local(0), ptr(Type::I64));
    let initial = [8, 3, 7, 1, 5, 2, 6, 4];
    let mut body = vec![assign(1, c_i64(0)), assign(2, c_i64(0))];
    for (index, value) in initial.iter().enumerate() {
        body.push(store(
            index_addr(base(), Type::I64, c_i64(index as i64)),
            Type::I64,
            c_i64(*value),
        ));
    }
    body.push(assign(1, c_i64(0)));
    body.push(loop_(vec![
        if_(
            bin(BinOp::Ge, local(1), c_i64(7)),
            vec![Stmt::Break],
            Vec::new(),
        ),
        assign(2, c_i64(0)),
        loop_(vec![
            if_(
                bin(BinOp::Ge, local(2), bin(BinOp::Sub, c_i64(7), local(1))),
                vec![Stmt::Break],
                Vec::new(),
            ),
            assign(3, load(index_addr(base(), Type::I64, local(2)), Type::I64)),
            assign(
                4,
                load(
                    index_addr(base(), Type::I64, bin(BinOp::Add, local(2), c_i64(1))),
                    Type::I64,
                ),
            ),
            if_(
                bin(BinOp::Gt, local(3), local(4)),
                vec![
                    store(index_addr(base(), Type::I64, local(2)), Type::I64, local(4)),
                    store(
                        index_addr(base(), Type::I64, bin(BinOp::Add, local(2), c_i64(1))),
                        Type::I64,
                        local(3),
                    ),
                ],
                Vec::new(),
            ),
            assign(2, bin(BinOp::Add, local(2), c_i64(1))),
        ]),
        assign(1, bin(BinOp::Add, local(1), c_i64(1))),
    ]));
    for (index, value) in [1, 2, 3, 4, 5, 6, 7, 8].iter().enumerate() {
        body.push(expect_i64(
            load(
                index_addr(base(), Type::I64, c_i64(index as i64)),
                Type::I64,
            ),
            *value,
        ));
    }
    body.push(ret(c_i64(0)));
    let m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[
            ("arr", arr_ty),
            ("i", Type::I64),
            ("j", Type::I64),
            ("a", Type::I64),
            ("b", Type::I64),
        ],
        body,
    )]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn static_globals_bss_and_data() {
    let bump = func(
        "bump",
        &[("p", ptr(Type::I64))],
        Type::I64,
        &[],
        vec![
            store(
                local(0),
                Type::I64,
                bin(BinOp::Add, load(local(0), Type::I64), c_i64(1)),
            ),
            ret(load(local(0), Type::I64)),
        ],
    );
    let global_addr = |id: usize| Expr::GlobalAddr(tc_ir::GlobalId(id));
    let body = vec![
        expect_i64(load(global_addr(0), Type::I64), 5),
        expect_i64(call(1, vec![global_addr(0)]), 6),
        expect_i64(load(global_addr(0), Type::I64), 6),
        expect_i64(load(global_addr(1), Type::I64), 0),
        expect_i64(call(1, vec![global_addr(1)]), 1),
        expect_i64(load(global_addr(1), Type::I64), 1),
        ret(c_i64(0)),
    ];
    let mut m = module(vec![func("main", &[], Type::I64, &[], body), bump]);
    m.globals.push(GlobalDef {
        name: "initialized".to_string(),
        ty: Type::I64,
        init: Some(5i64.to_le_bytes().to_vec()),
    });
    m.globals.push(GlobalDef {
        name: "zeroed".to_string(),
        ty: Type::I64,
        init: None,
    });
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn memcopy_struct() {
    let size_expr = Expr::SizeOf(Type::Struct(StructId(0)));
    let body = vec![
        store(
            field_addr(addr_local(0), 0, 0),
            Type::I64,
            c_i64(123_456_789),
        ),
        store(field_addr(addr_local(0), 0, 1), Type::I32, c_i32(-42)),
        store(
            field_addr(addr_local(0), 0, 2),
            Type::U16,
            c_uint(65535, Type::U16),
        ),
        Stmt::MemCopy {
            dst: addr_local(1),
            src: addr_local(0),
            size: size_expr,
        },
        expect_i64(
            load(field_addr(addr_local(1), 0, 0), Type::I64),
            123_456_789,
        ),
        expect_expr_eq(load(field_addr(addr_local(1), 0, 1), Type::I32), c_i32(-42)),
        expect_expr_eq(
            load(field_addr(addr_local(1), 0, 2), Type::U16),
            c_uint(65535, Type::U16),
        ),
        ret(c_i64(0)),
    ];
    let mut m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[
            ("src", Type::Struct(StructId(0))),
            ("dst", Type::Struct(StructId(0))),
        ],
        body,
    )]);
    m.structs.push(StructDef {
        name: "S".to_string(),
        fields: vec![Type::I64, Type::I32, Type::U16],
    });
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn syscall_mmap_munmap_6_args() {
    let u8_ptr = ptr(Type::U8);
    let body = vec![
        assign(
            0,
            syscall(
                9,
                vec![
                    c_u64(0),
                    c_u64(4096),
                    c_u64(3),
                    c_u64(34),
                    c_int(-1, Type::I64),
                    c_u64(0),
                ],
            ),
        ),
        assign(1, cast(cast(local(0), Type::U64), u8_ptr.clone())),
        trap_unless(bin(BinOp::Ne, cast(local(1), Type::U64), c_u64(0))),
        assign(2, c_u64(0)),
        loop_(vec![
            if_(
                bin(BinOp::Ge, local(2), c_u64(100)),
                vec![Stmt::Break],
                Vec::new(),
            ),
            store(
                index_addr(local(1), Type::U8, local(2)),
                Type::U8,
                cast(bin(BinOp::Add, local(2), c_u64(1)), Type::U8),
            ),
            assign(2, bin(BinOp::Add, local(2), c_u64(1))),
        ]),
        assign(2, c_u64(0)),
        assign(3, c_u64(0)),
        loop_(vec![
            if_(
                bin(BinOp::Ge, local(2), c_u64(100)),
                vec![Stmt::Break],
                Vec::new(),
            ),
            assign(
                3,
                bin(
                    BinOp::Add,
                    local(3),
                    cast(
                        load(index_addr(local(1), Type::U8, local(2)), Type::U8),
                        Type::U64,
                    ),
                ),
            ),
            assign(2, bin(BinOp::Add, local(2), c_u64(1))),
        ]),
        expect_u64(local(3), 5050),
        assign(0, syscall(11, vec![local(1), c_u64(4096)])),
        expect_i64(local(0), 0),
        ret(c_i64(0)),
    ];
    let m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[
            ("raw", Type::I64),
            ("mem", u8_ptr),
            ("i", Type::U64),
            ("sum", Type::U64),
        ],
        body,
    )]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn syscall_read_stdin_echo() {
    let body = vec![
        assign(1, syscall(0, vec![c_i64(0), addr_local(0), c_u64(32)])),
        expr(syscall(
            1,
            vec![c_i64(1), addr_local(0), cast(local(1), Type::U64)],
        )),
        ret(c_i64(0)),
    ];
    let m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[
            ("buffer", Type::Array(Box::new(Type::U8), 32)),
            ("n", Type::I64),
        ],
        body,
    )]);
    assert_parity(&m, &["prog"], b"echo me\n", 0, b"echo me\n");
}

#[test]
fn cli_argc_argv_forwarding() {
    let mut m = module(vec![func(
        "main",
        &[
            ("argc", Type::I64),
            ("argv", ptr(ptr(Type::U8))),
            ("envp", ptr(ptr(Type::U8))),
        ],
        Type::I64,
        &[("len", Type::U64), ("ptr", ptr(Type::U8))],
        Vec::new(),
    )]);
    let body = vec![
        trap_unless(bin(BinOp::Eq, local(0), c_i64(2))),
        assign(3, c_u64(0)),
        assign(
            4,
            load(index_addr(local(1), ptr(Type::U8), c_i64(1)), ptr(Type::U8)),
        ),
        loop_(vec![
            if_(
                bin(
                    BinOp::Eq,
                    load(index_addr(local(4), Type::U8, local(3)), Type::U8),
                    c_uint(0, Type::U8),
                ),
                vec![Stmt::Break],
                Vec::new(),
            ),
            assign(3, bin(BinOp::Add, local(3), c_u64(1))),
        ]),
        expr(syscall(1, vec![c_i64(1), local(4), local(3)])),
        expr(syscall(
            1,
            vec![c_i64(1), Expr::RodataPtr(tc_ir::RodataId(0)), c_u64(1)],
        )),
        ret(c_i64(0)),
    ];
    m.functions[0].body = body;
    m.rodata.push(RodataDef {
        name: "newline".to_string(),
        bytes: b"\n".to_vec(),
    });
    assert_parity(&m, &["prog", "hello_arg"], &[], 0, b"hello_arg\n");
}

#[test]
fn ir_formatter_roundtrip_smoke() {
    let m = hello_module();
    verify_module(&m).expect("module verifies");
    let first = format_module(&m);
    let second = format_module(&m);
    assert_eq!(first, second, "formatter must be deterministic");
    assert!(first.contains("fn #0 \"main\""), "missing function text");
    assert!(first.contains("rodata #0 \"hello\""), "missing rodata text");
    assert_eq!(
        format_type(&Type::Array(Box::new(Type::I64), 3)),
        "array<i64, 3>"
    );
    assert_eq!(
        format_type(&Type::FnPtr {
            params: vec![Type::I64],
            ret: Box::new(Type::Bool),
        }),
        "fnptr(i64) -> bool"
    );
}

#[test]
fn primitive_layouts() {
    let mut body = Vec::new();
    for (ty, size, align) in [
        (Type::Void, 0, 1),
        (Type::Bool, 1, 1),
        (Type::I8, 1, 1),
        (Type::U8, 1, 1),
        (Type::I16, 2, 2),
        (Type::U16, 2, 2),
        (Type::I32, 4, 4),
        (Type::U32, 4, 4),
        (Type::F32, 4, 4),
        (Type::I64, 8, 8),
        (Type::U64, 8, 8),
        (Type::F64, 8, 8),
        (ptr(Type::I64), 8, 8),
        (fnptr(vec![Type::I64, Type::F64], Type::Bool), 8, 8),
    ] {
        body.push(expect_u64(Expr::SizeOf(ty.clone()), size));
        body.push(expect_u64(Expr::AlignOf(ty), align));
    }
    body.push(expect_u64(
        Expr::SizeOf(Type::Array(Box::new(Type::I32), 3)),
        12,
    ));
    body.push(expect_u64(
        Expr::AlignOf(Type::Array(Box::new(Type::I32), 3)),
        4,
    ));
    body.push(ret(c_i64(0)));
    let m = module(vec![func("main", &[], Type::I64, &[], body)]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn null_pointer_and_ptr_casts() {
    let i64_ptr = ptr(Type::I64);
    let body = vec![
        assign(0, c_i64(7)),
        assign(1, Expr::ConstNull(i64_ptr.clone())),
        expect_u64(cast(local(1), Type::U64), 0),
        expect_u64(cast(cast(local(1), ptr(Type::U8)), Type::U64), 0),
        assign(1, cast(addr_local(0), i64_ptr.clone())),
        expect_i64(load(local(1), Type::I64), 7),
        trap_unless(bin(BinOp::Ne, cast(addr_local(0), Type::U64), c_u64(0))),
        ret(c_i64(0)),
    ];
    let m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[("x", Type::I64), ("p", i64_ptr)],
        body,
    )]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn void_function_three_args() {
    let helper = func(
        "touch",
        &[("p", ptr(Type::I64))],
        Type::Void,
        &[],
        vec![store(local(0), Type::I64, c_i64(99)), ret_void()],
    );
    let body = vec![
        assign(0, c_i64(0)),
        expr(call(1, vec![addr_local(0)])),
        expect_i64(local(0), 99),
        ret(c_i64(0)),
    ];
    let m = module(vec![
        func("main", &[], Type::I64, &[("x", Type::I64)], body),
        helper,
    ]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn narrow_parameter_extension() {
    let helper = func(
        "narrow",
        &[
            ("a", Type::I8),
            ("b", Type::U8),
            ("c", Type::I16),
            ("d", Type::U16),
        ],
        Type::I64,
        &[],
        vec![ret(bin(
            BinOp::Add,
            bin(
                BinOp::Add,
                cast(local(0), Type::I64),
                cast(local(1), Type::I64),
            ),
            bin(
                BinOp::Add,
                cast(local(2), Type::I64),
                cast(local(3), Type::I64),
            ),
        ))],
    );
    let body = vec![
        expect_i64(
            call(
                1,
                vec![
                    c_int(-5, Type::I8),
                    c_uint(250, Type::U8),
                    c_int(-300, Type::I16),
                    c_uint(65535, Type::U16),
                ],
            ),
            65_480,
        ),
        ret(c_i64(0)),
    ];
    let m = module(vec![func("main", &[], Type::I64, &[], body), helper]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn f32_union_bit_pun() {
    let mut m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[("u", Type::Union(UnionId(0)))],
        vec![
            store(union_field_addr(addr_local(0), 0, 0), Type::F32, c_f32(1.5)),
            expect_expr_eq(
                load(union_field_addr(addr_local(0), 0, 1), Type::U32),
                c_uint(0x3fc0_0000, Type::U32),
            ),
            ret(c_i64(0)),
        ],
    )]);
    m.unions.push(UnionDef {
        name: "F32Bits".to_string(),
        fields: vec![Type::F32, Type::U32],
    });
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn u64_float_edge_casts() {
    let max = u64::MAX;
    let body = vec![
        expect_f64(cast(c_u64(5), Type::F64), 5.0),
        expect_f64(cast(c_u64(max), Type::F64), 18_446_744_073_709_551_616.0),
        expect_f32(cast(c_u64(max), Type::F32), u64::MAX as f32),
        expect_u64(
            cast(c_f64(9_223_372_036_854_775_808.0), Type::U64),
            0x8000_0000_0000_0000,
        ),
        expect_u64(
            cast(c_f32(9_223_372_036_854_775_808.0), Type::U64),
            0x8000_0000_0000_0000,
        ),
        expect_u64(cast(c_f64(123.9), Type::U64), 123),
        ret(c_i64(0)),
    ];
    let m = module(vec![func("main", &[], Type::I64, &[], body)]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn zero_initialized_locals() {
    let body = vec![expect_i64(local(0), 0), ret(c_i64(0))];
    let m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[("x", Type::I64)],
        body,
    )]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn interpreter_trap_is_error() {
    let m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[],
        vec![Stmt::Trap, ret(c_i64(0))],
    )]);
    let error = interpret(&m, &["prog".to_string()], &[], 10_000).expect_err("trap must fail");
    assert!(matches!(error, InterpError::Trap));
}

#[test]
fn verifier_rejects_break_outside_loop() {
    let m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[],
        vec![Stmt::Break, ret(c_i64(0))],
    )]);
    assert!(verify_module(&m).is_err());
}

#[test]
fn verifier_rejects_assign_type_mismatch() {
    let m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[("x", Type::I64)],
        vec![assign(0, c_bool(true)), ret(c_i64(0))],
    )]);
    assert!(verify_module(&m).is_err());
}

#[test]
fn verifier_rejects_binary_type_mismatch() {
    let m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[],
        vec![expr(bin(BinOp::Add, c_i64(1), c_u64(2))), ret(c_i64(0))],
    )]);
    assert!(verify_module(&m).is_err());
}

#[test]
fn verifier_rejects_invalid_local_id() {
    let m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[],
        vec![expr(local(99)), ret(c_i64(0))],
    )]);
    assert!(verify_module(&m).is_err());
}

#[test]
fn verifier_rejects_syscall_7_args() {
    let m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[],
        vec![expr(syscall(1, vec![c_i64(1); 7])), ret(c_i64(0))],
    )]);
    assert!(verify_module(&m).is_err());
}

#[test]
fn verifier_rejects_recursive_struct() {
    let mut m = module(vec![func("main", &[], Type::I64, &[], vec![ret(c_i64(0))])]);
    m.structs.push(StructDef {
        name: "Recursive".to_string(),
        fields: vec![Type::Struct(StructId(0))],
    });
    assert!(verify_module(&m).is_err());
}

#[test]
fn verifier_rejects_return_type_mismatch() {
    let m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[],
        vec![ret(c_bool(true))],
    )]);
    assert!(verify_module(&m).is_err());
}

#[test]
fn verifier_rejects_invalid_entry_signature() {
    let m = module(vec![func(
        "main",
        &[("only", Type::I64)],
        Type::I64,
        &[],
        vec![ret(c_i64(0))],
    )]);
    assert!(verify_module(&m).is_err());
}

#[test]
fn entry_returns_i32() {
    let m = module(vec![func(
        "main",
        &[],
        Type::I32,
        &[],
        vec![Stmt::Return(Some(c_i32(7)))],
    )]);
    assert_parity(&m, &["prog"], &[], 7, b"");
}

#[test]
fn entry_returns_void() {
    let m = module(vec![func("main", &[], Type::Void, &[], vec![ret_void()])]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn f32_ten_stack_arguments() {
    let names = (0..10).map(|i| format!("f{i}")).collect::<Vec<_>>();
    let params = names
        .iter()
        .map(|name| (name.as_str(), Type::F32))
        .collect::<Vec<_>>();
    let mut sum = local(0);
    for index in 1..10 {
        sum = bin(BinOp::Add, sum, local(index));
    }
    let helper = func("sum10f32", &params, Type::F32, &[], vec![ret(sum)]);
    let args = (1..=10)
        .map(|value| c_f32(f32::from(u8::try_from(value).expect("small integer"))))
        .collect::<Vec<_>>();
    let body = vec![expect_f32(call(1, args), 55.0), ret(c_i64(0))];
    let m = module(vec![func("main", &[], Type::I64, &[], body), helper]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn empty_aggregate_layouts() {
    let mut m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[
            ("empty_struct", Type::Struct(StructId(0))),
            ("empty_union", Type::Union(UnionId(0))),
        ],
        vec![
            expect_u64(Expr::SizeOf(Type::Struct(StructId(0))), 1),
            expect_u64(Expr::AlignOf(Type::Struct(StructId(0))), 1),
            expect_u64(Expr::SizeOf(Type::Union(UnionId(0))), 1),
            expect_u64(Expr::AlignOf(Type::Union(UnionId(0))), 1),
            expect_u64(
                Expr::SizeOf(Type::Array(Box::new(Type::Struct(StructId(0))), 3)),
                3,
            ),
            ret(c_i64(0)),
        ],
    )]);
    m.structs.push(StructDef {
        name: "Empty".to_string(),
        fields: Vec::new(),
    });
    m.unions.push(UnionDef {
        name: "EmptyUnion".to_string(),
        fields: Vec::new(),
    });
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn seven_int_args_odd_stack() {
    let names = (0..7).map(|i| format!("p{i}")).collect::<Vec<_>>();
    let params = names
        .iter()
        .map(|name| (name.as_str(), Type::I64))
        .collect::<Vec<_>>();
    let mut sum = local(0);
    for index in 1..7 {
        sum = bin(BinOp::Add, sum, local(index));
    }
    let helper = func("sum7", &params, Type::I64, &[], vec![ret(sum)]);
    let args = (1..=7).map(c_i64).collect::<Vec<_>>();
    let body = vec![expect_i64(call(1, args), 28), ret(c_i64(0))];
    let m = module(vec![func("main", &[], Type::I64, &[], body), helper]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn nine_float_args_odd_stack() {
    let names = (0..9).map(|i| format!("f{i}")).collect::<Vec<_>>();
    let params = names
        .iter()
        .map(|name| (name.as_str(), Type::F64))
        .collect::<Vec<_>>();
    let mut sum = local(0);
    for index in 1..9 {
        sum = bin(BinOp::Add, sum, local(index));
    }
    let helper = func("sum9", &params, Type::F64, &[], vec![ret(sum)]);
    let args = (1..=9)
        .map(|value| c_f64(f64::from(u8::try_from(value).expect("small integer"))))
        .collect::<Vec<_>>();
    let body = vec![expect_f64(call(1, args), 45.0), ret(c_i64(0))];
    let m = module(vec![func("main", &[], Type::I64, &[], body), helper]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn nine_float_args_odd_stack_reversed_order() {
    let names = (0..9).map(|i| format!("f{i}")).collect::<Vec<_>>();
    let params = names
        .iter()
        .map(|name| (name.as_str(), Type::F64))
        .collect::<Vec<_>>();
    let helper_body = vec![ret(bin(BinOp::Sub, local(0), local(8)))];
    let helper = func("first_minus_last", &params, Type::F64, &[], helper_body);
    let args = (1..=9)
        .map(|value| c_f64(f64::from(u8::try_from(value).expect("small integer"))))
        .collect::<Vec<_>>();
    let body = vec![expect_f64(call(1, args), -8.0), ret(c_i64(0))];
    let m = module(vec![func("main", &[], Type::I64, &[], body), helper]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn syscall_write_stderr() {
    let mut m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[],
        vec![
            expr(syscall(
                1,
                vec![
                    c_i64(2),
                    Expr::RodataPtr(tc_ir::RodataId(0)),
                    Expr::RodataLen(tc_ir::RodataId(0)),
                ],
            )),
            ret(c_i64(0)),
        ],
    )]);
    m.rodata.push(RodataDef {
        name: "stderr".to_string(),
        bytes: b"stderr bytes\n".to_vec(),
    });
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn self_referential_pointer_struct() {
    let node_ptr = ptr(Type::Struct(StructId(0)));
    let mut m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[("node", Type::Struct(StructId(0)))],
        vec![
            expect_u64(Expr::SizeOf(Type::Struct(StructId(0))), 16),
            expect_u64(Expr::AlignOf(Type::Struct(StructId(0))), 8),
            store(field_addr(addr_local(0), 0, 0), Type::I64, c_i64(123)),
            store(
                field_addr(addr_local(0), 0, 1),
                node_ptr.clone(),
                Expr::ConstNull(node_ptr),
            ),
            expect_i64(load(field_addr(addr_local(0), 0, 0), Type::I64), 123),
            expect_u64(
                cast(
                    load(
                        field_addr(addr_local(0), 0, 1),
                        ptr(Type::Struct(StructId(0))),
                    ),
                    Type::U64,
                ),
                0,
            ),
            ret(c_i64(0)),
        ],
    )]);
    m.structs.push(StructDef {
        name: "Node".to_string(),
        fields: vec![Type::I64, ptr(Type::Struct(StructId(0)))],
    });
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn nonvoid_fallthrough_returns_zero() {
    let m = module(vec![func("main", &[], Type::I64, &[], Vec::new())]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn signed_division_overflow_wraps() {
    let body = vec![
        expect_i64(
            bin(BinOp::Div, c_int(i64::MIN, Type::I64), c_int(-1, Type::I64)),
            i64::MIN,
        ),
        expect_i64(
            bin(BinOp::Mod, c_int(i64::MIN, Type::I64), c_int(-1, Type::I64)),
            0,
        ),
        ret(c_i64(0)),
    ];
    let m = module(vec![func("main", &[], Type::I64, &[], body)]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn memcopy_overlap_forward_semantics() {
    let base = || cast(addr_local(0), ptr(Type::U8));
    let mut body = vec![
        store(
            index_addr(base(), Type::U8, c_i64(0)),
            Type::U8,
            c_uint(1, Type::U8),
        ),
        store(
            index_addr(base(), Type::U8, c_i64(1)),
            Type::U8,
            c_uint(2, Type::U8),
        ),
        store(
            index_addr(base(), Type::U8, c_i64(2)),
            Type::U8,
            c_uint(3, Type::U8),
        ),
        store(
            index_addr(base(), Type::U8, c_i64(3)),
            Type::U8,
            c_uint(4, Type::U8),
        ),
        Stmt::MemCopy {
            dst: index_addr(base(), Type::U8, c_i64(1)),
            src: base(),
            size: c_u64(4),
        },
    ];
    for (index, expected) in [1u64, 1, 1, 1, 1, 0, 0, 0].iter().enumerate() {
        body.push(expect_expr_eq(
            load(index_addr(base(), Type::U8, c_i64(index as i64)), Type::U8),
            c_uint(*expected, Type::U8),
        ));
    }
    body.push(ret(c_i64(0)));
    let m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[("buf", Type::Array(Box::new(Type::U8), 8))],
        body,
    )]);
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn array_of_structs_layout_and_access() {
    let pair = Type::Struct(StructId(0));
    let base = || cast(addr_local(0), ptr(pair.clone()));
    let mut body = vec![
        expect_u64(Expr::SizeOf(Type::Array(Box::new(pair.clone()), 3)), 48),
        expect_u64(Expr::AlignOf(Type::Array(Box::new(pair.clone()), 3)), 8),
        assign(1, c_i64(0)),
    ];
    body.push(loop_(vec![
        if_(
            bin(BinOp::Ge, local(1), c_i64(3)),
            vec![Stmt::Break],
            Vec::new(),
        ),
        store(
            field_addr(index_addr(base(), pair.clone(), local(1)), 0, 0),
            Type::I32,
            cast(local(1), Type::I32),
        ),
        store(
            field_addr(index_addr(base(), pair.clone(), local(1)), 0, 1),
            Type::I64,
            bin(BinOp::Mul, local(1), c_i64(100)),
        ),
        assign(1, bin(BinOp::Add, local(1), c_i64(1))),
    ]));
    for index in 0..3 {
        body.push(expect_expr_eq(
            load(
                field_addr(index_addr(base(), pair.clone(), c_i64(index)), 0, 0),
                Type::I32,
            ),
            c_i32(i32::try_from(index).expect("small index")),
        ));
        body.push(expect_i64(
            load(
                field_addr(index_addr(base(), pair.clone(), c_i64(index)), 0, 1),
                Type::I64,
            ),
            index * 100,
        ));
    }
    body.push(ret(c_i64(0)));
    let mut m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[
            ("pairs", Type::Array(Box::new(Type::Struct(StructId(0))), 3)),
            ("i", Type::I64),
        ],
        body,
    )]);
    m.structs.push(StructDef {
        name: "Pair".to_string(),
        fields: vec![Type::I32, Type::I64],
    });
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn syscall_exit_is_immediate() {
    let mut m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[],
        vec![
            expr(syscall(
                1,
                vec![
                    c_i64(1),
                    Expr::RodataPtr(tc_ir::RodataId(0)),
                    Expr::RodataLen(tc_ir::RodataId(0)),
                ],
            )),
            expr(syscall(60, vec![c_i64(7)])),
            ret(c_i64(99)),
        ],
    )]);
    m.rodata.push(RodataDef {
        name: "before_exit".to_string(),
        bytes: b"bye\n".to_vec(),
    });
    assert_parity(&m, &["prog"], &[], 7, b"bye\n");
}

#[test]
fn bool_locals_and_memory() {
    let mut m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[("flag", Type::Bool), ("bits", Type::Union(UnionId(0)))],
        vec![
            assign(0, c_bool(true)),
            if_(
                local(0),
                vec![store(
                    union_field_addr(addr_local(1), 0, 0),
                    Type::U8,
                    c_uint(7, Type::U8),
                )],
                Vec::new(),
            ),
            expect_expr_eq(
                load(union_field_addr(addr_local(1), 0, 1), Type::Bool),
                c_bool(true),
            ),
            ret(c_i64(0)),
        ],
    )]);
    m.unions.push(UnionDef {
        name: "Bits".to_string(),
        fields: vec![Type::U8, Type::Bool],
    });
    assert_parity(&m, &["prog"], &[], 0, b"");
}

#[test]
fn zero_libc_elf64_audit() {
    let m = module(vec![func(
        "main",
        &[],
        Type::I64,
        &[],
        vec![ret(c_i64(42))],
    )]);
    verify_module(&m).expect("module verifies");
    let path = unique_elf("audit");
    compile_to_elf(&m, &path).expect("compile audit ELF");

    let run = run_elf(&path, &["prog"], &[]);
    assert_eq!(run.exit_code, 42);

    let ldd = Command::new("ldd").arg(&path).output().expect("run ldd");
    let ldd_text = format!(
        "{}{}",
        String::from_utf8_lossy(&ldd.stdout),
        String::from_utf8_lossy(&ldd.stderr)
    );
    assert!(
        ldd_text.contains("not a dynamic executable"),
        "ldd output: {ldd_text}"
    );

    let dynamic = Command::new("readelf")
        .arg("-d")
        .arg(&path)
        .output()
        .expect("run readelf -d");
    let dynamic_text = String::from_utf8_lossy(&dynamic.stdout);
    assert!(
        dynamic_text.contains("There is no dynamic section"),
        "readelf -d output: {dynamic_text}"
    );

    let program_headers = Command::new("readelf")
        .arg("-l")
        .arg(&path)
        .output()
        .expect("run readelf -l");
    let program_text = String::from_utf8_lossy(&program_headers.stdout);
    assert!(
        !program_text.contains("INTERP"),
        "unexpected INTERP segment: {program_text}"
    );

    let file = Command::new("file").arg(&path).output().expect("run file");
    let file_text = String::from_utf8_lossy(&file.stdout);
    assert!(
        file_text.contains("ELF 64-bit") && file_text.contains("statically linked"),
        "file output: {file_text}"
    );

    let _ = std::fs::remove_file(&path);
}
