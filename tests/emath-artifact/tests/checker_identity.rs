//! Checker-side identity round trip: the independent checker recomputes artifact identity and refuses dishonest trees.
mod common;
use emath_artifact::{manifest_identity, write_artifact_manifest};
use emath_core::{ContentId, content_id_of_str};
use emath_evidence::checker::check_artifact_dir;
use emath_test_harness::Probe;
use common::{cleanup, fresh_manifest, fresh_tree};

fn codes(root: &std::path::Path) -> Vec<String> {
    check_artifact_dir(root).expect("still yields report").issues.iter().map(|i| i.code.to_string()).collect()
}
fn err_contains(root: &std::path::Path) -> String {
    check_artifact_dir(root).expect_err("must refuse").to_string()
}

#[test]
fn probe() {
    let mut p = Probe::new("checker recomputes identity and refuses dishonest trees with typed codes");
    p.case("fresh", |p| {
        let (root, _) = fresh_tree();
        p.demand("valid", check_artifact_dir(&root).expect("report").valid(), "fresh must verify");
        cleanup(&root);
    });
    p.case("tampered", |p| {
        let (root, _) = fresh_tree();
        std::fs::write(root.join("src/lib.rs"), "pub const CATEGORY: &str = \"tampered\";\n").expect("tamper");
        p.demand("E-EVID-101", codes(&root).contains(&"E-EVID-101".to_string()), "must be inventory mismatch");
        cleanup(&root);
    });
    p.case("forged-id", |p| {
        let (root, manifest_path) = fresh_tree();
        let original = std::fs::read_to_string(&manifest_path).expect("read");
        let prefix = "\"artifact_id\": \"";
        let start = original.find(prefix).expect("id") + prefix.len();
        let end = original[start..].find('"').expect("quote") + start;
        std::fs::write(&manifest_path, format!("{}fnv1a64:0000000000000000{}", &original[..start], &original[end..])).expect("forge");
        p.demand("E-EVID-102", codes(&root).contains(&"E-EVID-102".to_string()), "must be identity mismatch");
        cleanup(&root);
    });
    p.case("empty-files", |p| {
        let (root, manifest_path) = fresh_tree();
        let mut manifest = fresh_manifest();
        manifest.artifact_id = manifest_identity(&manifest);
        std::fs::write(&manifest_path, write_artifact_manifest(&manifest)).expect("replace");
        p.demand("E-EVID-109", codes(&root).contains(&"E-EVID-109".to_string()), "empty files must refuse");
        cleanup(&root);
    });
    #[cfg(unix)]
    p.case("symlink", |p| {
        use std::os::unix::fs::symlink;
        let (root, _) = fresh_tree();
        let original = root.join("src/lib.rs");
        std::fs::rename(&original, root.join("src/lib.rs.actual")).expect("move");
        symlink(root.join("src/lib.rs.actual"), &original).expect("link");
        p.contains("E-EVID-113", &err_contains(&root), "E-EVID-113");
        cleanup(&root);
    });
    p.case("non-utf8", |p| {
        let (root, _) = fresh_tree();
        std::fs::write(root.join("src/lib.rs"), [0xFFu8, 0xFE, 0x00, 0x80]).expect("write");
        p.contains("E-EVID-114", &err_contains(&root), "E-EVID-114");
        cleanup(&root);
    });
    p.case("missing", |p| {
        let (root, _) = fresh_tree();
        std::fs::remove_file(root.join("emath/evidence-bundle.json")).expect("remove");
        p.contains("E-EVID-105", &err_contains(&root), "E-EVID-105");
        cleanup(&root);
    });
    p.case("unparseable", |p| {
        let (root, _) = fresh_tree();
        std::fs::write(root.join("emath/resolution-plan.json"), "not json {").expect("corrupt");
        p.contains("E-EVID-108", &err_contains(&root), "E-EVID-108");
        cleanup(&root);
    });
    p.case("id-excludes-self", |p| {
        let mut manifest = fresh_manifest();
        let early = manifest_identity(&manifest);
        manifest.artifact_id = early.clone();
        let text = write_artifact_manifest(&manifest);
        manifest.artifact_id = ContentId("unset".into());
        manifest.files.insert("emath/artifact-manifest.json".to_string(), content_id_of_str(&text));
        p.eq("stable-id", manifest_identity(&manifest), early);
    });
    p.finish();
}
