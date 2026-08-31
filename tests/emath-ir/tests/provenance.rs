//! Closed provenance and `core::measure` schema contracts.

use emath_ir::{
    DistributionKind, InstrumentRef, Measured, Provenance, SchemeBody, Timestamp,
    core_measure_schemes,
};

#[test]
fn measured_schema_has_all_fields_and_closed_provenance_variants() {
    let schemes = core_measure_schemes();
    assert_eq!(schemes.len(), 2);

    let SchemeBody::Record(fields) = &schemes[0].body else {
        panic!("Measured<T> must be a record");
    };
    assert_eq!(
        fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        [
            "value",
            "std_uncertainty",
            "distribution",
            "provenance",
            "timestamp",
            "instrument",
        ]
    );

    let SchemeBody::Variant(variants) = &schemes[1].body else {
        panic!("Provenance must be a variant");
    };
    assert_eq!(
        variants
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        [
            "Exact",
            "Citation",
            "InstrumentRun",
            "Fitted",
            "Assumed",
            "Unstated",
        ]
    );
}

#[test]
fn measured_values_require_provenance_and_unstated_is_explicit() {
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
    assert_eq!(measured.provenance.variant_name(), "InstrumentRun");
    assert_eq!(measured.instrument.unwrap().0, "balance-7");

    let bare = Measured::unstated(1.0_f64, 0.1);
    assert_eq!(bare.provenance, Provenance::Unstated);
    assert_eq!(bare.distribution, DistributionKind::Normal);
}

#[test]
fn optional_provenance_fields_are_identity_distinct_from_empty_values() {
    assert_ne!(
        Provenance::Citation {
            reference: "doi:10.1234/example".into(),
            adjustment: None,
        }
        .canonical(),
        Provenance::Citation {
            reference: "doi:10.1234/example".into(),
            adjustment: Some(String::new()),
        }
        .canonical()
    );
    assert_ne!(
        Provenance::Assumed { reason: None }.canonical(),
        Provenance::Assumed {
            reason: Some(String::new()),
        }
        .canonical()
    );
}
