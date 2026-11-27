//! Finite English semantic front end for `tc_r1`.
//!
//! Pipeline position: intent, discourse, and clause lowering into the typed
//! AST consumed by `tc-typeck`.
//!
//! The crate classifies every sentence through a strict three-way intent gate,
//! matches declarative `Phrase:` signatures, and lowers executable English into
//! the Phase 1 typed AST through one unified recursive statement lowerer.

mod clause_graph;
mod cond;
mod discourse;
mod discourse_scope;
mod expr;
mod frame;
mod lower;
mod normalize;
mod number_words;
mod paraphrase;
mod place;
mod quantifier;
mod suggest;
mod textutil;
mod value;

pub mod intent;
pub mod phrases;

use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::fmt;
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
use std::path::{Path, PathBuf};

use tc_lexicon::{SemanticConcept, VerbPredicate};
use tc_typeck::{AstModule, AstType, Program};

pub use intent::IntentVerdict;
pub use paraphrase::{
    ParaphraseDoc, ParaphraseEntry, canonicalize_statement, format_paraphrase,
    format_paraphrase_entries,
};
pub use phrases::{PhraseBinding, PhraseMatch, is_phrase_declaration, match_phrase};
pub use tc_syntax::token;
pub use tc_syntax::{
    ClauseTree, LgLink, LgParse, LinkGrammarParser, PrepPhrase, RawBlock, RawDocument, RawItem,
    RawSentence, SourceFile, Span, SyntaxError, Token, TokenKind, VerbHead, Voice, analyze_clause,
    segment_source, sentence_tokens, tokenize,
};

/// Returns true when a word can participate in an English number phrase.
#[must_use]
pub fn is_english_number_word(word: &str) -> bool {
    number_words::is_word(&word.to_ascii_lowercase())
}

/// Parses a complete English cardinal number phrase.
#[must_use]
pub fn parse_english_number(text: &str) -> Option<i64> {
    number_words::parse(text)
}

/// Category of a semantic-front-end failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SemanticErrorKind {
    /// Source segmentation failed.
    Segmentation,
    /// A declaration was malformed.
    Declaration,
    /// An executable sentence could not be lowered.
    Lowering,
    /// A computational directive has no Core-7 lowering.
    Unsupported,
    /// The typed AST failed type checking or IR lowering.
    TypeCheck,
}

/// One structured semantic diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticError {
    /// Module name.
    pub module: String,
    /// One-based line number.
    pub line: u32,
    /// One-based column number.
    pub col: u32,
    /// Diagnostic category.
    pub kind: SemanticErrorKind,
    /// Human-readable explanation.
    pub message: String,
}

impl fmt::Display for SemanticError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{}:{}: {:?}: {}",
            self.module, self.line, self.col, self.kind, self.message
        )
    }
}

impl std::error::Error for SemanticError {}

/// Provenance and intent for one processed English sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentenceTrace {
    /// Module name.
    pub module: String,
    /// One-based line number.
    pub line: u32,
    /// One-based column number.
    pub col: u32,
    /// Cleaned sentence text.
    pub text: String,
    /// Leading discourse fillers that were stripped.
    pub fillers: Vec<String>,
    /// Three-way intent verdict.
    pub intent: IntentVerdict,
    /// True when Link Grammar produced a complete linkage.
    pub lg_used: bool,
    /// Link Grammar word count excluding walls.
    pub lg_words: usize,
    /// Link Grammar link count.
    pub lg_links: usize,
    /// Normalized voice.
    pub voice: Voice,
    /// Main verb spelling.
    pub verb: Option<String>,
    /// Lemma.
    pub lemma: Option<String>,
    /// VerbNet predicate, when available.
    pub predicate: Option<VerbPredicate>,
    /// WordNet concept, when available.
    pub concept: Option<SemanticConcept>,
    /// VerbNet class id.
    pub class_id: Option<&'static str>,
    /// WordNet synset id.
    pub synset_id: Option<&'static str>,
}

/// A lowered but not yet type-checked English project.
#[derive(Debug, Clone)]
pub struct LoweredProject {
    /// High-level typed AST.
    pub program: Program,
    /// Per-sentence analysis and provenance.
    pub trace: Vec<SentenceTrace>,
    /// Deterministic paraphrase echo, grouped by module.
    pub paraphrase: Vec<ParaphraseDoc>,
}

