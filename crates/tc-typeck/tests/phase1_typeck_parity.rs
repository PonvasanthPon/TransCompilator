//! Phase 1 end-to-end parity tests: AST -> tc-typeck -> tc-ir -> ELF64.

use std::io::Write as _;
use std::os::unix::process::CommandExt as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use tc_codegen_x86::compile_to_elf;
use tc_ir::{BinOp, UnOp, interpret, verify_module};
use tc_typeck::collect_phrases;
use tc_typeck::{
    AstExpr, AstFn, AstGlobal, AstModule, AstStmt, AstStruct, AstType, AstUnion, Import,
    PhrasePart, PhrasePattern, Program, Span, TypeErrorKind, Visibility, check_and_lower,
};

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
        "tc_r1_phase1_{}_{}_{tag}.elf",
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

fn assert_program_parity(
    program: &Program,
    args: &[&str],
    stdin: &[u8],
    expected_exit: i32,
    expected_stdout: &[u8],
) {
    let module = check_and_lower(program)
        .unwrap_or_else(|errors| panic!("type checking/lowering failed: {errors:#?}"));
    verify_module(&module)
        .unwrap_or_else(|errors| panic!("lowered IR verification failed: {errors:#?}"));

    let owned_args = args.iter().map(ToString::to_string).collect::<Vec<_>>();
    let interp = interpret(&module, &owned_args, stdin, 20_000_000)
        .unwrap_or_else(|error| panic!("interpreter failed: {error:?}"));
    assert_eq!(
        interp.exit_code,
        expected_exit,
        "interpreter exit mismatch; stdout={:?}",
        String::from_utf8_lossy(&interp.stdout)
    );
    assert_eq!(
        interp.stdout, expected_stdout,
        "interpreter stdout mismatch"
    );

    let path = unique_elf("parity");
    compile_to_elf(&module, &path)
        .unwrap_or_else(|error| panic!("ELF compilation failed: {error}"));
    let elf = run_elf(&path, args, stdin);
    let _ = std::fs::remove_file(&path);

    assert_eq!(
        elf.exit_code,
        interp.exit_code,
        "ELF/interpreter exit mismatch; stderr={:?}",
        String::from_utf8_lossy(&elf.stderr)
    );
    assert_eq!(
        elf.stdout,
        interp.stdout,
        "ELF/interpreter stdout mismatch; stderr={:?}",
        String::from_utf8_lossy(&elf.stderr)
    );
    assert_eq!(elf.stderr, interp.stderr);
    assert_eq!(elf.exit_code, expected_exit);
    assert_eq!(elf.stdout, expected_stdout);
}

fn expect_error(program: &Program, kind: TypeErrorKind) -> Vec<tc_typeck::TypeError> {
    let errors = check_and_lower(program).expect_err("program should be rejected");
    assert!(
        errors.iter().any(|error| error.kind == kind),
        "expected {kind:?}, got {errors:#?}"
    );
    errors
}

fn expect_phrase_error(program: &Program, kind: TypeErrorKind) {
    let errors = collect_phrases(program).expect_err("phrases should be rejected");
    assert!(
        errors.iter().any(|error| error.kind == kind),
        "expected {kind:?}, got {errors:#?}"
    );
}

fn span(module: &str, line: u32) -> Span {
    Span::new(module, line, 1)
}

fn t_void() -> AstType {
    AstType::Void
}

fn t_bool() -> AstType {
    AstType::Bool
}

fn t_i8() -> AstType {
    AstType::I8
}

fn t_i16() -> AstType {
    AstType::I16
}

fn t_i32() -> AstType {
    AstType::I32
}

fn t_i64() -> AstType {
    AstType::I64
}

fn t_u8() -> AstType {
    AstType::U8
}

fn t_u16() -> AstType {
    AstType::U16
}

fn t_u32() -> AstType {
    AstType::U32
}

fn t_u64() -> AstType {
    AstType::U64
}

fn t_f32() -> AstType {
    AstType::F32
}

fn t_f64() -> AstType {
    AstType::F64
}

fn t_ptr(inner: AstType) -> AstType {
    AstType::ptr(inner)
}

fn t_array(inner: AstType, count: usize) -> AstType {
    AstType::array(inner, count)
}

fn t_named(name: &str) -> AstType {
    AstType::named(name)
}

fn t_named_args(name: &str, args: Vec<AstType>) -> AstType {
    AstType::Named {
        module: None,
        name: name.to_string(),
        type_args: args,
    }
}

fn t_module_named(module: &str, name: &str) -> AstType {
    AstType::Named {
        module: Some(module.to_string()),
        name: name.to_string(),
        type_args: Vec::new(),
    }
}

fn t_fn_ptr(params: Vec<AstType>, ret: AstType) -> AstType {
    AstType::FnPtr {
        params,
        ret: Box::new(ret),
    }
}

fn int(value: i64) -> AstExpr {
    AstExpr::ConstInt(value, None)
}

fn int_typed(value: i64, ty: AstType) -> AstExpr {
    AstExpr::ConstInt(value, Some(ty))
}

fn uint(value: u64) -> AstExpr {
    AstExpr::ConstUint(value, None)
}

fn uint_typed(value: u64, ty: AstType) -> AstExpr {
    AstExpr::ConstUint(value, Some(ty))
}

fn float(value: f64) -> AstExpr {
    AstExpr::ConstFloat(value, None)
}

fn float_typed(value: f64, ty: AstType) -> AstExpr {
    AstExpr::ConstFloat(value, Some(ty))
}

fn boolean(value: bool) -> AstExpr {
    AstExpr::ConstBool(value)
}

fn null(ty: AstType) -> AstExpr {
    AstExpr::ConstNull(ty)
}

fn bytes(bytes: &[u8]) -> AstExpr {
    AstExpr::BytesLit(bytes.to_vec())
}

fn bytes_len(bytes: &[u8]) -> AstExpr {
    AstExpr::BytesLen(bytes.to_vec())
}

fn variable(name: &str) -> AstExpr {
    AstExpr::Var(name.to_string())
}

fn addr_var(name: &str) -> AstExpr {
    AstExpr::AddrOfVar(name.to_string())
}

fn global_addr(name: &str) -> AstExpr {
    AstExpr::GlobalAddr {
        module: None,
        name: name.to_string(),
    }
}

fn field_addr(base: AstExpr, field: &str) -> AstExpr {
    AstExpr::FieldAddr {
        base_ptr: Box::new(base),
        field: field.to_string(),
    }
}

fn index_addr(base: AstExpr, index: AstExpr) -> AstExpr {
    AstExpr::IndexAddr {
        base_ptr: Box::new(base),
        index: Box::new(index),
    }
}

fn load(ptr: AstExpr) -> AstExpr {
    AstExpr::Load(Box::new(ptr))
}

fn size_of(ty: AstType) -> AstExpr {
    AstExpr::SizeOf(ty)
}

fn align_of(ty: AstType) -> AstExpr {
    AstExpr::AlignOf(ty)
}

fn unary(op: UnOp, val: AstExpr) -> AstExpr {
    AstExpr::Unary {
        op,
        val: Box::new(val),
    }
}

fn binary(op: BinOp, lhs: AstExpr, rhs: AstExpr) -> AstExpr {
    AstExpr::Binary {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
    }
}

fn cast(val: AstExpr, to: AstType) -> AstExpr {
    AstExpr::Cast {
        val: Box::new(val),
        to,
    }
}

fn call(name: &str, type_args: Vec<AstType>, args: Vec<AstExpr>) -> AstExpr {
    AstExpr::Call {
        module: None,
        name: name.to_string(),
        type_args,
        args,
    }
}

fn module_call(module: &str, name: &str, type_args: Vec<AstType>, args: Vec<AstExpr>) -> AstExpr {
    AstExpr::Call {
        module: Some(module.to_string()),
        name: name.to_string(),
        type_args,
        args,
    }
}

fn fn_addr(name: &str, type_args: Vec<AstType>) -> AstExpr {
    AstExpr::FnAddr {
        module: None,
        name: name.to_string(),
        type_args,
    }
}

fn indirect_call(callee: AstExpr, args: Vec<AstExpr>) -> AstExpr {
    AstExpr::IndirectCall {
        callee: Box::new(callee),
        args,
    }
}

fn syscall(num: AstExpr, args: Vec<AstExpr>) -> AstExpr {
    AstExpr::Syscall {
        num: Box::new(num),
        args,
    }
}

fn let_stmt(name: &str, ty: Option<AstType>, init: AstExpr) -> AstStmt {
    AstStmt::Let {
        name: name.to_string(),
        ty,
        init,
        span: span("test", 1),
    }
}

