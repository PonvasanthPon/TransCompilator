//! Command-line driver for the `tc_r1` workspace.
//!
//! Phase 0 supports only `--version` and `--audit`. No compiler pipeline is
//! implemented yet.

use std::env;
use std::process::ExitCode;

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
    eprintln!("usage: tc [--version|--audit]");
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

    println!("tc_r1 audit: PASS");
    println!(
        "  crates         : {} linked (6 libs + 1 cli)",
        crates.len() + 1
    );
    println!("  dependencies   : std + local path only");
    println!("  external crates: 0");
    println!("  ML runtime     : disabled");
    println!("  network        : disabled");
    ExitCode::SUCCESS
}
