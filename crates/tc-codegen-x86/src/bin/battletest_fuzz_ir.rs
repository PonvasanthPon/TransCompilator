//! Deterministic Core-7 IR/x86 differential fuzzer for Phase 3.5.
//!
//! Generates valid verified IR modules and requires exact byte-for-byte
//! equality between the static ELF64 binary and the reference interpreter.

use std::io::Write as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tc_codegen_x86::compile_to_elf;
use tc_ir::{
    BinOp, CallTarget, Expr, FnId, Function, LocalDef, LocalId, Module, RodataDef, Stmt, StructDef,
    StructId, Type, interpret, verify_module,
};

fn typed(value: i64, ty: &Type) -> Expr {
    if ty.is_unsigned_int() {
        Expr::ConstUint(value as u64, ty.clone())
    } else {
        Expr::ConstInt(value, ty.clone())
    }
}

fn local(id: usize) -> Expr {
    Expr::Local(LocalId(id))
}

fn binary(op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
    Expr::Binary {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
    }
}

fn cast(value: Expr, ty: Type) -> Expr {
    Expr::Cast {
        val: Box::new(value),
        to: ty,
    }
}

fn bitand_255(value: Expr) -> Expr {
    binary(BinOp::BitAnd, value, Expr::ConstInt(255, Type::I64))
}

fn main_function(body: Vec<Stmt>) -> Function {
    Function {
        name: "main".to_string(),
        params: Vec::new(),
        ret: Type::I64,
        locals: Vec::new(),
        body,
    }
}

fn module(seed: u64) -> Module {
    match seed % 7 {
        0 => scalar_module(seed),
        1 => float_module(seed),
        2 => control_module(seed),
        3 => helper_module(seed),
        4 => indirect_module(seed),
        5 => struct_module(seed),
        _ => syscall_module(seed),
    }
}

fn scalar_module(seed: u64) -> Module {
    let types = [
        Type::I8,
        Type::I16,
        Type::I32,
        Type::I64,
        Type::U8,
        Type::U16,
        Type::U32,
        Type::U64,
    ];
    let ty = types[usize::try_from(seed % 8).expect("small")].clone();
    let mut body = vec![Stmt::Assign {
        local: LocalId(0),
        val: typed(i64::try_from(seed % 61).expect("small"), &ty),
    }];
    let operations = [
        BinOp::Add,
        BinOp::Sub,
        BinOp::Mul,
        BinOp::BitAnd,
        BinOp::BitOr,
        BinOp::BitXor,
        BinOp::Shl,
        BinOp::Shr,
        BinOp::Div,
        BinOp::Mod,
    ];
    let steps = 4 + (seed % 7);
    for step in 0..steps {
        let op = operations[usize::try_from((seed + step) % 10).expect("small")];
        let mut value = i64::try_from((seed * 17 + step * 13) % 29 + 1).expect("small");
        if matches!(op, BinOp::Div | BinOp::Mod) {
            value = value.max(1);
        }
        if matches!(op, BinOp::Shl | BinOp::Shr) {
            let bits = match ty {
                Type::I8 | Type::U8 => 7,
                Type::I16 | Type::U16 => 15,
                Type::I32 | Type::U32 => 31,
                _ => 63,
            };
            value %= bits;
        }
        body.push(Stmt::Assign {
            local: LocalId(0),
            val: binary(op, local(0), typed(value, &ty)),
        });
    }
    body.push(Stmt::Return(Some(bitand_255(cast(local(0), Type::I64)))));
    let mut main = main_function(body);
    main.locals = vec![LocalDef {
        name: "acc".to_string(),
        ty,
    }];
    Module {
        structs: Vec::new(),
        unions: Vec::new(),
        rodata: Vec::new(),
        globals: Vec::new(),
        functions: vec![main],
        entry: FnId(0),
    }
}