fn assign(name: &str, val: AstExpr) -> AstStmt {
    AstStmt::Assign {
        name: name.to_string(),
        val,
        span: span("test", 1),
    }
}

fn store(ptr: AstExpr, val: AstExpr) -> AstStmt {
    AstStmt::Store {
        ptr,
        val,
        span: span("test", 1),
    }
}

fn memcopy(dst: AstExpr, src: AstExpr, size: AstExpr) -> AstStmt {
    AstStmt::MemCopy {
        dst,
        src,
        size,
        span: span("test", 1),
    }
}

fn expr(value: AstExpr) -> AstStmt {
    AstStmt::Expr(value, span("test", 1))
}

fn block(body: Vec<AstStmt>) -> AstStmt {
    AstStmt::Block(body, span("test", 1))
}

fn if_(cond: AstExpr, then_body: Vec<AstStmt>, else_body: Vec<AstStmt>) -> AstStmt {
    AstStmt::If {
        cond,
        then_body,
        else_body,
        span: span("test", 1),
    }
}

fn loop_(body: Vec<AstStmt>) -> AstStmt {
    AstStmt::Loop {
        body,
        span: span("test", 1),
    }
}

fn break_stmt() -> AstStmt {
    AstStmt::Break(span("test", 1))
}

fn continue_stmt() -> AstStmt {
    AstStmt::Continue(span("test", 1))
}

fn ret(value: AstExpr) -> AstStmt {
    AstStmt::Return(Some(value), span("test", 1))
}

fn ret_void() -> AstStmt {
    AstStmt::Return(None, span("test", 1))
}

fn trap() -> AstStmt {
    AstStmt::Trap(span("test", 1))
}

fn param(name: &str, ty: AstType) -> (String, AstType) {
    (name.to_string(), ty)
}

fn ast_fn(
    vis: Visibility,
    name: &str,
    type_params: Vec<&str>,
    params: Vec<(String, AstType)>,
    ret: AstType,
    body: Vec<AstStmt>,
) -> AstFn {
    AstFn {
        vis,
        name: name.to_string(),
        type_params: type_params.into_iter().map(ToString::to_string).collect(),
        params,
        ret,
        phrases: Vec::new(),
        body,
        span: span("test", 1),
    }
}

fn public_fn(
    name: &str,
    params: Vec<(String, AstType)>,
    ret: AstType,
    body: Vec<AstStmt>,
) -> AstFn {
    ast_fn(Visibility::Public, name, Vec::new(), params, ret, body)
}

fn ast_struct(
    vis: Visibility,
    name: &str,
    type_params: Vec<&str>,
    fields: Vec<(String, AstType)>,
) -> AstStruct {
    AstStruct {
        vis,
        name: name.to_string(),
        type_params: type_params.into_iter().map(ToString::to_string).collect(),
        fields,
        span: span("test", 1),
    }
}

fn ast_union(
    vis: Visibility,
    name: &str,
    type_params: Vec<&str>,
    fields: Vec<(String, AstType)>,
) -> AstUnion {
    AstUnion {
        vis,
        name: name.to_string(),
        type_params: type_params.into_iter().map(ToString::to_string).collect(),
        fields,
        span: span("test", 1),
    }
}

fn ast_global(vis: Visibility, name: &str, ty: AstType, init: Option<Vec<u8>>) -> AstGlobal {
    AstGlobal {
        vis,
        name: name.to_string(),
        ty,
        init,
        span: span("test", 1),
    }
}

fn import(module: &str, alias: Option<&str>) -> Import {
    Import {
        module: module.to_string(),
        alias: alias.map(ToString::to_string),
        span: span("test", 1),
    }
}

fn ast_module(
    name: &str,
    imports: Vec<Import>,
    structs: Vec<AstStruct>,
    unions: Vec<AstUnion>,
    globals: Vec<AstGlobal>,
    functions: Vec<AstFn>,
) -> AstModule {
    AstModule {
        name: name.to_string(),
        imports,
        structs,
        unions,
        globals,
        functions,
    }
}

fn program(modules: Vec<AstModule>, entry_module: &str, entry_fn: &str) -> Program {
    Program {
        modules,
        entry_module: entry_module.to_string(),
        entry_fn: entry_fn.to_string(),
    }
}

fn phrase(parts: Vec<PhrasePart>) -> PhrasePattern {
    PhrasePattern { parts }
}

fn word(value: &str) -> PhrasePart {
    PhrasePart::Word(value.to_string())
}

fn phrase_param(name: &str) -> PhrasePart {
    PhrasePart::Param(name.to_string())
}

#[test]
fn exit_42_end_to_end() {
    let main = public_fn("main", Vec::new(), t_i64(), vec![ret(int(42))]);
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 42, b"");
}

#[test]
fn two_module_import_and_call() {
    let add = public_fn(
        "add",
        vec![param("a", t_i64()), param("b", t_i64())],
        t_i64(),
        vec![ret(binary(BinOp::Add, variable("a"), variable("b")))],
    );
    let math = ast_module("math", vec![], vec![], vec![], vec![], vec![add]);
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![ret(module_call(
            "math",
            "add",
            Vec::new(),
            vec![int(40), int(2)],
        ))],
    );
    let main_module = ast_module(
        "main",
        vec![import("math", None)],
        vec![],
        vec![],
        vec![],
        vec![main],
    );
    let p = program(vec![math, main_module], "main", "main");
    assert_program_parity(&p, &["prog"], &[], 42, b"");
}

#[test]
fn generic_fn_explicit_type_args() {
    let identity = ast_fn(
        Visibility::Public,
        "identity",
        vec!["T"],
        vec![param("x", t_named("T"))],
        t_named("T"),
        vec![ret(variable("x"))],
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![ret(call("identity", vec![t_i64()], vec![int(42)]))],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main, identity],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 42, b"");
}

#[test]
fn local_type_inference_scalars() {
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt("x", None, int(10)),
            let_stmt("f", None, float(1.5)),
            let_stmt("b", None, boolean(true)),
            let_stmt("p", None, addr_var("x")),
            ret(load(variable("p"))),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 10, b"");
}

#[test]
fn lexical_scope_shadowing() {
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt("x", None, int(1)),
            let_stmt("observed", None, int(0)),
            block(vec![
                let_stmt("x", Some(t_f64()), float(2.5)),
                assign("observed", cast(variable("x"), t_i64())),
            ]),
            ret(binary(BinOp::Add, variable("x"), variable("observed"))),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 3, b"");
}

#[test]
fn loop_and_if_scope_isolation() {
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt("x", None, int(5)),
            if_(
                boolean(true),
                vec![let_stmt("y", None, int(9))],
                vec![let_stmt("z", None, int(8))],
            ),
            loop_(vec![
                let_stmt("x", Some(t_i64()), int(9)),
                if_(
                    binary(BinOp::Eq, variable("x"), int(9)),
                    vec![break_stmt()],
                    Vec::new(),
                ),
            ]),
            ret(variable("x")),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 5, b"");
}

#[test]
fn diamond_module_dependency() {
    let shared = public_fn(
        "shared",
        vec![param("x", t_i64())],
        t_i64(),
        vec![ret(binary(BinOp::Add, variable("x"), int(1)))],
    );
    let base = ast_module("base", vec![], vec![], vec![], vec![], vec![shared]);
    let left = public_fn(
        "left",
        vec![param("x", t_i64())],
        t_i64(),
        vec![ret(binary(
            BinOp::Add,
            module_call("base", "shared", Vec::new(), vec![variable("x")]),
            int(1),
        ))],
    );
    let left_module = ast_module(
        "left",
        vec![import("base", None)],
        vec![],
        vec![],
        vec![],
        vec![left],
    );
    let right = public_fn(
        "right",
        vec![param("x", t_i64())],
        t_i64(),
        vec![ret(binary(
            BinOp::Add,
            module_call("base", "shared", Vec::new(), vec![variable("x")]),
            int(2),
        ))],
    );
    let right_module = ast_module(
        "right",
        vec![import("base", None)],
        vec![],
        vec![],
        vec![],
        vec![right],
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![ret(binary(
            BinOp::Add,
            call("left", Vec::new(), vec![int(1)]),
            call("right", Vec::new(), vec![int(2)]),
        ))],
    );
    let main_module = ast_module(
        "main",
        vec![import("left", None), import("right", None)],
        vec![],
        vec![],
        vec![],
        vec![main],
    );
    let p = program(
        vec![base, left_module, right_module, main_module],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 8, b"");
}

