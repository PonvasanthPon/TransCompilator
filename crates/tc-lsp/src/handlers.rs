//! JSON-RPC request handlers for the TransCompilator English language server.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;

use tc_editor::highlight::{LineState, TokenStyle, highlight_line};
use tc_editor::{Theme, ThemeId};
use tc_lexicon::classify_verb;
use tc_semantics::IntentVerdict;
use tc_typeck::AstModule;

use crate::analysis::{Analysis, Diagnostic, analyze, type_name};
use crate::doc::{Document, Position, TextRange, line_text, path_to_uri, word_at};
use crate::json::Json;
use crate::rpc::{error_response, notification, response};

const STD_MODULES: [(&str, &str); 8] = [
    ("alloc", "dynamic memory allocator and byte accounting"),
    ("core", "Option, Result, panic, and byte utilities"),
    ("fs", "sandboxed file descriptors and text files"),
    ("io", "console output, number formatting, and input"),
    (
        "list",
        "growable lists, sum/average/min/max, contains/find/reverse",
    ),
    ("map", "IntMap and StrMap hash tables"),
    (
        "math",
        "absolute value, min/max, square root, powers, clamp",
    ),
    (
        "string",
        "TcString text bytes, trim, parse, format, starts/ends/contains",
    ),
];

const COMPARATORS: [&str; 18] = [
    "is more than ",
    "is less than ",
    "at least ",
    "at most ",
    "exceeds ",
    "equals ",
    "differs from ",
    "does not equal ",
    "is between ",
    "is even",
    "is odd",
    "is positive",
    "is negative",
    "is zero",
    "is nonzero",
    "is divisible by ",
    "is empty",
    "is not empty",
];

const STANDARD_PHRASES: [&str; 30] = [
    "say <n>",
    "show <n>",
    "print <n>",
    "display <n>",
    "square root of <x>",
    "absolute value of <x>",
    "smaller of <a> and <b>",
    "larger of <a> and <b>",
    "push <val> to <list>",
    "pop from <list>",
    "count of <items>",
    "first item of <items>",
    "last item of <items>",
    "sum of <items>",
    "average of <items>",
    "minimum of <items>",
    "maximum of <items>",
    "reverse <items>",
    "find <target> in <bag>",
    "list <bag> contains <target>",
    "sort <items> using <compare>",
    "trim text <s>",
    "text <s> starts with <prefix>",
    "text <s> ends with <suffix>",
    "text <s> contains <needle>",
    "parse whole number from <s>",
    "format whole number <n> as text",
    "save <s> to file <path>",
    "read text from file <path>",
    "delete file <path>",
];

const STARTERS: [&str; 8] = [
    "Put ... in ...",
    "Let ... be ...",
    "For every ... from ... to ...",
    "Update ... so its ...",
    "To ... , giving ...",
    "A record ... has ...",
    "Give back ...",
    "If ...",
];

/// One server response batch.
#[derive(Debug, Default)]
pub struct Outcome {
    /// Outgoing JSON-RPC messages.
    pub messages: Vec<Json>,
    /// Process exit code when `exit` was received.
    pub exit_code: Option<i32>,
}

/// One in-memory language server.
#[derive(Debug, Default)]
pub struct Server {
    documents: HashMap<String, Document>,
    analyses: HashMap<String, Analysis>,
    shutting_down: bool,
}

