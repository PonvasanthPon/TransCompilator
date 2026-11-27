//! In-process bridge to the `tc_r1` compiler crates and built-in presets.

use std::collections::{HashMap, VecDeque};
use std::fmt::Write as _;
use std::fs;
use std::io::Read as _;
use std::io::Write as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tc_ir::{format_module, interpret_in_dir, verify_module};
use tc_semantics::{
    CompiledProject, LoweredProject, SourceFile, compile_english_to_ir, find_std_dir,
    format_paraphrase, lower_project,
};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Output pane tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputTab {
    /// Native ELF64 execution plus reference-interpreter parity.
    Run,
    /// Canonical paraphrase echo.
    Paraphrase,
    /// Per-sentence syntax, lexicon, and intent trace.
    Trace,
    /// Verified Core-7 IR.
    Ir,
    /// Build a static ELF64 artifact on disk.
    Build,
}

impl OutputTab {
    /// Short uppercase label used by the UI.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Run => "RUN",
            Self::Paraphrase => "PARAPHRASE",
            Self::Trace => "TRACE",
            Self::Ir => "IR",
            Self::Build => "BUILD",
        }
    }

    /// Cycles to the next tab.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Run => Self::Paraphrase,
            Self::Paraphrase => Self::Trace,
            Self::Trace => Self::Ir,
            Self::Ir => Self::Build,
            Self::Build => Self::Run,
        }
    }
}

/// Result shown in the output pane.
#[derive(Debug, Clone)]
pub struct RunResult {
    /// True when the action completed successfully.
    pub ok: bool,
    /// One-line status banner.
    pub summary: String,
    /// Output lines.
    pub lines: Vec<String>,
    /// One-based source lines with diagnostics.
    pub error_lines: Vec<usize>,
    /// One-based narrative lines omitted by the intent gate.
    pub omitted_lines: Vec<usize>,
}

impl RunResult {
    fn failure(summary: impl Into<String>, lines: Vec<String>) -> Self {
        Self {
            ok: false,
            summary: summary.into(),
            lines,
            error_lines: Vec::new(),
            omitted_lines: Vec::new(),
        }
    }

    fn failure_with_lines(
        summary: impl Into<String>,
        lines: Vec<String>,
        error_lines: Vec<usize>,
    ) -> Self {
        let mut result = Self::failure(summary, lines);
        result.error_lines = normalized_lines(error_lines);
        result
    }
}

fn normalized_lines(mut lines: Vec<usize>) -> Vec<usize> {
    lines.sort_unstable();
    lines.dedup();
    lines
}

fn error_lines_from_message(message: &str) -> Vec<usize> {
    message
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix('[')?;
            let (number, _) = rest.split_once(':')?;
            number.parse::<usize>().ok()
        })
        .collect()
}

/// One built-in `.eng` starter program.
#[derive(Debug, Clone, Copy)]
pub struct Preset {
    /// File-style preset name.
    pub name: &'static str,
    /// Human description.
    pub description: &'static str,
    /// English source.
    pub source: &'static str,
}

