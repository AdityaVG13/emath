//! Demo host: integrates the generated `AffinePolicy` crate at build time
//! (build.rs runs the full eMath pipeline) and promotes the artifact
//! through the protected host surface: construct via the checked
//! constructor, evaluate, and verify the artifact fingerprints.

include!(concat!(env!("OUT_DIR"), "/affine_policy.rs"));

use emath_artifact::{required_artifact_paths, stage, verify_artifact, StagedFile};

fn main() {
    let artifact_id = env!("EMATH_ARTIFACT_ID");
    println!("artifact: {artifact_id}");

    // Construct through the generated constructor (invariants enforced).
    let policy = AffinePolicy::new(2.0, 1.0).expect("preconditions hold");
    let score = policy.score(3.0);
    println!("AffinePolicy::new(2.0, 1.0).score(3.0) = {score}");
    assert!(
        (score - 7.0).abs() < 1e-9,
        "score must equal state.scale * x + state.bias, got {score}"
    );

    // Negative control: invalid construction must be refused at runtime.
    let refused = AffinePolicy::new(-1.0, 0.5);
    assert!(
        refused.is_err(),
        "scale >= 0 must be enforced by the constructor"
    );
    println!(
        "negative control: new(-1.0, 0.5) refused: {:?}",
        refused.unwrap_err()
    );

    // Independent artifact verification: re-read and re-fingerprint the
    // published artifact under OUT_DIR.
    let out_dir = env!("OUT_DIR");
    let root = std::path::Path::new(out_dir)
        .join("emath")
        .join(artifact_id);
    let mut files = Vec::new();
    for relative in required_artifact_paths() {
        let bytes = std::fs::read(root.join(relative)).expect("artifact file exists");
        files.push(StagedFile {
            relative_path: (*relative).to_string(),
            bytes,
        });
    }
    let staging = stage(&files, None).expect("required artifact set is complete");
    assert!(
        staging.artifact_id.0 == artifact_id,
        "artifact identity mismatch: staged vs manifest"
    );
    verify_artifact(&root, &staging).expect("artifact fingerprints verified");
    println!("artifact verified ({} files)", files.len());

    println!("host integration ok");
}