impl Server {
    /// Creates an empty server.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Handles one JSON-RPC message.
    pub fn handle(&mut self, message: Json) -> Outcome {
        let method = message.get("method").and_then(Json::as_str);
        let id = message.get("id").cloned();
        let params = message.get("params").cloned().unwrap_or(Json::Null);
        let Some(method) = method else {
            return Outcome::default();
        };
        if method == "exit" {
            return Outcome {
                messages: Vec::new(),
                exit_code: Some(i32::from(!self.shutting_down)),
            };
        }
        if method == "initialize" {
            let Some(id) = id else {
                return Outcome::default();
            };
            return self.reply(id, initialize_result());
        }
        if method == "shutdown" {
            self.shutting_down = true;
            let Some(id) = id else {
                return Outcome::default();
            };
            return self.reply(id, Json::Null);
        }
        if let Some(outcome) = self.notification(method, &params) {
            return outcome;
        }
        let Some(id) = id else {
            return Outcome::default();
        };
        let result = match method {
            "textDocument/hover" => self.hover(&params),
            "textDocument/completion" => self.completion(&params),
            "textDocument/definition" => self.definition(&params),
            "textDocument/documentSymbol" => self.document_symbols(&params),
            "textDocument/semanticTokens/full" => self.semantic_tokens(&params),
            _ => {
                return Outcome {
                    messages: vec![error_response(&id, -32601, "Method not found")],
                    exit_code: None,
                };
            }
        };
        self.reply(id, result.unwrap_or(Json::Null))
    }

    fn reply(&self, id: Json, result: Json) -> Outcome {
        Outcome {
            messages: vec![response(&id, result)],
            exit_code: None,
        }
    }

    fn notification(&mut self, method: &str, params: &Json) -> Option<Outcome> {
        match method {
            "initialized" => Some(Outcome::default()),
            "textDocument/didOpen" => Some(self.did_open(params)),
            "textDocument/didChange" => Some(self.did_change(params)),
            "textDocument/didSave" => Some(self.did_save(params)),
            "textDocument/didClose" => Some(self.did_close(params)),
            _ => None,
        }
    }

    fn did_open(&mut self, params: &Json) -> Outcome {
        let Some(document) = params.get("textDocument") else {
            return Outcome::default();
        };
        let Some(uri) = document.get("uri").and_then(Json::as_str) else {
            return Outcome::default();
        };
        let text = document
            .get("text")
            .and_then(Json::as_str)
            .unwrap_or_default();
        let version = document.get("version").and_then(Json::as_i64).unwrap_or(0);
        let doc = Document::new(uri, text, version);
        let analysis = analyze(uri, &doc.text);
        let messages = vec![diagnostics_notification(uri, &analysis.diagnostics)];
        self.documents.insert(uri.to_string(), doc);
        self.analyses.insert(uri.to_string(), analysis);
        Outcome {
            messages,
            exit_code: None,
        }
    }

    fn did_change(&mut self, params: &Json) -> Outcome {
        let Some(document) = params.get("textDocument") else {
            return Outcome::default();
        };
        let Some(uri) = document.get("uri").and_then(Json::as_str) else {
            return Outcome::default();
        };
        let Some(changes) = params.get("contentChanges").and_then(Json::as_array) else {
            return Outcome::default();
        };
        let Some(change) = changes.last() else {
            return Outcome::default();
        };
        let Some(text) = change.get("text").and_then(Json::as_str) else {
            return Outcome::default();
        };
        let version = document.get("version").and_then(Json::as_i64).unwrap_or(0);
        let doc = Document::new(uri, text, version);
        let analysis = analyze(uri, &doc.text);
        let messages = vec![diagnostics_notification(uri, &analysis.diagnostics)];
        self.documents.insert(uri.to_string(), doc);
        self.analyses.insert(uri.to_string(), analysis);
        Outcome {
            messages,
            exit_code: None,
        }
    }

    fn did_save(&mut self, params: &Json) -> Outcome {
        let Some(uri) = params
            .get("textDocument")
            .and_then(|value| value.get("uri"))
            .and_then(Json::as_str)
        else {
            return Outcome::default();
        };
        let Some(document) = self.documents.get(uri).cloned() else {
            return Outcome::default();
        };
        let analysis = analyze(uri, &document.text);
        let messages = vec![diagnostics_notification(uri, &analysis.diagnostics)];
        self.analyses.insert(uri.to_string(), analysis);
        Outcome {
            messages,
            exit_code: None,
        }
    }