/// Five starter programs that all pass exact ELF64/interpreter parity.
pub const PRESETS: [Preset; 8] = [
    Preset {
        name: "01_discourse_cash.eng",
        description: "ACE discourse, expletive prose, comma precedence, For every",
        source: concat!(
            "Yesterday Ravi called me twice.\n",
            "It was raining outside.\n\n",
            "Put 500 in cash.\n",
            "Add 100 to it.\n",
            "If it is more than 400 or cash == 0, and cash is less than 1000, remove 50 from it.\n\n",
            "Put 0 in sum.\n",
            "For every i from 1 to 5, add it to sum.\n",
            "Return (cash + sum) % 256.\n",
        ),
    },
    Preset {
        name: "02_struct_anaphora.eng",
        description: "Struct fields with its, the point's y, and appositive names",
        source: concat!(
            "Define struct Point with x as i64 and y as i64.\n",
            "Create a Point called origin.\n",
            "Set its x to 15.\n",
            "Set the point's y to 27.\n",
            "Return origin.x + origin.y.\n",
        ),
    },
    Preset {
        name: "03_quantifiers_prime.eng",
        description: "Pure-English primality with a local range quantifier",
        source: concat!(
            "Define function is_prime taking n as i64 returning i64:\n",
            "    If n is less than 2, return 0.\n",
            "    If no d from 2 to n - 1 satisfies n % d == 0, return 1.\n",
            "    Return 0.\n\n",
            "Put 0 in count.\n",
            "For every k from 2 to 30:\n",
            "    If is_prime(k) == 1, add 1 to count.\n",
            "Return count.\n",
        ),
    },
    Preset {
        name: "04_phrase_syscall_hello.eng",
        description: "Declarative Phrase wrapping a raw Linux write syscall",
        source: concat!(
            "To emit_text given buf as a pointer to byte, len as an unsigned number, named write_stdout, returning a whole number.\n",
            "Call this as \"emit <buf> with length <len>\".\n",
            "Give back ask Linux kernel 1 with 1, buf, and len.\n\n",
            "Emit bytes_ptr(\"Hello from R1 Editor (.eng -> zero-libc ELF64)!\\n\") with length bytes_len(\"Hello from R1 Editor (.eng -> zero-libc ELF64)!\\n\").\n",
            "Give back 0.\n",
        ),
    },
    Preset {
        name: "05_passive_and_transfers.eng",
        description: "Narrative omission, Transfer, passive voice, Unless",
        source: concat!(
            "Today shop was very busy because of rain.\n",
            "Put 1000 in bank.\n",
            "Put 200 in wallet.\n",
            "Transfer 300 from bank to wallet.\n",
            "50 is removed from wallet.\n",
            "Unless wallet is less than 400, add 10 to wallet.\n",
            "Return wallet % 256.\n",
        ),
    },
    Preset {
        name: "06_std_io_and_math.eng",
        description: "Natural-English std/io formatting and std/math roots",
        source: concat!(
            "Use io and math.\n\n",
            "Say 42.\n",
            "Say 3.5.\n",
            "Say true.\n",
            "Let root two be square root of 2.0.\n",
            "Say root two.\n",
            "Let root nine be square root of 9.0.\n",
            "Say root nine.\n",
            "Give back 0.\n",
        ),
    },
    Preset {
        name: "07_std_generic_list.eng",
        description: "A generic list of whole numbers, sorted naturally",
        source: concat!(
            "A list of whole numbers, sorted with a plain English comparator.\n\n",
            "Use io and list.\n\n",
            "To compare left (a pointer to whole number) with right (a pointer to whole number), named compare, giving a whole number.\n",
            "Let left value be value at left.\n",
            "Let right value be value at right.\n",
            "Give back -1 if left value is less than right value.\n",
            "Give back 1 if left value is more than right value.\n",
            "Give back 0.\n\n",
            "Let nums be List of whole number.\n",
            "Push 9 to address of nums.\n",
            "Push 1 to address of nums.\n",
            "Push 5 to address of nums.\n",
            "Sort address of nums using function compare.\n",
            "Let i be 0 as an unsigned number.\n",
            "While i is less than 3.\n",
            "Say item i of nums's data.\n",
            "Add 1 to i.\n",
            "End the loop.\n",
            "Free list address of nums.\n",
            "Give back 0.\n",
        ),
    },
    Preset {
        name: "08_std_file_and_map.eng",
        description: "Natural-English file round-trip and integer map",
        source: concat!(
            "A file round-trip and a small integer map.\n\n",
            "Use alloc, core, fs, io, map, and string.\n\n",
            "Let name be create text from \"preset.txt\" of length 10.\n",
            "Save 321 to file address of name.\n",
            "Display file address of name.\n",
            "Delete file address of name.\n",
            "Let table be IntMap of whole number.\n",
            "Let k be 0 as a whole number.\n",
            "While k is less than 20.\n",
            "Put k times 2 into int map address of table for key k.\n",
            "Add 1 to k.\n",
            "End the loop.\n",
            "Let found be look up 7 in int map address of table.\n",
            "Say found.value.\n",
            "Let map size be count of int map address of table.\n",
            "Say map size.\n",
            "Free int map address of table.\n",
            "Release text address of name.\n",
            "Let used be bytes in use.\n",
            "Say used.\n",
            "Give back 0.\n",
        ),
    },
];

