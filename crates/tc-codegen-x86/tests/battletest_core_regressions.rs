//! Phase 3.5 permanent regressions for Core-7 float/int and cast parity.

use std::os::unix::process::CommandExt as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use tc_codegen_x86::compile_to_elf;
use tc_ir::{
    BinOp, Expr, FnId, Function, LocalDef, Module, Stmt, Type, UnOp, interpret, verify_module,
};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_elf(tag: &str) -> PathBuf {
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "tc_r1_battletest_codegen_{}_{}_{tag}.elf",
        std::process::id(),
        counter
    ))
}

fn module_with_main(body: Vec<Stmt>) -> Module {
    Module {
        structs: Vec::new(),
        unions: Vec::new(),
        rodata: Vec::new(),
        globals: Vec::new(),
        functions: vec![Function {
            name: "main".to_string(),
            params: Vec::new(),
            ret: Type::I64,
            locals: Vec::new(),
            body,
        }],
        entry: FnId(0),
    }
}

fn i64_const(value: i64) -> Expr {
    Expr::ConstInt(value, Type::I64)
}

fn assert_parity(module: &Module, expected_exit: i32) {
    verify_module(module).unwrap_or_else(|errors| panic!("verification failed: {errors:#?}"));
    let args = vec!["prog".to_string()];
    let interp = interpret(module, &args, &[], 5_000_000).expect("interpreter");
    assert_eq!(interp.exit_code, expected_exit, "interpreter exit");
    let path = unique_elf("cast");
    compile_to_elf(module, &path).expect("ELF");
    let output = Command::new(&path)
        .arg0("prog")
        .env_clear()
        .stdin(Stdio::null())
        .output()
        .expect("run ELF");
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        output.status.code().unwrap_or(-1),
        interp.exit_code,
        "ELF/interpreter exit mismatch; stderr={:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, interp.stdout);
    assert_eq!(output.stderr, interp.stderr);
    assert_eq!(output.status.code().unwrap_or(-1), expected_exit);
}

fn cast(value: Expr, to: Type) -> Expr {
    Expr::Cast {
        val: Box::new(value),
        to,
    }
}

fn return_low_byte(value: Expr) -> Module {
    module_with_main(vec![Stmt::Return(Some(Expr::Binary {
        op: BinOp::BitAnd,
        lhs: Box::new(value),
        rhs: Box::new(i64_const(255)),
    }))])
}

fn return_if(condition: Expr, yes: i64, no: i64) -> Module {
    module_with_main(vec![Stmt::If {
        cond: condition,
        then_body: vec![Stmt::Return(Some(i64_const(yes)))],
        else_body: vec![Stmt::Return(Some(i64_const(no)))],
    }])
}

#[test]
fn huge_float_to_i64_saturates_to_max() {
    let value = cast(Expr::ConstFloat(1.0e300, Type::F64), Type::I64);
    assert_parity(&return_low_byte(value), 255);
}

#[test]
fn huge_negative_float_to_i64_saturates_to_min() {
    let value = cast(Expr::ConstFloat(-1.0e300, Type::F64), Type::I64);
    let condition = Expr::Binary {
        op: BinOp::Lt,
        lhs: Box::new(value),
        rhs: Box::new(i64_const(0)),
    };
    assert_parity(&return_if(condition, 1, 0), 1);
}

#[test]
fn nan_float_to_i64_is_zero() {
    let nan = Expr::Binary {
        op: BinOp::Div,
        lhs: Box::new(Expr::ConstFloat(0.0, Type::F64)),
        rhs: Box::new(Expr::ConstFloat(0.0, Type::F64)),
    };
    let value = cast(nan, Type::I64);
    let condition = Expr::Binary {
        op: BinOp::Eq,
        lhs: Box::new(value),
        rhs: Box::new(i64_const(0)),
    };
    assert_parity(&return_if(condition, 1, 0), 1);
}

#[test]
fn float_to_narrow_signed_saturates_at_width() {
    let value = cast(Expr::ConstFloat(200.0, Type::F64), Type::I8);
    let widened = cast(value, Type::I64);
    assert_parity(&return_low_byte(widened), 127);
}

