//! The seeded negative-control battery must actually run against an honest, real staged tree.
mod common;
use emath_evidence::checker::{ArtifactCheckConfig, artifact_input_from_dir, check_artifact, run_standard_battery};
use emath_test_harness::Probe;
use common::{cleanup, fresh_tree};

#[test]
fn probe() {
    let mut p = Probe::new("seeded battery refuses all five controls on the honest tree");
    let (root, _manifest) = fresh_tree();
    let input = match artifact_input_from_dir(&root) {
        Ok(v) => v,
        Err(e) => {
            p.fail("reconstruct", format!("must reconstruct: {e}"));
            cleanup(&root);
            p.finish();
            return;
        }
    };
    p.demand("baseline-clean", check_artifact(&input, &ArtifactCheckConfig::default()).valid(), "honest baseline must verify");
    let run = run_standard_battery(&input);
    p.demand("all-refused", run.all_refused(), format!("escaped: {:?}", run.escaped));
    p.eq("five-seeds", run.refused.len(), 5);
    cleanup(&root);
    p.finish();
}
