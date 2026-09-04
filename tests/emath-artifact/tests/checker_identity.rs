//! Checker-side identity round trip: the independent checker must
//! recompute the same artifact identity from the on-disk manifest that the
//! writer derived, and it must refuse dishonest trees: empty `files`
//! (E-EVID-109), symlinked required paths (E-EVID-113), non-UTF-8 payloads
//! (E-EVID-114), and a manifest whose embedded `artifact_id` disagrees
//! with a fresh computation (E-EVID-102).

mod common;

use emath_artifact::{manifest_identity, write_artifact_manifest};
use emath_core::{ContentId, content_id_of_str};
use emath_evidence::checker::check_artifact_dir;

use common::{cleanup, fresh_manifest, fresh_tree};

#[test]
fn fresh_artifact_verifies() {
    let (root, _manifest) = fresh_tree();
    let report = check_artifact_dir(&root).expect("fresh tree must produce a report");
    assert!(
        report.valid(),
        "expected a clean report, got: {:?}",
        report.issues
    );
    cleanup(&root);
}

#[test]
fn tampered_payload_is_inventory_mismatch() {
    let (root, _manifest) = fresh_tree();
    std::fs::write(
        root.join("src/lib.rs"),
        "pub const CATEGORY: &str = \"tampered\";\n",
    )
    .expect("tamper write");
    let report = check_artifact_dir(&root).expect("tampered tree still yields a report");
    let codes: Vec<&str> = report.issues.iter().map(|issue| issue.code).collect();
    assert!(
        codes.contains(&"E-EVID-101"),
        "expected E-EVID-101, got: {codes:?}"
    );
    cleanup(&root);
}

#[test]
fn forged_artifact_id_is_identity_mismatch() {
    let (root, manifest_path) = fresh_tree();
    let original = std::fs::read_to_string(&manifest_path).expect("read manifest");
    let prefix = "\"artifact_id\": \"";
    let start = original.find(prefix).expect("artifact_id present") + prefix.len();
    let end = original[start..].find('"').expect("closing quote") + start;
    let forged = format!(
        "{}fnv1a64:0000000000000000{}",
        &original[..start],
        &original[end..]
    );
    assert_ne!(original, forged, "fixture must contain a fnv1a64 id");
    std::fs::write(&manifest_path, forged).expect("forge id");
    let report = check_artifact_dir(&root).expect("forged manifest still yields a report");
    let codes: Vec<&str> = report.issues.iter().map(|issue| issue.code).collect();
    assert!(
        codes.contains(&"E-EVID-102"),
        "expected E-EVID-102, got: {codes:?}"
    );
    cleanup(&root);
}

#[test]
fn empty_files_map_is_refused() {
    let (root, manifest_path) = fresh_tree();
    // A manifest claiming no files paints a tree whose provenance cannot
    // be established from disk; the checker refuses it outright.
    let mut manifest = fresh_manifest();
    manifest.artifact_id = manifest_identity(&manifest);
    let text = write_artifact_manifest(&manifest);
    std::fs::write(&manifest_path, text).expect("replace manifest");
    let report = check_artifact_dir(&root).expect("empty-files manifest still yields a report");
    let codes: Vec<&str> = report.issues.iter().map(|issue| issue.code).collect();
    assert!(
        codes.contains(&"E-EVID-109"),
        "expected E-EVID-109, got: {codes:?}"
    );
    cleanup(&root);
}

#[cfg(unix)]
#[test]
fn symlinked_required_path_is_refused_by_dir_check() {
    use std::os::unix::fs::symlink;

    let (root, _manifest) = fresh_tree();
    let original = root.join("src/lib.rs");
    std::fs::rename(&original, root.join("src/lib.rs.actual")).expect("move original");
    symlink(root.join("src/lib.rs.actual"), &original).expect("create symlink");
    let error = check_artifact_dir(&root).expect_err("symlink must be refused as an error");
    let message = error.to_string();
    assert!(
        message.contains("E-EVID-113"),
        "expected E-EVID-113, got: {message}"
    );
    cleanup(&root);
}

#[test]
fn non_utf8_payload_is_refused_by_dir_check() {
    let (root, _manifest) = fresh_tree();
    std::fs::write(root.join("src/lib.rs"), [0xFFu8, 0xFE, 0x00, 0x80])
        .expect("write non-UTF-8 payload");
    let error = check_artifact_dir(&root).expect_err("non-UTF-8 must be refused as an error");
    let message = error.to_string();
    assert!(
        message.contains("E-EVID-114"),
        "expected E-EVID-114, got: {message}"
    );
    cleanup(&root);
}

#[test]
fn missing_required_path_is_refused_by_dir_check() {
    let (root, _manifest) = fresh_tree();
    std::fs::remove_file(root.join("emath/evidence-bundle.json")).expect("remove evidence");
    let error = check_artifact_dir(&root).expect_err("missing required path must be refused");
    let message = error.to_string();
    assert!(
        message.contains("E-EVID-105"),
        "expected E-EVID-105, got: {message}"
    );
    cleanup(&root);
}

#[test]
#[allow(dead_code)]
fn referenced_document_unparseable_is_refused_by_dir_check() {
    let (root, _manifest) = fresh_tree();
    std::fs::write(root.join("emath/resolution-plan.json"), "not json {").expect("corrupt plan");
    let error = check_artifact_dir(&root).expect_err("unparseable document must be refused");
    let message = error.to_string();
    assert!(
        message.contains("E-EVID-108"),
        "expected E-EVID-108, got: {message}"
    );
    cleanup(&root);
}

#[test]
fn manifest_id_excludes_its_own_text() {
    // The manifest is absent from its own `files` map, so identity must
    // be independent of the serialized text that carries it; otherwise
    // writing an id back into the manifest would change the id forever.
    let mut manifest = fresh_manifest();
    let early = manifest_identity(&manifest);
    manifest.artifact_id = early.clone();
    let text = write_artifact_manifest(&manifest);
    manifest.artifact_id = ContentId("unset".into());
    let mut manifest_with_file = manifest;
    manifest_with_file.files.insert(
        "emath/artifact-manifest.json".to_string(),
        content_id_of_str(&text),
    );
    assert_eq!(
        manifest_identity(&manifest_with_file),
        early,
        "the manifest's own entry must not perturb artifact identity"
    );
}