#[test]
fn cross_module_struct_and_global() {
    let point = ast_struct(
        Visibility::Public,
        "Point",
        Vec::new(),
        vec![("x".to_string(), t_i64()), ("y".to_string(), t_i64())],
    );
    let origin = ast_global(
        Visibility::Public,
        "origin",
        t_named("Point"),
        Some(vec![0; 16]),
    );
    let offset = ast_global(
        Visibility::Public,
        "offset",
        t_i64(),
        Some(10i64.to_le_bytes().to_vec()),
    );
    let data = ast_module(
        "data",
        vec![],
        vec![point],
        vec![],
        vec![origin, offset],
        vec![],
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt(
                "p",
                Some(t_module_named("data", "Point")),
                load(AstExpr::GlobalAddr {
                    module: Some("data".to_string()),
                    name: "origin".to_string(),
                }),
            ),
            store(field_addr(addr_var("p"), "x"), int(5)),
            store(field_addr(addr_var("p"), "y"), int(7)),
            ret(binary(
                BinOp::Add,
                binary(
                    BinOp::Add,
                    load(field_addr(addr_var("p"), "x")),
                    load(field_addr(addr_var("p"), "y")),
                ),
                load(AstExpr::GlobalAddr {
                    module: Some("data".to_string()),
                    name: "offset".to_string(),
                }),
            )),
        ],
    );
    let main_module = ast_module(
        "main",
        vec![import("data", None)],
        vec![],
        vec![],
        vec![],
        vec![main],
    );
    let p = program(vec![data, main_module], "main", "main");
    assert_program_parity(&p, &["prog"], &[], 22, b"");
}

#[test]
fn module_alias_import() {
    let add = public_fn(
        "add",
        vec![param("a", t_i64()), param("b", t_i64())],
        t_i64(),
        vec![ret(binary(BinOp::Add, variable("a"), variable("b")))],
    );
    let math = ast_module("math", vec![], vec![], vec![], vec![], vec![add]);
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![ret(module_call(
            "m",
            "add",
            Vec::new(),
            vec![int(40), int(2)],
        ))],
    );
    let main_module = ast_module(
        "main",
        vec![import("math", Some("m"))],
        vec![],
        vec![],
        vec![],
        vec![main],
    );
    let p = program(vec![math, main_module], "main", "main");
    assert_program_parity(&p, &["prog"], &[], 42, b"");
}

#[test]
fn named_struct_field_access() {
    let point = ast_struct(
        Visibility::Public,
        "Point",
        Vec::new(),
        vec![("x".to_string(), t_i64()), ("y".to_string(), t_i64())],
    );
    let rect = ast_struct(
        Visibility::Public,
        "Rect",
        Vec::new(),
        vec![
            ("origin".to_string(), t_named("Point")),
            ("w".to_string(), t_i64()),
            ("h".to_string(), t_i64()),
        ],
    );
    let template = ast_global(
        Visibility::Private,
        "rect_zero",
        t_named("Rect"),
        Some(vec![0; 32]),
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt(
                "rect",
                Some(t_named("Rect")),
                load(global_addr("rect_zero")),
            ),
            store(
                field_addr(field_addr(addr_var("rect"), "origin"), "x"),
                int(2),
            ),
            store(
                field_addr(field_addr(addr_var("rect"), "origin"), "y"),
                int(3),
            ),
            store(field_addr(addr_var("rect"), "w"), int(4)),
            store(field_addr(addr_var("rect"), "h"), int(5)),
            ret(binary(
                BinOp::Add,
                binary(
                    BinOp::Add,
                    load(field_addr(field_addr(addr_var("rect"), "origin"), "x")),
                    load(field_addr(field_addr(addr_var("rect"), "origin"), "y")),
                ),
                binary(
                    BinOp::Add,
                    load(field_addr(addr_var("rect"), "w")),
                    load(field_addr(addr_var("rect"), "h")),
                ),
            )),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![point, rect],
            vec![],
            vec![template],
            vec![main],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 14, b"");
}

#[test]
fn named_union_f64_u64_bit_pun() {
    let bits = ast_union(
        Visibility::Public,
        "Bits",
        Vec::new(),
        vec![("f".to_string(), t_f64()), ("u".to_string(), t_u64())],
    );
    let template = ast_global(
        Visibility::Private,
        "bits_zero",
        t_named("Bits"),
        Some(vec![0; 8]),
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt(
                "bits",
                Some(t_named("Bits")),
                load(global_addr("bits_zero")),
            ),
            store(field_addr(addr_var("bits"), "f"), float(1.5)),
            if_(
                binary(
                    BinOp::Ne,
                    load(field_addr(addr_var("bits"), "u")),
                    uint_typed(0x3ff8_0000_0000_0000, t_u64()),
                ),
                vec![trap()],
                Vec::new(),
            ),
            ret(int(0)),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![bits],
            vec![template],
            vec![main],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 0, b"");
}

#[test]
fn struct_memcopy_between_locals() {
    let s = ast_struct(
        Visibility::Public,
        "S",
        Vec::new(),
        vec![("a".to_string(), t_i64()), ("b".to_string(), t_i32())],
    );
    let template = ast_global(
        Visibility::Private,
        "s_zero",
        t_named("S"),
        Some(vec![0; 16]),
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt("src", Some(t_named("S")), load(global_addr("s_zero"))),
            let_stmt("dst", Some(t_named("S")), load(global_addr("s_zero"))),
            store(field_addr(addr_var("src"), "a"), int(123)),
            store(field_addr(addr_var("src"), "b"), int_typed(-7, t_i32())),
            memcopy(addr_var("dst"), addr_var("src"), size_of(t_named("S"))),
            if_(
                binary(BinOp::Ne, load(field_addr(addr_var("dst"), "a")), int(123)),
                vec![trap()],
                Vec::new(),
            ),
            if_(
                binary(
                    BinOp::Ne,
                    load(field_addr(addr_var("dst"), "b")),
                    int_typed(-7, t_i32()),
                ),
                vec![trap()],
                Vec::new(),
            ),
            ret(int(0)),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![s],
            vec![],
            vec![template],
            vec![main],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 0, b"");
}

#[test]
fn generic_fn_inferred_type_args() {
    let swap = ast_fn(
        Visibility::Public,
        "swap",
        vec!["T"],
        vec![
            param("a", t_ptr(t_named("T"))),
            param("b", t_ptr(t_named("T"))),
        ],
        t_void(),
        vec![
            let_stmt("temp", None, load(variable("a"))),
            store(variable("a"), load(variable("b"))),
            store(variable("b"), variable("temp")),
            ret_void(),
        ],
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt("x", None, int(1)),
            let_stmt("y", None, int(2)),
            let_stmt("f1", None, float(1.5)),
            let_stmt("f2", None, float(2.5)),
            expr(call("swap", Vec::new(), vec![addr_var("x"), addr_var("y")])),
            expr(call(
                "swap",
                Vec::new(),
                vec![addr_var("f1"), addr_var("f2")],
            )),
            if_(
                binary(
                    BinOp::LogOr,
                    binary(BinOp::Ne, variable("x"), int(2)),
                    binary(BinOp::Ne, variable("y"), int(1)),
                ),
                vec![trap()],
                Vec::new(),
            ),
            if_(
                binary(
                    BinOp::LogOr,
                    binary(BinOp::Ne, variable("f1"), float(2.5)),
                    binary(BinOp::Ne, variable("f2"), float(1.5)),
                ),
                vec![trap()],
                Vec::new(),
            ),
            ret(int(0)),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main, swap],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 0, b"");
}

#[test]
fn generic_struct_pair_multi_instantiation() {
    let point = ast_struct(
        Visibility::Public,
        "Point",
        Vec::new(),
        vec![("x".to_string(), t_i64()), ("y".to_string(), t_i64())],
    );
    let pair = ast_struct(
        Visibility::Public,
        "Pair",
        vec!["A", "B"],
        vec![
            ("first".to_string(), t_named("A")),
            ("second".to_string(), t_named("B")),
        ],
    );
    let p1_zero = ast_global(
        Visibility::Private,
        "p1_zero",
        t_named_args("Pair", vec![t_i64(), t_f64()]),
        Some(vec![0; 16]),
    );
    let p2_zero = ast_global(
        Visibility::Private,
        "p2_zero",
        t_named_args("Pair", vec![t_i64(), t_named("Point")]),
        Some(vec![0; 24]),
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt(
                "p1",
                Some(t_named_args("Pair", vec![t_i64(), t_f64()])),
                load(global_addr("p1_zero")),
            ),
            let_stmt(
                "p2",
                Some(t_named_args("Pair", vec![t_i64(), t_named("Point")])),
                load(global_addr("p2_zero")),
            ),
            store(field_addr(addr_var("p1"), "first"), int(10)),
            store(field_addr(addr_var("p1"), "second"), float(1.5)),
            store(field_addr(addr_var("p2"), "first"), int(20)),
            store(
                field_addr(field_addr(addr_var("p2"), "second"), "x"),
                int(30),
            ),
            store(
                field_addr(field_addr(addr_var("p2"), "second"), "y"),
                int(40),
            ),
            if_(
                binary(
                    BinOp::Ne,
                    size_of(t_named_args("Pair", vec![t_i64(), t_named("Point")])),
                    uint_typed(24, t_u64()),
                ),
                vec![trap()],
                Vec::new(),
            ),
            ret(binary(
                BinOp::Add,
                binary(
                    BinOp::Add,
                    load(field_addr(addr_var("p1"), "first")),
                    load(field_addr(addr_var("p2"), "first")),
                ),
                binary(
                    BinOp::Add,
                    load(field_addr(field_addr(addr_var("p2"), "second"), "x")),
                    load(field_addr(field_addr(addr_var("p2"), "second"), "y")),
                ),
            )),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![point, pair],
            vec![],
            vec![p1_zero, p2_zero],
            vec![main],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 100, b"");
}

