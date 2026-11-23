//! Prototype-style compositional English: Emit, clause graphs, and recursive NPs.

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

fn assert_omitted(name: &str, source: &str) {
    let result = run_elf_with_parity(source, None, "");
    assert!(result.ok, "{name}: {}", result.lines.join("\n"));
    assert!(stdout_lines(&result).is_empty(), "{name}");
    assert!(
        !result.omitted_lines.is_empty(),
        "{name}: {:?}",
        result.omitted_lines
    );
}

#[test]
fn emit_motivating_where_sum() {
    assert_program(
        "motivating-where",
        "Print the sum of a and b, where a is 12 and b is 23.\n",
        &["35"],
    );
}

#[test]
fn emit_show_motivating_where_sum() {
    assert_program(
        "show-where",
        "Show the sum of a and b, where a is 12 and b is 23.\n",
        &["35"],
    );
}

#[test]
fn emit_display_product_where() {
    assert_program(
        "display-product",
        "Display the product of x and y, where x is 6 and y is 7.\n",
        &["42"],
    );
}

#[test]
fn emit_output_difference() {
    assert_program(
        "output-difference",
        "Output the difference between 50 and 18.\n",
        &["32"],
    );
}

#[test]
fn emit_say_quotient() {
    assert_program("say-quotient", "Say the quotient of 100 and 4.\n", &["25"]);
}

#[test]
fn emit_report_remainder() {
    assert_program(
        "report-remainder",
        "Report the remainder of 29 divided by 5.\n",
        &["4"],
    );
}

#[test]
fn emit_print_integer_literal() {
    assert_program("print-int", "Print 12.\n", &["12"]);
}

#[test]
fn emit_print_text_literal() {
    assert_program(
        "print-text",
        "Print \"Hello, world!\".\n",
        &["Hello, world!"],
    );
}

#[test]
fn emit_print_decimal_literal() {
    assert_program("print-decimal", "Print 3.5.\n", &["3.5000"]);
}

#[test]
fn emit_print_boolean_literal() {
    assert_program("print-bool", "Print true.\n", &["true"]);
}

#[test]
fn emit_explicit_use_io_is_idempotent() {
    assert_program(
        "explicit-io",
        "Use io.\nPrint the sum of a and b, where a is 10 and b is 20.\n",
        &["30"],
    );
}

#[test]
fn recursive_nested_product_sum_difference() {
    assert_program(
        "nested-product",
        "Print the product of the sum of a and b and the difference between c and d, where a is 3, b is 4, c is 10, and d is 2.\n",
        &["56"],
    );
}

#[test]
fn recursive_three_operand_sum() {
    assert_program("sum-three", "Print the sum of 10, 20, and 30.\n", &["60"]);
}

#[test]
fn recursive_three_operand_average() {
    assert_program(
        "average-three",
        "Print the average of 10, 20, and 30.\n",
        &["20"],
    );
}

#[test]
fn recursive_minimum() {
    assert_program("minimum", "Print the minimum of 42, 7, and 19.\n", &["7"]);
}

#[test]
fn recursive_maximum() {
    assert_program("maximum", "Print the maximum of 42, 7, and 19.\n", &["42"]);
}

#[test]
fn recursive_twice_sum() {
    assert_program("twice-sum", "Print twice the sum of 10 and 5.\n", &["30"]);
}

#[test]
fn recursive_half_sum() {
    assert_program("half-sum", "Print half of the sum of 12 and 24.\n", &["18"]);
}

#[test]
fn recursive_sum_of_squares() {
    assert_program(
        "sum-squares",
        "Print the sum of a squared and b squared, where a is 3 and b is 4.\n",
        &["25"],
    );
}

#[test]
fn recursive_cube_of() {
    assert_program("cube", "Print the cube of 3.\n", &["27"]);
}

#[test]
fn recursive_absolute_value() {
    assert_program(
        "absolute",
        "Print the absolute value of negative 42.\n",
        &["42"],
    );
}

#[test]
fn recursive_decimal_sum() {
    assert_program(
        "decimal-sum",
        "Print the sum of a and b, where a is 1.5 and b is 2.25.\n",
        &["3.7500"],
    );
}

#[test]
fn recursive_expression_inside_condition() {
    assert_program(
        "condition-np",
        "Given a is 10 and b is 20, if the sum of a and b is greater than 25, print the product of a and b.\n",
        &["200"],
    );
}

#[test]
fn dependent_where_bindings() {
    assert_program(
        "dependent-where",
        "Print the quotient of total and count, where total is the sum of 10 and 20 and count is 3.\n",
        &["10"],
    );
}

