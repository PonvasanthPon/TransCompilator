//! Native `eframe::App` implementation and headless controller.

use std::path::{Path, PathBuf};

use eframe::egui::{
    self, Align, Color32, Frame, Layout, Margin, Rect, RichText, Stroke, TextEdit, Vec2,
};
use tc_editor::highlight::LineState;
use tc_editor::runner::{
    PRESETS, RunResult, build_elf_artifact, run_elf_with_parity, run_ir, run_paraphrase, run_trace,
};

use crate::gui_highlight::highlight_job;
use crate::gui_theme::{GuiTheme, ThemeMode};
use crate::workspace::{StdModule, Workspace, std_index};

/// Default editor font size in points.
pub const DEFAULT_EDITOR_FONT_SIZE: f32 = 15.0;
/// Smallest permitted editor font size.
pub const MIN_EDITOR_FONT_SIZE: f32 = 12.0;
/// Largest permitted editor font size.
pub const MAX_EDITOR_FONT_SIZE: f32 = 22.0;
/// Sidebar width in logical pixels.
pub const SIDEBAR_WIDTH: f32 = 236.0;
/// Top toolbar height in logical pixels.
pub const TOOLBAR_HEIGHT: f32 = 44.0;
/// Minimum inspector width in logical pixels.
pub const INSPECTOR_MIN_WIDTH: f32 = 320.0;
/// Default inspector fraction of the window width.
pub const INSPECTOR_DEFAULT_FRACTION: f32 = 0.42;
/// Sidebar and toolbar text size.
pub const SIDEBAR_FONT_SIZE: f32 = 13.5;
/// Status-bar and inspector-meta text size.
pub const STATUS_FONT_SIZE: f32 = 12.0;
/// Uppercase section-label text size.
pub const SECTION_FONT_SIZE: f32 = 11.0;

/// Returns the clamped default inspector width for a window width.
#[must_use]
pub fn inspector_default_width(screen_width: f32) -> f32 {
    (screen_width * INSPECTOR_DEFAULT_FRACTION).max(INSPECTOR_MIN_WIDTH)
}

/// The three top-toolbar groups, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolbarSpec {
    /// Identity and file actions.
    pub left: &'static [&'static str],
    /// Primary run and build actions.
    pub center: &'static [&'static str],
    /// View and tool actions.
    pub right: &'static [&'static str],
}

/// Returns the top-toolbar contract.
#[must_use]
pub const fn toolbar_spec() -> ToolbarSpec {
    ToolbarSpec {
        left: &["R1 Editor", "New", "Open", "Save"],
        center: &["> Run", "Build .elf"],
        right: &["Presets v", "Wrap", "std/ Docs", "Light / Dark"],
    }
}

/// Colors for one sidebar pill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PillStyle {
    /// Pill fill.
    pub fill: Color32,
    /// Label color.
    pub text: Color32,
    /// Whether a left accent bar is drawn.
    pub accent_bar: bool,
}

/// Returns the sidebar pill colors for a selection state.
#[must_use]
pub fn sidebar_pill_style(theme: &GuiTheme, selected: bool) -> PillStyle {
    if selected {
        PillStyle {
            fill: theme.selected_bg,
            text: theme.accent,
            accent_bar: true,
        }
    } else {
        PillStyle {
            fill: Color32::TRANSPARENT,
            text: theme.ink,
            accent_bar: false,
        }
    }
}

/// Output inspector tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputTab {
    /// Native run and parity.
    #[default]
    Run,
    /// Resolved English paraphrase.
    Paraphrase,
    /// Lexicon and intent trace.
    Trace,
    /// Formatted Core-7 IR.
    Ir,
    /// Static ELF64 build.
    Build,
}

impl OutputTab {
    /// All inspector tabs, in display order.
    #[must_use]
    pub const fn all() -> [Self; 5] {
        [
            Self::Run,
            Self::Paraphrase,
            Self::Trace,
            Self::Ir,
            Self::Build,
        ]
    }

    /// Tab label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Run => "Run & Parity",
            Self::Paraphrase => "Paraphrase",
            Self::Trace => "Trace",
            Self::Ir => "Core-7 IR",
            Self::Build => "Build",
        }
    }
}

/// Rendering kind for one inspector card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardKind {
    /// Green pass status card.
    StatusPass,
    /// Red error status card.
    StatusError,
    /// White section card.
    Section,
    /// Neutral first-run empty state.
    Empty,
}

/// One inspector output card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiCard {
    /// Card heading.
    pub title: String,
    /// Selectable monospace body.
    pub body: String,
    /// Status or section styling.
    pub kind: CardKind,
}

/// Inline badge category used by trace and paraphrase output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineBadge {
    /// Resolved anaphor marker such as `[it -> cash]`.
    Anaphor,
    /// Omitted prose marker.
    Omitted,
    /// Executed lexicon marker.
    Exec,
    /// Unsupported construct marker.
    Unsupported,
}