/// Returns the built-in presets.
#[must_use]
pub fn presets() -> &'static [Preset] {
    &PRESETS
}

/// Returns one preset by zero-based index.
#[must_use]
pub fn preset(index: usize) -> Option<&'static Preset> {
    PRESETS.get(index)
}

fn display_name(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("untitled");
    if let Some(stem) = name
        .strip_suffix(".eng")
        .or_else(|| name.strip_suffix(".ENG"))
    {
        format!("{stem}.tc")
    } else {
        name.to_string()
    }
}

fn module_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("untitled")
        .to_ascii_lowercase()
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

fn find_import(module: &str, directory: Option<&Path>, std_dir: Option<&Path>) -> Option<PathBuf> {
    let mut directories = Vec::new();
    if let Some(directory) = directory {
        directories.push(directory.to_path_buf());
    }
    if let Some(std_dir) = std_dir {
        if !directories.contains(&std_dir.to_path_buf()) {
            directories.push(std_dir.to_path_buf());
        }
    }
    for directory in directories {
        let english = directory.join(format!("{module}.eng"));
        if english.is_file() {
            return Some(english);
        }
        let legacy = directory.join(format!("{module}.tc"));
        if legacy.is_file() {
            return Some(legacy);
        }
    }
    None
}

fn load_sources(
    source: &str,
    file_path: Option<&Path>,
) -> Result<(Vec<SourceFile>, String), String> {
    let entry_module = file_path.map_or_else(|| "untitled".to_string(), module_name);
    let entry_name = file_path.map_or_else(|| "untitled.tc".to_string(), display_name);
    let directory = file_path.and_then(Path::parent).map(Path::to_path_buf);
    let std_dir = find_std_dir();
    let entry = SourceFile::new(entry_name, source.to_string());
    let mut queue = VecDeque::from([(entry, directory)]);
    let mut seen = HashMap::from([(entry_module.clone(), file_path.map(Path::to_path_buf))]);
    let mut sources = Vec::new();
    while let Some((current, current_dir)) = queue.pop_front() {
        for import in module_imports(&current) {
            if seen.contains_key(&import) {
                continue;
            }
            let Some(path) = find_import(&import, current_dir.as_deref(), std_dir.as_deref())
            else {
                return Err(format!(
                    "imported module `{import}` was not found next to the active .eng file or in std/"
                ));
            };
            let imported = SourceFile::new(
                display_name(&path),
                fs::read_to_string(&path)
                    .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?,
            );
            let imported_dir = path.parent().map(Path::to_path_buf);
            seen.insert(import, Some(path));
            queue.push_back((imported, imported_dir));
        }
        sources.push(current);
    }
    Ok((sources, entry_module))
}