#[test]
fn leading_given_bindings() {
    assert_program(
        "given",
        "Given price is 100 and tax is 18, print the sum of price and tax.\n",
        &["118"],
    );
}

#[test]
fn sequential_then_chain() {
    assert_program(
        "then-chain",
        "Let x be 10, then add 5 to it, then print it.\n",
        &["15"],
    );
}

#[test]
fn sequential_and_then_chain() {
    assert_program(
        "and-then-chain",
        "Let a be 12 and b be 23, then print their sum.\n",
        &["35"],
    );
}

#[test]
fn plural_product_anaphora() {
    assert_program(
        "plural-product",
        "Let a be 12 and b be 23. Print their product.\n",
        &["276"],
    );
}

#[test]
fn plural_average_where() {
    assert_program(
        "plural-average",
        "Print their average, where a is 10 and b is 30.\n",
        &["20"],
    );
}

#[test]
fn the_result_anaphora() {
    assert_program(
        "the-result",
        "Let x be the sum of 12 and 23. Print the result.\n",
        &["35"],
    );
}

#[test]
fn the_result_prefers_user_variable() {
    assert_program(
        "result-shadow",
        "Let result be 7. Let x be the sum of 1 and 2. Print the result.\n",
        &["7"],
    );
}

#[test]
fn where_bindings_do_not_leak() {
    assert_program(
        "where-scope",
        "a is 5. Print the sum of a and b, where a is 1 and b is 2. Print a.\n",
        &["3", "5"],
    );
}

#[test]
fn compound_let_bindings() {
    assert_program(
        "compound-let",
        "Let a be 12 and b be 23. Print their sum.\n",
        &["35"],
    );
}

#[test]
fn compound_let_comma_bindings() {
    assert_program(
        "compound-let-comma",
        "Let a be 12, b be 23, and c be 34. Print their sum.\n",
        &["69"],
    );
}

#[test]
fn compound_set_bindings() {
    assert_program(
        "compound-set",
        "Set a to 12 and b to 23. Print their sum.\n",
        &["35"],
    );
}

#[test]
fn compound_put_bindings() {
    assert_program(
        "compound-put",
        "Put 12 in a and 23 in b. Print their sum.\n",
        &["35"],
    );
}

#[test]
fn compound_declarative_bindings() {
    assert_program(
        "compound-declarative",
        "a is 12 and b is 23. Print their sum.\n",
        &["35"],
    );
}

#[test]
fn plural_difference() {
    assert_program(
        "plural-difference",
        "Let a be 12 and b be 23. Print their difference.\n",
        &["-11"],
    );
}

#[test]
fn plural_quotient() {
    assert_program(
        "plural-quotient",
        "Let a be 12 and b be 23. Print their quotient.\n",
        &["0"],
    );
}

#[test]
fn plural_minimum_and_maximum() {
    assert_program(
        "plural-minmax",
        "Let a be 12 and b be 23. Print their minimum. Print their maximum.\n",
        &["12", "23"],
    );
}

#[test]
fn recursive_ratio_phrase() {
    assert_program("ratio", "Print the ratio of 100 to 4.\n", &["25"]);
}

#[test]
fn recursive_square_of() {
    assert_program("square-of", "Print the square of 7.\n", &["49"]);
}

#[test]
fn recursive_postfix_squared() {
    assert_program("postfix-squared", "Print 7 squared.\n", &["49"]);
}

#[test]
fn recursive_difference_between() {
    assert_program(
        "difference-between",
        "Print the difference between 10 and 3.\n",
        &["7"],
    );
}

#[test]
fn recursive_precedence_additive_multiplicative() {
    assert_program("precedence", "Print 2 plus 3 times 4.\n", &["14"]);
}

#[test]
fn recursive_np_plus_additive() {
    assert_program(
        "np-additive",
        "Print the product of 2 and 3 plus 4.\n",
        &["10"],
    );
}

#[test]
fn recursive_where_comma_operands() {
    assert_program(
        "where-comma",
        "Print the sum of a, b, and c, where a is 1, b is 2, and c is 3.\n",
        &["6"],
    );
}

#[test]
fn list_sum_phrase_preserved() {
    assert_program(
        "list-sum",
        concat!(
            "Use io and list.\n",
            "Let nums be List of whole number.\n",
            "Push 4 to address of nums.\n",
            "Push 9 to address of nums.\n",
            "Let total be sum of address of nums.\n",
            "Print total.\n",
            "Free list address of nums.\n",
        ),
        &["13"],
    );
}

