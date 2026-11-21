//! Clean Light and Soft Dark palettes for the native R1 Editor window.

use eframe::egui::{Color32, Stroke, Visuals};

/// Exact light-theme chrome colors.
pub const LIGHT_WINDOW: [u8; 3] = [246, 248, 250];
/// Exact editor-canvas color.
pub const LIGHT_EDITOR: [u8; 3] = [255, 255, 255];
/// Exact toolbar color.
pub const LIGHT_TOOLBAR: [u8; 3] = [240, 243, 246];
/// Exact selected-item tint.
pub const LIGHT_SELECTED: [u8; 3] = [230, 240, 255];
/// Exact hairline border color.
pub const LIGHT_BORDER: [u8; 3] = [208, 215, 222];
/// Exact primary ink color.
pub const LIGHT_INK: [u8; 3] = [31, 35, 40];
/// Exact muted text color.
pub const LIGHT_MUTED: [u8; 3] = [87, 96, 106];
/// Exact gutter text color.
pub const LIGHT_GUTTER: [u8; 3] = [140, 149, 159];
/// Exact primary accent color.
pub const LIGHT_ACCENT: [u8; 3] = [9, 105, 218];
/// Exact success foreground.
pub const LIGHT_SUCCESS_FG: [u8; 3] = [26, 127, 55];
/// Exact success background.
pub const LIGHT_SUCCESS_BG: [u8; 3] = [218, 251, 225];
/// Exact warning foreground.
pub const LIGHT_WARNING_FG: [u8; 3] = [154, 103, 0];
/// Exact warning background.
pub const LIGHT_WARNING_BG: [u8; 3] = [255, 248, 197];
/// Exact error foreground.
pub const LIGHT_ERROR_FG: [u8; 3] = [207, 34, 46];
/// Exact error background.
pub const LIGHT_ERROR_BG: [u8; 3] = [255, 235, 233];
/// Exact control-anchor syntax color.
pub const LIGHT_SYNTAX_CONTROL: [u8; 3] = [5, 80, 174];
/// Exact action-verb syntax color.
pub const LIGHT_SYNTAX_VERB: [u8; 3] = [9, 105, 218];
/// Exact anaphor syntax color.
pub const LIGHT_SYNTAX_ANAPHOR: [u8; 3] = [14, 116, 144];
/// Exact type syntax color.
pub const LIGHT_SYNTAX_TYPE: [u8; 3] = [149, 56, 0];
/// Exact number syntax color.
pub const LIGHT_SYNTAX_NUMBER: [u8; 3] = [130, 80, 223];
/// Exact string syntax color.
pub const LIGHT_SYNTAX_STRING: [u8; 3] = [17, 99, 41];
/// Exact operator syntax color.
pub const LIGHT_SYNTAX_OPERATOR: [u8; 3] = [87, 96, 106];
/// Exact omitted-prose syntax color.
pub const LIGHT_SYNTAX_OMITTED: [u8; 3] = [110, 119, 129];

fn color(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

/// Selectable native window theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    /// Calm GitHub-style light chrome, selected by default.
    #[default]
    Light,
    /// Quiet slate dark chrome.
    Dark,
}

impl ThemeMode {
    /// Returns the display name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    /// Returns the opposite mode.
    #[must_use]
    pub fn toggled(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
    }
}

/// Native GUI palette shared by all widgets and the syntax layouter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiTheme {
    /// Active mode.
    pub mode: ThemeMode,
    /// Window and sidebar chrome.
    pub window_bg: Color32,
    /// Editor canvas.
    pub editor_bg: Color32,
    /// Toolbar and status bar.
    pub toolbar_bg: Color32,
    /// Selection and active tabs.
    pub selected_bg: Color32,
    /// Hairline borders.
    pub border: Color32,
    /// Primary text.
    pub ink: Color32,
    /// Secondary text.
    pub muted: Color32,
    /// Line-number gutter.
    pub gutter: Color32,
    /// Primary action accent.
    pub accent: Color32,
    /// Success foreground.
    pub success_fg: Color32,
    /// Success pill background.
    pub success_bg: Color32,
    /// Warning foreground.
    pub warning_fg: Color32,
    /// Warning pill background.
    pub warning_bg: Color32,
    /// Error foreground.
    pub error_fg: Color32,
    /// Error pill background.
    pub error_bg: Color32,
    /// Control-anchor syntax.
    pub syntax_control: Color32,
    /// Action-verb syntax.
    pub syntax_verb: Color32,
    /// Anaphor syntax.
    pub syntax_anaphor: Color32,
    /// Type syntax.
    pub syntax_type: Color32,
    /// Number syntax.
    pub syntax_number: Color32,
    /// String syntax.
    pub syntax_string: Color32,
    /// Operator syntax.
    pub syntax_operator: Color32,
    /// Omitted-prose syntax.
    pub syntax_omitted: Color32,
}

