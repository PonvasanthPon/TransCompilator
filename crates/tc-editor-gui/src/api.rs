//! REST API handlers for the R1 Editor GUI.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use tc_editor::highlight::{LineState, TokenStyle, highlight_line};
use tc_editor::runner::{
    PRESETS, RunResult, build_elf_artifact, run_elf_with_parity, run_ir, run_paraphrase, run_trace,
};
use tc_editor::{Theme, ThemeId};

use crate::http::{Request, Response};
use crate::json::Json;

const FRONTEND: &str = include_str!("frontend.html");

/// Shared API context.
#[derive(Debug, Clone)]
pub struct ApiContext {
    workspace: PathBuf,
    std_dir: Option<PathBuf>,
}

impl ApiContext {
    /// Creates an API context rooted at a workspace directory.
    #[must_use]
    pub fn new(workspace: &Path) -> Self {
        let workspace = workspace
            .canonicalize()
            .unwrap_or_else(|_| workspace.to_path_buf());
        Self {
            workspace,
            std_dir: find_std_dir(),
        }
    }

    /// Workspace directory.
    #[must_use]
    pub fn workspace(&self) -> &Path {
        &self.workspace
    }
}

fn find_std_dir() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("TC_STD_DIR") {
        let path = PathBuf::from(value);
        if path.join("core.eng").is_file() {
            return Some(path);
        }
    }
    if let Ok(executable) = std::env::current_exe() {
        let mut directory = executable.parent().map(Path::to_path_buf);
        for _ in 0..4 {
            if let Some(current) = &directory {
                let candidate = current.join("std");
                if candidate.join("core.eng").is_file() {
                    return Some(candidate);
                }
                directory = current.parent().map(Path::to_path_buf);
            }
        }
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../std");
    manifest
        .join("core.eng")
        .is_file()
        .then(|| manifest.canonicalize().unwrap_or(manifest))
}

fn error_response(status: u16, message: impl Into<String>) -> Response {
    Response::json(
        status,
        &Json::object([("ok", Json::Bool(false)), ("error", Json::string(message))]),
    )
}

fn safe_path(workspace: &Path, raw: &str) -> Option<PathBuf> {
    if raw.trim().is_empty() {
        return None;
    }
    let candidate = Path::new(raw);
    if candidate.is_absolute() {
        return None;
    }
    let mut relative = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(value) => relative.push(value),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(workspace.join(relative))
}

fn body_object(request: &Request) -> Result<Json, Response> {
    let value = request
        .json()
        .map_err(|message| error_response(400, format!("malformed JSON: {message}")))?;
    if !matches!(value, Json::Object(_)) {
        return Err(error_response(400, "JSON body must be an object"));
    }
    Ok(value)
}

fn string_member(body: &Json, key: &str) -> String {
    body.get(key)
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string()
}

fn lines_member(body: &Json, key: &str) -> Vec<usize> {
    body.get(key)
        .and_then(Json::as_array)
        .map(|values| values.iter().filter_map(Json::as_usize).collect())
        .unwrap_or_default()
}

fn result_json(result: &RunResult) -> Json {
    Json::object([
        ("ok", Json::Bool(result.ok)),
        ("summary", Json::string(&result.summary)),
        (
            "lines",
            Json::Array(result.lines.iter().map(Json::string).collect::<Vec<_>>()),
        ),
        (
            "error_lines",
            Json::Array(
                result
                    .error_lines
                    .iter()
                    .map(|line| Json::uint(*line))
                    .collect(),
            ),
        ),
        (
            "omitted_lines",
            Json::Array(
                result
                    .omitted_lines
                    .iter()
                    .map(|line| Json::uint(*line))
                    .collect(),
            ),
        ),
    ])
}

fn kind_name(kind: TokenStyle) -> &'static str {
    match kind {
        TokenStyle::ControlAnchor => "control",
        TokenStyle::ActionVerb => "verb",
        TokenStyle::AnaphorOrQuantifier => "anaphor",
        TokenStyle::Type => "type",
        TokenStyle::Number => "number",
        TokenStyle::String => "string",
        TokenStyle::Operator => "operator",
        TokenStyle::Identifier => "ident",
        TokenStyle::Muted => "muted",
        TokenStyle::Omitted => "prose",
        TokenStyle::SearchMatch => "search",
    }
}

