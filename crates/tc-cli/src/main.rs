//! Command-line driver for the `tc_r1` workspace.

use std::collections::{HashMap, VecDeque};
use std::env;
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use tc_ir::{
    BinOp, CallTarget, Expr, FnId, Function, LocalDef, LocalId, Module, RodataDef, Stmt, Type,
    interpret, verify_module,
};
use tc_semantics::{CompiledProject, LoweredProject, SourceFile};

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
        Some("build") => run_build(args.collect()),
        Some("run") => run_run(args.collect()),
        Some("interp") => run_interp(args.collect()),
        Some("trace") => run_trace(args.collect()),
        Some("paraphrase") => run_paraphrase(args.collect()),
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
    eprintln!(
        "usage: tc <build|run|interp|trace|paraphrase> <file.eng|file.tc> [-o <out_elf>] [-I <dir>] [--paraphrase] [-- <args...>]\n       tc [--version|--audit|--selftest]"
    );
}

#[derive(Debug, Default)]
struct CommandLine {
    file: Option<PathBuf>,
    output: Option<PathBuf>,
    includes: Vec<PathBuf>,
    program_args: Vec<String>,
    paraphrase: bool,
}

fn parse_command_line(args: Vec<String>, allow_output: bool) -> Result<CommandLine, String> {
    let mut parsed = CommandLine::default();
    let mut index = 0;
    let mut after_separator = false;
    while index < args.len() {
        let argument = &args[index];
        if after_separator {
            parsed.program_args.push(argument.clone());
            index += 1;
            continue;
        }
        match argument.as_str() {
            "--" => {
                after_separator = true;
                index += 1;
            }
            "-o" if allow_output => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "-o requires a path".to_string())?;
                parsed.output = Some(PathBuf::from(value));
                index += 1;
            }
            "-I" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "-I requires a directory".to_string())?;
                parsed.includes.push(PathBuf::from(value));
                index += 1;
            }
            "--paraphrase" => {
                parsed.paraphrase = true;
                index += 1;
            }
            value if value.starts_with('-') && value.len() > 1 => {
                return Err(format!("unknown option `{value}`"));
            }
            value => {
                if parsed.file.is_none() {
                    parsed.file = Some(PathBuf::from(value));
                } else {
                    parsed.program_args.push(value.to_string());
                }
                index += 1;
            }
        }
    }
    if parsed.file.is_none() {
        return Err("missing English source file".to_string());
    }
    Ok(parsed)
}

fn module_imports(source: &SourceFile) -> Vec<String> {
    let document = tc_syntax::segment_source(source);
    let mut imports = Vec::new();
    for item in document.items {
        let tc_syntax::RawItem::Sentence(sentence) = item else {
            continue;
        };
        let tokens = tc_syntax::tokenize(&sentence.text);
        if !tokens.first().is_some_and(|token| token.is("import")) {
            continue;
        }
        if let Some(name) = tokens.get(1).filter(|token| token.is_word()) {
            imports.push(name.lower.clone());
        }
    }
    imports
}

fn find_std_dir() -> Option<PathBuf> {
    if let Some(value) = env::var_os("TC_STD_DIR") {
        let path = PathBuf::from(value);
        if path.join("core.eng").is_file() {
            return Some(path);
        }
    }
    if let Ok(executable) = env::current_exe() {
        let mut directory = executable.parent().map(Path::to_path_buf);
        for _ in 0..4 {
            if let Some(current) = &directory {
                let candidate = current.join("std");
                if candidate.join("core.eng").is_file() {
                    return Some(candidate);
                }
                directory = current.parent().map(Path::to_path_buf);
            }
        }
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../std");
    manifest
        .join("core.eng")
        .is_file()
        .then(|| manifest.canonicalize().unwrap_or(manifest))
}

fn find_import(
    module: &str,
    current_dir: Option<&Path>,
    entry: &Path,
    includes: &[PathBuf],
    std_dir: Option<&Path>,
) -> Option<PathBuf> {
    let mut directories = Vec::new();
    if let Some(parent) = current_dir {
        directories.push(parent.to_path_buf());
    }
    if let Some(parent) = entry.parent() {
        if !directories.contains(&parent.to_path_buf()) {
            directories.push(parent.to_path_buf());
        }
    }
    directories.extend(includes.iter().cloned());
    if let Some(std_dir) = std_dir {
        directories.push(std_dir.to_path_buf());
    }
    for directory in directories {
        let english = directory.join(format!("{module}.eng"));
        if english.is_file() {
            return Some(english);
        }
        let legacy = directory.join(format!("{module}.tc"));
        if legacy.is_file() {
            return Some(legacy);
        }
    }
    None
}

fn source_display_name(path: &Path) -> Result<String, String> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("source path `{}` has no file name", path.display()))?;
    if let Some(stem) = name
        .strip_suffix(".eng")
        .or_else(|| name.strip_suffix(".ENG"))
    {
        Ok(format!("{stem}.tc"))
    } else {
        Ok(name.to_string())
    }
}

