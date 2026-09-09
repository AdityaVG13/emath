//! Translation-witness validation tests.

use emath_evidence::checker::{
    TranslationRelation, TranslationSample, check_witness, validate_translation,
};
use emath_test_harness::Probe;

fn relation(label: &str, inputs: &[f64], outputs: &[f64]) -> TranslationRelation {
    TranslationRelation { label: label.into(), inputs: inputs.into(), outputs: outputs.into() }
}

fn sample(label: &str, inputs: &[f64], outputs: &[f64]) -> TranslationSample {
    TranslationSample { label: label.into(), inputs: inputs.into(), outputs: outputs.into() }
}

#[test]
fn translation_witness() {
    let mut p = Probe::new("translation witnesses are content-addressed and tamper-evident");
    p.case("happy-path-rechecks", |p| {
        let relations = vec![relation("f", &[2.0, 3.0], &[6.0])];
        let samples = vec![sample("f", &[2.0, 3.0], &[6.0])];
        let witness = validate_translation(&relations, &samples).expect("bit-identical rows validate");
        p.eq("relation-len", witness.relation_footprint.len(), 16);
        p.eq("sample-len", witness.sample_footprint.len(), 16);
        p.demand("relation-hex", witness.relation_footprint.chars().all(|c| c.is_ascii_hexdigit()), "relation footprint is hex");
        p.demand("sample-hex", witness.sample_footprint.chars().all(|c| c.is_ascii_hexdigit()), "sample footprint is hex");
        p.eq("content-id", witness.relation_footprint.clone(), witness.sample_footprint.clone());
        let again = validate_translation(&relations, &samples).expect("replay validates");
        p.eq("deterministic", again, witness.clone());
        p.demand("rechecks", check_witness(&witness, &relations, &samples).is_ok(), "fresh witness rechecks");
    });
    p.case("content-tracks-rows", |p| {
        let relations = vec![relation("f", &[2.0, 3.0], &[6.0])];
        let samples = vec![sample("f", &[2.0, 3.0], &[6.0])];
        let _ = validate_translation(&relations, &samples).expect("baseline validates");
        let changed = vec![sample("f", &[2.0, 3.0], &[7.0])];
        p.eq("code", validate_translation(&relations, &changed).unwrap_err().code, "E-EVID-301");
    });
    p.case("diverging-output-refused", |p| {
        let relations = vec![relation("f", &[2.0, 3.0], &[6.0])];
        let samples = vec![sample("f", &[2.0, 3.0], &[6.5])];
        p.eq("code", validate_translation(&relations, &samples).unwrap_err().code, "E-EVID-301");
    });
    p.case("tampered-witness-refused", |p| {
        let relations = vec![relation("f", &[1.0], &[1.0])];
        let samples = vec![sample("f", &[1.0], &[1.0])];
        let mut witness = validate_translation(&relations, &samples).expect("identical rows validate");
        witness.sample_footprint = "0000000000000000".into();
        p.eq("code", check_witness(&witness, &relations, &samples).unwrap_err().code, "E-EVID-302");
    });
    p.finish();
}
