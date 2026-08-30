//! Contract tests for significant figures + unit-preserving formatting
//! (bead emath-r3-sigfigs-formatting-yf28, 04 sections 1.6 + 1.7).
//!
//! Success criteria under test:
//! 1. display mode records sf counts
//! 2. `emath fmt` rounds to minimum input sf
//! 3. `format: preferred_unit min` reports minutes while the value stays seconds
//! 4. format is excluded from the identity hash
//! 5. incompatible unit in format = E-UNIT-FMT
//! 6. enforce mode: under-reporting sf = warning receipt
//! 7. mixing Measured with bare sf-values = warning

use emath_core::sigfigs::{
    count_sig_figs, quantity_identity, round_to_sig_figs, FormatSpec, FormattedQuantity,
    PrecisionLedger, PrecisionWarning, SigFigMode, SigFigSpec,
};
use emath_core::units::{Quantity, QuantityKind, UnitSpec, UnitTable};

const S: [i64; 7] = [0, 0, 1, 0, 0, 0, 0];
const M: [i64; 7] = [1, 0, 0, 0, 0, 0, 0];

fn seconds_table() -> UnitTable {
    let mut table = UnitTable::new();
    table
        .declare_unit(UnitSpec::new("s", S, 1.0, 0.0))
        .expect("s");
    table
        .declare_unit(UnitSpec::new("min", S, 60.0, 0.0))
        .expect("min");
    table
}

fn seconds(value: f64) -> Quantity {
    Quantity {
        value,
        unit: UnitSpec::new("s", S, 1.0, 0.0),
        kind: QuantityKind::Absolute,
    }
}

// --- 1.6 sig-figs ---

#[test]
fn display_mode_records_sf_counts() {
    let spec = SigFigSpec {
        mode: SigFigMode::Display,
        count: 0,
    };
    assert_eq!(spec.mode, SigFigMode::Display);
    // Documented convention: leading zeros never significant; trailing
    // zeros after a decimal point significant; trailing zeros of an
    // integer without a decimal point NOT significant.
    assert_eq!(count_sig_figs("1.230"), Some(4));
    assert_eq!(count_sig_figs("0.0012"), Some(2));
    assert_eq!(count_sig_figs("1230"), Some(3));
    assert_eq!(count_sig_figs("-45.6700"), Some(6));
    assert_eq!(count_sig_figs("abc"), None);
}

#[test]
fn fmt_rounds_output_to_minimum_input_sf() {
    assert_eq!(round_to_sig_figs(1234.5, 3), 1230.0);
    assert_eq!(round_to_sig_figs(0.001234, 2), 0.0012);
    assert_eq!(round_to_sig_figs(9.99, 2), 10.0);
    assert_eq!(round_to_sig_figs(0.0, 3), 0.0);
    assert_eq!(round_to_sig_figs(-1234.5, 3), -1230.0);
}

#[test]
fn enforce_mode_under_report_is_a_warning_receipt() {
    let spec = SigFigSpec {
        mode: SigFigMode::Enforce,
        count: 3,
    };
    assert_eq!(
        spec.enforce_check(2),
        Some(PrecisionWarning::UnderReported {
            declared: 3,
            literal: 2
        })
    );
    assert_eq!(spec.enforce_check(3), None);
    assert_eq!(spec.enforce_check(4), None);
}

// --- 1.7 unit-preserving formatting ---

#[test]
fn preferred_unit_reports_minutes_value_stays_seconds() {
    let table = seconds_table();
    let quantity = seconds(90.0);
    let spec = FormatSpec::parse("preferred_unit min").expect("parse");
    let formatted = FormattedQuantity {
        quantity: quantity.clone(),
        format: spec,
    };
    assert_eq!(formatted.display(&table, None).expect("display"), "1.5 min");
    // The value itself is untouched: presentation only.
    assert_eq!(formatted.quantity.value, 90.0);
}

#[test]
fn decimal_pattern_formats_and_appends_suffix() {
    let table = seconds_table();
    let spec = FormatSpec::parse("0.1 %").expect("parse");
    let formatted = FormattedQuantity {
        quantity: seconds(12.34),
        format: spec,
    };
    assert_eq!(formatted.display(&table, None).expect("display"), "12.3 %");
}

#[test]
fn format_is_excluded_from_identity_hash() {
    let quantity = seconds(90.0);
    let a = FormattedQuantity {
        quantity: quantity.clone(),
        format: FormatSpec::parse("0.1 %").expect("a"),
    };
    let b = FormattedQuantity {
        quantity,
        format: FormatSpec::parse("preferred_unit min").expect("b"),
    };
    // Same value, different presentation: same identity.
    assert_eq!(a.identity(), b.identity());
    assert_eq!(a.identity(), quantity_identity(&a.quantity));
}

#[test]
fn incompatible_unit_in_format_is_e_unit_fmt() {
    let mut table = seconds_table();
    table
        .declare_unit(UnitSpec::new("m", M, 1.0, 0.0))
        .expect("m");
    let formatted = FormattedQuantity {
        quantity: seconds(90.0),
        format: FormatSpec::parse("preferred_unit m").expect("parse"),
    };
    let err = formatted.display(&table, None).expect_err("must refuse");
    assert_eq!(err.code, "E-UNIT-FMT");
}

#[test]
fn malformed_format_is_e_unit_fmt() {
    assert_eq!(
        FormatSpec::parse("preferred_unit")
            .expect_err("missing unit")
            .code,
        "E-UNIT-FMT"
    );
    assert_eq!(FormatSpec::parse("0.1.2 x").expect_err("bad pattern").code, "E-UNIT-FMT");
}

// --- mixed precision ---

#[test]
fn mixing_measured_with_bare_sf_values_warns() {
    let mut ledger = PrecisionLedger::default();
    ledger.record_measured();
    assert_eq!(ledger.mix_warning(), None);
    ledger.record_bare_sf();
    assert_eq!(
        ledger.mix_warning(),
        Some(PrecisionWarning::MixedMeasuredBareSf {
            measured: 1,
            bare_sf: 1
        })
    );
    ledger.record_bare_sf();
    assert_eq!(
        ledger.mix_warning(),
        Some(PrecisionWarning::MixedMeasuredBareSf {
            measured: 1,
            bare_sf: 2
        })
    );
}
