//! Lexicon scaffolding for `tc_r1`.
//!
//! Phase 0 intentionally contains no lexicon implementation. This crate only
//! establishes the workspace boundary for the future lexical layer.

/// Returns the canonical package name of this crate.
#[must_use]
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

#[cfg(test)]
mod tests {
    use super::crate_name;

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(crate_name(), "tc-lexicon");
    }
}
