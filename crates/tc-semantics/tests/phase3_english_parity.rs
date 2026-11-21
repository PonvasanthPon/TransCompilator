//! Phase 3 end-to-end English parity: English source -> typed AST -> Core-7 IR
//! -> reference interpreter and static ELF64, with exact parity.

use std::io::Write as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use tc_codegen_x86::compile_to_elf;
use tc_ir::{interpret, verify_module};
use tc_semantics::{
    IntentVerdict, LinkGrammarParser, RawSentence, SourceFile, Span, Voice, analyze_clause,
    compile_english_to_ir, lower_project, segment_source,
};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_path(tag: &str, extension: &str) -> PathBuf {
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "tc_r1_phase3_{}_{}_{tag}.{extension}",
        std::process::id(),
        counter
    ))
}

#[derive(Debug)]
struct RunResult {
    exit_code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn run_elf(path: &Path, args: &[&str], stdin: &[u8]) -> RunResult {
    assert!(!args.is_empty(), "argv must include argv[0]");
    let mut child = Command::new(path)
        .arg0(args[0])
        .args(&args[1..])
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ELF");
    child
        .stdin
        .take()
        .expect("stdin pipe")
        .write_all(stdin)
        .expect("write stdin");
    let output = child.wait_with_output().expect("wait ELF");
    RunResult {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: output.stdout,
        stderr: output.stderr,
    }
}

fn assert_english(name: &str, text: &str, expected_exit: i32, expected_stdout: &[u8]) {
    let sources = [SourceFile::new("main.tc", text)];
    let compiled = compile_english_to_ir(&sources, "main")
        .unwrap_or_else(|errors| panic!("{name}: English compilation failed: {errors:#?}"));
    verify_module(&compiled.module)
        .unwrap_or_else(|errors| panic!("{name}: IR verification failed: {errors:#?}"));

    let args = ["prog".to_string()];
    let interpreted = interpret(&compiled.module, &args, &[], 50_000_000)
        .unwrap_or_else(|error| panic!("{name}: interpreter failed: {error:?}"));
    assert_eq!(
        interpreted.exit_code,
        expected_exit,
        "{name}: interpreter exit; stdout={:?}",
        String::from_utf8_lossy(&interpreted.stdout)
    );
    assert_eq!(
        interpreted.stdout, expected_stdout,
        "{name}: interpreter stdout"
    );

    let path = unique_path(name, "elf");
    compile_to_elf(&compiled.module, &path)
        .unwrap_or_else(|error| panic!("{name}: ELF compilation failed: {error}"));
    let elf = run_elf(&path, &["prog"], &[]);
    let _ = std::fs::remove_file(&path);

    assert_eq!(
        elf.exit_code,
        interpreted.exit_code,
        "{name}: ELF/interpreter exit mismatch; stderr={:?}",
        String::from_utf8_lossy(&elf.stderr)
    );
    assert_eq!(
        elf.stdout, interpreted.stdout,
        "{name}: ELF/interpreter stdout mismatch"
    );
    assert_eq!(elf.stderr, interpreted.stderr, "{name}: stderr mismatch");
    assert_eq!(elf.exit_code, expected_exit, "{name}: ELF exit");
    assert_eq!(elf.stdout, expected_stdout, "{name}: ELF stdout");
}

fn sentence(text: &str) -> RawSentence {
    RawSentence {
        text: text.to_string(),
        raw: text.to_string(),
        span: Span::new(1, 1),
        fillers: Vec::new(),
    }
}

fn clause(text: &str) -> tc_semantics::ClauseTree {
    let parser = LinkGrammarParser::new().expect("Link Grammar parser");
    analyze_clause(&parser, &sentence(text))
}

fn trace_of<'a>(
    project: &'a tc_semantics::LoweredProject,
    text: &str,
) -> &'a tc_semantics::SentenceTrace {
    project
        .trace
        .iter()
        .find(|entry| entry.text == text)
        .unwrap_or_else(|| panic!("missing trace for `{text}`: {:#?}", project.trace))
}

