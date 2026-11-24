//! Native `std`-only Language Server for TransCompilator English `.eng` files.
//!
//! Pipeline position: in-process language server over the same semantic
//! analysis pipeline used by the compiler.

pub mod analysis;
pub mod doc;
pub mod handlers;
pub mod json;
pub mod rpc;
mod tokens;

use std::io::{self, BufRead, Write};

pub use handlers::Server;
pub use json::Json;

/// Runs the server over arbitrary buffered input/output.
///
/// Returns the LSP process exit code (`0` after `shutdown` + `exit`, `1` for
/// an `exit` without `shutdown`, and `0` on clean EOF).
pub fn run_with_io<R: BufRead, W: Write>(reader: &mut R, writer: &mut W) -> i32 {
    let mut server = Server::new();
    loop {
        match rpc::read_json(reader) {
            Ok(Some(message)) => {
                let outcome = server.handle(message);
                for message in outcome.messages {
                    if rpc::write_json(writer, &message).is_err() {
                        return 1;
                    }
                }
                if let Some(code) = outcome.exit_code {
                    return code;
                }
            }
            Ok(None) => return 0,
            Err(_) => return 1,
        }
    }
}

/// Runs the server over process stdin/stdout.
///
/// # Errors
///
/// Returns an I/O error when stdin or stdout cannot be used.
pub fn run_stdio() -> io::Result<i32> {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    Ok(run_with_io(&mut input, &mut output))
}

/// Returns the crate name.
#[must_use]
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

/// Returns the server version.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::{crate_name, version};

    #[test]
    fn crate_identity() {
        assert_eq!(crate_name(), "tc-lsp");
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    }
}