fn format_semantic_errors(errors: &[tc_semantics::SemanticError]) -> String {
    errors
        .iter()
        .map(|error| {
            format!(
                "[{}:{}] {:?}: {}",
                error.line, error.col, error.kind, error.message
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn compile(source: &str, file_path: Option<&Path>) -> Result<CompiledProject, String> {
    let (sources, entry) = load_sources(source, file_path)?;
    compile_english_to_ir(&sources, &entry).map_err(|errors| format_semantic_errors(&errors))
}

fn lower(source: &str, file_path: Option<&Path>) -> Result<LoweredProject, String> {
    let (sources, entry) = load_sources(source, file_path)?;
    lower_project(&sources, &entry).map_err(|errors| format_semantic_errors(&errors))
}

fn lines_of(text: &str) -> Vec<String> {
    if text.is_empty() {
        Vec::new()
    } else {
        text.lines().map(str::to_string).collect()
    }
}

#[derive(Debug)]
struct ProcessOutput {
    exit_code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    timed_out: bool,
}

fn execute_elf(
    path: &Path,
    directory: Option<&Path>,
    stdin_text: &str,
    timeout: Duration,
) -> std::io::Result<ProcessOutput> {
    let mut command = Command::new(path);
    command
        .arg0("program")
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
    let mut child = command.spawn()?;
    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        return Err(std::io::Error::other("child stdout pipe was not created"));
    };
    let Some(mut stderr) = child.stderr.take() else {
        let _ = child.kill();
        return Err(std::io::Error::other("child stderr pipe was not created"));
    };
    let out_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stdout.read_to_end(&mut buffer);
        buffer
    });
    let err_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stderr.read_to_end(&mut buffer);
        buffer
    });
    if let Some(mut input) = child.stdin.take() {
        let _ = input.write_all(stdin_text.as_bytes());
    }
    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            timed_out = true;
            break child.wait()?;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    Ok(ProcessOutput {
        exit_code: status.code().unwrap_or(-1),
        stdout,
        stderr,
        timed_out,
    })
}

fn temp_elf_path() -> PathBuf {
    let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("r1_editor_{}_{}.elf", std::process::id(), sequence))
}