// ---------------------------------------------------------------------------
// Link Grammar and clause normalization
// ---------------------------------------------------------------------------

#[test]
fn lg_parses_active_english() {
    let parsed = clause("Remove 180 from cash");
    assert!(
        parsed.lg_used,
        "Link Grammar must parse the active sentence"
    );
    assert!(parsed.lg_links > 0);
    assert_eq!(parsed.voice, Voice::Imperative);
    assert_eq!(parsed.verb.as_ref().expect("verb").lemma, "remove");
}

#[test]
fn lg_parses_passive_english() {
    let parsed = clause("180 is removed from cash");
    assert!(parsed.lg_used);
    assert!(parsed.lg_links > 0);
    assert_eq!(parsed.voice, Voice::Passive);
    assert_eq!(parsed.verb.as_ref().expect("verb").lemma, "remove");
    assert!(
        parsed
            .subject
            .as_ref()
            .expect("passive subject")
            .iter()
            .any(|token| token.text == "180")
    );
}

#[test]
fn lg_parses_imperative_english() {
    let parsed = clause("Add 5 to x");
    assert!(parsed.lg_used);
    assert_eq!(parsed.voice, Voice::Imperative);
    assert_eq!(parsed.verb.as_ref().expect("verb").lemma, "add");
}

#[test]
fn lg_parses_prose_english() {
    let parsed = clause("I had 2 cups of coffee");
    assert!(parsed.lg_used, "narrative prose must still be parsed");
    assert!(parsed.lg_links > 0);
    assert_eq!(parsed.voice, Voice::Active);
}

#[test]
fn clause_walker_extracts_prepositional_roles() {
    let parsed = clause("Remove 180 from cash");
    assert!(
        parsed
            .prep_phrases
            .iter()
            .any(|phrase| phrase.prep == "from"
                && phrase.tokens.iter().any(|token| token.text == "cash"))
    );
    let parsed = clause("Add 5 to x");
    assert!(
        parsed.prep_phrases.iter().any(
            |phrase| phrase.prep == "to" && phrase.tokens.iter().any(|token| token.text == "x")
        )
    );
}

#[test]
fn segmenter_builds_blocks_and_preserves_literals() {
    let source = SourceFile::new(
        "main.tc",
        "If x is 1:\n    Add 2 to x.\nElse:\n    Remove 1 from x.\nSet y to 12.5.\nWrite \"a. b\".\n",
    );
    let document = segment_source(&source);
    assert!(
        document
            .items
            .iter()
            .any(|item| matches!(item, tc_semantics::RawItem::Block(_)))
    );
    let debug = format!("{document:?}");
    assert!(debug.contains("12.5"));
    assert!(debug.contains("a. b"));
}

// ---------------------------------------------------------------------------
// Lexicon-driven assignments and transfers
// ---------------------------------------------------------------------------

#[test]
fn put_assignment() {
    assert_english("put", "Put 10 in x.\nAdd 5 to x.\nReturn x.\n", 15, b"");
}

#[test]
fn set_assignment() {
    assert_english("set", "Set x to 10.\nAdd 5 to x.\nReturn x.\n", 15, b"");
}

#[test]
fn let_assignment() {
    assert_english("let", "Let x be 10.\nReturn x.\n", 10, b"");
}

#[test]
fn keep_assignment() {
    assert_english("keep", "Keep 10 in x.\nReturn x.\n", 10, b"");
}

#[test]
fn copula_assignment_forms() {
    assert_english(
        "copula",
        "a is 1.\nb = 2.\nc becomes 3.\nd equals 4.\nReturn a + b + c + d.\n",
        10,
        b"",
    );
}

#[test]
fn case_insensitive_identifiers_preserve_string_literals() {
    assert_english(
        "case",
        "Tea is 100.\nAdd tea to tea.\nReturn TEA.\n",
        200,
        b"",
    );
}

#[test]
fn bare_i_is_statement_executes() {
    assert_english("bare-i", "i is 1.\nReturn i.\n", 1, b"");
}

