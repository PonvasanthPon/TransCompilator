//! Native `eframe::App` implementation and headless controller.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Align, Color32, Frame, Layout, Margin, RichText, Stroke, TextEdit};
use tc_editor::highlight::LineState;
use tc_editor::runner::{
    PRESETS, RunResult, build_elf_artifact, run_elf_with_parity, run_ir, run_paraphrase, run_trace,
};

use crate::gui_highlight::highlight_job;
use crate::gui_theme::{GuiTheme, ThemeMode};
use crate::workspace::{StdModule, Workspace, std_index};

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
    /// Tab label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Run => "Run & Parity",
            Self::Paraphrase => "Paraphrase",
            Self::Trace => "Trace",
            Self::Ir => "Core-7 IR",
            Self::Build => "Build .elf",
        }
    }
}

const TABS: [OutputTab; 5] = [
    OutputTab::Run,
    OutputTab::Paraphrase,
    OutputTab::Trace,
    OutputTab::Ir,
    OutputTab::Build,
];

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
    std_ref: bool,
    open_path: String,
    open_dialog: bool,
    new_name: String,
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
            std_ref: false,
            open_path: String::new(),
            open_dialog: false,
            new_name: "untitled.eng".to_string(),
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

    /// Active output tab.
    #[must_use]
    pub fn tab(&self) -> OutputTab {
        self.tab
    }

    /// Selects an output tab.
    pub fn set_tab(&mut self, tab: OutputTab) {
        self.tab = tab;
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
        self.std_ref
    }

    /// Toggles the `std/` reference.
    pub fn set_std_ref_open(&mut self, open: bool) {
        self.std_ref = open;
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

    /// Returns the gutter with line numbers and diagnostic markers.
    #[must_use]
    pub fn gutter_text(&self) -> String {
        let states = self.line_states();
        states
            .iter()
            .enumerate()
            .map(|(index, state)| {
                let marker = match state {
                    LineState::Error => '✗',
                    LineState::ProseOmitted => '·',
                    LineState::Normal => ' ',
                };
                format!("{:>3} {}", index + 1, marker)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Returns the latest output joined for display and tests.
    #[must_use]
    pub fn output_text(&self) -> String {
        self.result
            .as_ref()
            .map_or_else(String::new, |result| result.lines.join("\n"))
    }

    /// Returns the eight-module `std/` reference index.
    #[must_use]
    pub fn std_reference(&self) -> Vec<StdModule> {
        std_index()
    }

    fn handle_shortcuts(&mut self, context: &egui::Context) {
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
        if context.input(|input| {
            input.key_pressed(egui::Key::F9)
                || (input.modifiers.command && input.key_pressed(egui::Key::B))
        }) {
            self.build();
        }
        if context.input(|input| input.modifiers.command && input.key_pressed(egui::Key::S)) {
            if let Err(error) = self.save_current() {
                self.status = format!("Save failed: {error}");
            }
        }
    }

    fn toolbar(&mut self, context: &egui::Context) {
        let frame = Frame::new()
            .fill(self.theme.toolbar_bg)
            .inner_margin(Margin::symmetric(10, 7))
            .stroke(Stroke::new(1.0_f32, self.theme.border));
        egui::TopBottomPanel::top("r1_toolbar")
            .frame(frame)
            .show(context, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new("R1 Editor")
                            .strong()
                            .size(15.0)
                            .color(self.theme.ink),
                    );
                    let file_name = self
                        .workspace
                        .current()
                        .and_then(|path| path.file_name())
                        .and_then(|value| value.to_str())
                        .unwrap_or("untitled.eng");
                    ui.label(RichText::new(file_name).color(self.theme.muted));
                    if self.workspace.dirty() {
                        ui.label(RichText::new("●").color(self.theme.warning_fg));
                    }
                    ui.separator();
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("▶ Run").color(Color32::WHITE).strong(),
                            )
                            .fill(self.theme.accent),
                        )
                        .on_hover_text("Run + exact ELF64/interpreter parity (Ctrl+Enter / F5)")
                        .clicked()
                    {
                        self.run();
                    }
                    if ui
                        .button("Paraphrase")
                        .on_hover_text("Resolved English echo (F6)")
                        .clicked()
                    {
                        self.paraphrase();
                    }
                    if ui
                        .button("Trace")
                        .on_hover_text("Lexicon trace (F7)")
                        .clicked()
                    {
                        self.trace();
                    }
                    if ui
                        .button("Core-7 IR")
                        .on_hover_text("Formatted Core-7 IR (F8)")
                        .clicked()
                    {
                        self.ir();
                    }
                    if ui
                        .button("Build .elf")
                        .on_hover_text("Static zero-libc ELF64 (F9 / Ctrl+B)")
                        .clicked()
                    {
                        self.build();
                    }
                    ui.separator();
                    if ui.button("New").clicked() {
                        self.new_buffer();
                    }
                    if ui.button("Open").clicked() {
                        self.open_dialog = true;
                    }
                    if ui.button("Save").on_hover_text("Ctrl+S").clicked() {
                        if let Err(error) = self.save_current() {
                            self.status = format!("Save failed: {error}");
                        }
                    }
                    ui.menu_button("Presets ▾", |ui| {
                        for (index, preset) in PRESETS.iter().enumerate() {
                            if ui.button(preset.name).clicked() {
                                self.load_preset(index);
                                ui.close_menu();
                            }
                        }
                    });
                    ui.toggle_value(&mut self.std_ref, "std/ Ref");
                    if ui
                        .button(format!("Theme: {}", self.theme.mode.name()))
                        .clicked()
                    {
                        self.toggle_theme();
                    }
                });
            });
    }

    fn sidebar(&mut self, context: &egui::Context) {
        let frame = Frame::new()
            .fill(self.theme.window_bg)
            .inner_margin(Margin::symmetric(10, 8))
            .stroke(Stroke::new(1.0_f32, self.theme.border));
        egui::SidePanel::left("r1_sidebar")
            .resizable(false)
            .default_width(210.0)
            .frame(frame)
            .show(context, |ui| {
                ui.label(
                    RichText::new("WORKSPACE .ENG")
                        .size(10.0)
                        .color(self.theme.muted)
                        .strong(),
                );
                if ui.button("＋ New .eng").clicked() {
                    self.new_buffer();
                }
                ui.add_space(4.0);
                let files = self.workspace.files();
                if files.is_empty() {
                    ui.label(
                        RichText::new("No .eng files")
                            .italics()
                            .color(self.theme.muted),
                    );
                }
                for file in files {
                    let selected = self
                        .workspace
                        .current()
                        .is_some_and(|path| path.to_string_lossy() == file);
                    if ui.selectable_label(selected, &file).clicked() {
                        if let Err(error) = self.open_file(&file) {
                            self.status = format!("Open failed: {error}");
                        }
                    }
                }
                ui.add_space(10.0);
                ui.label(
                    RichText::new("STARTER PRESETS")
                        .size(10.0)
                        .color(self.theme.muted)
                        .strong(),
                );
                for (index, preset) in PRESETS.iter().enumerate() {
                    if ui
                        .selectable_label(false, preset.name)
                        .on_hover_text(preset.description)
                        .clicked()
                    {
                        self.load_preset(index);
                    }
                }
            });
    }

    fn output_panel(&mut self, context: &egui::Context) {
        let frame = Frame::new()
            .fill(self.theme.window_bg)
            .inner_margin(Margin::symmetric(10, 8))
            .stroke(Stroke::new(1.0_f32, self.theme.border));
        egui::SidePanel::right("r1_output")
            .resizable(true)
            .default_width(520.0)
            .min_width(340.0)
            .frame(frame)
            .show(context, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for tab in TABS {
                        if ui.selectable_label(self.tab == tab, tab.label()).clicked() {
                            self.tab = tab;
                        }
                    }
                });
                ui.add_space(6.0);
                let (pill_text, pill_fill) = if self.result.as_ref().is_some_and(|result| result.ok)
                {
                    (self.theme.success_fg, self.theme.success_bg)
                } else if self.result.is_some() {
                    (self.theme.error_fg, self.theme.error_bg)
                } else {
                    (self.theme.muted, self.theme.toolbar_bg)
                };
                Frame::new()
                    .fill(pill_fill)
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.label(RichText::new(&self.status).color(pill_text).strong());
                    });
                ui.add_space(4.0);
                let output = self.output_text();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if output.is_empty() {
                            ui.label(
                                RichText::new("No output yet. Press Run or choose a tab.")
                                    .italics()
                                    .color(self.theme.muted),
                            );
                        } else {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(output)
                                        .monospace()
                                        .size(12.0)
                                        .color(self.theme.ink),
                                )
                                .selectable(true),
                            );
                        }
                    });
                if self.std_ref {
                    ui.separator();
                    egui::CollapsingHeader::new("std/ Reference")
                        .default_open(true)
                        .show(ui, |ui| {
                            for module in self.std_reference() {
                                ui.label(
                                    RichText::new(&module.name)
                                        .strong()
                                        .color(self.theme.syntax_type),
                                );
                                for function in module.functions {
                                    ui.label(
                                        RichText::new(format!("  {}", function.signature))
                                            .monospace()
                                            .size(11.0)
                                            .color(self.theme.ink),
                                    );
                                    for phrase in function.phrases {
                                        ui.label(
                                            RichText::new(format!("    “{phrase}”"))
                                                .italics()
                                                .size(11.0)
                                                .color(self.theme.syntax_anaphor),
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
                let error_lines = self.error_lines.clone();
                let omitted_lines = self.omitted_lines.clone();
                let mut layouter = move |ui: &egui::Ui, text: &str, _wrap_width: f32| {
                    let job = highlight_job(text, &error_lines, &omitted_lines, &theme, 13.0);
                    ui.fonts(|fonts| fonts.layout_job(job))
                };
                let gutter = self.gutter_text();
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(gutter)
                                        .monospace()
                                        .size(13.0)
                                        .color(self.theme.gutter),
                                )
                                .selectable(false),
                            );
                            ui.separator();
                            let response = ui.add(
                                TextEdit::multiline(&mut self.editor)
                                    .code_editor()
                                    .desired_width(f32::INFINITY)
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
            .inner_margin(Margin::symmetric(10, 4))
            .stroke(Stroke::new(1.0_f32, self.theme.border));
        egui::TopBottomPanel::bottom("r1_status")
            .frame(frame)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&self.status)
                            .size(11.0)
                            .color(self.theme.muted),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let lines = self.editor.lines().count().max(1);
                        ui.label(
                            RichText::new(format!(
                                "Ln 1, Col 1  ·  {lines} lines  ·  .eng  ·  Static Zero-Libc x86_64"
                            ))
                            .size(11.0)
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
