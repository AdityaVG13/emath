//! Closed provenance and `core::measure` schema contracts.

use emath_ir::{
    DistributionKind, InstrumentRef, Measured, Provenance, SchemeBody, Timestamp,
    core_measure_schemes,
};
use emath_test_harness::Probe;

#[test]
fn intent() {
    let mut p = Probe::new("Closed provenance and `core::measure` schema contracts.");
    p.case("measured_schema_has_all_fields_and_closed_provenance_variants", |p| {

    let schemes = core_measure_schemes();
    p.eq("measured_schema_has_all_fields_and_closed_provenance_variants#1", schemes.len(), 2);

    let SchemeBody::Record(fields) = &schemes[0].body else {
        { p.fail("measured_schema_has_all_fields_and_closed_provenance_variants#2", format!("Measured<T> must be a record")); return; };
    };
    p.eq("measured_schema_has_all_fields_and_closed_provenance_variants#3", fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(), [
            "value",
            "std_uncertainty",
            "distribution",
            "provenance",
            "timestamp",
            "instrument",
        ].to_vec());

    let SchemeBody::Variant(variants) = &schemes[1].body else {
        { p.fail("measured_schema_has_all_fields_and_closed_provenance_variants#4", format!("Provenance must be a variant")); return; };
    };
    p.eq("measured_schema_has_all_fields_and_closed_provenance_variants#5", variants
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(), [
            "Exact",
            "Citation",
            "InstrumentRun",
            "Fitted",
            "Assumed",
            "Unstated",
        ].to_vec());

    });
    p.case("measured_values_require_provenance_and_unstated_is_explicit", |p| {

    let measured = Measured::new(
        0.42_f64,
        0.03,
        DistributionKind::Normal,
        Provenance::InstrumentRun {
            file: "sha256:abc".into(),
            processing: "baseline subtraction".into(),
            sha256: None,
        },
        Some(Timestamp("2026-08-27T00:00:00Z".into())),
        Some(InstrumentRef("balance-7".into())),
    );
    p.demand("measured_values_require_provenance_and_unstated_is_explicit#1", measured.provenance.variant_name() == "InstrumentRun", format!("expected {:?}, got {:?}", "InstrumentRun", measured.provenance.variant_name()));
    p.demand("measured_values_require_provenance_and_unstated_is_explicit#2", measured.instrument.as_ref().unwrap().0 == "balance-7", format!("expected {:?}, got {:?}", "balance-7", measured.instrument.as_ref().unwrap().0));

    let bare = Measured::unstated(1.0_f64, 0.1);
    p.eq("measured_values_require_provenance_and_unstated_is_explicit#3", bare.provenance, Provenance::Unstated);
    p.eq("measured_values_require_provenance_and_unstated_is_explicit#4", bare.distribution, DistributionKind::Normal);

    });
    p.case("optional_provenance_fields_are_identity_distinct_from_empty_values", |p| {

    p.ne("optional_provenance_fields_are_identity_distinct_from_empty_values#1", Provenance::Citation {
            reference: "doi:10.1234/example".into(),
            adjustment: None,
        }
        .canonical(), Provenance::Citation {
            reference: "doi:10.1234/example".into(),
            adjustment: Some(String::new()),
        }
        .canonical());
    p.ne("optional_provenance_fields_are_identity_distinct_from_empty_values#2", Provenance::Assumed { reason: None }.canonical(), Provenance::Assumed {
            reason: Some(String::new()),
        }
        .canonical());

    });
    p.finish();
}





