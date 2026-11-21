//! Native R1 Editor library: UTF-8 buffer, key decoding, terminal safety,
//! in-process compiler bridge, ANSI UI, and application state.

pub mod app;
pub mod buffer;
pub mod highlight;
pub mod keys;
pub mod runner;
pub mod terminal;
pub mod theme;
pub mod ui;

pub use app::{EditorApp, Mode, PromptKind};
pub use buffer::TextBuffer;
pub use highlight::{LineState, StyledSpan, TokenStyle, highlight_line};
pub use keys::{KeyEvent, decode_keys};
pub use runner::{
    OutputTab, PRESETS, Preset, RunResult, build_elf_artifact, preset, presets,
    run_elf_with_parity, run_ir, run_paraphrase, run_trace,
};
pub use terminal::{RawTerminal, terminal_size};
pub use theme::{Rgb, Style, Theme, ThemeId};

/// Returns the canonical crate name.
#[must_use]
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

/// Returns the editor version string.
#[must_use]
pub fn version() -> String {
    format!("R1 Editor {} (tc_r1)", env!("CARGO_PKG_VERSION"))
}
