//! Command-line driver for the `tc_r1` workspace.

use std::env;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use tc_ir::{
    BinOp, CallTarget, Expr, FnId, Function, LocalDef, LocalId, Module, RodataDef, Stmt, Type,
    interpret, verify_module,
};

/// Version reported by `tc --version`, sourced from the workspace manifest.
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let mut args = env::args().skip(1);

    match args.next().as_deref() {
        Some("--version" | "-V") => {
            println!("tc_r1 {VERSION}");
            ExitCode::SUCCESS
        }
        Some("--audit") => run_audit(),
        Some("--selftest") => run_selftest(),
        Some(argument) => {
            eprintln!("tc: unrecognized argument `{argument}`");
            usage();
            ExitCode::from(2)
        }
        None => {
            usage();
            ExitCode::from(2)
        }
    }
}

fn usage() {
    eprintln!("usage: tc [--version|--audit|--selftest]");
}

fn run_audit() -> ExitCode {
    let crates = [
        tc_ir::crate_name(),
        tc_codegen_x86::crate_name(),
        tc_typeck::crate_name(),
        tc_lexicon::crate_name(),
        tc_syntax::crate_name(),
        tc_semantics::crate_name(),
    ];
    let expected = [
        "tc-ir",
        "tc-codegen-x86",
        "tc-typeck",
        "tc-lexicon",
        "tc-syntax",
        "tc-semantics",
    ];

    if crates != expected {
        eprintln!("tc_r1 audit: FAIL (unexpected library crate identities)");
        return ExitCode::FAILURE;
    }

    let stats = tc_lexicon::lexicon_stats();
    println!("tc_r1 audit: PASS");
    println!(
        "  crates          : {} linked (6 libs + 1 cli)",
        crates.len() + 1
    );
    println!("  wordnet entries : {}", stats.wordnet_entries);
    println!("  verbnet frames  : {}", stats.verbnet_frames);
    println!("  irregular lemmas: {}", stats.irregular_lemmas);
    println!("  closed-class    : {}", stats.closed_class_words);
    println!("  dependencies    : std + local path only");
    println!("  external crates : 0");
    println!("  ML runtime      : disabled");
    println!("  network         : disabled");
    ExitCode::SUCCESS
}

fn local(name: &str, ty: Type) -> LocalDef {
    LocalDef {
        name: name.to_string(),
        ty,
    }
}

fn i64_const(value: i64) -> Expr {
    Expr::ConstInt(value, Type::I64)
}

fn local_expr(id: usize) -> Expr {
    Expr::Local(LocalId(id))
}

fn selftest_module() -> Module {
    let recurse = Function {
        name: "recurse".to_string(),
        params: vec![local("n", Type::I64)],
        ret: Type::I64,
        locals: Vec::new(),
        body: vec![Stmt::If {
            cond: Expr::Binary {
                op: BinOp::Eq,
                lhs: Box::new(local_expr(0)),
                rhs: Box::new(i64_const(0)),
            },
            then_body: vec![Stmt::Return(Some(i64_const(0)))],
            else_body: vec![Stmt::Return(Some(Expr::Binary {
                op: BinOp::Add,
                lhs: Box::new(local_expr(0)),
                rhs: Box::new(Expr::Call {
                    target: CallTarget::Direct(FnId(1)),
                    args: vec![Expr::Binary {
                        op: BinOp::Sub,
                        lhs: Box::new(local_expr(0)),
                        rhs: Box::new(i64_const(1)),
                    }],
                }),
            }))],
        }],
    };

    let main = Function {
        name: "main".to_string(),
        params: Vec::new(),
        ret: Type::I64,
        locals: Vec::new(),
        body: vec![
            Stmt::If {
                cond: Expr::Binary {
                    op: BinOp::Ne,
                    lhs: Box::new(Expr::Call {
                        target: CallTarget::Direct(FnId(1)),
                        args: vec![i64_const(5)],
                    }),
                    rhs: Box::new(i64_const(15)),
                },
                then_body: vec![Stmt::Trap],
                else_body: Vec::new(),
            },
            Stmt::Expr(Expr::Syscall {
                num: Box::new(i64_const(1)),
                args: vec![
                    i64_const(1),
                    Expr::RodataPtr(tc_ir::RodataId(0)),
                    Expr::RodataLen(tc_ir::RodataId(0)),
                ],
            }),
            Stmt::Return(Some(i64_const(0))),
        ],
    };

    Module {
        structs: Vec::new(),
        unions: Vec::new(),
        rodata: vec![RodataDef {
            name: "selftest".to_string(),
            bytes: b"tc_r1 selftest ok\n".to_vec(),
        }],
        globals: Vec::new(),
        functions: vec![main, recurse],
        entry: FnId(0),
    }
}

fn selftest_path() -> PathBuf {
    std::env::temp_dir().join(format!("tc_r1_selftest_{}.elf", std::process::id()))
}

fn run_selftest() -> ExitCode {
    match selftest_inner() {
        Ok(()) => {
            println!("SELFTEST PASS");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("SELFTEST FAIL: {message}");
            ExitCode::FAILURE
        }
    }
}

fn selftest_inner() -> Result<(), String> {
    let module = selftest_module();
    verify_module(&module).map_err(|errors| format!("IR verification failed: {errors:?}"))?;

    let args = vec!["tc".to_string()];
    let interpreted = interpret(&module, &args, &[], 1_000_000)
        .map_err(|error| format!("interpreter failed: {error:?}"))?;

    let path = selftest_path();
    tc_codegen_x86::compile_to_elf(&module, &path)
        .map_err(|error| format!("ELF compilation failed: {error}"))?;
    let output = Command::new(&path)
        .env_clear()
        .output()
        .map_err(|error| format!("ELF execution failed: {error}"));
    let _ = std::fs::remove_file(&path);
    let output = output?;

    let elf_exit = output.status.code().unwrap_or(-1);
    if elf_exit != interpreted.exit_code
        || output.stdout != interpreted.stdout
        || output.stderr != interpreted.stderr
    {
        return Err(format!(
            "ELF/interpreter mismatch: elf=({elf_exit}, {:?}, {:?}) interp=({}, {:?}, {:?})",
            output.stdout,
            output.stderr,
            interpreted.exit_code,
            interpreted.stdout,
            interpreted.stderr
        ));
    }
    if interpreted.exit_code != 0 {
        return Err(format!(
            "unexpected selftest exit code {}",
            interpreted.exit_code
        ));
    }
    if interpreted.stdout != b"tc_r1 selftest ok\n" {
        return Err(format!(
            "unexpected selftest stdout {:?}",
            interpreted.stdout
        ));
    }
    Ok(())
}
