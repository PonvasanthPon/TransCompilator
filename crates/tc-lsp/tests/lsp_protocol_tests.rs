//! Headless JSON-RPC protocol tests for `tc-lsp`.

use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

use tc_lsp::json::Json;
use tc_lsp::rpc;
use tc_lsp::run_with_io;

const KEYWORD: i64 = 0;
const FUNCTION: i64 = 1;
const VARIABLE: i64 = 2;
const STRING: i64 = 3;
const NUMBER: i64 = 4;
const COMMENT: i64 = 5;
const DECLARATION: i64 = 1;
const READONLY: i64 = 2;
const DOCUMENTATION: i64 = 4;

const ELEVEN_LINE_REPRO: &str = concat!(
    "She showed me the book yesterday.\n",
    "\n",
    "Let n be six.\n",
    "Square n.\n",
    "If n is divisible by nine, print \"yes\".\n",
    "Otherwise, print \"no\".\n",
    "Add four to it, then echo it.\n",
    "\n",
    "She showed me the book yesterday.\n",
    "\n",
    "Printt \"12\".\n",
);

const PRIME_DISCOURSE_REPRO: &str = "Look at every number from 2 up to 50. For each number, check whether any smaller number starting from 2 divides into it evenly. If none of them do, print that number out and count it. Once you are done, print how many numbers were found.\n";

const FILTER_PIPELINE_REPRO: &str = "Take all the numbers between 1 and 30. Skip any multiple of 4, unless it is also divisible by 6. For each remaining number, if it is odd, triple it and add 1; otherwise cut it in half. Print the final value only if it ends up greater than 20, and keep track of the largest value you printed. Once you finish going through all of them, print that largest value.\n";

const FIBONACCI_SEQUENCE_REPRO: &str = "Start with 1 and 2. Keep generating the next number by adding the previous two numbers together, as long as the new number does not exceed 200. For each number in this entire sequence, including the starting two, add it to a running total. Whenever that total goes over 100, subtract 100 from it and count an overflow. At the end, print the final running total and how many overflows occurred.\n";

const TWO_BOX_SIMULATION_REPRO: &str = "Imagine a red box starting with 5 and a blue box starting with 16. Walk through every number from 1 up to 40. If the number is a multiple of 3, move 4 from the blue box into the red box; otherwise move 1 from the red box into the blue box. However, if a move would ever cause either box to drop below 5, skip that move and leave both boxes unchanged. Stop immediately as soon as the red box holds more than twice as much as the blue box. Finally, print the number where you stopped, followed by the amounts in the red box and the blue box.\n";

const ADJACENT_SWAP_REPRO: &str = "Start with the list of numbers 14, 7, 22, 9, 15, 4, and 18. Go through the list from left to right, comparing each number with the one immediately after it. Whenever the left number is larger than the right one, swap their positions and count a swap. After finishing that pass, remove any number from the list that is divisible by 3. Finally, print the remaining numbers in order, followed by the total number of swaps made.\n";

const WORD_FREQUENCY_REPRO: &str = "Take the sentence \"the quick brown fox jumps over the lazy dog and the fox runs\". Split it into words, and count how many times each word appears. Ignore any word that has fewer than 4 letters, unless it appears more than once. Among the remaining words, print each word along with its count, and finally print the word that appeared the most times.\n";

const COLLATZ_REPRO: &str = concat!(
    "Let current be 27, and let steps start at 0.\n",
    "\n",
    "Print \"Tracing sequence from \" with current.\n",
    "\n",
    "Repeat while current is greater than 1:\n",
    "If current is even, divide current by 2.\n",
    "Otherwise, multiply current by 3 and add 1 to current.\n",
    "Add 1 to steps, then print current.\n",
    "\n",
    "Print \"Reached 1 in \" with steps and \" steps.\"\n",
);

const TEN_LINE_VALID_REPRO: &str = concat!(
    "She showed me the book yesterday.\n",
    "\n",
    "Let n be six.\n",
    "Square n.\n",
    "If n is divisible by nine, print \"yes\".\n",
    "Otherwise, print \"no\".\n",
    "Add four to it, then echo it.\n",
    "\n",
    "She showed me the book yesterday.\n",
);

#[derive(Debug, Clone, PartialEq, Eq)]
struct SemanticToken {
    line: usize,
    start: usize,
    length: usize,
    token_type: i64,
    modifiers: i64,
    text: String,
}

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

fn semantic_tokens_for(uri: &str, text: &str) -> (Json, Vec<SemanticToken>) {
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
    let legend = response_for(&responses, 1)
        .and_then(|value| value.get("result"))
        .and_then(|value| value.get("capabilities"))
        .and_then(|value| value.get("semanticTokensProvider"))
        .and_then(|value| value.get("legend"))
        .expect("legend")
        .clone();
    let data = result_for(&responses, 2)
        .expect("semantic tokens")
        .get("data")
        .and_then(Json::as_array)
        .expect("token data")
        .iter()
        .filter_map(Json::as_i64)
        .collect::<Vec<_>>();
    let tokens = decode_semantic_tokens(&data, text);
    (legend, tokens)
}

