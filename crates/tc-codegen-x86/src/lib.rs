//! Static `x86_64` `ELF64` code generation scaffolding for `tc_r1`.
//!
//! Phase 0 intentionally contains no code generator. This crate only
//! establishes the workspace boundary for the future backend.

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