fn highlight_response(body: &Json) -> Response {
    let source = string_member(body, "source");
    let error_lines = lines_member(body, "error_lines");
    let omitted_lines = lines_member(body, "omitted_lines");
    let theme = Theme::from_id(ThemeId::Midnight);
    let lines = source
        .lines()
        .enumerate()
        .map(|(index, line)| {
            let number = index + 1;
            let state = if error_lines.contains(&number) {
                LineState::Error
            } else if omitted_lines.contains(&number) {
                LineState::ProseOmitted
            } else {
                LineState::Normal
            };
            Json::Array(
                highlight_line(line, state, None, &theme)
                    .into_iter()
                    .map(|span| {
                        Json::object([
                            ("text", Json::string(span.text)),
                            ("kind", Json::string(kind_name(span.kind))),
                        ])
                    })
                    .collect(),
            )
        })
        .collect::<Vec<_>>();
    Response::json(200, &Json::object([("lines", Json::Array(lines))]))
}

fn list_workspace_files(context: &ApiContext) -> Vec<String> {
    let mut files = Vec::new();
    let workspace = &context.workspace;
    if let Ok(entries) = fs::read_dir(workspace) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|value| value.to_str()) == Some("eng") {
                if let Some(name) = path.file_name().and_then(|value| value.to_str()) {
                    files.push(name.to_string());
                }
            } else if path.is_dir() {
                if let Ok(nested) = fs::read_dir(&path) {
                    for child in nested.filter_map(Result::ok) {
                        let child_path = child.path();
                        if child_path.is_file()
                            && child_path.extension().and_then(|value| value.to_str())
                                == Some("eng")
                        {
                            if let Ok(relative) = child_path.strip_prefix(workspace) {
                                files.push(relative.to_string_lossy().to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    files.sort();
    files
}

fn std_modules(context: &ApiContext) -> Vec<Json> {
    let Some(directory) = &context.std_dir else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("eng"))
        .collect::<Vec<_>>();
    paths.sort();
    let mut modules = Vec::new();
    for path in paths {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("unknown.eng")
            .to_string();
        let mut functions = Vec::new();
        let mut current: Option<(String, Vec<String>)> = None;
        for line in text.lines() {
            let trimmed = line.trim();
            if let Some(signature) = trimmed.strip_prefix("Public function ") {
                if let Some((name, phrases)) = current.take() {
                    functions.push(Json::object([
                        ("signature", Json::string(name)),
                        (
                            "phrases",
                            Json::Array(phrases.into_iter().map(Json::string).collect()),
                        ),
                    ]));
                }
                current = Some((signature.trim_end_matches(':').to_string(), Vec::new()));
            } else if let Some(phrase) = trimmed.strip_prefix("Phrase:") {
                if let Some((_, phrases)) = &mut current {
                    phrases.push(phrase.trim().to_string());
                }
            }
        }
        if let Some((name, phrases)) = current.take() {
            functions.push(Json::object([
                ("signature", Json::string(name)),
                (
                    "phrases",
                    Json::Array(phrases.into_iter().map(Json::string).collect()),
                ),
            ]));
        }
        modules.push(Json::object([
            ("name", Json::string(name)),
            ("functions", Json::Array(functions)),
        ]));
    }
    modules
}

fn presets_json() -> Json {
    Json::Array(
        PRESETS
            .iter()
            .map(|preset| {
                Json::object([
                    ("name", Json::string(preset.name)),
                    ("description", Json::string(preset.description)),
                    ("source", Json::string(preset.source)),
                ])
            })
            .collect(),
    )
}

fn workspace_path(context: &ApiContext, body: &Json) -> Result<PathBuf, Response> {
    let raw = string_member(body, "path");
    safe_path(&context.workspace, &raw)
        .ok_or_else(|| error_response(400, "path must stay inside the workspace directory"))
}

fn handle_file_open(context: &ApiContext, request: &Request) -> Response {
    let body = match body_object(request) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let path = match workspace_path(context, &body) {
        Ok(value) => value,
        Err(response) => return response,
    };
    match fs::read_to_string(&path) {
        Ok(file_text) => Response::json(
            200,
            &Json::object([
                ("ok", Json::Bool(true)),
                ("path", Json::string(path.to_string_lossy())),
                ("content", Json::string(file_text)),
            ]),
        ),
        Err(error) => error_response(400, format!("cannot open file: {error}")),
    }
}

fn handle_file_save(context: &ApiContext, request: &Request) -> Response {
    let body = match body_object(request) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let mut path = match workspace_path(context, &body) {
        Ok(value) => value,
        Err(response) => return response,
    };
    if path.extension().is_none() {
        path.set_extension("eng");
    }
    if path.extension().and_then(|value| value.to_str()) != Some("eng") {
        return error_response(400, "only .eng files may be saved");
    }
    let file_text = string_member(&body, "content");
    let mut temporary = path.as_os_str().to_os_string();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    if let Err(error) = fs::write(&temporary, file_text.as_bytes()) {
        return error_response(500, format!("cannot write file: {error}"));
    }
    if let Err(error) = fs::rename(&temporary, &path) {
        return error_response(500, format!("cannot finalize file: {error}"));
    }
    Response::json(
        200,
        &Json::object([
            ("ok", Json::Bool(true)),
            ("path", Json::string(path.to_string_lossy())),
        ]),
    )
}

fn runner_path(context: &ApiContext, body: &Json) -> Option<PathBuf> {
    let raw = string_member(body, "file_path");
    if raw.is_empty() {
        None
    } else {
        safe_path(&context.workspace, &raw)
    }
}

fn run_action(context: &ApiContext, request: &Request, action: &str) -> Response {
    let body = match body_object(request) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let source = string_member(&body, "source");
    let file_path = runner_path(context, &body);
    let result = match action {
        "run" => {
            let stdin = string_member(&body, "stdin");
            run_elf_with_parity(&source, file_path.as_deref(), &stdin)
        }
        "paraphrase" => run_paraphrase(&source, file_path.as_deref()),
        "trace" => run_trace(&source, file_path.as_deref()),
        "ir" => run_ir(&source, file_path.as_deref()),
        "build" => build_elf_artifact(&source, file_path.as_deref()),
        _ => return error_response(404, "unknown action"),
    };
    Response::json(200, &result_json(&result))
}

/// Dispatches one HTTP request.
#[must_use]
pub fn handle(context: &ApiContext, request: &Request) -> Response {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") => Response::html(FRONTEND),
        ("GET", "/api/health") => Response::json(
            200,
            &Json::object([
                ("status", Json::string("ok")),
                ("app", Json::string("R1 Editor GUI")),
                ("version", Json::string(env!("CARGO_PKG_VERSION"))),
                (
                    "workspace",
                    Json::string(context.workspace.to_string_lossy()),
                ),
            ]),
        ),
        ("GET", "/api/presets") => Response::json(200, &presets_json()),
        ("GET", "/api/std") => Response::json(
            200,
            &Json::object([("modules", Json::Array(std_modules(context)))]),
        ),
        ("GET", "/api/files") => Response::json(
            200,
            &Json::object([(
                "files",
                Json::Array(
                    list_workspace_files(context)
                        .into_iter()
                        .map(Json::string)
                        .collect(),
                ),
            )]),
        ),
        ("POST", "/api/file/open") => handle_file_open(context, request),
        ("POST", "/api/file/save") => handle_file_save(context, request),
        ("POST", "/api/highlight") => match body_object(request) {
            Ok(body) => highlight_response(&body),
            Err(response) => response,
        },
        ("POST", "/api/run") => run_action(context, request, "run"),
        ("POST", "/api/paraphrase") => run_action(context, request, "paraphrase"),
        ("POST", "/api/trace") => run_action(context, request, "trace"),
        ("POST", "/api/ir") => run_action(context, request, "ir"),
        ("POST", "/api/build") => run_action(context, request, "build"),
        (_, "/" | "/api/health" | "/api/presets" | "/api/std" | "/api/files") => {
            error_response(405, "method not allowed")
        }
        _ => error_response(404, "not found"),
    }
}

/// Returns a deterministic map of std module names to file names for tests.
#[must_use]
pub fn std_inventory(context: &ApiContext) -> BTreeMap<String, String> {
    std_modules(context)
        .into_iter()
        .filter_map(|module| {
            let name = module.get("name")?.as_str()?.to_string();
            Some((name.clone(), name))
        })
        .collect()
}
