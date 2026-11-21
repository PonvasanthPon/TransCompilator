//! Headless HTTP API tests for the standalone R1 Editor GUI.

use std::fs;
use std::io::{Read as _, Write as _};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tc_editor::PRESETS;
use tc_editor_gui::GuiServer;
use tc_editor_gui::json::{Json, parse, stringify};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "r1_gui_test_{}_{}_{tag}",
        std::process::id(),
        counter
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("workspace");
    path
}

fn start(tag: &str) -> (GuiServer, PathBuf) {
    let workspace = temp_dir(tag);
    let server = GuiServer::start("127.0.0.1:0", &workspace).expect("server");
    (server, workspace)
}

fn request(server: &GuiServer, method: &str, path: &str, body: Option<&Json>) -> (u16, String) {
    let body_text = body.map(stringify).unwrap_or_default();
    let mut stream = TcpStream::connect(server.address()).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("timeout");
    let request_text = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body_text.len(),
        body_text
    );
    stream
        .write_all(request_text.as_bytes())
        .expect("write request");
    let mut response = Vec::new();
    stream.read_to_end(&mut response).expect("read response");
    let text = String::from_utf8_lossy(&response).to_string();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(0);
    let body = text
        .split_once("\r\n\r\n")
        .map_or_else(String::new, |(_, body)| body.to_string());
    (status, body)
}

fn json_body(body: &str) -> Json {
    parse(body).expect("JSON response")
}

