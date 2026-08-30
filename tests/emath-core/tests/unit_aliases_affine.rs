//! Failure-first contract tests for unit aliases and affine units
//! (bead emath-r3-unit-aliases-affine-tao6, 04 sections 1.2 + 1.3).
//!
//! Contracts (each test names the behavior it kills):
//! 1. alias_resolution_identity — `alias liter = L` makes liter and L the
//!    SAME unit: equal identity hash, never an approximation.
//! 2. alias_rebinding_refused — `cal` declared at 4.184 J-scale; rebinding
//!    `cal` to a 4.1868 J-scale unit is a refusal (two units with names
//!    differing by more than zero are two units; a conversion that is not
//!    exact is a conversion function, never an alias).
//! 3. affine_creation_c13_order — degC = affine K offset 273.15 scale 1;
//!    degF = affine K with offset ALWAYS pre-scale: K = (F + 459.67) * 5/9.
//! 4. c13_conformance_32f — 32 degF == 273.15 K (the pinned conformance).
//! 5. affine_plus_affine_refused — 22 degC + 10 degC = E-UNIT-AFFINE-1
//!    ("cannot add absolute temperatures").
//! 6. affine_plus_difference_admitted — 22 degC + 10 K-difference = 32 degC.
//! 7. affine_subtraction_yields_difference — 22 degC - 10 degC = difference
//!    12 in ΔdegC, which is multiplicative and equal in scale to K.
//! 8. offset_order_negative_control — pre-scale conversion matches 273.15 K
//!    while the post-scale encoding (the C13 bug) differs by ~204 K, so the
//!    wrong order cannot silently pass.

use emath_core::units::{
    add, difference_unit, mul, sub, Quantity, QuantityKind, UnitSpec, UnitTable,
};

const KELVIN: [i64; 7] = [0, 0, 0, 0, 1, 0, 0];

fn seed() -> UnitTable {
    let mut table = UnitTable::new();
    table
        .declare_unit(UnitSpec::new("K", KELVIN, 1.0, 0.0))
        .expect("K");
    table
        .declare_unit(UnitSpec::new("degC", KELVIN, 1.0, 273.15))
        .expect("degC");
    table
        .declare_unit(UnitSpec::new("degF", KELVIN, 5.0 / 9.0, 459.67))
        .expect("degF");
    table
}

fn absolute(table: &UnitTable, name: &str, value: f64) -> Quantity {
    Quantity {
        value,
        unit: table.resolve(name).expect("unit exists"),
        kind: QuantityKind::Absolute,
    }
}

fn difference_of(table: &UnitTable, name: &str, value: f64) -> Quantity {
    Quantity {
        value,
        unit: difference_unit(&table.resolve(name).expect("unit exists")),
        kind: QuantityKind::Difference,
    }
}

#[test]
fn alias_resolution_identity() {
    let cubic_metre: [i64; 7] = [3, 0, 0, 0, 0, 0, 0];
    let mut table = UnitTable::new();
    table
        .declare_unit(UnitSpec::new("L", cubic_metre, 1e-3, 0.0))
        .expect("L");
    table.declare_alias("liter", "L").expect("alias liter");
    table.declare_alias("litre", "L").expect("alias litre");
    assert_eq!(
        table.identity("liter").expect("identity"),
        table.identity("L").expect("identity"),
        "alias must hash to the SAME unit as its target (alias-as-identity)"
    );
    assert_eq!(
        table.identity("litre").expect("identity"),
        table.identity("L").expect("identity"),
    );
    let resolved = table.resolve("liter").expect("resolve");
    assert_eq!(resolved.scale, 1e-3, "alias carries the target's scale");
    assert_eq!(resolved.offset, 0.0);
}

#[test]
fn alias_rebinding_over_different_scale_is_refused() {
    const JOULE: [i64; 7] = [2, 1, -2, 0, 0, 0, 0];
    let mut table = UnitTable::new();
    table
        .declare_unit(UnitSpec::new("J", JOULE, 1.0, 0.0))
        .expect("J");
    // `cal` declared at 4.184 J.
    table
        .declare_unit(UnitSpec::new("cal", JOULE, 4.184, 0.0))
        .expect("cal at 4.184 J");
    // cal_IT is a DIFFERENT unit at 4.1868 J (names differing by more than
    // zero are two units).
    table
        .declare_unit(UnitSpec::new("cal_IT", JOULE, 4.1868, 0.0))
        .expect("cal_IT");
    // Rebinding `cal` to cal_IT must be a refusal, not a silent redefinition.
    let error = table
        .declare_alias("cal", "cal_IT")
        .expect_err("rebinding cal over 4.184 J with 4.1868 J must refuse");
    assert_eq!(error.code, "E-UNIT-ALIAS-CONFLICT", "{:?}", error);
    // The original binding survives the refused rebind.
    assert_eq!(table.resolve("cal").expect("cal survives").scale, 4.184);
}

#[test]
fn alias_to_unknown_target_is_refused() {
    let mut table = UnitTable::new();
    let error = table
        .declare_alias("liter", "L")
        .expect_err("alias to undeclared target");
    assert_eq!(error.code, "E-UNIT-ALIAS-CONFLICT", "{:?}", error);
}

