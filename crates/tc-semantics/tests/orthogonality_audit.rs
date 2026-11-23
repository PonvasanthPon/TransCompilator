//! Live-test regressions and the Statement x Target x Scope orthogonality matrix.

use std::fmt::Write as _;

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

#[test]
fn live_test_1_verbatim() {
    let source = r#"Good morning, today shop was very busy because of rain. My brother came in evening and drank 2 teas. It was pouring outside.

Use io and math.

Put 1000 in cash, add 450 to it, add 320 to it, and remove 180 from it. 50 is added to cash. Say cash.

If cash is more than 1500, say "Good profit today", otherwise say "Slow day today".

Width is 3.0. Height is 4.0.
Let diagonal be square root of (width times width plus height times height).
Show diagonal.

Give back 0.
"#;
    assert_program(
        "live-test-1",
        source,
        &["1640", "Good profit today", "5.0000"],
    );
}

#[test]
fn live_test_2_verbatim() {
    let source = r#"Yesterday Ravi called me twice about our weekend road trip. It is 3 pm right now.

Use io.

A record Waypoint has distance (a whole number) and toll (a whole number).

Put 8000 in travel budget, remove 1800 from it, and remove 350 from it.
Put 0 in emergency fund.
Transfer 1000 from travel budget to emergency fund.
If travel budget is more than 4000 or emergency fund equals 0, and travel budget is less than 6000, say "Budget looks healthy", otherwise say "Watch spending".
Display travel budget.

Put 0 in bonus.
For every day from 1 to 5, add it to bonus.
Add bonus to travel budget.
Show travel budget.

Create a Waypoint called pondy.
Update pondy so its distance is 150 and its toll is 220.
Print the waypoint's distance plus its toll.

Give back 0.
"#;
    assert_program(
        "live-test-2",
        source,
        &["Budget looks healthy", "4850", "4865", "370"],
    );
}

#[derive(Clone, Copy, Debug)]
enum Statement {
    Let,
    Put,
    Set,
    Store,
    Keep,
    Make,
    Becomes,
    Add,
    Remove,
    Transfer,
    Update,
    Output,
    Increment,
    Decrement,
    Reset,
    Copy,
    Swap,
}

#[derive(Clone, Copy, Debug)]
enum Target {
    Scalar,
    MultiWord,
    It,
    ValueField,
    PointerField,
    ItsField,
    DefiniteField,
}

const TARGETS: [Target; 7] = [
    Target::Scalar,
    Target::MultiWord,
    Target::It,
    Target::ValueField,
    Target::PointerField,
    Target::ItsField,
    Target::DefiniteField,
];

#[derive(Clone, Copy, Debug)]
enum Scope {
    Top,
    Function,
    If,
    Else,
    While,
    Until,
    ForEvery,
}

const SCOPES: [Scope; 7] = [
    Scope::Top,
    Scope::Function,
    Scope::If,
    Scope::Else,
    Scope::While,
    Scope::Until,
    Scope::ForEvery,
];

struct TargetParts {
    type_setup: &'static str,
    instance_setup: &'static str,
    target: &'static str,
    read: &'static str,
}

fn target_parts(target: Target) -> TargetParts {
    const RECORD: &str =
        "A record Waypoint has distance (a whole number) and toll (a whole number).";
    const CREATE: &str =
        "Create a Waypoint called pondy.\nUpdate pondy so its distance is 10 and its toll is 2.";
    match target {
        Target::Scalar => TargetParts {
            type_setup: "",
            instance_setup: "Let x be 10.",
            target: "x",
            read: "x",
        },
        Target::MultiWord => TargetParts {
            type_setup: "",
            instance_setup: "Let travel budget be 10.",
            target: "travel budget",
            read: "travel budget",
        },
        Target::It => TargetParts {
            type_setup: "",
            instance_setup: "Let x be 10.",
            target: "it",
            read: "x",
        },
        Target::ValueField => TargetParts {
            type_setup: RECORD,
            instance_setup: CREATE,
            target: "pondy's distance",
            read: "pondy's distance",
        },
        Target::PointerField => TargetParts {
            type_setup: RECORD,
            instance_setup: "Create a Waypoint called pondy.\nUpdate pondy so its distance is 10 and its toll is 2.\nLet block be address of pondy.",
            target: "block's distance",
            read: "block's distance",
        },
        Target::ItsField => TargetParts {
            type_setup: RECORD,
            instance_setup: CREATE,
            target: "its distance",
            read: "pondy's distance",
        },
        Target::DefiniteField => TargetParts {
            type_setup: RECORD,
            instance_setup: CREATE,
            target: "the waypoint's toll",
            read: "pondy's toll",
        },
    }
}