fn decode_semantic_tokens(data: &[i64], text: &str) -> Vec<SemanticToken> {
    assert_eq!(
        data.len() % 5,
        0,
        "token data must be five integers per token"
    );
    let lines = text.split('\n').collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut line = 0_i64;
    let mut start = 0_i64;
    let mut has_previous = false;
    for chunk in data.chunks(5) {
        let line_delta = chunk[0];
        let start_delta = chunk[1];
        let length = chunk[2];
        let token_type = chunk[3];
        let modifiers = chunk[4];
        assert!(line_delta >= 0, "{chunk:?}");
        assert!(start_delta >= 0, "{chunk:?}");
        assert!(length > 0, "{chunk:?}");
        assert!((0..=COMMENT).contains(&token_type), "{chunk:?}");
        assert!((0..=7).contains(&modifiers), "{chunk:?}");
        if has_previous {
            line += line_delta;
            start = if line_delta == 0 {
                start + start_delta
            } else {
                start_delta
            };
        } else {
            line = line_delta;
            start = start_delta;
        }
        has_previous = true;
        let line_index = usize::try_from(line).expect("line");
        let start_index = usize::try_from(start).expect("start");
        let length_index = usize::try_from(length).expect("length");
        let raw = lines.get(line_index).expect("token line");
        let utf16 = raw.encode_utf16().collect::<Vec<_>>();
        let slice = utf16
            .get(start_index..start_index + length_index)
            .expect("token span");
        let token_text = char::decode_utf16(slice.iter().copied())
            .collect::<Result<String, _>>()
            .expect("utf16 token");
        tokens.push(SemanticToken {
            line: line_index,
            start: start_index,
            length: length_index,
            token_type,
            modifiers,
            text: token_text,
        });
    }
    for pair in tokens.windows(2) {
        assert!(
            pair[0].line < pair[1].line
                || (pair[0].line == pair[1].line
                    && pair[0].start + pair[0].length <= pair[1].start),
            "overlapping or unsorted tokens: {pair:?}"
        );
    }
    tokens
}

fn token_named<'a>(tokens: &'a [SemanticToken], text: &str) -> &'a SemanticToken {
    tokens
        .iter()
        .find(|token| token.text == text)
        .unwrap_or_else(|| panic!("missing token `{text}`: {tokens:?}"))
}

fn has_token(tokens: &[SemanticToken], text: &str) -> bool {
    tokens.iter().any(|token| token.text == text)
}

fn repository_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
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
    assert_eq!(
        token_types,
        [
            "keyword", "function", "variable", "string", "number", "comment"
        ]
        .map(Json::string)
    );
    let token_modifiers = legend
        .and_then(|value| value.get("tokenModifiers"))
        .and_then(Json::as_array)
        .expect("token modifiers");
    assert_eq!(
        token_modifiers,
        ["declaration", "readonly", "documentation"].map(Json::string)
    );
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
        "starts with",
        "ends with",
        "contains",
        "does not contain",
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
    assert!(data.chunks(5).any(|chunk| chunk[3] == COMMENT), "{data:?}");
}

#[test]
fn lsp_semantic_tokens_mark_verbs_keywords_variables_numbers_and_glue() {
    let text = "Print the sum of a and b, where a is 12 and b is 23.\n";
    let (_, tokens) = semantic_tokens_for("file:///tmp/tc_lsp_tokens_where.eng", text);
    assert_eq!(token_named(&tokens, "Print").token_type, FUNCTION);
    assert_eq!(token_named(&tokens, "where").token_type, KEYWORD);
    assert_eq!(token_named(&tokens, "a").token_type, VARIABLE);
    assert_eq!(token_named(&tokens, "b").token_type, VARIABLE);
    assert_eq!(token_named(&tokens, "12").token_type, NUMBER);
    assert_eq!(token_named(&tokens, "23").token_type, NUMBER);
    for glue in ["the", "of", "and"] {
        assert!(
            !has_token(&tokens, glue),
            "glue `{glue}` was tokenized: {tokens:?}"
        );
    }
}

#[test]
fn lsp_semantic_tokens_mark_spoken_numbers_and_readonly_anaphors() {
    let text = "Let a be twelve and b be twenty-three. Print their sum.\n";
    let (_, tokens) = semantic_tokens_for("file:///tmp/tc_lsp_tokens_numbers.eng", text);
    assert_eq!(token_named(&tokens, "twelve").token_type, NUMBER);
    assert_eq!(token_named(&tokens, "twenty-three").token_type, NUMBER);
    let their = token_named(&tokens, "their");
    assert_eq!(their.token_type, VARIABLE);
    assert_ne!(their.modifiers & READONLY, 0, "{their:?}");
}