impl Default for GuiTheme {
    fn default() -> Self {
        Self::light()
    }
}

impl GuiTheme {
    /// Returns the default clean light palette.
    #[must_use]
    pub fn light() -> Self {
        Self {
            mode: ThemeMode::Light,
            window_bg: color(LIGHT_WINDOW),
            editor_bg: color(LIGHT_EDITOR),
            toolbar_bg: color(LIGHT_TOOLBAR),
            selected_bg: color(LIGHT_SELECTED),
            border: color(LIGHT_BORDER),
            ink: color(LIGHT_INK),
            muted: color(LIGHT_MUTED),
            gutter: color(LIGHT_GUTTER),
            accent: color(LIGHT_ACCENT),
            success_fg: color(LIGHT_SUCCESS_FG),
            success_bg: color(LIGHT_SUCCESS_BG),
            warning_fg: color(LIGHT_WARNING_FG),
            warning_bg: color(LIGHT_WARNING_BG),
            error_fg: color(LIGHT_ERROR_FG),
            error_bg: color(LIGHT_ERROR_BG),
            syntax_control: color(LIGHT_SYNTAX_CONTROL),
            syntax_verb: color(LIGHT_SYNTAX_VERB),
            syntax_anaphor: color(LIGHT_SYNTAX_ANAPHOR),
            syntax_type: color(LIGHT_SYNTAX_TYPE),
            syntax_number: color(LIGHT_SYNTAX_NUMBER),
            syntax_string: color(LIGHT_SYNTAX_STRING),
            syntax_operator: color(LIGHT_SYNTAX_OPERATOR),
            syntax_omitted: color(LIGHT_SYNTAX_OMITTED),
        }
    }

    /// Returns the optional quiet dark palette.
    #[must_use]
    pub fn dark() -> Self {
        Self {
            mode: ThemeMode::Dark,
            window_bg: Color32::from_rgb(30, 34, 40),
            editor_bg: Color32::from_rgb(36, 41, 48),
            toolbar_bg: Color32::from_rgb(25, 29, 34),
            selected_bg: Color32::from_rgb(46, 58, 74),
            border: Color32::from_rgb(58, 66, 77),
            ink: Color32::from_rgb(230, 233, 237),
            muted: Color32::from_rgb(160, 168, 178),
            gutter: Color32::from_rgb(120, 130, 142),
            accent: Color32::from_rgb(88, 166, 255),
            success_fg: Color32::from_rgb(63, 185, 80),
            success_bg: Color32::from_rgb(18, 38, 26),
            warning_fg: Color32::from_rgb(210, 153, 34),
            warning_bg: Color32::from_rgb(42, 33, 19),
            error_fg: Color32::from_rgb(248, 81, 73),
            error_bg: Color32::from_rgb(42, 20, 22),
            syntax_control: Color32::from_rgb(121, 192, 255),
            syntax_verb: Color32::from_rgb(88, 166, 255),
            syntax_anaphor: Color32::from_rgb(86, 212, 221),
            syntax_type: Color32::from_rgb(227, 179, 65),
            syntax_number: Color32::from_rgb(210, 168, 255),
            syntax_string: Color32::from_rgb(126, 231, 135),
            syntax_operator: Color32::from_rgb(160, 168, 178),
            syntax_omitted: Color32::from_rgb(139, 148, 158),
        }
    }

    /// Returns the palette for a mode.
    #[must_use]
    pub fn from_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Light => Self::light(),
            ThemeMode::Dark => Self::dark(),
        }
    }

    /// Applies the palette to an egui context.
    pub fn apply(&self, context: &eframe::egui::Context) {
        let mut visuals = if self.mode == ThemeMode::Light {
            Visuals::light()
        } else {
            Visuals::dark()
        };
        visuals.panel_fill = self.window_bg;
        visuals.window_fill = self.window_bg;
        visuals.extreme_bg_color = self.editor_bg;
        visuals.faint_bg_color = self.toolbar_bg;
        visuals.code_bg_color = self.editor_bg;
        visuals.override_text_color = Some(self.ink);
        visuals.selection.bg_fill = self.selected_bg;
        visuals.selection.stroke = Stroke::new(1.0_f32, self.accent);
        visuals.widgets.inactive.bg_fill = self.toolbar_bg;
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, self.ink);
        visuals.widgets.hovered.bg_fill = self.selected_bg;
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, self.ink);
        visuals.widgets.active.bg_fill = self.selected_bg;
        visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, self.accent);
        visuals.window_stroke = Stroke::new(1.0_f32, self.border);
        visuals.warn_fg_color = self.warning_fg;
        visuals.error_fg_color = self.error_fg;
        visuals.hyperlink_color = self.accent;
        context.set_visuals(visuals);
    }
}
