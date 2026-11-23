//! Headless JSON-RPC protocol tests for `tc-lsp`.

use std::io::Cursor;

use tc_lsp::json::Json;
use tc_lsp::rpc;
use tc_lsp::run_with_io;

fn object(entries: impl IntoIterator<Item = (&'static str, Json)>) -> Json {
    Json::object(entries)
}

fn position(line: i64, character: i64) -> Json {
    object([
        ("line", Json::Number(line)),
        ("character", Json::Number(character)),
    ])
}

fn text_document(uri: &str) -> Json {
    object([("uri", Json::string(uri))])
}

fn request(id: i64, method: &str, params: Json) -> Json {
    object([
        ("jsonrpc", Json::string("2.0")),
        ("id", Json::Number(id)),
        ("method", Json::string(method)),
        ("params", params),
    ])
}

fn notification(method: &str, params: Json) -> Json {
    object([
        ("jsonrpc", Json::string("2.0")),
        ("method", Json::string(method)),
        ("params", params),
    ])
}

fn open(uri: &str, text: &str) -> Json {
    notification(
        "textDocument/didOpen",
        object([(
            "textDocument",
            object([
                ("uri", Json::string(uri)),
                ("languageId", Json::string("eng")),
                ("version", Json::Number(1)),
                ("text", Json::string(text)),
            ]),
        )]),
    )
}

fn change(uri: &str, text: &str) -> Json {
    notification(
        "textDocument/didChange",
        object([
            ("textDocument", object([("uri", Json::string(uri))])),
            (
                "contentChanges",
                Json::Array(vec![object([("text", Json::string(text))])]),
            ),
        ]),
    )
}

fn shutdown() -> Json {
    request(999, "shutdown", Json::Null)
}

fn exit() -> Json {
    notification("exit", Json::Null)
}

fn run(messages: Vec<Json>) -> (i32, Vec<Json>) {
    let mut input = Vec::new();
    for message in messages {
        rpc::write_json(&mut input, &message).expect("frame");
    }
    let mut reader = Cursor::new(input);
    let mut output = Vec::new();
    let code = run_with_io(&mut reader, &mut output);
    let mut cursor = Cursor::new(output);
    let mut responses = Vec::new();
    while let Some(response) = rpc::read_json(&mut cursor).expect("read response") {
        responses.push(response);
    }
    (code, responses)
}

fn response_for(responses: &[Json], id: i64) -> Option<&Json> {
    responses.iter().find(|value| {
        value.get("id").and_then(Json::as_i64) == Some(id) && value.get("result").is_some()
    })
}

fn result_for(responses: &[Json], id: i64) -> Option<&Json> {
    responses
        .iter()
        .find(|value| value.get("id").and_then(Json::as_i64) == Some(id))
        .and_then(|value| value.get("result"))
}

fn diagnostics_after<'a>(responses: &'a [Json], uri: &str) -> Vec<&'a [Json]> {
    responses
        .iter()
        .filter(|value| {
            value.get("method").and_then(Json::as_str) == Some("textDocument/publishDiagnostics")
        })
        .filter(|value| {
            value
                .get("params")
                .and_then(|params| params.get("uri"))
                .and_then(Json::as_str)
                == Some(uri)
        })
        .filter_map(|value| value.get("params")?.get("diagnostics")?.as_array())
        .collect()
}

fn diagnostic_messages(diagnostics: &[Json]) -> String {
    diagnostics
        .iter()
        .filter_map(|value| value.get("message").and_then(Json::as_str))
        .collect::<Vec<_>>()
        .join("\n")
}