#[test]
fn lsp_semantic_tokens_mark_it_and_string_literals() {
    let text = "Let x be 10. Add 5 to it. Print \"done\".\n";
    let (_, tokens) = semantic_tokens_for("file:///tmp/tc_lsp_tokens_it.eng", text);
    let it = token_named(&tokens, "it");
    assert_eq!(it.token_type, VARIABLE);
    assert_ne!(it.modifiers & READONLY, 0, "{it:?}");
    assert_eq!(token_named(&tokens, "\"done\"").token_type, STRING);
}

#[test]
fn lsp_semantic_tokens_mark_omitted_prose_with_documentation_modifier() {
    let text = "She cleared the table after dinner.\n";
    let (_, tokens) = semantic_tokens_for("file:///tmp/tc_lsp_tokens_prose.eng", text);
    assert_eq!(tokens.len(), 1, "{tokens:?}");
    assert_eq!(tokens[0].token_type, COMMENT);
    assert_ne!(tokens[0].modifiers & DOCUMENTATION, 0, "{tokens:?}");
    assert!(
        tokens[0].text.contains("She cleared the table"),
        "{tokens:?}"
    );
}

#[test]
fn lsp_semantic_tokens_obey_utf16_delta_and_non_overlap_invariants() {
    let text = concat!(
        "Let travel budget be 10.\n",
        "Let greeting be \"வணக்கம்😀\".\n",
        "Print travel budget.\n",
        "She cleared the table after dinner.\n",
    );
    let (_, tokens) = semantic_tokens_for("file:///tmp/tc_lsp_tokens_invariants.eng", text);
    assert!(!tokens.is_empty());
    let travel_budget = token_named(&tokens, "travel budget");
    assert_eq!(travel_budget.token_type, VARIABLE);
    assert_ne!(
        travel_budget.modifiers & DECLARATION,
        0,
        "{travel_budget:?}"
    );
    assert!(
        tokens
            .iter()
            .filter(|token| token.text == "travel budget")
            .count()
            >= 2,
        "{tokens:?}"
    );
    let literal = "\"வணக்கம்😀\"";
    let string = token_named(&tokens, literal);
    assert_eq!(string.token_type, STRING);
    assert_eq!(string.length, literal.encode_utf16().count());
    let comment = token_named(&tokens, "She cleared the table after dinner.");
    assert_eq!(comment.token_type, COMMENT);
    assert_ne!(comment.modifiers & DOCUMENTATION, 0, "{comment:?}");
}

#[test]
fn lsp_semantic_tokens_mark_variable_declarations() {
    let text = "Let a be 12.\nPrint a.\n";
    let (_, tokens) = semantic_tokens_for("file:///tmp/tc_lsp_tokens_decl.eng", text);
    let declarations = tokens
        .iter()
        .filter(|token| token.text == "a")
        .collect::<Vec<_>>();
    assert_eq!(declarations.len(), 2, "{tokens:?}");
    assert_ne!(declarations[0].modifiers & DECLARATION, 0, "{tokens:?}");
    assert_eq!(declarations[1].modifiers & DECLARATION, 0, "{tokens:?}");
}

#[test]
fn lsp_semantic_tokens_mark_explicit_comments() {
    let text = "# note\n// another note\nPrint 1.\n";
    let (_, tokens) = semantic_tokens_for("file:///tmp/tc_lsp_tokens_comments.eng", text);
    assert_eq!(token_named(&tokens, "# note").token_type, COMMENT);
    assert_eq!(token_named(&tokens, "// another note").token_type, COMMENT);
    assert_eq!(token_named(&tokens, "# note").modifiers & DOCUMENTATION, 0);
}

#[test]
fn lsp_semantic_tokens_mark_natural_predicate_keywords() {
    let text = "Let x be 10. If x is at least 10 and x is even, print x.\n";
    let (_, tokens) = semantic_tokens_for("file:///tmp/tc_lsp_tokens_predicates.eng", text);
    for keyword in ["least", "and", "even"] {
        assert_eq!(
            token_named(&tokens, keyword).token_type,
            KEYWORD,
            "{keyword}: {tokens:?}"
        );
    }
    assert!(
        !has_token(&tokens, "at"),
        "mid-sentence `at` should stay upright: {tokens:?}"
    );
    assert_eq!(token_named(&tokens, "print").token_type, FUNCTION);
}