/// Classifies one output line for an inline badge.
#[must_use]
pub fn line_badge(line: &str) -> Option<LineBadge> {
    if line.contains("[it ->") || line.contains("[its ->") {
        Some(LineBadge::Anaphor)
    } else if line.contains("COMMENT_OMITTED") {
        Some(LineBadge::Omitted)
    } else if line.contains("UNSUPPORTED") {
        Some(LineBadge::Unsupported)
    } else if line.contains("EXEC") {
        Some(LineBadge::Exec)
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy)]
struct ViewOptions {
    word_wrap: bool,
    std_ref: bool,
}

/// Native R1 Editor application state.
#[derive(Debug)]
pub struct R1GuiApp {
    workspace: Workspace,
    editor: String,
    stdin: String,
    theme: GuiTheme,
    theme_dirty: bool,
    tab: OutputTab,
    result: Option<RunResult>,
    status: String,
    error_lines: Vec<usize>,
    omitted_lines: Vec<usize>,
    view: ViewOptions,
    open_path: String,
    open_dialog: bool,
    new_name: String,
    editor_font_size: f32,
}

impl R1GuiApp {
    /// Builds the native app and applies the default light theme.
    #[must_use]
    pub fn new(context: &eframe::CreationContext<'_>, file: Option<&Path>) -> Self {
        let workspace_dir = file.and_then(Path::parent).map_or_else(
            || std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            Path::to_path_buf,
        );
        let mut app = Self::headless(&workspace_dir);
        if let Some(file) = file {
            let name = file
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("untitled.eng");
            let _ = app.open_file(name);
        }
        app.theme.apply(&context.egui_ctx);
        app
    }

    /// Builds a headless controller for tests and automation.
    #[must_use]
    pub fn headless(workspace_dir: &Path) -> Self {
        let workspace = Workspace::new(workspace_dir).unwrap_or_else(|_| {
            Workspace::new(Path::new(".")).unwrap_or_else(|error| panic!("workspace: {error}"))
        });
        Self {
            workspace,
            editor: String::new(),
            stdin: String::new(),
            theme: GuiTheme::light(),
            theme_dirty: true,
            tab: OutputTab::Run,
            result: None,
            status: "Ready".to_string(),
            error_lines: Vec::new(),
            omitted_lines: Vec::new(),
            view: ViewOptions {
                word_wrap: true,
                std_ref: false,
            },
            open_path: String::new(),
            open_dialog: false,
            new_name: "untitled.eng".to_string(),
            editor_font_size: DEFAULT_EDITOR_FONT_SIZE,
        }
    }

    /// Active workspace.
    #[must_use]
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// Mutable workspace.
    #[must_use]
    pub fn workspace_mut(&mut self) -> &mut Workspace {
        &mut self.workspace
    }

    /// Current source text.
    #[must_use]
    pub fn editor_text(&self) -> &str {
        &self.editor
    }

    /// Replaces the source text and marks the buffer dirty.
    pub fn set_editor_text(&mut self, text: impl Into<String>) {
        self.editor = text.into();
        self.workspace.set_dirty(true);
        self.error_lines.clear();
    }

    /// Program stdin.
    #[must_use]
    pub fn stdin_text(&self) -> &str {
        &self.stdin
    }

    /// Sets program stdin.
    pub fn set_stdin_text(&mut self, text: impl Into<String>) {
        self.stdin = text.into();
    }

    /// Active palette.
    #[must_use]
    pub fn theme(&self) -> &GuiTheme {
        &self.theme
    }

    /// Active theme mode.
    #[must_use]
    pub fn theme_mode(&self) -> ThemeMode {
        self.theme.mode
    }

    /// Switches the palette.
    pub fn set_theme_mode(&mut self, mode: ThemeMode) {
        self.theme = GuiTheme::from_mode(mode);
        self.theme_dirty = true;
    }

    /// Toggles Light and Dark.
    pub fn toggle_theme(&mut self) {
        self.set_theme_mode(self.theme.mode.toggled());
    }

    /// Current editor font size in points.
    #[must_use]
    pub fn editor_font_size(&self) -> f32 {
        self.editor_font_size
    }

    /// Sets and clamps the editor font size.
    pub fn set_editor_font_size(&mut self, size: f32) {
        self.editor_font_size = size.clamp(MIN_EDITOR_FONT_SIZE, MAX_EDITOR_FONT_SIZE);
    }

    /// Increases the editor font size by one point.
    pub fn zoom_in(&mut self) {
        self.set_editor_font_size(self.editor_font_size + 1.0);
    }

    /// Decreases the editor font size by one point.
    pub fn zoom_out(&mut self) {
        self.set_editor_font_size(self.editor_font_size - 1.0);
    }

    /// Resets the editor font size to the default.
    pub fn reset_zoom(&mut self) {
        self.set_editor_font_size(DEFAULT_EDITOR_FONT_SIZE);
    }