fn completion_labels(response: &Json) -> Vec<String> {
    let result = response.get("result").unwrap_or(response);
    result
        .get("items")
        .and_then(Json::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(|item| item.get("label").and_then(Json::as_str))
        .map(str::to_string)
        .collect()
}

fn hover_value(response: &Json) -> String {
    let result = response.get("result").unwrap_or(response);
    result
        .get("contents")
        .and_then(|contents| contents.get("value"))
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string()
}

const BASIC: &str = concat!(
    "Use io.\n",
    "Put 500 in cash.\n",
    "Add 100 to it.\n",
    "Say cash.\n",
    "Return 0.\n",
);

const LIVE_ONE: &str = concat!(
    "Yesterday Ravi called me twice about our weekend road trip. It is 3 pm right now.\n\n",
    "Use io.\n\n",
    "A record Waypoint has distance (a whole number) and toll (a whole number).\n\n",
    "Put 8000 in travel budget, remove 1800 from it, and remove 350 from it.\n",
    "Put 0 in emergency fund.\n",
    "Transfer 1000 from travel budget to emergency fund.\n",
    "If travel budget is more than 4000 or emergency fund equals 0, and travel budget is less than 6000, say \"Budget looks healthy\", otherwise say \"Watch spending\".\n",
    "Display travel budget.\n\n",
    "Put 0 in bonus.\n",
    "For every day from 1 to 5, add it to bonus.\n",
    "Add bonus to travel budget.\n",
    "Show travel budget.\n\n",
    "Create a Waypoint called pondy.\n",
    "Update pondy so its distance is 150 and its toll is 220.\n",
    "Print the waypoint's distance plus its toll.\n\n",
    "Give back 0.\n",
);

const LIVE_TWO: &str = concat!(
    "Use core, io, list, math, and string.\n\n",
    "Let root be square root of 144.0.\n",
    "Let nums be List of whole number.\n",
    "Push 4 to address of nums.\n",
    "Push 9 to address of nums.\n",
    "Let total be sum of address of nums.\n",
    "Let average be average of address of nums.\n",
    "Say root.\n",
    "Say total.\n",
    "Say average.\n",
    "Let text value be create text from \"  hello  \" of length 9.\n",
    "Let trimmed be trim text address of text value.\n",
    "Say address of trimmed.\n",
    "Release text address of text value.\n",
    "Release text address of trimmed.\n",
    "Free list address of nums.\n",
    "Give back 0.\n",
);

#[test]
fn lsp_initialize_and_clean_shutdown_exit_0() {
    let (code, responses) = run(vec![
        request(1, "initialize", object([("capabilities", object([]))])),
        notification("initialized", object([])),
        shutdown(),
        exit(),
    ]);
    assert_eq!(code, 0);
    let initialize = response_for(&responses, 1).expect("initialize response");
    let capabilities = initialize
        .get("result")
        .and_then(|value| value.get("capabilities"))
        .expect("capabilities");
    assert_eq!(
        capabilities.get("textDocumentSync").and_then(Json::as_i64),
        Some(1)
    );
    assert_eq!(
        capabilities.get("hoverProvider").and_then(Json::as_bool),
        Some(true)
    );
    assert_eq!(
        capabilities
            .get("definitionProvider")
            .and_then(Json::as_bool),
        Some(true)
    );
    assert_eq!(
        capabilities
            .get("documentSymbolProvider")
            .and_then(Json::as_bool),
        Some(true)
    );
    let legend = capabilities
        .get("semanticTokensProvider")
        .and_then(|value| value.get("legend"));
    let token_types = legend
        .and_then(|value| value.get("tokenTypes"))
        .and_then(Json::as_array)
        .expect("token types");
    assert_eq!(token_types.len(), 9);
    assert_eq!(token_types[0].as_str(), Some("keyword"));
}

#[test]
fn lsp_did_open_valid_eng_publishes_empty_diagnostics() {
    let uri = "file:///tmp/tc_lsp_valid.eng";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, BASIC),
        shutdown(),
        exit(),
    ]);
    let batches = diagnostics_after(&responses, uri);
    assert_eq!(batches.len(), 1);
    assert!(batches[0].is_empty());
}

#[test]
fn lsp_did_open_typo_publishes_levenshtein_suggestion_diagnostic() {
    let uri = "file:///tmp/tc_lsp_typo.eng";
    let text = "Let travel_budget be 10.\nLet x be travle_budget.\nReturn x.\n";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, text),
        shutdown(),
        exit(),
    ]);
    let batches = diagnostics_after(&responses, uri);
    let messages = diagnostic_messages(batches[0]);
    assert!(
        messages.contains("did you mean 'travel budget'?"),
        "{messages}"
    );
}

#[test]
fn lsp_did_open_reserved_word_publishes_hint_diagnostic() {
    let uri = "file:///tmp/tc_lsp_reserved.eng";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, "A record Stop has x (a whole number).\nReturn 0.\n"),
        shutdown(),
        exit(),
    ]);
    let messages = diagnostic_messages(diagnostics_after(&responses, uri)[0]);
    assert!(messages.contains("reserved control word"), "{messages}");
}

