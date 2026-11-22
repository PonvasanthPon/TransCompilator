//! End-to-end coverage for the expanded SemanticConcept/SemanticFrame vocabulary.

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

fn assert_ok(name: &str, source: &str) {
    let result = run_elf_with_parity(source, None, "");
    assert!(
        result.ok,
        "{name}: {}\n{}",
        result.summary,
        result.lines.join("\n")
    );
}

#[test]
fn increment_default() {
    assert_program(
        "increment-default",
        "Use io.\nLet x be 10.\nIncrement x.\nSay x.\nGive back 0.\n",
        &["11"],
    );
}

#[test]
fn increment_by() {
    assert_program(
        "increment-by",
        "Use io.\nLet x be 10.\nIncrement x by 5.\nSay x.\nGive back 0.\n",
        &["15"],
    );
}

#[test]
fn increment_multi_word() {
    assert_program(
        "increment-multi",
        "Use io.\nLet travel budget be 10.\nIncrement travel budget.\nSay travel budget.\nGive back 0.\n",
        &["11"],
    );
}

#[test]
fn increment_it() {
    assert_program(
        "increment-it",
        "Use io.\nLet x be 10.\nIncrement it.\nSay x.\nGive back 0.\n",
        &["11"],
    );
}

#[test]
fn increment_possessive_field() {
    assert_program(
        "increment-field",
        "Use io.\nA record Waypoint has distance (a whole number) and toll (a whole number).\nCreate a Waypoint called pondy.\nUpdate pondy so its distance is 10 and its toll is 2.\nIncrement pondy's distance.\nSay pondy's distance.\nGive back 0.\n",
        &["11"],
    );
}

#[test]
fn decrement_default() {
    assert_program(
        "decrement-default",
        "Use io.\nLet x be 10.\nDecrement x.\nSay x.\nGive back 0.\n",
        &["9"],
    );
}

#[test]
fn decrement_by() {
    assert_program(
        "decrement-by",
        "Use io.\nLet x be 10.\nDecrement x by 3.\nSay x.\nGive back 0.\n",
        &["7"],
    );
}

#[test]
fn decrement_multi_word() {
    assert_program(
        "decrement-multi",
        "Use io.\nLet travel budget be 10.\nDecrement travel budget by 20.\nSay travel budget.\nGive back 0.\n",
        &["-10"],
    );
}

#[test]
fn decrement_it() {
    assert_program(
        "decrement-it",
        "Use io.\nLet x be 10.\nDecrement it.\nSay x.\nGive back 0.\n",
        &["9"],
    );
}

#[test]
fn reset_default() {
    assert_program(
        "reset-default",
        "Use io.\nLet x be 10.\nReset x.\nSay x.\nGive back 0.\n",
        &["0"],
    );
}

#[test]
fn reset_to() {
    assert_program(
        "reset-to",
        "Use io.\nLet x be 10.\nReset x to 42.\nSay x.\nGive back 0.\n",
        &["42"],
    );
}

#[test]
fn clear_scalar() {
    assert_program(
        "clear-scalar",
        "Use io.\nLet x be 10.\nClear x.\nSay x.\nGive back 0.\n",
        &["0"],
    );
}

#[test]
fn reset_multi_word() {
    assert_program(
        "reset-multi",
        "Use io.\nLet travel budget be 10.\nReset travel budget.\nSay travel budget.\nGive back 0.\n",
        &["0"],
    );
}

#[test]
fn clear_bits_does_not_collide_with_reset() {
    assert_program(
        "clear-bits",
        "Use io.\nLet n be 255.\nClear bits 15 from n.\nSay n.\nGive back 0.\n",
        &["240"],
    );
}

#[test]
fn swap_scalars() {
    assert_program(
        "swap-scalars",
        "Use io.\nLet a be 10.\nLet b be 20.\nSwap a and b.\nSay a.\nSay b.\nGive back 0.\n",
        &["20", "10"],
    );
}

#[test]
fn exchange_scalars() {
    assert_program(
        "exchange-scalars",
        "Use io.\nLet a be 10.\nLet b be 20.\nExchange a and b.\nSay a.\nSay b.\nGive back 0.\n",
        &["20", "10"],
    );
}