    /// Whether soft word-wrap is enabled.
    #[must_use]
    pub fn word_wrap(&self) -> bool {
        self.view.word_wrap
    }

    /// Enables or disables soft word-wrap.
    pub fn set_word_wrap(&mut self, enabled: bool) {
        self.view.word_wrap = enabled;
    }

    /// Toggles soft word-wrap.
    pub fn toggle_word_wrap(&mut self) {
        self.view.word_wrap = !self.view.word_wrap;
    }

    /// Active output tab.
    #[must_use]
    pub fn tab(&self) -> OutputTab {
        self.tab
    }

    /// Selects an output tab without computing it.
    pub fn set_tab(&mut self, tab: OutputTab) {
        self.tab = tab;
    }

    /// Selects an inspector tab, computing prose and IR views immediately.
    pub fn select_tab(&mut self, tab: OutputTab) {
        match tab {
            OutputTab::Run | OutputTab::Build => self.tab = tab,
            OutputTab::Paraphrase => self.paraphrase(),
            OutputTab::Trace => self.trace(),
            OutputTab::Ir => self.ir(),
        }
    }

    /// Latest result.
    #[must_use]
    pub fn result(&self) -> Option<&RunResult> {
        self.result.as_ref()
    }

    /// Latest status line.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Error line numbers from the latest compile.
    #[must_use]
    pub fn error_lines(&self) -> &[usize] {
        &self.error_lines
    }

    /// Omitted prose line numbers from the latest run.
    #[must_use]
    pub fn omitted_lines(&self) -> &[usize] {
        &self.omitted_lines
    }

    /// Whether the `std/` reference is open.
    #[must_use]
    pub fn std_ref_open(&self) -> bool {
        self.view.std_ref
    }

    /// Toggles the `std/` reference.
    pub fn set_std_ref_open(&mut self, open: bool) {
        self.view.std_ref = open;
    }

    /// Loads one of the eight starter presets.
    pub fn load_preset(&mut self, index: usize) -> bool {
        let Some(preset) = PRESETS.get(index) else {
            return false;
        };
        self.editor = preset.source.to_string();
        self.workspace.set_dirty(true);
        self.error_lines.clear();
        self.omitted_lines.clear();
        self.result = None;
        self.status = format!("Loaded {}", preset.name);
        true
    }

    /// Opens a workspace file.
    ///
    /// # Errors
    ///
    /// Returns a path-validation or I/O error.
    pub fn open_file(&mut self, raw: &str) -> std::io::Result<()> {
        let (_, content) = self.workspace.open(raw)?;
        self.editor = content;
        self.error_lines.clear();
        self.omitted_lines.clear();
        self.result = None;
        self.status = format!("Opened {raw}");
        Ok(())
    }

    /// Saves the current buffer.
    ///
    /// # Errors
    ///
    /// Returns a path-validation or I/O error.
    pub fn save_current(&mut self) -> std::io::Result<PathBuf> {
        let name = self
            .workspace
            .current()
            .and_then(|path| path.file_name())
            .and_then(|value| value.to_str())
            .map_or_else(|| self.new_name.clone(), ToString::to_string);
        let path = self.workspace.save(&name, &self.editor)?;
        self.status = format!("Saved {}", path.display());
        Ok(path)
    }

    /// Clears the buffer for a new file.
    pub fn new_buffer(&mut self) {
        self.editor.clear();
        self.result = None;
        self.error_lines.clear();
        self.omitted_lines.clear();
        self.new_name = "untitled.eng".to_string();
        self.workspace.set_dirty(false);
        self.status = "New .eng buffer".to_string();
    }

    /// Runs the native ELF64 and reference-interpreter parity check.
    pub fn run(&mut self) {
        let path = self.workspace.current_path();
        let result = run_elf_with_parity(&self.editor, path.as_deref(), &self.stdin);
        self.finish(result, OutputTab::Run);
    }

    /// Produces the resolved English paraphrase.
    pub fn paraphrase(&mut self) {
        let path = self.workspace.current_path();
        let result = run_paraphrase(&self.editor, path.as_deref());
        self.finish(result, OutputTab::Paraphrase);
    }

    /// Produces the lexicon and intent trace.
    pub fn trace(&mut self) {
        let path = self.workspace.current_path();
        let result = run_trace(&self.editor, path.as_deref());
        self.finish(result, OutputTab::Trace);
    }

    /// Produces formatted Core-7 IR.
    pub fn ir(&mut self) {
        let path = self.workspace.current_path();
        let result = run_ir(&self.editor, path.as_deref());
        self.finish(result, OutputTab::Ir);
    }

    /// Builds a standalone static ELF64 artifact.
    pub fn build(&mut self) {
        let path = self.workspace.current_path();
        let result = build_elf_artifact(&self.editor, path.as_deref());
        self.finish(result, OutputTab::Build);
    }

