//! High-level typed AST, module resolution, type checking, generics, and
//! lowering into the Core-7 IR.
//!
//! Pipeline position: language-agnostic typed middle-end after semantic
//! lowering and before Core-7 verification.

pub mod ast;
mod check;
mod lower;
mod mono;
mod resolve;
pub mod suggest;

use tc_ir::Module;

pub use ast::{
    AstExpr, AstFn, AstGlobal, AstModule, AstStmt, AstStruct, AstType, AstUnion, Import,
    PhraseEntry, PhrasePart, PhrasePattern, Program, Span, TypeError, TypeErrorKind, Visibility,
};

use check::check_program;
use lower::lower_program;
use mono::monomorphize;
use resolve::resolve_program;

/// Type-checks a program, monomorphizes reachable generics, and lowers it into
/// a verified Core-7 IR module.
///
/// # Errors
///
/// Returns all structured diagnostics collected while resolving modules,
/// checking types, instantiating generics, or lowering.
pub fn check_and_lower(program: &Program) -> Result<Module, Vec<TypeError>> {
    let resolved = resolve_program(program)?;
    let checked = check_program(resolved)?;
    let monomorphized = monomorphize(&checked)?;
    lower_program(&monomorphized)
}

/// Validates all declarative phrase patterns and exports them for Phase 3.
///
/// # Errors
///
/// Returns diagnostics for malformed modules, visibility violations, or phrase
/// patterns that bind unknown/duplicate parameters.
pub fn collect_phrases(program: &Program) -> Result<Vec<PhraseEntry>, Vec<TypeError>> {
    let resolved = resolve_program(program)?;
    check::validate_phrases(&resolved)
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
        assert_eq!(crate_name(), "tc-typeck");
        assert_eq!(dependency_names(), ["tc-ir"]);
    }
}
