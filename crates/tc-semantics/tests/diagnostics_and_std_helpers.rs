//! Plain-English "Try Instead" diagnostics and everyday `std/` helper parity.

use tc_editor::runner::{RunResult, run_elf_with_parity};

fn stdout_lines(result: &RunResult) -> Vec<String> {
    let mut output = Vec::new();
    let mut in_stdout = false;
    for line in &result.lines {
        if line == "--- stdout ---" {
            in_stdout = true;
            continue;
        }
        if in_stdout && line.starts_with("--- ") {
            break;
        }
        if in_stdout {
            output.push(line.clone());
        }
    }
    output
}

fn assert_program(name: &str, source: &str, expected: &[&str]) {
    let result = run_elf_with_parity(source, None, "");
    assert!(
        result.ok,
        "{name}: {}\n{}",
        result.summary,
        result.lines.join("\n")
    );
    assert_eq!(stdout_lines(&result), expected, "{name}");
}

fn assert_compile_failure(name: &str, source: &str, needles: &[&str]) {
    let result = run_elf_with_parity(source, None, "");
    assert!(!result.ok, "{name}: unexpectedly compiled");
    let text = result.lines.join("\n").to_ascii_lowercase();
    for needle in needles {
        assert!(
            text.contains(&needle.to_ascii_lowercase()),
            "{name}: expected `{needle}` in:\n{}",
            result.lines.join("\n")
        );
    }
}

fn list_source(body: &str) -> String {
    format!("Use io, list.\n{body}Give back 0.\n")
}

fn string_source(body: &str) -> String {
    format!("Use core, io, string.\n{body}Give back 0.\n")
}

// ---------------------------------------------------------------------------
// Part I: plain-English diagnostics
// ---------------------------------------------------------------------------

#[test]
fn unknown_single_word_variable_suggests_natural_name() {
    assert_compile_failure(
        "diag-variable",
        "Let travel_budget be 10.\nLet x be travle_budget.\nReturn x.\n",
        &["travle_budget", "did you mean 'travel budget'?"],
    );
}

#[test]
fn unknown_multi_word_variable_suggests_spaced_name() {
    assert_compile_failure(
        "diag-multi-variable",
        "Let travel budget be 10.\nLet x be travle budget.\nReturn x.\n",
        &["unexpected trailing token", "did you mean 'travel budget'?"],
    );
}

#[test]
fn unknown_record_field_suggests_field_and_lists_available_fields() {
    assert_compile_failure(
        "diag-field",
        concat!(
            "Use io.\n",
            "A record Waypoint has distance (a whole number) and toll (a whole number).\n",
            "Create a Waypoint called pondy.\n",
            "Say pondy's distnce.\n",
            "Return 0.\n",
        ),
        &[
            "has no field `distnce`",
            "did you mean 'distance'?",
            "available fields: distance, toll",
        ],
    );
}

#[test]
fn unknown_union_field_suggests_member() {
    assert_compile_failure(
        "diag-union-field",
        concat!(
            "Use io.\n",
            "A union Payload has number (a whole number) and truth (a truth value).\n",
            "Let value be Payload.\n",
            "Say value's numbr.\n",
            "Return 0.\n",
        ),
        &["has no member `numbr`", "did you mean 'number'?"],
    );
}

#[test]
fn unknown_function_suggests_imported_function() {
    assert_compile_failure(
        "diag-function",
        "Use math.\nLet x be sqtr(4.0).\nReturn x.\n",
        &["function `sqtr` not found", "did you mean 'sqrt'?"],
    );
}

#[test]
fn unknown_function_in_say_suggests_imported_function() {
    assert_compile_failure(
        "diag-say-function",
        "Use math.\nSay sqtr(4.0).\nReturn 0.\n",
        &["function `sqtr` not found", "did you mean 'sqrt'?"],
    );
}

#[test]
fn reserved_record_name_has_actionable_hint() {
    assert_compile_failure(
        "diag-reserved-record",
        "A record Stop has x (a whole number).\nReturn 0.\n",
        &["reserved control word", "record name", "Waypoint"],
    );
}

#[test]
fn reserved_variable_name_has_actionable_hint() {
    assert_compile_failure(
        "diag-reserved-variable",
        "Put 10 in while.\nReturn 0.\n",
        &["reserved control word", "variable name", "counter"],
    );
}

