//! Finite English syntax front end for `tc_r1`.
//!
//! The crate wraps the vendored CMU Link Grammar parser, segments paragraphs
//! and indented blocks, and normalizes dependency links into a small finite set
//! of clause roles.  It performs no semantic classification.

pub mod clause;
pub mod lg_ffi;
pub mod segment;
pub mod token;

pub use clause::{ClauseTree, PrepPhrase, VerbHead, Voice, analyze_clause};
pub use lg_ffi::{LgLink, LgParse, LinkGrammarParser, SyntaxError};
pub use segment::{
    RawBlock, RawDocument, RawItem, RawSentence, SourceFile, segment_source, sentence_tokens,
};
pub use token::{Token, TokenKind, decode_string_token, tokenize};

/// Source location shared by syntax and semantic diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct Span {
    /// One-based line number.
    pub line: u32,
    /// One-based column number.
    pub col: u32,
}

impl Span {
    /// Creates a source span.
    #[must_use]
    pub const fn new(line: u32, col: u32) -> Self {
        Self { line, col }
    }
}

/// Returns the canonical package name of this crate.
#[must_use]
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

/// Returns the canonical package names of this crate's direct dependencies.
#[must_use]
pub fn dependency_names() -> [&'static str; 1] {
    [tc_lexicon::crate_name()]
}

#[cfg(test)]
mod tests {
    use super::{crate_name, dependency_names, tokenize};

    #[test]
    fn crate_identity_matches_workspace() {
        assert_eq!(crate_name(), "tc-syntax");
        assert_eq!(dependency_names(), ["tc-lexicon"]);
    }

    #[test]
    fn tokenization_preserves_strings_and_decimals() {
        let tokens = tokenize("Set x to 12.5 and write \"a. b\".");
        assert!(tokens.iter().any(|token| token.text == "12.5"));
        assert!(tokens.iter().any(|token| token.text == "\"a. b\""));
    }
}
