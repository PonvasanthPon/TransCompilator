//! Refined Light and Soft Dark palettes for the native R1 Editor window.

use eframe::egui::{Color32, CornerRadius, Stroke, Visuals};

/// Exact light window and sidebar chrome color.
pub const LIGHT_WINDOW: [u8; 3] = [248, 250, 252];
/// Exact light editor and output-card color.
pub const LIGHT_EDITOR: [u8; 3] = [255, 255, 255];
/// Exact light toolbar and status-bar color.
pub const LIGHT_TOOLBAR: [u8; 3] = [241, 245, 249];
/// Exact light selected-item tint.
pub const LIGHT_SELECTED: [u8; 3] = [235, 243, 255];
/// Exact light hover tint.
pub const LIGHT_HOVER: [u8; 3] = [238, 242, 246];
/// Exact light hairline border color.
pub const LIGHT_BORDER: [u8; 3] = [226, 232, 240];
/// Exact light primary ink color.
pub const LIGHT_INK: [u8; 3] = [15, 23, 42];
/// Exact light secondary ink color.
pub const LIGHT_MUTED: [u8; 3] = [71, 85, 105];
/// Exact light section-label color.
pub const LIGHT_SECTION: [u8; 3] = [100, 116, 139];
/// Exact light gutter text color.
pub const LIGHT_GUTTER: [u8; 3] = [148, 163, 184];
/// Exact light primary accent color.
pub const LIGHT_ACCENT: [u8; 3] = [9, 105, 218];
/// Exact light accent hover color.
pub const LIGHT_ACCENT_HOVER: [u8; 3] = [5, 80, 174];
/// Exact light success foreground.
pub const LIGHT_SUCCESS_FG: [u8; 3] = [22, 101, 52];
/// Exact light success background.
pub const LIGHT_SUCCESS_BG: [u8; 3] = [240, 253, 244];
/// Exact light success border.
pub const LIGHT_SUCCESS_BORDER: [u8; 3] = [187, 247, 208];
/// Exact light warning foreground.
pub const LIGHT_WARNING_FG: [u8; 3] = [146, 64, 14];
/// Exact light warning background.
pub const LIGHT_WARNING_BG: [u8; 3] = [255, 251, 235];
/// Exact light warning border.
pub const LIGHT_WARNING_BORDER: [u8; 3] = [253, 230, 138];
/// Exact light error foreground.
pub const LIGHT_ERROR_FG: [u8; 3] = [153, 27, 27];
/// Exact light error background.
pub const LIGHT_ERROR_BG: [u8; 3] = [254, 242, 242];
/// Exact light error border.
pub const LIGHT_ERROR_BORDER: [u8; 3] = [254, 202, 202];
/// Exact light error line tint.
pub const LIGHT_ERROR_LINE: [u8; 3] = [220, 38, 38];
/// Exact control-anchor syntax color.
pub const LIGHT_SYNTAX_CONTROL: [u8; 3] = [29, 78, 216];
/// Exact action-verb syntax color.
pub const LIGHT_SYNTAX_VERB: [u8; 3] = [3, 105, 161];
/// Exact anaphor syntax color.
pub const LIGHT_SYNTAX_ANAPHOR: [u8; 3] = [15, 118, 110];
/// Exact type syntax color.
pub const LIGHT_SYNTAX_TYPE: [u8; 3] = [154, 52, 18];
/// Exact number syntax color.
pub const LIGHT_SYNTAX_NUMBER: [u8; 3] = [109, 40, 217];
/// Exact string syntax color.
pub const LIGHT_SYNTAX_STRING: [u8; 3] = [21, 128, 61];
/// Exact operator syntax color.
pub const LIGHT_SYNTAX_OPERATOR: [u8; 3] = [71, 85, 105];
/// Exact omitted-prose syntax color.
pub const LIGHT_SYNTAX_OMITTED: [u8; 3] = [100, 116, 139];

