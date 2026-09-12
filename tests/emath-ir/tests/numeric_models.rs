//! Numeric model matrix, unit catalog, and domain/shape well-formedness.

use emath_ir::{
    Interval, NumericProfile, STRICT_F64_MACHINE_EPS, STRICT_F64_PRECISION_BITS, Shape,
    check_error_limit, check_precision_demand, lookup_unit, numeric_behavior,
    parse_numeric_profile, per_unit,
};
use emath_test_harness::Probe;

#[test]
fn intent() {
    let mut p = Probe::new("Numeric model matrix, unit catalog, and domain/shape well-formedness.");
    p.case("unspecified_numeric_model_defaults_to_strict_f64", |p| {

    p.eq("unspecified_numeric_model_defaults_to_strict_f64#1", parse_numeric_profile("").unwrap(), NumericProfile::StrictF64);
    p.eq("unspecified_numeric_model_defaults_to_strict_f64#2", NumericProfile::default_phase1(), NumericProfile::StrictF64);
    p.eq("unspecified_numeric_model_defaults_to_strict_f64#3", NumericProfile::default(), NumericProfile::StrictF64);

    });
    p.case("explicit_models_are_honored", |p| {

    p.eq("explicit_models_are_honored#1", parse_numeric_profile("strict-f64").unwrap(), NumericProfile::StrictF64);
    p.eq("explicit_models_are_honored#2", parse_numeric_profile("interval-f64").unwrap(), NumericProfile::IntervalF64);
    p.eq("explicit_models_are_honored#3", parse_numeric_profile("Float64").unwrap(), NumericProfile::StrictF64);
    p.eq("explicit_models_are_honored#4", parse_numeric_profile("Interval").unwrap(), NumericProfile::IntervalF64);

    });
    p.case("unknown_numeric_model_is_typed_refusal", |p| {

    let error = parse_numeric_profile("float128").unwrap_err();
    p.demand("unknown_numeric_model_is_typed_refusal#1", error.code == "E-NUM-001", format!("expected {:?}, got {:?}", "E-NUM-001", error.code));

    });
    p.case("per_model_determinism_descriptors_are_stable", |p| {

    let strict = numeric_behavior(NumericProfile::StrictF64);
    p.demand("per_model_determinism_descriptors_are_stable#1", strict.name == "strict-f64", format!("expected {:?}, got {:?}", "strict-f64", strict.name));
    p.demand("per_model_determinism_descriptors_are_stable#2", strict.rounding == "nearest-even", format!("expected {:?}, got {:?}", "nearest-even", strict.rounding));
    p.demand("per_model_determinism_descriptors_are_stable#3", strict.overflow == "error", format!("expected {:?}, got {:?}", "error", strict.overflow));
    p.demand("per_model_determinism_descriptors_are_stable#4", strict.determinism == "ieee754-binary64-round-ties-to-even", format!("expected {:?}, got {:?}", "ieee754-binary64-round-ties-to-even", strict.determinism));
    p.eq("per_model_determinism_descriptors_are_stable#5", strict.max_precision_bits, STRICT_F64_PRECISION_BITS);

    let interval = numeric_behavior(NumericProfile::IntervalF64);
    p.demand("per_model_determinism_descriptors_are_stable#6", interval.name == "interval-f64", format!("expected {:?}, got {:?}", "interval-f64", interval.name));
    p.demand("per_model_determinism_descriptors_are_stable#7", interval.rounding == "outward", format!("expected {:?}, got {:?}", "outward", interval.rounding));
    p.demand("per_model_determinism_descriptors_are_stable#8", interval.determinism == "binary64-endpoint-interval-outward", format!("expected {:?}, got {:?}", "binary64-endpoint-interval-outward", interval.determinism));
    p.eq("per_model_determinism_descriptors_are_stable#9", interval.max_precision_bits, STRICT_F64_PRECISION_BITS);
    p.ne("per_model_determinism_descriptors_are_stable#10", strict.determinism, interval.determinism);

    });
    p.case("precision_demand_no_model_can_honor_is_refused", |p| {

    let error = check_precision_demand(NumericProfile::StrictF64, 128).unwrap_err();
    p.demand("precision_demand_no_model_can_honor_is_refused#1", error.code == "E-NUM-002", format!("expected {:?}, got {:?}", "E-NUM-002", error.code));
    let also = check_precision_demand(NumericProfile::IntervalF64, 0).unwrap_err();
    p.demand("precision_demand_no_model_can_honor_is_refused#2", also.code == "E-NUM-002", format!("expected {:?}, got {:?}", "E-NUM-002", also.code));
    p.demand("precision_demand_no_model_can_honor_is_refused#3", check_precision_demand(NumericProfile::StrictF64, 53).is_ok(), "precision_demand_no_model_can_honor_is_refused#3: check_precision_demand(NumericProfile::StrictF64, 53).is_ok()");

    });
    p.case("error_limit_tighter_than_strict_f64_is_refused", |p| {

    let error = check_error_limit(NumericProfile::StrictF64, 1e-20).unwrap_err();
    p.demand("error_limit_tighter_than_strict_f64_is_refused#1", error.code == "E-NUM-003", format!("expected {:?}, got {:?}", "E-NUM-003", error.code));
    p.demand("error_limit_tighter_than_strict_f64_is_refused#2", check_error_limit(NumericProfile::StrictF64, STRICT_F64_MACHINE_EPS).is_ok(), "error_limit_tighter_than_strict_f64_is_refused#2: check_error_limit(NumericProfile::StrictF64, STRICT_F64_MACHINE_EPS).is_ok()");
    p.demand("error_limit_tighter_than_strict_f64_is_refused#3", check_error_limit(NumericProfile::IntervalF64, 1e-12).is_ok(), "error_limit_tighter_than_strict_f64_is_refused#3: check_error_limit(NumericProfile::IntervalF64, 1e-12).is_ok()");
    let exact = check_error_limit(NumericProfile::IntervalF64, 0.0).unwrap_err();
    p.demand("error_limit_tighter_than_strict_f64_is_refused#4", exact.code == "E-NUM-003", format!("expected {:?}, got {:?}", "E-NUM-003", exact.code));

    });
    p.case("unknown_unit_and_ill_formed_per_are_typed", |p| {

    let unknown = lookup_unit("furlong").unwrap_err();
    p.demand("unknown_unit_and_ill_formed_per_are_typed#1", unknown.code == "E-UNIT-104", format!("expected {:?}, got {:?}", "E-UNIT-104", unknown.code));
    let empty = per_unit("furlong").unwrap_err();
    p.demand("unknown_unit_and_ill_formed_per_are_typed#2", empty.code == "E-UNIT-104", format!("expected {:?}, got {:?}", "E-UNIT-104", empty.code));
    for name in ["Duration", "km", "ms", "MiB", "m", "degC"] {
        let error = lookup_unit(name).unwrap_err();
        p.demand(
            format!("leftover leftover unit `{name}` refuses"),
            error.code == "E-UNIT-104",
            format!("expected E-UNIT-104, got {:?}", error.code),
        );
        let per = per_unit(name).unwrap_err();
        p.demand(
            format!("leftover leftover Per<{name}> refuses"),
            per.code == "E-UNIT-104",
            format!("expected E-UNIT-104, got {:?}", per.code),
        );
    }

    });
    p.case("inverted_interval_and_empty_shape_are_typed", |p| {

    let domain = Interval::checked(5.0, 1.0).unwrap_err();
    p.demand("inverted_interval_and_empty_shape_are_typed#1", domain.code == "E-DOM-002", format!("expected {:?}, got {:?}", "E-DOM-002", domain.code));
    p.demand("inverted_interval_and_empty_shape_are_typed#2", Interval::checked(0.0, 1.0).is_ok(), "inverted_interval_and_empty_shape_are_typed#2: Interval::checked(0.0, 1.0).is_ok()");
    let shape = Shape::declare(vec![]).unwrap_err();
    p.demand("inverted_interval_and_empty_shape_are_typed#3", shape.code == "E-SHAPE-004", format!("expected {:?}, got {:?}", "E-SHAPE-004", shape.code));
    let zero = Shape::declare(vec![emath_ir::Extent::Fixed(0)]).unwrap_err();
    p.demand("inverted_interval_and_empty_shape_are_typed#4", zero.code == "E-SHAPE-004", format!("expected {:?}, got {:?}", "E-SHAPE-004", zero.code));

    });
    p.finish();
}















