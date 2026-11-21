//! `r1-editor-gui` CLI entry point.

use std::env;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};
use std::thread;
use std::time::Duration;

use tc_editor_gui::GuiServer;

fn usage() {
    eprintln!(
        "usage: r1-editor-gui [file.eng] [--port <N>] [--no-open] [--dir <workspace>]\n       r1-editor-gui --version"
    );
}

fn open_browser(url: &str) {
    for candidate in [
        "chromium",
        "google-chrome",
        "brave-browser",
        "microsoft-edge",
        "xdg-open",
    ] {
        let mut command = Command::new(candidate);
        if candidate == "xdg-open" {
            command.arg(url);
        } else {
            command.arg(format!("--app={url}"));
        }
        if command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
        {
            return;
        }
    }
    eprintln!("r1-editor-gui: no browser launcher found; open {url} manually");
}

fn main() -> ExitCode {
    let mut port = 8792_u16;
    let mut no_open = false;
    let mut workspace = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--version" | "-V" => {
                println!("R1 Editor GUI {} (tc_r1)", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "--no-open" => no_open = true,
            "--port" => {
                let Some(value) = arguments.next() else {
                    eprintln!("r1-editor-gui: --port requires a number");
                    return ExitCode::from(2);
                };
                if let Ok(value) = value.parse::<u16>() {
                    port = value;
                } else {
                    eprintln!("r1-editor-gui: invalid port `{value}`");
                    return ExitCode::from(2);
                }
            }
            "--dir" => {
                let Some(value) = arguments.next() else {
                    eprintln!("r1-editor-gui: --dir requires a path");
                    return ExitCode::from(2);
                };
                workspace = PathBuf::from(value);
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
            file => {
                let path = PathBuf::from(file);
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() {
                        workspace = parent.to_path_buf();
                    }
                }
            }
        }
    }
    let address = format!("127.0.0.1:{port}");
    let server = match GuiServer::start(&address, &workspace) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("r1-editor-gui: cannot start {address}: {error}");
            return ExitCode::FAILURE;
        }
    };
    let url = server.url();
    println!("R1 Editor GUI: {url}");
    println!("workspace: {}", workspace.display());
    if !no_open {
        open_browser(&url);
    }
    loop {
        thread::sleep(Duration::from_secs(3600));
    }
}