/// Lowering result that preserves per-sentence trace even when diagnostics
/// prevent a complete project.
#[derive(Debug, Clone)]
pub struct LowerProjectOutcome {
    /// Complete lowered project when no diagnostics were produced.
    pub project: Option<LoweredProject>,
    /// Per-sentence analysis collected from every processed sentence.
    pub trace: Vec<SentenceTrace>,
    /// Declaration, lowering, or syntax diagnostics.
    pub errors: Vec<SemanticError>,
}

/// A fully checked and lowered English project.
#[derive(Debug, Clone)]
pub struct CompiledProject {
    /// High-level typed AST.
    pub program: Program,
    /// Verified Core-7 IR module.
    pub module: tc_ir::Module,
    /// Per-sentence analysis and provenance.
    pub trace: Vec<SentenceTrace>,
    /// Deterministic paraphrase echo, grouped by module.
    pub paraphrase: Vec<ParaphraseDoc>,
}

fn type_error(module: &str, error: &tc_typeck::TypeError) -> SemanticError {
    SemanticError {
        module: module.to_string(),
        line: error.span.line,
        col: error.span.col,
        kind: SemanticErrorKind::TypeCheck,
        message: error.message.clone(),
    }
}

/// Lowers a set of English source files into a typed program without running
/// the type checker.
///
/// # Errors
///
/// Returns declaration, lowering, or syntax diagnostics.  Sentences whose
/// intent verdict is [`IntentVerdict::Unsupported`] are recorded in
/// [`LoweredProject::trace`] rather than silently dropped.
pub fn lower_project(
    sources: &[SourceFile],
    entry_module: &str,
) -> Result<LoweredProject, Vec<SemanticError>> {
    let outcome = lower_project_outcome(sources, entry_module);
    outcome.project.ok_or(outcome.errors)
}

/// Lowers English while retaining every collected sentence trace, even when a
/// later sentence produces a diagnostic.
#[must_use]
pub fn lower_project_outcome(sources: &[SourceFile], entry_module: &str) -> LowerProjectOutcome {
    let parser = match LinkGrammarParser::new() {
        Ok(parser) => parser,
        Err(error) => {
            return LowerProjectOutcome {
                project: None,
                trace: Vec::new(),
                errors: vec![SemanticError {
                    module: entry_module.to_string(),
                    line: 1,
                    col: 1,
                    kind: SemanticErrorKind::Segmentation,
                    message: error.message,
                }],
            };
        }
    };
    lower_project_analysis_with_parser(sources, entry_module, &parser)
}

fn is_std_module(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "core" | "alloc" | "io" | "string" | "list" | "fs" | "map" | "math"
    )
}

/// Finds the repository `std/` directory.
///
/// Resolution order is `TC_STD_DIR`, the executable's ancestors, then the
/// compile-time workspace root. Every candidate must contain `core.eng`.
#[must_use]
pub fn find_std_dir() -> Option<PathBuf> {
    if let Some(value) = env::var_os("TC_STD_DIR") {
        let path = PathBuf::from(value);
        if path.join("core.eng").is_file() {
            return Some(path);
        }
    }
    if let Ok(executable) = env::current_exe() {
        let mut directory = executable.parent().map(Path::to_path_buf);
        for _ in 0..6 {
            let Some(current) = &directory else {
                break;
            };
            let candidate = current.join("std");
            if candidate.join("core.eng").is_file() {
                return Some(candidate);
            }
            directory = current.parent().map(Path::to_path_buf);
        }
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../std");
    manifest
        .join("core.eng")
        .is_file()
        .then(|| manifest.canonicalize().unwrap_or(manifest))
}

/// The eight standard-library modules embedded for WebAssembly builds.
///
/// Browsers have no host filesystem, so the same sources that `find_std_dir`
/// reads on native targets are compiled into the wasm module.  Keeping the
/// table explicit preserves the module DAG and makes the browser build fully
/// deterministic.
#[cfg(target_arch = "wasm32")]
const EMBEDDED_STD: [(&str, &str); 8] = [
    ("core", include_str!("../../../std/core.eng")),
    ("alloc", include_str!("../../../std/alloc.eng")),
    ("io", include_str!("../../../std/io.eng")),
    ("string", include_str!("../../../std/string.eng")),
    ("list", include_str!("../../../std/list.eng")),
    ("fs", include_str!("../../../std/fs.eng")),
    ("map", include_str!("../../../std/map.eng")),
    ("math", include_str!("../../../std/math.eng")),
];

#[cfg(target_arch = "wasm32")]
fn embedded_std(module: &str) -> Option<&'static str> {
    EMBEDDED_STD
        .iter()
        .find(|(name, _)| *name == module)
        .map(|(_, text)| *text)
}