#[test]
fn float_to_narrow_unsigned_saturates_at_width() {
    let value = cast(Expr::ConstFloat(70_000.0, Type::F64), Type::U16);
    let widened = cast(value, Type::I64);
    assert_parity(&return_low_byte(widened), 255);
}

#[test]
fn negative_float_to_u64_is_zero() {
    let value = cast(Expr::ConstFloat(-1.0, Type::F64), Type::U64);
    let condition = Expr::Binary {
        op: BinOp::Eq,
        lhs: Box::new(value),
        rhs: Box::new(Expr::ConstUint(0, Type::U64)),
    };
    assert_parity(&return_if(condition, 1, 0), 1);
}

#[test]
fn huge_float_to_u64_saturates_to_max() {
    let value = cast(Expr::ConstFloat(1.0e300, Type::F64), Type::U64);
    let condition = Expr::Binary {
        op: BinOp::Eq,
        lhs: Box::new(value),
        rhs: Box::new(Expr::ConstUint(u64::MAX, Type::U64)),
    };
    assert_parity(&return_if(condition, 1, 0), 1);
}

#[test]
fn u64_max_to_f64_is_positive_and_large() {
    let value = cast(Expr::ConstUint(u64::MAX, Type::U64), Type::F64);
    let condition = Expr::Binary {
        op: BinOp::Gt,
        lhs: Box::new(value),
        rhs: Box::new(Expr::ConstFloat(1.0e19, Type::F64)),
    };
    assert_parity(&return_if(condition, 1, 0), 1);
}

#[test]
fn f32_to_i32_saturates_and_normalizes() {
    let value = cast(Expr::ConstFloat(1.0e30, Type::F32), Type::I32);
    let widened = cast(value, Type::I64);
    assert_parity(&return_low_byte(widened), 255);
}

#[test]
fn negating_i64_min_wraps_identically() {
    let minimum = Expr::Binary {
        op: BinOp::Sub,
        lhs: Box::new(i64_const(-9_223_372_036_854_775_807)),
        rhs: Box::new(i64_const(1)),
    };
    let negated = Expr::Unary {
        op: UnOp::Neg,
        val: Box::new(minimum),
    };
    assert_parity(&return_low_byte(negated), 0);
}

#[test]
fn division_by_minus_one_matches_interpreter() {
    let minimum = Expr::Binary {
        op: BinOp::Sub,
        lhs: Box::new(i64_const(-9_223_372_036_854_775_807)),
        rhs: Box::new(i64_const(1)),
    };
    let quotient = Expr::Binary {
        op: BinOp::Div,
        lhs: Box::new(minimum),
        rhs: Box::new(i64_const(-1)),
    };
    assert_parity(&return_low_byte(quotient), 0);
}

#[test]
fn remainder_by_minus_one_matches_interpreter() {
    let minimum = Expr::Binary {
        op: BinOp::Sub,
        lhs: Box::new(i64_const(-9_223_372_036_854_775_807)),
        rhs: Box::new(i64_const(1)),
    };
    let remainder = Expr::Binary {
        op: BinOp::Mod,
        lhs: Box::new(minimum),
        rhs: Box::new(i64_const(-1)),
    };
    assert_parity(&return_low_byte(remainder), 0);
}

#[test]
fn unsigned_shift_and_signed_shift_differ() {
    let shifted = Expr::Binary {
        op: BinOp::Shr,
        lhs: Box::new(Expr::ConstUint(0x8000_0000_0000_0000, Type::U64)),
        rhs: Box::new(Expr::ConstUint(63, Type::U64)),
    };
    let module = module_with_main(vec![Stmt::Return(Some(Expr::Cast {
        val: Box::new(shifted),
        to: Type::I64,
    }))]);
    assert_parity(&module, 1);
}

#[test]
fn zero_initialized_local_defaults_are_clean() {
    let module = Module {
        structs: Vec::new(),
        unions: Vec::new(),
        rodata: Vec::new(),
        globals: Vec::new(),
        functions: vec![Function {
            name: "main".to_string(),
            params: Vec::new(),
            ret: Type::I64,
            locals: vec![LocalDef {
                name: "unused".to_string(),
                ty: Type::I64,
            }],
            body: vec![Stmt::Return(Some(i64_const(0)))],
        }],
        entry: FnId(0),
    };
    assert_parity(&module, 0);
}
