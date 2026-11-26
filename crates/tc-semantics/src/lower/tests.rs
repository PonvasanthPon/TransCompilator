//! Regression tests for declaration and normalization helpers.

use super::{Span, normalize_function_header, normalize_natural_source};

#[test]
fn natural_header_with_given_parameters() {
    let header = normalize_function_header(
        "Publicly, to twice given value as a whole number, named twice, returning a whole number.",
        "main",
        Span::default(),
    )
    .expect("header");
    assert!(header.contains("function twice"));
    assert!(header.contains("taking value as a whole number"));
    assert!(header.contains("returning a whole number"));
}

#[test]
fn paragraph_function_ends_at_blank_line() {
    let (normalized, _) = normalize_natural_source(
        "Use core.\nTo twice given value as a whole number, named twice, returning a whole number.\nGive back value plus value.\n\nReturn twice 5.\n",
    );
    assert!(normalized.contains("function twice"));
    assert!(normalized.contains("Return value plus value"));
    assert!(normalized.contains("Return twice 5"));
}

#[test]
fn trailing_if_becomes_control_block() {
    let (normalized, _) = normalize_natural_source(
        "To smaller given a as a whole number, b as a whole number, named smaller, returning a whole number.\nGive back a if a is less than b, otherwise give back b.\n",
    );
    assert!(normalized.contains("If a is less than b:"));
    assert!(normalized.contains("Return a"));
    assert!(normalized.contains("Otherwise:"));
    assert!(normalized.contains("Return b"));
}

#[test]
fn inline_naming_becomes_let() {
    let (normalized, _) = normalize_natural_source(
        "To make given value as a whole number, named make, returning nothing.\nvalue plus 1 and call it answer.\n",
    );
    assert!(normalized.contains("Let answer be value plus 1."));
}
