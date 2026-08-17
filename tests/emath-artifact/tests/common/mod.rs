//! Shared fixture: a complete, self-consistent staged artifact tree
//! built with the real in-tree writers (no serde, no fake-rs). Both the
//! identity lane (`checker_identity.rs`) and the negative-control
//! battery (`battery_seed.rs`) stage trees with these helpers, so the
//! battery's honest baseline is always the same tree the identity lane
//! verifies.

use std::collections::BTreeMap;
use std::path::PathBuf;

use emath_artifact::{
    ArtifactClass, ArtifactManifest, EvidenceBundleRecord, OperationRecord, PlanRecord, SourceMap,
    SourceMapEntry, manifest_identity, write_artifact_manifest, write_evidence_bundle,
    write_resolution_plan, write_source_map,
};
use emath_core::{ContentId, SchemaId, content_id_of_str};
use emath_ir::{ClaimVerdict, EvidenceClaim, EvidenceLevel, TargetProfile};

pub fn staging_dir() -> PathBuf {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "emath-checker-{}",
        std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .replace("::", "-")
    ));
    root
}

pub fn cleanup(root: &std::path::Path) {
    if root.exists() {
        std::fs::remove_dir_all(root).expect("remove fake artifact tree");
    }
}

pub fn source_package() -> ContentId {
    content_id_of_str("emath-checker-identity")
}

pub fn fake_source() -> String {
    "pub const CATEGORY: &str = \"category\";\n".to_string()
}

pub fn fresh_manifest() -> ArtifactManifest {
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

/// Build a complete, self-consistent artifact tree whose manifest `files`
/// map covers every required path. The manifest itself is deliberately
/// excluded from its own map (self-referential fingerprint), exactly like
/// `emath-build`'s `stage_files`.
pub fn write_fake_artifact(root: &std::path::Path, source: &str) {
    std::fs::create_dir_all(root.join("src")).expect("create src dir");
    std::fs::create_dir_all(root.join("emath")).expect("create emath dir");

    let source_map = SourceMap {
        schema: SchemaId("emath.source-map.v1".into()),
        source_package: source_package(),
        entries: vec![SourceMapEntry {
            file: 0,
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

/// A freshly staged, fully honest tree plus its manifest path.
pub fn fresh_tree() -> (PathBuf, PathBuf) {
    let root = staging_dir();
    cleanup(&root);
    std::fs::create_dir_all(&root).expect("create root");
    write_fake_artifact(&root, &fake_source());
    let manifest_path = root.join("emath/artifact-manifest.json");
    (root, manifest_path)
}
