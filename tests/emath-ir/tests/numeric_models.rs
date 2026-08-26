//! Numeric model matrix, unit catalog, and domain/shape well-formedness.

use emath_ir::{
    Interval, NumericProfile, STRICT_F64_MACHINE_EPS, STRICT_F64_PRECISION_BITS, Shape,
    check_error_limit, check_precision_demand, lookup_unit, numeric_behavior,
    parse_numeric_profile, per_unit,
};

#[test]
fn unspecified_numeric_model_defaults_to_strict_f64() {
    assert_eq!(
        parse_numeric_profile("").unwrap(),
        NumericProfile::StrictF64
    );
    assert_eq!(NumericProfile::default_phase1(), NumericProfile::StrictF64);
    assert_eq!(NumericProfile::default(), NumericProfile::StrictF64);
}

#[test]
fn explicit_models_are_honored() {
    assert_eq!(
        parse_numeric_profile("strict-f64").unwrap(),
        NumericProfile::StrictF64
    );
    assert_eq!(
        parse_numeric_profile("interval-f64").unwrap(),
        NumericProfile::IntervalF64
    );
    assert_eq!(
        parse_numeric_profile("Float64").unwrap(),
        NumericProfile::StrictF64
    );
    assert_eq!(
        parse_numeric_profile("Interval").unwrap(),
        NumericProfile::IntervalF64
    );
}

#[test]
fn unknown_numeric_model_is_typed_refusal() {
    let error = parse_numeric_profile("float128").unwrap_err();
    assert_eq!(error.code, "E-NUM-001");
}

#[test]
fn per_model_determinism_descriptors_are_stable() {
    let strict = numeric_behavior(NumericProfile::StrictF64);
    assert_eq!(strict.name, "strict-f64");
    assert_eq!(strict.rounding, "nearest-even");
    assert_eq!(strict.overflow, "error");
    assert_eq!(strict.determinism, "ieee754-binary64-round-ties-to-even");
    assert_eq!(strict.max_precision_bits, STRICT_F64_PRECISION_BITS);

    let interval = numeric_behavior(NumericProfile::IntervalF64);
    assert_eq!(interval.name, "interval-f64");
    assert_eq!(interval.rounding, "outward");
    assert_eq!(interval.determinism, "binary64-endpoint-interval-outward");
    assert_eq!(interval.max_precision_bits, STRICT_F64_PRECISION_BITS);
    assert_ne!(strict.determinism, interval.determinism);
}

#[test]
fn precision_demand_no_model_can_honor_is_refused() {
    let error = check_precision_demand(NumericProfile::StrictF64, 128).unwrap_err();
    assert_eq!(error.code, "E-NUM-002");
    let also = check_precision_demand(NumericProfile::IntervalF64, 0).unwrap_err();
    assert_eq!(also.code, "E-NUM-002");
    assert!(check_precision_demand(NumericProfile::StrictF64, 53).is_ok());
}

#[test]
fn error_limit_tighter_than_strict_f64_is_refused() {
    let error = check_error_limit(NumericProfile::StrictF64, 1e-20).unwrap_err();
    assert_eq!(error.code, "E-NUM-003");
    assert!(check_error_limit(NumericProfile::StrictF64, STRICT_F64_MACHINE_EPS).is_ok());
    assert!(check_error_limit(NumericProfile::IntervalF64, 1e-12).is_ok());
    let exact = check_error_limit(NumericProfile::IntervalF64, 0.0).unwrap_err();
    assert_eq!(exact.code, "E-NUM-003");
}

#[test]
fn unknown_unit_and_ill_formed_per_are_typed() {
    let unknown = lookup_unit("furlong").unwrap_err();
    assert_eq!(unknown.code, "E-UNIT-104");
    let empty = per_unit("furlong").unwrap_err();
    assert_eq!(empty.code, "E-UNIT-104");
    assert!(lookup_unit("Duration").is_ok());
    assert!(per_unit("Duration").is_ok());
    let km = lookup_unit("km").expect("km is a known length unit");
    assert_eq!(km.to_si(1.0), 1_000.0);
    let ms = lookup_unit("ms").expect("ms is a known duration unit");
    assert_eq!(ms.to_si(1.0), 1e-3);
    let mib = lookup_unit("MiB").expect("MiB is a known information unit");
    assert_eq!(mib.to_si(1.0), 1_048_576.0);
    let metre = lookup_unit("m").expect("m is a known length unit");
    let area = metre.mul(&metre).expect("m * m is area");
    assert_eq!(area.dims, emath_ir::UnitDim::base(2, 0, 0, 0, 0, 0, 0));
    assert_eq!(area.dims.kind_name(), Some("area"));
    let cancelled = metre.div(&metre).expect("m / m is dimensionless");
    assert!(cancelled.is_dimensionless());
    let celsius = lookup_unit("degC").expect("degC is a known affine temperature");
    assert!(celsius.is_affine());
    assert_eq!(celsius.to_si(0.0), 273.15);
    assert_eq!(celsius.mul(&metre).unwrap_err().code, "E-UNIT-102");
}

#[test]
fn inverted_interval_and_empty_shape_are_typed() {
    let domain = Interval::checked(5.0, 1.0).unwrap_err();
    assert_eq!(domain.code, "E-DOM-002");
    assert!(Interval::checked(0.0, 1.0).is_ok());
    let shape = Shape::declare(vec![]).unwrap_err();
    assert_eq!(shape.code, "E-SHAPE-004");
    let zero = Shape::declare(vec![emath_ir::Extent::Fixed(0)]).unwrap_err();
    assert_eq!(zero.code, "E-SHAPE-004");
}