#[test]
fn reserved_field_name_has_actionable_hint() {
    assert_compile_failure(
        "diag-reserved-field",
        "A record Weird has if (a whole number).\nReturn 0.\n",
        &["reserved control word", "field name", "distance"],
    );
}

#[test]
fn unresolved_it_preserves_code_and_suggests_put() {
    assert_compile_failure(
        "diag-it",
        "Add 10 to it.\nReturn 0.\n",
        &[
            "unresolved_anaphor",
            "introduce one first",
            "put 10 in cash",
        ],
    );
}

#[test]
fn unresolved_its_lists_accessible_fields() {
    assert_compile_failure(
        "diag-its",
        concat!(
            "A record Waypoint has distance (a whole number) and toll (a whole number).\n",
            "Create a Waypoint called pondy.\n",
            "Say its distnce.\n",
            "Return 0.\n",
        ),
        &["unresolved_anaphor", "available on 'pondy': distance, toll"],
    );
}

#[test]
fn unresolved_the_type_suggests_creation() {
    assert_compile_failure(
        "diag-the",
        concat!(
            "A record Waypoint has distance (a whole number).\n",
            "Say the waypoint's distance.\n",
            "Return 0.\n",
        ),
        &[
            "unresolved_anaphor",
            "'the waypoint'",
            "create a waypoint called p",
        ],
    );
}

#[test]
fn unresolved_appositive_keeps_type_mismatch_code() {
    assert_compile_failure(
        "diag-appositive",
        concat!(
            "A record Waypoint has distance (a whole number).\n",
            "A record Other has speed (a whole number).\n",
            "Create a Waypoint called pondy.\n",
            "Create a Other called thing.\n",
            "Say the waypoint thing's speed.\n",
            "Return 0.\n",
        ),
        &["anaphor_type_mismatch", "'thing' of type 'other'"],
    );
}

#[test]
fn unsupported_verb_has_rewrite_hint() {
    assert_compile_failure(
        "diag-unsupported",
        "Bake the cake.\nReturn 0.\n",
        &[
            "unsupported",
            "imperative verb `bake`",
            "try phrasing it with put/let",
        ],
    );
}

// ---------------------------------------------------------------------------
// Part II: std/list helpers
// ---------------------------------------------------------------------------

#[test]
fn list_sum_i64_populated() {
    assert_program(
        "list-sum-i64",
        &list_source(concat!(
            "Let nums be List of whole number.\n",
            "Push 3 to address of nums.\n",
            "Push 4 to address of nums.\n",
            "Push 5 to address of nums.\n",
            "Let total be sum of address of nums.\n",
            "Say total.\n",
            "Free list address of nums.\n",
        )),
        &["12"],
    );
}

#[test]
fn list_sum_i64_empty() {
    assert_program(
        "list-sum-empty",
        &list_source(concat!(
            "Let nums be List of whole number.\n",
            "Let total be sum of address of nums.\n",
            "Say total.\n",
            "Free list address of nums.\n",
        )),
        &["0"],
    );
}

#[test]
fn list_sum_f64_populated() {
    assert_program(
        "list-sum-f64",
        &list_source(concat!(
            "Let nums be List of decimal.\n",
            "Push 1.5 to address of nums.\n",
            "Push 2.5 to address of nums.\n",
            "Let total be sum of address of nums.\n",
            "Say total.\n",
            "Free list address of nums.\n",
        )),
        &["4.0000"],
    );
}

#[test]
fn list_sum_f64_empty() {
    assert_program(
        "list-sum-f64-empty",
        &list_source(concat!(
            "Let nums be List of decimal.\n",
            "Let total be sum of address of nums.\n",
            "Say total.\n",
            "Free list address of nums.\n",
        )),
        &["0.0000"],
    );
}

#[test]
fn list_average_i64_populated() {
    assert_program(
        "list-avg-i64",
        &list_source(concat!(
            "Let nums be List of whole number.\n",
            "Push 1 to address of nums.\n",
            "Push 2 to address of nums.\n",
            "Push 5 to address of nums.\n",
            "Let average be average of address of nums.\n",
            "Say average.\n",
            "Free list address of nums.\n",
        )),
        &["2.6667"],
    );
}

#[test]
fn list_average_i64_empty() {
    assert_program(
        "list-avg-i64-empty",
        &list_source(concat!(
            "Let nums be List of whole number.\n",
            "Let average be average of address of nums.\n",
            "Say average.\n",
            "Free list address of nums.\n",
        )),
        &["0.0000"],
    );
}