#[test]
fn lsp_did_open_unresolved_anaphor_publishes_guidance() {
    let uri = "file:///tmp/tc_lsp_anaphor.eng";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, "Add 10 to it.\nReturn 0.\n"),
        shutdown(),
        exit(),
    ]);
    let messages = diagnostic_messages(diagnostics_after(&responses, uri)[0]);
    assert!(messages.contains("UNRESOLVED_ANAPHOR"), "{messages}");
    assert!(messages.contains("Put 10 in cash"), "{messages}");
}

#[test]
fn lsp_did_change_updates_diagnostics_live() {
    let uri = "file:///tmp/tc_lsp_change.eng";
    let broken = "Let travel_budget be 10.\nLet x be travle_budget.\nReturn x.\n";
    let fixed = "Let travel_budget be 10.\nLet x be travel_budget.\nReturn x.\n";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, broken),
        change(uri, fixed),
        shutdown(),
        exit(),
    ]);
    let batches = diagnostics_after(&responses, uri);
    assert_eq!(batches.len(), 2);
    assert!(!batches[0].is_empty());
    assert!(batches[1].is_empty());
}

#[test]
fn lsp_did_close_clears_diagnostics() {
    let uri = "file:///tmp/tc_lsp_close.eng";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, "A record Stop has x (a whole number).\nReturn 0.\n"),
        notification(
            "textDocument/didClose",
            object([("textDocument", text_document(uri))]),
        ),
        shutdown(),
        exit(),
    ]);
    let batches = diagnostics_after(&responses, uri);
    assert_eq!(batches.len(), 2);
    assert!(!batches[0].is_empty());
    assert!(batches[1].is_empty());
}

#[test]
fn lsp_hover_on_anaphor_it_shows_resolved_target() {
    let uri = "file:///tmp/tc_lsp_hover_it.eng";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, BASIC),
        request(
            2,
            "textDocument/hover",
            object([
                ("textDocument", text_document(uri)),
                ("position", position(2, 12)),
            ]),
        ),
        shutdown(),
        exit(),
    ]);
    let value = hover_value(response_for(&responses, 2).expect("hover"));
    assert!(value.contains("[it -> cash]"), "{value}");
}

#[test]
fn lsp_hover_on_its_field_shows_resolved_field() {
    let uri = "file:///tmp/tc_lsp_hover_its.eng";
    let text = concat!(
        "A record Waypoint has distance (a whole number) and toll (a whole number).\n",
        "Create a Waypoint called pondy.\n",
        "Set its distance to 10.\n",
        "Return 0.\n",
    );
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, text),
        request(
            2,
            "textDocument/hover",
            object([
                ("textDocument", text_document(uri)),
                ("position", position(2, 12)),
            ]),
        ),
        shutdown(),
        exit(),
    ]);
    let value = hover_value(response_for(&responses, 2).expect("hover"));
    assert!(
        value.contains("[its distance -> pondy.distance]"),
        "{value}"
    );
}

#[test]
fn lsp_hover_on_lexicon_verb_shows_wordnet_verbnet_provenance() {
    let uri = "file:///tmp/tc_lsp_hover_verb.eng";
    let text = concat!(
        "Put 100 in cash.\n",
        "Put 100 in bank.\n",
        "Add 10 to cash.\n",
        "Remove 5 from cash.\n",
        "Transfer 10 from cash to bank.\n",
        "Return 0.\n",
    );
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, text),
        request(
            2,
            "textDocument/hover",
            object([
                ("textDocument", text_document(uri)),
                ("position", position(2, 1)),
            ]),
        ),
        request(
            3,
            "textDocument/hover",
            object([
                ("textDocument", text_document(uri)),
                ("position", position(3, 1)),
            ]),
        ),
        request(
            4,
            "textDocument/hover",
            object([
                ("textDocument", text_document(uri)),
                ("position", position(4, 1)),
            ]),
        ),
        shutdown(),
        exit(),
    ]);
    for id in 2..=4 {
        let value = hover_value(response_for(&responses, id).expect("verb hover"));
        assert!(value.contains("Computational verb"), "{value}");
        assert!(
            value.contains("WordNet synset") || value.contains("VerbNet class"),
            "{value}"
        );
    }
}