#[test]
fn nested_generic_struct() {
    let pair = ast_struct(
        Visibility::Public,
        "Pair",
        vec!["A", "B"],
        vec![
            ("first".to_string(), t_named("A")),
            ("second".to_string(), t_named("B")),
        ],
    );
    let nested_ty = t_named_args(
        "Pair",
        vec![t_i64(), t_named_args("Pair", vec![t_u8(), t_i64()])],
    );
    let template = ast_global(
        Visibility::Private,
        "nested_zero",
        nested_ty.clone(),
        Some(vec![0; 24]),
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt(
                "nested",
                Some(nested_ty.clone()),
                load(global_addr("nested_zero")),
            ),
            store(field_addr(addr_var("nested"), "first"), int(7)),
            store(
                field_addr(field_addr(addr_var("nested"), "second"), "first"),
                uint_typed(255, t_u8()),
            ),
            store(
                field_addr(field_addr(addr_var("nested"), "second"), "second"),
                int(11),
            ),
            if_(
                binary(
                    BinOp::Ne,
                    size_of(nested_ty.clone()),
                    uint_typed(24, t_u64()),
                ),
                vec![trap()],
                Vec::new(),
            ),
            if_(
                binary(
                    BinOp::Ne,
                    align_of(nested_ty.clone()),
                    uint_typed(8, t_u64()),
                ),
                vec![trap()],
                Vec::new(),
            ),
            if_(
                binary(
                    BinOp::Ne,
                    binary(
                        BinOp::Add,
                        load(field_addr(addr_var("nested"), "first")),
                        binary(
                            BinOp::Add,
                            cast(
                                load(field_addr(
                                    field_addr(addr_var("nested"), "second"),
                                    "first",
                                )),
                                t_i64(),
                            ),
                            load(field_addr(
                                field_addr(addr_var("nested"), "second"),
                                "second",
                            )),
                        ),
                    ),
                    int(273),
                ),
                vec![trap()],
                Vec::new(),
            ),
            ret(int(0)),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![pair],
            vec![],
            vec![template],
            vec![main],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 0, b"");
}

fn point_bytes(x: i64, y: i64) -> Vec<u8> {
    let mut bytes = x.to_le_bytes().to_vec();
    bytes.extend_from_slice(&y.to_le_bytes());
    bytes
}

#[test]
fn generic_stack_i64_and_point_over_mmap() {
    let point = ast_struct(
        Visibility::Public,
        "Point",
        Vec::new(),
        vec![("x".to_string(), t_i64()), ("y".to_string(), t_i64())],
    );
    let stack = ast_struct(
        Visibility::Public,
        "Stack",
        vec!["T"],
        vec![
            ("data".to_string(), t_ptr(t_named("T"))),
            ("len".to_string(), t_u64()),
            ("cap".to_string(), t_u64()),
        ],
    );
    let point0 = ast_global(
        Visibility::Private,
        "point0",
        t_named("Point"),
        Some(point_bytes(1, 2)),
    );
    let point1 = ast_global(
        Visibility::Private,
        "point1",
        t_named("Point"),
        Some(point_bytes(10, 20)),
    );
    let point2 = ast_global(
        Visibility::Private,
        "point2",
        t_named("Point"),
        Some(point_bytes(100, 200)),
    );

    let stack_ty = || t_named_args("Stack", vec![t_named("T")]);
    let stack_ptr_ty = || t_ptr(stack_ty());
    let init = ast_fn(
        Visibility::Public,
        "stack_init",
        vec!["T"],
        vec![param("cap", t_u64())],
        stack_ptr_ty(),
        vec![
            let_stmt(
                "stack_raw",
                None,
                syscall(
                    int(9),
                    vec![
                        uint(0),
                        size_of(stack_ty()),
                        uint(3),
                        uint(34),
                        int_typed(-1, t_i64()),
                        uint(0),
                    ],
                ),
            ),
            let_stmt(
                "data_raw",
                None,
                syscall(
                    int(9),
                    vec![
                        uint(0),
                        binary(BinOp::Mul, variable("cap"), size_of(t_named("T"))),
                        uint(3),
                        uint(34),
                        int_typed(-1, t_i64()),
                        uint(0),
                    ],
                ),
            ),
            let_stmt(
                "stack",
                None,
                cast(cast(variable("stack_raw"), t_u64()), stack_ptr_ty()),
            ),
            let_stmt(
                "data",
                None,
                cast(cast(variable("data_raw"), t_u64()), t_ptr(t_named("T"))),
            ),
            store(field_addr(variable("stack"), "data"), variable("data")),
            store(field_addr(variable("stack"), "len"), uint(0)),
            store(field_addr(variable("stack"), "cap"), variable("cap")),
            ret(variable("stack")),
        ],
    );

    let push = ast_fn(
        Visibility::Public,
        "stack_push",
        vec!["T"],
        vec![param("s", stack_ptr_ty()), param("val", t_named("T"))],
        t_void(),
        vec![
            let_stmt("len", None, load(field_addr(variable("s"), "len"))),
            let_stmt("data", None, load(field_addr(variable("s"), "data"))),
            store(
                index_addr(variable("data"), variable("len")),
                variable("val"),
            ),
            store(
                field_addr(variable("s"), "len"),
                binary(BinOp::Add, variable("len"), uint_typed(1, t_u64())),
            ),
            ret_void(),
        ],
    );

    let pop = ast_fn(
        Visibility::Public,
        "stack_pop",
        vec!["T"],
        vec![param("s", stack_ptr_ty())],
        t_named("T"),
        vec![
            let_stmt("len", None, load(field_addr(variable("s"), "len"))),
            if_(
                binary(BinOp::Eq, variable("len"), uint_typed(0, t_u64())),
                vec![trap()],
                Vec::new(),
            ),
            let_stmt(
                "new_len",
                None,
                binary(BinOp::Sub, variable("len"), uint_typed(1, t_u64())),
            ),
            store(field_addr(variable("s"), "len"), variable("new_len")),
            let_stmt("data", None, load(field_addr(variable("s"), "data"))),
            ret(load(index_addr(variable("data"), variable("new_len")))),
        ],
    );

    let free = ast_fn(
        Visibility::Public,
        "stack_free",
        vec!["T"],
        vec![param("s", stack_ptr_ty())],
        t_void(),
        vec![
            let_stmt("data", None, load(field_addr(variable("s"), "data"))),
            let_stmt("cap", None, load(field_addr(variable("s"), "cap"))),
            expr(syscall(
                int(11),
                vec![
                    variable("data"),
                    binary(BinOp::Mul, variable("cap"), size_of(t_named("T"))),
                ],
            )),
            expr(syscall(int(11), vec![variable("s"), size_of(stack_ty())])),
            ret_void(),
        ],
    );

    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt("is", None, call("stack_init", vec![t_i64()], vec![int(64)])),
            let_stmt("i", None, int(0)),
            let_stmt("sum", None, int(0)),
            loop_(vec![
                if_(
                    binary(BinOp::Ge, variable("i"), int(20)),
                    vec![break_stmt()],
                    Vec::new(),
                ),
                expr(call(
                    "stack_push",
                    Vec::new(),
                    vec![variable("is"), binary(BinOp::Add, variable("i"), int(1))],
                )),
                assign("i", binary(BinOp::Add, variable("i"), int(1))),
            ]),
            assign("i", int(0)),
            loop_(vec![
                if_(
                    binary(BinOp::Ge, variable("i"), int(20)),
                    vec![break_stmt()],
                    Vec::new(),
                ),
                assign(
                    "sum",
                    binary(
                        BinOp::Add,
                        variable("sum"),
                        call("stack_pop", Vec::new(), vec![variable("is")]),
                    ),
                ),
                assign("i", binary(BinOp::Add, variable("i"), int(1))),
            ]),
            if_(
                binary(BinOp::Ne, variable("sum"), int(210)),
                vec![trap()],
                Vec::new(),
            ),
            expr(call("stack_free", Vec::new(), vec![variable("is")])),
            let_stmt(
                "ps",
                None,
                call("stack_init", vec![t_named("Point")], vec![int(8)]),
            ),
            let_stmt("p0", None, load(global_addr("point0"))),
            let_stmt("p1", None, load(global_addr("point1"))),
            let_stmt("p2", None, load(global_addr("point2"))),
            expr(call(
                "stack_push",
                Vec::new(),
                vec![variable("ps"), variable("p0")],
            )),
            expr(call(
                "stack_push",
                Vec::new(),
                vec![variable("ps"), variable("p1")],
            )),
            expr(call(
                "stack_push",
                Vec::new(),
                vec![variable("ps"), variable("p2")],
            )),
            let_stmt(
                "q2",
                None,
                call("stack_pop", Vec::new(), vec![variable("ps")]),
            ),
            let_stmt(
                "q1",
                None,
                call("stack_pop", Vec::new(), vec![variable("ps")]),
            ),
            let_stmt(
                "q0",
                None,
                call("stack_pop", Vec::new(), vec![variable("ps")]),
            ),
            if_(
                binary(BinOp::Ne, load(field_addr(addr_var("q2"), "x")), int(100)),
                vec![trap()],
                Vec::new(),
            ),
            if_(
                binary(BinOp::Ne, load(field_addr(addr_var("q2"), "y")), int(200)),
                vec![trap()],
                Vec::new(),
            ),
            if_(
                binary(
                    BinOp::Ne,
                    binary(
                        BinOp::Add,
                        load(field_addr(addr_var("q1"), "x")),
                        load(field_addr(addr_var("q1"), "y")),
                    ),
                    int(30),
                ),
                vec![trap()],
                Vec::new(),
            ),
            if_(
                binary(
                    BinOp::Ne,
                    binary(
                        BinOp::Add,
                        load(field_addr(addr_var("q0"), "x")),
                        load(field_addr(addr_var("q0"), "y")),
                    ),
                    int(3),
                ),
                vec![trap()],
                Vec::new(),
            ),
            expr(call("stack_free", Vec::new(), vec![variable("ps")])),
            ret(int(0)),
        ],
    );

    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![point, stack],
            vec![],
            vec![point0, point1, point2],
            vec![main, init, push, pop, free],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 0, b"");
}

