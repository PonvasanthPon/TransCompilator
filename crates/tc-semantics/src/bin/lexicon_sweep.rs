//! Exhaustive live-pipeline sweep of the generated WordNet and VerbNet tables.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

use tc_lexicon::{
    IRREGULAR_LEMMAS, Pos, PrepRole, VERBNET_FRAMES, VerbPredicate, WORDNET_ENTRIES, classify_verb,
};
use tc_semantics::intent::{self, IntentVerdict};
use tc_semantics::{
    LinkGrammarParser, RawSentence, SourceFile, Span, analyze_clause, lower_project_with_parser,
};

fn sentence(text: &str) -> RawSentence {
    RawSentence {
        text: text.to_string(),
        raw: text.to_string(),
        span: Span::new(1, 1),
        fillers: Vec::new(),
    }
}

fn classify(parser: &LinkGrammarParser, text: &str) -> IntentVerdict {
    let clause = analyze_clause(parser, &sentence(text));
    intent::classify(text, &clause, None)
}

fn past_form(lemma: &str) -> String {
    IRREGULAR_LEMMAS
        .iter()
        .find(|(_, base)| *base == lemma)
        .map_or_else(|| format!("{lemma}ed"), |(form, _)| (*form).to_string())
}

fn default_prep(predicate: Option<VerbPredicate>, role: PrepRole) -> &'static str {
    match predicate {
        Some(VerbPredicate::TransferSub) => "from",
        Some(VerbPredicate::TransferMove) => "from",
        Some(VerbPredicate::TransferAdd) => "to",
        Some(VerbPredicate::Assign) => "to",
        Some(VerbPredicate::Combine) => "into",
        Some(VerbPredicate::Transform) => "by",
        _ => match role {
            PrepRole::Source => "from",
            PrepRole::Destination => "to",
            PrepRole::Extent => "by",
            PrepRole::Instrument => "with",
            PrepRole::Recipient => "for",
            PrepRole::Location => "at",
            PrepRole::Theme => "of",
        },
    }
}

#[derive(Default)]
struct WordNetResult {
    total: usize,
    verbs: usize,
    exec: usize,
    comment_ok: usize,
    unsupported: usize,
    silent_drop: usize,
    panic: usize,
    samples: Vec<String>,
}

#[derive(Default)]
struct VerbNetResult {
    total: usize,
    unique: usize,
    exec: usize,
    unsupported: usize,
    comment: usize,
    type_error: usize,
    target_clobber: usize,
    panic: usize,
    samples: Vec<String>,
}

fn sweep_wordnet(parser: &LinkGrammarParser) -> WordNetResult {
    let mut result = WordNetResult::default();
    for entry in WORDNET_ENTRIES {
        result.total += 1;
        if entry.pos != Pos::Verb {
            continue;
        }
        result.verbs += 1;
        let imperative_to = format!("{} 10 to x.", entry.lemma);
        let imperative_from = format!("{} 10 from x.", entry.lemma);
        let narrative = format!("Yesterday Sam {} 2 more.", past_form(entry.lemma));
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            (
                classify(parser, &imperative_to),
                classify(parser, &imperative_from),
                classify(parser, &narrative),
            )
        }));
        match outcome {
            Err(_) => {
                result.panic += 1;
                result.samples.push(format!("panic: {}", entry.lemma));
            }
            Ok((to, from, prose)) => {
                let imperative_exec = to.is_exec() || from.is_exec();
                let imperative_unsupported = to.is_unsupported() || from.is_unsupported();
                if imperative_exec {
                    result.exec += 1;
                } else if imperative_unsupported {
                    result.unsupported += 1;
                }
                if prose.is_comment() {
                    result.comment_ok += 1;
                }
                if !imperative_exec
                    && !imperative_unsupported
                    && (classify_verb(entry.lemma, Some("to")).is_some()
                        || classify_verb(entry.lemma, Some("from")).is_some())
                {
                    result.silent_drop += 1;
                    result.samples.push(format!("silent_drop: {}", entry.lemma));
                }
            }
        }
    }
    result
}

fn program_for_frame(
    verb: &str,
    prep: &str,
    predicate: Option<VerbPredicate>,
    role: PrepRole,
) -> String {
    let prep = if prep.is_empty() {
        default_prep(predicate, role)
    } else {
        prep
    };
    if predicate == Some(VerbPredicate::TransferMove) {
        return format!(
            "a is 100.\nb is 0.\n{verb} 10 from a to b.\nIf a is not 100 and b is not 0:\n    Return 1.\nReturn 0.\n"
        );
    }
    format!("x is 100.\n{verb} 10 {prep} x.\nReturn x.\n")
}