#[test]
fn lsp_semantic_tokens_prioritize_alloc_fields_and_locals_over_verbs() {
    let text = fs::read_to_string(repository_path("std/alloc.eng")).expect("std/alloc.eng");
    let (_, tokens) = semantic_tokens_for("file:///tmp/tc_lsp_tokens_alloc.eng", &text);
    let padding = tokens
        .iter()
        .filter(|token| token.text == "padding")
        .collect::<Vec<_>>();
    assert!(!padding.is_empty(), "{tokens:?}");
    assert!(
        padding.iter().all(|token| token.token_type == VARIABLE),
        "{padding:?}"
    );
    let rest = tokens
        .iter()
        .filter(|token| token.text == "rest")
        .collect::<Vec<_>>();
    assert!(rest.len() >= 3, "{rest:?}");
    assert!(
        rest.iter().all(|token| token.token_type == VARIABLE),
        "{rest:?}"
    );
}

#[test]
fn lsp_semantic_tokens_keep_mid_sentence_copulas_and_prepositions_unbolded() {
    let text = concat!(
        "Let a be 12.\n",
        "A record MemoryBlock has a pointer to MemoryBlock.\n",
        "Its size is 1.\n",
        "Move a from b for c.\n",
    );
    let (_, tokens) = semantic_tokens_for("file:///tmp/tc_lsp_tokens_glue.eng", text);
    for glue in ["is", "be", "to", "from", "for"] {
        assert!(
            !has_token(&tokens, glue),
            "mid-sentence `{glue}` should stay upright: {tokens:?}"
        );
    }
    assert_eq!(token_named(&tokens, "Let").token_type, KEYWORD);
    assert_eq!(token_named(&tokens, "Its").token_type, VARIABLE);
}

#[test]
fn lsp_semantic_tokens_mark_possessive_fields_and_anaphor_fields() {
    let text = concat!(
        "A record Waypoint has distance (a whole number).\n",
        "Create a Waypoint called pondy.\n",
        "Update pondy so its distance is 5.\n",
        "Print pondy's distance.\n",
    );
    let (_, tokens) = semantic_tokens_for("file:///tmp/tc_lsp_tokens_fields.eng", text);
    assert_eq!(
        token_named(&tokens, "pondy's distance").token_type,
        VARIABLE
    );
    let its_distance = token_named(&tokens, "its distance");
    assert_eq!(its_distance.token_type, VARIABLE);
    assert_ne!(its_distance.modifiers & READONLY, 0, "{its_distance:?}");
}

#[test]
fn lsp_semantic_token_legend_matches_zed_rules_and_settings() {
    let (legend, _) = semantic_tokens_for("file:///tmp/tc_lsp_legend.eng", "Print 1.\n");
    let token_types = legend
        .get("tokenTypes")
        .and_then(Json::as_array)
        .expect("token types")
        .iter()
        .filter_map(Json::as_str)
        .collect::<Vec<_>>();
    let token_modifiers = legend
        .get("tokenModifiers")
        .and_then(Json::as_array)
        .expect("token modifiers")
        .iter()
        .filter_map(Json::as_str)
        .collect::<Vec<_>>();
    let rules_text = fs::read_to_string(repository_path(
        "editors/zed/tc-eng-extension/languages/eng/semantic_token_rules.json",
    ))
    .expect("semantic token rules");
    let rules = tc_lsp::json::parse(&rules_text).expect("rules JSON");
    let rules = rules.as_array().expect("rules array");
    assert!(!rules.is_empty());
    let first = &rules[0];
    assert_eq!(
        first.get("token_type").and_then(Json::as_str),
        Some("variable")
    );
    assert_eq!(
        first
            .get("token_modifiers")
            .and_then(Json::as_array)
            .expect("first modifiers"),
        ["readonly"].map(Json::string).as_slice()
    );
    for rule in rules {
        let token_type = rule
            .get("token_type")
            .and_then(Json::as_str)
            .expect("token type");
        assert!(token_types.contains(&token_type), "{token_type}");
        assert!(
            rule.get("foreground_color")
                .and_then(Json::as_str)
                .is_some()
        );
        assert_ne!(
            rule.get("underline").and_then(Json::as_bool),
            Some(false),
            "novel-style rules must omit underline:false: {rule:?}"
        );
        for modifier in rule
            .get("token_modifiers")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .filter_map(Json::as_str)
        {
            assert!(token_modifiers.contains(&modifier), "{modifier}");
        }
    }
    let variable_rule = rules
        .iter()
        .find(|rule| {
            rule.get("token_type").and_then(Json::as_str) == Some("variable")
                && rule.get("token_modifiers").is_none()
        })
        .expect("bare variable rule");
    assert_eq!(
        variable_rule.get("underline").and_then(Json::as_str),
        Some("#c8c8cc")
    );
    let comment_rule = rules
        .iter()
        .find(|rule| rule.get("token_type").and_then(Json::as_str) == Some("comment"))
        .expect("comment rule");
    assert_eq!(
        comment_rule.get("foreground_color").and_then(Json::as_str),
        Some("#7a7a7a")
    );
    let settings_text =
        fs::read_to_string(repository_path("editors/zed/settings.json")).expect("settings");
    let settings = tc_lsp::json::parse(&settings_text).expect("settings JSON");
    assert_eq!(
        settings.get("disable_ai").and_then(Json::as_bool),
        Some(true)
    );
    let language = settings
        .get("languages")
        .and_then(|value| value.get("TransCompilator English"))
        .expect("language settings");
    assert_eq!(
        language.get("semantic_tokens").and_then(Json::as_str),
        Some("full")
    );
    assert_eq!(
        language
            .get("enable_language_server")
            .and_then(Json::as_bool),
        Some(true)
    );
    assert_eq!(
        settings
            .get("autosave")
            .and_then(|value| value.get("after_delay"))
            .and_then(|value| value.get("milliseconds"))
            .and_then(Json::as_i64),
        Some(300)
    );
}