#[test]
fn subtractive_remove() {
    assert_english(
        "remove",
        "cash is 100.\nRemove 30 from cash.\nReturn cash.\n",
        70,
        b"",
    );
}

#[test]
fn subtractive_deduct_withdraw_subtract() {
    assert_english(
        "sub-verbs",
        "a is 100.\nb is 100.\nc is 100.\nDeduct 30 from a.\nWithdraw 40 from b.\nSubtract 20 from c.\nReturn a + b + c.\n",
        210,
        b"",
    );
}

#[test]
fn subtractive_passive_voice() {
    assert_english(
        "passive-sub",
        "cash is 100.\n30 is removed from cash.\nReturn cash.\n",
        70,
        b"",
    );
}

#[test]
fn subtractive_spend_and_take() {
    assert_english(
        "spend-take",
        "wallet is 100.\nx is 100.\nSpend 25 from wallet.\nTake 30 from x.\nReturn wallet + x.\n",
        145,
        b"",
    );
}

#[test]
fn transfer_between_variables() {
    assert_english(
        "transfer",
        "bank is 300.\nwallet is 0.\nTransfer 200 from bank to wallet.\nReturn wallet.\n",
        200,
        b"",
    );
}

#[test]
fn move_between_variables() {
    assert_english(
        "move",
        "a is 50.\nb is 10.\nMove 20 from a to b.\nReturn b.\n",
        30,
        b"",
    );
}

#[test]
fn additive_active_verbs() {
    assert_english("add-active", "x is 10.\nAdd 5 to x.\nReturn x.\n", 15, b"");
}

#[test]
fn additive_passive_voice() {
    assert_english(
        "add-passive",
        "x is 10.\n5 is added to x.\nReturn x.\n",
        15,
        b"",
    );
}

#[test]
fn append_is_additive() {
    assert_english("append", "x is 1.\nAppend 2 to x.\nReturn x.\n", 3, b"");
}

#[test]
fn increase_and_decrease() {
    assert_english(
        "increase",
        "x is 10.\nIncrease x by 5.\nDecrease x by 3.\nReturn x.\n",
        12,
        b"",
    );
}

#[test]
fn compound_assignment_operators() {
    assert_english(
        "compound",
        "x is 10.\nx += 5.\nx -= 3.\nx *= 2.\nx /= 4.\nReturn x.\n",
        6,
        b"",
    );
}

#[test]
fn double_halve_negate() {
    assert_english(
        "scale",
        "x is 3.\nDouble x.\nHalve x.\nNegate x.\nReturn x + 8.\n",
        5,
        b"",
    );
}

#[test]
fn multiply_and_divide() {
    assert_english(
        "multiply",
        "x is 100.\nMultiply x by 3.\nDivide x by 4.\nReturn x.\n",
        75,
        b"",
    );
}

// ---------------------------------------------------------------------------
// Expressions, where scoping, and the LIM-001 regression
// ---------------------------------------------------------------------------

#[test]
fn arithmetic_word_forms() {
    assert_english(
        "words",
        "a is 10.\nb is 4.\nReturn sum of a and b + difference between 20 and 5 + product of 2 and 3.\n",
        35,
        b"",
    );
}

#[test]
fn percent_half_twice_square() {
    assert_english(
        "percent",
        "Return half of 10 + twice 6 + square of 5 + 10 percent of 250.\n",
        67,
        b"",
    );
}

#[test]
fn where_bindings_are_scoped_to_the_containing_statement() {
    assert_english(
        "where",
        "x is 0.\nIf x is 0:\n    Add 10 to x where x is 5.\nReturn x.\n",
        0,
        b"",
    );
}

#[test]
fn where_bindings_feed_a_return_expression() {
    assert_english(
        "where-return",
        "Return sum of a and b where a is 3 and b is 4.\n",
        7,
        b"",
    );
}