#[test]
fn list_average_f64_populated() {
    assert_program(
        "list-avg-f64",
        &list_source(concat!(
            "Let nums be List of decimal.\n",
            "Push 1.5 to address of nums.\n",
            "Push 2.5 to address of nums.\n",
            "Let average be average of address of nums.\n",
            "Say average.\n",
            "Free list address of nums.\n",
        )),
        &["2.0000"],
    );
}

#[test]
fn list_average_f64_empty() {
    assert_program(
        "list-avg-f64-empty",
        &list_source(concat!(
            "Let nums be List of decimal.\n",
            "Let average be average of address of nums.\n",
            "Say average.\n",
            "Free list address of nums.\n",
        )),
        &["0.0000"],
    );
}

#[test]
fn list_minimum_handles_negative_values() {
    assert_program(
        "list-min",
        &list_source(concat!(
            "Let nums be List of whole number.\n",
            "Push -3 to address of nums.\n",
            "Push 7 to address of nums.\n",
            "Push -9 to address of nums.\n",
            "Push 0 to address of nums.\n",
            "Let smallest be minimum of address of nums.\n",
            "Say smallest.\n",
            "Free list address of nums.\n",
        )),
        &["-9"],
    );
}

#[test]
fn list_maximum_handles_negative_values() {
    assert_program(
        "list-max",
        &list_source(concat!(
            "Let nums be List of whole number.\n",
            "Push -3 to address of nums.\n",
            "Push 7 to address of nums.\n",
            "Push -9 to address of nums.\n",
            "Push 0 to address of nums.\n",
            "Let largest be maximum of address of nums.\n",
            "Say largest.\n",
            "Free list address of nums.\n",
        )),
        &["7"],
    );
}

#[test]
fn list_minimum_empty_panics_with_parity() {
    let result = run_elf_with_parity(
        &list_source(concat!(
            "Let nums be List of whole number.\n",
            "Let smallest be minimum of address of nums.\n",
            "Say smallest.\n",
            "Free list address of nums.\n",
        )),
        None,
        "",
    );
    assert!(result.ok, "{}", result.lines.join("\n"));
    assert!(result.summary.contains("exit=1"), "{}", result.summary);
    assert!(result.lines.join("\n").contains("minimum of empty list"));
}

#[test]
fn list_maximum_empty_panics_with_parity() {
    let result = run_elf_with_parity(
        &list_source(concat!(
            "Let nums be List of whole number.\n",
            "Let largest be maximum of address of nums.\n",
            "Say largest.\n",
            "Free list address of nums.\n",
        )),
        None,
        "",
    );
    assert!(result.ok, "{}", result.lines.join("\n"));
    assert!(result.summary.contains("exit=1"), "{}", result.summary);
    assert!(result.lines.join("\n").contains("maximum of empty list"));
}

#[test]
fn list_contains_finds_start_middle_and_end() {
    assert_program(
        "list-contains",
        &list_source(concat!(
            "Let nums be List of whole number.\n",
            "Push 4 to address of nums.\n",
            "Push 1 to address of nums.\n",
            "Push 4 to address of nums.\n",
            "Push 2 to address of nums.\n",
            "Push 5 to address of nums.\n",
            "Let first hit be list address of nums contains 4.\n",
            "Let middle hit be list address of nums contains 1.\n",
            "Let last hit be list address of nums contains 5.\n",
            "Say first hit.\n",
            "Say middle hit.\n",
            "Say last hit.\n",
            "Let missing be list address of nums contains 9.\n",
            "Say missing.\n",
            "Free list address of nums.\n",
        )),
        &["true", "true", "true", "false"],
    );
}

#[test]
fn list_find_returns_zero_based_indexes() {
    assert_program(
        "list-find",
        &list_source(concat!(
            "Let nums be List of whole number.\n",
            "Push 4 to address of nums.\n",
            "Push 1 to address of nums.\n",
            "Push 4 to address of nums.\n",
            "Push 2 to address of nums.\n",
            "Push 5 to address of nums.\n",
            "Let first hit be find 4 in address of nums.\n",
            "Let middle hit be find 1 in address of nums.\n",
            "Let last hit be find 5 in address of nums.\n",
            "Say first hit.\n",
            "Say middle hit.\n",
            "Say last hit.\n",
            "Free list address of nums.\n",
        )),
        &["0", "1", "4"],
    );
}

