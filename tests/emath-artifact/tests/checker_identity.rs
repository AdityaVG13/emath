//! Checker-side identity round trip: the independent checker must
//! recompute the same artifact identity from the on-disk manifest that the
//! writer derived, and it must refuse dishonest trees: empty `files`
//! (E-EVID-109), symlinked required paths (E-EVID-113), non-UTF-8 payloads
//! (E-EVID-114), and a manifest whose embedded `artifact_id` disagrees
//! with a fresh computation (E-EVID-102).

use std::collections::BTreeMap;
use std::path::PathBuf;

use emath_artifact::{
    manifest_identity, write_artifact_manifest, write_evidence_bundle, write_resolution_plan,
    write_source_map, ArtifactClass, ArtifactManifest, EvidenceBundleRecord, OperationRecord,
    PlanRecord, SourceMap, SourceMapEntry,
};
use emath_checker::check_artifact_dir;
use emath_core::{content_id_of_str, ContentId, SchemaId};
use emath_ir::{ClaimVerdict, EvidenceClaim, EvidenceLevel, TargetProfile};

fn staging_dir() -> PathBuf {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "emath-checker-identity-{}",
        std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .replace("::", "-")
    ));
    root
}

fn cleanup(root: &std::path::Path) {
    if root.exists() {
        std::fs::remove_dir_all(root).expect("remove fake artifact tree");
    }
}

fn source_package() -> ContentId {
    content_id_of_str("emath-checker-identity")
}

fn fresh_manifest() -> ArtifactManifest {
    ArtifactManifest {
        schema: SchemaId("emath.artifact.v1".into()),
        artifact_id: ContentId("unset".into()),
        class: ArtifactClass::Native,
        source_package: source_package(),
        compiler: content_id_of_str("emath-build"),
        target: TargetProfile {
            family: "rust".to_string(),
            triple: None,
            features: Vec::new(),
        },
        numeric_profile: "f64".to_string(),
        providers: Vec::new(),
        evidence_level: EvidenceLevel::E1,
        public_exports: vec!["category".to_string()],
        assumptions: Vec::new(),
        files: BTreeMap::new(),
        source_map: content_id_of_str("unset"),
        resolution_plan: content_id_of_str("unset"),
        evidence_bundle: content_id_of_str("unset"),
    }
}

fn fake_source() -> String {
    "pub const CATEGORY: &str = \"category\";\n".to_string()
}

/// Build a complete, self-consistent artifact tree whose manifest `files`
/// map covers every required path. The manifest itself is deliberately
/// excluded from its own map (self-referential fingerprint), exactly like
/// `emath-build`'s `stage_files`.
fn write_fake_artifact(root: &std::path::Path, source: &str) {
    std::fs::create_dir_all(root.join("src")).expect("create src dir");
    std::fs::create_dir_all(root.join("emath")).expect("create emath dir");

    let source_map = SourceMap {
        schema: SchemaId("emath.source-map.v1".into()),
        source_package: source_package(),
        entries: vec![SourceMapEntry {
            source_file: "src/lib.rs".to_string(),
            source_start: 0,
            source_end: 44,
            semantic_node: "category".to_string(),
            plan_node: None,
            generated_file: "src/lib.rs".to_string(),
            generated_start: 0,
            generated_end: 44,
            generated_symbol: None,
        }],
    };
    let source_map_text = write_source_map(&source_map);

    let plan = PlanRecord {
        schema: SchemaId("emath.resolution-plan.v1".into()),
        plan_id: content_id_of_str("emath-checker-identity/plan"),
        goal: 0,
        policy: "native-everything".to_string(),
        artifact_class: "native".to_string(),
        operations: vec![OperationRecord {
            node: 0,
            operation: "package".to_string(),
            dependencies: Vec::new(),
            fallback: None,
        }],
        excluded_candidates: Vec::new(),
    };
    let plan_text = write_resolution_plan(&plan);

    let evidence = EvidenceBundleRecord {
        schema: SchemaId("emath.evidence-bundle.v1".into()),
        bundle_id: content_id_of_str("emath-checker-identity/evidence"),
        source_package: source_package(),
        resolution_plan: content_id_of_str(&plan_text),
        claims: vec![EvidenceClaim {
            id: "claim-1".to_string(),
            statement: "the category binding compiles and is exported".to_string(),
            class: "correctness".to_string(),
            scope: "category".to_string(),
            assumptions: Vec::new(),
            producer: "emath-build".to_string(),
            checker: Some("emath-checker".to_string()),
            verdict: ClaimVerdict::Pass,
            level: EvidenceLevel::E1,
            falsifiers: Vec::new(),
            artifacts: vec!["src/lib.rs".to_string()],
            fresh_until: Some("2030-01-01T00:00:00Z".to_string()),
        }],
        artifact_paths: vec!["src/lib.rs".to_string()],
        reproduction: vec!["emath build category".to_string()],
    };
    let evidence_text = write_evidence_bundle(&evidence);

    // Resolve every referenced document id, then freeze the manifest
    // identity exactly like the production build flow.
    let mut manifest = fresh_manifest();
    manifest
        .files
        .insert("Cargo.toml".to_string(), content_id_of_str(source));
    manifest
        .files
        .insert("src/lib.rs".to_string(), content_id_of_str(source));
    manifest.files.insert(
        "emath/source-map.json".to_string(),
        content_id_of_str(&source_map_text),
    );
    manifest.files.insert(
        "emath/resolution-plan.json".to_string(),
        content_id_of_str(&plan_text),
    );
    manifest.files.insert(
        "emath/evidence-bundle.json".to_string(),
        content_id_of_str(&evidence_text),
    );
    manifest.source_map = manifest
        .files
        .get("emath/source-map.json")
        .expect("set")
        .clone();
    manifest.resolution_plan = manifest
        .files
        .get("emath/resolution-plan.json")
        .expect("set")
        .clone();
    manifest.evidence_bundle = manifest
        .files
        .get("emath/evidence-bundle.json")
        .expect("set")
        .clone();
    manifest.artifact_id = manifest_identity(&manifest);
    let manifest_text = write_artifact_manifest(&manifest);

    std::fs::write(root.join("Cargo.toml"), source).expect("write Cargo.toml");
    std::fs::write(root.join("src/lib.rs"), source).expect("write src/lib.rs");
    std::fs::write(root.join("emath/source-map.json"), source_map_text).expect("write source map");
    std::fs::write(root.join("emath/resolution-plan.json"), plan_text).expect("write plan");
    std::fs::write(root.join("emath/evidence-bundle.json"), evidence_text).expect("write evidence");
    std::fs::write(root.join("emath/artifact-manifest.json"), manifest_text)
        .expect("write manifest");
}

fn fresh_tree() -> (PathBuf, PathBuf) {
    let root = staging_dir();
    cleanup(&root);
    std::fs::create_dir_all(&root).expect("create root");
    write_fake_artifact(&root, &fake_source());
    let manifest_path = root.join("emath/artifact-manifest.json");
    (root, manifest_path)
}

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
