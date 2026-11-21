//! Assembling and static linking for generated x86_64 code.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::CodegenError;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Assembles `assembly` and links it into a static ELF64 at `out_path`.
pub(crate) fn assemble_and_link(assembly: &str, out_path: &Path) -> Result<(), CodegenError> {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp_dir =
        std::env::temp_dir().join(format!("tc_r1_codegen_{}_{}", std::process::id(), counter));
    fs::create_dir_all(&temp_dir)?;

    let result = (|| {
        let asm_path = temp_dir.join("module.s");
        let obj_path = temp_dir.join("module.o");
        fs::write(&asm_path, assembly)?;

        if let Some(parent) = out_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        let assembler = Command::new("as")
            .arg("--64")
            .arg("-o")
            .arg(&obj_path)
            .arg(&asm_path)
            .output()?;
        if !assembler.status.success() {
            return Err(CodegenError::Assembler(
                String::from_utf8_lossy(&assembler.stderr).into_owned(),
            ));
        }

        let linker = Command::new("ld")
            .arg("-static")
            .arg("-o")
            .arg(out_path)
            .arg(&obj_path)
            .output()?;
        if !linker.status.success() {
            return Err(CodegenError::Linker(
                String::from_utf8_lossy(&linker.stderr).into_owned(),
            ));
        }
        Ok(())
    })();

    let _ = fs::remove_dir_all(&temp_dir);
    result
}
