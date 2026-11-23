//! In-memory project analysis for one open document.

use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use tc_semantics::{
    IntentVerdict, LoweredProject, RawItem, SemanticError, SentenceTrace, SourceFile,
    lower_project_outcome, segment_source, tokenize,
};
use tc_typeck::{AstExpr, AstModule, AstStmt, AstType, Program};

/// One LSP diagnostic ready for JSON serialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Zero-based line.
    pub line: u32,
    /// Zero-based UTF-16 column.
    pub character: u32,
    /// LSP severity (1 error).
    pub severity: u32,
    /// Plain-English message.
    pub message: String,
    /// Diagnostic source.
    pub source: String,
}

/// One record or union field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldInfo {
    /// Canonical field name.
    pub name: String,
    /// Human-readable type.
    pub ty: String,
}

/// One record, union, or textual type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeInfo {
    /// Canonical lower-case name.
    pub name: String,
    /// `record` or `union`.
    pub kind: &'static str,
    /// Fields in declaration order.
    pub fields: Vec<FieldInfo>,
    /// Module name.
    pub module: String,
    /// One-based declaration line.
    pub line: u32,
    /// One-based declaration column.
    pub col: u32,
}

/// One variable or parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableInfo {
    /// Canonical lower-case name.
    pub name: String,
    /// Human-readable type.
    pub ty: String,
    /// Module name.
    pub module: String,
    /// One-based declaration line.
    pub line: u32,
    /// One-based declaration column.
    pub col: u32,
}

/// One function or phrase-backed helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionInfo {
    /// Canonical lower-case name.
    pub name: String,
    /// Module name.
    pub module: String,
    /// One-based declaration line.
    pub line: u32,
    /// One-based declaration column.
    pub col: u32,
    /// Parameters.
    pub params: Vec<(String, String)>,
    /// Return type.
    pub ret: String,
    /// Declarative phrase templates.
    pub phrases: Vec<String>,
}

/// One shared global.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalInfo {
    /// Canonical lower-case name.
    pub name: String,
    /// Human-readable type.
    pub ty: String,
    /// Module name.
    pub module: String,
    /// One-based declaration line.
    pub line: u32,
    /// One-based declaration column.
    pub col: u32,
}

/// A searchable symbol index for completion, hover, and definition.
#[derive(Debug, Clone, Default)]
pub struct SymbolIndex {
    /// Record/union types.
    pub types: Vec<TypeInfo>,
    /// Variables and parameters.
    pub variables: Vec<VariableInfo>,
    /// Functions.
    pub functions: Vec<FunctionInfo>,
    /// Globals.
    pub globals: Vec<GlobalInfo>,
    /// Deduplicated phrase templates.
    pub phrases: Vec<String>,
}

impl SymbolIndex {
    /// Finds a type by canonical name.
    #[must_use]
    pub fn type_named(&self, name: &str) -> Option<&TypeInfo> {
        self.types.iter().find(|value| value.name == name)
    }

    /// Finds the latest variable before a source line.
    #[must_use]
    pub fn variable_named(&self, name: &str, line: u32) -> Option<&VariableInfo> {
        self.variables
            .iter()
            .filter(|value| value.name == name && value.line <= line + 1)
            .max_by_key(|value| value.line)
            .or_else(|| self.variables.iter().find(|value| value.name == name))
    }

    /// Finds a function by canonical name.
    #[must_use]
    pub fn function_named(&self, name: &str) -> Option<&FunctionInfo> {
        self.functions.iter().find(|value| value.name == name)
    }
}

/// One complete analysis result.
#[derive(Debug, Clone)]
pub struct Analysis {
    /// Lowered project when semantic parsing succeeded.
    pub lowered: Option<LoweredProject>,
    /// Per-sentence trace, preserved even when a later sentence errors.
    pub trace: Vec<SentenceTrace>,
    /// Diagnostics for the open document.
    pub diagnostics: Vec<Diagnostic>,
    /// Module name to source path.
    pub paths: HashMap<String, PathBuf>,
    /// Entry module name.
    pub entry: String,
    /// Symbol index.
    pub symbols: SymbolIndex,
}

