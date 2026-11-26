//! Zero-dependency WebAssembly bridge for the browser playground.
//!
//! The module exposes a tiny C ABI over linear memory.  JavaScript allocates a
//! source buffer with [`tc_alloc`], copies UTF-8 English into it, calls
//! [`tc_eval`], reads the returned JSON buffer, and releases both buffers with
//! [`tc_dealloc`].  No `wasm-bindgen`, `serde`, or `js-sys` is involved.
//!
//! The compiler pipeline is the same five crates used by the native CLI:
//! `tc-semantics` → `tc-typeck` → `tc-ir::verify` → `tc-ir::interp`.  The
//! syntax crate's closed-class fallback replaces Link Grammar on wasm32, and
//! `tc-semantics` embeds the eight `std/*.eng` modules instead of reading the
//! host filesystem.

use std::alloc::{Layout, alloc, alloc_zeroed, dealloc};
use std::fmt::Write as _;

use tc_ir::{InterpError, format_module, interpret, verify_module};
use tc_semantics::{
    IntentVerdict, LowerProjectOutcome, SentenceTrace, SourceFile, format_paraphrase,
    lower_project_outcome,
};

/// Default interpreter budget when the caller passes zero.
const DEFAULT_MAX_STEPS: usize = 2_000_000;

/// One structured playground diagnostic.
struct Diagnostic {
    line: u32,
    col: u32,
    severity: &'static str,
    category: &'static str,
    message: String,
}

/// Everything the playground renders for one evaluation.
#[derive(Default)]
struct Evaluation {
    ok: bool,
    stdout: String,
    stderr: String,
    exit_code: i32,
    steps: usize,
    diagnostics: Vec<Diagnostic>,
    trace: Vec<SentenceTrace>,
    paraphrase: String,
    ir: String,
}

impl Evaluation {
    fn push_error(&mut self, line: u32, col: u32, category: &'static str, message: String) {
        self.diagnostics.push(Diagnostic {
            line,
            col,
            severity: "error",
            category,
            message,
        });
    }

    fn to_json(&self) -> String {
        let mut json = String::with_capacity(4096);
        json.push_str("{\"ok\":");
        json.push_str(if self.ok { "true" } else { "false" });
        json.push_str(",\"version\":");
        push_string(&mut json, env!("CARGO_PKG_VERSION"));
        json.push_str(",\"stdout\":");
        push_string(&mut json, &self.stdout);
        json.push_str(",\"stderr\":");
        push_string(&mut json, &self.stderr);
        let _ = write!(json, ",\"exit_code\":{},", self.exit_code);
        let _ = write!(json, "\"steps\":{},", self.steps);
        json.push_str("\"diagnostics\":[");
        for (index, diagnostic) in self.diagnostics.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            let _ = write!(
                json,
                "{{\"line\":{},\"col\":{},\"severity\":",
                diagnostic.line, diagnostic.col
            );
            push_string(&mut json, diagnostic.severity);
            json.push_str(",\"category\":");
            push_string(&mut json, diagnostic.category);
            json.push_str(",\"message\":");
            push_string(&mut json, &diagnostic.message);
            json.push('}');
        }
        json.push_str("],\"trace\":[");
        for (index, trace) in self.trace.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            push_trace(&mut json, trace);
        }
        json.push_str("],\"paraphrase\":");
        push_string(&mut json, &self.paraphrase);
        json.push_str(",\"ir\":");
        push_string(&mut json, &self.ir);
        json.push('}');
        json
    }
}

fn push_trace(json: &mut String, trace: &SentenceTrace) {
    let (intent, reason) = match &trace.intent {
        IntentVerdict::Exec => ("EXEC", None),
        IntentVerdict::CommentOmitted { reason } => ("COMMENT_OMITTED", Some(reason.as_str())),
        IntentVerdict::Unsupported { reason } => ("UNSUPPORTED", Some(reason.as_str())),
    };
    let _ = write!(json, "{{\"line\":{},\"col\":{},", trace.line, trace.col);
    json.push_str("\"text\":");
    push_string(json, &trace.text);
    json.push_str(",\"intent\":");
    push_string(json, intent);
    json.push_str(",\"reason\":");
    push_optional_string(json, reason);
    let _ = write!(
        json,
        ",\"lg_used\":{},\"lg_words\":{},\"lg_links\":{},\"voice\":",
        trace.lg_used, trace.lg_words, trace.lg_links
    );
    push_string(json, &trace.voice.to_string());
    json.push_str(",\"verb\":");
    push_optional_string(json, trace.verb.as_deref());
    json.push_str(",\"lemma\":");
    push_optional_string(json, trace.lemma.as_deref());
    json.push_str(",\"predicate\":");
    push_optional_string(
        json,
        trace.predicate.map(|value| format!("{value:?}")).as_deref(),
    );
    json.push_str(",\"concept\":");
    push_optional_string(
        json,
        trace.concept.map(|value| format!("{value:?}")).as_deref(),
    );
    json.push_str(",\"class_id\":");
    push_optional_string(json, trace.class_id);
    json.push_str(",\"synset_id\":");
    push_optional_string(json, trace.synset_id);
    json.push_str(",\"fillers\":[");
    for (index, filler) in trace.fillers.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        push_string(json, filler);
    }
    json.push_str("]}");
}