    fn finish(&mut self, result: RunResult, tab: OutputTab) {
        self.error_lines.clone_from(&result.error_lines);
        self.omitted_lines.clone_from(&result.omitted_lines);
        self.status.clone_from(&result.summary);
        self.tab = tab;
        self.result = Some(result);
    }

    /// Returns one line state per editor line.
    #[must_use]
    pub fn line_states(&self) -> Vec<LineState> {
        let count = self.editor.lines().count().max(1);
        (1..=count)
            .map(|line| {
                crate::gui_highlight::line_state(line, &self.error_lines, &self.omitted_lines)
            })
            .collect()
    }

    /// Returns the gutter with one row per logical line.
    #[must_use]
    pub fn gutter_text(&self) -> String {
        self.gutter_text_with_rows(&[])
    }

    /// Returns the gutter with wrapped-row counts for each logical line.
    #[must_use]
    pub fn gutter_text_with_rows(&self, rows_per_line: &[usize]) -> String {
        let states = self.line_states();
        let mut lines = Vec::new();
        for (index, state) in states.iter().enumerate() {
            let marker = match state {
                LineState::Error => '!',
                LineState::ProseOmitted => '~',
                LineState::Normal => ' ',
            };
            let rows = rows_per_line.get(index).copied().unwrap_or(1).max(1);
            lines.push(format!("{:>3} {marker}", index + 1));
            for _ in 1..rows {
                lines.push("    ".to_string());
            }
        }
        lines.join("\n")
    }

    /// Returns the latest output joined for display and tests.
    #[must_use]
    pub fn output_text(&self) -> String {
        self.result
            .as_ref()
            .map_or_else(String::new, |result| result.lines.join("\n"))
    }

    /// Returns the pass/error status card text.
    #[must_use]
    pub fn status_card_text(&self) -> String {
        self.result.as_ref().map_or_else(
            || "Ready  -  Ctrl+Enter to run".to_string(),
            |result| {
                if result.ok {
                    format!("[OK] PASS  -  {}", result.summary)
                } else {
                    format!("[!] ERROR  -  {}", result.summary)
                }
            },
        )
    }

    /// Returns the inspector cards for the active result.
    #[must_use]
    pub fn output_cards(&self) -> Vec<GuiCard> {
        let Some(result) = &self.result else {
            return vec![GuiCard {
                title: "Ready".to_string(),
                body: concat!(
                    "Ctrl+Enter  Run & Parity\n",
                    "Ctrl+S  Save\n",
                    "F6  Paraphrase\n",
                    "F7  Trace\n",
                    "F8  Core-7 IR\n",
                    "Ctrl+Plus / Ctrl+Minus  Zoom\n",
                    "Click any tab above to inspect Paraphrase, Trace, or Core-7 IR",
                )
                .to_string(),
                kind: CardKind::Empty,
            }];
        };
        let status_kind = if result.ok {
            CardKind::StatusPass
        } else {
            CardKind::StatusError
        };
        let mut cards = vec![GuiCard {
            title: self.tab.label().to_string(),
            body: self.status_card_text(),
            kind: status_kind,
        }];
        if self.tab != OutputTab::Run {
            cards.push(GuiCard {
                title: self.tab.label().to_ascii_uppercase(),
                body: result.lines.join("\n"),
                kind: CardKind::Section,
            });
            return cards;
        }
        let mut current_title: Option<String> = None;
        let mut current_body = Vec::new();
        for line in &result.lines {
            if let Some(title) = section_title(line) {
                if current_title.is_some() || !current_body.is_empty() {
                    cards.push(GuiCard {
                        title: current_title
                            .take()
                            .unwrap_or_else(|| "RUN SUMMARY".to_string()),
                        body: current_body.join("\n"),
                        kind: CardKind::Section,
                    });
                    current_body.clear();
                }
                current_title = Some(title);
            } else {
                current_body.push(line.clone());
            }
        }
        if current_title.is_some() || !current_body.is_empty() {
            cards.push(GuiCard {
                title: current_title.unwrap_or_else(|| "RUN SUMMARY".to_string()),
                body: current_body.join("\n"),
                kind: CardKind::Section,
            });
        }
        cards
    }

    /// Returns the eight-module `std/` reference index.
    #[must_use]
    pub fn std_reference(&self) -> Vec<StdModule> {
        std_index()
    }

