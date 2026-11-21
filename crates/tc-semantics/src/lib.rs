//! Semantic analysis scaffolding for `tc_r1`.
//!
//! Phase 0 intentionally contains no semantic implementation. This crate only
//! establishes the workspace boundary for the future semantic layer.

/// Returns the canonical package name of this crate.
#[must_use]
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

/// Returns the canonical package names of this crate's direct dependencies.
#[must_use]
pub fn dependency_names() -> [&'static str; 4] {
    [
        tc_ir::crate_name(),
        tc_lexicon::crate_name(),
        tc_syntax::crate_name(),
        tc_typeck::crate_name(),
    ]
}

#[cfg(test)]
mod tests {
    use super::{crate_name, dependency_names};

    #[test]
    fn crate_identity_matches_workspace() {
        assert_eq!(crate_name(), "tc-semantics");
        assert_eq!(
            dependency_names(),
            ["tc-ir", "tc-lexicon", "tc-syntax", "tc-typeck"]
        );
    }
}
