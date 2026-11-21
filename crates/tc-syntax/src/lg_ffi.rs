//! Minimal safe wrapper around the vendored CMU Link Grammar 5.13.2 parser.
//!
//! The C library is loaded through the link step declared in `build.rs`.  All
//! raw pointers are confined to this module; callers receive plain Rust data
//! and every C allocation is released through a `Drop` guard.

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[repr(C)]
struct DictionaryOpaque {
    _private: [u8; 0],
}

#[repr(C)]
struct ParseOptionsOpaque {
    _private: [u8; 0],
}

#[repr(C)]
struct SentenceOpaque {
    _private: [u8; 0],
}

#[repr(C)]
struct LinkageOpaque {
    _private: [u8; 0],
}

#[repr(C)]
struct ErrorInfo {
    severity: c_int,
    severity_label: *const c_char,
    text: *const c_char,
}

type DictionaryPtr = *mut DictionaryOpaque;
type ParseOptionsPtr = *mut ParseOptionsOpaque;
type SentencePtr = *mut SentenceOpaque;
type LinkagePtr = *mut LinkageOpaque;

unsafe extern "C" {
    fn dictionary_set_data_dir(path: *const c_char);
    fn dictionary_create_lang(lang: *const c_char) -> DictionaryPtr;
    fn dictionary_delete(dict: DictionaryPtr);

    fn parse_options_create() -> ParseOptionsPtr;
    fn parse_options_delete(opts: ParseOptionsPtr) -> c_int;
    fn parse_options_set_linkage_limit(opts: ParseOptionsPtr, limit: c_int);
    fn parse_options_set_max_null_count(opts: ParseOptionsPtr, count: c_int);
    fn parse_options_set_spell_guess(opts: ParseOptionsPtr, guess: c_int);
    fn parse_options_set_verbosity(opts: ParseOptionsPtr, verbosity: c_int);
    fn parse_options_set_max_parse_time(opts: ParseOptionsPtr, seconds: c_int);

    fn sentence_create(input: *const c_char, dict: DictionaryPtr) -> SentencePtr;
    fn sentence_delete(sentence: SentencePtr);
    fn sentence_parse(sentence: SentencePtr, opts: ParseOptionsPtr) -> c_int;
    fn sentence_num_linkages_found(sentence: SentencePtr) -> c_int;

    fn linkage_create(index: usize, sentence: SentencePtr, opts: ParseOptionsPtr) -> LinkagePtr;
    fn linkage_delete(linkage: LinkagePtr);
    fn linkage_get_num_words(linkage: LinkagePtr) -> usize;
    fn linkage_get_num_links(linkage: LinkagePtr) -> usize;
    fn linkage_get_word(linkage: LinkagePtr, index: usize) -> *const c_char;
    fn linkage_get_link_lword(linkage: LinkagePtr, index: usize) -> usize;
    fn linkage_get_link_rword(linkage: LinkagePtr, index: usize) -> usize;
    fn linkage_get_link_label(linkage: LinkagePtr, index: usize) -> *const c_char;

    fn linkgrammar_get_version() -> *const c_char;
    fn lg_error_set_handler(
        handler: Option<unsafe extern "C" fn(*mut ErrorInfo, *mut c_void)>,
        data: *mut c_void,
    );
}

extern "C" fn ignore_error(_info: *mut ErrorInfo, _data: *mut c_void) {}

/// A syntax-layer failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    /// Human-readable explanation.
    pub message: String,
}

impl SyntaxError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SyntaxError {}

/// One labelled dependency link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LgLink {
    /// Left word index, counting the leading wall.
    pub left: usize,
    /// Right word index, counting the leading wall.
    pub right: usize,
    /// Link Grammar label such as `Sp`, `Pv`, `MVp`, or `Ju`.
    pub label: String,
}

/// A single deterministic Link Grammar linkage.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LgParse {
    /// Tokenized words reported by Link Grammar, including the left and right
    /// wall pseudo-words.
    pub words: Vec<String>,
    /// Labelled dependency links.
    pub links: Vec<LgLink>,
    /// Number of complete linkages that Link Grammar discovered.
    pub num_linkages: usize,
}

impl LgParse {
    /// Returns true when a complete linkage was produced.
    #[must_use]
    pub fn is_used(&self) -> bool {
        self.num_linkages > 0 && !self.links.is_empty()
    }
}

fn data_dir_candidate() -> Result<PathBuf, SyntaxError> {
    let relative =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/link-grammar/share/link-grammar");
    if relative.join("en/4.0.dict").is_file() {
        return Ok(relative.canonicalize().unwrap_or_else(|_| relative.clone()));
    }
    let executable = std::fs::read_link("/proc/self/exe")
        .map_err(|error| SyntaxError::new(format!("cannot locate /proc/self/exe: {error}")))?;
    for ancestor in executable.ancestors() {
        let candidate = ancestor.join("vendor/link-grammar/share/link-grammar");
        if candidate.join("en/4.0.dict").is_file() {
            return Ok(candidate);
        }
    }
    Err(SyntaxError::new(
        "cannot locate vendor/link-grammar/share/link-grammar",
    ))
}