#[test]
fn primitive_types_and_array_layouts() {
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt("a", None, int_typed(127, t_i8())),
            let_stmt("b", None, int_typed(-32768, t_i16())),
            let_stmt("c", None, uint_typed(65535, t_u16())),
            let_stmt("d", None, uint_typed(4_294_967_295, t_u32())),
            let_stmt("e", None, float_typed(1.5, t_f32())),
            if_(
                binary(
                    BinOp::Ne,
                    size_of(t_array(t_i8(), 4)),
                    uint_typed(4, t_u64()),
                ),
                vec![trap()],
                Vec::new(),
            ),
            if_(
                binary(
                    BinOp::Ne,
                    align_of(t_array(t_i8(), 4)),
                    uint_typed(1, t_u64()),
                ),
                vec![trap()],
                Vec::new(),
            ),
            if_(
                binary(BinOp::Ne, size_of(t_i16()), uint_typed(2, t_u64())),
                vec![trap()],
                Vec::new(),
            ),
            if_(
                binary(BinOp::Ne, size_of(t_u32()), uint_typed(4, t_u64())),
                vec![trap()],
                Vec::new(),
            ),
            if_(
                binary(
                    BinOp::Ne,
                    unary(UnOp::Neg, variable("a")),
                    int_typed(-127, t_i8()),
                ),
                vec![trap()],
                Vec::new(),
            ),
            if_(
                binary(BinOp::Ne, variable("e"), float_typed(1.5, t_f32())),
                vec![trap()],
                Vec::new(),
            ),
            expr(variable("b")),
            expr(variable("c")),
            expr(variable("d")),
            ret(int(0)),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 0, b"");
}

#[test]
fn bytes_rodata_and_null_pointer() {
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt("p", None, bytes(b"hi")),
            let_stmt("n", None, bytes_len(b"hi")),
            let_stmt("z", None, null(t_ptr(t_i64()))),
            if_(
                binary(BinOp::Ne, variable("n"), uint_typed(2, t_u64())),
                vec![trap()],
                Vec::new(),
            ),
            if_(
                binary(BinOp::Ne, load(variable("p")), uint_typed(104, t_u8())),
                vec![trap()],
                Vec::new(),
            ),
            if_(
                binary(BinOp::Ne, cast(variable("z"), t_u64()), uint(0)),
                vec![trap()],
                Vec::new(),
            ),
            ret(int(0)),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 0, b"");
}

#[test]
fn short_circuit_lowering_preserves_side_effects() {
    let bump = public_fn(
        "bump",
        vec![param("p", t_ptr(t_i64()))],
        t_bool(),
        vec![
            store(
                variable("p"),
                binary(BinOp::Add, load(variable("p")), int(1)),
            ),
            ret(boolean(true)),
        ],
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt("count", None, int(0)),
            let_stmt(
                "a",
                None,
                binary(
                    BinOp::LogAnd,
                    boolean(false),
                    call("bump", Vec::new(), vec![addr_var("count")]),
                ),
            ),
            if_(variable("a"), vec![trap()], Vec::new()),
            if_(
                binary(BinOp::Ne, variable("count"), int(0)),
                vec![trap()],
                Vec::new(),
            ),
            let_stmt(
                "b",
                None,
                binary(
                    BinOp::LogOr,
                    boolean(true),
                    call("bump", Vec::new(), vec![addr_var("count")]),
                ),
            ),
            if_(variable("b"), Vec::new(), vec![trap()]),
            if_(
                binary(BinOp::Ne, variable("count"), int(0)),
                vec![trap()],
                Vec::new(),
            ),
            let_stmt(
                "c",
                None,
                binary(
                    BinOp::LogAnd,
                    boolean(true),
                    call("bump", Vec::new(), vec![addr_var("count")]),
                ),
            ),
            if_(variable("c"), Vec::new(), vec![trap()]),
            if_(
                binary(BinOp::Ne, variable("count"), int(1)),
                vec![trap()],
                Vec::new(),
            ),
            ret(int(0)),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main, bump],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 0, b"");
}

#[test]
fn generic_union_instantiations() {
    let maybe = ast_union(
        Visibility::Public,
        "Maybe",
        vec!["T"],
        vec![
            ("value".to_string(), t_named("T")),
            ("bits".to_string(), t_u64()),
        ],
    );
    let zero_i = ast_global(
        Visibility::Private,
        "maybe_i_zero",
        t_named_args("Maybe", vec![t_i64()]),
        Some(vec![0; 8]),
    );
    let zero_f = ast_global(
        Visibility::Private,
        "maybe_f_zero",
        t_named_args("Maybe", vec![t_f64()]),
        Some(vec![0; 8]),
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt(
                "a",
                Some(t_named_args("Maybe", vec![t_i64()])),
                load(global_addr("maybe_i_zero")),
            ),
            let_stmt(
                "b",
                Some(t_named_args("Maybe", vec![t_f64()])),
                load(global_addr("maybe_f_zero")),
            ),
            store(field_addr(addr_var("a"), "value"), int(-1)),
            if_(
                binary(
                    BinOp::Ne,
                    load(field_addr(addr_var("a"), "bits")),
                    uint_typed(u64::MAX, t_u64()),
                ),
                vec![trap()],
                Vec::new(),
            ),
            store(field_addr(addr_var("b"), "value"), float(1.5)),
            if_(
                binary(
                    BinOp::Ne,
                    load(field_addr(addr_var("b"), "bits")),
                    uint_typed(0x3ff8_0000_0000_0000, t_u64()),
                ),
                vec![trap()],
                Vec::new(),
            ),
            ret(int(0)),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![maybe],
            vec![zero_i, zero_f],
            vec![main],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 0, b"");
}

#[test]
fn high_level_recursion_factorial() {
    let factorial = public_fn(
        "factorial",
        vec![param("n", t_i64())],
        t_i64(),
        vec![if_(
            binary(BinOp::Le, variable("n"), int(1)),
            vec![ret(int(1))],
            vec![ret(binary(
                BinOp::Mul,
                variable("n"),
                call(
                    "factorial",
                    Vec::new(),
                    vec![binary(BinOp::Sub, variable("n"), int(1))],
                ),
            ))],
        )],
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt("value", None, call("factorial", Vec::new(), vec![int(10)])),
            if_(
                binary(BinOp::Ne, variable("value"), int(3_628_800)),
                vec![trap()],
                Vec::new(),
            ),
            ret(int(0)),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main, factorial],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 0, b"");
}