fn source_body(source: &str) -> Json {
    Json::object([("source", Json::string(source))])
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

#[test]
fn gui_get_root_serves_offline_html_no_cdn() {
    let (server, workspace) = start("root");
    let (status, body) = request(&server, "GET", "/", None);
    assert_eq!(status, 200);
    assert!(body.contains("R1 Editor"));
    assert!(!body.contains("http://"));
    assert!(!body.contains("https://"));
    assert!(!body.contains("cdn."));
    drop(server);
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn gui_health_endpoint() {
    let (server, workspace) = start("health");
    let (status, body) = request(&server, "GET", "/api/health", None);
    assert_eq!(status, 200);
    let value = json_body(&body);
    assert_eq!(value.get("status").and_then(Json::as_str), Some("ok"));
    assert_eq!(
        value.get("app").and_then(Json::as_str),
        Some("R1 Editor GUI")
    );
    drop(server);
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn gui_presets_returns_all_8_presets() {
    let (server, workspace) = start("presets");
    let (status, body) = request(&server, "GET", "/api/presets", None);
    assert_eq!(status, 200);
    let value = json_body(&body);
    let presets = value.as_array().expect("presets array");
    assert_eq!(presets.len(), 8);
    assert_eq!(
        presets[7].get("name").and_then(Json::as_str),
        Some("08_std_file_and_map.eng")
    );
    drop(server);
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn gui_std_endpoint_lists_all_8_std_modules() {
    let (server, workspace) = start("std");
    let (status, body) = request(&server, "GET", "/api/std", None);
    assert_eq!(status, 200);
    let value = json_body(&body);
    let modules = value
        .get("modules")
        .and_then(Json::as_array)
        .expect("modules");
    assert_eq!(modules.len(), 8);
    let names = modules
        .iter()
        .filter_map(|module| module.get("name").and_then(Json::as_str))
        .collect::<Vec<_>>();
    for expected in [
        "core.eng",
        "alloc.eng",
        "io.eng",
        "string.eng",
        "list.eng",
        "fs.eng",
        "math.eng",
        "map.eng",
    ] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    let text = stringify(&value);
    assert!(text.contains("heap_alloc"));
    assert!(text.contains("say"));
    drop(server);
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn gui_run_all_8_presets_pass_exact_parity() {
    let (server, workspace) = start("run-all");
    for preset in &PRESETS {
        let body = post_json_preset(preset.source);
        let (status, response) = request(&server, "POST", "/api/run", Some(&body));
        assert_eq!(status, 200, "{}", preset.name);
        let value = json_body(&response);
        assert_eq!(
            value.get("ok").and_then(Json::as_bool),
            Some(true),
            "{}: {}",
            preset.name,
            response
        );
        assert!(
            value
                .get("summary")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .contains("PARITY: EXACT MATCH"),
            "{}: {response}",
            preset.name
        );
    }
    drop(server);
    let _ = fs::remove_dir_all(workspace);
}

fn post_json_preset(source: &str) -> Json {
    source_body(source)
}

#[test]
fn gui_paraphrase_endpoint_returns_annotations() {
    let (server, workspace) = start("paraphrase");
    let source = "Put 500 in cash.\nAdd 100 to it.\nReturn cash.\n";
    let (status, body) = request(
        &server,
        "POST",
        "/api/paraphrase",
        Some(&source_body(source)),
    );
    assert_eq!(status, 200);
    let value = json_body(&body);
    assert_eq!(value.get("ok").and_then(Json::as_bool), Some(true));
    let lines = value
        .get("lines")
        .and_then(Json::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(Json::as_str)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(lines.contains("[it -> cash]"), "{lines}");
    drop(server);
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn gui_trace_endpoint_returns_lexicon_provenance() {
    let (server, workspace) = start("trace");
    let source = "Put 10 in x.\nYesterday Ravi called me twice.\nReturn x.\n";
    let (status, body) = request(&server, "POST", "/api/trace", Some(&source_body(source)));
    assert_eq!(status, 200);
    let value = json_body(&body);
    let lines = value
        .get("lines")
        .and_then(Json::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(Json::as_str)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(lines.contains("EXEC"), "{lines}");
    assert!(lines.contains("COMMENT_OMITTED"), "{lines}");
    assert!(
        lines.contains("wordnet=") || lines.contains("verbnet="),
        "{lines}"
    );
    drop(server);
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn gui_ir_endpoint_returns_core7_ir() {
    let (server, workspace) = start("ir");
    let (status, body) = request(
        &server,
        "POST",
        "/api/ir",
        Some(&source_body("Return 42.\n")),
    );
    assert_eq!(status, 200);
    let value = json_body(&body);
    let lines = value
        .get("lines")
        .and_then(Json::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(Json::as_str)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(lines.contains("tc_fn"), "{lines}");
    assert!(lines.contains("return"), "{lines}");
    drop(server);
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn gui_highlight_endpoint_uses_tc_lexicon() {
    let (server, workspace) = start("highlight");
    let body = Json::object([
        ("source", Json::string("If Remove 42 from i64\")")),
        ("error_lines", Json::array(vec![])),
        ("omitted_lines", Json::array(vec![])),
    ]);
    let (status, response) = request(&server, "POST", "/api/highlight", Some(&body));
    assert_eq!(status, 200);
    let value = json_body(&response);
    let lines = value.get("lines").and_then(Json::as_array).expect("lines");
    let kinds = lines[0]
        .as_array()
        .unwrap_or_default()
        .iter()
        .filter_map(|span| span.get("kind").and_then(Json::as_str))
        .collect::<Vec<_>>();
    assert!(kinds.contains(&"control"));
    assert!(kinds.contains(&"verb"));
    assert!(kinds.contains(&"number"));
    assert!(kinds.contains(&"type"));
    drop(server);
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn gui_file_save_appends_eng_and_open_roundtrip() {
    let (server, workspace) = start("files");
    let save = Json::object([
        ("path", Json::string("demo")),
        ("content", Json::string("Return 7.\n")),
    ]);
    let (status, _body) = request(&server, "POST", "/api/file/save", Some(&save));
    assert_eq!(status, 200);
    assert!(workspace.join("demo.eng").is_file());
    let open = Json::object([("path", Json::string("demo.eng"))]);
    let (status, body) = request(&server, "POST", "/api/file/open", Some(&open));
    assert_eq!(status, 200);
    let value = json_body(&body);
    assert_eq!(
        value.get("content").and_then(Json::as_str),
        Some("Return 7.\n")
    );
    let (status, body) = request(&server, "GET", "/api/files", None);
    assert_eq!(status, 200);
    assert!(
        json_body(&body)
            .get("files")
            .and_then(Json::as_array)
            .unwrap_or_default()
            .iter()
            .filter_map(Json::as_str)
            .any(|name| name == "demo.eng")
    );
    drop(server);
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn gui_file_save_blocks_path_traversal_outside_workspace() {
    let (server, workspace) = start("traversal");
    for path in ["../../etc/passwd", "/etc/passwd", "../escape.eng"] {
        let body = Json::object([
            ("path", Json::string(path)),
            ("content", Json::string("bad")),
        ]);
        let (status, _) = request(&server, "POST", "/api/file/save", Some(&body));
        assert_eq!(status, 400, "{path}");
    }
    drop(server);
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn gui_build_endpoint_emits_static_zero_libc_elf() {
    let (server, workspace) = start("build");
    let body = Json::object([
        ("source", Json::string("Import io.\nSay 42.\nReturn 0.\n")),
        ("file_path", Json::string("prog.eng")),
    ]);
    let (status, response) = request(&server, "POST", "/api/build", Some(&body));
    assert_eq!(status, 200);
    let value = json_body(&response);
    assert_eq!(value.get("ok").and_then(Json::as_bool), Some(true));
    let elf = workspace.join("prog.elf");
    assert!(elf.is_file());
    let readelf = Command::new("readelf")
        .arg("-d")
        .arg(&elf)
        .output()
        .expect("readelf");
    assert!(String::from_utf8_lossy(&readelf.stdout).contains("There is no dynamic section"));
    drop(server);
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn gui_malformed_json_returns_400_no_panic() {
    let (server, workspace) = start("malformed");
    let (status, body) = request(&server, "POST", "/api/run", Some(&Json::Null));
    assert_eq!(status, 400);
    assert!(body.contains("JSON"));
    let mut stream = TcpStream::connect(server.address()).expect("connect");
    let raw = b"POST /api/run HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: 4\r\nConnection: close\r\n\r\n{bad";
    stream.write_all(raw).expect("write");
    let mut response = Vec::new();
    stream.read_to_end(&mut response).expect("read");
    let text = String::from_utf8_lossy(&response);
    assert!(text.starts_with("HTTP/1.1 400"), "{text}");
    drop(server);
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn gui_run_unresolved_anaphor_returns_error_lines() {
    let (server, workspace) = start("unresolved");
    let (status, body) = request(
        &server,
        "POST",
        "/api/run",
        Some(&source_body("Add 10 to it.\n")),
    );
    assert_eq!(status, 200);
    let value = json_body(&body);
    assert_eq!(value.get("ok").and_then(Json::as_bool), Some(false));
    let errors = value
        .get("error_lines")
        .and_then(Json::as_array)
        .unwrap_or_default();
    assert_eq!(errors.first().and_then(Json::as_usize), Some(1));
    drop(server);
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn gui_cli_version_flag() {
    let binary = workspace_root().join("target/debug/r1-editor-gui");
    assert!(binary.is_file(), "run make build first");
    let output = Command::new(binary)
        .arg("--version")
        .output()
        .expect("version");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "R1 Editor GUI 0.1.0 (tc_r1)"
    );
}

#[test]
fn gui_files_lists_one_level_of_eng_files() {
    let (server, workspace) = start("listing");
    fs::write(workspace.join("a.eng"), "Return 0.\n").expect("a");
    fs::create_dir_all(workspace.join("nested")).expect("dir");
    fs::write(workspace.join("nested/b.eng"), "Return 0.\n").expect("b");
    fs::write(workspace.join("ignore.txt"), "x").expect("txt");
    let (status, body) = request(&server, "GET", "/api/files", None);
    assert_eq!(status, 200);
    let files = json_body(&body)
        .get("files")
        .and_then(Json::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(Json::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    assert!(files.contains(&"a.eng".to_string()), "{files:?}");
    assert!(files.contains(&"nested/b.eng".to_string()), "{files:?}");
    assert!(!files.iter().any(|name| {
        std::path::Path::new(name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
    }));
    drop(server);
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn gui_theme_classes_and_controls_exist_in_html() {
    let (server, workspace) = start("html-controls");
    let (_, body) = request(&server, "GET", "/", None);
    for needle in [
        "Run &amp; Parity",
        "std/ Docs",
        "Theme: Midnight",
        "Core-7 IR",
        "Build .elf",
        "program input",
        "kind-prose",
        "kind-error",
    ] {
        assert!(body.contains(needle), "missing {needle}");
    }
    drop(server);
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn gui_stop_releases_ephemeral_port() {
    let workspace = temp_dir("shutdown");
    let mut server = GuiServer::start("127.0.0.1:0", &workspace).expect("server");
    let address = server.address();
    server.stop();
    drop(server);
    let rebound = std::net::TcpListener::bind(address);
    assert!(rebound.is_ok(), "port was not released");
    drop(rebound);
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn gui_std_drawer_contains_phrase_signatures() {
    let (server, workspace) = start("std-phrases");
    let (_, body) = request(&server, "GET", "/api/std", None);
    assert!(body.contains("say <n>") || body.contains("say <x>"));
    assert!(body.contains("push <val> to <items>") || body.contains("push <val> to <list>"));
    assert!(body.contains("square root of <x>"));
    drop(server);
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn gui_build_unknown_route_returns_404() {
    let (server, workspace) = start("404");
    let (status, body) = request(&server, "GET", "/api/nope", None);
    assert_eq!(status, 404);
    assert!(body.contains("not found"));
    drop(server);
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn gui_highlight_marks_omitted_lines_as_prose() {
    let (server, workspace) = start("highlight-prose");
    let body = Json::object([
        ("source", Json::string("It was raining outside.\n")),
        ("error_lines", Json::array(vec![])),
        ("omitted_lines", Json::array(vec![Json::int(1)])),
    ]);
    let (status, response) = request(&server, "POST", "/api/highlight", Some(&body));
    assert_eq!(status, 200);
    let value = json_body(&response);
    let kinds = value
        .get("lines")
        .and_then(Json::as_array)
        .unwrap_or_default()[0]
        .as_array()
        .unwrap_or_default()
        .iter()
        .filter_map(|span| span.get("kind").and_then(Json::as_str))
        .collect::<Vec<_>>();
    assert!(kinds.contains(&"prose"), "{kinds:?}");
    drop(server);
    let _ = fs::remove_dir_all(workspace);
}
