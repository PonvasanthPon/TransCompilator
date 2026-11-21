//! Core-7 typed IR, verifier, formatter, and reference interpreter.

pub mod fmt;
pub mod interp;
pub mod ir;
pub mod types;
pub mod verify;

pub use fmt::{format_expr, format_module, format_type};
pub use interp::{InterpError, InterpOutput, interpret};
pub use ir::{
    BinOp, CallTarget, Expr, FnId, Function, GlobalDef, GlobalId, LocalDef, LocalId, Module,
    RodataDef, RodataId, Stmt, StructDef, StructId, UnOp, UnionDef, UnionId,
};
pub use types::{Layout, LocalLayout, LocalSlot, Type};
pub use verify::{VerifyError, infer_expr_type, verify_module};

/// Returns the canonical package name of this crate.
#[must_use]
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

/// Returns the canonical package names of this crate's direct dependencies.
#[must_use]
pub fn dependency_names() -> [&'static str; 0] {
    []
}

#[cfg(test)]
mod tests {
    use super::{crate_name, dependency_names};

    #[test]
    fn crate_identity_matches_workspace() {
        assert_eq!(crate_name(), "tc-ir");
        assert!(dependency_names().is_empty());
    }
}