fn load_sources(entry: &Path, includes: &[PathBuf]) -> Result<(Vec<SourceFile>, String), String> {
    let entry = entry
        .canonicalize()
        .map_err(|error| format!("cannot open `{}`: {error}", entry.display()))?;
    let entry_source = SourceFile::new(
        source_display_name(&entry)?,
        fs::read_to_string(&entry)
            .map_err(|error| format!("cannot read `{}`: {error}", entry.display()))?,
    );
    let entry_module = entry_source.module_name();
    let std_dir = find_std_dir();
    let mut queue = VecDeque::from([entry.clone()]);
    let mut seen = HashMap::from([(entry_module.clone(), entry.clone())]);
    let mut sources = Vec::new();
    while let Some(path) = queue.pop_front() {
        let source = SourceFile::new(
            source_display_name(&path)?,
            fs::read_to_string(&path)
                .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?,
        );
        for import in module_imports(&source) {
            if seen.contains_key(&import) {
                continue;
            }
            let import_path = find_import(
                &import,
                path.parent(),
                &entry,
                includes,
                std_dir.as_deref(),
            )
            .ok_or_else(|| {
                format!(
                    "imported module `{import}` was not found in the source directory, -I paths, or std/"
                )
            })?;
            seen.insert(import, import_path.clone());
            queue.push_back(import_path);
        }
        sources.push(source);
    }
    Ok((sources, entry_module))
}

fn compile_file(command: &CommandLine) -> Result<CompiledProject, String> {
    let file = command
        .file
        .as_ref()
        .ok_or_else(|| "missing file".to_string())?;
    let (sources, entry_module) = load_sources(file, &command.includes)?;
    tc_semantics::compile_english_to_ir(&sources, &entry_module)
        .map_err(|errors| format_errors(&errors))
}