fn sweep_verbnet(parser: &LinkGrammarParser) -> VerbNetResult {
    let mut result = VerbNetResult::default();
    let mut cache: HashMap<String, (usize, usize, bool)> = HashMap::new();
    for frame in VERBNET_FRAMES {
        result.total += 1;
        let program = program_for_frame(frame.verb, frame.prep, Some(frame.predicate), frame.role);
        let outcome = cache.entry(program.clone()).or_insert_with(|| {
            match catch_unwind(AssertUnwindSafe(|| {
                lower_project_with_parser(
                    &[SourceFile::new("main.tc", program.clone())],
                    "main",
                    parser,
                )
            })) {
                Err(_) => (3, 0, false),
                Ok(Err(_)) => (2, 0, false),
                Ok(Ok(lowered)) => {
                    let verb_prefix = format!("{} ", frame.verb);
                    let verdict = lowered
                        .trace
                        .iter()
                        .find(|entry| entry.text.starts_with(&verb_prefix))
                        .map_or(IntentVerdict::Exec, |entry| entry.intent.clone());
                    match verdict {
                        IntentVerdict::Unsupported { .. } => (2, 0, false),
                        IntentVerdict::CommentOmitted { .. } => (1, 0, false),
                        IntentVerdict::Exec => match tc_typeck::check_and_lower(&lowered.program) {
                            Err(_) => (4, 0, false),
                            Ok(module) => {
                                let args = vec!["prog".to_string()];
                                match tc_ir::interpret(&module, &args, &[], 5_000_000) {
                                    Ok(output) => {
                                        let clobber =
                                            if frame.predicate == VerbPredicate::TransferMove {
                                                output.exit_code == 0
                                            } else {
                                                output.exit_code == 100
                                            };
                                        (0, 0, clobber)
                                    }
                                    Err(_) => (5, 0, false),
                                }
                            }
                        },
                    }
                }
            }
        });
        match *outcome {
            (0, _, clobbered) => {
                result.exec += 1;
                if clobbered
                    && matches!(
                        frame.predicate,
                        VerbPredicate::TransferAdd
                            | VerbPredicate::TransferSub
                            | VerbPredicate::TransferMove
                    )
                {
                    result.target_clobber += 1;
                    result.samples.push(format!(
                        "target_clobber: {} {} {:?}",
                        frame.verb, frame.prep, frame.predicate
                    ));
                }
            }
            (1, _, _) => {
                result.comment += 1;
                if matches!(
                    frame.predicate,
                    VerbPredicate::TransferAdd
                        | VerbPredicate::TransferSub
                        | VerbPredicate::TransferMove
                ) {
                    result.target_clobber += 1;
                    result.samples.push(format!(
                        "silent_drop: {} {} {:?}",
                        frame.verb, frame.prep, frame.predicate
                    ));
                }
            }
            (2, _, _) => result.unsupported += 1,
            (4, _, _) => {
                result.type_error += 1;
                result.samples.push(format!(
                    "type_error: {} {} {:?} :: {}",
                    frame.verb, frame.prep, frame.predicate, program
                ));
            }
            (5, _, _) => {
                result.type_error += 1;
                result.samples.push(format!(
                    "interp_error: {} {} {:?} :: {}",
                    frame.verb, frame.prep, frame.predicate, program
                ));
            }
            (3, _, _) => result.panic += 1,
            _ => {}
        }
        result.unique = cache.len();
    }
    result
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn main() -> std::process::ExitCode {
    let mut output = std::env::temp_dir().join("tc_r1_lexicon_sweep.json");
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        if argument == "--output" {
            output = args.next().map_or(output, PathBuf::from);
        }
    }
    let parser = match LinkGrammarParser::new() {
        Ok(parser) => parser,
        Err(error) => {
            eprintln!("lexicon_sweep: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let wordnet = sweep_wordnet(&parser);
    let verbnet = sweep_verbnet(&parser);
    let mut json = String::new();
    json.push_str("{\n");
    let _ = writeln!(
        json,
        "  \"wordnet\": {{\"total\": {}, \"verbs\": {}, \"exec\": {}, \"comment_ok\": {}, \"unsupported\": {}, \"silent_drop\": {}, \"panic\": {}}},\n",
        wordnet.total,
        wordnet.verbs,
        wordnet.exec,
        wordnet.comment_ok,
        wordnet.unsupported,
        wordnet.silent_drop,
        wordnet.panic
    );
    let _ = writeln!(
        json,
        "  \"verbnet\": {{\"total\": {}, \"unique\": {}, \"exec\": {}, \"unsupported\": {}, \"comment\": {}, \"type_error\": {}, \"target_clobber\": {}, \"panic\": {}}},\n",
        verbnet.total,
        verbnet.unique,
        verbnet.exec,
        verbnet.unsupported,
        verbnet.comment,
        verbnet.type_error,
        verbnet.target_clobber,
        verbnet.panic
    );
    json.push_str("  \"samples\": [\n");
    let samples = wordnet
        .samples
        .iter()
        .chain(verbnet.samples.iter())
        .take(50)
        .map(|sample| format!("    \"{}\"", json_escape(sample)))
        .collect::<Vec<_>>();
    json.push_str(&samples.join(",\n"));
    if !samples.is_empty() {
        json.push('\n');
    }
    json.push_str("  ]\n}\n");
    if let Err(error) = std::fs::write(&output, json) {
        eprintln!("lexicon_sweep: cannot write {}: {error}", output.display());
        return std::process::ExitCode::FAILURE;
    }
    println!(
        "wordnet: {}/{} verbs, exec={} comment_ok={} unsupported={} silent_drop={} panic={}",
        wordnet.verbs,
        wordnet.total,
        wordnet.exec,
        wordnet.comment_ok,
        wordnet.unsupported,
        wordnet.silent_drop,
        wordnet.panic
    );
    println!(
        "verbnet: {}/{} frames, unique={} exec={} unsupported={} comment={} type_error={} target_clobber={} panic={}",
        verbnet.total,
        VERBNET_FRAMES.len(),
        verbnet.unique,
        verbnet.exec,
        verbnet.unsupported,
        verbnet.comment,
        verbnet.type_error,
        verbnet.target_clobber,
        verbnet.panic
    );
    if wordnet.silent_drop + verbnet.target_clobber + wordnet.panic + verbnet.panic > 0 {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    }
}
