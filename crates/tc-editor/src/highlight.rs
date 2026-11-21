//! Live `.eng` semantic highlighting driven by `tc-lexicon`.

use tc_lexicon::{ClosedClass, classify_verb, lookup_closed_class};

use crate::theme::{Rgb, Style, Theme};

/// Semantic line state populated from compiler results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineState {
    /// Ordinary executable or blank line.
    #[default]
    Normal,
    /// Narrative line omitted by the three-way intent gate.
    ProseOmitted,
    /// Line with a compile/type diagnostic.
    Error,
}

/// Semantic token category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenStyle {
    /// Structural control anchor.
    ControlAnchor,
    /// WordNet/VerbNet computational verb.
    ActionVerb,
    /// ACE anaphor or quantifier.
    AnaphorOrQuantifier,
    /// Hardware type keyword.
    Type,
    /// Numeric literal.
    Number,
    /// String or byte literal.
    String,
    /// Operator, punctuation, preposition, conjunction, or negation.
    Operator,
    /// Ordinary identifier or whitespace.
    Identifier,
    /// Discourse filler.
    Muted,
    /// Omitted prose.
    Omitted,
    /// Active search match.
    SearchMatch,
}

/// One styled span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledSpan {
    /// Visible text.
    pub text: String,
    /// Semantic category.
    pub kind: TokenStyle,
    /// Resolved terminal style.
    pub style: Style,
}

fn style_for(kind: TokenStyle, theme: &Theme) -> Style {
    match kind {
        TokenStyle::ControlAnchor => Style::fg(theme.control).bold(),
        TokenStyle::ActionVerb => Style::fg(theme.action_verb),
        TokenStyle::AnaphorOrQuantifier => Style::fg(theme.anaphor).bold(),
        TokenStyle::Type => Style::fg(theme.type_color),
        TokenStyle::Number => Style::fg(theme.number),
        TokenStyle::String => Style::fg(theme.string),
        TokenStyle::Operator => Style::fg(theme.operator),
        TokenStyle::Identifier => Style::fg(theme.text),
        TokenStyle::Muted => Style::fg(theme.muted),
        TokenStyle::Omitted => Style::fg(theme.omitted).dim().italic(),
        TokenStyle::SearchMatch => Style::on(theme.header_bg, theme.warning).bold(),
    }
}

fn is_hardware_type(word: &str) -> bool {
    matches!(
        word,
        "i8" | "i16"
            | "i32"
            | "i64"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "f32"
            | "f64"
            | "bool"
            | "void"
            | "ptr"
            | "whole"
            | "unsigned"
            | "signed"
            | "number"
            | "integer"
            | "decimal"
            | "real"
            | "byte"
            | "pointer"
            | "flag"
            | "boolean"
            | "truth"
            | "nothing"
            | "count"
            | "short"
    )
}

fn is_natural_control(word: &str) -> bool {
    matches!(
        word,
        "done"
            | "give"
            | "to"
            | "use"
            | "keep"
            | "record"
            | "publicly"
            | "given"
            | "returning"
            | "ask"
            | "kernel"
    )
}

fn is_anaphor_or_quantifier(word: &str) -> bool {
    matches!(word, "it" | "its" | "every" | "each" | "some" | "no")
}

fn classify_word(word: &str, theme: &Theme) -> StyledSpan {
    let lower = word.to_ascii_lowercase();
    let kind = if is_hardware_type(&lower) {
        TokenStyle::Type
    } else if is_natural_control(&lower) {
        TokenStyle::ControlAnchor
    } else if is_anaphor_or_quantifier(&lower) {
        TokenStyle::AnaphorOrQuantifier
    } else {
        match lookup_closed_class(&lower) {
            Some(ClosedClass::ControlAnchor) => TokenStyle::ControlAnchor,
            Some(ClosedClass::Preposition | ClosedClass::Conjunction | ClosedClass::Negation) => {
                TokenStyle::Operator
            }
            Some(ClosedClass::DiscourseFiller) => TokenStyle::Muted,
            Some(_) => TokenStyle::Identifier,
            None => {
                if classify_verb(&lower, None).is_some() {
                    TokenStyle::ActionVerb
                } else {
                    TokenStyle::Identifier
                }
            }
        }
    };
    StyledSpan {
        text: word.to_string(),
        kind,
        style: style_for(kind, theme),
    }
}

fn push_span(spans: &mut Vec<StyledSpan>, text: String, kind: TokenStyle, theme: &Theme) {
    if text.is_empty() {
        return;
    }
    spans.push(StyledSpan {
        text,
        kind,
        style: style_for(kind, theme),
    });
}

