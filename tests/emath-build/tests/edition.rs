//! edition tests migrated from the in-crate `#[cfg(test)]` module.
use emath_build::edition::*;
use emath_core::Edition;
use emath_core::E_PKG_EDITION_UNKNOWN;
use emath_test_harness::{Case, Probe, check_all};
use std::path::Path;

#[test]
fn source_build_emits_named_entries_and_a_buildable_crate() {
    let dir = std::env::temp_dir().join(format!("emath-build-entries-{}", std::process::id()));
    let source = "emath function First:
    inputs:
        x: Int
    outputs:
        result: Int
    definitions:
        result = x + 1
emath function Second:
    inputs:
        x: Int
    outputs:
        result: Int
    definitions:
        result = x + 2
";
    let report = emath_build::build_text(
        "entries.emath", source, &dir, emath_build::BuildOptions::default(),
    ).unwrap();
    let rust = std::fs::read_to_string(report.artifact_dir.join("src/lib.rs")).unwrap();
    assert!(rust.contains("pub fn First(") && rust.contains("pub fn Second("), "each function needs its own entry, not duplicate `entry` symbols");
    assert!(report.artifact_dir.join("Cargo.toml").is_file(), "a source build must write the emitted crate's manifest");
    let local_source = dir.join("entries.emath");
    std::fs::write(&local_source, source).unwrap();
    let local = emath_build::locked_build_script(
        &local_source, dir.join("locked-local"), &emath_build::ScriptLock::default(),
    ).expect("a self-contained crate needs no registry access");
    assert!(local.out_dir_isolated);

    // An engine-backed entry must resolve its retained local imports even
    // when the public source-build API receives a relative root path.
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../fixtures/keep/dream_tune.emath");
    let cwd = std::env::current_dir().unwrap();
    let relative = fixture.strip_prefix(&cwd).unwrap();
    assert!(!relative.is_absolute());
    let locked_dir = dir.join("locked-engine");
    let locked = emath_build::locked_build_script(
        relative, &locked_dir, &emath_build::ScriptLock::default(),
    );
    assert!(
        matches!(&locked, Err(error) if error.code == "E-CODEGEN-008"),
        "locked source builds must refuse unvendored registry dependencies: {locked:?}",
    );
    assert!(!locked_dir.exists(), "a lock refusal must not publish an artifact");
    let engine = emath_build::build_file(
        relative, dir.join("engine"), emath_build::BuildOptions::default(),
    ).unwrap();
    emath_build::locked_build_script(
        relative, dir.join("allowed-engine"),
        &emath_build::ScriptLock { allow_network: true, ..Default::default() },
    ).expect("explicit network allowance permits pinned engine dependencies");
    std::fs::write(
        engine.artifact_dir.join("src/main.rs"),
        format!("fn main() {{ {}::SeedTune(0).unwrap(); println!(\"retained imports resolved\"); }}", engine.crate_name),
    ).unwrap();
    let target_base = std::env::var_os("RCH_TARGET_BASE")
        .or_else(|| std::env::var_os("TMPDIR"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let output = std::process::Command::new("cargo")
        .args(["run", "--offline", "--quiet", "--jobs", "1", "-p", &engine.crate_name])
        .current_dir(&engine.artifact_dir)
        .env("CARGO_TARGET_DIR", target_base.join("rch_target_emath_test/source_build_imports"))
        .env("CARGO_PROFILE_DEV_DEBUG", "0")
        .output().unwrap();
    assert!(
        output.status.success(),
        "the relative-path artifact must compile and run:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr),
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "retained imports resolved");
}

#[test]
fn source_build_obeys_semantic_admission_and_package_edition() {
    if std::env::var_os("EMATH_TEST_RELATIVE_EDITION").is_some() {
        let result = emath_build::build_file(
            "input.emath", "unknown-edition", emath_build::BuildOptions::default(),
        );
        assert!(
            matches!(&result, Err(emath_build::BuildError::ReadFailed(detail)) if detail.contains(E_PKG_EDITION_UNKNOWN)),
            "relative file builds must honor ancestor package editions: {result:?}",
        );
        return;
    }
    let dir = std::env::temp_dir().join(format!("emath-build-admission-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = "emath function _:
    inputs:
        x: Int
    outputs:
        result: Int
    definitions:
        result = x + 1
";
    let result = emath_build::build_text(
        "input.emath", source, dir.join("refused"), emath_build::BuildOptions::default(),
    );
    assert!(
        matches!(&result, Err(emath_build::BuildError::AdmittedWithErrors(codes)) if codes.iter().any(|code| code.starts_with("E-NAME"))),
        "source builds must not bypass reserved-name admission: {result:?}"
    );
    let file = dir.join("input.emath");
    std::fs::write(&file, source.replacen("function _", "function Increment", 1)).unwrap();
    std::fs::write(dir.join("emath.toml"), "[package]\nedition = \"2099\"\n").unwrap();
    let result = emath_build::build_file(
        &file, dir.join("unknown-edition"), emath_build::BuildOptions::default(),
    );
    assert!(result.is_err(), "file builds must honor the package edition");
    let nested = dir.join("src");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("input.emath"), std::fs::read(&file).unwrap()).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "source_build_obeys_semantic_admission_and_package_edition", "--nocapture"])
        .current_dir(&nested)
        .env("EMATH_TEST_RELATIVE_EDITION", "1")
        .output().unwrap();
    assert!(
        output.status.success(),
        "nested-CWD relative source must obey the same edition:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn probe() {
    let mut p = Probe::new("edition field parses 2026, first wins, and refuses unknown/missing with codes");
    if let Err(m) = check_all(
        &[
            Case::new("bare", "edition = \"2026\"\n", Ok(Edition::Ed2026)),
            Case::new("table", "[package]\n  edition=\"2026\"  # founding epoch\n", Ok(Edition::Ed2026)),
            Case::new("first-wins", "edition = \"2026\"\nedition = \"2030\"\n", Ok(Edition::Ed2026)),
        ],
        |s| parse_edition_field(*s),
    ) {
        p.fail("parse", m);
    } else {
        p.demand("parse", true, "ok");
    }
    p.case("refusals", |p| {
        p.eq("unknown-code", parse_edition_field("edition = \"2099\"\n").expect_err("2099").code(), E_PKG_EDITION_UNKNOWN);
        p.eq("missing", parse_edition_field("[package]\nname = \"x\"\n"), Err(ManifestEditionError::Missing));
        p.eq("unquoted-code", parse_edition_field("edition = 2026\n").expect_err("unquoted").code(), E_PKG_EDITION_UNKNOWN);
        p.eq("unreadable", manifest_edition(Path::new("/nonexistent/emath.toml")).expect_err("missing").code(), "E-PKG-MANIFEST-UNREADABLE");
    });
    p.finish();
}