fn statement_operation(statement: Statement, target: Target, target_text: &str) -> String {
    match statement {
        Statement::Let => format!("Let {target_text} be 20."),
        Statement::Put => format!("Put 20 in {target_text}."),
        Statement::Set => format!("Set {target_text} to 20."),
        Statement::Store => format!("Store 20 in {target_text}."),
        Statement::Keep => format!("Keep 20 in {target_text}."),
        Statement::Make => format!("Make {target_text} 20."),
        Statement::Becomes => format!("{target_text} becomes 20."),
        Statement::Add => format!("Add 20 to {target_text}."),
        Statement::Remove => format!("Remove 20 from {target_text}."),
        Statement::Transfer => format!("Transfer 5 from source to {target_text}."),
        Statement::Update => match target {
            Target::PointerField => {
                "Update block so its distance is 20 and its toll is 2.".to_string()
            }
            _ => "Update pondy so its distance is 20 and its toll is 2.".to_string(),
        },
        Statement::Output => format!("Say {target_text}."),
        Statement::Increment => format!("Increment {target_text}."),
        Statement::Decrement => format!("Decrement {target_text}."),
        Statement::Reset => format!("Reset {target_text}."),
        Statement::Copy => format!("Copy 5 into {target_text}."),
        Statement::Swap => format!("Swap {target_text} and other."),
    }
}