#[test]
fn list_find_returns_minus_one_when_missing() {
    assert_program(
        "list-find-missing",
        &list_source(concat!(
            "Let nums be List of whole number.\n",
            "Push 4 to address of nums.\n",
            "Let missing be find 9 in address of nums.\n",
            "Say missing.\n",
            "Free list address of nums.\n",
        )),
        &["-1"],
    );
}

#[test]
fn list_reverse_even_count() {
    assert_program(
        "list-reverse-even",
        &list_source(concat!(
            "Let nums be List of whole number.\n",
            "Push 1 to address of nums.\n",
            "Push 2 to address of nums.\n",
            "Push 3 to address of nums.\n",
            "Push 4 to address of nums.\n",
            "Reverse address of nums.\n",
            "Say item 0 of nums's data.\n",
            "Say item 1 of nums's data.\n",
            "Say item 2 of nums's data.\n",
            "Say item 3 of nums's data.\n",
            "Free list address of nums.\n",
        )),
        &["4", "3", "2", "1"],
    );
}

#[test]
fn list_reverse_odd_count() {
    assert_program(
        "list-reverse-odd",
        &list_source(concat!(
            "Let nums be List of whole number.\n",
            "Push 1 to address of nums.\n",
            "Push 2 to address of nums.\n",
            "Push 3 to address of nums.\n",
            "Reverse address of nums.\n",
            "Say item 0 of nums's data.\n",
            "Say item 1 of nums's data.\n",
            "Say item 2 of nums's data.\n",
            "Free list address of nums.\n",
        )),
        &["3", "2", "1"],
    );
}

#[test]
fn list_reverse_empty_and_single() {
    assert_program(
        "list-reverse-small",
        &list_source(concat!(
            "Let empty be List of whole number.\n",
            "Reverse address of empty.\n",
            "Let single be List of whole number.\n",
            "Push 7 to address of single.\n",
            "Reverse address of single.\n",
            "Say item 0 of single's data.\n",
            "Free list address of empty.\n",
            "Free list address of single.\n",
        )),
        &["7"],
    );
}

#[test]
fn list_reverse_generic_point_records() {
    assert_program(
        "list-reverse-points",
        &list_source(concat!(
            "A record Point has x (a whole number) and y (a whole number).\n",
            "Let points be List of Point.\n",
            "Let p be Point.\n",
            "Set p's x to 1.\n",
            "Set p's y to 2.\n",
            "Push p to address of points.\n",
            "Set p's x to 3.\n",
            "Set p's y to 4.\n",
            "Push p to address of points.\n",
            "Reverse address of points.\n",
            "Let first point be item 0 of points's data.\n",
            "Let second point be item 1 of points's data.\n",
            "Say first point's x.\n",
            "Say second point's x.\n",
            "Free list address of points.\n",
        )),
        &["3", "1"],
    );
}

#[test]
fn list_helpers_release_all_heap_memory() {
    assert_program(
        "list-leaks",
        &format!(
            "Use alloc, io, list.\n{}Give back 0.\n",
            concat!(
                "Let nums be List of whole number.\n",
                "Push 4 to address of nums.\n",
                "Push 9 to address of nums.\n",
                "Let total be sum of address of nums.\n",
                "Let smallest be minimum of address of nums.\n",
                "Let largest be maximum of address of nums.\n",
                "Let found be list address of nums contains 9.\n",
                "Let index be find 9 in address of nums.\n",
                "Reverse address of nums.\n",
                "Free list address of nums.\n",
                "Let used be bytes in use.\n",
                "Say total.\n",
                "Say smallest plus largest.\n",
                "Say found.\n",
                "Say index.\n",
                "Say used.\n",
            )
        ),
        &["13", "13", "true", "1", "0"],
    );
}

// ---------------------------------------------------------------------------
// Part II: std/string helpers
// ---------------------------------------------------------------------------

#[test]
fn text_starts_with_handles_true_false_and_long_prefix() {
    assert_program(
        "string-starts",
        &string_source(concat!(
            "Let s be create text from \"hello\" of length 5.\n",
            "Let yes prefix be create text from \"he\" of length 2.\n",
            "Let no prefix be create text from \"lo\" of length 2.\n",
            "Let long prefix be create text from \"hello world\" of length 11.\n",
            "Let yes be text address of s starts with address of yes prefix.\n",
            "Let no be text address of s starts with address of no prefix.\n",
            "Let long be text address of s starts with address of long prefix.\n",
            "Say yes.\n",
            "Say no.\n",
            "Say long.\n",
            "Release text address of s.\n",
            "Release text address of yes prefix.\n",
            "Release text address of no prefix.\n",
            "Release text address of long prefix.\n",
        )),
        &["true", "false", "false"],
    );
}