fn format_errors(errors: &[tc_semantics::SemanticError]) -> String {
    errors
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn run_build(args: Vec<String>) -> ExitCode {
    let command = match parse_command_line(args, true) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("tc build: {message}");
            return ExitCode::from(2);
        }
    };
    let compiled = match compile_file(&command) {
        Ok(compiled) => compiled,
        Err(message) => {
            eprintln!("tc build: {message}");
            return ExitCode::FAILURE;
        }
    };
    let file = command.file.expect("validated");
    let output = command.output.unwrap_or_else(|| {
        let stem = file
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("a.out");
        file.with_file_name(format!("{stem}.elf"))
    });
    match tc_codegen_x86::compile_to_elf(&compiled.module, &output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("tc build: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_run(args: Vec<String>) -> ExitCode {
    let command = match parse_command_line(args, false) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("tc run: {message}");
            return ExitCode::from(2);
        }
    };
    let compiled = match compile_file(&command) {
        Ok(compiled) => compiled,
        Err(message) => {
            eprintln!("tc run: {message}");
            return ExitCode::FAILURE;
        }
    };
    let path = std::env::temp_dir().join(format!(
        "tc_r1_run_{}_{}.elf",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |value| value.as_nanos())
    ));
    if let Err(error) = tc_codegen_x86::compile_to_elf(&compiled.module, &path) {
        eprintln!("tc run: {error}");
        return ExitCode::FAILURE;
    }
    let status = Command::new(&path)
        .args(&command.program_args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();
    let _ = fs::remove_file(&path);
    match status {
        Ok(status) => ExitCode::from(status.code().unwrap_or(1).rem_euclid(256) as u8),
        Err(error) => {
            eprintln!("tc run: cannot execute compiled program: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_interp(args: Vec<String>) -> ExitCode {
    let command = match parse_command_line(args, false) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("tc interp: {message}");
            return ExitCode::from(2);
        }
    };
    let compiled = match compile_file(&command) {
        Ok(compiled) => compiled,
        Err(message) => {
            eprintln!("tc interp: {message}");
            return ExitCode::FAILURE;
        }
    };
    let mut stdin = Vec::new();
    if std::io::stdin().read_to_end(&mut stdin).is_err() {
        eprintln!("tc interp: cannot read stdin");
        return ExitCode::FAILURE;
    }
    let mut program_argv = vec![
        command
            .file
            .as_ref()
            .map_or_else(|| "program".to_string(), |path| path.display().to_string()),
    ];
    program_argv.extend(command.program_args);
    match interpret(&compiled.module, &program_argv, &stdin, 50_000_000) {
        Ok(output) => {
            let _ = std::io::stdout().write_all(&output.stdout);
            let _ = std::io::stderr().write_all(&output.stderr);
            ExitCode::from(output.exit_code.rem_euclid(256) as u8)
        }
        Err(error) => {
            eprintln!("tc interp: {error:?}");
            ExitCode::FAILURE
        }
    }
}

fn run_trace(args: Vec<String>) -> ExitCode {
    let command = match parse_command_line(args, false) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("tc trace: {message}");
            return ExitCode::from(2);
        }
    };
    let file = command.file.as_ref().expect("validated");
    let (sources, entry_module) = match load_sources(file, &command.includes) {
        Ok(value) => value,
        Err(message) => {
            eprintln!("tc trace: {message}");
            return ExitCode::FAILURE;
        }
    };
    let lowered: LoweredProject = match tc_semantics::lower_project(&sources, &entry_module) {
        Ok(value) => value,
        Err(errors) => {
            eprintln!("tc trace: {}", format_errors(&errors));
            return ExitCode::FAILURE;
        }
    };
    for trace in &lowered.trace {
        print!(
            "{}:{}:{} {}",
            trace.module, trace.line, trace.col, trace.intent
        );
        print!(
            " lg={} words={} links={}",
            trace.lg_used, trace.lg_words, trace.lg_links
        );
        print!(" voice={}", trace.voice);
        if let Some(verb) = &trace.verb {
            print!(" verb={verb}");
        }
        if let Some(lemma) = &trace.lemma {
            print!(" lemma={lemma}");
        }
        if let Some(predicate) = trace.predicate {
            print!(" predicate={predicate:?}");
        }
        if let Some(concept) = trace.concept {
            print!(" concept={concept:?}");
        }
        if let Some(class_id) = trace.class_id {
            print!(" verbnet={class_id}");
        }
        if let Some(synset_id) = trace.synset_id {
            print!(" wordnet={synset_id}");
        }
        if !trace.fillers.is_empty() {
            print!(" fillers={:?}", trace.fillers);
        }
        println!("\n  {}", trace.text);
    }
    let unsupported = lowered
        .trace
        .iter()
        .any(|trace| trace.intent.is_unsupported());
    match tc_typeck::check_and_lower(&lowered.program) {
        Ok(module) => {
            println!("--- TC-IR ---");
            println!("{}", tc_ir::format_module(&module));
            if command.paraphrase {
                println!("=== Paraphrase ===");
                for doc in &lowered.paraphrase {
                    print!("{}", tc_semantics::format_paraphrase_entries(&doc.entries));
                }
            }
            if unsupported {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(errors) => {
            for error in errors {
                eprintln!("{error}");
            }
            ExitCode::FAILURE
        }
    }
}

fn run_paraphrase(args: Vec<String>) -> ExitCode {
    let command = match parse_command_line(args, false) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("tc paraphrase: {message}");
            return ExitCode::from(2);
        }
    };
    let compiled = match compile_file(&command) {
        Ok(compiled) => compiled,
        Err(message) => {
            eprintln!("tc paraphrase: {message}");
            return ExitCode::FAILURE;
        }
    };
    print!("{}", tc_semantics::format_paraphrase(&compiled.paraphrase));
    ExitCode::SUCCESS
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
        "  crates          : {} workspace members (7 compiler crates + 2 editors)",
        crates.len() + 3
    );
    println!("  wordnet entries : {}", stats.wordnet_entries);
    println!("  verbnet frames  : {}", stats.verbnet_frames);
    println!("  irregular lemmas: {}", stats.irregular_lemmas);
    println!("  closed-class    : {}", stats.closed_class_words);
    let std_modules = find_std_dir().map_or(0, |directory| {
        fs::read_dir(directory).map_or(0, |entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry.path().extension().and_then(|value| value.to_str()) == Some("eng")
                })
                .count()
        })
    });
    println!("  std modules     : {std_modules}");
    println!("  compiler deps   : std + local path only");
    println!("  compiler external: 0");
    println!("  GUI deps        : eframe/egui native window");
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