    fn did_close(&mut self, params: &Json) -> Outcome {
        let Some(uri) = params
            .get("textDocument")
            .and_then(|value| value.get("uri"))
            .and_then(Json::as_str)
        else {
            return Outcome::default();
        };
        self.documents.remove(uri);
        self.analyses.remove(uri);
        Outcome {
            messages: vec![diagnostics_notification(uri, &[])],
            exit_code: None,
        }
    }

    fn hover(&mut self, params: &Json) -> Option<Json> {
        let (_uri, document, analysis) = self.context(params)?;
        let position = position_param(params)?;
        let line = line_text(&document.text, position.line);
        let (word, range) = word_at(&document.text, position)?;
        let value = hover_text(&analysis, line, position, &word)?;
        Some(Json::object([
            (
                "contents",
                Json::object([
                    ("kind", Json::string("markdown")),
                    ("value", Json::string(value)),
                ]),
            ),
            ("range", range_json(range)),
        ]))
    }

    fn completion(&mut self, params: &Json) -> Option<Json> {
        let (_uri, document, analysis) = self.context(params)?;
        let position = position_param(params)?;
        let offset = document.offset_at(position);
        let line_start = document.text[..offset]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        let prefix = &document.text[line_start..offset];
        let items = completion_items(&document, &analysis, prefix, position.line);
        Some(Json::object([
            ("isIncomplete", Json::Bool(false)),
            ("items", Json::Array(items)),
        ]))
    }

    fn definition(&mut self, params: &Json) -> Option<Json> {
        let (_uri, document, analysis) = self.context(params)?;
        let position = position_param(params)?;
        let (word, _) = word_at(&document.text, position)?;
        let location = definition_location(&analysis, &document, &word, position.line)?;
        Some(Json::Array(vec![location]))
    }

    fn document_symbols(&mut self, params: &Json) -> Option<Json> {
        let (_uri, _document, analysis) = self.context(params)?;
        let project = analysis.lowered.as_ref()?;
        let module = project
            .program
            .modules
            .iter()
            .find(|module| module.name == analysis.entry)?;
        Some(Json::Array(document_symbols(module)))
    }

    fn semantic_tokens(&mut self, params: &Json) -> Option<Json> {
        let (_uri, document, analysis) = self.context(params)?;
        let mut data = Vec::new();
        let mut previous_line = 0_i64;
        let mut previous_start = 0_i64;
        for (line_index, line) in document.text.split('\n').enumerate() {
            let line_number = line_index as u32;
            let state = line_state(&analysis, line_number);
            let theme = Theme::from_id(ThemeId::Light);
            let spans = highlight_line(line.trim_end_matches('\r'), state, None, &theme);
            let mut start = 0_i64;
            for span in spans {
                let length = span.text.encode_utf16().count() as i64;
                if length == 0 {
                    continue;
                }
                let token_type = token_type(span.kind);
                if token_type.is_none() || span.text.trim().is_empty() {
                    start += length;
                    continue;
                }
                let line_delta = i64::from(line_number) - previous_line;
                let start_delta = if line_delta == 0 {
                    start - previous_start
                } else {
                    start
                };
                data.extend([
                    line_delta,
                    start_delta,
                    length,
                    i64::from(token_type.unwrap_or_default()),
                    0,
                ]);
                previous_line = i64::from(line_number);
                previous_start = start;
                start += length;
            }
        }
        Some(Json::object([(
            "data",
            Json::Array(data.into_iter().map(Json::Number).collect()),
        )]))
    }

    fn context(&mut self, params: &Json) -> Option<(String, Document, Analysis)> {
        let uri = params
            .get("textDocument")
            .and_then(|value| value.get("uri"))
            .and_then(Json::as_str)?
            .to_string();
        let document = self.documents.get(&uri)?.clone();
        if !self.analyses.contains_key(&uri) {
            let analysis = analyze(&uri, &document.text);
            self.analyses.insert(uri.clone(), analysis);
        }
        let analysis = self.analyses.get(&uri)?.clone();
        Some((uri, document, analysis))
    }
}