#[test]
fn lsp_hover_on_omitted_prose_shows_comment_omitted_reason() {
    let uri = "file:///tmp/tc_lsp_hover_prose.eng";
    let text = "Use io.\nShe cleared the table after dinner.\nSay 1.\nReturn 0.\n";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, text),
        request(
            2,
            "textDocument/hover",
            object([
                ("textDocument", text_document(uri)),
                ("position", position(1, 5)),
            ]),
        ),
        shutdown(),
        exit(),
    ]);
    let value = hover_value(response_for(&responses, 2).expect("hover"));
    assert!(value.contains("COMMENT_OMITTED"), "{value}");
    assert!(value.contains("narrative"), "{value}");
}

#[test]
fn lsp_completion_after_use_suggests_all_8_std_modules() {
    let uri = "file:///tmp/tc_lsp_use.eng";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, "Use "),
        request(
            2,
            "textDocument/completion",
            object([
                ("textDocument", text_document(uri)),
                ("position", position(0, 4)),
            ]),
        ),
        shutdown(),
        exit(),
    ]);
    let labels = completion_labels(response_for(&responses, 2).expect("completion"));
    for module in ["alloc", "core", "fs", "io", "list", "map", "math", "string"] {
        assert!(
            labels.iter().any(|label| label == module),
            "{module}: {labels:?}"
        );
    }
}

#[test]
fn lsp_completion_after_possessive_suggests_record_fields() {
    let uri = "file:///tmp/tc_lsp_possessive.eng";
    let text = concat!(
        "A record Waypoint has distance (a whole number) and toll (a whole number).\n",
        "Create a Waypoint called pondy.\n",
        "Set pondy's ",
    );
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, text),
        request(
            2,
            "textDocument/completion",
            object([
                ("textDocument", text_document(uri)),
                ("position", position(2, 12)),
            ]),
        ),
        shutdown(),
        exit(),
    ]);
    let labels = completion_labels(response_for(&responses, 2).expect("completion"));
    assert!(labels.contains(&"distance".to_string()), "{labels:?}");
    assert!(labels.contains(&"toll".to_string()), "{labels:?}");
}

#[test]
fn lsp_completion_after_its_suggests_active_discourse_record_fields() {
    let uri = "file:///tmp/tc_lsp_its_completion.eng";
    let text = concat!(
        "A record Waypoint has distance (a whole number) and toll (a whole number).\n",
        "Create a Waypoint called pondy.\n",
        "Set its ",
    );
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, text),
        request(
            2,
            "textDocument/completion",
            object([
                ("textDocument", text_document(uri)),
                ("position", position(2, 8)),
            ]),
        ),
        shutdown(),
        exit(),
    ]);
    let labels = completion_labels(response_for(&responses, 2).expect("completion"));
    assert!(labels.contains(&"distance".to_string()), "{labels:?}");
    assert!(labels.contains(&"toll".to_string()), "{labels:?}");
}

#[test]
fn lsp_completion_after_if_var_suggests_english_comparators() {
    let uri = "file:///tmp/tc_lsp_comparators.eng";
    let text = "Put 10 in cash.\nIf cash ";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, text),
        request(
            2,
            "textDocument/completion",
            object([
                ("textDocument", text_document(uri)),
                ("position", position(1, 8)),
            ]),
        ),
        shutdown(),
        exit(),
    ]);
    let labels = completion_labels(response_for(&responses, 2).expect("completion"));
    assert!(labels.contains(&"is more than".to_string()), "{labels:?}");
    assert!(labels.contains(&"is less than".to_string()), "{labels:?}");
    assert!(labels.contains(&"equals".to_string()), "{labels:?}");
}

#[test]
fn lsp_completion_after_if_var_suggests_natural_predicates() {
    let uri = "file:///tmp/tc_lsp_natural_predicates.eng";
    let text = "Put 10 in cash.\nIf cash ";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, text),
        request(
            2,
            "textDocument/completion",
            object([
                ("textDocument", text_document(uri)),
                ("position", position(1, 8)),
            ]),
        ),
        shutdown(),
        exit(),
    ]);
    let labels = completion_labels(response_for(&responses, 2).expect("completion"));
    for expected in [
        "is even",
        "is odd",
        "is positive",
        "is negative",
        "is zero",
        "is nonzero",
        "is divisible by",
        "is between",
        "exceeds",
        "differs from",
        "is empty",
        "is not empty",
    ] {
        assert!(
            labels.iter().any(|label| label == expected),
            "missing {expected}: {labels:?}"
        );
    }
}

