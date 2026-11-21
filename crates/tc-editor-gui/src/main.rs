//! `r1-editor-gui` native desktop launcher.

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use eframe::egui;
use tc_editor::runner::run_ir;
use tc_editor_gui::R1GuiApp;

fn usage() {
    eprintln!(
        "usage: r1-editor-gui [file.eng] [--version] [--check <file.eng>]\n       r1-editor-gui --help"
    );
}

fn check(path: &Path) -> ExitCode {
    let source = match std::fs::read_to_string(path) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("r1-editor-gui: cannot read {}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let result = run_ir(&source, Some(path));
    if result.ok {
        println!("CHECK PASS: {}", path.display());
        ExitCode::SUCCESS
    } else {
        eprintln!("CHECK FAIL: {}", path.display());
        for line in result.lines {
            eprintln!("{line}");
        }
        ExitCode::FAILURE
    }
}

fn main() -> ExitCode {
    let mut file: Option<PathBuf> = None;
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--version" | "-V" => {
                println!("{}", tc_editor_gui::version());
                return ExitCode::SUCCESS;
            }
            "--check" => {
                let Some(value) = arguments.next() else {
                    eprintln!("r1-editor-gui: --check requires a path");
                    return ExitCode::from(2);
                };
                return check(&PathBuf::from(value));
            }
            "--help" | "-h" => {
                usage();
                return ExitCode::SUCCESS;
            }
            value if value.starts_with('-') => {
                eprintln!("r1-editor-gui: unknown option `{value}`");
                usage();
                return ExitCode::from(2);
            }
            value => file = Some(PathBuf::from(value)),
        }
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("R1 Editor")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([920.0, 600.0]),
        ..eframe::NativeOptions::default()
    };
    let result = eframe::run_native(
        "R1 Editor",
        options,
        Box::new(move |context| Ok(Box::new(R1GuiApp::new(context, file.as_deref())))),
    );
    if let Err(error) = result {
        eprintln!("r1-editor-gui: native window failed: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