#[test]
fn swap_multi_word() {
    assert_program(
        "swap-multi",
        "Use io.\nLet travel budget be 10.\nLet emergency fund be 20.\nSwap travel budget and emergency fund.\nSay travel budget.\nSay emergency fund.\nGive back 0.\n",
        &["20", "10"],
    );
}

#[test]
fn swap_struct_fields() {
    assert_program(
        "swap-fields",
        "Use io.\nA record Waypoint has distance (a whole number) and toll (a whole number).\nCreate a Waypoint called pondy.\nUpdate pondy so its distance is 10 and its toll is 20.\nSwap pondy's distance and pondy's toll.\nSay pondy's distance.\nSay pondy's toll.\nGive back 0.\n",
        &["20", "10"],
    );
}

#[test]
fn copy_scalar_into() {
    assert_program(
        "copy-scalar",
        "Use io.\nLet x be 10.\nCopy x into y.\nSay y.\nGive back 0.\n",
        &["10"],
    );
}

#[test]
fn duplicate_multi_word() {
    assert_program(
        "duplicate-multi",
        "Use io.\nLet travel budget be 10.\nDuplicate travel budget into backup.\nSay backup.\nGive back 0.\n",
        &["10"],
    );
}

#[test]
fn copy_bytes_does_not_collide_with_value_copy() {
    assert_ok(
        "copy-bytes",
        "Use core, io.\nLet src be block of 4 byte.\nSet item 0 of src to 65.\nLet dst be block of 4 byte.\nCopy 4 bytes from address of item 0 of src to address of item 0 of dst.\nSay bytes address of item 0 of dst of length 4 as an unsigned number.\nGive back 0.\n",
    );
}

#[test]
fn convert_to_decimal() {
    assert_ok(
        "convert-decimal",
        "Use io.\nLet x be 5.\nConvert x to a decimal.\nSay x.\nGive back 0.\n",
    );
}

#[test]
fn convert_to_whole_number() {
    assert_ok(
        "convert-whole",
        "Use io.\nLet d be 3.75.\nConvert d to a whole number.\nSay d.\nGive back 0.\n",
    );
}

#[test]
fn adjust_positive() {
    assert_program(
        "adjust-positive",
        "Use io.\nLet x be 10.\nAdjust x by 10.\nSay x.\nGive back 0.\n",
        &["20"],
    );
}

#[test]
fn adjust_negative() {
    assert_program(
        "adjust-negative",
        "Use io.\nLet x be 10.\nAdjust x by -4.\nSay x.\nGive back 0.\n",
        &["6"],
    );
}

#[test]
fn modify_to() {
    assert_program(
        "modify-to",
        "Use io.\nLet x be 10.\nModify x to 99.\nSay x.\nGive back 0.\n",
        &["99"],
    );
}

#[test]
fn alter_multi_word() {
    assert_program(
        "alter-multi",
        "Use io.\nLet travel budget be 10.\nAlter travel budget to 500.\nSay travel budget.\nGive back 0.\n",
        &["500"],
    );
}

#[test]
fn new_frames_inside_control_blocks() {
    assert_program(
        "new-frames-control",
        "Use io.\nLet total be 0.\nIf 1 is more than 0:\n    Increment total.\nOtherwise:\n    Decrement total.\nWhile total is less than 3:\n    Increment total.\nFor every i from 1 to 1:\n    Increment total.\nSay total.\nGive back 0.\n",
        &["4"],
    );
}

#[test]
fn new_frames_inside_until_and_functions() {
    assert_program(
        "new-frames-function",
        "Use io.\nTo bump named bump, giving a whole number.\nLet x be 10.\nIncrement x by 5.\nReset x to 20.\nGive back x.\n\nSay bump().\nGive back 0.\n",
        &["20"],
    );
}

#[test]
fn prose_safety_for_new_anchors() {
    let source = "Use io.\nShe cleared the table after dinner.\nHe swapped his old jacket yesterday.\nThey copied the painting in class.\nMy brother adjusted his glasses.\nGive back 0.\n";
    let result = run_elf_with_parity(source, None, "");
    assert!(result.ok, "{}", result.summary);
    for line in 2..=5 {
        assert!(
            result.omitted_lines.contains(&line),
            "line {line} must be COMMENT_OMITTED: {:?}",
            result.omitted_lines
        );
    }
    assert!(stdout_lines(&result).is_empty());
}
