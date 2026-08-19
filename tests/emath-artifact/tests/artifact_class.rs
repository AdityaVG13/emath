//! Artifact-class protocol witnesses: the seven classes round-trip their
//! stable tokens, every class package carries the four metadata documents
//! (code-bearing classes additionally ship a Cargo crate), and the
//! manifest schema id / version this crate publishes stay pinned.

use emath_artifact::{
    ARTIFACT_MANIFEST_SCHEMA, ARTIFACT_MANIFEST_VERSION, ArtifactClass, required_paths_for_class,
};

/// The four metadata documents every artifact package carries regardless
/// of class (also the full inventory of the metadata-only classes). Durable
/// paths pinning the same set `emath_artifact::required_artifact_paths`
/// publishes.
const METADATA_DOCUMENTS: [&str; 4] = [
    "emath/artifact-manifest.json",
    "emath/source-map.json",
    "emath/resolution-plan.json",
    "emath/evidence-bundle.json",
];

#[cfg(test)]
mod artifact_class_tests {
    use super::*;
    use std::str::FromStr;

    /// The protocol is seven classes exactly, each with a stable string
    /// token that round-trips (a class whose token cannot round-trip
    /// would silently vanish from parsed manifests).
    #[test]
    fn all_seven_classes_round_trip_their_tokens() {
        assert_eq!(ArtifactClass::ALL.len(), 7);
        for class in ArtifactClass::ALL {
            let token = class.as_str();
            assert_eq!(ArtifactClass::from_str(token), Ok(class), "{token}");
        }
        assert!(ArtifactClass::from_str("unknown").is_err());
    }

    /// Every class package carries the four metadata documents; the
    /// code-bearing classes additionally ship a Cargo crate. Diagnostic
    /// and exploration artifacts are metadata-only by design.
    #[test]
    fn every_class_has_a_package_inventory_with_the_metadata_documents() {
        for class in ArtifactClass::ALL {
            let paths = required_paths_for_class(class);
            for document in METADATA_DOCUMENTS {
                assert!(
                    paths.contains(&document),
                    "{} package must carry {document}",
                    class.as_str()
                );
            }
        }
        assert!(required_paths_for_class(ArtifactClass::Native).contains(&"src/lib.rs"));
        assert!(!required_paths_for_class(ArtifactClass::Diagnostic).contains(&"src/lib.rs"));
    }

    /// Manifest v1 marker: the schema id and version constant this crate
    /// publishes are what every consumer pins against.
    #[test]
    fn manifest_schema_and_version_are_pinned() {
        assert_eq!(ARTIFACT_MANIFEST_SCHEMA, "emath.artifact");
        assert_eq!(ARTIFACT_MANIFEST_VERSION, 1);
    }
}
