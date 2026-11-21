//! Build-script wiring for the vendored Link Grammar shared library.
//!
//! The library lives inside this repository.  All paths are derived from
//! `CARGO_MANIFEST_DIR`, so no absolute developer path is embedded.  Relocatable
//! `$ORIGIN` run-paths are emitted for the common target layouts used by the
//! workspace binary and by Cargo test binaries.

use std::env;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let vendor = manifest.join("../../vendor/link-grammar");
    let lib = vendor.join("lib");
    let header = vendor.join("include/link-grammar/link-includes.h");

    println!("cargo:rustc-link-search=native={}", lib.display());
    println!("cargo:rustc-link-lib=dylib=link-grammar");
    println!("cargo:rerun-if-changed={}", header.display());
    println!(
        "cargo:rerun-if-changed={}",
        lib.join("liblink-grammar.so.5.13.2").display()
    );

    // `target/debug/tc` needs two levels; `target/debug/deps/<test>` needs
    // three; build-script output directories can sit deeper still.  Emitting
    // every common depth keeps the binaries runnable without LD_LIBRARY_PATH.
    for depth in 2..=6 {
        let origin = "../".repeat(depth);
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/{origin}vendor/link-grammar/lib");
    }
    // Absolute fallback is still derived from the manifest, never hardcoded.
    println!(
        "cargo:rustc-link-arg=-Wl,-rpath,{}",
        lib.canonicalize().unwrap_or(lib).display()
    );
}
