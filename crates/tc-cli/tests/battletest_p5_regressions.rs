//! Phase 5 battle-test regressions.

use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

#[test]
fn interp_cwd_enables_sandboxed_file_syscalls() {
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!(
        "tc_r1_p5_cli_cwd_{}_{}",
        std::process::id(),
        counter
    ));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("sandbox directory");
    let program = directory.join("fs_probe.eng");
    fs::write(
        &program,
        concat!(
            "Use fs, io, and string.\n",
            "Let name be create text from \"probe.txt\" of length 9.\n",
            "Save 7 to file address of name.\n",
            "Display file address of name.\n",
            "Delete file address of name.\n",
            "Release text address of name.\n",
            "Give back 0.\n",
        ),
    )
    .expect("program");
    let output = Command::new(env!("CARGO_BIN_EXE_tc"))
        .arg("interp")
        .arg("--cwd")
        .arg(&directory)
        .arg(&program)
        .output()
        .expect("run tc interp");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"7\n");
    assert!(!directory.join("probe.txt").exists());
    let _ = fs::remove_dir_all(directory);
}