    fn handle_shortcuts(&mut self, context: &egui::Context) {
        let command = context.input(|input| input.modifiers.command);
        if command {
            if context.input(|input| {
                input.key_pressed(egui::Key::Plus) || input.key_pressed(egui::Key::Equals)
            }) {
                self.zoom_in();
            }
            if context.input(|input| input.key_pressed(egui::Key::Minus)) {
                self.zoom_out();
            }
            if context.input(|input| input.key_pressed(egui::Key::Num0)) {
                self.reset_zoom();
            }
            if context.input(|input| input.key_pressed(egui::Key::S)) {
                if let Err(error) = self.save_current() {
                    self.status = format!("Save failed: {error}");
                }
            }
            if context.input(|input| input.key_pressed(egui::Key::B)) {
                self.build();
            }
        }
        let run = context.input(|input| {
            (input.modifiers.command && input.key_pressed(egui::Key::Enter))
                || input.key_pressed(egui::Key::F5)
        });
        if run {
            self.run();
        }
        if context.input(|input| input.key_pressed(egui::Key::F6)) {
            self.paraphrase();
        }
        if context.input(|input| input.key_pressed(egui::Key::F7)) {
            self.trace();
        }
        if context.input(|input| input.key_pressed(egui::Key::F8)) {
            self.ir();
        }
        if context.input(|input| input.key_pressed(egui::Key::F9)) {
            self.build();
        }
    }

