//! Witnesses: staged paths cannot escape the artifact destination, and publish is atomic.
use emath_artifact::{ArtifactError, StagedFile, publish, stage, verify_artifact};
use emath_test_harness::Probe;
use std::path::{Path, PathBuf};

fn staging_dir() -> PathBuf {
    let name = std::thread::current().name().unwrap_or("unnamed").to_string();
    std::env::temp_dir().join(format!("emath-artifact-test-{}-{name}", std::process::id()))
}
fn valid_files() -> Vec<StagedFile> {
    ["Cargo.toml", "src/lib.rs", "emath/artifact-manifest.json", "emath/source-map.json", "emath/resolution-plan.json", "emath/evidence-bundle.json"]
        .map(|path| StagedFile { relative_path: path.to_string(), bytes: format!("content:{path}").into_bytes() })
        .to_vec()
}
fn cleanup(root: &Path) {
    let _ = std::fs::remove_dir_all(root);
}
fn tmp_dirs_under(root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else { return vec![] };
    let mut names: Vec<String> = entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with(".tmp-")).collect();
    names.sort_unstable();
    names
}

#[test]
fn probe() {
    let mut p = Probe::new("artifact publish refuses escapes and stays atomic");
    p.case("parent-escape", |p| {
        let e = stage(&[StagedFile { relative_path: "../escape.rs".into(), bytes: b"evil".to_vec() }], None).expect_err("must refuse");
        p.contains("msg", &e.to_string(), "refusing unsafe staged path");
        p.demand("typed", matches!(e, ArtifactError::InvalidStagedPath(_)), "must be typed");
    });
    p.case("absolute-escape", |p| {
        let e = stage(&[StagedFile { relative_path: "/tmp/escape.rs".into(), bytes: b"evil".to_vec() }], None).expect_err("must refuse");
        p.contains("msg", &e.to_string(), "refusing unsafe staged path");
        p.demand("typed", matches!(e, ArtifactError::InvalidStagedPath(_)), "must be typed");
    });
    p.case("publish-idempotent", |p| {
        let root = staging_dir();
        cleanup(&root);
        std::fs::create_dir_all(&root).expect("root");
        let files = valid_files();
        let staging = stage(&files, None).expect("stages");
        let dest = publish(&root, &staging.artifact_id, &files).expect("publish");
        p.demand("dir", dest.is_dir(), "must exist");
        p.demand("manifest", dest.join("emath/artifact-manifest.json").is_file(), "manifest published");
        p.demand("no-tmp", tmp_dirs_under(&root.join("emath")).is_empty(), "no tmp left");
        p.eq("republish", publish(&root, &staging.artifact_id, &files).expect("republish"), dest);
        cleanup(&root);
    });
    p.case("tamper-refuses", |p| {
        let root = staging_dir();
        cleanup(&root);
        std::fs::create_dir_all(&root).expect("root");
        let files = valid_files();
        let staging = stage(&files, None).expect("stages");
        let dest = publish(&root, &staging.artifact_id, &files).expect("publish");
        std::fs::write(dest.join("Cargo.toml"), b"tampered").expect("tamper");
        match publish(&root, &staging.artifact_id, &files) {
            Err(ArtifactError::VerificationMismatch(_)) => p.demand("typed", true, "ok"),
            other => p.fail("tamper", format!("must be VerificationMismatch, got {other:?}")),
        };
        cleanup(&root);
    });
    #[cfg(unix)]
    p.case("symlink-verify", |p| {
        use std::os::unix::fs::symlink;
        let root = staging_dir();
        cleanup(&root);
        std::fs::create_dir_all(&root).expect("root");
        let files = valid_files();
        let staging = stage(&files, None).expect("stages");
        let dest = publish(&root, &staging.artifact_id, &files).expect("publish");
        let target = dest.join("Cargo.toml");
        let real = dest.join("emath/Cargo.toml.real");
        std::fs::rename(&target, &real).expect("move");
        symlink(&real, &target).expect("link");
        p.contains("symlink", &verify_artifact(&dest, &staging).expect_err("must refuse").to_string(), "symlink");
        cleanup(&root);
    });
    p.case("failure-atomic", |p| {
        let root = staging_dir();
        cleanup(&root);
        std::fs::create_dir_all(&root).expect("root");
        let mut files = vec![StagedFile { relative_path: "emath".into(), bytes: b"file instead of directory".to_vec() }];
        files.extend(valid_files());
        let staging = stage(&files, None).expect("stages");
        let e = publish(&root, &staging.artifact_id, &files).expect_err("must fail");
        p.demand("msg", e.to_string().contains("cannot create") || e.to_string().contains("cannot write"), format!("unexpected {e}"));
        p.demand("no-dest", !root.join("emath").join(&staging.artifact_id.0).exists(), "no dest survives");
        p.demand("no-tmp", tmp_dirs_under(&root.join("emath")).is_empty(), "tmp cleaned");
        cleanup(&root);
    });
    p.finish();
}