#[test]
fn pointer_store_statement() {
    assert_english(
        "store",
        "x is 0.\nDefine local ptr as ptr<i64>.\nSet ptr to address of x.\nStore 42 at ptr.\nReturn x.\n",
        42,
        b"",
    );
}

#[test]
fn size_align_and_cast() {
    assert_english(
        "layout",
        "Return (size of i64) as i64 + (align of i64) as i64 + (3.9 as i64).\n",
        19,
        b"",
    );
}

#[test]
fn bitwise_word_operators() {
    assert_english(
        "bitwise",
        "Return (6 bitand 3) + (1 bitor 2) + (7 bitxor 3) + (1 shl 3) + (16 shr 2).\n",
        21,
        b"",
    );
}

// ---------------------------------------------------------------------------
// Prose omission and the strict three-way intent gate
// ---------------------------------------------------------------------------

#[test]
fn flowing_prose_is_omitted_and_a_directive_executes() {
    assert_english(
        "prose-mixed",
        "Yesterday Ravi called me twice. I had 2 cups of coffee. My uncle said petrol is very costly. Today the weather is nice. He is 30 years old. I am 20. Return 7.\n",
        7,
        b"",
    );
}

#[test]
fn narrative_sentences_are_comment_omitted() {
    let text = "Yesterday Ravi called me twice. I had 2 cups of coffee. My uncle said petrol is very costly. Today the weather is nice. He is 30 years old. I am 20.\n";
    let project = lower_project(&[SourceFile::new("main.tc", text)], "main").expect("lower");
    for entry in &project.trace {
        assert!(
            entry.intent.is_comment(),
            "`{}` must be COMMENT_OMITTED, got {:?}",
            entry.text,
            entry.intent
        );
    }
}

#[test]
fn bare_i_is_is_exec_but_i_am_is_comment() {
    let project = lower_project(
        &[SourceFile::new(
            "main.tc",
            "i is 1.\nI am 20.\nI had 2 cups of coffee.\nI have 3 books.\nI waited 2 hours.\n",
        )],
        "main",
    )
    .expect("lower");
    assert!(trace_of(&project, "i is 1").intent.is_exec());
    for text in [
        "I am 20",
        "I had 2 cups of coffee",
        "I have 3 books",
        "I waited 2 hours",
    ] {
        assert!(
            trace_of(&project, text).intent.is_comment(),
            "`{text}` must be omitted"
        );
    }
}

#[test]
fn out_of_scope_thread_is_unsupported() {
    let project = lower_project(
        &[SourceFile::new(
            "main.tc",
            "Start a new thread.\nReturn 0.\n",
        )],
        "main",
    )
    .expect("lower");
    assert!(
        trace_of(&project, "Start a new thread")
            .intent
            .is_unsupported()
    );
    let errors = compile_english_to_ir(
        &[SourceFile::new(
            "main.tc",
            "Start a new thread.\nReturn 0.\n",
        )],
        "main",
    )
    .expect_err("out-of-scope directive must fail loudly");
    assert!(
        errors
            .iter()
            .any(|error| error.kind == tc_semantics::SemanticErrorKind::Unsupported)
    );
}

#[test]
fn out_of_scope_window_is_unsupported() {
    let project = lower_project(
        &[SourceFile::new("main.tc", "Draw a window.\nReturn 0.\n")],
        "main",
    )
    .expect("lower");
    assert!(trace_of(&project, "Draw a window").intent.is_unsupported());
}

#[test]
fn out_of_scope_regex_is_unsupported() {
    let project = lower_project(
        &[SourceFile::new(
            "main.tc",
            "Match with a regular expression.\nReturn 0.\n",
        )],
        "main",
    )
    .expect("lower");
    assert!(
        trace_of(&project, "Match with a regular expression")
            .intent
            .is_unsupported()
    );
}

#[test]
fn unsupported_is_never_silently_omitted() {
    let project = lower_project(
        &[SourceFile::new(
            "main.tc",
            "Start a new thread. Draw a window. Match with a regular expression. Return 0.\n",
        )],
        "main",
    )
    .expect("lower");
    let unsupported = project
        .trace
        .iter()
        .filter(|entry| entry.intent.is_unsupported())
        .count();
    assert_eq!(unsupported, 3);
    assert_eq!(
        project.trace.len(),
        4,
        "every sentence must receive a trace verdict"
    );
}