fn float_module(seed: u64) -> Module {
    let ty = if seed % 2 == 0 { Type::F64 } else { Type::F32 };
    let mut body = vec![Stmt::Assign {
        local: LocalId(0),
        val: Expr::ConstFloat(1.5 + (seed % 5) as f64, ty.clone()),
    }];
    for step in 0..(3 + seed % 5) {
        let op = match (seed + step) % 4 {
            0 => BinOp::Add,
            1 => BinOp::Sub,
            2 => BinOp::Mul,
            _ => BinOp::Div,
        };
        let value = 1.0 + ((seed + step * 3) % 7) as f64;
        body.push(Stmt::Assign {
            local: LocalId(0),
            val: binary(op, local(0), Expr::ConstFloat(value, ty.clone())),
        });
    }
    body.push(Stmt::Return(Some(bitand_255(cast(local(0), Type::I64)))));
    let mut main = main_function(body);
    main.locals = vec![LocalDef {
        name: "acc".to_string(),
        ty,
    }];
    Module {
        structs: Vec::new(),
        unions: Vec::new(),
        rodata: Vec::new(),
        globals: Vec::new(),
        functions: vec![main],
        entry: FnId(0),
    }
}

fn control_module(seed: u64) -> Module {
    let bound = i64::try_from(3 + seed % 9).expect("small");
    let divisor = i64::try_from(2 + seed % 5).expect("small");
    let residue = i64::try_from(seed % u64::try_from(divisor).expect("small")).expect("small");
    let addend = i64::try_from(seed % 11 + 1).expect("small");
    let body = vec![
        Stmt::Assign {
            local: LocalId(0),
            val: Expr::ConstInt(0, Type::I64),
        },
        Stmt::Assign {
            local: LocalId(1),
            val: Expr::ConstInt(0, Type::I64),
        },
        Stmt::Loop {
            body: vec![
                Stmt::Assign {
                    local: LocalId(0),
                    val: binary(BinOp::Add, local(0), Expr::ConstInt(1, Type::I64)),
                },
                Stmt::If {
                    cond: binary(BinOp::Ge, local(0), Expr::ConstInt(bound, Type::I64)),
                    then_body: vec![Stmt::Break],
                    else_body: Vec::new(),
                },
                Stmt::If {
                    cond: binary(
                        BinOp::Eq,
                        binary(BinOp::Mod, local(0), Expr::ConstInt(divisor, Type::I64)),
                        Expr::ConstInt(residue, Type::I64),
                    ),
                    then_body: vec![Stmt::Continue],
                    else_body: Vec::new(),
                },
                Stmt::Assign {
                    local: LocalId(1),
                    val: binary(
                        BinOp::Add,
                        local(1),
                        binary(BinOp::Add, local(0), Expr::ConstInt(addend, Type::I64)),
                    ),
                },
            ],
        },
        Stmt::Return(Some(bitand_255(local(1)))),
    ];
    let mut main = main_function(body);
    main.locals = vec![
        LocalDef {
            name: "counter".to_string(),
            ty: Type::I64,
        },
        LocalDef {
            name: "total".to_string(),
            ty: Type::I64,
        },
    ];
    Module {
        structs: Vec::new(),
        unions: Vec::new(),
        rodata: Vec::new(),
        globals: Vec::new(),
        functions: vec![main],
        entry: FnId(0),
    }
}

fn helper_module(seed: u64) -> Module {
    let count = usize::try_from(seed % 11).expect("small");
    let mut params = Vec::with_capacity(count);
    let mut sum = Expr::ConstInt(i64::try_from(seed % 50).expect("small"), Type::I64);
    for index in 0..count {
        params.push(LocalDef {
            name: format!("a{index}"),
            ty: Type::I64,
        });
        sum = binary(BinOp::Add, sum, local(index));
    }
    let helper = Function {
        name: "helper".to_string(),
        params,
        ret: Type::I64,
        locals: Vec::new(),
        body: vec![Stmt::Return(Some(sum))],
    };
    let args = (0..count)
        .map(|index| Expr::ConstInt(i64::try_from(index + 1).expect("small"), Type::I64))
        .collect();
    let main = main_function(vec![Stmt::Return(Some(Expr::Call {
        target: CallTarget::Direct(FnId(1)),
        args,
    }))]);
    Module {
        structs: Vec::new(),
        unions: Vec::new(),
        rodata: Vec::new(),
        globals: Vec::new(),
        functions: vec![main, helper],
        entry: FnId(0),
    }
}