#[test]
fn cross_module_type_directed_overload() {
    let add_i = public_fn(
        "add",
        vec![param("a", t_i64()), param("b", t_i64())],
        t_i64(),
        vec![ret(binary(BinOp::Add, variable("a"), variable("b")))],
    );
    let add_f = public_fn(
        "add",
        vec![param("a", t_f64()), param("b", t_f64())],
        t_f64(),
        vec![ret(binary(BinOp::Add, variable("a"), variable("b")))],
    );
    let a = ast_module("a", vec![], vec![], vec![], vec![], vec![add_i]);
    let b = ast_module("b", vec![], vec![], vec![], vec![], vec![add_f]);
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![ret(binary(
            BinOp::Add,
            call("add", Vec::new(), vec![int(2), int(3)]),
            cast(
                call("add", Vec::new(), vec![float(1.5), float(2.5)]),
                t_i64(),
            ),
        ))],
    );
    let main_module = ast_module(
        "main",
        vec![import("a", None), import("b", None)],
        vec![],
        vec![],
        vec![],
        vec![main],
    );
    let p = program(vec![a, b, main_module], "main", "main");
    assert_program_parity(&p, &["prog"], &[], 9, b"");
}

#[test]
fn module_alias_type_and_global() {
    let point = ast_struct(
        Visibility::Public,
        "Point",
        Vec::new(),
        vec![("x".to_string(), t_i64()), ("y".to_string(), t_i64())],
    );
    let origin = ast_global(
        Visibility::Public,
        "origin",
        t_named("Point"),
        Some(vec![0; 16]),
    );
    let data = ast_module("data", vec![], vec![point], vec![], vec![origin], vec![]);
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt(
                "p",
                Some(AstType::Named {
                    module: Some("d".to_string()),
                    name: "Point".to_string(),
                    type_args: Vec::new(),
                }),
                load(AstExpr::GlobalAddr {
                    module: Some("d".to_string()),
                    name: "origin".to_string(),
                }),
            ),
            store(field_addr(addr_var("p"), "x"), int(20)),
            store(field_addr(addr_var("p"), "y"), int(22)),
            ret(binary(
                BinOp::Add,
                load(field_addr(addr_var("p"), "x")),
                load(field_addr(addr_var("p"), "y")),
            )),
        ],
    );
    let main_module = ast_module(
        "main",
        vec![import("data", Some("d"))],
        vec![],
        vec![],
        vec![],
        vec![main],
    );
    let p = program(vec![data, main_module], "main", "main");
    assert_program_parity(&p, &["prog"], &[], 42, b"");
}

#[test]
fn syscall_write_bytes_literal() {
    let message = b"hello tc_r1 from typeck\n";
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            expr(syscall(
                int(1),
                vec![int(1), bytes(message), bytes_len(message)],
            )),
            ret(int(0)),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 0, message);
}

#[test]
fn syscall_read_stdin_echo_high_level() {
    let buffer_ty = t_array(t_u8(), 32);
    let buffer_zero = ast_global(
        Visibility::Private,
        "buffer_zero",
        buffer_ty.clone(),
        Some(vec![0; 32]),
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt("buf", Some(buffer_ty), load(global_addr("buffer_zero"))),
            let_stmt(
                "n",
                None,
                syscall(
                    int(0),
                    vec![int(0), addr_var("buf"), uint_typed(32, t_u64())],
                ),
            ),
            expr(syscall(
                int(1),
                vec![int(1), addr_var("buf"), cast(variable("n"), t_u64())],
            )),
            ret(int(0)),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![buffer_zero],
            vec![main],
        )],
        "main",
        "main",
    );
    assert_program_parity(
        &p,
        &["prog"],
        b"echo through typeck\n",
        0,
        b"echo through typeck\n",
    );
}

#[test]
fn deterministic_monomorphized_names_and_rodata() {
    let pair = ast_struct(
        Visibility::Public,
        "Pair",
        vec!["A", "B"],
        vec![
            ("first".to_string(), t_named("A")),
            ("second".to_string(), t_named("B")),
        ],
    );
    let identity = ast_fn(
        Visibility::Public,
        "identity",
        vec!["T"],
        vec![param("x", t_named("T"))],
        t_named("T"),
        vec![ret(variable("x"))],
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            expr(size_of(t_named_args("Pair", vec![t_i64(), t_f64()]))),
            let_stmt("a", None, bytes(b"same")),
            let_stmt("b", None, bytes_len(b"same")),
            expr(variable("a")),
            expr(variable("b")),
            ret(call("identity", vec![t_i64()], vec![int(42)])),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![pair],
            vec![],
            vec![],
            vec![main, identity],
        )],
        "main",
        "main",
    );
    let first = check_and_lower(&p).expect("first lowering succeeds");
    let second = check_and_lower(&p).expect("second lowering succeeds");
    assert_eq!(
        first
            .functions
            .iter()
            .map(|function| function.name.as_str())
            .collect::<Vec<_>>(),
        second
            .functions
            .iter()
            .map(|function| function.name.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        first
            .functions
            .iter()
            .any(|function| function.name == "main__identity__i64")
    );
    assert!(
        first
            .structs
            .iter()
            .any(|def| def.name == "main__Pair__i64__f64")
    );
    assert_eq!(first.rodata.len(), 1, "identical byte literals deduplicate");
}

#[test]
fn generic_call_inside_generic() {
    let identity = ast_fn(
        Visibility::Public,
        "identity",
        vec!["T"],
        vec![param("x", t_named("T"))],
        t_named("T"),
        vec![ret(variable("x"))],
    );
    let outer = ast_fn(
        Visibility::Public,
        "outer",
        vec!["T"],
        vec![param("x", t_named("T"))],
        t_named("T"),
        vec![ret(call(
            "identity",
            vec![t_named("T")],
            vec![variable("x")],
        ))],
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![ret(call("outer", vec![t_i64()], vec![int(42)]))],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main, identity, outer],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 42, b"");
}

#[test]
fn err_infinite_generic_struct() {
    let boom = ast_struct(
        Visibility::Public,
        "Boom",
        vec!["T"],
        vec![(
            "next".to_string(),
            t_named_args("Boom", vec![t_ptr(t_named("T"))]),
        )],
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            expr(size_of(t_named_args("Boom", vec![t_i64()]))),
            ret(int(0)),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![boom],
            vec![],
            vec![],
            vec![main],
        )],
        "main",
        "main",
    );
    expect_error(&p, TypeErrorKind::InfiniteGenericInstantiation);
}

#[test]
fn array_indexing_and_pointer_decay() {
    let array_ty = t_array(t_i64(), 4);
    let template = ast_global(
        Visibility::Private,
        "arr_zero",
        array_ty.clone(),
        Some(vec![0; 32]),
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt("arr", Some(array_ty), load(global_addr("arr_zero"))),
            let_stmt("i", None, int(0)),
            let_stmt("sum", None, int(0)),
            loop_(vec![
                if_(
                    binary(BinOp::Ge, variable("i"), int(4)),
                    vec![break_stmt()],
                    Vec::new(),
                ),
                store(
                    index_addr(addr_var("arr"), variable("i")),
                    binary(BinOp::Add, variable("i"), int(1)),
                ),
                assign("i", binary(BinOp::Add, variable("i"), int(1))),
            ]),
            assign("i", int(0)),
            loop_(vec![
                if_(
                    binary(BinOp::Ge, variable("i"), int(4)),
                    vec![break_stmt()],
                    Vec::new(),
                ),
                assign(
                    "sum",
                    binary(
                        BinOp::Add,
                        variable("sum"),
                        load(index_addr(addr_var("arr"), variable("i"))),
                    ),
                ),
                assign("i", binary(BinOp::Add, variable("i"), int(1))),
            ]),
            ret(variable("sum")),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![template],
            vec![main],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 10, b"");
}