#[test]
fn prose_trace_still_carries_link_grammar_and_lexicon_provenance() {
    let project = lower_project(
        &[SourceFile::new(
            "main.tc",
            "My uncle said petrol is very costly.\nReturn 0.\n",
        )],
        "main",
    )
    .expect("lower");
    let entry = trace_of(&project, "My uncle said petrol is very costly");
    assert!(entry.intent.is_comment());
    assert!(entry.lg_used);
    assert!(entry.lg_links > 0);
    assert_eq!(entry.verb.as_deref(), Some("said"));
}

// ---------------------------------------------------------------------------
// Control flow and unified statement lowering
// ---------------------------------------------------------------------------

#[test]
fn inline_if_otherwise() {
    assert_english(
        "inline-if",
        "x is 3.\nIf x more than 5, Add 10 to x otherwise Remove 1 from x.\nReturn x.\n",
        2,
        b"",
    );
}

#[test]
fn indented_if_else_blocks() {
    assert_english(
        "if-else",
        "x is 3.\nIf x more than 5:\n    Add 10 to x.\nElse:\n    Remove 1 from x.\nReturn x.\n",
        2,
        b"",
    );
}

#[test]
fn otherwise_block() {
    assert_english(
        "otherwise",
        "x is 7.\nIf x more than 5:\n    Add 1 to x.\nOtherwise:\n    Remove 1 from x.\nReturn x.\n",
        8,
        b"",
    );
}

#[test]
fn unless_statement() {
    assert_english(
        "unless",
        "x is 3.\nUnless x more than 5, Add 1 to x.\nReturn x.\n",
        4,
        b"",
    );
}

#[test]
fn while_block() {
    assert_english(
        "while",
        "x is 0.\nWhile x less than 3:\n    Add 1 to x.\nReturn x.\n",
        3,
        b"",
    );
}

#[test]
fn repeat_single_line_compound_actions() {
    assert_english(
        "repeat-line",
        "savings is 0.\nRepeat 3 times, add 2500 to savings and remove 1200 from savings.\nReturn savings / 100.\n",
        39,
        b"",
    );
}

#[test]
fn repeat_indented_block() {
    assert_english(
        "repeat-block",
        "x is 0.\nRepeat 4 times:\n    Add 2 to x.\nReturn x.\n",
        8,
        b"",
    );
}

#[test]
fn loop_break_and_continue() {
    assert_english(
        "loop-control",
        "x is 0.\nLoop:\n    Add 1 to x.\n    If x less than 3, Continue.\n    Break.\nReturn x.\n",
        3,
        b"",
    );
}

#[test]
fn nested_if_inside_repeat() {
    assert_english(
        "nested",
        "i is 0.\nsum is 0.\nRepeat 3 times:\n    If i is less than 2:\n        Add 10 to sum.\n    Else:\n        Add 20 to sum.\n    i += 1.\nReturn sum.\n",
        40,
        b"",
    );
}

#[test]
fn between_and_dropped_is_conditions() {
    assert_english(
        "conditions",
        "x is 5.\nIf x is between 1 and 10, Add 2 to x.\nIf x more than 6:\n    Add 3 to x.\nReturn x.\n",
        10,
        b"",
    );
}

#[test]
fn numbered_steps_fillers_and_missing_periods() {
    assert_english(
        "steps",
        "First, x is 1\nAfter that, Add 2 to x\nFinally, Return x\n",
        3,
        b"",
    );
}

#[test]
fn semicolons_and_explicit_end_closer() {
    assert_english("semicolon", "x is 1; Add 2 to x; Return x. End.\n", 3, b"");
}

// ---------------------------------------------------------------------------
// Core-7 systems English: aggregates, generics, recursion, modules, phrases
// ---------------------------------------------------------------------------

