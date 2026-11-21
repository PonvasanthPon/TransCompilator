//! Editor state machine and keybindings.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::buffer::TextBuffer;
use crate::highlight::LineState;
use crate::keys::KeyEvent;
use crate::runner::{
    OutputTab, PRESETS, RunResult, build_elf_artifact, run_elf_with_parity, run_ir, run_paraphrase,
    run_trace,
};
use crate::theme::{Theme, ThemeId};

/// Interactive mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Normal editing.
    Editing,
    /// Inline prompt at the bottom of the screen.
    Prompt,
    /// Preset picker overlay.
    PresetPicker,
    /// Help overlay.
    Help,
    /// Output pane has scroll focus.
    OutputFocus,
}

/// Which inline prompt is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    /// Save as a `.eng` path.
    SaveAs,
    /// Open a `.eng` path.
    Open,
    /// Find text.
    Find,
    /// Set stdin for the next run.
    Stdin,
}

impl PromptKind {
    /// Human prompt label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::SaveAs => "Save as (.eng): ",
            Self::Open => "Open (.eng): ",
            Self::Find => "Find: ",
            Self::Stdin => "Stdin: ",
        }
    }
}

/// Full editor application state.
#[derive(Debug)]
pub struct EditorApp {
    /// Active text buffer.
    pub buffer: TextBuffer,
    /// Current output result.
    pub output: RunResult,
    /// Active output tab.
    pub tab: OutputTab,
    /// Active mode.
    pub mode: Mode,
    /// Prompt kind when in prompt mode.
    pub prompt_kind: Option<PromptKind>,
    /// Prompt input.
    pub prompt: String,
    /// Bottom status message.
    pub status: String,
    /// Output scroll offset.
    pub output_scroll: usize,
    /// Optional stdin text for F5.
    pub stdin_text: String,
    /// First Ctrl+Q on a dirty buffer arms this confirmation.
    pub quit_confirm: bool,
    /// Set when the event loop should exit.
    pub should_quit: bool,
    /// Current terminal width.
    pub width: usize,
    /// Current terminal height.
    pub height: usize,
    /// Active color theme.
    pub theme: Theme,
    /// One-based line diagnostic and prose states.
    pub line_states: HashMap<usize, LineState>,
    /// Selected row in the preset picker.
    pub preset_selection: usize,
    /// Last committed `Ctrl+F` query, used for match highlighting.
    pub search_query: Option<String>,
}

