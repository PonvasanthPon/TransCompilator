//! Finite English semantic front end for `tc_r1`.
//!
//! The crate classifies every sentence through a strict three-way intent gate,
//! matches declarative `Phrase:` signatures, and lowers executable English into
//! the Phase 1 typed AST through one unified recursive statement lowerer.

mod cond;
mod discourse;
mod expr;
mod frame;
mod lower;
mod paraphrase;
mod place;
mod quantifier;
mod suggest;
mod textutil;
mod value;

pub mod intent;
pub mod phrases;

use std::collections::{HashMap, HashSet};
use std::fmt;

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
    let parser = LinkGrammarParser::new().map_err(|error: SyntaxError| {
        vec![SemanticError {
            module: entry_module.to_string(),
            line: 1,
            col: 1,
            kind: SemanticErrorKind::Segmentation,
            message: error.message,
        }]
    })?;
    lower_project_with_parser(sources, entry_module, &parser)
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
    if sources.is_empty() {
        return Err(vec![SemanticError {
            module: entry_module.to_string(),
            line: 1,
            col: 1,
            kind: SemanticErrorKind::Segmentation,
            message: "no source files were supplied".to_string(),
        }]);
    }
    let mut jobs = Vec::with_capacity(sources.len());
    let mut errors = Vec::new();
    for source in sources {
        match lower::parse_module(source) {
            Ok(job) => jobs.push(job),
            Err(mut diagnostics) => errors.append(&mut diagnostics),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
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
        return Err(errors);
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
        let is_entry = job.module.name == entry_key;
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
        trace.append(&mut traces);
        paraphrase.push(docs);
        errors.append(&mut diagnostics);
    }
    if errors.is_empty() {
        Ok(LoweredProject {
            program: Program {
                modules,
                entry_module: entry_key,
                entry_fn: "main".to_string(),
            },
            trace,
            paraphrase,
        })
    } else {
        Err(errors)
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
                kind: SemanticErrorKind::Unsupported,
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