#[test]
fn struct_fields() {
    assert_english(
        "struct",
        "Define struct Point with x as i64 and y as i64.\np is Point.\nSet p.x to 4.\nSet p.y to 5.\nReturn p.x + p.y.\n",
        9,
        b"",
    );
}

#[test]
fn nested_struct_fields() {
    assert_english(
        "nested-struct",
        "Define struct Point with x as i64 and y as i64.\nDefine struct Rect with origin as Point and w as i64 and h as i64.\nrect is Rect.\nSet rect.origin.x to 2.\nSet rect.origin.y to 3.\nSet rect.w to 4.\nSet rect.h to 5.\nReturn rect.origin.x + rect.origin.y + rect.w + rect.h.\n",
        14,
        b"",
    );
}

#[test]
fn union_f64_u64_bit_pun() {
    assert_english(
        "union",
        "Define union Bits with f as f64 and u as u64.\nbits is Bits.\nSet bits.f to 1.5.\nReturn ((bits.u shr 52) as i64) bitand 255.\n",
        255,
        b"",
    );
}

#[test]
fn possessive_and_of_field_syntax() {
    assert_english(
        "fields",
        "Define struct Point with x as i64 and y as i64.\np is Point.\nSet p's x to 4.\nSet y of p to 5.\nReturn x of p + p's y.\n",
        9,
        b"",
    );
}

#[test]
fn generic_struct_pair() {
    assert_english(
        "generic-struct",
        "Define struct Pair of T and U with first as T and second as U.\npair is Pair<i64,i64>.\nSet pair.first to 7.\nSet pair.second to 8.\nReturn pair.first + pair.second.\n",
        15,
        b"",
    );
}

#[test]
fn generic_function_swap() {
    assert_english(
        "generic-fn",
        "Define function swap of T taking a as ptr<T> and b as ptr<T> returning void:\n    temp is a.\n    a = b.\n    b = temp.\nx is 10.\ny is 20.\nswap(address of x, address of y).\nReturn x + y.\n",
        30,
        b"",
    );
}

#[test]
fn recursive_factorial() {
    assert_english(
        "factorial",
        "Define function factorial taking n as i64 returning i64:\n    If n is at most 1:\n        Return 1.\n    Return n * factorial(n - 1).\nReturn factorial(5).\n",
        120,
        b"",
    );
}

#[test]
fn mutual_recursion() {
    assert_english(
        "mutual",
        "Define function even taking n as i64 returning i64:\n    If n is 0:\n        Return 1.\n    Return odd(n - 1).\nDefine function odd taking n as i64 returning i64:\n    If n is 0:\n        Return 0.\n    Return even(n - 1).\nReturn even(10).\n",
        1,
        b"",
    );
}

#[test]
fn global_assignment() {
    assert_english(
        "global",
        "Define global cash as i64.\ncash is 100.\nRemove 80 from cash.\nReturn cash.\n",
        20,
        b"",
    );
}

#[test]
fn global_array_elements() {
    assert_english(
        "array",
        "Define global buf as array<i64, 3>.\nSet buf[0] to 5.\nSet buf[1] to 6.\nSet buf[2] to 7.\nReturn buf[0] + buf[1] + buf[2].\n",
        18,
        b"",
    );
}

#[test]
fn function_pointer_indirect_call() {
    assert_english(
        "fnptr",
        "Define function add taking a as i64 and b as i64 returning i64:\n    Return a + b.\nfp is function add.\nReturn fp(3, 4).\n",
        7,
        b"",
    );
}

#[test]
fn explicit_main_function() {
    assert_english(
        "explicit-main",
        "Define function main returning i64:\n    Return 42.\n",
        42,
        b"",
    );
}