fn indirect_module(seed: u64) -> Module {
    let count = usize::try_from(seed % 9 + 1).expect("small");
    let mut params = Vec::with_capacity(count);
    let mut sum = Expr::ConstInt(i64::try_from(seed % 31).expect("small"), Type::I64);
    for index in 0..count {
        params.push(LocalDef {
            name: format!("p{index}"),
            ty: Type::I64,
        });
        sum = binary(BinOp::Add, sum, local(index));
    }
    let signature = Type::FnPtr {
        params: vec![Type::I64; count],
        ret: Box::new(Type::I64),
    };
    let helper = Function {
        name: "target".to_string(),
        params,
        ret: Type::I64,
        locals: Vec::new(),
        body: vec![Stmt::Return(Some(sum))],
    };
    let args = (0..count)
        .map(|index| Expr::ConstInt(i64::try_from(index * 2 + 1).expect("small"), Type::I64))
        .collect();
    let body = vec![
        Stmt::Assign {
            local: LocalId(0),
            val: Expr::FnAddr(FnId(1)),
        },
        Stmt::Return(Some(Expr::Call {
            target: CallTarget::Indirect(Box::new(local(0))),
            args,
        })),
    ];
    let mut main = main_function(body);
    main.locals = vec![LocalDef {
        name: "fp".to_string(),
        ty: signature,
    }];
    Module {
        structs: Vec::new(),
        unions: Vec::new(),
        rodata: Vec::new(),
        globals: Vec::new(),
        functions: vec![main, helper],
        entry: FnId(0),
    }
}

fn struct_module(seed: u64) -> Module {
    let def = StructDef {
        name: "Pair".to_string(),
        fields: vec![Type::I64, Type::F64],
    };
    let base = Expr::AddrOfLocal(LocalId(0));
    let field = Expr::FieldAddr {
        base_ptr: Box::new(base),
        struct_id: StructId(0),
        field_idx: 0,
    };
    let body = vec![
        Stmt::Store {
            ptr: field.clone(),
            ty: Type::I64,
            val: Expr::ConstInt(i64::try_from(seed % 97).expect("small"), Type::I64),
        },
        Stmt::Return(Some(bitand_255(Expr::Load {
            ptr: Box::new(field),
            ty: Type::I64,
        }))),
    ];
    let mut main = main_function(body);
    main.locals = vec![LocalDef {
        name: "pair".to_string(),
        ty: Type::Struct(StructId(0)),
    }];
    Module {
        structs: vec![def],
        unions: Vec::new(),
        rodata: Vec::new(),
        globals: Vec::new(),
        functions: vec![main],
        entry: FnId(0),
    }
}

fn syscall_module(seed: u64) -> Module {
    let text = format!("fuzz-{seed}\n");
    let body = vec![
        Stmt::Expr(Expr::Syscall {
            num: Box::new(Expr::ConstInt(1, Type::I64)),
            args: vec![
                Expr::ConstInt(1, Type::I64),
                Expr::RodataPtr(tc_ir::RodataId(0)),
                Expr::RodataLen(tc_ir::RodataId(0)),
            ],
        }),
        Stmt::Return(Some(Expr::ConstInt(
            i64::try_from(seed % 256).expect("small"),
            Type::I64,
        ))),
    ];
    Module {
        structs: Vec::new(),
        unions: Vec::new(),
        rodata: vec![RodataDef {
            name: "msg".to_string(),
            bytes: text.into_bytes(),
        }],
        globals: Vec::new(),
        functions: vec![main_function(body)],
        entry: FnId(0),
    }
}