fn push_optional_string(json: &mut String, value: Option<&str>) {
    match value {
        Some(text) => push_string(json, text),
        None => json.push_str("null"),
    }
}

fn push_string(json: &mut String, text: &str) {
    json.push('"');
    for character in text.chars() {
        match character {
            '"' => json.push_str("\\\""),
            '\\' => json.push_str("\\\\"),
            '\n' => json.push_str("\\n"),
            '\r' => json.push_str("\\r"),
            '\t' => json.push_str("\\t"),
            '\u{0008}' => json.push_str("\\b"),
            '\u{000c}' => json.push_str("\\f"),
            '\u{2028}' => json.push_str("\\u2028"),
            '\u{2029}' => json.push_str("\\u2029"),
            value if value.is_control() => {
                let _ = write!(json, "\\u{:04x}", u32::from(value));
            }
            value => json.push(value),
        }
    }
    json.push('"');
}

fn evaluate(source: &str, max_steps: usize) -> Evaluation {
    let mut evaluation = Evaluation::default();
    let sources = [SourceFile::new("program.tc", source.to_string())];
    let LowerProjectOutcome {
        project,
        trace,
        errors,
    } = lower_project_outcome(&sources, "program");

    for error in &errors {
        evaluation.push_error(
            error.line,
            error.col,
            error_kind(error.kind),
            error.message.clone(),
        );
    }

    evaluation.trace = trace;

    if let Some(project) = &project {
        for entry in &project.trace {
            if let IntentVerdict::Unsupported { reason } = &entry.intent {
                evaluation.push_error(entry.line, entry.col, "UNSUPPORTED", reason.clone());
            }
        }
        evaluation.paraphrase = format_paraphrase(&project.paraphrase);
    }

    let mut module = None;
    if evaluation.diagnostics.is_empty() {
        if let Some(project) = &project {
            match tc_typeck::check_and_lower(&project.program) {
                Ok(lowered) => match verify_module(&lowered) {
                    Ok(()) => module = Some(lowered),
                    Err(errors) => {
                        for error in errors {
                            evaluation.push_error(
                                1,
                                1,
                                "IR",
                                format!("Core-7 verification failed: {error:?}"),
                            );
                        }
                    }
                },
                Err(errors) => {
                    for error in errors {
                        evaluation.push_error(
                            error.span.line,
                            error.span.col,
                            "TYPE",
                            error.message,
                        );
                    }
                }
            }
        }
    }

    if let Some(module) = &module {
        let arguments = [String::from("program.eng")];
        match interpret(module, &arguments, &[], max_steps) {
            Ok(output) => {
                evaluation.stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                evaluation.stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                evaluation.exit_code = output.exit_code;
                evaluation.steps = output.steps;
                evaluation.ok = evaluation.diagnostics.is_empty();
            }
            Err(error) => {
                if let InterpError::StepLimit(limit) = error {
                    evaluation.steps = limit;
                }
                evaluation.push_error(1, 1, "RUNTIME", interpreter_message(&error));
                evaluation.exit_code = 1;
            }
        }
    }

    if !evaluation.diagnostics.is_empty() {
        evaluation.ok = false;
        let mut report = String::new();
        for diagnostic in &evaluation.diagnostics {
            let _ = writeln!(
                report,
                "{}:{}: {}",
                diagnostic.line, diagnostic.col, diagnostic.message
            );
        }
        if !evaluation.stderr.is_empty() {
            report.push_str(&evaluation.stderr);
            report.push('\n');
        }
        evaluation.stderr = report;
    }

    if let Some(module) = &module {
        evaluation.ir = format_module(module);
    }

    evaluation
}

fn error_kind(kind: tc_semantics::SemanticErrorKind) -> &'static str {
    match kind {
        tc_semantics::SemanticErrorKind::Segmentation => "SEGMENTATION",
        tc_semantics::SemanticErrorKind::Declaration => "DECLARATION",
        tc_semantics::SemanticErrorKind::Lowering => "LOWERING",
        tc_semantics::SemanticErrorKind::Unsupported => "UNSUPPORTED",
        tc_semantics::SemanticErrorKind::TypeCheck => "TYPE",
    }
}