#[test]
fn lsp_did_open_fluent_constructs_publishes_empty_diagnostics() {
    let uri = "file:///tmp/tc_lsp_fluent_valid.eng";
    let text = concat!(
        "Please let prices be a list of ten, twenty, and thirty.\n",
        "If the number of items in prices is at least three and prices is not empty, sum up prices.\n",
        "Print the average of prices.\n",
        "Print the count of \"abcd\".\n",
    );
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, text),
        shutdown(),
        exit(),
    ]);
    let batches = diagnostics_after(&responses, uri);
    assert_eq!(batches.len(), 1);
    assert!(batches[0].is_empty(), "{:?}", batches[0]);
}

#[test]
fn lsp_did_change_fluent_program_updates_diagnostics_live() {
    let uri = "file:///tmp/tc_lsp_fluent_change.eng";
    let broken = "Let x be 5.\nIf x is divisble by 5, print \"hit\".\nReturn 0.\n";
    let fixed = "Let x be 5.\nIf x is divisible by 5, print \"hit\".\nReturn 0.\n";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, broken),
        change(uri, fixed),
        shutdown(),
        exit(),
    ]);
    let batches = diagnostics_after(&responses, uri);
    assert_eq!(batches.len(), 2);
    assert!(!batches[0].is_empty());
    assert!(batches[1].is_empty(), "{:?}", batches[1]);
}

#[test]
fn lsp_completion_general_suggests_multiword_vars_and_std_phrases() {
    let uri = "file:///tmp/tc_lsp_general.eng";
    let text = concat!(
        "Use io and math.\n",
        "Let travel budget be 10.\n",
        "Let root be square root of 2.0.\n",
        "Return 0.\n",
    );
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, text),
        request(
            2,
            "textDocument/completion",
            object([
                ("textDocument", text_document(uri)),
                ("position", position(3, 0)),
            ]),
        ),
        shutdown(),
        exit(),
    ]);
    let labels = completion_labels(response_for(&responses, 2).expect("completion"));
    for expected in [
        "travel budget",
        "say <n>",
        "square root of <x>",
        "sum of <items>",
        "trim text <s>",
        "save <s> to file <path>",
        "Put ... in ...",
    ] {
        assert!(
            labels.iter().any(|label| label == expected),
            "missing {expected}: {labels:?}"
        );
    }
}

#[test]
fn lsp_definition_jumps_to_local_record_and_function() {
    let uri = "file:///tmp/tc_lsp_definition.eng";
    let text = concat!(
        "A record Waypoint has distance (a whole number) and toll (a whole number).\n",
        "Create a Waypoint called pondy.\n",
        "To bump given x as a whole number, giving a whole number.\n",
        "Give back x plus 1.\n",
        "Let raised be bump(4).\n",
        "Say raised.\n",
        "Return 0.\n",
    );
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, text),
        request(
            2,
            "textDocument/definition",
            object([
                ("textDocument", text_document(uri)),
                ("position", position(0, 12)),
            ]),
        ),
        request(
            3,
            "textDocument/definition",
            object([
                ("textDocument", text_document(uri)),
                ("position", position(4, 17)),
            ]),
        ),
        shutdown(),
        exit(),
    ]);
    for id in 2..=3 {
        let result = result_for(&responses, id).expect("definition");
        let location = &result.as_array().expect("array")[0];
        assert_eq!(
            location.get("uri").and_then(Json::as_str),
            Some(uri),
            "{result:?}"
        );
    }
    let function = &result_for(&responses, 3)
        .expect("function definition")
        .as_array()
        .expect("array")[0];
    let line = function
        .get("range")
        .and_then(|value| value.get("start"))
        .and_then(|value| value.get("line"))
        .and_then(Json::as_i64);
    assert_eq!(line, Some(2));
}

#[test]
fn lsp_definition_jumps_into_std_io_eng() {
    let uri = "file:///tmp/tc_lsp_definition_std.eng";
    let text = "Use io.\nPut 10 in cash.\nSay cash.\nReturn 0.\n";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, text),
        request(
            2,
            "textDocument/definition",
            object([
                ("textDocument", text_document(uri)),
                ("position", position(2, 1)),
            ]),
        ),
        shutdown(),
        exit(),
    ]);
    let result = result_for(&responses, 2).expect("definition");
    let location = &result.as_array().expect("array")[0];
    let uri = location
        .get("uri")
        .and_then(Json::as_str)
        .unwrap_or_default();
    assert!(uri.ends_with("/std/io.eng"), "{uri}");
}