    fn toolbar(&mut self, context: &egui::Context) {
        let frame = Frame::new()
            .fill(self.theme.toolbar_bg)
            .inner_margin(Margin::symmetric(14, 8))
            .stroke(Stroke::new(1.0_f32, self.theme.border));
        let theme = self.theme.clone();
        egui::TopBottomPanel::top("r1_toolbar")
            .exact_height(TOOLBAR_HEIGHT)
            .frame(frame)
            .show(context, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new("R1 Editor")
                            .strong()
                            .size(15.5)
                            .color(theme.ink),
                    );
                    let file_name = self
                        .workspace
                        .current()
                        .and_then(|path| path.file_name())
                        .and_then(|value| value.to_str())
                        .unwrap_or("untitled.eng");
                    Frame::new()
                        .fill(theme.card_bg)
                        .corner_radius(6)
                        .inner_margin(Margin::symmetric(8, 3))
                        .stroke(Stroke::new(1.0_f32, theme.border))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(file_name)
                                        .size(SIDEBAR_FONT_SIZE)
                                        .color(theme.muted),
                                );
                                if self.workspace.dirty() {
                                    ui.label(RichText::new("*").size(11.0).color(theme.warning_fg));
                                }
                            });
                        });
                    if flat_button(ui, &theme, "New").clicked() {
                        self.new_buffer();
                    }
                    if flat_button(ui, &theme, "Open").clicked() {
                        self.open_dialog = true;
                    }
                    if flat_button(ui, &theme, "Save").clicked() {
                        if let Err(error) = self.save_current() {
                            self.status = format!("Save failed: {error}");
                        }
                    }
                    toolbar_divider(ui, &theme);
                    if primary_button(ui, &theme, "> Run").clicked() {
                        self.run();
                    }
                    if secondary_button(ui, &theme, "Build .elf").clicked() {
                        self.build();
                    }
                    toolbar_divider(ui, &theme);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let theme_label = match theme.mode {
                            ThemeMode::Light => "Light / Dark",
                            ThemeMode::Dark => "Dark / Light",
                        };
                        if toggle_button(ui, &theme, theme_label, false).clicked() {
                            self.toggle_theme();
                        }
                        if toggle_button(ui, &theme, "std/ Docs", self.view.std_ref).clicked() {
                            self.view.std_ref = !self.view.std_ref;
                        }
                        if toggle_button(ui, &theme, "Wrap", self.view.word_wrap).clicked() {
                            self.toggle_word_wrap();
                        }
                        ui.menu_button(
                            RichText::new("Presets v")
                                .size(SIDEBAR_FONT_SIZE)
                                .color(theme.ink),
                            |ui| {
                                for (index, preset) in PRESETS.iter().enumerate() {
                                    let label = preset.name.trim_end_matches(".eng");
                                    if ui.button(label).clicked() {
                                        self.load_preset(index);
                                        ui.close_menu();
                                    }
                                }
                            },
                        );
                    });
                });
            });
    }

    fn sidebar(&mut self, context: &egui::Context) {
        let frame = Frame::new()
            .fill(self.theme.window_bg)
            .inner_margin(Margin::same(12))
            .stroke(Stroke::new(1.0_f32, self.theme.border));
        let theme = self.theme.clone();
        egui::SidePanel::left("r1_sidebar")
            .resizable(false)
            .exact_width(SIDEBAR_WIDTH)
            .frame(frame)
            .show(context, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        section_header(ui, &theme, "WORKSPACE FILES");
                        let new_button = egui::Button::new(
                            RichText::new("+ New .eng")
                                .size(12.5)
                                .strong()
                                .color(theme.accent),
                        )
                        .fill(theme.card_bg)
                        .corner_radius(6)
                        .stroke(Stroke::new(1.0_f32, theme.border))
                        .min_size(Vec2::new(ui.available_width(), 28.0));
                        if ui.add(new_button).clicked() {
                            self.new_buffer();
                        }
                        ui.add_space(4.0);
                        let files = self.workspace.files();
                        if files.is_empty() {
                            ui.label(
                                RichText::new("No .eng files")
                                    .italics()
                                    .size(SIDEBAR_FONT_SIZE)
                                    .color(theme.muted),
                            );
                        }
                        for file in files {
                            let selected = self
                                .workspace
                                .current()
                                .is_some_and(|path| path.to_string_lossy() == file);
                            if sidebar_pill(ui, &theme, &file, selected).clicked() {
                                if let Err(error) = self.open_file(&file) {
                                    self.status = format!("Open failed: {error}");
                                }
                            }
                        }
                        ui.add_space(12.0);
                        section_header(ui, &theme, "STARTER PRESETS");
                        for (index, preset) in PRESETS.iter().enumerate() {
                            let label = preset.name.trim_end_matches(".eng");
                            let response = sidebar_pill(ui, &theme, label, false);
                            if response.on_hover_text(preset.description).clicked() {
                                self.load_preset(index);
                            }
                        }
                    });
            });
    }

    fn output_panel(&mut self, context: &egui::Context) {
        let frame = Frame::new()
            .fill(self.theme.window_bg)
            .inner_margin(Margin::same(14))
            .stroke(Stroke::new(1.0_f32, self.theme.border));
        let default_width = inspector_default_width(context.screen_rect().width());
        let theme = self.theme.clone();
        egui::SidePanel::right("r1_output")
            .resizable(true)
            .default_width(default_width)
            .min_width(INSPECTOR_MIN_WIDTH)
            .frame(frame)
            .show(context, |ui| {
                segmented_tabs(self, ui, &theme);
                ui.add_space(10.0);
                let (card_fill, card_stroke, card_ink) =
                    if self.result.as_ref().is_some_and(|result| result.ok) {
                        (theme.success_bg, theme.success_border, theme.success_fg)
                    } else if self.result.is_some() {
                        (theme.error_bg, theme.error_border, theme.error_fg)
                    } else {
                        (theme.toolbar_bg, theme.border, theme.muted)
                    };
                Frame::new()
                    .fill(card_fill)
                    .corner_radius(8)
                    .stroke(Stroke::new(1.0_f32, card_stroke))
                    .inner_margin(Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            RichText::new(self.status_card_text())
                                .color(card_ink)
                                .strong()
                                .size(13.0),
                        );
                    });
                ui.add_space(8.0);
                let cards = self.output_cards();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for card in &cards {
                            Frame::new()
                                .fill(theme.card_bg)
                                .stroke(Stroke::new(1.0_f32, theme.border))
                                .corner_radius(8)
                                .inner_margin(Margin::symmetric(12, 10))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(
                                        RichText::new(&card.title)
                                            .size(SECTION_FONT_SIZE)
                                            .strong()
                                            .color(theme.section),
                                    );
                                    ui.add_space(4.0);
                                    render_card_body(ui, &theme, &card.body);
                                });
                            ui.add_space(8.0);
                        }
                    });
                if self.view.std_ref {
                    ui.add_space(6.0);
                    egui::CollapsingHeader::new(
                        RichText::new("std/ Reference")
                            .size(SIDEBAR_FONT_SIZE)
                            .strong()
                            .color(theme.ink),
                    )
                    .default_open(true)
                    .show(ui, |ui| {
                        for module in self.std_reference() {
                            ui.label(
                                RichText::new(&module.name)
                                    .strong()
                                    .color(theme.syntax_type),
                            );
                            for function in module.functions {
                                ui.label(
                                    RichText::new(format!("  {}", function.signature))
                                        .monospace()
                                        .size(11.0)
                                        .color(theme.ink),
                                );
                                for phrase in function.phrases {
                                    ui.label(
                                        RichText::new(format!("    \"{phrase}\""))
                                            .italics()
                                            .size(11.0)
                                            .color(theme.syntax_anaphor),
                                    );
                                }
                            }
                        }
                    });
                }
            });
    }

    fn editor_panel(&mut self, context: &egui::Context) {
        let frame = Frame::new()
            .fill(self.theme.editor_bg)
            .inner_margin(Margin::ZERO);
        egui::CentralPanel::default()
            .frame(frame)
            .show(context, |ui| {
                let stdin_frame = Frame::new()
                    .fill(self.theme.toolbar_bg)
                    .inner_margin(Margin::symmetric(8, 5));
                egui::TopBottomPanel::bottom("r1_stdin")
                    .frame(stdin_frame)
                    .show_inside(ui, |ui| {
                        egui::CollapsingHeader::new("Program input (stdin)")
                            .default_open(false)
                            .show(ui, |ui| {
                                ui.add(
                                    TextEdit::singleline(&mut self.stdin)
                                        .hint_text("optional program input")
                                        .desired_width(f32::INFINITY),
                                );
                            });
                    });
                let theme = self.theme.clone();
                let font_size = self.editor_font_size;
                let word_wrap = self.view.word_wrap;
                let error_lines = self.error_lines.clone();
                let omitted_lines = self.omitted_lines.clone();
                let scroll = if word_wrap {
                    egui::ScrollArea::vertical()
                } else {
                    egui::ScrollArea::both()
                };
                scroll.auto_shrink([false, false]).show(ui, |ui| {
                    let gutter_width = 64.0;
                    let editor_width = (ui.available_width() - gutter_width - 12.0).max(160.0);
                    let rows = if word_wrap {
                        self.editor
                            .split_inclusive('\n')
                            .map(|raw| {
                                let line = raw.strip_suffix('\n').unwrap_or(raw);
                                let mut job = highlight_job(
                                    line,
                                    &error_lines,
                                    &omitted_lines,
                                    &theme,
                                    font_size,
                                );
                                job.wrap.max_width = editor_width;
                                ui.fonts(|fonts| fonts.layout_job(job)).rows.len().max(1)
                            })
                            .collect::<Vec<_>>()
                    } else {
                        Vec::new()
                    };
                    let gutter = self.gutter_text_with_rows(&rows);
                    ui.horizontal_top(|ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(gutter)
                                    .monospace()
                                    .size(font_size)
                                    .color(self.theme.gutter),
                            )
                            .selectable(false),
                        );
                        ui.separator();
                        let mut layouter = move |ui: &egui::Ui, text: &str, wrap_width: f32| {
                            let mut job = highlight_job(
                                text,
                                &error_lines,
                                &omitted_lines,
                                &theme,
                                font_size,
                            );
                            if word_wrap {
                                job.wrap.max_width = if wrap_width.is_finite() && wrap_width > 0.0 {
                                    wrap_width
                                } else {
                                    editor_width
                                };
                            } else {
                                job.wrap.max_width = f32::INFINITY;
                            }
                            ui.fonts(|fonts| fonts.layout_job(job))
                        };
                        let response = ui.add(
                            TextEdit::multiline(&mut self.editor)
                                .code_editor()
                                .desired_width(editor_width)
                                .desired_rows(22)
                                .frame(false)
                                .layouter(&mut layouter),
                        );
                        if response.changed() {
                            self.workspace.set_dirty(true);
                            self.error_lines.clear();
                        }
                    });
                });
            });
    }

    fn status_bar(&mut self, context: &egui::Context) {
        let frame = Frame::new()
            .fill(self.theme.toolbar_bg)
            .inner_margin(Margin::symmetric(14, 5))
            .stroke(Stroke::new(1.0_f32, self.theme.border));
        egui::TopBottomPanel::bottom("r1_status")
            .frame(frame)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&self.status)
                            .size(STATUS_FONT_SIZE)
                            .color(self.theme.muted),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let lines = self.editor.lines().count().max(1);
                        ui.label(
                            RichText::new("Static Zero-Libc x86_64")
                                .size(STATUS_FONT_SIZE)
                                .color(self.theme.muted),
                        );
                        ui.label(
                            RichText::new(format!("{:.0} pt", self.editor_font_size))
                                .size(STATUS_FONT_SIZE)
                                .color(self.theme.accent),
                        );
                        ui.label(
                            RichText::new(if self.view.word_wrap {
                                "Wrap On"
                            } else {
                                "Wrap Off"
                            })
                            .size(STATUS_FONT_SIZE)
                            .color(self.theme.muted),
                        );
                        ui.label(
                            RichText::new(format!("{lines} lines"))
                                .size(STATUS_FONT_SIZE)
                                .color(self.theme.muted),
                        );
                    });
                });
            });
    }

    fn open_dialog(&mut self, context: &egui::Context) {
        if !self.open_dialog {
            return;
        }
        let mut open = self.open_dialog;
        egui::Window::new("Open .eng file")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.add(
                    TextEdit::singleline(&mut self.open_path)
                        .hint_text("budget.eng")
                        .desired_width(280.0),
                );
                if ui.button("Open").clicked() {
                    let path = self.open_path.clone();
                    if let Err(error) = self.open_file(&path) {
                        self.status = format!("Open failed: {error}");
                    } else {
                        self.open_dialog = false;
                    }
                }
            });
        if !open {
            self.open_dialog = false;
        }
    }
}

