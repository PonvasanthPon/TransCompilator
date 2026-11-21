//! Adds relocatable run-paths for the vendored Link Grammar library.

use std::env;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let vendor = manifest.join("../../vendor/link-grammar");
    let lib = vendor.join("lib");
    println!("cargo:rerun-if-changed={}", lib.display());
    for depth in 2..=6 {
        let origin = "../".repeat(depth);
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/{origin}vendor/link-grammar/lib");
    }
    println!(
        "cargo:rustc-link-arg=-Wl,-rpath,{}",
        lib.canonicalize().unwrap_or(lib).display()
    );
}
