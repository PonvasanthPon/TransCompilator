//! `tc-lsp` command-line entry point.

use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("--version" | "-V") => {
            println!("tc-lsp {} (tc_r1)", tc_lsp::version());
            ExitCode::SUCCESS
        }
        Some("--help" | "-h") => {
            println!("usage: tc-lsp [--stdio] [--version]");
            ExitCode::SUCCESS
        }
        Some("--stdio") | None => match tc_lsp::run_stdio() {
            Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
            Err(error) => {
                eprintln!("tc-lsp: {error}");
                ExitCode::FAILURE
            }
        },
        Some(argument) => {
            eprintln!("tc-lsp: unrecognized argument `{argument}`");
            ExitCode::from(2)
        }
    }
}