#[test]
fn entry_argc_argv_high_level() {
    let main = public_fn(
        "main",
        vec![
            param("argc", t_i64()),
            param("argv", t_ptr(t_ptr(t_u8()))),
            param("envp", t_ptr(t_ptr(t_u8()))),
        ],
        t_i64(),
        vec![ret(variable("argc"))],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog", "a", "b"], &[], 3, b"");
}

#[test]
fn aggregate_function_pointer_abi() {
    let point = ast_struct(
        Visibility::Public,
        "Point",
        Vec::new(),
        vec![("x".to_string(), t_i64()), ("y".to_string(), t_i64())],
    );
    let point_zero = ast_global(
        Visibility::Private,
        "point_zero",
        t_named("Point"),
        Some(vec![0; 16]),
    );
    let pass = public_fn(
        "pass_point",
        vec![param("p", t_named("Point"))],
        t_named("Point"),
        vec![ret(variable("p"))],
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt(
                "fp",
                Some(t_fn_ptr(vec![t_named("Point")], t_named("Point"))),
                fn_addr("pass_point", Vec::new()),
            ),
            let_stmt("p", Some(t_named("Point")), load(global_addr("point_zero"))),
            store(field_addr(addr_var("p"), "x"), int(21)),
            store(field_addr(addr_var("p"), "y"), int(21)),
            let_stmt(
                "q",
                None,
                indirect_call(variable("fp"), vec![variable("p")]),
            ),
            ret(binary(
                BinOp::Add,
                load(field_addr(addr_var("q"), "x")),
                load(field_addr(addr_var("q"), "y")),
            )),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![point],
            vec![],
            vec![point_zero],
            vec![main, pass],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 42, b"");
}

#[test]
fn err_private_struct_cross_module() {
    let secret = ast_struct(
        Visibility::Private,
        "Secret",
        Vec::new(),
        vec![("x".to_string(), t_i64())],
    );
    let a = ast_module("a", vec![], vec![secret], vec![], vec![], vec![]);
    let main = public_fn(
        "main",
        Vec::new(),
        t_module_named("a", "Secret"),
        Vec::new(),
    );
    let main_module = ast_module(
        "main",
        vec![import("a", None)],
        vec![],
        vec![],
        vec![],
        vec![main],
    );
    let p = program(vec![a, main_module], "main", "main");
    expect_error(&p, TypeErrorKind::PrivateAccess);
}

#[test]
fn err_private_global_cross_module() {
    let g = ast_global(
        Visibility::Private,
        "g",
        t_i64(),
        Some(1i64.to_le_bytes().to_vec()),
    );
    let a = ast_module("a", vec![], vec![], vec![], vec![g], vec![]);
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![ret(load(AstExpr::GlobalAddr {
            module: Some("a".to_string()),
            name: "g".to_string(),
        }))],
    );
    let main_module = ast_module(
        "main",
        vec![import("a", None)],
        vec![],
        vec![],
        vec![],
        vec![main],
    );
    let p = program(vec![a, main_module], "main", "main");
    expect_error(&p, TypeErrorKind::PrivateAccess);
}

#[test]
fn err_duplicate_module_name() {
    let main1 = public_fn("main", Vec::new(), t_i64(), vec![ret(int(0))]);
    let p = program(
        vec![
            ast_module("main", vec![], vec![], vec![], vec![], vec![main1]),
            ast_module("main", vec![], vec![], vec![], vec![], vec![]),
        ],
        "main",
        "main",
    );
    expect_error(&p, TypeErrorKind::DuplicateModule);
}

#[test]
fn err_missing_entry_function() {
    let helper = public_fn("helper", Vec::new(), t_i64(), vec![ret(int(0))]);
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![helper],
        )],
        "main",
        "entry",
    );
    expect_error(&p, TypeErrorKind::MissingEntryFunction);
}

#[test]
fn err_generic_entry_function() {
    let main = ast_fn(
        Visibility::Public,
        "main",
        vec!["T"],
        Vec::new(),
        t_i64(),
        vec![ret(int(0))],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main],
        )],
        "main",
        "main",
    );
    expect_error(&p, TypeErrorKind::InvalidEntrySignature);
}

#[test]
fn literal_argument_context_inference() {
    let take_u8 = public_fn(
        "take_u8",
        vec![param("x", t_u8())],
        t_i64(),
        vec![ret(cast(variable("x"), t_i64()))],
    );
    let take_u64 = public_fn(
        "take_u64",
        vec![param("x", t_u64())],
        t_i64(),
        vec![ret(cast(variable("x"), t_i64()))],
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![ret(binary(
            BinOp::Add,
            call("take_u8", Vec::new(), vec![int(2)]),
            call("take_u64", Vec::new(), vec![int(40)]),
        ))],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main, take_u8, take_u64],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 42, b"");
}

#[test]
fn loop_continue_and_break() {
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt("i", None, int(0)),
            let_stmt("sum", None, int(0)),
            loop_(vec![
                assign("i", binary(BinOp::Add, variable("i"), int(1))),
                if_(
                    binary(BinOp::Gt, variable("i"), int(10)),
                    vec![break_stmt()],
                    Vec::new(),
                ),
                if_(
                    binary(BinOp::Eq, variable("i"), int(5)),
                    vec![continue_stmt()],
                    Vec::new(),
                ),
                assign("sum", binary(BinOp::Add, variable("sum"), variable("i"))),
            ]),
            ret(variable("sum")),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 50, b"");
}

#[test]
fn err_undefined_variable() {
    let main = public_fn("main", Vec::new(), t_i64(), vec![ret(variable("missing"))]);
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main],
        )],
        "main",
        "main",
    );
    expect_error(&p, TypeErrorKind::UndefinedVariable);
}

#[test]
fn err_assign_type_mismatch() {
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt("x", None, int(0)),
            assign("x", boolean(true)),
            ret(variable("x")),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main],
        )],
        "main",
        "main",
    );
    expect_error(&p, TypeErrorKind::TypeMismatch);
}

#[test]
fn err_return_type_mismatch() {
    let main = public_fn("main", Vec::new(), t_i64(), vec![ret(boolean(true))]);
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main],
        )],
        "main",
        "main",
    );
    expect_error(&p, TypeErrorKind::TypeMismatch);
}

fn circular_module(name: &str, target: &str, with_main: bool) -> AstModule {
    let functions = if with_main {
        vec![public_fn("main", Vec::new(), t_i64(), vec![ret(int(0))])]
    } else {
        Vec::new()
    };
    ast_module(
        name,
        vec![import(target, None)],
        vec![],
        vec![],
        vec![],
        functions,
    )
}

#[test]
fn err_circular_import_2_cycle() {
    let p = program(
        vec![
            circular_module("a", "b", true),
            circular_module("b", "a", false),
        ],
        "a",
        "main",
    );
    expect_error(&p, TypeErrorKind::CircularImport);
}

#[test]
fn err_circular_import_3_cycle() {
    let p = program(
        vec![
            circular_module("a", "b", true),
            circular_module("b", "c", false),
            circular_module("c", "a", false),
        ],
        "a",
        "main",
    );
    expect_error(&p, TypeErrorKind::CircularImport);
}

#[test]
fn err_private_fn_cross_module() {
    let secret = ast_fn(
        Visibility::Private,
        "secret",
        Vec::new(),
        Vec::new(),
        t_i64(),
        vec![ret(int(1))],
    );
    let a = ast_module("a", vec![], vec![], vec![], vec![], vec![secret]);
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![ret(module_call("a", "secret", Vec::new(), Vec::new()))],
    );
    let main_module = ast_module(
        "main",
        vec![import("a", None)],
        vec![],
        vec![],
        vec![],
        vec![main],
    );
    let p = program(vec![a, main_module], "main", "main");
    expect_error(&p, TypeErrorKind::PrivateAccess);
}

#[test]
fn err_ambiguous_overload() {
    let generic = ast_fn(
        Visibility::Public,
        "over",
        vec!["T"],
        vec![param("x", t_named("T"))],
        t_i64(),
        vec![ret(int(1))],
    );
    let concrete = public_fn(
        "over",
        vec![param("x", t_i64())],
        t_i64(),
        vec![ret(int(2))],
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![ret(call("over", Vec::new(), vec![int(1)]))],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main, generic, concrete],
        )],
        "main",
        "main",
    );
    expect_error(&p, TypeErrorKind::AmbiguousCall);
}

#[test]
fn err_wrong_generic_arity() {
    let identity = ast_fn(
        Visibility::Public,
        "identity",
        vec!["T"],
        vec![param("x", t_named("T"))],
        t_named("T"),
        vec![ret(variable("x"))],
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![ret(call("identity", vec![t_i64(), t_f64()], vec![int(1)]))],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main, identity],
        )],
        "main",
        "main",
    );
    expect_error(&p, TypeErrorKind::WrongGenericArity);
}

#[test]
fn err_unconstrained_generic_type_param() {
    let make = ast_fn(
        Visibility::Public,
        "make",
        vec!["T"],
        Vec::new(),
        t_named("T"),
        Vec::new(),
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![ret(call("make", Vec::new(), Vec::new()))],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main, make],
        )],
        "main",
        "main",
    );
    expect_error(&p, TypeErrorKind::UnconstrainedGeneric);
}