#[test]
fn lsp_document_symbols_lists_records_globals_and_functions() {
    let uri = "file:///tmp/tc_lsp_symbols.eng";
    let text = concat!(
        "Keep a shared seed as a whole number.\n",
        "A record Waypoint has distance (a whole number) and toll (a whole number).\n",
        "To bump given x as a whole number, giving a whole number.\n",
        "Give back x plus 1.\n",
        "Let raised be bump(4).\n",
        "Return raised.\n",
    );
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, text),
        request(
            2,
            "textDocument/documentSymbol",
            object([("textDocument", text_document(uri))]),
        ),
        shutdown(),
        exit(),
    ]);
    let symbols = result_for(&responses, 2)
        .expect("symbols")
        .as_array()
        .expect("array");
    let names = symbols
        .iter()
        .filter_map(|value| value.get("name").and_then(Json::as_str))
        .collect::<Vec<_>>();
    assert!(names.contains(&"waypoint"), "{names:?}");
    assert!(names.contains(&"bump"), "{names:?}");
    assert!(names.iter().any(|name| name.contains("seed")), "{names:?}");
}

#[test]
fn lsp_semantic_tokens_encodes_valid_delta_stream_with_prose_comments() {
    let uri = "file:///tmp/tc_lsp_tokens.eng";
    let text =
        "Use io.\nShe cleared the table after dinner.\nPut 500 in cash.\nSay cash.\nReturn 0.\n";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, text),
        request(
            2,
            "textDocument/semanticTokens/full",
            object([("textDocument", text_document(uri))]),
        ),
        shutdown(),
        exit(),
    ]);
    let data = result_for(&responses, 2)
        .expect("tokens")
        .get("data")
        .and_then(Json::as_array)
        .expect("data")
        .iter()
        .filter_map(Json::as_i64)
        .collect::<Vec<_>>();
    assert_eq!(data.len() % 5, 0);
    assert!(data.chunks(5).any(|chunk| chunk[3] == 7), "{data:?}");
}

#[test]
fn lsp_utf16_position_conversion_handles_multibyte_utf8_safely() {
    let uri = "file:///tmp/tc_lsp_utf16.eng";
    let line = "Let greeting be \"வணக்கம்😀\".";
    let text = format!("Use io.\n{line}\nSay greeting.\nReturn 0.\n");
    let utf16_end = line.encode_utf16().count() as i64;
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, &text),
        request(
            2,
            "textDocument/completion",
            object([
                ("textDocument", text_document(uri)),
                ("position", position(1, utf16_end)),
            ]),
        ),
        request(
            3,
            "textDocument/semanticTokens/full",
            object([("textDocument", text_document(uri))]),
        ),
        shutdown(),
        exit(),
    ]);
    assert!(response_for(&responses, 2).is_some());
    let data = result_for(&responses, 3)
        .expect("tokens")
        .get("data")
        .and_then(Json::as_array)
        .expect("data");
    assert!(!data.is_empty());
    assert!(
        data.chunks(5)
            .any(|chunk| chunk[3].as_i64() == Some(5)
                && chunk[2].as_i64().is_some_and(|len| len >= 5))
    );
}

#[test]
fn lsp_unknown_method_returns_method_not_found_no_panic() {
    let (code, responses) = run(vec![
        request(1, "initialize", object([])),
        request(2, "textDocument/unknownMethod", Json::Null),
        shutdown(),
        exit(),
    ]);
    assert_eq!(code, 0);
    let error = responses
        .iter()
        .find(|value| value.get("id").and_then(Json::as_i64) == Some(2))
        .and_then(|value| value.get("error"))
        .expect("error");
    assert_eq!(error.get("code").and_then(Json::as_i64), Some(-32601));
}

#[test]
fn lsp_malformed_json_does_not_panic() {
    let mut input = Vec::new();
    let body = b"{not json}";
    input.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
    input.extend_from_slice(body);
    for message in [request(1, "initialize", object([])), shutdown(), exit()] {
        rpc::write_json(&mut input, &message).expect("frame");
    }
    let mut reader = Cursor::new(input);
    let mut output = Vec::new();
    let code = run_with_io(&mut reader, &mut output);
    assert_eq!(code, 0);
    let mut cursor = Cursor::new(output);
    let mut responses = Vec::new();
    while let Some(response) = rpc::read_json(&mut cursor).expect("read") {
        responses.push(response);
    }
    assert!(response_for(&responses, 1).is_some());
}

