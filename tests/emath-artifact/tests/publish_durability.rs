//! witnesses: staged paths cannot escape the artifact destination,
//! and publish is atomic (a failure leaves no destination directory).

use emath_artifact::{publish, stage, verify_artifact, ArtifactError, StagedFile};
use std::path::{Path, PathBuf};

fn staging_dir() -> PathBuf {
    let thread = std::thread::current();
    let name = thread.name().unwrap_or("unnamed");
    std::env::temp_dir().join(format!("emath-artifact-test-{}-{name}", std::process::id()))
}

fn valid_files() -> Vec<StagedFile> {
    [
        "Cargo.toml",
        "src/lib.rs",
        "emath/artifact-manifest.json",
        "emath/source-map.json",
        "emath/resolution-plan.json",
        "emath/evidence-bundle.json",
    ]
    .map(|path| StagedFile {
        relative_path: path.to_string(),
        bytes: format!("content:{path}").into_bytes(),
    })
    .to_vec()
}

fn cleanup(root: &Path) {
    let _ = std::fs::remove_dir_all(root);
}

fn tmp_dirs_under(root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".tmp-"))
        .collect();
    names.sort_unstable();
    names
}

#[test]
fn parent_dir_staged_path_is_refused() {
    let files = [StagedFile {
        relative_path: "../escape.rs".to_string(),
        bytes: b"evil".to_vec(),
    }];
    let error = stage(&files, None).expect_err("`..` path must be refused");
    assert!(
        error.to_string().contains("refusing unsafe staged path"),
        "unexpected error: {error}"
    );
    assert!(matches!(error, ArtifactError::InvalidStagedPath(_)));
}

#[test]
fn absolute_staged_path_is_refused() {
    let files = [StagedFile {
        relative_path: "/tmp/escape.rs".to_string(),
        bytes: b"evil".to_vec(),
    }];
    let error = stage(&files, None).expect_err("absolute path must be refused");
    assert!(
        error.to_string().contains("refusing unsafe staged path"),
        "unexpected error: {error}"
    );
    assert!(matches!(error, ArtifactError::InvalidStagedPath(_)));
}

#[test]
fn publish_succeeds_and_is_idempotent() {
    let root = staging_dir();
    cleanup(&root);
    std::fs::create_dir_all(&root).expect("create staging root");

    let files = valid_files();
    let staging = stage(&files, None).expect("valid artifact stages");
    let destination = publish(&root, &staging.artifact_id, &files).expect("publish succeeds");
    assert!(destination.is_dir(), "destination must exist after publish");
    assert!(
        destination.join("emath/artifact-manifest.json").is_file(),
        "manifest must be published"
    );
    assert!(tmp_dirs_under(&root.join("emath")).is_empty());
    assert!(
        root.join("emath").is_dir(),
        "publish root must not be left behind"
    );

    // Idempotent republish: identical content identity re-verifies in place.
    let second = publish(&root, &staging.artifact_id, &files).expect("republish ok");
    assert_eq!(destination, second);

    cleanup(&root);
}

#[test]
fn tampered_tree_refuses_republish() {
    let root = staging_dir();
    cleanup(&root);
    std::fs::create_dir_all(&root).expect("create staging root");

    let files = valid_files();
    let staging = stage(&files, None).expect("valid artifact stages");
    let destination = publish(&root, &staging.artifact_id, &files).expect("publish succeeds");

    // Tamper with one published file, then republish the original set:
    // content identity is fixed, so the mismatch is tamper, not a rebuild;
    // the verification failure stays typed instead of collapsing into a
    // "target exists" refusal.
    std::fs::write(destination.join("Cargo.toml"), b"tampered").expect("tamper write");
    let error = publish(&root, &staging.artifact_id, &files)
        .expect_err("tampered tree must refuse republish");
    assert!(matches!(error, ArtifactError::VerificationMismatch(_)));
    assert!(
        error.to_string().contains("content-identity verification"),
        "unexpected error: {error}"
    );

    cleanup(&root);
}

#[cfg(unix)]
#[test]
fn symlinked_required_path_is_refused_on_verify() {
    use std::os::unix::fs::symlink;

    let root = staging_dir();
    cleanup(&root);
    std::fs::create_dir_all(&root).expect("create staging root");

    let files = valid_files();
    let staging = stage(&files, None).expect("valid artifact stages");
    let destination = publish(&root, &staging.artifact_id, &files).expect("publish succeeds");

    // Replace a required file with a symlink to the original content:
    // a regular read would hash identically, but verification must refuse
    // to follow links into or out of the artifact tree.
    let target = destination.join("Cargo.toml");
    let real = destination.join("emath/Cargo.toml.real");
    std::fs::rename(&target, &real).expect("move original aside");
    symlink(&real, &target).expect("create symlink");
    let error = verify_artifact(&destination, &staging)
        .expect_err("symlinked required path must be refused");
    assert!(
        error.to_string().contains("symlink"),
        "unexpected error: {error}"
    );

    cleanup(&root);
}

#[test]
fn publish_refuses_symlinked_state_directory() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let root = staging_dir();
        cleanup(&root);
        std::fs::create_dir_all(&root).expect("create staging root");
        let real_emath = root.join("real-emath");
        std::fs::create_dir_all(&real_emath).expect("create real state dir");
        let link = root.join("emath");
        symlink(&real_emath, &link).expect("create state symlink");

        let files = valid_files();
        let staging = stage(&files, None).expect("valid artifact stages");
        let error = publish(&root, &staging.artifact_id, &files)
            .expect_err("publish through a symlinked state dir must be refused");
        assert!(
            error.to_string().contains("symlinked state directory"),
            "unexpected error: {error}"
        );

        cleanup(&root);
    }
}

#[test]
fn publish_failure_leaves_no_destination() {
    let root = staging_dir();
    cleanup(&root);
    std::fs::create_dir_all(&root).expect("create staging root");

    // A top-level file named `emath` collides with the required
    // `emath/...` directories; writing it first makes a later
    // create_dir_all fail mid-publish.
    let mut files = vec![StagedFile {
        relative_path: "emath".to_string(),
        bytes: b"file instead of directory".to_vec(),
    }];
    files.extend(valid_files());
    let staging = stage(&files, None).expect("required set still stages");

    let error = publish(&root, &staging.artifact_id, &files).expect_err("publish must fail");
    let message = error.to_string();
    assert!(
        message.contains("cannot create") || message.contains("cannot write"),
        "unexpected error: {error}"
    );
    let emath_root = root.join("emath");
    assert!(
        !emath_root.join(&staging.artifact_id.0).exists(),
        "no destination dir may survive a failed publish"
    );
    assert!(
        tmp_dirs_under(&emath_root).is_empty(),
        "staging temporaries must be cleaned up"
    );

    cleanup(&root);
}
