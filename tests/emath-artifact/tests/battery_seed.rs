//! The seeded negative-control battery must actually run against an
//! honest, real staged tree (built by the shared fixture with the in-tree
//! writers), and every control must be refused with its expected code.
//! A control that escapes is an escaped defect: the checker admitted a
//! dishonest artifact.

mod common;

use emath_evidence::checker::{
    ArtifactCheckConfig, artifact_input_from_dir, check_artifact, run_standard_battery,
};

use common::{cleanup, fresh_tree};

#[test]
fn battery_refuses_honest_tree_seeds_with_expected_codes() {
    let (root, _manifest) = fresh_tree();

    // Honest baseline: the reconstructed input itself verifies clean,
    // so any refusal after seeding is caused by the seed, not by a
    // pre-existing defect in the fixture.
    let input = artifact_input_from_dir(&root).expect("reconstruct staged artifact input");
    let baseline = check_artifact(&input, &ArtifactCheckConfig::default());
    assert!(
        baseline.valid(),
        "honest baseline must be clean, got: {:?}",
        baseline.issues
    );

    let run = run_standard_battery(&input);
    assert!(
        run.all_refused(),
        "seeded controls escaped the checker: {:?}",
        run.escaped
    );
    assert_eq!(
        run.refused.len(),
        5,
        "all five seeds must be refused, got: {:?}",
        run.refused
    );

    cleanup(&root);
}
