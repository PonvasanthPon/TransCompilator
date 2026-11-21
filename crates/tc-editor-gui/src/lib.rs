//! Native eframe/egui R1 Editor library: workspace, theme, highlighting, and app state.

pub use eframe::egui;

pub mod app;
pub mod gui_highlight;
pub mod gui_theme;
pub mod workspace;

pub use app::{OutputTab, R1GuiApp};
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