#[test]
fn affine_creation_c13_pre_scale_order() {
    let table = seed();
    let celsius = table.resolve("degC").expect("degC");
    assert!(celsius.is_affine());
    assert_eq!(celsius.offset, 273.15);
    assert_eq!(celsius.scale, 1.0);
    // C13: offset is ALWAYS pre-scale — K = (F + 459.67) * 5/9, not
    // K = F * 5/9 + 459.67.
    let fahrenheit = table.resolve("degF").expect("degF");
    assert_eq!(fahrenheit.scale, 5.0 / 9.0);
    assert_eq!(fahrenheit.offset, 459.67);
    let freezing = fahrenheit.to_si(32.0);
    assert!(
        (freezing - 273.15).abs() < 1e-9,
        "pre-scale conversion of 32 degF must give 273.15 K, got {freezing}"
    );
    let boiling = fahrenheit.to_si(212.0);
    assert!(
        (boiling - 373.15).abs() < 1e-9,
        "pre-scale conversion of 212 degF must give 373.15 K, got {boiling}"
    );
    assert!((celsius.to_si(0.0) - 273.15).abs() < 1e-9);
}

#[test]
fn c13_conformance_32f_equals_273_15k() {
    let table = seed();
    let fahrenheit = table.resolve("degF").expect("degF");
    let celsius = table.resolve("degC").expect("degC");
    // Both SI magnitudes must agree: 32 degF and 0 degC are the same point,
    // 273.15 K. (K.to_si(0.0) is absolute zero, not the conformance point.)
    let freezing_f = fahrenheit.to_si(32.0);
    let freezing_c = celsius.to_si(0.0);
    assert!((freezing_f - 273.15).abs() < 1e-9, "32 degF -> {freezing_f}");
    assert!(
        (freezing_f - freezing_c).abs() < 1e-9,
        "32 degF == 0 degC == 273.15 K (C13): {freezing_f} vs {freezing_c}"
    );
}

#[test]
fn affine_plus_affine_is_refused() {
    let table = seed();
    let left = absolute(&table, "degC", 22.0);
    let right = absolute(&table, "degC", 10.0);
    let error = add(&left, &right).expect_err("cannot add absolute temperatures");
    assert_eq!(error.code, "E-UNIT-AFFINE-1", "{:?}", error);
    assert!(
        error.message.contains("difference"),
        "refusal must suggest a difference or mixture average: {:?}",
        error
    );
}

#[test]
fn affine_plus_difference_is_admitted() {
    let table = seed();
    let point = absolute(&table, "degC", 22.0);
    let delta = difference_of(&table, "degC", 10.0);
    let sum = add(&point, &delta).expect("affine + difference = affine");
    assert_eq!(sum.kind, QuantityKind::Absolute);
    assert_eq!(sum.unit.name, "degC");
    assert!((sum.value - 32.0).abs() < 1e-9, "22 degC + 10 ΔdegC = 32 degC, got {}", sum.value);

    // A K difference is the same delta scale as ΔdegC (both scale 1).
    let kelvin_delta = Quantity {
        value: 10.0,
        unit: table.resolve("K").expect("K"),
        kind: QuantityKind::Difference,
    };
    let sum_k = add(&point, &kelvin_delta).expect("affine + K difference");
    assert!((sum_k.value - 32.0).abs() < 1e-9);
}

#[test]
fn affine_subtraction_yields_difference_quantity() {
    let table = seed();
    let left = absolute(&table, "degC", 22.0);
    let right = absolute(&table, "degC", 10.0);
    let delta = sub(&left, &right).expect("affine - affine = difference");
    assert_eq!(
        delta.kind,
        QuantityKind::Difference,
        "T - T_room must be a difference, not an absolute point"
    );
    assert_eq!(delta.unit.name, "ΔdegC");
    assert!((delta.value - 12.0).abs() < 1e-9);
    // ΔdegC is multiplicative and equal in scale to K.
    assert_eq!(delta.unit.scale, 1.0);
    assert_eq!(delta.unit.dims, table.resolve("K").expect("K").dims);
}

#[test]
fn affine_multiplication_is_refused() {
    let table = seed();
    let left = absolute(&table, "degC", 22.0);
    let right = absolute(&table, "degC", 10.0);
    let error = mul(&left, &right).expect_err("affine * affine is meaningless");
    assert_eq!(error.code, "E-UNIT-AFFINE-2", "{:?}", error);
}

#[test]
fn offset_order_negative_control() {
    // The C13 bug: post-scale encoding (v*scale + offset) for degF.
    let scale = 5.0_f64 / 9.0;
    let offset = 459.67_f64;
    let post_scale = 32.0 * scale + offset;
    let pre_scale = (32.0 + offset) * scale;
    assert!(
        (pre_scale - 273.15).abs() < 1e-9,
        "pre-scale must match the conformance point"
    );
    assert!(
        (post_scale - pre_scale).abs() > 200.0,
        "post-scale must differ by ~255.37 K; got {post_scale} vs {pre_scale}"
    );
}

#[test]
fn linear_difference_arithmetic_still_works() {
    let table = seed();
    let kelvin_delta_a = Quantity {
        value: 10.0,
        unit: table.resolve("K").expect("K"),
        kind: QuantityKind::Difference,
    };
    let kelvin_delta_b = Quantity {
        value: 4.0,
        unit: table.resolve("K").expect("K"),
        kind: QuantityKind::Difference,
    };
    let sum = add(&kelvin_delta_a, &kelvin_delta_b).expect("difference + difference");
    assert_eq!(sum.kind, QuantityKind::Difference);
    assert!((sum.value - 14.0).abs() < 1e-9);
}