impl eframe::App for R1GuiApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        if self.theme_dirty {
            self.theme.apply(context);
            self.theme_dirty = false;
        }
        self.handle_shortcuts(context);
        self.toolbar(context);
        self.status_bar(context);
        self.sidebar(context);
        self.output_panel(context);
        self.editor_panel(context);
        self.open_dialog(context);
    }
}

fn section_title(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let inner = trimmed.strip_prefix("--- ")?.strip_suffix(" ---")?;
    Some(inner.to_ascii_uppercase())
}

fn toolbar_divider(ui: &mut egui::Ui, theme: &GuiTheme) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, 24.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, theme.border);
}

fn flat_button(ui: &mut egui::Ui, theme: &GuiTheme, label: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(label).size(12.5).color(theme.muted)).frame(false))
}

fn primary_button(ui: &mut egui::Ui, theme: &GuiTheme, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            RichText::new(label)
                .color(Color32::WHITE)
                .strong()
                .size(13.5),
        )
        .fill(theme.accent)
        .corner_radius(6)
        .min_size(Vec2::new(88.0, 28.0)),
    )
}

fn secondary_button(ui: &mut egui::Ui, theme: &GuiTheme, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(label).size(12.5).color(theme.ink))
            .fill(theme.card_bg)
            .corner_radius(6)
            .stroke(Stroke::new(1.0_f32, theme.border))
            .min_size(Vec2::new(0.0, 28.0)),
    )
}