/// Compiles, interprets, emits ELF64, executes it, and checks exact parity.
#[must_use]
pub fn run_elf_with_parity(source: &str, file_path: Option<&Path>, stdin_text: &str) -> RunResult {
    let compiled = match compile(source, file_path) {
        Ok(value) => value,
        Err(message) => {
            return RunResult::failure_with_lines(
                "COMPILE FAILED",
                lines_of(&message),
                error_lines_from_message(&message),
            );
        }
    };
    if let Err(errors) = verify_module(&compiled.module) {
        return RunResult::failure(
            "IR VERIFY FAILED",
            errors
                .iter()
                .map(|error| format!("{error:?}"))
                .collect::<Vec<_>>(),
        );
    }
    let arguments = vec!["program".to_string()];
    let work = std::env::temp_dir().join(format!(
        "r1_editor_run_{}_{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let interp_dir = work.join("interp");
    let elf_dir = work.join("elf");
    let _ = fs::create_dir_all(&interp_dir);
    let _ = fs::create_dir_all(&elf_dir);
    let interpreted = match interpret_in_dir(
        &compiled.module,
        &arguments,
        stdin_text.as_bytes(),
        &interp_dir,
        50_000_000,
    ) {
        Ok(value) => value,
        Err(error) => {
            let _ = fs::remove_dir_all(&work);
            return RunResult::failure("INTERPRETER FAILED", vec![format!("{error:?}")]);
        }
    };
    let path = temp_elf_path();
    if let Err(error) = tc_codegen_x86::compile_to_elf(&compiled.module, &path) {
        return RunResult::failure("ELF64 BUILD FAILED", vec![error.to_string()]);
    }
    let byte_size = fs::metadata(&path).map_or(0, |metadata| metadata.len());
    let native = match execute_elf(&path, Some(&elf_dir), stdin_text, Duration::from_secs(5)) {
        Ok(value) => value,
        Err(error) => {
            let _ = fs::remove_file(&path);
            let _ = fs::remove_dir_all(&work);
            return RunResult::failure(
                "ELF64 EXECUTION FAILED",
                vec![format!("cannot execute `{}`: {error}", path.display())],
            );
        }
    };
    let _ = fs::remove_file(&path);
    let _ = fs::remove_dir_all(&work);
    if native.timed_out {
        return RunResult::failure(
            "ELF64 TIMEOUT",
            vec!["program exceeded the 5-second editor timeout".to_string()],
        );
    }
    let parity = native.exit_code == interpreted.exit_code
        && native.stdout == interpreted.stdout
        && native.stderr == interpreted.stderr;
    let status = if parity { "PASS" } else { "FAIL" };
    let parity_text = if parity {
        "EXACT MATCH (ELF64 == IR Interpreter)"
    } else {
        "MISMATCH"
    };
    let summary = format!(
        "ELF64 {status} (exit={}, {byte_size}B) | PARITY: {parity_text}",
        native.exit_code
    );
    let mut lines = vec![
        format!("exit code      : {}", native.exit_code),
        format!("ELF64 size     : {byte_size} bytes"),
        format!("reference exit : {}", interpreted.exit_code),
    ];
    if !native.stdout.is_empty() {
        lines.push("--- stdout ---".to_string());
        lines.extend(lines_of(&String::from_utf8_lossy(&native.stdout)));
    }
    if !native.stderr.is_empty() {
        lines.push("--- stderr ---".to_string());
        lines.extend(lines_of(&String::from_utf8_lossy(&native.stderr)));
    }
    if !parity {
        lines.push("--- interpreter stdout ---".to_string());
        lines.extend(lines_of(&String::from_utf8_lossy(&interpreted.stdout)));
        lines.push("--- interpreter stderr ---".to_string());
        lines.extend(lines_of(&String::from_utf8_lossy(&interpreted.stderr)));
    }
    RunResult {
        ok: parity,
        summary,
        lines,
        error_lines: Vec::new(),
        omitted_lines: compiled
            .trace
            .iter()
            .filter(|entry| entry.intent.is_comment())
            .map(|entry| entry.line as usize)
            .collect(),
    }
}

/// Returns the canonical paraphrase echo.
#[must_use]
pub fn run_paraphrase(source: &str, file_path: Option<&Path>) -> RunResult {
    match compile(source, file_path) {
        Ok(compiled) => {
            let text = format_paraphrase(&compiled.paraphrase);
            let lines = lines_of(&text);
            RunResult {
                ok: true,
                summary: format!("PARAPHRASE ({} lines)", lines.len()),
                lines,
                error_lines: Vec::new(),
                omitted_lines: compiled
                    .trace
                    .iter()
                    .filter(|entry| entry.intent.is_comment())
                    .map(|entry| entry.line as usize)
                    .collect(),
            }
        }
        Err(message) => RunResult::failure_with_lines(
            "PARAPHRASE FAILED",
            lines_of(&message),
            error_lines_from_message(&message),
        ),
    }
}

/// Returns the per-sentence trace.
#[must_use]
pub fn run_trace(source: &str, file_path: Option<&Path>) -> RunResult {
    let lowered = match lower(source, file_path) {
        Ok(value) => value,
        Err(message) => {
            return RunResult::failure_with_lines(
                "TRACE FAILED",
                lines_of(&message),
                error_lines_from_message(&message),
            );
        }
    };
    let mut lines = Vec::new();
    for entry in &lowered.trace {
        let mut line = format!(
            "{}:{}:{} {} lg={} words={} links={} voice={}",
            entry.module,
            entry.line,
            entry.col,
            entry.intent,
            entry.lg_used,
            entry.lg_words,
            entry.lg_links,
            entry.voice
        );
        if let Some(verb) = &entry.verb {
            let _ = write!(line, " verb={verb}");
        }
        if let Some(lemma) = &entry.lemma {
            let _ = write!(line, " lemma={lemma}");
        }
        if let Some(predicate) = entry.predicate {
            let _ = write!(line, " predicate={predicate:?}");
        }
        if let Some(concept) = entry.concept {
            let _ = write!(line, " concept={concept:?}");
        }
        if let Some(class_id) = entry.class_id {
            let _ = write!(line, " verbnet={class_id}");
        }
        if let Some(synset_id) = entry.synset_id {
            let _ = write!(line, " wordnet={synset_id}");
        }
        lines.push(line);
        lines.push(format!("  {}", entry.text));
    }
    let unsupported = lowered
        .trace
        .iter()
        .any(|entry| entry.intent.is_unsupported());
    RunResult {
        ok: !unsupported,
        summary: format!(
            "TRACE ({} sentences){}",
            lowered.trace.len(),
            if unsupported {
                " | UNSUPPORTED PRESENT"
            } else {
                ""
            }
        ),
        lines,
        error_lines: lowered
            .trace
            .iter()
            .filter(|entry| entry.intent.is_unsupported())
            .map(|entry| entry.line as usize)
            .collect(),
        omitted_lines: lowered
            .trace
            .iter()
            .filter(|entry| entry.intent.is_comment())
            .map(|entry| entry.line as usize)
            .collect(),
    }
}

/// Returns formatted Core-7 IR.
#[must_use]
pub fn run_ir(source: &str, file_path: Option<&Path>) -> RunResult {
    match compile(source, file_path) {
        Ok(compiled) => {
            let text = format_module(&compiled.module);
            let lines = lines_of(&text);
            RunResult {
                ok: true,
                summary: format!("TC-IR ({} lines)", lines.len()),
                lines,
                error_lines: Vec::new(),
                omitted_lines: compiled
                    .trace
                    .iter()
                    .filter(|entry| entry.intent.is_comment())
                    .map(|entry| entry.line as usize)
                    .collect(),
            }
        }
        Err(message) => RunResult::failure_with_lines(
            "IR FAILED",
            lines_of(&message),
            error_lines_from_message(&message),
        ),
    }
}

fn static_check(path: &Path) -> (bool, String) {
    match Command::new("readelf").arg("-d").arg(path).output() {
        Ok(output) => {
            let text = String::from_utf8_lossy(&output.stdout);
            let ok = text.contains("There is no dynamic section");
            (ok, text.trim().to_string())
        }
        Err(error) => (false, format!("readelf unavailable: {error}")),
    }
}

/// Builds `<stem>.elf` next to the active file, or `./out.elf` when unsaved.
#[must_use]
pub fn build_elf_artifact(source: &str, file_path: Option<&Path>) -> RunResult {
    let compiled = match compile(source, file_path) {
        Ok(value) => value,
        Err(message) => {
            return RunResult::failure_with_lines(
                "BUILD FAILED",
                lines_of(&message),
                error_lines_from_message(&message),
            );
        }
    };
    let target = file_path.map_or_else(
        || PathBuf::from("out.elf"),
        |path| path.with_extension("elf"),
    );
    if let Err(error) = tc_codegen_x86::compile_to_elf(&compiled.module, &target) {
        return RunResult::failure("BUILD FAILED", vec![error.to_string()]);
    }
    let byte_size = fs::metadata(&target).map_or(0, |metadata| metadata.len());
    let (static_ok, evidence) = static_check(&target);
    let mut lines = vec![
        format!("artifact : {}", target.display()),
        format!("size     : {byte_size} bytes"),
        format!("readelf  : {evidence}"),
    ];
    if !static_ok {
        lines.push("warning  : could not prove a static zero-libc ELF64".to_string());
    }
    RunResult {
        ok: static_ok,
        summary: format!(
            "BUILT {} ({byte_size}B) | STATIC ELF64: {}",
            target.display(),
            if static_ok { "yes" } else { "no" }
        ),
        lines,
        error_lines: Vec::new(),
        omitted_lines: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{PRESETS, run_elf_with_parity};

    #[test]
    fn all_presets_have_unique_names() {
        let mut names = PRESETS.iter().map(|preset| preset.name).collect::<Vec<_>>();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 8);
    }

    #[test]
    fn first_preset_runs() {
        let result = run_elf_with_parity(PRESETS[0].source, None, "");
        assert!(result.ok, "{result:?}");
    }
}