#[test]
fn lsp_eleven_line_error_keeps_exact_line_10_and_midfile_prose_tokens() {
    let uri = "file:///tmp/tc_lsp_eleven_line_repro.eng";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, ELEVEN_LINE_REPRO),
        request(
            2,
            "textDocument/semanticTokens/full",
            object([("textDocument", text_document(uri))]),
        ),
        shutdown(),
        exit(),
    ]);
    let diagnostics = diagnostics_after(&responses, uri);
    let diagnostic = diagnostics
        .iter()
        .flat_map(|batch| batch.iter())
        .find(|value| {
            value
                .get("message")
                .and_then(Json::as_str)
                .is_some_and(|message| message.contains("printt"))
        })
        .expect("printt diagnostic");
    let start = diagnostic
        .get("range")
        .and_then(|value| value.get("start"))
        .expect("range start");
    assert_eq!(start.get("line").and_then(Json::as_i64), Some(10));
    assert_eq!(start.get("character").and_then(Json::as_i64), Some(0));
    let data = result_for(&responses, 2)
        .expect("tokens")
        .get("data")
        .and_then(Json::as_array)
        .expect("data")
        .iter()
        .filter_map(Json::as_i64)
        .collect::<Vec<_>>();
    let tokens = decode_semantic_tokens(&data, ELEVEN_LINE_REPRO);
    for line in [0, 8] {
        let line_tokens = tokens
            .iter()
            .filter(|token| token.line == line)
            .collect::<Vec<_>>();
        assert!(!line_tokens.is_empty(), "line {line}: {tokens:?}");
        assert!(
            line_tokens
                .iter()
                .all(|token| token.token_type == COMMENT && token.modifiers & DOCUMENTATION != 0),
            "line {line}: {line_tokens:?}"
        );
        assert!(
            line_tokens
                .iter()
                .any(|token| token.text.contains("She showed me the book yesterday")),
            "line {line}: {line_tokens:?}"
        );
        assert!(
            !line_tokens
                .iter()
                .any(|token| matches!(token.token_type, FUNCTION | KEYWORD)),
            "line {line}: {line_tokens:?}"
        );
    }
}

#[test]
fn lsp_valid_file_keeps_midfile_prose_tokens_without_errors() {
    let uri = "file:///tmp/tc_lsp_valid_midfile_prose.eng";
    let (_, tokens) = semantic_tokens_for(uri, TEN_LINE_VALID_REPRO);
    for line in [0, 8] {
        let line_tokens = tokens
            .iter()
            .filter(|token| token.line == line)
            .collect::<Vec<_>>();
        assert!(!line_tokens.is_empty(), "line {line}: {tokens:?}");
        assert!(
            line_tokens
                .iter()
                .all(|token| token.token_type == COMMENT && token.modifiers & DOCUMENTATION != 0),
            "line {line}: {line_tokens:?}"
        );
        assert!(
            !line_tokens
                .iter()
                .any(|token| matches!(token.token_type, FUNCTION | KEYWORD)),
            "line {line}: {line_tokens:?}"
        );
    }
}

#[test]
fn lsp_bottom_omitted_prose_stays_a_comment_token() {
    let text = "Let n be 1.\nPrint n.\nShe showed me the book yesterday.\n";
    let (_, tokens) = semantic_tokens_for("file:///tmp/tc_lsp_bottom_prose.eng", text);
    let line_tokens = tokens
        .iter()
        .filter(|token| token.line == 2)
        .collect::<Vec<_>>();
    assert_eq!(line_tokens.len(), 1, "{tokens:?}");
    assert_eq!(line_tokens[0].token_type, COMMENT);
    assert_ne!(line_tokens[0].modifiers & DOCUMENTATION, 0);
    assert!(line_tokens[0].text.contains("She showed me the book"));
}