fn std_sources_available() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        true
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        find_std_dir().is_some()
    }
}

fn load_std_module(module: &str) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        embedded_std(module).map(str::to_string)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let std_dir = find_std_dir()?;
        ["eng", "tc"]
            .iter()
            .map(|extension| std_dir.join(format!("{module}.{extension}")))
            .find(|path| path.is_file())
            .and_then(|path| fs::read_to_string(path).ok())
    }
}

fn module_imports(source: &SourceFile) -> Vec<String> {
    let document = tc_syntax::segment_source(source);
    let mut imports = Vec::new();
    for item in document.items {
        let tc_syntax::RawItem::Sentence(sentence) = item else {
            continue;
        };
        let tokens = tc_syntax::tokenize(&sentence.text);
        let Some(first) = tokens.first() else {
            continue;
        };
        if !first.is("import") && !first.is("use") {
            continue;
        }
        for token in tokens.iter().skip(1) {
            if token.is("as") {
                break;
            }
            if token.is_word() && !token.is("and") && !token.is("module") {
                imports.push(token.lower.clone());
            }
        }
    }
    imports
}

fn augmented_sources(
    sources: &[SourceFile],
    entry_module: &str,
) -> (Vec<SourceFile>, HashSet<String>) {
    let mut all = sources.to_vec();
    let mut added = HashSet::new();
    if is_std_module(entry_module) {
        return (all, added);
    }
    if !std_sources_available() {
        return (all, added);
    }
    let source_text = all
        .iter()
        .map(|source| source.text.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join("\n");
    let needs_list = source_text.contains("a list of ")
        || source_text.contains("an empty list of ")
        || source_text.contains("number of items in ")
        || source_text.contains("the size of ")
        || source_text.contains("the list ")
        || source_text.contains("the list,")
        || source_text.contains("list of numbers ")
        || source_text.contains("start with the numbers ")
        || source_text.contains("begin with the numbers ")
        || source_text.contains("start with the values ")
        || source_text.contains("start with the items ")
        || (source_text.contains("for each ") && source_text.contains(" in "));
    let needs_string = source_text.contains("a list of \"")
        || source_text.contains("list of text")
        || source_text.contains("list of strings")
        || source_text.contains("number of characters in ")
        || source_text.contains("trim ")
        || source_text.contains("trimmed ")
        || source_text.contains(" starts with ")
        || source_text.contains(" start with ")
        || source_text.contains(" ends with ")
        || source_text.contains(" end with ")
        || source_text.contains(" contains ")
        || source_text.contains(" contain ")
        || source_text.contains("length of ")
        || source_text.contains(" matches text ");
    let needs_math = source_text.contains("square root of");
    let mut present = all
        .iter()
        .map(SourceFile::module_name)
        .collect::<HashSet<_>>();
    let mut wanted = vec!["io".to_string()];
    // Explicit `Use`/`Import` declarations are authoritative. The browser
    // build has no filesystem, so these names must enter the embedded-module
    // queue even when the prose heuristics below do not recognize a phrase.
    for source in &all {
        for module in module_imports(source) {
            if is_std_module(&module) {
                wanted.push(module);
            }
        }
    }
    if needs_list {
        wanted.push("list".to_string());
    }
    if needs_string {
        wanted.push("string".to_string());
    }
    if needs_math {
        wanted.push("math".to_string());
    }
    let mut queue = VecDeque::from(wanted);
    let mut seen = HashSet::new();
    while let Some(module) = queue.pop_front() {
        if !seen.insert(module.clone()) || present.contains(&module) {
            continue;
        }
        let Some(text) = load_std_module(&module) else {
            continue;
        };
        let source = SourceFile::new(format!("{module}.tc"), text);
        for import in module_imports(&source) {
            queue.push_back(import);
        }
        present.insert(source.module_name());
        added.insert(source.module_name());
        all.push(source);
    }
    (all, added)
}

/// Lowers a project with a caller-owned Link Grammar parser.  This is the
/// high-throughput entry point used by the exhaustive lexicon sweeper.
///
/// # Errors
///
/// Returns declaration, lowering, or syntax diagnostics.
pub fn lower_project_with_parser(
    sources: &[SourceFile],
    entry_module: &str,
    parser: &LinkGrammarParser,
) -> Result<LoweredProject, Vec<SemanticError>> {
    let outcome = lower_project_analysis_with_parser(sources, entry_module, parser);
    outcome.project.ok_or(outcome.errors)
}

/// Lowers a project with a caller-owned parser and preserves the sentence trace
/// when any diagnostic occurs.
#[must_use]
pub fn lower_project_analysis_with_parser(
    sources: &[SourceFile],
    entry_module: &str,
    parser: &LinkGrammarParser,
) -> LowerProjectOutcome {
    lower_project_analysis_with_mode(sources, entry_module, parser, true)
}

/// Lowers only the caller-supplied sources, without loading or injecting the
/// standard library.
///
/// This is the fast path used by editor semantic highlighting.  It preserves
/// the same parser, intent gate, and lowering rules as a full project, but it
/// deliberately skips module augmentation so live token generation does not
/// re-analyze `std/*.eng` on every keystroke.
#[must_use]
pub fn lower_project_outcome_bare(
    sources: &[SourceFile],
    entry_module: &str,
) -> LowerProjectOutcome {
    let parser = match LinkGrammarParser::new() {
        Ok(parser) => parser,
        Err(error) => {
            return LowerProjectOutcome {
                project: None,
                trace: Vec::new(),
                errors: vec![SemanticError {
                    module: entry_module.to_string(),
                    line: 1,
                    col: 1,
                    kind: SemanticErrorKind::Segmentation,
                    message: error.message,
                }],
            };
        }
    };
    lower_project_analysis_with_mode(sources, entry_module, &parser, false)
}

fn lower_project_analysis_with_mode(
    sources: &[SourceFile],
    entry_module: &str,
    parser: &LinkGrammarParser,
    augment_std: bool,
) -> LowerProjectOutcome {
    if sources.is_empty() {
        return LowerProjectOutcome {
            project: None,
            trace: Vec::new(),
            errors: vec![SemanticError {
                module: entry_module.to_string(),
                line: 1,
                col: 1,
                kind: SemanticErrorKind::Segmentation,
                message: "no source files were supplied".to_string(),
            }],
        };
    }
    let (sources, auto_modules) = if augment_std {
        augmented_sources(sources, entry_module)
    } else {
        (sources.to_vec(), HashSet::new())
    };
    let auto_prelude = augment_std && !is_std_module(entry_module);
    let mut jobs = Vec::with_capacity(sources.len());
    let mut errors = Vec::new();
    for source in &sources {
        match lower::parse_module(source) {
            Ok(job) => jobs.push(job),
            Err(mut diagnostics) => errors.append(&mut diagnostics),
        }
    }
    if !errors.is_empty() {
        return LowerProjectOutcome {
            project: None,
            trace: Vec::new(),
            errors,
        };
    }
    if auto_prelude {
        let mut auto_imports = vec!["io"];
        if auto_modules.contains("list") {
            auto_imports.push("list");
        }
        if auto_modules.contains("string") {
            auto_imports.push("string");
        }
        if auto_modules.contains("math") {
            auto_imports.push("math");
        }
        for job in &mut jobs {
            if is_std_module(&job.module.name) {
                continue;
            }
            for module in &auto_imports {
                if !job
                    .module
                    .imports
                    .iter()
                    .any(|import| import.module == *module)
                {
                    job.module.imports.push(tc_typeck::Import {
                        module: (*module).to_string(),
                        alias: None,
                        span: tc_typeck::Span::new(&job.module.name, 1, 1),
                    });
                }
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    for job in &jobs {
        if !seen.insert(job.module.name.clone()) {
            errors.push(SemanticError {
                module: job.module.name.clone(),
                line: 1,
                col: 1,
                kind: SemanticErrorKind::Declaration,
                message: format!("duplicate module `{}`", job.module.name),
            });
        }
    }
    if errors.is_empty() {
        let entry_key = entry_module.to_ascii_lowercase();
        if !jobs.iter().any(|job| job.module.name == entry_key) {
            errors.push(SemanticError {
                module: entry_key.clone(),
                line: 1,
                col: 1,
                kind: SemanticErrorKind::Declaration,
                message: format!("entry module `{entry_key}` was not found"),
            });
        }
    }
    if !errors.is_empty() {
        return LowerProjectOutcome {
            project: None,
            trace: Vec::new(),
            errors,
        };
    }

    let bindings = lower::collect_phrase_bindings(&jobs);
    let mut global_returns: HashMap<String, Option<AstType>> = HashMap::new();
    let mut global_types = HashSet::new();
    let mut global_aggregates = HashMap::new();
    for job in &jobs {
        for function in &job.module.functions {
            global_returns.insert(
                function.name.clone(),
                (!lower::type_contains_params(&function.ret, &function.type_params))
                    .then(|| function.ret.clone()),
            );
        }
        for definition in &job.module.structs {
            if definition.vis == tc_typeck::Visibility::Public {
                global_types.insert(definition.name.clone());
                global_aggregates.insert(
                    definition.name.clone(),
                    crate::expr::AggregateInfo {
                        type_params: definition.type_params.clone(),
                        fields: definition.fields.iter().cloned().collect(),
                    },
                );
            }
        }
        for definition in &job.module.unions {
            if definition.vis == tc_typeck::Visibility::Public {
                global_types.insert(definition.name.clone());
                global_aggregates.insert(
                    definition.name.clone(),
                    crate::expr::AggregateInfo {
                        type_params: definition.type_params.clone(),
                        fields: definition.fields.iter().cloned().collect(),
                    },
                );
            }
        }
    }
    let mut module_names = jobs
        .iter()
        .map(|job| job.module.name.clone())
        .collect::<Vec<_>>();
    for job in &jobs {
        for import in &job.module.imports {
            if let Some(alias) = &import.alias {
                if !module_names.contains(alias) {
                    module_names.push(alias.clone());
                }
            }
        }
    }
    let entry_key = entry_module.to_ascii_lowercase();
    let mut modules = Vec::with_capacity(jobs.len());
    let mut trace = Vec::new();
    let mut paraphrase = Vec::new();
    for (index, job) in jobs.into_iter().enumerate() {
        let job_module_name = job.module.name.clone();
        let is_entry = job_module_name == entry_key;
        let (module, mut traces, mut diagnostics, docs) = lower::lower_module(
            job,
            parser,
            module_names.clone(),
            bindings[index].clone(),
            global_returns.clone(),
            global_types.clone(),
            global_aggregates.clone(),
            is_entry,
        );
        modules.push(module);
        if !auto_modules.contains(&job_module_name) {
            trace.append(&mut traces);
            paraphrase.push(docs);
        }
        errors.append(&mut diagnostics);
    }
    let project = errors.is_empty().then(|| LoweredProject {
        program: Program {
            modules,
            entry_module: entry_key,
            entry_fn: "main".to_string(),
        },
        trace: trace.clone(),
        paraphrase,
    });
    LowerProjectOutcome {
        project,
        trace,
        errors,
    }
}

/// Lowers English and runs the Phase 1 type checker, monomorphizer, and IR
/// lowering.
///
/// # Errors
///
/// Returns semantic diagnostics plus a loud [`SemanticErrorKind::Unsupported`]
/// diagnostic for every out-of-scope computational directive.
pub fn compile_english_to_ir(
    sources: &[SourceFile],
    entry_module: &str,
) -> Result<CompiledProject, Vec<SemanticError>> {
    let lowered = lower_project(sources, entry_module)?;
    let mut errors = lowered
        .trace
        .iter()
        .filter_map(|entry| match &entry.intent {
            IntentVerdict::Unsupported { reason } => Some(SemanticError {
                module: entry.module.clone(),
                line: entry.line,
                col: entry.col,
                kind: if reason.starts_with("unrecognized imperative") {
                    SemanticErrorKind::Lowering
                } else {
                    SemanticErrorKind::Unsupported
                },
                message: reason.clone(),
            }),
            _ => None,
        })
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        return Err(errors);
    }
    let module = match tc_typeck::check_and_lower(&lowered.program) {
        Ok(module) => module,
        Err(type_errors) => {
            errors.extend(
                type_errors
                    .iter()
                    .map(|error| type_error(&lowered.program.entry_module, error)),
            );
            return Err(errors);
        }
    };
    Ok(CompiledProject {
        program: lowered.program,
        module,
        trace: lowered.trace,
        paraphrase: lowered.paraphrase,
    })
}

/// Returns the canonical package name of this crate.
#[must_use]
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

/// Returns the canonical package names of this crate's direct dependencies.
#[must_use]
pub fn dependency_names() -> [&'static str; 4] {
    [
        tc_ir::crate_name(),
        tc_lexicon::crate_name(),
        tc_syntax::crate_name(),
        tc_typeck::crate_name(),
    ]
}

/// Returns the module names of a lowered project.
#[must_use]
pub fn module_names(project: &LoweredProject) -> Vec<String> {
    project
        .program
        .modules
        .iter()
        .map(|module: &AstModule| module.name.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{crate_name, dependency_names};

    #[test]
    fn crate_identity_matches_workspace() {
        assert_eq!(crate_name(), "tc-semantics");
        assert_eq!(
            dependency_names(),
            ["tc-ir", "tc-lexicon", "tc-syntax", "tc-typeck"]
        );
    }
}
