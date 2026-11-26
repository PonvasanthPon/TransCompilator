//! Public semantic highlighting for editor front ends.
//!
//! The browser playground needs the same monochrome typography as the Zed
//! extension, but it must not guess token classes with regular expressions.
//! This module reuses the language server's semantic-token builder and maps
//! the six advertised LSP classes onto the playground's six CSS kinds.

use std::collections::HashMap;

use tc_semantics::{SourceFile, lower_project_outcome_bare};

use crate::analysis::SymbolIndex;
use crate::tokens::{COMMENT, FUNCTION, KEYWORD, READONLY, STRING, VARIABLE, semantic_tokens_for};

/// One zero-based UTF-16 highlight span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightToken {
    /// Zero-based source line.
    pub line: u32,
    /// Zero-based UTF-16 start column.
    pub start: u32,
    /// UTF-16 span length.
    pub len: u32,
    /// One of `verb`, `keyword`, `variable`, `anaphor`, `string`, or `comment`.
    pub kind: &'static str,
}

/// Builds semantic highlight spans for one English source document.
///
/// This runs the compiler's analysis and lowering passes but never invokes
/// the Core-7 interpreter, so it is safe to call on every debounced edit.
#[must_use]
pub fn highlight_text(text: &str) -> Vec<HighlightToken> {
    let sources = [SourceFile::new("untitled.tc", text.to_string())];
    let outcome = lower_project_outcome_bare(&sources, "untitled");
    let symbols = outcome.project.as_ref().map_or_else(
        || SymbolIndex::from_text(text),
        |project| SymbolIndex::from_program(&project.program, &HashMap::new(), text),
    );
    semantic_tokens_for(text, "untitled", &outcome.trace, &symbols)
        .into_iter()
        .filter_map(|token| {
            let kind = match token.token_type {
                FUNCTION => "verb",
                KEYWORD => "keyword",
                STRING => "string",
                COMMENT => "comment",
                VARIABLE if token.modifiers & READONLY != 0 => "anaphor",
                VARIABLE => "variable",
                _ => return None,
            };
            Some(HighlightToken {
                line: token.line,
                start: token.start,
                len: token.length,
                kind,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::highlight_text;

    #[test]
    fn emits_all_six_playground_token_kinds() {
        let text = concat!(
            "She cleared the table after dinner.\n\n",
            "A record Trip has a distance (a whole number) and a toll (a whole number).\n",
            "Create a Trip called pondy.\n",
            "Add 15 to its toll.\n",
            "Print \"Toll: \" with pondy's toll.\n",
        );
        let tokens = highlight_text(text);
        let kinds = tokens
            .iter()
            .map(|token| token.kind)
            .collect::<HashSet<_>>();
        assert!(kinds.contains("verb"), "{kinds:?}");
        assert!(kinds.contains("keyword"), "{kinds:?}");
        assert!(kinds.contains("variable"), "{kinds:?}");
        assert!(kinds.contains("anaphor"), "{kinds:?}");
        assert!(kinds.contains("string"), "{kinds:?}");
        assert!(kinds.contains("comment"), "{kinds:?}");
        for token in tokens {
            let line = text.lines().nth(token.line as usize).expect("line");
            assert!(token.len > 0);
            assert!(token.start as usize <= line.encode_utf16().count());
            assert!((token.start + token.len) as usize <= line.encode_utf16().count());
        }
    }
}