fn indent(text: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    text.lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn scoped_program(statement: Statement, target: Target, scope: Scope) -> String {
    let parts = target_parts(target);
    let operation = statement_operation(statement, target, parts.target);
    let it_seed = if matches!(target, Target::It) {
        "Put 10 in x."
    } else {
        ""
    };
    let mut body = String::new();
    if !parts.instance_setup.is_empty() {
        body.push_str(parts.instance_setup);
        body.push('\n');
    }
    body.push_str("Let source be 5.\nLet other be 5.\n");
    if !it_seed.is_empty() {
        body.push_str(it_seed);
        body.push('\n');
    }
    body.push_str(&operation);
    body.push('\n');
    let _ = writeln!(body, "Say {}.", parts.read);
    match scope {
        Scope::Top => format!(
            "Use io.\n{}{}\nGive back 0.\n",
            if parts.type_setup.is_empty() {
                String::new()
            } else {
                format!("{}\n", parts.type_setup)
            },
            body
        ),
        Scope::Function => format!(
            "Use io.\n{}\nTo exercise named exercise, giving a whole number.\n{}\n    Give back 0.\n\nexercise().\nGive back 0.\n",
            parts.type_setup,
            indent(&body, 4)
        ),
        Scope::If => {
            let mut scoped = String::new();
            if !it_seed.is_empty() {
                scoped.push_str(it_seed);
                scoped.push('\n');
            }
            scoped.push_str(&operation);
            scoped.push('\n');
            let _ = write!(scoped, "Say {}.", parts.read);
            format!(
                "Use io.\n{}\n{}\nIf 1 is more than 0:\n{}\nGive back 0.\n",
                parts.type_setup,
                parts.instance_setup,
                indent(&scoped, 4)
            )
        }
        Scope::Else => {
            let mut scoped = String::new();
            if !it_seed.is_empty() {
                scoped.push_str(it_seed);
                scoped.push('\n');
            }
            scoped.push_str(&operation);
            scoped.push('\n');
            let _ = write!(scoped, "Say {}.", parts.read);
            format!(
                "Use io.\n{}\n{}\nIf 1 is more than 2:\n    Say 0.\nOtherwise:\n{}\nGive back 0.\n",
                parts.type_setup,
                parts.instance_setup,
                indent(&scoped, 4)
            )
        }
        Scope::While => {
            let mut scoped = String::new();
            if !it_seed.is_empty() {
                scoped.push_str(it_seed);
                scoped.push('\n');
            }
            scoped.push_str(&operation);
            scoped.push('\n');
            let _ = writeln!(scoped, "Say {}.", parts.read);
            scoped.push_str("Add 1 to counter.");
            format!(
                "Use io.\n{}\n{}\nLet counter be 0.\nWhile counter is less than 1:\n{}\nGive back 0.\n",
                parts.type_setup,
                parts.instance_setup,
                indent(&scoped, 4)
            )
        }
        Scope::Until => {
            let inline_op = operation.trim_end_matches('.');
            let seed = if it_seed.is_empty() {
                ""
            } else {
                "Put 10 in x, "
            };
            format!(
                "Use io.\n{}\n{}\nLet source be 5.\nLet other be 5.\nLet counter be 0.\nUntil counter is at least 1, {}{}, Say {}, Add 1 to counter.\nGive back 0.\n",
                parts.type_setup, parts.instance_setup, seed, inline_op, parts.read
            )
        }
        Scope::ForEvery => {
            let mut scoped = String::new();
            if !it_seed.is_empty() {
                scoped.push_str(it_seed);
                scoped.push('\n');
            }
            scoped.push_str(&operation);
            scoped.push('\n');
            let _ = write!(scoped, "Say {}.", parts.read);
            format!(
                "Use io.\n{}\n{}\nFor every counter from 1 to 1:\n{}\nGive back 0.\n",
                parts.type_setup,
                parts.instance_setup,
                indent(&scoped, 4)
            )
        }
    }
}

fn run_statement_matrix(statement: Statement) {
    let mut passed = 0_usize;
    let mut not_applicable = 0_usize;
    for target in TARGETS {
        for scope in SCOPES {
            if matches!(statement, Statement::Update)
                && !matches!(target, Target::ValueField | Target::PointerField)
            {
                not_applicable += 1;
                continue;
            }
            let source = scoped_program(statement, target, scope);
            let label = format!("{statement:?} x {target:?} x {scope:?}");
            let result = run_elf_with_parity(&source, None, "");
            assert!(
                result.ok,
                "{label} failed: {}\n{source}\n{}",
                result.summary,
                result.lines.join("\n")
            );
            assert!(
                !stdout_lines(&result).is_empty(),
                "{label} produced no stdout"
            );
            passed += 1;
        }
    }
    if matches!(statement, Statement::Update) {
        assert_eq!(passed, 14, "{statement:?} valid matrix cells");
        assert_eq!(not_applicable, 35, "{statement:?} inapplicable cells");
    } else {
        assert_eq!(passed, 49, "{statement:?} valid matrix cells");
        assert_eq!(not_applicable, 0, "{statement:?} inapplicable cells");
    }
}

macro_rules! matrix_test {
    ($name:ident, $variant:ident) => {
        #[test]
        fn $name() {
            run_statement_matrix(Statement::$variant);
        }
    };
}

matrix_test!(matrix_let, Let);
matrix_test!(matrix_put, Put);
matrix_test!(matrix_set, Set);
matrix_test!(matrix_store, Store);
matrix_test!(matrix_keep, Keep);
matrix_test!(matrix_make, Make);
matrix_test!(matrix_becomes, Becomes);
matrix_test!(matrix_add, Add);
matrix_test!(matrix_remove, Remove);
matrix_test!(matrix_transfer, Transfer);
matrix_test!(matrix_update, Update);
matrix_test!(matrix_output, Output);
matrix_test!(matrix_increment, Increment);
matrix_test!(matrix_decrement, Decrement);
matrix_test!(matrix_reset, Reset);
matrix_test!(matrix_copy, Copy);
matrix_test!(matrix_swap, Swap);