fn assert_multi_file(
    name: &str,
    files: &[(&str, &str)],
    expected_exit: i32,
    expected_stdout: &[u8],
) {
    let sources = files
        .iter()
        .map(|(file, text)| SourceFile::new(*file, *text))
        .collect::<Vec<_>>();
    let compiled = compile_english_to_ir(&sources, "main")
        .unwrap_or_else(|errors| panic!("{name}: compilation failed: {errors:#?}"));
    verify_module(&compiled.module).expect("IR verification");
    let args = ["prog".to_string()];
    let interpreted = interpret(&compiled.module, &args, &[], 50_000_000).expect("interpreter");
    assert_eq!(interpreted.exit_code, expected_exit, "{name}: exit");
    assert_eq!(interpreted.stdout, expected_stdout, "{name}: stdout");
    let path = unique_path(name, "elf");
    compile_to_elf(&compiled.module, &path).expect("ELF");
    let elf = run_elf(&path, &["prog"], &[]);
    let _ = std::fs::remove_file(&path);
    assert_eq!(elf.exit_code, interpreted.exit_code);
    assert_eq!(elf.stdout, interpreted.stdout);
    assert_eq!(elf.stderr, interpreted.stderr);
}

#[test]
fn multi_file_phrase_dispatch_writes_stdout() {
    assert_multi_file(
        "phrase-bytes",
        &[
            (
                "console.tc",
                "Public function write_bytes taking p as ptr<u8> and n as u64 returning i64:\n    Phrase: \"emit <p> with length <n>\"\n    Return syscall(1, 1, p, n).\n",
            ),
            (
                "main.tc",
                "Import console.\nEmit bytes_ptr(\"hello from english!\\n\") with length bytes_len(\"hello from english!\\n\").\nReturn 0.\n",
            ),
        ],
        0,
        b"hello from english!\n",
    );
}

#[test]
fn phrase_dispatch_with_integer_argument() {
    assert_multi_file(
        "phrase-int",
        &[
            (
                "math.tc",
                "Public function twice taking x as i64 returning i64:\n    Phrase: \"double value <x>\"\n    Return x * 2.\n",
            ),
            ("main.tc", "Import math.\nReturn double value 21.\n"),
        ],
        42,
        b"",
    );
}

#[test]
fn imported_function_and_global_are_visible() {
    assert_multi_file(
        "modules",
        &[
            (
                "lib.tc",
                "Public global seed as i64.\nPublic function add_seed taking x as i64 returning i64:\n    Return x + seed.\n",
            ),
            (
                "main.tc",
                "Import lib.\nlib.seed is 5.\nReturn lib.add_seed(7).\n",
            ),
        ],
        12,
        b"",
    );
}

// ---------------------------------------------------------------------------
// CLI integration and zero-libc audit
// ---------------------------------------------------------------------------

fn tc_binary() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/debug/tc")
        .canonicalize()
        .expect("target/debug/tc must exist; run `make build` before the Phase 3 tests")
}

fn cli_command() -> Command {
    let mut command = Command::new(tc_binary());
    command.env_remove("LD_LIBRARY_PATH");
    command
}

fn temp_source(tag: &str, text: &str) -> PathBuf {
    let path = unique_path(tag, "tc");
    std::fs::write(&path, text).expect("write temp source");
    path
}

#[test]
fn cli_build_run_and_interp_agree() {
    let source = temp_source("cli", "cash is 100.\nRemove 30 from cash.\nReturn cash.\n");
    let elf = unique_path("cli-build", "elf");
    let build = cli_command()
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(&elf)
        .output()
        .expect("tc build");
    assert!(
        build.status.success(),
        "tc build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let compiled = run_elf(&elf, &["prog"], &[]);
    let run = cli_command()
        .arg("run")
        .arg(&source)
        .output()
        .expect("tc run");
    let interp = cli_command()
        .arg("interp")
        .arg(&source)
        .output()
        .expect("tc interp");
    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&elf);
    assert_eq!(compiled.exit_code, 70);
    assert_eq!(run.status.code(), Some(70));
    assert_eq!(interp.status.code(), Some(70));
    assert_eq!(run.stdout, compiled.stdout);
    assert_eq!(interp.stdout, compiled.stdout);
}