fn color(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

/// Selectable native window theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    /// Calm paper-light chrome, selected by default.
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
    /// Window, sidebar, and inspector chrome.
    pub window_bg: Color32,
    /// Editor canvas and elevated output cards.
    pub editor_bg: Color32,
    /// Elevated card surface.
    pub card_bg: Color32,
    /// Toolbar and status bar.
    pub toolbar_bg: Color32,
    /// Selection and active controls.
    pub selected_bg: Color32,
    /// Gentle hover fill.
    pub hover_bg: Color32,
    /// Hairline borders.
    pub border: Color32,
    /// Primary text.
    pub ink: Color32,
    /// Secondary text.
    pub muted: Color32,
    /// Uppercase section labels.
    pub section: Color32,
    /// Line-number gutter.
    pub gutter: Color32,
    /// Primary action accent.
    pub accent: Color32,
    /// Primary action hover accent.
    pub accent_hover: Color32,
    /// Success foreground.
    pub success_fg: Color32,
    /// Success pill background.
    pub success_bg: Color32,
    /// Success pill border.
    pub success_border: Color32,
    /// Warning foreground.
    pub warning_fg: Color32,
    /// Warning pill background.
    pub warning_bg: Color32,
    /// Warning pill border.
    pub warning_border: Color32,
    /// Error foreground.
    pub error_fg: Color32,
    /// Error pill background.
    pub error_bg: Color32,
    /// Error pill border.
    pub error_border: Color32,
    /// Error line tint.
    pub error_line: Color32,
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
    /// Returns the default refined light palette.
    #[must_use]
    pub fn light() -> Self {
        Self {
            mode: ThemeMode::Light,
            window_bg: color(LIGHT_WINDOW),
            editor_bg: color(LIGHT_EDITOR),
            card_bg: color(LIGHT_EDITOR),
            toolbar_bg: color(LIGHT_TOOLBAR),
            selected_bg: color(LIGHT_SELECTED),
            hover_bg: color(LIGHT_HOVER),
            border: color(LIGHT_BORDER),
            ink: color(LIGHT_INK),
            muted: color(LIGHT_MUTED),
            section: color(LIGHT_SECTION),
            gutter: color(LIGHT_GUTTER),
            accent: color(LIGHT_ACCENT),
            accent_hover: color(LIGHT_ACCENT_HOVER),
            success_fg: color(LIGHT_SUCCESS_FG),
            success_bg: color(LIGHT_SUCCESS_BG),
            success_border: color(LIGHT_SUCCESS_BORDER),
            warning_fg: color(LIGHT_WARNING_FG),
            warning_bg: color(LIGHT_WARNING_BG),
            warning_border: color(LIGHT_WARNING_BORDER),
            error_fg: color(LIGHT_ERROR_FG),
            error_bg: color(LIGHT_ERROR_BG),
            error_border: color(LIGHT_ERROR_BORDER),
            error_line: color(LIGHT_ERROR_LINE),
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
            card_bg: Color32::from_rgb(40, 46, 54),
            toolbar_bg: Color32::from_rgb(25, 29, 34),
            selected_bg: Color32::from_rgb(46, 58, 74),
            hover_bg: Color32::from_rgb(42, 49, 58),
            border: Color32::from_rgb(58, 66, 77),
            ink: Color32::from_rgb(230, 233, 237),
            muted: Color32::from_rgb(160, 168, 178),
            section: Color32::from_rgb(148, 163, 184),
            gutter: Color32::from_rgb(120, 130, 142),
            accent: Color32::from_rgb(88, 166, 255),
            accent_hover: Color32::from_rgb(121, 192, 255),
            success_fg: Color32::from_rgb(74, 222, 128),
            success_bg: Color32::from_rgb(18, 38, 26),
            success_border: Color32::from_rgb(34, 78, 48),
            warning_fg: Color32::from_rgb(251, 191, 36),
            warning_bg: Color32::from_rgb(42, 33, 19),
            warning_border: Color32::from_rgb(84, 66, 25),
            error_fg: Color32::from_rgb(248, 113, 113),
            error_bg: Color32::from_rgb(42, 20, 22),
            error_border: Color32::from_rgb(88, 34, 38),
            error_line: Color32::from_rgb(248, 113, 113),
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

    /// Applies the palette and comfortable control metrics to an egui context.
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
        visuals.widgets.hovered.bg_fill = self.hover_bg;
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, self.ink);
        visuals.widgets.active.bg_fill = self.selected_bg;
        visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, self.accent);
        visuals.widgets.inactive.corner_radius = CornerRadius::same(6);
        visuals.widgets.hovered.corner_radius = CornerRadius::same(6);
        visuals.widgets.active.corner_radius = CornerRadius::same(6);
        visuals.window_stroke = Stroke::new(1.0_f32, self.border);
        visuals.warn_fg_color = self.warning_fg;
        visuals.error_fg_color = self.error_fg;
        visuals.hyperlink_color = self.accent;
        context.set_visuals(visuals);
        let mut style = (*context.style()).clone();
        style.spacing.button_padding = eframe::egui::vec2(10.0, 5.0);
        style.spacing.item_spacing = eframe::egui::vec2(8.0, 6.0);
        context.set_style(style);
    }
}