fn interpreter_message(error: &InterpError) -> String {
    match error {
        InterpError::StepLimit(limit) => format!(
            "Execution stopped: exceeded {}-step safety limit (possible infinite loop)",
            grouped(*limit)
        ),
        InterpError::InvalidModule(errors) => {
            format!("Core-7 verification failed: {}", errors.join("; "))
        }
        InterpError::Trap => "Execution stopped: Core-7 trap instruction reached.".to_string(),
        InterpError::MemoryFault(address) => {
            format!("Execution stopped: invalid memory access at 0x{address:x}.")
        }
        InterpError::DivisionByZero => "Execution stopped: integer division by zero.".to_string(),
        InterpError::BadFunctionPointer(address) => {
            format!("Execution stopped: unknown function address 0x{address:x}.")
        }
        InterpError::TypeError(message) => {
            format!("Execution stopped: runtime type error: {message}")
        }
        InterpError::StackOverflow => {
            "Execution stopped: the 64 MiB virtual stack limit was exceeded.".to_string()
        }
        InterpError::HeapExhausted => {
            "Execution stopped: the 64 MiB virtual heap limit was exceeded.".to_string()
        }
        InterpError::CallDepth => {
            "Execution stopped: the call-depth limit was exceeded.".to_string()
        }
        InterpError::Internal(message) => {
            format!("Execution stopped: internal interpreter error: {message}")
        }
    }
}

fn grouped(value: usize) -> String {
    let digits = value.to_string();
    let mut output = String::with_capacity(digits.len() + digits.len() / 3);
    let offset = digits.len() % 3;
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (index + 3 - offset) % 3 == 0 {
            output.push(',');
        }
        output.push(character);
    }
    output
}

/// Allocates a zeroed source buffer of `len` bytes.
///
/// Returns a null pointer when the allocation cannot be satisfied.  A zero
/// length returns a non-null dangling pointer.
#[unsafe(no_mangle)]
pub extern "C" fn tc_alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::dangling_mut::<u8>();
    }
    let Some(layout) = byte_layout(len) else {
        return std::ptr::null_mut();
    };
    // SAFETY: `layout` is non-zero and valid; the returned block is owned by
    // the JavaScript caller until it passes the same layout to `tc_dealloc`.
    unsafe { alloc_zeroed(layout) }
}

/// Releases a buffer previously returned by [`tc_alloc`] or [`tc_eval`].
///
/// The wasm C ABI keeps this entry point safe for JavaScript: the caller owns
/// the linear-memory contract, and `ptr` must have been produced by
/// [`tc_alloc`] or [`tc_eval`] with the same `len`.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn tc_dealloc(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    let Some(layout) = byte_layout(len) else {
        return;
    };
    // SAFETY: `ptr` was allocated by `tc_alloc` or `emit` with exactly this
    // layout, and ownership was transferred to the caller.  It is freed once.
    unsafe { dealloc(ptr, layout) }
}

fn byte_layout(len: usize) -> Option<Layout> {
    Layout::from_size_align(len, 1).ok()
}

/// Compiles and runs UTF-8 English source, returning a packed JSON buffer.
///
/// The low 32 bits of the result are the buffer length and the high 32 bits
/// are its linear-memory address.  The caller must release the buffer with
/// [`tc_dealloc`].  A `max_steps` value of zero selects the default
/// 2,000,000-step budget.
///
/// The wasm C ABI keeps this entry point safe for JavaScript: `ptr` must point
/// to at least `len` readable bytes in the module's linear memory.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn tc_eval(ptr: *const u8, len: usize, max_steps: u32) -> u64 {
    let budget = if max_steps == 0 {
        DEFAULT_MAX_STEPS
    } else {
        max_steps as usize
    };
    if len > 0 && ptr.is_null() {
        return emit(fatal_json("Source buffer pointer was null."));
    }
    // SAFETY: the caller supplies `(ptr, len)` from `tc_alloc` and keeps the
    // buffer alive for the duration of this call.  `len == 0` never dereferences.
    let bytes = unsafe {
        if len == 0 {
            &[][..]
        } else {
            std::slice::from_raw_parts(ptr, len)
        }
    };
    let source = match std::str::from_utf8(bytes) {
        Ok(source) => source,
        Err(_) => {
            return emit(fatal_json(
                "Source is not valid UTF-8; the editor must send UTF-8 text.",
            ));
        }
    };
    let json = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        evaluate(source, budget).to_json()
    }))
    .unwrap_or_else(|_| {
        fatal_json("The compiler could not process this input; no partial result was kept.")
    });
    emit(json)
}

fn fatal_json(message: &str) -> String {
    let mut evaluation = Evaluation::default();
    evaluation.push_error(1, 1, "INPUT", message.to_string());
    evaluation.stderr = message.to_string();
    evaluation.to_json()
}

fn emit(json: String) -> u64 {
    let bytes = json.into_bytes();
    let len = bytes.len();
    let Some(layout) = byte_layout(len) else {
        return 0;
    };
    // SAFETY: `layout` is valid and non-zero; the caller frees the copy with
    // `tc_dealloc` using the same length.
    let ptr = unsafe { alloc(layout) };
    if ptr.is_null() {
        return 0;
    }
    // SAFETY: both buffers are valid for `len` bytes and do not overlap.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, len) };
    ((ptr as usize as u64) << 32) | (len as u64 & 0xffff_ffff)
}