#[test]
fn cli_run_reports_stdout_and_exit_code() {
    let source = temp_source(
        "cli-out",
        "Emit bytes_ptr(\"cli says hi\\n\") with length bytes_len(\"cli says hi\\n\").\nReturn 9.\n",
    );
    // This program needs a phrase; define it in an importable module.
    let console = source.with_file_name("console.tc");
    std::fs::write(
        &console,
        "Public function write_bytes taking p as ptr<u8> and n as u64 returning i64:\n    Phrase: \"emit <p> with length <n>\"\n    Return syscall(1, 1, p, n).\n",
    )
    .expect("write console");
    let text = format!(
        "Import console.\n{}",
        std::fs::read_to_string(&source).expect("read source")
    );
    std::fs::write(&source, text).expect("rewrite source");
    let output = cli_command()
        .arg("run")
        .arg(&source)
        .arg("-I")
        .arg(source.parent().expect("parent"))
        .output()
        .expect("tc run");
    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&console);
    assert_eq!(output.status.code(), Some(9));
    assert_eq!(output.stdout, b"cli says hi\n");
}

#[test]
fn cli_trace_reports_intent_provenance_and_ir() {
    let source = temp_source(
        "cli-trace",
        "cash is 100.\nI had 2 cups of coffee.\nRemove 30 from cash.\nReturn cash.\n",
    );
    let output = cli_command()
        .arg("trace")
        .arg(&source)
        .output()
        .expect("tc trace");
    let _ = std::fs::remove_file(&source);
    assert!(output.status.success(), "tc trace failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("EXEC"), "{stdout}");
    assert!(stdout.contains("COMMENT_OMITTED"), "{stdout}");
    assert!(stdout.contains("predicate=TransferSub"), "{stdout}");
    assert!(stdout.contains("lg=true"), "{stdout}");
    assert!(stdout.contains("--- TC-IR ---"), "{stdout}");
}

#[test]
fn cli_trace_is_loud_for_unsupported_directives() {
    let source = temp_source("cli-unsupported", "Start a new thread.\nReturn 0.\n");
    let output = cli_command()
        .arg("trace")
        .arg(&source)
        .output()
        .expect("tc trace");
    let _ = std::fs::remove_file(&source);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!output.status.success(), "unsupported trace must fail");
    assert!(stdout.contains("UNSUPPORTED"), "{stdout}");
}

#[test]
fn compiled_english_is_static_and_zero_libc() {
    let source = temp_source("static", "Return 0.\n");
    let elf = unique_path("static-build", "elf");
    let build = cli_command()
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(&elf)
        .output()
        .expect("tc build");
    assert!(build.status.success());
    let dynamic = Command::new("readelf")
        .arg("-d")
        .arg(&elf)
        .output()
        .expect("readelf");
    let dynamic_text = String::from_utf8_lossy(&dynamic.stdout);
    assert!(
        !dynamic_text.contains("NEEDED"),
        "ELF must not need shared libraries: {dynamic_text}"
    );
    let ldd = Command::new("ldd").arg(&elf).output().expect("ldd");
    let ldd_text = format!(
        "{}{}",
        String::from_utf8_lossy(&ldd.stdout),
        String::from_utf8_lossy(&ldd.stderr)
    );
    assert!(
        ldd_text.contains("not a dynamic executable") || ldd_text.contains("statically linked"),
        "unexpected ldd output: {ldd_text}"
    );
    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&elf);
}

#[test]
fn split_verdicts_are_all_reachable() {
    let project = lower_project(
        &[SourceFile::new(
            "main.tc",
            "Return 0. I had coffee. Start a new thread.\n",
        )],
        "main",
    )
    .expect("lower");
    assert!(
        project
            .trace
            .iter()
            .any(|entry| matches!(entry.intent, IntentVerdict::Exec))
    );
    assert!(project.trace.iter().any(|entry| entry.intent.is_comment()));
    assert!(
        project
            .trace
            .iter()
            .any(|entry| entry.intent.is_unsupported())
    );
}