#[test]
fn lsp_line_one_typo_reports_exact_line_zero() {
    let uri = "file:///tmp/tc_lsp_line_one_typo.eng";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, "Printt \"12\".\n"),
        shutdown(),
        exit(),
    ]);
    let diagnostics = diagnostics_after(&responses, uri);
    let diagnostic = diagnostics
        .iter()
        .flat_map(|batch| batch.iter())
        .find(|value| {
            value
                .get("message")
                .and_then(Json::as_str)
                .is_some_and(|message| message.contains("printt"))
        })
        .expect("printt diagnostic");
    let start = diagnostic
        .get("range")
        .and_then(|value| value.get("start"))
        .expect("range start");
    assert_eq!(start.get("line").and_then(Json::as_i64), Some(0));
    assert_eq!(start.get("character").and_then(Json::as_i64), Some(0));
}

#[test]
fn lsp_zero_boilerplate_natural_text_and_math_program_has_no_diagnostics() {
    let uri = "file:///tmp/tc_lsp_natural_text_math.eng";
    let text = concat!(
        "Let raw be \"  42  \".\n",
        "Let clean be trim raw.\n",
        "Print the length of clean.\n",
        "If clean starts with \"4\", print \"starts with four\".\n",
        "Print the square root of 81.0.\n",
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
fn lsp_variable_tokens_survive_a_later_lowering_error() {
    let text = concat!(
        "Let raw be \"  42  \".\n",
        "Let clean be trim raw.\n",
        "Printt \"oops\".\n",
    );
    let (_, tokens) = semantic_tokens_for("file:///tmp/tc_lsp_variables_on_error.eng", text);
    for name in ["raw", "clean"] {
        let matching = tokens
            .iter()
            .filter(|token| token.text == name)
            .collect::<Vec<_>>();
        assert!(!matching.is_empty(), "{name}: {tokens:?}");
        assert!(
            matching.iter().all(|token| token.token_type == VARIABLE),
            "{name}: {matching:?}"
        );
        assert!(
            matching.iter().all(|token| token.token_type != FUNCTION),
            "{name}: {matching:?}"
        );
    }
}

#[test]
fn lsp_collatz_program_has_zero_diagnostics_and_multipart_output_tokens() {
    let uri = "file:///tmp/tc_lsp_collatz.eng";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, COLLATZ_REPRO),
        request(
            2,
            "textDocument/semanticTokens/full",
            object([("textDocument", text_document(uri))]),
        ),
        shutdown(),
        exit(),
    ]);
    let batches = diagnostics_after(&responses, uri);
    assert_eq!(batches.len(), 1);
    assert!(batches[0].is_empty(), "{:?}", batches[0]);
    let data = result_for(&responses, 2)
        .expect("tokens")
        .get("data")
        .and_then(Json::as_array)
        .expect("data")
        .iter()
        .filter_map(Json::as_i64)
        .collect::<Vec<_>>();
    let tokens = decode_semantic_tokens(&data, COLLATZ_REPRO);
    let line = tokens
        .iter()
        .filter(|token| token.line == 9)
        .collect::<Vec<_>>();
    assert!(
        line.iter()
            .any(|token| token.text == "Print" && token.token_type == FUNCTION),
        "{line:?}"
    );
    assert!(
        line.iter()
            .any(|token| token.text == "\"Reached 1 in \"" && token.token_type == STRING),
        "{line:?}"
    );
    assert!(
        line.iter()
            .any(|token| token.text == "\" steps.\"" && token.token_type == STRING),
        "{line:?}"
    );
    assert!(
        line.iter()
            .any(|token| token.text == "steps" && token.token_type == VARIABLE),
        "{line:?}"
    );
    assert!(
        !line.iter().any(|token| token.token_type == COMMENT),
        "{line:?}"
    );
}

#[test]
fn lsp_paragraph_loop_preserves_exact_following_error_line() {
    let uri = "file:///tmp/tc_lsp_paragraph_loop_span.eng";
    let text = concat!(
        "Let n be 0.\n",
        "Repeat while n is less than 2:\n",
        "Add 1 to n.\n",
        "\n",
        "Printt \"x\".\n",
    );
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, text),
        shutdown(),
        exit(),
    ]);
    let diagnostics = diagnostics_after(&responses, uri);
    let diagnostic = diagnostics
        .iter()
        .flat_map(|batch| batch.iter())
        .find(|value| {
            value
                .get("message")
                .and_then(Json::as_str)
                .is_some_and(|message| message.contains("printt"))
        })
        .expect("printt diagnostic");
    let start = diagnostic
        .get("range")
        .and_then(|value| value.get("start"))
        .expect("range start");
    assert_eq!(start.get("line").and_then(Json::as_i64), Some(4));
    assert_eq!(start.get("character").and_then(Json::as_i64), Some(0));
}