#[test]
fn err_infinite_generic_instantiation() {
    let boom = ast_fn(
        Visibility::Public,
        "boom",
        vec!["T"],
        Vec::new(),
        t_void(),
        vec![
            expr(call("boom", vec![t_ptr(t_named("T"))], Vec::new())),
            ret_void(),
        ],
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![expr(call("boom", vec![t_i64()], Vec::new())), ret(int(0))],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main, boom],
        )],
        "main",
        "main",
    );
    expect_error(&p, TypeErrorKind::InfiniteGenericInstantiation);
}

#[test]
fn err_invalid_phrase_pattern_unknown_param() {
    let mut f = public_fn("f", vec![param("x", t_i64())], t_void(), vec![ret_void()]);
    f.phrases.push(phrase(vec![phrase_param("missing")]));
    let main = public_fn("main", Vec::new(), t_i64(), vec![ret(int(0))]);
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main, f],
        )],
        "main",
        "main",
    );
    expect_phrase_error(&p, TypeErrorKind::InvalidPhraseParam);
}

#[test]
fn err_duplicate_phrase_param() {
    let mut f = public_fn("f", vec![param("x", t_i64())], t_void(), vec![ret_void()]);
    f.phrases
        .push(phrase(vec![phrase_param("x"), phrase_param("x")]));
    let main = public_fn("main", Vec::new(), t_i64(), vec![ret(int(0))]);
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main, f],
        )],
        "main",
        "main",
    );
    expect_phrase_error(&p, TypeErrorKind::DuplicatePhraseParam);
}

#[test]
fn err_missing_imported_module() {
    let main = public_fn("main", Vec::new(), t_i64(), vec![ret(int(0))]);
    let p = program(
        vec![ast_module(
            "main",
            vec![import("missing", None)],
            vec![],
            vec![],
            vec![],
            vec![main],
        )],
        "main",
        "main",
    );
    expect_error(&p, TypeErrorKind::MissingModule);
}

#[test]
fn aggregate_parameter_and_return_abi() {
    let point = ast_struct(
        Visibility::Public,
        "Point",
        Vec::new(),
        vec![("x".to_string(), t_i64()), ("y".to_string(), t_i64())],
    );
    let point_zero = ast_global(
        Visibility::Private,
        "point_zero",
        t_named("Point"),
        Some(vec![0; 16]),
    );
    let pass = public_fn(
        "pass_point",
        vec![param("p", t_named("Point"))],
        t_named("Point"),
        vec![ret(variable("p"))],
    );
    let sum = public_fn(
        "sum_point",
        vec![param("p", t_named("Point"))],
        t_i64(),
        vec![ret(binary(
            BinOp::Add,
            load(field_addr(addr_var("p"), "x")),
            load(field_addr(addr_var("p"), "y")),
        ))],
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt("p", Some(t_named("Point")), load(global_addr("point_zero"))),
            store(field_addr(addr_var("p"), "x"), int(20)),
            store(field_addr(addr_var("p"), "y"), int(22)),
            let_stmt(
                "q",
                None,
                call("pass_point", Vec::new(), vec![variable("p")]),
            ),
            ret(call("sum_point", Vec::new(), vec![variable("q")])),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![point],
            vec![],
            vec![point_zero],
            vec![main, pass, sum],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 42, b"");
}

#[test]
fn generic_option_struct() {
    let option = ast_struct(
        Visibility::Public,
        "Option",
        vec!["T"],
        vec![
            ("has_val".to_string(), t_bool()),
            ("val".to_string(), t_named("T")),
        ],
    );
    let unwrap_or = ast_fn(
        Visibility::Public,
        "unwrap_or",
        vec!["T"],
        vec![
            param("opt", t_ptr(t_named_args("Option", vec![t_named("T")]))),
            param("fallback", t_named("T")),
        ],
        t_named("T"),
        vec![if_(
            load(field_addr(variable("opt"), "has_val")),
            vec![ret(load(field_addr(variable("opt"), "val")))],
            vec![ret(variable("fallback"))],
        )],
    );
    let template = ast_global(
        Visibility::Private,
        "opt_zero",
        t_named_args("Option", vec![t_i64()]),
        Some(vec![0; 16]),
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt(
                "a",
                Some(t_named_args("Option", vec![t_i64()])),
                load(global_addr("opt_zero")),
            ),
            let_stmt(
                "b",
                Some(t_named_args("Option", vec![t_i64()])),
                load(global_addr("opt_zero")),
            ),
            store(field_addr(addr_var("a"), "has_val"), boolean(true)),
            store(field_addr(addr_var("a"), "val"), int(42)),
            store(field_addr(addr_var("b"), "has_val"), boolean(false)),
            store(field_addr(addr_var("b"), "val"), int(99)),
            ret(binary(
                BinOp::Add,
                call("unwrap_or", Vec::new(), vec![addr_var("a"), int(0)]),
                call("unwrap_or", Vec::new(), vec![addr_var("b"), int(7)]),
            )),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![option],
            vec![],
            vec![template],
            vec![main, unwrap_or],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 49, b"");
}

#[test]
fn generic_fn_pointer_instantiation() {
    let identity = ast_fn(
        Visibility::Public,
        "identity",
        vec!["T"],
        vec![param("x", t_named("T"))],
        t_named("T"),
        vec![ret(variable("x"))],
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt(
                "fp",
                Some(t_fn_ptr(vec![t_i64()], t_i64())),
                fn_addr("identity", vec![t_i64()]),
            ),
            ret(indirect_call(variable("fp"), vec![int(42)])),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main, identity],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 42, b"");
}

#[test]
fn type_directed_overload_resolution() {
    let point = ast_struct(
        Visibility::Public,
        "Point",
        Vec::new(),
        vec![("x".to_string(), t_i64()), ("y".to_string(), t_i64())],
    );
    let point_zero = ast_global(
        Visibility::Private,
        "point_zero",
        t_named("Point"),
        Some(vec![0; 16]),
    );
    let combine_i = public_fn(
        "combine",
        vec![param("a", t_i64()), param("b", t_i64())],
        t_i64(),
        vec![ret(binary(BinOp::Add, variable("a"), variable("b")))],
    );
    let combine_f = public_fn(
        "combine",
        vec![param("a", t_f64()), param("b", t_f64())],
        t_f64(),
        vec![ret(binary(BinOp::Add, variable("a"), variable("b")))],
    );
    let combine_p = public_fn(
        "combine",
        vec![param("p", t_ptr(t_named("Point"))), param("n", t_i64())],
        t_i64(),
        vec![ret(binary(
            BinOp::Add,
            load(field_addr(variable("p"), "x")),
            variable("n"),
        ))],
    );
    let main = public_fn(
        "main",
        Vec::new(),
        t_i64(),
        vec![
            let_stmt("p", Some(t_named("Point")), load(global_addr("point_zero"))),
            store(field_addr(addr_var("p"), "x"), int(10)),
            let_stmt("a", None, call("combine", Vec::new(), vec![int(2), int(3)])),
            let_stmt(
                "b",
                None,
                call("combine", Vec::new(), vec![float(1.5), float(2.5)]),
            ),
            let_stmt(
                "c",
                None,
                call("combine", Vec::new(), vec![addr_var("p"), int(4)]),
            ),
            ret(binary(
                BinOp::Add,
                binary(BinOp::Add, variable("a"), cast(variable("b"), t_i64())),
                variable("c"),
            )),
        ],
    );
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![point],
            vec![],
            vec![point_zero],
            vec![main, combine_i, combine_f, combine_p],
        )],
        "main",
        "main",
    );
    assert_program_parity(&p, &["prog"], &[], 23, b"");
}

#[test]
fn phrase_pattern_collection_and_validation() {
    let mut push = public_fn(
        "push",
        vec![param("val", t_i64()), param("stack", t_ptr(t_i64()))],
        t_void(),
        vec![ret_void()],
    );
    push.phrases.push(phrase(vec![
        word("push"),
        phrase_param("val"),
        word("to"),
        phrase_param("stack"),
    ]));
    let main = public_fn("main", Vec::new(), t_i64(), vec![ret(int(0))]);
    let p = program(
        vec![ast_module(
            "main",
            vec![],
            vec![],
            vec![],
            vec![],
            vec![main, push],
        )],
        "main",
        "main",
    );
    let entries = collect_phrases(&p).expect("phrases validate");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].module, "main");
    assert_eq!(entries[0].function, "push");
    assert_eq!(
        entries[0].pattern,
        phrase(vec![
            word("push"),
            phrase_param("val"),
            word("to"),
            phrase_param("stack"),
        ])
    );
}
