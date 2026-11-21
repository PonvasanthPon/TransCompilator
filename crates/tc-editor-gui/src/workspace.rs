//! Headless workspace, atomic file persistence, and `std/` reference indexing.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

/// One exported function in the `std/` reference drawer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StdFunction {
    /// Plain-English signature line.
    pub signature: String,
    /// Declarative call phrases.
    pub phrases: Vec<String>,
}

/// One `std/*.eng` module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StdModule {
    /// File name such as `alloc.eng`.
    pub name: String,
    /// Exported functions.
    pub functions: Vec<StdFunction>,
}

/// Native editor workspace rooted at one directory.
#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
    current: Option<PathBuf>,
    dirty: bool,
}

impl Workspace {
    /// Creates or opens a workspace directory.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the directory cannot be created or resolved.
    pub fn new(root: &Path) -> io::Result<Self> {
        fs::create_dir_all(root)?;
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        Ok(Self {
            root,
            current: None,
            dirty: false,
        })
    }

    /// Workspace root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Currently open relative path.
    #[must_use]
    pub fn current(&self) -> Option<&Path> {
        self.current.as_deref()
    }

    /// Absolute path of the open file, when any.
    #[must_use]
    pub fn current_path(&self) -> Option<PathBuf> {
        self.current
            .as_ref()
            .map(|relative| self.root.join(relative))
    }

    /// Whether the buffer has unsaved edits.
    #[must_use]
    pub fn dirty(&self) -> bool {
        self.dirty
    }

    /// Updates the dirty flag.
    pub fn set_dirty(&mut self, dirty: bool) {
        self.dirty = dirty;
    }

    /// Lists `.eng` files in the root and one nested level, sorted.
    #[must_use]
    pub fn files(&self) -> Vec<String> {
        let mut files = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.root) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.is_file() && is_english(&path) {
                    if let Some(name) = path.file_name().and_then(|value| value.to_str()) {
                        files.push(name.to_string());
                    }
                } else if path.is_dir() {
                    if let Ok(nested) = fs::read_dir(&path) {
                        for child in nested.filter_map(Result::ok) {
                            let child_path = child.path();
                            if child_path.is_file() && is_english(&child_path) {
                                if let Ok(relative) = child_path.strip_prefix(&self.root) {
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

    /// Resolves a relative `.eng` path without allowing traversal outside the root.
    ///
    /// # Errors
    ///
    /// Returns an error for absolute paths, parent traversal, or non-`.eng` names.
    pub fn safe_path(&self, raw: &str) -> io::Result<PathBuf> {
        if raw.trim().is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "path is empty"));
        }
        let candidate = Path::new(raw);
        if candidate.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "absolute paths are not allowed",
            ));
        }
        let mut relative = PathBuf::new();
        for component in candidate.components() {
            match component {
                Component::CurDir => {}
                Component::Normal(value) => relative.push(value),
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "path must stay inside the workspace",
                    ));
                }
            }
        }
        if relative.extension().and_then(|value| value.to_str()) != Some("eng") {
            relative.set_extension("eng");
        }
        Ok(self.root.join(relative))
    }

    /// Opens a workspace `.eng` file and makes it current.
    ///
    /// # Errors
    ///
    /// Returns an I/O or path validation error.
    pub fn open(&mut self, raw: &str) -> io::Result<(String, String)> {
        let path = self.safe_path(raw)?;
        let content = fs::read_to_string(&path)?;
        let relative = path.strip_prefix(&self.root).unwrap_or(&path).to_path_buf();
        self.current = Some(relative);
        self.dirty = false;
        Ok((path.to_string_lossy().to_string(), content))
    }

    /// Atomically saves a `.eng` file inside the workspace.
    ///
    /// # Errors
    ///
    /// Returns an I/O or path validation error.
    pub fn save(&mut self, raw: &str, content: &str) -> io::Result<PathBuf> {
        let path = self.safe_path(raw)?;
        let mut temporary = path.as_os_str().to_os_string();
        temporary.push(".tmp");
        let temporary = PathBuf::from(temporary);
        fs::write(&temporary, content.as_bytes())?;
        fs::rename(&temporary, &path)?;
        let relative = path.strip_prefix(&self.root).unwrap_or(&path).to_path_buf();
        self.current = Some(relative);
        self.dirty = false;
        Ok(path)
    }
}

fn is_english(path: &Path) -> bool {
    path.extension().and_then(|value| value.to_str()) == Some("eng")
}

/// Finds the repository `std/` directory.
#[must_use]
pub fn find_std_dir() -> Option<PathBuf> {
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

fn quoted_values(line: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut parts = line.split('"');
    parts.next();
    while let Some(value) = parts.next() {
        values.push(value.to_string());
        parts.next();
    }
    values
}

/// Parses all eight `std/*.eng` modules into a reference index.
#[must_use]
pub fn std_index() -> Vec<StdModule> {
    let Some(directory) = find_std_dir() else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| is_english(path))
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
        let mut current: Option<StdFunction> = None;
        for line in text.lines() {
            let trimmed = line.trim();
            let header = trimmed
                .strip_prefix("Publicly, to ")
                .or_else(|| trimmed.strip_prefix("Public to "))
                .or_else(|| trimmed.strip_prefix("Public function "))
                .or_else(|| trimmed.strip_prefix("function "));
            if let Some(signature) = header {
                if let Some(previous) = current.take() {
                    functions.push(previous);
                }
                current = Some(StdFunction {
                    signature: signature
                        .trim_end_matches('.')
                        .trim_end_matches(':')
                        .to_string(),
                    phrases: Vec::new(),
                });
                continue;
            }
            if let Some(function) = &mut current {
                let lower = trimmed.to_ascii_lowercase();
                if lower.starts_with("phrase:") || lower.contains("call this as") {
                    function.phrases.extend(quoted_values(trimmed));
                }
            }
        }
        if let Some(previous) = current.take() {
            functions.push(previous);
        }
        modules.push(StdModule { name, functions });
    }
    modules
}