#[test]
fn list_average_phrase_preserved() {
    assert_program(
        "list-average",
        concat!(
            "Use io and list.\n",
            "Let nums be List of whole number.\n",
            "Push 4 to address of nums.\n",
            "Push 9 to address of nums.\n",
            "Let average be average of address of nums.\n",
            "Print average.\n",
            "Free list address of nums.\n",
        ),
        &["6.5000"],
    );
}

#[test]
fn list_minimum_phrase_preserved() {
    assert_program(
        "list-minimum",
        concat!(
            "Use io and list.\n",
            "Let nums be List of whole number.\n",
            "Push 4 to address of nums.\n",
            "Push 9 to address of nums.\n",
            "Let smallest be minimum of address of nums.\n",
            "Print smallest.\n",
            "Free list address of nums.\n",
        ),
        &["4"],
    );
}

#[test]
fn list_maximum_phrase_preserved() {
    assert_program(
        "list-maximum",
        concat!(
            "Use io and list.\n",
            "Let nums be List of whole number.\n",
            "Push 4 to address of nums.\n",
            "Push 9 to address of nums.\n",
            "Let largest be maximum of address of nums.\n",
            "Print largest.\n",
            "Free list address of nums.\n",
        ),
        &["9"],
    );
}

#[test]
fn write_file_phrase_still_wins_over_emit() {
    assert_program(
        "file-phrase",
        concat!(
            "Use alloc, core, fs, io, and string.\n",
            "Let name be create text from \"emit_probe.txt\" of length 14.\n",
            "Save 7 to file address of name.\n",
            "Delete file address of name.\n",
            "Release text address of name.\n",
            "Print 1.\n",
        ),
        &["1"],
    );
}

#[test]
fn narrative_printed_is_comment_omitted() {
    assert_omitted("narrative-printed", "She printed the letter yesterday.\n");
}

#[test]
fn narrative_showed_is_comment_omitted() {
    assert_omitted("narrative-showed", "He showed his painting in class.\n");
}

#[test]
fn narrative_displayed_is_comment_omitted() {
    assert_omitted(
        "narrative-displayed",
        "They displayed the banner outside.\n",
    );
}

#[test]
fn narrative_output_is_comment_omitted() {
    assert_omitted("narrative-output", "They output the results yesterday.\n");
}

#[test]
fn emit_record_field() {
    assert_program(
        "emit-field",
        concat!(
            "A record Waypoint has distance (a whole number) and toll (a whole number).\n",
            "Create a Waypoint called pondy.\n",
            "Update pondy so its distance is 150 and its toll is 20.\n",
            "Print pondy's distance.\n",
        ),
        &["150"],
    );
}

#[test]
fn emit_its_field_anaphor() {
    assert_program(
        "emit-its",
        concat!(
            "A record Waypoint has distance (a whole number) and toll (a whole number).\n",
            "Create a Waypoint called pondy.\n",
            "Update pondy so its distance is 150 and its toll is 220.\n",
            "Show its toll.\n",
        ),
        &["220"],
    );
}

#[test]
fn emit_it_anaphor() {
    assert_program("emit-it", "Let x be 7. Display it.\n", &["7"]);
}

#[test]
fn emit_multiword_variable() {
    assert_program(
        "emit-multiword",
        "Let travel budget be 500. Print travel budget.\n",
        &["500"],
    );
}

#[test]
fn emit_assuming_keeps_main_declaration_visible() {
    assert_program(
        "assuming-visible",
        "Assuming width is 8 and height is 5, let area be the product of width and height. Print area.\n",
        &["40"],
    );
}

#[test]
fn emit_show_decimal_literal() {
    assert_program("show-decimal", "Show 3.14.\n", &["3.1400"]);
}

#[test]
fn emit_display_boolean_literal() {
    assert_program("display-bool", "Display false.\n", &["false"]);
}

#[test]
fn emit_output_boolean_literal() {
    assert_program("output-bool", "Output true.\n", &["true"]);
}

#[test]
fn compound_declarative_then() {
    assert_program(
        "compound-declarative-then",
        "a is 12 and b is 23, then print their sum.\n",
        &["35"],
    );
}

#[test]
fn plural_sum_inside_product() {
    assert_program(
        "plural-in-product",
        "Let a be 3 and b be 4. Print the product of their sum and 2.\n",
        &["14"],
    );
}

#[test]
fn emit_say_text_without_io_use() {
    assert_program(
        "say-text-auto",
        "Say \"auto prelude\".\n",
        &["auto prelude"],
    );
}

#[test]
fn recursive_nested_decimal_product() {
    assert_program(
        "decimal-nested",
        "Print the product of the sum of a and b and 2.0, where a is 1.5 and b is 2.5.\n",
        &["8.0000"],
    );
}