#[test]
fn lsp_trim_text_and_start_at_program_has_zero_diagnostics() {
    let uri = "file:///tmp/tc_lsp_trim_text_start_at.eng";
    let text = concat!(
        "Use string. Use math.\n",
        "Let raw be \"  42  \".\n",
        "Let clean be trim text raw.\n",
        "Let a starts at 3, and let b begins at 4.\n",
        "Print the length of clean.\n",
        "If clean starts with \"4\", print \"starts with four\".\n",
        "Print square root of 81.0.\n",
        "Print a plus b.\n",
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
fn lsp_prime_discourse_program_has_zero_diagnostics_and_no_comment_tokens() {
    let uri = "file:///tmp/tc_lsp_prime_discourse.eng";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, PRIME_DISCOURSE_REPRO),
        request(
            2,
            "textDocument/semanticTokens/full",
            object([("textDocument", text_document(uri))]),
        ),
        shutdown(),
        exit(),
    ]);
    let batches = diagnostics_after(&responses, uri);
    assert_eq!(batches.len(), 1);
    assert!(batches[0].is_empty(), "{:?}", batches[0]);
    let data = result_for(&responses, 2)
        .expect("tokens")
        .get("data")
        .and_then(Json::as_array)
        .expect("data")
        .iter()
        .filter_map(Json::as_i64)
        .collect::<Vec<_>>();
    let tokens = decode_semantic_tokens(&data, PRIME_DISCOURSE_REPRO);
    assert!(
        tokens.iter().any(|token| token.token_type == FUNCTION),
        "{tokens:?}"
    );
    assert!(
        tokens
            .iter()
            .any(|token| token.text == "number" && token.token_type == VARIABLE),
        "{tokens:?}"
    );
    assert!(
        !tokens.iter().any(|token| token.token_type == COMMENT),
        "{tokens:?}"
    );
}

#[test]
fn lsp_filtered_range_pipeline_has_zero_diagnostics_and_no_comment_tokens() {
    let uri = "file:///tmp/tc_lsp_filter_pipeline.eng";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, FILTER_PIPELINE_REPRO),
        request(
            2,
            "textDocument/semanticTokens/full",
            object([("textDocument", text_document(uri))]),
        ),
        shutdown(),
        exit(),
    ]);
    let batches = diagnostics_after(&responses, uri);
    assert_eq!(batches.len(), 1);
    assert!(batches[0].is_empty(), "{:?}", batches[0]);
    let data = result_for(&responses, 2)
        .expect("tokens")
        .get("data")
        .and_then(Json::as_array)
        .expect("data")
        .iter()
        .filter_map(Json::as_i64)
        .collect::<Vec<_>>();
    let tokens = decode_semantic_tokens(&data, FILTER_PIPELINE_REPRO);
    assert!(
        tokens
            .iter()
            .any(|token| token.text == "number" && token.token_type == VARIABLE),
        "{tokens:?}"
    );
    assert!(
        tokens
            .iter()
            .any(|token| token.text == "triple" && token.token_type == FUNCTION),
        "{tokens:?}"
    );
    assert!(
        !tokens.iter().any(|token| token.token_type == COMMENT),
        "{tokens:?}"
    );
}

#[test]
fn lsp_fibonacci_sequence_pipeline_has_zero_diagnostics_and_no_comment_tokens() {
    let uri = "file:///tmp/tc_lsp_fibonacci_sequence.eng";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, FIBONACCI_SEQUENCE_REPRO),
        request(
            2,
            "textDocument/semanticTokens/full",
            object([("textDocument", text_document(uri))]),
        ),
        shutdown(),
        exit(),
    ]);
    let batches = diagnostics_after(&responses, uri);
    assert_eq!(batches.len(), 1);
    assert!(batches[0].is_empty(), "{:?}", batches[0]);
    let data = result_for(&responses, 2)
        .expect("tokens")
        .get("data")
        .and_then(Json::as_array)
        .expect("data")
        .iter()
        .filter_map(Json::as_i64)
        .collect::<Vec<_>>();
    let tokens = decode_semantic_tokens(&data, FIBONACCI_SEQUENCE_REPRO);
    assert!(
        tokens
            .iter()
            .any(|token| token.text == "number" && token.token_type == VARIABLE),
        "{tokens:?}"
    );
    assert!(
        !tokens.iter().any(|token| token.token_type == COMMENT),
        "{tokens:?}"
    );
}

#[test]
fn lsp_two_box_simulation_has_zero_diagnostics_and_no_comment_tokens() {
    let uri = "file:///tmp/tc_lsp_two_box_simulation.eng";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, TWO_BOX_SIMULATION_REPRO),
        request(
            2,
            "textDocument/semanticTokens/full",
            object([("textDocument", text_document(uri))]),
        ),
        shutdown(),
        exit(),
    ]);
    let batches = diagnostics_after(&responses, uri);
    assert_eq!(batches.len(), 1);
    assert!(batches[0].is_empty(), "{:?}", batches[0]);
    let data = result_for(&responses, 2)
        .expect("tokens")
        .get("data")
        .and_then(Json::as_array)
        .expect("data")
        .iter()
        .filter_map(Json::as_i64)
        .collect::<Vec<_>>();
    let tokens = decode_semantic_tokens(&data, TWO_BOX_SIMULATION_REPRO);
    assert!(
        tokens
            .iter()
            .any(|token| token.text == "number" && token.token_type == VARIABLE),
        "{tokens:?}"
    );
    assert!(
        !tokens.iter().any(|token| token.token_type == COMMENT),
        "{tokens:?}"
    );
}