fn scan_tokens(line: &str, theme: &Theme) -> Vec<StyledSpan> {
    let chars = line.chars().collect::<Vec<_>>();
    let mut spans = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let character = chars[index];
        if character == '"' {
            let start = index;
            index += 1;
            let mut escaped = false;
            while index < chars.len() {
                let current = chars[index];
                index += 1;
                if escaped {
                    escaped = false;
                } else if current == '\\' {
                    escaped = true;
                } else if current == '"' {
                    break;
                }
            }
            push_span(
                &mut spans,
                chars[start..index].iter().collect(),
                TokenStyle::String,
                theme,
            );
            continue;
        }
        if character.is_ascii_digit() {
            let start = index;
            index += 1;
            if character == '0' && index < chars.len() {
                let prefix = chars[index];
                if matches!(prefix, 'x' | 'X' | 'b' | 'B' | 'o' | 'O') {
                    index += 1;
                    while index < chars.len()
                        && (chars[index].is_ascii_alphanumeric() || chars[index] == '_')
                    {
                        index += 1;
                    }
                }
            }
            while index < chars.len() {
                let current = chars[index];
                if current.is_ascii_digit()
                    || current == '_'
                    || (current == '.'
                        && index + 1 < chars.len()
                        && chars[index + 1].is_ascii_digit())
                {
                    index += 1;
                } else if matches!(current, 'e' | 'E') {
                    index += 1;
                    if index < chars.len() && matches!(chars[index], '+' | '-') {
                        index += 1;
                    }
                } else {
                    break;
                }
            }
            push_span(
                &mut spans,
                chars[start..index].iter().collect(),
                TokenStyle::Number,
                theme,
            );
            continue;
        }
        if character.is_ascii_alphabetic() || character == '_' {
            let start = index;
            index += 1;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric() || chars[index] == '_')
            {
                index += 1;
            }
            let word = chars[start..index].iter().collect::<String>();
            spans.push(classify_word(&word, theme));
            continue;
        }
        if character.is_whitespace() {
            let start = index;
            while index < chars.len() && chars[index].is_whitespace() {
                index += 1;
            }
            push_span(
                &mut spans,
                chars[start..index].iter().collect(),
                TokenStyle::Identifier,
                theme,
            );
            continue;
        }
        push_span(
            &mut spans,
            character.to_string(),
            TokenStyle::Operator,
            theme,
        );
        index += 1;
    }
    spans
}

#[allow(clippy::redundant_closure_for_method_calls)]
fn apply_search(spans: Vec<StyledSpan>, query: Option<&str>, theme: &Theme) -> Vec<StyledSpan> {
    let Some(query) = query.filter(|value| !value.is_empty()) else {
        return spans;
    };
    let needle = query
        .chars()
        .map(|character| character.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let mut output = Vec::new();
    for span in spans {
        let chars = span.text.chars().collect::<Vec<_>>();
        let haystack = chars
            .iter()
            .map(|character| character.to_ascii_lowercase())
            .collect::<Vec<_>>();
        if haystack.len() < needle.len() {
            output.push(span);
            continue;
        }
        let mut cursor = 0;
        let last = haystack.len() - needle.len();
        let mut position = 0;
        while position <= last {
            if haystack[position..position + needle.len()] == needle[..] {
                if position > cursor {
                    output.push(StyledSpan {
                        text: chars[cursor..position].iter().collect(),
                        kind: span.kind,
                        style: span.style,
                    });
                }
                output.push(StyledSpan {
                    text: chars[position..position + needle.len()].iter().collect(),
                    kind: TokenStyle::SearchMatch,
                    style: style_for(TokenStyle::SearchMatch, theme),
                });
                position += needle.len();
                cursor = position;
            } else {
                position += 1;
            }
        }
        if cursor < chars.len() {
            output.push(StyledSpan {
                text: chars[cursor..].iter().collect(),
                kind: span.kind,
                style: span.style,
            });
        }
    }
    output
}

/// Highlights one line using the lexical engine and active theme.
#[must_use]
pub fn highlight_line(
    line: &str,
    line_state: LineState,
    search_query: Option<&str>,
    theme: &Theme,
) -> Vec<StyledSpan> {
    let mut spans = if line_state == LineState::ProseOmitted {
        vec![StyledSpan {
            text: line.to_string(),
            kind: TokenStyle::Omitted,
            style: style_for(TokenStyle::Omitted, theme),
        }]
    } else {
        scan_tokens(line, theme)
    };
    if line_state == LineState::Error {
        for span in &mut spans {
            span.style.bg = Some(match theme.id {
                crate::theme::ThemeId::Monochrome => theme.error_bg,
                _ => theme.error_bg,
            });
        }
    }
    let highlighted = apply_search(spans, search_query, theme);
    if highlighted.is_empty() && !line.is_empty() {
        return vec![StyledSpan {
            text: line.to_string(),
            kind: TokenStyle::Identifier,
            style: Style::fg(Rgb(theme.text.0, theme.text.1, theme.text.2)),
        }];
    }
    highlighted
}
