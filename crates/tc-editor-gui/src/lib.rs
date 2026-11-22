//! Native eframe/egui R1 Editor library: workspace, theme, highlighting, and app state.

pub use eframe::egui;

pub mod app;
pub mod gui_highlight;
pub mod gui_theme;
pub mod workspace;

pub use app::{
    CardKind, DEFAULT_EDITOR_FONT_SIZE, GuiCard, INSPECTOR_DEFAULT_FRACTION, INSPECTOR_MIN_WIDTH,
    LineBadge, MAX_EDITOR_FONT_SIZE, MIN_EDITOR_FONT_SIZE, OutputTab, PillStyle, R1GuiApp,
    SECTION_FONT_SIZE, SIDEBAR_FONT_SIZE, SIDEBAR_WIDTH, STATUS_FONT_SIZE, TOOLBAR_HEIGHT,
    ToolbarSpec, inspector_default_width, line_badge, sidebar_pill_style, toolbar_spec,
};
pub use gui_highlight::{highlight_job, line_state, span_format};
pub use gui_theme::{GuiTheme, ThemeMode};
pub use workspace::{StdFunction, StdModule, Workspace, find_std_dir, std_index};

/// Returns the canonical crate name.
#[must_use]
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

/// Returns the GUI version string.
#[must_use]
pub fn version() -> String {
    format!("R1 Editor GUI {} (tc_r1)", env!("CARGO_PKG_VERSION"))
}