#[test]
fn lsp_adjacent_swap_pass_has_zero_diagnostics_and_no_comment_tokens() {
    let uri = "file:///tmp/tc_lsp_adjacent_swap.eng";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, ADJACENT_SWAP_REPRO),
        request(
            2,
            "textDocument/semanticTokens/full",
            object([("textDocument", text_document(uri))]),
        ),
        shutdown(),
        exit(),
    ]);
    let batches = diagnostics_after(&responses, uri);
    assert_eq!(batches.len(), 1);
    assert!(batches[0].is_empty(), "{:?}", batches[0]);
    let data = result_for(&responses, 2)
        .expect("tokens")
        .get("data")
        .and_then(Json::as_array)
        .expect("data")
        .iter()
        .filter_map(Json::as_i64)
        .collect::<Vec<_>>();
    let tokens = decode_semantic_tokens(&data, ADJACENT_SWAP_REPRO);
    assert!(
        tokens
            .iter()
            .any(|token| token.text == "numbers" && token.token_type == VARIABLE),
        "{tokens:?}"
    );
    assert!(
        !tokens.iter().any(|token| token.token_type == COMMENT),
        "{tokens:?}"
    );
}

#[test]
fn lsp_word_frequency_pipeline_has_zero_diagnostics_and_no_comment_tokens() {
    let uri = "file:///tmp/tc_lsp_word_frequency.eng";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, WORD_FREQUENCY_REPRO),
        request(
            2,
            "textDocument/semanticTokens/full",
            object([("textDocument", text_document(uri))]),
        ),
        shutdown(),
        exit(),
    ]);
    let batches = diagnostics_after(&responses, uri);
    assert_eq!(batches.len(), 1);
    assert!(batches[0].is_empty(), "{:?}", batches[0]);
    let data = result_for(&responses, 2)
        .expect("tokens")
        .get("data")
        .and_then(Json::as_array)
        .expect("data")
        .iter()
        .filter_map(Json::as_i64)
        .collect::<Vec<_>>();
    let tokens = decode_semantic_tokens(&data, WORD_FREQUENCY_REPRO);
    assert!(
        tokens.iter().any(|token| token.token_type == STRING),
        "{tokens:?}"
    );
    assert!(
        !tokens.iter().any(|token| token.token_type == COMMENT),
        "{tokens:?}"
    );
}

#[test]
fn lsp_empty_loop_body_reports_header_line() {
    let uri = "file:///tmp/tc_lsp_empty_loop.eng";
    let text = "Let x be 5.\nRepeat while x is greater than 1:\n\nPrint x.\n";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, text),
        shutdown(),
        exit(),
    ]);
    let diagnostics = diagnostics_after(&responses, uri);
    let diagnostic = diagnostics
        .iter()
        .flat_map(|batch| batch.iter())
        .find(|value| {
            value
                .get("message")
                .and_then(Json::as_str)
                .is_some_and(|message| message.contains("empty loop body"))
        })
        .expect("empty loop diagnostic");
    let start = diagnostic
        .get("range")
        .and_then(|value| value.get("start"))
        .expect("range start");
    assert_eq!(start.get("line").and_then(Json::as_i64), Some(1));
    assert_eq!(start.get("character").and_then(Json::as_i64), Some(0));
}

#[test]
fn lsp_unresolved_ellipsis_reports_anaphor_diagnostic() {
    let uri = "file:///tmp/tc_lsp_unresolved_ellipsis.eng";
    let (_, responses) = run(vec![
        request(1, "initialize", object([])),
        open(uri, "If none of them do, print \"x\".\n"),
        shutdown(),
        exit(),
    ]);
    let diagnostics = diagnostics_after(&responses, uri);
    let diagnostic = diagnostics
        .iter()
        .flat_map(|batch| batch.iter())
        .find(|value| {
            value
                .get("message")
                .and_then(Json::as_str)
                .is_some_and(|message| message.contains("UNRESOLVED_ANAPHOR"))
        })
        .expect("anaphor diagnostic");
    let start = diagnostic
        .get("range")
        .and_then(|value| value.get("start"))
        .expect("range start");
    assert_eq!(start.get("line").and_then(Json::as_i64), Some(0));
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
    assert!(data.chunks(5).any(|chunk| {
        chunk[3].as_i64() == Some(STRING) && chunk[2].as_i64().is_some_and(|len| len >= 5)
    }));
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
