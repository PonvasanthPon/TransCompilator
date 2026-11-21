//! `r1-editor` CLI entry point and terminal event loop.

use std::env;
use std::fs;
use std::io::{self, IsTerminal as _, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tc_editor::{
    EditorApp, RawTerminal, decode_keys, presets, run_elf_with_parity, run_paraphrase, version,
};

fn usage() {
    let _ = writeln!(
        io::stderr(),
        "usage: r1-editor [file.eng]\n       r1-editor --version\n       r1-editor --list-presets\n       r1-editor --check <file.eng>"
    );
}

fn write_line(text: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(text.as_bytes())?;
    stdout.write_all(b"\n")?;
    stdout.flush()
}

fn broken_pipe_is_success(result: io::Result<()>) -> Option<ExitCode> {
    match result {
        Ok(()) => None,
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Some(ExitCode::SUCCESS),
        Err(error) => {
            eprintln!("r1-editor: output failed: {error}");
            Some(ExitCode::FAILURE)
        }
    }
}

fn check_file(path: &Path) -> ExitCode {
    let source = match fs::read_to_string(path) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("r1-editor: cannot read `{}`: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let run = run_elf_with_parity(&source, Some(path), "");
    let paraphrase = run_paraphrase(&source, Some(path));
    let write = || -> io::Result<()> {
        write_line(&run.summary)?;
        for line in &run.lines {
            write_line(line)?;
        }
        write_line(&paraphrase.summary)?;
        for line in &paraphrase.lines {
            write_line(line)?;
        }
        Ok(())
    };
    if let Some(code) = broken_pipe_is_success(write()) {
        return code;
    }
    if run.ok && paraphrase.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn event_loop(mut app: EditorApp) -> Result<(), String> {
    if !io::stdin().is_terminal() {
        return Err("stdin is not a terminal; use --check for headless execution".to_string());
    }
    let terminal = RawTerminal::enter().map_err(|error| format!("raw mode failed: {error}"))?;
    let stdin = io::stdin();
    loop {
        let (width, height) = terminal.size();
        app.resize(width, height);
        let frame = app.render();
        let mut stdout = io::stdout();
        stdout
            .write_all(frame.as_bytes())
            .and_then(|()| stdout.flush())
            .map_err(|error| format!("render failed: {error}"))?;
        let mut bytes = [0_u8; 512];
        let read = stdin
            .lock()
            .read(&mut bytes)
            .map_err(|error| format!("input failed: {error}"))?;
        if read == 0 {
            break;
        }
        for event in decode_keys(&bytes[..read]) {
            let _ = app.handle_key(event);
            if app.should_quit {
                break;
            }
        }
        if app.should_quit {
            break;
        }
    }
    drop(terminal);
    Ok(())
}

fn main() -> ExitCode {
    let mut arguments = env::args().skip(1);
    let Some(first) = arguments.next() else {
        let app = EditorApp::new();
        return match event_loop(app) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("r1-editor: {message}");
                ExitCode::FAILURE
            }
        };
    };
    match first.as_str() {
        "--version" | "-V" => {
            println!("{}", version());
            ExitCode::SUCCESS
        }
        "--list-presets" => {
            for (index, preset) in presets().iter().enumerate() {
                println!("{}. {} - {}", index + 1, preset.name, preset.description);
            }
            ExitCode::SUCCESS
        }
        "--check" => {
            let Some(path) = arguments.next() else {
                eprintln!("r1-editor: --check requires a file");
                return ExitCode::from(2);
            };
            check_file(Path::new(&path))
        }
        "--help" | "-h" => {
            usage();
            ExitCode::SUCCESS
        }
        value if value.starts_with('-') => {
            eprintln!("r1-editor: unknown option `{value}`");
            usage();
            ExitCode::from(2)
        }
        path => {
            let target = PathBuf::from(path);
            let mut app = EditorApp::new();
            if target.exists() {
                if let Err(error) = app.open_path(&target) {
                    eprintln!("r1-editor: {error}");
                    return ExitCode::FAILURE;
                }
            } else {
                app = EditorApp::with_target(target);
            }
            match event_loop(app) {
                Ok(()) => ExitCode::SUCCESS,
                Err(message) => {
                    eprintln!("r1-editor: {message}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}
