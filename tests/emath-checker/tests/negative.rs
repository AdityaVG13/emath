//! Seeded negative controls for the independent evidence pipeline.

use emath_checker::{TranslationRelation, seed_wrong_derivative, validate_translation};

#[test]
fn seeded_wrong_derivative_is_refused_with_e_evid_301() {
    // CONTRACT.md (emath-checker): translation mismatch refuses with
    // E-EVID-301. `seed_wrong_derivative` plants a claimed derivative
    // output that disagrees with the retained evaluate relation.
    // Phase 1 has no differentiate producer; this is the strongest
    // existing evidence seam. The full wrong-derivative control lands
    // with the differentiate goal.
    let relation = TranslationRelation {
        label: "d_score_d_x".into(),
        inputs: vec![3.0, 1.0, 4.0],
        outputs: vec![1.0],
    };
    let planted = seed_wrong_derivative(&relation, 0.0);
    assert_eq!(planted.outputs, vec![0.0]);
    let error = validate_translation(&[relation], &[planted])
        .expect_err("a planted wrong derivative row must be refused");
    assert_eq!(error.code, "E-EVID-301");
}
