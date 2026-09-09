//! Seeded negative controls for the independent evidence pipeline.

use emath_evidence::checker::{TranslationRelation, seed_wrong_derivative, validate_translation};
use emath_test_harness::Probe;

#[test]
fn seeded_negative_control() {
    let mut p = Probe::new("a planted wrong-derivative row is refused with E-EVID-301");
    let relation = TranslationRelation {
        label: "d_score_d_x".into(),
        inputs: vec![3.0, 1.0, 4.0],
        outputs: vec![1.0],
    };
    let planted = seed_wrong_derivative(&relation, 0.0);
    p.eq("planted", planted.outputs.clone(), vec![0.0]);
    let error = validate_translation(&[relation], &[planted])
        .expect_err("a planted wrong derivative row must be refused");
    p.eq("code", error.code, "E-EVID-301");
    p.finish();
}