fn initialize_result() -> Json {
    let token_types = [
        "keyword", "function", "variable", "type", "number", "string", "operator", "comment",
        "property",
    ];
    Json::object([
        (
            "capabilities",
            Json::object([
                ("textDocumentSync", Json::Number(1)),
                ("hoverProvider", Json::Bool(true)),
                (
                    "completionProvider",
                    Json::object([(
                        "triggerCharacters",
                        Json::Array(
                            [" ", "'", "."]
                                .into_iter()
                                .map(Json::string)
                                .collect::<Vec<_>>(),
                        ),
                    )]),
                ),
                ("definitionProvider", Json::Bool(true)),
                ("documentSymbolProvider", Json::Bool(true)),
                (
                    "semanticTokensProvider",
                    Json::object([
                        (
                            "legend",
                            Json::object([
                                (
                                    "tokenTypes",
                                    Json::Array(
                                        token_types.into_iter().map(Json::string).collect(),
                                    ),
                                ),
                                (
                                    "tokenModifiers",
                                    Json::Array(
                                        ["declaration", "readonly"]
                                            .into_iter()
                                            .map(Json::string)
                                            .collect(),
                                    ),
                                ),
                            ]),
                        ),
                        ("full", Json::Bool(true)),
                    ]),
                ),
            ]),
        ),
        (
            "serverInfo",
            Json::object([
                ("name", Json::string("tc-lsp")),
                ("version", Json::string(env!("CARGO_PKG_VERSION"))),
            ]),
        ),
    ])
}

fn position_param(params: &Json) -> Option<Position> {
    let position = params.get("position")?;
    Some(Position::new(
        u32::try_from(position.get("line")?.as_i64()?).ok()?,
        u32::try_from(position.get("character")?.as_i64()?).ok()?,
    ))
}

fn range_json(range: TextRange) -> Json {
    Json::object([
        (
            "start",
            Json::object([
                ("line", Json::Number(i64::from(range.start.line))),
                ("character", Json::Number(i64::from(range.start.character))),
            ]),
        ),
        (
            "end",
            Json::object([
                ("line", Json::Number(i64::from(range.end.line))),
                ("character", Json::Number(i64::from(range.end.character))),
            ]),
        ),
    ])
}

fn diagnostics_notification(uri: &str, diagnostics: &[Diagnostic]) -> Json {
    notification(
        "textDocument/publishDiagnostics",
        Json::object([
            ("uri", Json::string(uri)),
            (
                "diagnostics",
                Json::Array(diagnostics.iter().map(diagnostic_json).collect()),
            ),
        ]),
    )
}

fn diagnostic_json(diagnostic: &Diagnostic) -> Json {
    let range = TextRange::new(
        Position::new(diagnostic.line, diagnostic.character),
        Position::new(diagnostic.line, diagnostic.character.saturating_add(1)),
    );
    Json::object([
        ("range", range_json(range)),
        ("severity", Json::Number(i64::from(diagnostic.severity))),
        ("source", Json::string(&diagnostic.source)),
        ("message", Json::string(&diagnostic.message)),
    ])
}

