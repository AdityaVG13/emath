//! Host build script: compile the `.emath` spec into a real artifact under
//! `$OUT_DIR` and surface the generated code for `include!`.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let spec = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../../tests/valid/affine_scorer.emath");
    assert!(spec.is_file(), "missing spec: {}", spec.display());
    emath_build::emit_rerun_if_changed(&spec);

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is set by cargo"));
    let report = emath_build::build_into_out_dir(&spec, &out_dir)
        .expect("spec must build in the host's build script");

    // `include!` cannot carry `#![...]` inner attributes: strip them and
    // surface the body of the generated crate for the host.
    let lib = fs::read(report.artifact_dir.join("src/lib.rs")).expect("generated src/lib.rs");
    let text = String::from_utf8(lib).expect("generated code is utf-8");
    let stripped: String = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("#!["))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(out_dir.join("affine_scorer.rs"), stripped).expect("write host copy");

    // Hand the artifact id to the host via an env var for runtime reporting.
    println!("cargo:rustc-env=EMATH_ARTIFACT_ID={}", report.artifact_id.0);
}
