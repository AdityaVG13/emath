//! Binding provenance admission and identity behavior.

use emath_ir::{BindingSite, Provenance};
use emath_test_harness::{boot, Probe, Source};

fn source(reference: &str) -> String {
    format!(
        "emath function CalibratedLength:\n    inputs:\n        length: Float64\n        correction: Float64\n    outputs:\n        adjusted: Float64\n    definitions:\n        adjusted = length + correction\n    provenance:\n        length:\n            kind: \"Citation\"\n            reference: \"{reference}\"\n            adjustment: \"temperature corrected\"\n        correction:\n            kind: \"Assumed\"\n            reason: \"small calibration offset\"\n"
    )
}

#[test]
fn provenance_is_artifact_data_not_meaning() {
    boot();
    let mut p = Probe::new("provenance admits, feeds artifact identity, and never changes the admitted formula");
    p.case("artifact-vs-meaning", |p| {
        let first = Source::from_str("provenance-a", source("doi:10.1234/a")).must_admit(p);
        let repeat =
            Source::from_str("provenance-repeat", source("doi:10.1234/a")).must_admit(p);
        let changed = Source::from_str("provenance-b", source("doi:10.1234/b")).must_admit(p);
        p.eq(
            "citation",
            first.package.binding_provenance.get(&BindingSite::new(
                first.package.declarations[0].id,
                "length",
            )),
            Some(&Provenance::Citation {
                reference: "doi:10.1234/a".into(),
                adjustment: Some("temperature corrected".into()),
            }),
        );
        match (
            first.package.identity.as_ref(),
            repeat.package.identity.as_ref(),
            changed.package.identity.as_ref(),
        ) {
            (Some(a), Some(b), Some(c)) => {
                p.eq("deterministic", a.content.clone(), b.content.clone());
                p.ne("artifact-data", a.content.clone(), c.content.clone());
            }
            _ => {
                p.fail("identity", "admitted packages must carry content identity");
            }
        }
        match (
            first.package.meaning_id(&[]),
            changed.package.meaning_id(&[]),
        ) {
            (Ok(a), Ok(b)) => {
                p.eq("meaning-stable", a, b);
            }
            _ => {
                p.fail("meaning", "meaning ids must compute for admitted packages");
            }
        }
    });
    p.case("six-variants", |p| {
        let result = Source::from_str(
            "all-provenance",
            "emath function Sources:\n    inputs:\n        exact_value: Float64\n        cited_value: Float64\n        instrument_value: Float64\n        fitted_value: Float64\n        assumed_value: Float64\n        unstated_value: Float64\n    definitions:\n        result = exact_value\n    provenance:\n        exact_value:\n            kind: \"Exact\"\n            source: \"SI definition\"\n        cited_value:\n            kind: \"Citation\"\n            reference: \"doi:10.1234/example\"\n        instrument_value:\n            kind: \"InstrumentRun\"\n            file: \"sha256:abc\"\n            processing: \"raw\"\n        fitted_value:\n            kind: \"Fitted\"\n            fit_id: \"sha256:def\"\n        assumed_value:\n            kind: \"Assumed\"\n        unstated_value:\n            kind: \"Unstated\"\n",
        )
        .must_admit(p);
        p.eq("count", result.package.binding_provenance.len(), 6);
    });
    p.case("unknown-keys", |p| {
        Source::from_str(
            "bad-provenance",
            "emath function BadSource:\n    inputs:\n        value: Float64\n    provenance:\n        value:\n            kind: \"Citation\"\n            urlish: \"not a declared key\"\n        missing:\n            kind: \"Unstated\"\n",
        )
        .must_refuse(p, &["E-SYN-152", "E-NAME-028"]);
    });
    p.finish();
}
