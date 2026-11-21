//! Direct x86_64 static ELF64 backend for the Core-7 IR.

mod emit;
mod link;

use std::fmt;
use std::path::Path;

use tc_ir::{Module, verify_module};

/// Backend failure.
#[derive(Debug)]
pub enum CodegenError {
    /// The module did not pass static verification.
    InvalidIr(String),
    /// Assembly emission failed.
    Emit(String),
    /// Filesystem or process I/O failed.
    Io(std::io::Error),
    /// GNU `as` rejected the generated assembly.
    Assembler(String),
    /// GNU `ld` rejected the assembled object.
    Linker(String),
}

impl fmt::Display for CodegenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIr(message) => write!(formatter, "invalid IR: {message}"),
            Self::Emit(message) => write!(formatter, "emission failed: {message}"),
            Self::Io(error) => write!(formatter, "I/O failed: {error}"),
            Self::Assembler(message) => write!(formatter, "assembler failed: {message}"),
            Self::Linker(message) => write!(formatter, "linker failed: {message}"),
        }
    }
}

impl std::error::Error for CodegenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for CodegenError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

/// Emits Intel-syntax x86_64 assembly for a verified module.
///
/// # Errors
///
/// Returns [`CodegenError`] when verification fails or the module uses a
/// construct unsupported by the Phase 0 backend.
pub fn emit_asm(module: &Module) -> Result<String, CodegenError> {
    if let Err(errors) = verify_module(module) {
        let messages = errors
            .into_iter()
            .map(|error| format!("{error:?}"))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(CodegenError::InvalidIr(messages));
    }
    emit::emit_module(module)
}

/// Emits, assembles, and statically links a module to an ELF64 executable.
///
/// # Errors
///
/// Returns [`CodegenError`] when verification, emission, assembly, linking, or
/// filesystem access fails.
pub fn compile_to_elf(module: &Module, out_path: &Path) -> Result<(), CodegenError> {
    let assembly = emit_asm(module)?;
    link::assemble_and_link(&assembly, out_path)
}

/// Returns the canonical package name of this crate.
#[must_use]
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

/// Returns the canonical package names of this crate's direct dependencies.
#[must_use]
pub fn dependency_names() -> [&'static str; 1] {
    [tc_ir::crate_name()]
}

#[cfg(test)]
mod tests {
    use super::{crate_name, dependency_names};

    #[test]
    fn crate_identity_matches_workspace() {
        assert_eq!(crate_name(), "tc-codegen-x86");
        assert_eq!(dependency_names(), ["tc-ir"]);
    }
}