/// Finds the repository `std/` directory.
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
        for _ in 0..5 {
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

/// Analyzes one open document, resolving sibling and `std/` imports.
#[must_use]
pub fn analyze(uri: &str, text: &str) -> Analysis {
    let path = crate::doc::uri_to_path(uri);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .map_or_else(|| "untitled".to_string(), str::to_ascii_lowercase);
    let entry = stem.clone();
    let mut paths = HashMap::from([(entry.clone(), path.clone())]);
    let mut sources = vec![SourceFile::new(format!("{stem}.tc"), text.to_string())];
    let mut diagnostics = Vec::new();
    let std_dir = find_std_dir();
    let mut seen = HashSet::from([entry.clone()]);
    let mut queue = VecDeque::from([(path, text.to_string())]);
    while let Some((current_path, current_text)) = queue.pop_front() {
        let source = SourceFile::new(
            format!(
                "{}.tc",
                current_path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("untitled")
            ),
            current_text,
        );
        for import in imports_of(&source) {
            if !seen.insert(import.clone()) {
                continue;
            }
            let Some(import_path) = find_import(&import, current_path.parent(), std_dir.as_deref())
            else {
                diagnostics.push(Diagnostic {
                    line: 0,
                    character: 0,
                    severity: 1,
                    message: format!(
                        "imported module `{import}` was not found next to this document or in std/"
                    ),
                    source: "tc-lsp".to_string(),
                });
                continue;
            };
            match fs::read_to_string(&import_path) {
                Ok(import_text) => {
                    paths.insert(import.clone(), import_path.clone());
                    sources.push(SourceFile::new(
                        format!(
                            "{}.tc",
                            import_path
                                .file_stem()
                                .and_then(|value| value.to_str())
                                .unwrap_or(&import)
                        ),
                        import_text.clone(),
                    ));
                    queue.push_back((import_path, import_text));
                }
                Err(error) => diagnostics.push(Diagnostic {
                    line: 0,
                    character: 0,
                    severity: 1,
                    message: format!("cannot read `{}`: {error}", import_path.display()),
                    source: "tc-lsp".to_string(),
                }),
            }
        }
    }

    let outcome = lower_project_outcome(&sources, &entry);
    diagnostics.extend(outcome.errors.iter().map(from_semantic_error));
    let lowered = outcome.project;
    let trace = outcome.trace;
    for entry in &trace {
        if let IntentVerdict::Unsupported { reason } = &entry.intent {
            diagnostics.push(Diagnostic {
                line: entry.line.saturating_sub(1),
                character: entry.col.saturating_sub(1),
                severity: 1,
                message: reason.clone(),
                source: "tc-lsp".to_string(),
            });
        }
    }
    if let Some(project) = &lowered
        && let Err(errors) = tc_typeck::check_and_lower(&project.program)
    {
        diagnostics.extend(errors.iter().map(|error| Diagnostic {
            line: error.span.line.saturating_sub(1),
            character: error.span.col.saturating_sub(1),
            severity: 1,
            message: error.message.clone(),
            source: "tc-lsp".to_string(),
        }));
    }
    let has_sentences = trace.iter().any(|entry| !entry.text.trim().is_empty());
    let has_exec = trace.iter().any(|entry| entry.intent.is_exec());
    if has_sentences && !has_exec {
        diagnostics.push(Diagnostic {
            line: 0,
            character: 0,
            severity: 2,
            message: "program produced 0 executable statements (all sentences were omitted as prose/comments)".to_string(),
            source: "tc-lsp".to_string(),
        });
    }
    diagnostics.sort_by_key(|value| (value.line, value.character, value.message.clone()));
    diagnostics.dedup_by(|left, right| {
        left.line == right.line
            && left.character == right.character
            && left.message == right.message
    });

    let symbols = lowered.as_ref().map_or_else(
        || SymbolIndex::from_text(text),
        |project| SymbolIndex::from_program(&project.program, &paths, text),
    );
    Analysis {
        lowered,
        trace,
        diagnostics,
        paths,
        entry,
        symbols,
    }
}

fn imports_of(source: &SourceFile) -> Vec<String> {
    let document = segment_source(source);
    let mut imports = Vec::new();
    for item in document.items {
        let RawItem::Sentence(sentence) = item else {
            continue;
        };
        let tokens = tokenize(&sentence.text);
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

fn find_import(module: &str, current: Option<&Path>, std_dir: Option<&Path>) -> Option<PathBuf> {
    let mut directories = Vec::new();
    if let Some(current) = current {
        directories.push(current.to_path_buf());
    }
    if let Some(std_dir) = std_dir {
        directories.push(std_dir.to_path_buf());
    }
    for directory in directories {
        for extension in ["eng", "tc", "ENG"] {
            let candidate = directory.join(format!("{module}.{extension}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn from_semantic_error(error: &SemanticError) -> Diagnostic {
    Diagnostic {
        line: error.line.saturating_sub(1),
        character: error.col.saturating_sub(1),
        severity: 1,
        message: error.message.clone(),
        source: "tc-lsp".to_string(),
    }
}

impl SymbolIndex {
    fn from_program(program: &Program, _paths: &HashMap<String, PathBuf>, text: &str) -> Self {
        let mut index = Self::default();
        for module in &program.modules {
            add_module(module, &mut index);
        }
        let textual = Self::from_text(text);
        for value in textual.types {
            if index.type_named(&value.name).is_none() {
                index.types.push(value);
            }
        }
        for value in textual.variables {
            if !index
                .variables
                .iter()
                .any(|candidate| candidate.name == value.name)
            {
                index.variables.push(value);
            }
        }
        index.dedup();
        index
    }

    fn from_text(text: &str) -> Self {
        let mut index = Self::default();
        for line in text.lines() {
            let tokens = tokenize(line);
            if tokens.len() >= 4
                && tokens[0].is("a")
                && (tokens[1].is("record") || tokens[1].is("union"))
            {
                let kind = if tokens[1].is("record") {
                    "record"
                } else {
                    "union"
                };
                let name = tokens[2].lower.clone();
                let fields = tokens
                    .iter()
                    .position(|token| token.is("has"))
                    .map_or_else(Vec::new, |position| {
                        parse_text_fields(&line[tokens[position].end..])
                    });
                index.types.push(TypeInfo {
                    name,
                    kind,
                    fields,
                    module: String::new(),
                    line: 1,
                    col: 1,
                });
            }
            if tokens.len() >= 5 && tokens[0].is("create") {
                let called = tokens.iter().position(|token| token.is("called"));
                if let Some(called) = called {
                    if called >= 2 && called + 1 < tokens.len() {
                        let type_name = tokens[called - 1].lower.clone();
                        index.variables.push(VariableInfo {
                            name: tokens[called + 1].lower.clone(),
                            ty: type_name,
                            module: String::new(),
                            line: 1,
                            col: 1,
                        });
                    }
                }
            }
        }
        index
    }

    fn dedup(&mut self) {
        let mut seen = HashSet::new();
        self.types.retain(|value| seen.insert(value.name.clone()));
        let mut seen = HashSet::new();
        self.variables
            .retain(|value| seen.insert((value.name.clone(), value.ty.clone(), value.line)));
        let mut seen = HashSet::new();
        self.functions
            .retain(|value| seen.insert((value.name.clone(), value.module.clone())));
        let mut seen = HashSet::new();
        self.globals
            .retain(|value| seen.insert((value.name.clone(), value.module.clone())));
        let mut seen = HashSet::new();
        self.phrases.retain(|value| seen.insert(value.clone()));
        self.phrases.sort();
    }
}

fn parse_text_fields(text: &str) -> Vec<FieldInfo> {
    let mut fields = Vec::new();
    for raw in text.split(',') {
        for part in raw.split(" and ") {
            let trimmed = part.trim().trim_end_matches('.');
            if trimmed.is_empty() {
                continue;
            }
            let tokens = tokenize(trimmed);
            if tokens.is_empty() {
                continue;
            }
            let name = tokens[0].lower.clone();
            let ty = trimmed
                .find('(')
                .and_then(|start| {
                    trimmed[start + 1..]
                        .find(')')
                        .map(|end| &trimmed[start + 1..start + 1 + end])
                })
                .unwrap_or("value");
            fields.push(FieldInfo {
                name: name.replace('_', " "),
                ty: ty.to_string(),
            });
        }
    }
    fields
}

fn add_module(module: &AstModule, index: &mut SymbolIndex) {
    for definition in &module.structs {
        index.types.push(TypeInfo {
            name: definition.name.clone(),
            kind: "record",
            fields: definition
                .fields
                .iter()
                .map(|(name, ty)| FieldInfo {
                    name: name.replace('_', " "),
                    ty: type_name(ty),
                })
                .collect(),
            module: module.name.clone(),
            line: definition.span.line,
            col: definition.span.col,
        });
    }
    for definition in &module.unions {
        index.types.push(TypeInfo {
            name: definition.name.clone(),
            kind: "union",
            fields: definition
                .fields
                .iter()
                .map(|(name, ty)| FieldInfo {
                    name: name.replace('_', " "),
                    ty: type_name(ty),
                })
                .collect(),
            module: module.name.clone(),
            line: definition.span.line,
            col: definition.span.col,
        });
    }
    for global in &module.globals {
        index.globals.push(GlobalInfo {
            name: global.name.clone(),
            ty: type_name(&global.ty),
            module: module.name.clone(),
            line: global.span.line,
            col: global.span.col,
        });
    }
    for function in &module.functions {
        let mut phrases = function
            .phrases
            .iter()
            .map(pattern_text)
            .collect::<Vec<_>>();
        phrases.sort();
        index.phrases.extend(phrases.clone());
        index.functions.push(FunctionInfo {
            name: function.name.clone(),
            module: module.name.clone(),
            line: function.span.line,
            col: function.span.col,
            params: function
                .params
                .iter()
                .map(|(name, ty)| (name.clone(), type_name(ty)))
                .collect(),
            ret: type_name(&function.ret),
            phrases,
        });
        for (name, ty) in &function.params {
            index.variables.push(VariableInfo {
                name: name.clone(),
                ty: type_name(ty),
                module: module.name.clone(),
                line: function.span.line,
                col: function.span.col,
            });
        }
        walk_stmts(
            &function.body,
            module,
            &function.params,
            index,
            &mut HashMap::new(),
        );
    }
}

fn walk_stmts(
    statements: &[AstStmt],
    module: &AstModule,
    params: &[(String, AstType)],
    index: &mut SymbolIndex,
    known: &mut HashMap<String, String>,
) {
    if known.is_empty() {
        for (name, ty) in params {
            known.insert(name.clone(), type_name(ty));
        }
    }
    for statement in statements {
        match statement {
            AstStmt::Let {
                name,
                ty,
                init,
                span,
            } => {
                let resolved = ty
                    .as_ref()
                    .map_or_else(|| infer_expr_type(init, known), type_name);
                known.insert(name.clone(), resolved.clone());
                index.variables.push(VariableInfo {
                    name: name.clone(),
                    ty: resolved,
                    module: module.name.clone(),
                    line: span.line,
                    col: span.col,
                });
            }
            AstStmt::Block(body, _) => walk_stmts(body, module, params, index, known),
            AstStmt::If {
                then_body,
                else_body,
                ..
            } => {
                walk_stmts(then_body, module, params, index, known);
                walk_stmts(else_body, module, params, index, known);
            }
            AstStmt::Loop { body, .. } => walk_stmts(body, module, params, index, known),
            _ => {}
        }
    }
}

fn infer_expr_type(expression: &AstExpr, known: &HashMap<String, String>) -> String {
    match expression {
        AstExpr::ConstInt(..) | AstExpr::ConstUint(..) => "whole number".to_string(),
        AstExpr::ConstFloat(..) => "decimal".to_string(),
        AstExpr::ConstBool(_) => "truth value".to_string(),
        AstExpr::ConstNull(ty) | AstExpr::Zero(ty) => type_name(ty),
        AstExpr::BytesLit(_) | AstExpr::BytesLen(_) => "text".to_string(),
        AstExpr::Var(name) => known
            .get(name)
            .cloned()
            .unwrap_or_else(|| "value".to_string()),
        AstExpr::Call { name, .. } => format!("result of {name}"),
        AstExpr::Cast { to, .. } => type_name(to),
        _ => "value".to_string(),
    }
}

/// Renders a type in fluent English.
#[must_use]
pub fn type_name(ty: &AstType) -> String {
    match ty {
        AstType::Void => "nothing".to_string(),
        AstType::Bool => "truth value".to_string(),
        AstType::I8 | AstType::I16 | AstType::I32 | AstType::I64 => "whole number".to_string(),
        AstType::U8 | AstType::U16 | AstType::U32 | AstType::U64 => "unsigned number".to_string(),
        AstType::F32 | AstType::F64 => "decimal".to_string(),
        AstType::Ptr(inner) => format!("pointer to {}", type_name(inner)),
        AstType::Array(inner, count) => format!("{count} items of {}", type_name(inner)),
        AstType::Named {
            name, type_args, ..
        } => {
            if type_args.is_empty() {
                name.clone()
            } else {
                format!(
                    "{} of {}",
                    name,
                    type_args
                        .iter()
                        .map(type_name)
                        .collect::<Vec<_>>()
                        .join(" and ")
                )
            }
        }
        AstType::FnPtr { params, ret } => format!(
            "function taking {} returning {}",
            params.iter().map(type_name).collect::<Vec<_>>().join(", "),
            type_name(ret)
        ),
    }
}

/// Renders one phrase pattern with `<param>` placeholders.
#[must_use]
pub fn pattern_text(pattern: &tc_typeck::PhrasePattern) -> String {
    pattern
        .parts
        .iter()
        .map(|part| match part {
            tc_typeck::PhrasePart::Word(word) => word.clone(),
            tc_typeck::PhrasePart::Param(param) => format!("<{param}>"),
        })
        .collect::<Vec<_>>()
        .join(" ")
}