fn hover_text(analysis: &Analysis, line: &str, position: Position, word: &str) -> Option<String> {
    if let Some(project) = &analysis.lowered {
        if let Some(entry) = project
            .trace
            .iter()
            .find(|entry| entry.module == analysis.entry && entry.line == position.line + 1)
        {
            if let IntentVerdict::CommentOmitted { reason } = &entry.intent {
                return Some(format!("*Omitted prose (COMMENT_OMITTED)* — {reason}"));
            }
            let resolutions = project
                .paraphrase
                .iter()
                .find(|doc| doc.module == analysis.entry)
                .map(|doc| {
                    doc.entries
                        .iter()
                        .filter(|candidate| candidate.span.line == position.line + 1)
                        .flat_map(|candidate| candidate.resolutions.iter().cloned())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let relevant = resolutions
                .into_iter()
                .filter(|resolution| {
                    let lower = resolution.to_ascii_lowercase();
                    (lower.starts_with("[it ")
                        || lower.starts_with("[its ")
                        || lower.starts_with("[the ")
                        || lower.starts_with("[their "))
                        && (word == "it"
                            || word == "its"
                            || line
                                .to_ascii_lowercase()
                                .contains(&word.to_ascii_lowercase()))
                })
                .collect::<Vec<_>>();
            if !relevant.is_empty() {
                return Some(format!(
                    "**ACE Anaphor**: {}",
                    relevant
                        .into_iter()
                        .map(|value| format!("`{value}`"))
                        .collect::<Vec<_>>()
                        .join(" ")
                ));
            }
        }
    }
    if let Some(resolution) = classify_verb(word, None) {
        let mut text = format!("**Computational verb** `{}`", resolution.lemma);
        if let Some(predicate) = resolution.predicate {
            let _ = write!(text, "\n\n- VerbNet predicate: `{predicate:?}`");
        }
        if let Some(concept) = resolution.concept {
            let _ = write!(text, "\n- Semantic concept: `{concept:?}`");
        }
        if let Some(class_id) = resolution.class_id {
            let _ = write!(text, "\n- VerbNet class: `{class_id}`");
        }
        if let Some(synset) = resolution.synset_id {
            let _ = write!(text, "\n- WordNet synset: `{synset}`");
        }
        return Some(text);
    }
    if let Some(value) = analysis.symbols.type_named(word) {
        let mut text = format!(
            "**{}** `{}`",
            if value.kind == "record" {
                "Record"
            } else {
                "Union"
            },
            value.name
        );
        if !value.fields.is_empty() {
            text.push_str("\n\n");
            for field in &value.fields {
                let _ = writeln!(text, "- `{}` ({})", field.name, field.ty);
            }
        }
        return Some(text);
    }
    if let Some(value) = analysis.symbols.variable_named(word, position.line) {
        return Some(format!(
            "**Variable** `{}`\n\nType: `{}`",
            value.name, value.ty
        ));
    }
    if let Some(value) = analysis.symbols.function_named(word) {
        let params = value
            .params
            .iter()
            .map(|(name, ty)| format!("{name} ({ty})"))
            .collect::<Vec<_>>()
            .join(", ");
        let mut text = format!(
            "**Function** `{}`\n\nSignature: `({params}) -> {}`",
            value.name, value.ret
        );
        if !value.phrases.is_empty() {
            text.push_str("\n\nPhrases:\n");
            for phrase in &value.phrases {
                let _ = writeln!(text, "- `{phrase}`");
            }
        }
        return Some(text);
    }
    if let Some(value) = analysis
        .symbols
        .globals
        .iter()
        .find(|value| value.name == word)
    {
        return Some(format!(
            "**Shared global** `{}`\n\nType: `{}`",
            value.name, value.ty
        ));
    }
    for value in &analysis.symbols.types {
        if let Some(field) = value.fields.iter().find(|field| field.name == word) {
            return Some(format!(
                "**Field** `{}.{}`\n\nType: `{}`",
                value.name, field.name, field.ty
            ));
        }
    }
    None
}

fn completion_items(
    document: &Document,
    analysis: &Analysis,
    prefix: &str,
    line: u32,
) -> Vec<Json> {
    let trimmed = prefix.trim_start().to_ascii_lowercase();
    let first = trimmed.split_whitespace().next().unwrap_or_default();
    if first == "use" || first == "import" {
        let mut items = STD_MODULES
            .iter()
            .map(|(name, detail)| completion_item(name, 9, Some(detail)))
            .collect::<Vec<_>>();
        if let Some(parent) = document.path.parent() {
            if let Ok(entries) = std::fs::read_dir(parent) {
                let mut siblings = entries
                    .filter_map(Result::ok)
                    .filter_map(|entry| {
                        let path = entry.path();
                        (path.extension().and_then(|value| value.to_str()) == Some("eng"))
                            .then(|| {
                                path.file_stem()
                                    .and_then(|value| value.to_str())
                                    .map(str::to_string)
                            })
                            .flatten()
                    })
                    .collect::<Vec<_>>();
                siblings.sort();
                for name in siblings {
                    items.push(completion_item(&name, 9, Some("sibling .eng module")));
                }
            }
        }
        return dedup_items(items);
    }
    if let Some(items) = possessive_items(analysis, prefix, line) {
        return items;
    }
    if (trimmed.starts_with("if ")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("until "))
        && !trimmed.ends_with(" is ")
    {
        return COMPARATORS
            .iter()
            .map(|value| completion_item(value.trim(), 12, Some("English comparator")))
            .collect();
    }
    let mut items = Vec::new();
    for variable in &analysis.symbols.variables {
        items.push(completion_item(
            &variable.name.replace('_', " "),
            6,
            Some(&variable.ty),
        ));
    }
    for ty in &analysis.symbols.types {
        items.push(completion_item(&ty.name, 22, Some(ty.kind)));
    }
    for phrase in STANDARD_PHRASES
        .iter()
        .copied()
        .chain(analysis.symbols.phrases.iter().map(String::as_str))
    {
        items.push(completion_item(phrase, 3, Some("phrase template")));
    }
    for starter in STARTERS {
        items.push(completion_item(starter, 14, Some("English starter")));
    }
    dedup_items(items)
}

fn possessive_items(analysis: &Analysis, prefix: &str, line: u32) -> Option<Vec<Json>> {
    let trimmed = prefix.trim_end();
    let lower = trimmed.to_ascii_lowercase();
    let base = if lower.ends_with("its") {
        analysis
            .symbols
            .variables
            .iter()
            .filter(|value| value.line <= line + 1)
            .find(|value| analysis.symbols.type_named(&value.ty).is_some())
            .map(|value| value.ty.clone())
    } else if lower.ends_with("'s") || lower.ends_with("\u{2019}s") || lower.ends_with('.') {
        let position = lower
            .rfind("'s")
            .or_else(|| lower.rfind("\u{2019}s"))
            .or_else(|| lower.rfind('.'))?;
        let before = trimmed[..position].trim();
        let base = before
            .split_whitespace()
            .last()
            .unwrap_or_default()
            .trim_start_matches("the ");
        analysis
            .symbols
            .variable_named(base, line)
            .map(|value| value.ty.clone())
            .or_else(|| {
                analysis
                    .symbols
                    .type_named(base)
                    .map(|value| value.name.clone())
            })
    } else {
        return None;
    }?;
    let ty = analysis.symbols.type_named(&base)?;
    Some(
        ty.fields
            .iter()
            .map(|field| {
                let mut item = completion_item(&field.name, 5, Some(&field.ty));
                if let Some(object) = object_mut(&mut item) {
                    object.insert(
                        "labelDetails".to_string(),
                        Json::object([("detail", Json::string(format!(" ({})", field.ty)))]),
                    );
                }
                item
            })
            .collect(),
    )
}

fn object_mut(value: &mut Json) -> Option<&mut BTreeMap<String, Json>> {
    match value {
        Json::Object(object) => Some(object),
        _ => None,
    }
}

fn completion_item(label: &str, kind: i64, detail: Option<&str>) -> Json {
    let mut entries = vec![
        ("label".to_string(), Json::string(label)),
        ("kind".to_string(), Json::Number(kind)),
    ];
    if let Some(detail) = detail {
        entries.push(("detail".to_string(), Json::string(detail)));
    }
    Json::object(entries)
}

fn dedup_items(items: Vec<Json>) -> Vec<Json> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter(|item| {
            item.get("label")
                .and_then(Json::as_str)
                .is_none_or(|label| seen.insert(label.to_string()))
        })
        .collect()
}

fn definition_location(
    analysis: &Analysis,
    document: &Document,
    word: &str,
    line: u32,
) -> Option<Json> {
    let symbol = analysis
        .symbols
        .variable_named(word, line)
        .map(|value| (value.module.clone(), value.line, value.col))
        .or_else(|| {
            analysis
                .symbols
                .function_named(word)
                .map(|value| (value.module.clone(), value.line, value.col))
        })
        .or_else(|| {
            analysis
                .symbols
                .type_named(word)
                .map(|value| (value.module.clone(), value.line, value.col))
        })
        .or_else(|| {
            analysis
                .symbols
                .globals
                .iter()
                .find(|value| value.name == word)
                .map(|value| (value.module.clone(), value.line, value.col))
        })
        .or_else(|| {
            analysis
                .symbols
                .functions
                .iter()
                .find(|value| {
                    value.phrases.iter().any(|phrase| {
                        phrase
                            .split_whitespace()
                            .next()
                            .is_some_and(|first| first == word)
                    })
                })
                .map(|value| (value.module.clone(), value.line, value.col))
        })?;
    let (module, line, col) = symbol;
    let uri = analysis
        .paths
        .get(&module)
        .map_or_else(|| document.uri.clone(), |path| path_to_uri(path));
    let position = Position::new(line.saturating_sub(1), col.saturating_sub(1));
    let range = TextRange::new(
        position,
        Position::new(position.line, position.character + 1),
    );
    Some(Json::object([
        ("uri", Json::string(uri)),
        ("range", range_json(range)),
    ]))
}

fn document_symbols(module: &AstModule) -> Vec<Json> {
    let mut symbols = Vec::new();
    for definition in &module.structs {
        symbols.push(symbol_json(
            &definition.name,
            "record",
            23,
            definition.span.line,
            definition.span.col,
        ));
    }
    for definition in &module.unions {
        symbols.push(symbol_json(
            &definition.name,
            "union",
            23,
            definition.span.line,
            definition.span.col,
        ));
    }
    for global in &module.globals {
        symbols.push(symbol_json(
            &global.name,
            &type_name(&global.ty),
            13,
            global.span.line,
            global.span.col,
        ));
    }
    for function in &module.functions {
        symbols.push(symbol_json(
            &function.name,
            "function",
            12,
            function.span.line,
            function.span.col,
        ));
    }
    symbols
}

fn symbol_json(name: &str, detail: &str, kind: i64, line: u32, col: u32) -> Json {
    let start = Position::new(line.saturating_sub(1), col.saturating_sub(1));
    let range = TextRange::new(
        start,
        Position::new(start.line, start.character + name.len() as u32),
    );
    Json::object([
        (
            "name",
            Json::string(if name.is_empty() { detail } else { name }),
        ),
        ("detail", Json::string(detail)),
        ("kind", Json::Number(kind)),
        ("range", range_json(range)),
        ("selectionRange", range_json(range)),
    ])
}

fn line_state(analysis: &Analysis, line: u32) -> LineState {
    if analysis.diagnostics.iter().any(|value| value.line == line) {
        return LineState::Error;
    }
    if analysis.lowered.as_ref().is_some_and(|project| {
        project.trace.iter().any(|entry| {
            entry.module == analysis.entry && entry.line == line + 1 && entry.intent.is_comment()
        })
    }) {
        return LineState::ProseOmitted;
    }
    LineState::Normal
}

fn token_type(style: TokenStyle) -> Option<u32> {
    match style {
        TokenStyle::ControlAnchor => Some(0),
        TokenStyle::ActionVerb => Some(1),
        TokenStyle::AnaphorOrQuantifier | TokenStyle::Identifier => Some(2),
        TokenStyle::Type => Some(3),
        TokenStyle::Number => Some(4),
        TokenStyle::String => Some(5),
        TokenStyle::Operator => Some(6),
        TokenStyle::Omitted => Some(7),
        TokenStyle::Muted | TokenStyle::SearchMatch => None,
    }
}