fn toggle_button(ui: &mut egui::Ui, theme: &GuiTheme, label: &str, active: bool) -> egui::Response {
    let fill = if active {
        theme.selected_bg
    } else {
        theme.toolbar_bg
    };
    let color = if active { theme.accent } else { theme.muted };
    ui.add(
        egui::Button::new(RichText::new(label).size(12.0).strong().color(color))
            .fill(fill)
            .corner_radius(6)
            .stroke(Stroke::new(
                1.0_f32,
                if active { theme.accent } else { theme.border },
            ))
            .min_size(Vec2::new(0.0, 26.0)),
    )
}

fn section_header(ui: &mut egui::Ui, theme: &GuiTheme, label: &str) {
    ui.label(
        RichText::new(label)
            .size(SECTION_FONT_SIZE)
            .strong()
            .color(theme.section),
    );
    ui.add_space(4.0);
}

fn sidebar_pill(
    ui: &mut egui::Ui,
    theme: &GuiTheme,
    label: &str,
    selected: bool,
) -> egui::Response {
    let style = sidebar_pill_style(theme, selected);
    let response = ui.add(
        egui::Button::new(
            RichText::new(label)
                .size(SIDEBAR_FONT_SIZE)
                .color(style.text)
                .strong(),
        )
        .fill(style.fill)
        .corner_radius(6)
        .min_size(Vec2::new(ui.available_width(), 28.0)),
    );
    if style.accent_bar {
        let bar = Rect::from_min_size(
            response.rect.left_top(),
            Vec2::new(3.0, response.rect.height()),
        );
        ui.painter().rect_filled(bar, 1.0, theme.accent);
    }
    response
}

fn segmented_tabs(app: &mut R1GuiApp, ui: &mut egui::Ui, theme: &GuiTheme) {
    Frame::new()
        .fill(theme.border)
        .corner_radius(8)
        .inner_margin(Margin::same(3))
        .show(ui, |ui| {
            ui.spacing_mut().button_padding = Vec2::new(5.0, 3.0);
            ui.horizontal(|ui| {
                for tab in OutputTab::all() {
                    let active = app.tab == tab;
                    let button = egui::Button::new(
                        RichText::new(tab.label())
                            .size(12.0)
                            .strong()
                            .color(if active { theme.ink } else { theme.muted }),
                    )
                    .fill(if active {
                        theme.card_bg
                    } else {
                        Color32::TRANSPARENT
                    })
                    .corner_radius(6)
                    .min_size(Vec2::new(0.0, 26.0));
                    if ui.add(button).clicked() {
                        app.select_tab(tab);
                    }
                }
            });
        });
}

fn render_card_body(ui: &mut egui::Ui, theme: &GuiTheme, body: &str) {
    if body.is_empty() {
        ui.label(
            RichText::new("(empty)")
                .italics()
                .size(STATUS_FONT_SIZE)
                .color(theme.muted),
        );
        return;
    }
    for line in body.lines() {
        if let Some(badge) = line_badge(line) {
            ui.horizontal(|ui| {
                badge_pill(ui, theme, badge);
                ui.add(
                    egui::Label::new(RichText::new(line).monospace().size(12.5).color(theme.ink))
                        .selectable(true),
                );
            });
        } else {
            ui.add(
                egui::Label::new(RichText::new(line).monospace().size(12.5).color(theme.ink))
                    .selectable(true),
            );
        }
    }
}

fn badge_pill(ui: &mut egui::Ui, theme: &GuiTheme, badge: LineBadge) {
    let (label, fg, bg) = match badge {
        LineBadge::Anaphor => ("[it]", theme.syntax_anaphor, theme.success_bg),
        LineBadge::Omitted => ("[OMITTED]", theme.syntax_omitted, theme.toolbar_bg),
        LineBadge::Exec => ("EXEC", theme.success_fg, theme.success_bg),
        LineBadge::Unsupported => ("UNSUPPORTED", theme.error_fg, theme.error_bg),
    };
    Frame::new()
        .fill(bg)
        .corner_radius(4)
        .inner_margin(Margin::symmetric(5, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(label).size(10.0).strong().color(fg));
        });
}