#[test]
fn text_ends_with_handles_true_false_and_long_suffix() {
    assert_program(
        "string-ends",
        &string_source(concat!(
            "Let s be create text from \"hello\" of length 5.\n",
            "Let yes suffix be create text from \"lo\" of length 2.\n",
            "Let no suffix be create text from \"he\" of length 2.\n",
            "Let long suffix be create text from \"hello world\" of length 11.\n",
            "Let yes be text address of s ends with address of yes suffix.\n",
            "Let no be text address of s ends with address of no suffix.\n",
            "Let long be text address of s ends with address of long suffix.\n",
            "Say yes.\n",
            "Say no.\n",
            "Say long.\n",
            "Release text address of s.\n",
            "Release text address of yes suffix.\n",
            "Release text address of no suffix.\n",
            "Release text address of long suffix.\n",
        )),
        &["true", "false", "false"],
    );
}

#[test]
fn text_contains_handles_empty_needle_and_long_needle() {
    assert_program(
        "string-contains",
        &string_source(concat!(
            "Let s be create text from \"hello\" of length 5.\n",
            "Let needle be create text from \"ell\" of length 3.\n",
            "Let missing be create text from \"xyz\" of length 3.\n",
            "Let empty be create text from \"\" of length 0.\n",
            "Let long be create text from \"hello world\" of length 11.\n",
            "Let yes be text address of s contains address of needle.\n",
            "Let no be text address of s contains address of missing.\n",
            "Let empty hit be text address of s contains address of empty.\n",
            "Let long missing be text address of s contains address of long.\n",
            "Say yes.\n",
            "Say no.\n",
            "Say empty hit.\n",
            "Say long missing.\n",
            "Release text address of s.\n",
            "Release text address of needle.\n",
            "Release text address of missing.\n",
            "Release text address of empty.\n",
            "Release text address of long.\n",
        )),
        &["true", "false", "true", "false"],
    );
}

#[test]
fn trim_text_strips_spaces() {
    assert_program(
        "string-trim-spaces",
        &string_source(concat!(
            "Let spaced be create text from \"  hi  \" of length 6.\n",
            "Let expected be create text from \"hi\" of length 2.\n",
            "Let trimmed be trim text address of spaced.\n",
            "Let same be text address of trimmed matches text address of expected.\n",
            "Say same.\n",
            "Release text address of spaced.\n",
            "Release text address of expected.\n",
            "Release text address of trimmed.\n",
        )),
        &["true"],
    );
}

#[test]
fn trim_text_strips_tabs_and_carriage_returns() {
    assert_program(
        "string-trim-whitespace",
        &string_source(concat!(
            "Let spaced be create text from \"\t hi \r\" of length 6.\n",
            "Let expected be create text from \"hi\" of length 2.\n",
            "Let trimmed be trim text address of spaced.\n",
            "Let same be text address of trimmed matches text address of expected.\n",
            "Say same.\n",
            "Release text address of spaced.\n",
            "Release text address of expected.\n",
            "Release text address of trimmed.\n",
        )),
        &["true"],
    );
}

#[test]
fn trim_text_handles_empty_all_whitespace_and_already_trimmed() {
    assert_program(
        "string-trim-edge",
        &string_source(concat!(
            "Let empty be create text from \"\" of length 0.\n",
            "Let spaces be create text from \"    \" of length 4.\n",
            "Let clean text be create text from \"hi\" of length 2.\n",
            "Let trimmed empty be trim text address of empty.\n",
            "Let trimmed spaces be trim text address of spaces.\n",
            "Let trimmed clean be trim text address of clean text.\n",
            "Say trimmed empty's len.\n",
            "Say trimmed spaces's len.\n",
            "Say trimmed clean's len.\n",
            "Release text address of empty.\n",
            "Release text address of spaces.\n",
            "Release text address of clean text.\n",
            "Release text address of trimmed empty.\n",
            "Release text address of trimmed spaces.\n",
            "Release text address of trimmed clean.\n",
        )),
        &["0", "0", "2"],
    );
}