fn initialize() -> Result<PathBuf, SyntaxError> {
    static DIRECTORY: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    DIRECTORY
        .get_or_init(|| {
            let directory = data_dir_candidate().map_err(|error| error.message)?;
            let text = directory.to_string_lossy();
            let c_path = CString::new(text.as_bytes())
                .map_err(|_| "link-grammar data path contains NUL".to_string())?;
            // SAFETY: the pointer is valid for the duration of the call, and
            // the two library initialisers are process-global by design.
            unsafe {
                lg_error_set_handler(Some(ignore_error), std::ptr::null_mut());
                dictionary_set_data_dir(c_path.as_ptr());
            }
            Ok(directory)
        })
        .clone()
        .map_err(SyntaxError::new)
}

struct SentenceGuard(SentencePtr);

impl Drop for SentenceGuard {
    fn drop(&mut self) {
        // SAFETY: the pointer was produced by `sentence_create` and is freed
        // exactly once here.
        unsafe { sentence_delete(self.0) };
    }
}

struct LinkageGuard(LinkagePtr);

impl Drop for LinkageGuard {
    fn drop(&mut self) {
        // SAFETY: the pointer was produced by `linkage_create` and is freed
        // exactly once here.
        unsafe { linkage_delete(self.0) };
    }
}

/// A reusable Link Grammar parser with a private dictionary and parse options.
pub struct LinkGrammarParser {
    dictionary: DictionaryPtr,
    options: ParseOptionsPtr,
}

impl fmt::Debug for LinkGrammarParser {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("LinkGrammarParser")
    }
}

impl LinkGrammarParser {
    /// Creates a parser for the vendored English dictionary.
    ///
    /// # Errors
    ///
    /// Returns [`SyntaxError`] when the dictionary or parse options cannot be
    /// created, or when the vendored data directory cannot be located.
    pub fn new() -> Result<Self, SyntaxError> {
        initialize()?;
        // SAFETY: every call checks its returned pointer before use.
        unsafe {
            let dictionary = dictionary_create_lang(c"en".as_ptr());
            if dictionary.is_null() {
                return Err(SyntaxError::new(
                    "Link Grammar could not create the English dictionary",
                ));
            }
            let options = parse_options_create();
            if options.is_null() {
                dictionary_delete(dictionary);
                return Err(SyntaxError::new(
                    "Link Grammar could not create parse options",
                ));
            }
            parse_options_set_linkage_limit(options, 1);
            parse_options_set_max_null_count(options, 0);
            parse_options_set_spell_guess(options, 0);
            parse_options_set_verbosity(options, 0);
            parse_options_set_max_parse_time(options, 2);
            Ok(Self {
                dictionary,
                options,
            })
        }
    }

    /// Returns the upstream Link Grammar version string.
    #[must_use]
    pub fn version(&self) -> String {
        // SAFETY: the library returns a static NUL-terminated string.
        unsafe {
            let pointer = linkgrammar_get_version();
            if pointer.is_null() {
                return String::new();
            }
            CStr::from_ptr(pointer).to_string_lossy().into_owned()
        }
    }

    /// Parses one sentence.  A sentence with no complete linkage yields an
    /// empty [`LgParse`] rather than an error, allowing the deterministic
    /// closed-class fallback in [`crate::clause`] to take over.
    #[must_use]
    pub fn parse(&self, sentence: &str) -> LgParse {
        let Ok(input) = CString::new(sentence) else {
            return LgParse::default();
        };
        // SAFETY: all pointers are validated and guarded; all strings returned
        // by the library are copied before their owners are released.
        unsafe {
            let raw_sentence = sentence_create(input.as_ptr(), self.dictionary);
            if raw_sentence.is_null() {
                return LgParse::default();
            }
            let sentence_guard = SentenceGuard(raw_sentence);
            let parsed = sentence_parse(sentence_guard.0, self.options);
            if parsed <= 0 {
                return LgParse::default();
            }
            let num_linkages = sentence_num_linkages_found(sentence_guard.0).max(0) as usize;
            let raw_linkage = linkage_create(0, sentence_guard.0, self.options);
            if raw_linkage.is_null() {
                return LgParse {
                    words: Vec::new(),
                    links: Vec::new(),
                    num_linkages,
                };
            }
            let linkage_guard = LinkageGuard(raw_linkage);
            let word_count = linkage_get_num_words(linkage_guard.0);
            let link_count = linkage_get_num_links(linkage_guard.0);
            let mut words = Vec::with_capacity(word_count);
            for index in 0..word_count {
                let pointer = linkage_get_word(linkage_guard.0, index);
                if pointer.is_null() {
                    words.push(String::new());
                } else {
                    words.push(CStr::from_ptr(pointer).to_string_lossy().into_owned());
                }
            }
            let mut links = Vec::with_capacity(link_count);
            for index in 0..link_count {
                let label_ptr = linkage_get_link_label(linkage_guard.0, index);
                let label = if label_ptr.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(label_ptr).to_string_lossy().into_owned()
                };
                if label.is_empty() {
                    continue;
                }
                links.push(LgLink {
                    left: linkage_get_link_lword(linkage_guard.0, index),
                    right: linkage_get_link_rword(linkage_guard.0, index),
                    label,
                });
            }
            LgParse {
                words,
                links,
                num_linkages,
            }
        }
    }
}

impl Drop for LinkGrammarParser {
    fn drop(&mut self) {
        // SAFETY: both pointers were created by the matching constructors and
        // are released exactly once.
        unsafe {
            if !self.options.is_null() {
                parse_options_delete(self.options);
            }
            if !self.dictionary.is_null() {
                dictionary_delete(self.dictionary);
            }
        }
    }
}