impl Default for EditorApp {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorApp {
    /// Creates an empty editor.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buffer: TextBuffer::new(),
            output: RunResult {
                ok: true,
                summary: "Ready".to_string(),
                lines: vec![
                    "Press F2 to load a .eng preset or start typing English.".to_string(),
                    "Keys: F5/^R Run   F6/^P Paraphrase   F7/^T Trace".to_string(),
                    "F5 runs the native ELF64 and checks interpreter parity.".to_string(),
                ],
                error_lines: Vec::new(),
                omitted_lines: Vec::new(),
            },
            tab: OutputTab::Run,
            mode: Mode::Editing,
            prompt_kind: None,
            prompt: String::new(),
            status: "R1 Editor ready — press F2 for presets or start typing English!".to_string(),
            output_scroll: 0,
            stdin_text: String::new(),
            quit_confirm: false,
            should_quit: false,
            width: 100,
            height: 30,
            theme: Theme::initial(),
            line_states: HashMap::new(),
            preset_selection: 0,
            search_query: None,
        }
    }

    /// Creates an editor with an explicit theme (test-friendly and NO_COLOR-safe).
    #[must_use]
    pub fn with_theme(id: ThemeId) -> Self {
        let mut app = Self::new();
        app.theme = Theme::from_id(id);
        app
    }

    /// Returns the semantic state for a one-based source line.
    #[must_use]
    pub fn line_state(&self, line: usize) -> LineState {
        self.line_states
            .get(&line)
            .copied()
            .unwrap_or(LineState::Normal)
    }

    fn update_line_states(&mut self, result: &RunResult) {
        self.line_states.clear();
        for line in &result.omitted_lines {
            self.line_states.insert(*line, LineState::ProseOmitted);
        }
        for line in &result.error_lines {
            self.line_states.insert(*line, LineState::Error);
        }
    }

    /// Opens an existing file, or targets a new path if it does not exist.
    ///
    /// # Errors
    ///
    /// Returns the I/O error when an existing file cannot be read.
    pub fn open_path(&mut self, path: &Path) -> std::io::Result<()> {
        self.line_states.clear();
        self.search_query = None;
        if path.exists() {
            self.buffer = TextBuffer::load_from_file(path)?;
            self.status = format!("Opened {}", path.display());
        } else {
            self.buffer = TextBuffer::new();
            self.buffer.set_file_path(Some(path.to_path_buf()));
            self.status = format!("New buffer for {}", path.display());
        }
        self.mode = Mode::Editing;
        Ok(())
    }

    /// Loads a built-in preset into the editor.
    pub fn load_preset(&mut self, index: usize) -> bool {
        self.line_states.clear();
        self.preset_selection = index;
        let Some(preset) = PRESETS.get(index) else {
            self.status = format!("No preset {}", index + 1);
            return false;
        };
        self.buffer.replace_text(preset.source);
        self.buffer.set_file_path(None);
        self.buffer.mark_dirty();
        self.status = format!("Loaded preset {}", preset.name);
        self.mode = Mode::Editing;
        true
    }

    /// Saves the buffer to its current path, or opens the Save As prompt.
    pub fn save(&mut self) {
        let Some(path) = self.buffer.file_path().map(Path::to_path_buf) else {
            self.start_prompt(PromptKind::SaveAs);
            return;
        };
        match self.buffer.save_to_file(&path) {
            Ok(target) => self.status = format!("Saved {}", target.display()),
            Err(error) => self.status = format!("Save failed: {error}"),
        }
    }

    /// Saves to an explicit path.
    pub fn save_as(&mut self, path: &Path) {
        match self.buffer.save_to_file(path) {
            Ok(target) => self.status = format!("Saved {}", target.display()),
            Err(error) => self.status = format!("Save failed: {error}"),
        }
    }

    fn start_prompt(&mut self, kind: PromptKind) {
        self.mode = Mode::Prompt;
        self.prompt_kind = Some(kind);
        self.prompt.clear();
        self.status = kind.label().to_string();
    }

    fn commit_prompt(&mut self) {
        let kind = self.prompt_kind;
        let value = self.prompt.trim().to_string();
        self.mode = Mode::Editing;
        self.prompt_kind = None;
        self.prompt.clear();
        match kind {
            Some(PromptKind::SaveAs) => {
                if value.is_empty() {
                    self.status = "Save cancelled".to_string();
                } else {
                    self.save_as(Path::new(&value));
                }
            }
            Some(PromptKind::Open) => {
                if value.is_empty() {
                    self.status = "Open cancelled".to_string();
                } else {
                    match TextBuffer::load_from_file(Path::new(&value)) {
                        Ok(buffer) => {
                            self.buffer = buffer;
                            self.status = format!("Opened {value}");
                        }
                        Err(error) => self.status = format!("Open failed: {error}"),
                    }
                }
            }
            Some(PromptKind::Find) => {
                if value.is_empty() {
                    self.search_query = None;
                    self.status = "Search cleared".to_string();
                } else {
                    self.search_query = Some(value.clone());
                    if self.buffer.find_next(&value) {
                        self.status = format!("Found {value}");
                    } else {
                        self.status = format!("Not found: {value}");
                    }
                }
            }
            Some(PromptKind::Stdin) => {
                self.stdin_text.clone_from(&value);
                self.status = if value.is_empty() {
                    "stdin cleared".to_string()
                } else {
                    format!("stdin set ({} bytes)", value.len())
                };
            }
            None => {}
        }
    }

    /// Runs the active action for an output tab.
    pub fn execute(&mut self, tab: OutputTab) {
        let source = self.buffer.to_text();
        let file_path = self.buffer.file_path().map(Path::to_path_buf);
        let result = match tab {
            OutputTab::Run => run_elf_with_parity(&source, file_path.as_deref(), &self.stdin_text),
            OutputTab::Paraphrase => run_paraphrase(&source, file_path.as_deref()),
            OutputTab::Trace => run_trace(&source, file_path.as_deref()),
            OutputTab::Ir => run_ir(&source, file_path.as_deref()),
            OutputTab::Build => build_elf_artifact(&source, file_path.as_deref()),
        };
        if matches!(
            tab,
            OutputTab::Run | OutputTab::Paraphrase | OutputTab::Trace
        ) {
            self.update_line_states(&result);
        }
        self.status.clone_from(&result.summary);
        self.output = result;
        self.tab = tab;
        self.output_scroll = 0;
    }

    /// Runs F5.
    pub fn run(&mut self) {
        self.execute(OutputTab::Run);
    }

    /// Runs F6.
    pub fn paraphrase(&mut self) {
        self.execute(OutputTab::Paraphrase);
    }

    /// Runs F7.
    pub fn trace(&mut self) {
        self.execute(OutputTab::Trace);
    }

    /// Runs F8.
    pub fn show_ir(&mut self) {
        self.execute(OutputTab::Ir);
    }

    /// Runs F9.
    pub fn build(&mut self) {
        self.execute(OutputTab::Build);
    }

    /// Scrolls output by `delta` lines, saturating at the top.
    pub fn scroll_output(&mut self, delta: isize) {
        if delta.is_negative() {
            self.output_scroll = self.output_scroll.saturating_sub(delta.unsigned_abs());
        } else {
            let maximum = self.output.lines.len().saturating_sub(1);
            self.output_scroll = (self.output_scroll + delta.unsigned_abs()).min(maximum);
        }
    }

    fn request_quit(&mut self) {
        if !self.buffer.is_dirty() {
            self.should_quit = true;
            return;
        }
        if self.quit_confirm {
            self.should_quit = true;
        } else {
            self.quit_confirm = true;
            self.status =
                "Unsaved changes! Press Ctrl+Q again to quit or Esc to cancel.".to_string();
        }
    }

    /// Handles one decoded key event and returns true when a redraw is useful.
    pub fn handle_key(&mut self, event: KeyEvent) -> bool {
        match self.mode {
            Mode::Prompt => {
                match event {
                    KeyEvent::Enter => self.commit_prompt(),
                    KeyEvent::Esc => {
                        self.mode = Mode::Editing;
                        self.prompt_kind = None;
                        self.prompt.clear();
                        self.status = "Prompt cancelled".to_string();
                    }
                    KeyEvent::Backspace => {
                        self.prompt.pop();
                    }
                    KeyEvent::Char(character) => self.prompt.push(character),
                    KeyEvent::Tab => self.prompt.push_str("    "),
                    _ => {}
                }
                return true;
            }
            Mode::PresetPicker => {
                match event {
                    KeyEvent::Up => {
                        self.preset_selection =
                            (self.preset_selection + PRESETS.len() - 1) % PRESETS.len();
                    }
                    KeyEvent::Down => {
                        self.preset_selection = (self.preset_selection + 1) % PRESETS.len();
                    }
                    KeyEvent::Enter => {
                        let _ = self.load_preset(self.preset_selection);
                    }
                    KeyEvent::Char(character) => {
                        if let Some(index) = character.to_digit(10) {
                            if (1..=5).contains(&index) {
                                let _ = self.load_preset(index as usize - 1);
                                return true;
                            }
                        }
                    }
                    KeyEvent::Esc | KeyEvent::F(2) => {
                        self.mode = Mode::Editing;
                        self.status = "Preset picker closed".to_string();
                    }
                    _ => {}
                }
                return true;
            }
            Mode::Help => {
                if matches!(event, KeyEvent::Esc | KeyEvent::F(1)) {
                    self.mode = Mode::Editing;
                }
                return true;
            }
            Mode::Editing | Mode::OutputFocus => {}
        }

        match event {
            KeyEvent::Ctrl('q') | KeyEvent::Ctrl('c') => {
                self.request_quit();
                true
            }
            KeyEvent::Esc => {
                self.quit_confirm = false;
                if self.mode == Mode::OutputFocus {
                    self.mode = Mode::Editing;
                } else {
                    self.status = "Ready".to_string();
                }
                true
            }
            KeyEvent::F(1) => {
                self.mode = Mode::Help;
                true
            }
            KeyEvent::F(2) => {
                self.mode = Mode::PresetPicker;
                self.preset_selection = 0;
                self.status = "Preset picker: ↑/↓ then Enter, or 1-5".to_string();
                true
            }
            KeyEvent::F(4) => {
                self.theme = Theme::from_id(self.theme.id.next());
                self.status = format!("Theme: {}", self.theme.id.name());
                true
            }
            KeyEvent::F(3) => {
                self.start_prompt(PromptKind::Stdin);
                true
            }
            KeyEvent::F(5) | KeyEvent::Ctrl('r') => {
                self.run();
                true
            }
            KeyEvent::F(6) | KeyEvent::Ctrl('p') => {
                self.paraphrase();
                true
            }
            KeyEvent::F(7) | KeyEvent::Ctrl('t') => {
                self.trace();
                true
            }
            KeyEvent::F(8) | KeyEvent::Ctrl('i') => {
                self.show_ir();
                true
            }
            KeyEvent::F(9) | KeyEvent::Ctrl('b') => {
                self.build();
                true
            }
            KeyEvent::Ctrl('s') => {
                self.save();
                true
            }
            KeyEvent::Ctrl('o') => {
                self.start_prompt(PromptKind::Open);
                true
            }
            KeyEvent::Ctrl('n') => {
                if self.buffer.is_dirty() {
                    self.status = "Unsaved changes: save with Ctrl+S before creating a new buffer."
                        .to_string();
                } else {
                    self.buffer = TextBuffer::new();
                    self.mode = Mode::Editing;
                    self.tab = OutputTab::Run;
                    self.line_states.clear();
                    self.search_query = None;
                    self.status = "New .eng buffer".to_string();
                }
                true
            }
            KeyEvent::Ctrl('z') => {
                if self.buffer.undo() {
                    self.line_states.clear();
                    self.status = "Undo".to_string();
                }
                true
            }
            KeyEvent::Ctrl('y') => {
                if self.buffer.redo() {
                    self.line_states.clear();
                    self.status = "Redo".to_string();
                }
                true
            }
            KeyEvent::Ctrl('f') => {
                self.start_prompt(PromptKind::Find);
                true
            }
            KeyEvent::Ctrl('e') => {
                self.mode = if self.mode == Mode::OutputFocus {
                    Mode::Editing
                } else {
                    Mode::OutputFocus
                };
                true
            }
            _ if self.mode == Mode::OutputFocus => match event {
                KeyEvent::PageUp => {
                    self.scroll_output(-10);
                    true
                }
                KeyEvent::PageDown => {
                    self.scroll_output(10);
                    true
                }
                KeyEvent::Up => {
                    self.scroll_output(-1);
                    true
                }
                KeyEvent::Down => {
                    self.scroll_output(1);
                    true
                }
                _ => true,
            },
            _ => {
                let edits_buffer = matches!(
                    event,
                    KeyEvent::Char(_)
                        | KeyEvent::Enter
                        | KeyEvent::Tab
                        | KeyEvent::Backspace
                        | KeyEvent::Delete
                );
                if edits_buffer {
                    let row = self.buffer.cursor_row() + 1;
                    let line_count = self.buffer.lines().len();
                    self.edit_key(event);
                    if self.buffer.lines().len() == line_count {
                        self.line_states.remove(&row);
                        self.line_states.remove(&(self.buffer.cursor_row() + 1));
                    } else {
                        self.line_states.clear();
                    }
                } else {
                    self.edit_key(event);
                }
                true
            }
        }
    }

    fn edit_key(&mut self, event: KeyEvent) {
        match event {
            KeyEvent::Char('\n') | KeyEvent::Enter => self.buffer.insert_newline(),
            KeyEvent::Char(character) => self.buffer.insert_char(character),
            KeyEvent::Tab => self.buffer.insert_tab(),
            KeyEvent::Backspace => self.buffer.backspace(),
            KeyEvent::Delete => self.buffer.delete_char(),
            KeyEvent::Up => self.buffer.move_up(),
            KeyEvent::Down => self.buffer.move_down(),
            KeyEvent::Left => self.buffer.move_left(),
            KeyEvent::Right => self.buffer.move_right(),
            KeyEvent::CtrlLeft => self.buffer.move_word_left(),
            KeyEvent::CtrlRight => self.buffer.move_word_right(),
            KeyEvent::Home => self.buffer.move_home(),
            KeyEvent::End => self.buffer.move_end(),
            KeyEvent::PageUp => self.buffer.page_up(10),
            KeyEvent::PageDown => self.buffer.page_down(10),
            _ => {}
        }
        self.quit_confirm = false;
    }

    /// Renders the current frame as an ANSI string.
    #[must_use]
    pub fn render(&self) -> String {
        crate::ui::render(self, self.width, self.height)
    }

    /// Sets the viewport size.
    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = width.max(20);
        self.height = height.max(6);
    }

    /// Returns the display file name.
    #[must_use]
    pub fn file_label(&self) -> String {
        self.buffer.file_path().map_or_else(
            || "untitled.eng".to_string(),
            |path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("untitled.eng")
                    .to_string()
            },
        )
    }

    /// Creates a path-targeted empty buffer for the CLI.
    #[must_use]
    pub fn with_target(path: PathBuf) -> Self {
        let mut app = Self::new();
        app.buffer.set_file_path(Some(path.clone()));
        app.status = format!("New buffer for {}", path.display());
        app
    }
}