#[test]
fn parse_whole_number_from_plain_digits() {
    assert_program(
        "string-parse",
        &string_source(concat!(
            "Let text be create text from \"42\" of length 2.\n",
            "Let parsed be parse whole number from address of text.\n",
            "Say parsed's is_ok.\n",
            "Say parsed's ok_val.\n",
            "Release text address of text.\n",
        )),
        &["true", "42"],
    );
}

#[test]
fn parse_whole_number_skips_surrounding_whitespace_and_sign() {
    assert_program(
        "string-parse-space",
        &string_source(concat!(
            "Let text be create text from \"  -123  \" of length 8.\n",
            "Let parsed be parse whole number from address of text.\n",
            "Say parsed's is_ok.\n",
            "Say parsed's ok_val.\n",
            "Release text address of text.\n",
        )),
        &["true", "-123"],
    );
}

#[test]
fn parse_whole_number_accepts_plus_and_zero() {
    assert_program(
        "string-parse-plus",
        &string_source(concat!(
            "Let plus text be create text from \"+7\" of length 2.\n",
            "Let zero text be create text from \"0\" of length 1.\n",
            "Let plus parsed be parse whole number from address of plus text.\n",
            "Let zero parsed be parse whole number from address of zero text.\n",
            "Say plus parsed's is_ok.\n",
            "Say plus parsed's ok_val.\n",
            "Say zero parsed's ok_val.\n",
            "Release text address of plus text.\n",
            "Release text address of zero text.\n",
        )),
        &["true", "7", "0"],
    );
}

#[test]
fn parse_whole_number_rejects_invalid_and_empty_input() {
    assert_program(
        "string-parse-invalid",
        &string_source(concat!(
            "Let invalid be create text from \"abc\" of length 3.\n",
            "Let empty be create text from \"\" of length 0.\n",
            "Let invalid parsed be parse whole number from address of invalid.\n",
            "Let empty parsed be parse whole number from address of empty.\n",
            "Say invalid parsed's is_ok.\n",
            "Say empty parsed's is_ok.\n",
            "Release text address of invalid.\n",
            "Release text address of empty.\n",
        )),
        &["false", "false"],
    );
}

#[test]
fn format_whole_number_handles_zero_positive_and_negative() {
    assert_program(
        "string-format",
        &string_source(concat!(
            "Let zero text be format whole number 0 as text.\n",
            "Let positive text be format whole number 42 as text.\n",
            "Let negative text be format whole number -999 as text.\n",
            "Say address of zero text.\n",
            "Say address of positive text.\n",
            "Say address of negative text.\n",
            "Release text address of zero text.\n",
            "Release text address of positive text.\n",
            "Release text address of negative text.\n",
        )),
        &["0", "42", "-999"],
    );
}

#[test]
fn format_whole_number_handles_i64_minimum() {
    assert_program(
        "string-format-min",
        &string_source(concat!(
            "Let formatted be format whole number -9223372036854775808 as text.\n",
            "Say address of formatted.\n",
            "Release text address of formatted.\n",
        )),
        &["-9223372036854775808"],
    );
}

#[test]
fn format_then_parse_round_trips() {
    assert_program(
        "string-roundtrip",
        &string_source(concat!(
            "Let formatted be format whole number -1234567 as text.\n",
            "Let parsed be parse whole number from address of formatted.\n",
            "Say parsed's is_ok.\n",
            "Say parsed's ok_val.\n",
            "Release text address of formatted.\n",
        )),
        &["true", "-1234567"],
    );
}

#[test]
fn string_helpers_release_all_heap_memory() {
    assert_program(
        "string-leaks",
        &format!(
            "Use alloc, core, io, string.\n{}Give back 0.\n",
            concat!(
                "Let s be create text from \"  hello  \" of length 9.\n",
                "Let needle be create text from \"ell\" of length 3.\n",
                "Let found be text address of s contains address of needle.\n",
                "Let trimmed be trim text address of s.\n",
                "Let formatted be format whole number 77 as text.\n",
                "Let parsed be parse whole number from address of formatted.\n",
                "Say found.\n",
                "Say trimmed's len.\n",
                "Say parsed's ok_val.\n",
                "Release text address of s.\n",
                "Release text address of needle.\n",
                "Release text address of trimmed.\n",
                "Release text address of formatted.\n",
                "Let used be bytes in use.\n",
                "Say used.\n",
            )
        ),
        &["true", "5", "77", "0"],
    );
}