#[test]
fn lsp_cli_version_flag() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_tc-lsp"))
        .arg("--version")
        .output()
        .expect("run tc-lsp");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("tc-lsp"), "{stdout}");
    assert!(stdout.contains("0.1.0"), "{stdout}");
}

#[test]
fn lsp_compositional_emit_where_has_zero_diagnostics() {
    let uri = "file:///tmp/tc_lsp_compositional_emit.eng";
    let text = "Print the sum of a and b, where a is 12 and b is 23.\n";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, text),
        shutdown(),
        exit(),
    ]);
    let batches = diagnostics_after(&responses, uri);
    assert_eq!(batches.len(), 1);
    assert!(batches[0].is_empty(), "{}", diagnostic_messages(batches[0]));
}

#[test]
fn lsp_hover_on_print_shows_emit_provenance() {
    let uri = "file:///tmp/tc_lsp_print_provenance.eng";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, "Print 12.\n"),
        request(
            2,
            "textDocument/hover",
            object([
                ("textDocument", text_document(uri)),
                ("position", position(0, 1)),
            ]),
        ),
        shutdown(),
        exit(),
    ]);
    let value = hover_value(response_for(&responses, 2).expect("hover"));
    assert!(value.contains("Computational verb"), "{value}");
    assert!(value.contains("VerbNet class"), "{value}");
}

#[test]
fn lsp_hover_on_plural_anaphor_shows_resolution() {
    let uri = "file:///tmp/tc_lsp_plural_hover.eng";
    let text = "Let a be 12 and b be 23. Print their sum.\n";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, text),
        request(
            2,
            "textDocument/hover",
            object([
                ("textDocument", text_document(uri)),
                ("position", position(0, 33)),
            ]),
        ),
        shutdown(),
        exit(),
    ]);
    let value = hover_value(response_for(&responses, 2).expect("hover"));
    assert!(value.contains("ACE Anaphor"), "{value}");
    assert!(value.contains("their sum"), "{value}");
    assert!(value.contains('a'), "{value}");
}

#[test]
fn lsp_where_typo_reports_levenshtein_diagnostic() {
    let uri = "file:///tmp/tc_lsp_where_typo.eng";
    let text = "Print the sum of a and b, where a is 12 and c is 23.\n";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, text),
        shutdown(),
        exit(),
    ]);
    let messages = diagnostic_messages(diagnostics_after(&responses, uri)[0]);
    assert!(
        messages.contains("variable `b` is not in scope"),
        "{messages}"
    );
    assert!(messages.contains("did you mean"), "{messages}");
}

#[test]
fn lsp_typo_imperative_reports_unrecognized_hint() {
    let uri = "file:///tmp/tc_lsp_typo_imperative.eng";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, "Printt 5.\n"),
        shutdown(),
        exit(),
    ]);
    let messages = diagnostic_messages(diagnostics_after(&responses, uri)[0]);
    assert!(messages.contains("unrecognized imperative"), "{messages}");
    assert!(messages.contains("did you mean 'print'?"), "{messages}");
}

#[test]
fn lsp_zero_executable_program_reports_warning() {
    let uri = "file:///tmp/tc_lsp_zero_exec.eng";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, "Frobnicate the flux capacitor.\n"),
        shutdown(),
        exit(),
    ]);
    let diagnostics = diagnostics_after(&responses, uri)[0];
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.get("severity").and_then(Json::as_i64) == Some(2)
                && diagnostic
                    .get("message")
                    .and_then(Json::as_str)
                    .is_some_and(|message| {
                        message.contains("program produced 0 executable statements")
                    })
        }),
        "{diagnostics:?}"
    );
}

#[test]
fn lsp_live_test_1_and_live_test_2_zero_diagnostics() {
    let first = "file:///tmp/tc_lsp_live_one.eng";
    let second = "file:///tmp/tc_lsp_live_two.eng";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(first, LIVE_ONE),
        open(second, LIVE_TWO),
        shutdown(),
        exit(),
    ]);
    for uri in [first, second] {
        let batches = diagnostics_after(&responses, uri);
        assert_eq!(batches.len(), 1, "{uri}");
        assert!(
            batches[0].is_empty(),
            "{uri}: {}",
            diagnostic_messages(batches[0])
        );
    }
}
