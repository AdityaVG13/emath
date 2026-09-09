//! Artifact-class protocol witnesses: the seven classes round-trip their stable tokens and carry the four metadata documents.
use std::str::FromStr;
use emath_artifact::{ARTIFACT_MANIFEST_SCHEMA, ARTIFACT_MANIFEST_VERSION, ArtifactClass, required_paths_for_class};
use emath_test_harness::{Case, Probe, check_all};

const METADATA_DOCUMENTS: [&str; 4] = ["emath/artifact-manifest.json", "emath/source-map.json", "emath/resolution-plan.json", "emath/evidence-bundle.json"];

#[test]
fn probe() {
    let mut p = Probe::new("artifact classes round-trip seven tokens and pin manifest v1");
    p.eq("seven", ArtifactClass::ALL.len(), 7);
    if let Err(m) = check_all(&ArtifactClass::ALL.map(|c| Case::new(c.as_str(), c.as_str(), c)), |s| ArtifactClass::from_str(s).unwrap()) {
        p.fail("round-trip", m);
    } else {
        p.demand("round-trip", true, "ok");
    }
    p.demand("unknown", ArtifactClass::from_str("unknown").is_err(), "unknown must refuse");
    p.case("inventory", |p| {
        for class in ArtifactClass::ALL {
            let paths = required_paths_for_class(class);
            for doc in METADATA_DOCUMENTS {
                p.demand(format!("{}/{doc}", class.as_str()), paths.contains(&doc), "must carry metadata");
            }
        }
        p.demand("native-crate", required_paths_for_class(ArtifactClass::Native).contains(&"src/lib.rs"), "native ships crate");
        p.demand("diagnostic-meta", !required_paths_for_class(ArtifactClass::Diagnostic).contains(&"src/lib.rs"), "diagnostic is metadata-only");
    });
    p.eq("schema", ARTIFACT_MANIFEST_SCHEMA, "emath.artifact");
    p.eq("version", ARTIFACT_MANIFEST_VERSION, 1);
    p.finish();
}