fn temp_path(seed: u64) -> PathBuf {
    std::env::temp_dir().join(format!("tc_r1_fuzz_ir_{}_{}.elf", std::process::id(), seed))
}

fn check_static(path: &Path, seed: u64) -> Result<(), String> {
    let dynamic = Command::new("readelf")
        .arg("-d")
        .arg(path)
        .output()
        .map_err(|error| format!("seed {seed}: readelf: {error}"))?;
    let text = String::from_utf8_lossy(&dynamic.stdout);
    if text.contains("NEEDED") {
        return Err(format!("seed {seed}: ELF has NEEDED entries"));
    }
    let ldd = Command::new("ldd")
        .arg(path)
        .output()
        .map_err(|error| format!("seed {seed}: ldd: {error}"))?;
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&ldd.stdout),
        String::from_utf8_lossy(&ldd.stderr)
    );
    if !output.contains("not a dynamic executable") && !output.contains("statically linked") {
        return Err(format!("seed {seed}: unexpected ldd output: {output}"));
    }
    Ok(())
}

fn run_seed(seed: u64) -> Result<(), String> {
    let module = module(seed);
    verify_module(&module).map_err(|errors| format!("seed {seed}: verify: {errors:?}"))?;
    let args = vec!["prog".to_string()];
    let interp = interpret(&module, &args, &[], 20_000_000)
        .map_err(|error| format!("seed {seed}: interpreter: {error:?}"))?;
    let path = temp_path(seed);
    compile_to_elf(&module, &path).map_err(|error| format!("seed {seed}: codegen: {error}"))?;
    let result = (|| {
        check_static(&path, seed)?;
        let output = Command::new(&path)
            .arg0("prog")
            .env_clear()
            .stdin(Stdio::null())
            .output()
            .map_err(|error| format!("seed {seed}: run: {error}"))?;
        let exit = output.status.code().unwrap_or(-1);
        if exit != interp.exit_code
            || output.stdout != interp.stdout
            || output.stderr != interp.stderr
        {
            return Err(format!(
                "seed {seed}: parity mismatch elf=({exit}, {:?}, {:?}) interp=({}, {:?}, {:?})",
                output.stdout, output.stderr, interp.exit_code, interp.stdout, interp.stderr
            ));
        }
        Ok(())
    })();
    let _ = std::fs::remove_file(&path);
    result
}

fn main() -> std::process::ExitCode {
    let mut args = std::env::args().skip(1);
    let mut start = 1_u64;
    let mut end = 2000_u64;
    let mut log = PathBuf::from("battletest_core/FUZZ_LOG.jsonl");
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--start" => {
                start = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(1);
            }
            "--end" => {
                end = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(2000);
            }
            "--log" => {
                log = args.next().map_or(log, PathBuf::from);
            }
            other => {
                eprintln!("fuzz_ir: unknown argument `{other}`");
                return std::process::ExitCode::from(2);
            }
        }
    }
    let mut failures = 0_u64;
    let mut batch_started = start;
    let mut batch_passed = 0_u64;
    for seed in start..=end {
        match run_seed(seed) {
            Ok(()) => batch_passed += 1,
            Err(message) => {
                failures += 1;
                eprintln!("{message}");
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log)
                {
                    let _ = writeln!(
                        file,
                        "{{\"engine\":\"ir_x86\",\"seed\":{seed},\"status\":\"FAIL\"}}"
                    );
                }
            }
        }
        if seed % 50 == 0 || seed == end {
            println!("ir_x86 {batch_started}..{seed}: passed {batch_passed} failed {failures}");
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log)
            {
                let _ = writeln!(
                    file,
                    "{{\"engine\":\"ir_x86\",\"start\":{batch_started},\"end\":{seed},\"passed\":{batch_passed},\"failed\":{failures}}}"
                );
            }
            batch_started = seed + 1;
            batch_passed = 0;
        }
    }
    if failures == 0 {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
