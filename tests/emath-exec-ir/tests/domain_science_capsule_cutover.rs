use std::path::PathBuf;

use emath_exec_ir::interp::Value;
use emath_exec_ir::language_image::load_language_distribution;
use emath_exec_ir::native_kernel::{
    binding_semantic_hash, install_language_distribution, native_kernel,
};

fn distribution_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../language")
}

#[test]
fn domain_science_feature_ids_bind_capsule_semantics() {
    let distribution =
        load_language_distribution(&distribution_root()).expect("load authored language image");
    install_language_distribution(&distribution).expect("install capsule-active kernel bindings");

    for (feature_id, kernel_id) in [
        (
            "std.capability.geometry.inner-product",
            "pairwise-sum-products",
        ),
        ("std.capability.geometry.cross-3", "alternating-product-3"),
        (
            "std.capability.geometry.quaternion-product",
            "bilinear-product-4",
        ),
        (
            "std.capability.units.dimension-compose",
            "componentwise-integer-add",
        ),
        ("std.capability.units.affine-scale", "affine-map"),
        (
            "std.capability.chemistry.conservation-residual",
            "rectangular-linear-residual",
        ),
        (
            "std.capability.units.dimension-negate",
            "componentwise-integer-negate",
        ),
        (
            "std.capability.units.dimension-power",
            "componentwise-integer-scale",
        ),
        (
            "std.capability.units.homogeneity-check",
            "integer-vector-witness",
        ),
        ("std.capability.units.dimension-rank", "integer-row-rank"),
        (
            "std.capability.units.dimensionless-groups",
            "integer-nullspace-basis",
        ),
        (
            "std.capability.precision.sigfig-round",
            "decimal-significance-round",
        ),
        (
            "std.capability.precision.sigfig-count",
            "decimal-significance-count",
        ),
    ] {
        let capsule = distribution
            .capsules
            .iter()
            .find(|capsule| capsule.feature_id.as_str() == feature_id)
            .expect("domain-science capsule exists");
        let kernel = native_kernel(feature_id).expect("FeatureID resolves a native kernel");
        assert_eq!(kernel.kernel_id, kernel_id);
        assert_eq!(
            binding_semantic_hash(feature_id).as_deref(),
            Some(capsule.semantic_hash.as_str()),
            "binding authority is the capsule hash, never an alias"
        );
    }
}

#[test]
fn kernels_preserve_shape_domain_and_finiteness_diagnostics() {
    let distribution =
        load_language_distribution(&distribution_root()).expect("load authored language image");
    install_language_distribution(&distribution).expect("install capsule-active kernel bindings");

    let inner = native_kernel("std.capability.geometry.inner-product").unwrap();
    assert_eq!(
        (inner.handler)(&[
            Value::Vector(vec![1.0, 2.0, 3.0]),
            Value::Vector(vec![4.0, 5.0, 6.0]),
        ]),
        Ok(Value::F64(32.0))
    );
    let shape = (inner.handler)(&[Value::Vector(vec![1.0]), Value::Vector(vec![1.0, 2.0])])
        .expect_err("zip truncation must refuse");
    assert!(shape.starts_with("E-SHAPE-001:"), "{shape}");

    let dimensions = native_kernel("std.capability.units.dimension-compose").unwrap();
    assert_eq!(
        (dimensions.handler)(&[
            Value::Vector(vec![1.0, 0.0, -1.0]),
            Value::Vector(vec![-1.0, 0.0, 1.0]),
        ]),
        Ok(Value::Vector(vec![0.0, 0.0, 0.0]))
    );
    let fractional = (dimensions.handler)(&[Value::Vector(vec![0.5]), Value::Vector(vec![1.0])])
        .expect_err("dimension exponents stay integral");
    assert!(fractional.starts_with("E-UNIT-001:"), "{fractional}");

    let residual = native_kernel("std.capability.chemistry.conservation-residual").unwrap();
    assert_eq!(
        (residual.handler)(&[
            Value::Matrix {
                rows: 2,
                cols: 3,
                data: vec![2.0, 0.0, 2.0, 0.0, 2.0, 1.0],
            },
            Value::Vector(vec![2.0, 1.0, -2.0]),
        ]),
        Ok(Value::Vector(vec![0.0, 0.0]))
    );
    let malformed = (residual.handler)(&[
        Value::Matrix {
            rows: 1,
            cols: 2,
            data: vec![1.0],
        },
        Value::Vector(vec![1.0, 1.0]),
    ])
    .expect_err("malformed matrix storage refuses");
    assert!(malformed.starts_with("E-SHAPE-001:"), "{malformed}");

    let affine = native_kernel("std.capability.units.affine-scale").unwrap();
    let non_finite =
        (affine.handler)(&[Value::F64(f64::INFINITY), Value::F64(1.0), Value::F64(0.0)])
            .expect_err("non-finite quantities refuse");
    assert!(non_finite.starts_with("E-CELL-006:"), "{non_finite}");
}

#[test]
fn dimensional_analysis_kernels_compute_law_grade_algebra() {
    let distribution =
        load_language_distribution(&distribution_root()).expect("load authored language image");
    install_language_distribution(&distribution).expect("install capsule-active kernel bindings");

    let negate = native_kernel("std.capability.units.dimension-negate").unwrap();
    assert_eq!(
        (negate.handler)(&[Value::Vector(vec![1.0, 0.0, -2.0])]),
        Ok(Value::Vector(vec![-1.0, 0.0, 2.0]))
    );
    let fractional = (negate.handler)(&[Value::Vector(vec![0.5])])
        .expect_err("dimension exponents stay integral");
    assert!(fractional.starts_with("E-UNIT-001:"), "{fractional}");

    let power = native_kernel("std.capability.units.dimension-power").unwrap();
    assert_eq!(
        (power.handler)(&[Value::Vector(vec![1.0, 0.0, -1.0]), Value::I64(2)]),
        Ok(Value::Vector(vec![2.0, 0.0, -2.0]))
    );
    let negative = (power.handler)(&[Value::Vector(vec![0.0, 1.0]), Value::I64(-1)]).unwrap();
    assert_eq!(negative, Value::Vector(vec![0.0, -1.0]));

    let witness = native_kernel("std.capability.units.homogeneity-check").unwrap();
    assert_eq!(
        (witness.handler)(&[
            Value::Vector(vec![1.0, 1.0, -2.0]),
            Value::Vector(vec![1.0, 1.0, -2.0]),
        ]),
        Ok(Value::Vector(vec![1.0, 1.0, -2.0]))
    );
    let inhomogeneous = (witness.handler)(&[
        Value::Vector(vec![1.0, 0.0, 0.0]),
        Value::Vector(vec![0.0, 0.0, 1.0]),
    ])
    .expect_err("inhomogeneous dimensions refuse");
    assert!(inhomogeneous.starts_with("E-UNIT-001:"), "{inhomogeneous}");

    // length, time, and speed (= length/time) span rank 2.
    let rank = native_kernel("std.capability.units.dimension-rank").unwrap();
    assert_eq!(
        (rank.handler)(&[Value::Matrix {
            rows: 3,
            cols: 7,
            data: vec![
                1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, // length
                0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, // time
                1.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, // speed
            ],
        }]),
        Ok(Value::I64(2))
    );

    // Pendulum variables (L, g, T, m) admit exactly one dimensionless
    // group: T^2 * g / L, sign-canonical with first nonzero positive.
    let groups = native_kernel("std.capability.units.dimensionless-groups").unwrap();
    assert_eq!(
        (groups.handler)(&[Value::Matrix {
            rows: 4,
            cols: 7,
            data: vec![
                1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, // L
                1.0, 0.0, -2.0, 0.0, 0.0, 0.0, 0.0, // g
                0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, // T
                0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, // m
            ],
        }]),
        Ok(Value::Matrix {
            rows: 1,
            cols: 4,
            data: vec![1.0, -1.0, -2.0, 0.0],
        })
    );
    let fractional_matrix = (groups.handler)(&[Value::Matrix {
        rows: 1,
        cols: 2,
        data: vec![0.5, 1.0],
    }])
    .expect_err("dimension exponents stay integral");
    assert!(fractional_matrix.starts_with("E-UNIT-001:"), "{fractional_matrix}");
}

#[test]
fn sigfig_kernels_implement_the_documented_display_contract() {
    let distribution =
        load_language_distribution(&distribution_root()).expect("load authored language image");
    install_language_distribution(&distribution).expect("install capsule-active kernel bindings");

    let round = native_kernel("std.capability.precision.sigfig-round").unwrap();
    assert_eq!(
        (round.handler)(&[Value::F64(1.2345), Value::I64(3)]),
        Ok(Value::F64(1.23))
    );
    assert_eq!(
        (round.handler)(&[Value::F64(0.0), Value::I64(3)]),
        Ok(Value::F64(0.0))
    );
    assert_eq!(
        (round.handler)(&[Value::F64(1230.0), Value::I64(0)]),
        Ok(Value::F64(1230.0)),
        "sf count 0 leaves the value unchanged"
    );
    let negative_sf =
        (round.handler)(&[Value::F64(1.0), Value::I64(-1)]).expect_err("negative sf refuses");
    assert!(negative_sf.starts_with("E-PRECISION-001:"), "{negative_sf}");

    let count = native_kernel("std.capability.precision.sigfig-count").unwrap();
    for (literal, expected) in [
        ("0.0012", 2),
        ("1.230", 4),
        ("1230", 3),
        ("1000.", 4),
        ("-2.50e3", 3),
    ] {
        assert_eq!(
            (count.handler)(&[Value::Text(literal.to_string())]),
            Ok(Value::I64(expected)),
            "literal {literal}"
        );
    }
    let no_precision = (count.handler)(&[Value::Text("0.0".to_string())])
        .expect_err("zero carries no precision information");
    assert!(no_precision.starts_with("E-PRECISION-001:"), "{no_precision}");
}

#[test]
fn named_units_resolve_from_capsule_catalog_data() {
    let distribution =
        load_language_distribution(&distribution_root()).expect("load authored language image");
    let capsule = distribution
        .capsules
        .iter()
        .find(|capsule| capsule.feature_id.as_str() == "std.capability.units.catalog")
        .expect("unit catalog capsule exists");
    assert_eq!(
        distribution
            .authority
            .entries
            .get(&capsule.feature_id)
            .map(|entry| entry.state.as_str()),
        Some("capsule-active"),
        "the named unit catalog is capsule-active authority"
    );
    let emath_ir::CapsuleSlot::Value(semantics) = &capsule.slots["semantics"] else {
        panic!("semantics slot is a value");
    };
    let subfield = |key: &str| {
        semantics
            .split(';')
            .find_map(|part| part.trim().strip_prefix(key))
            .unwrap_or_else(|| panic!("semantics has {key}"))
    };
    // Independent re-parse of the capsule data: every declared named unit
    // must resolve through `emath_ir::lookup_unit` with bit-identical
    // dims, scale, offset, and family. A hardcoded Rust drift fails here.
    for entry in subfield("catalog=").split('|') {
        let mut fields = entry.split('~');
        let name = fields.next().expect("entry has a name");
        let dims: Vec<i64> = fields
            .next()
            .expect("entry has dims")
            .split(',')
            .map(|exponent| exponent.parse().expect("integer exponent"))
            .collect();
        assert_eq!(dims.len(), 7, "seven base dimensions for {name}");
        let scale: f64 = fields.next().expect("entry has scale").parse().expect("scale parses");
        let offset: f64 = fields
            .next()
            .expect("entry has offset")
            .parse()
            .expect("offset parses");
        let family = fields.next().expect("entry has family");
        let unit = emath_ir::lookup_unit(name).expect("capsule-declared unit resolves");
        let expected_dims = emath_ir::UnitDim::base(
            dims[0], dims[1], dims[2], dims[3], dims[4], dims[5], dims[6],
        );
        assert_eq!(unit.dimensions(), expected_dims, "dims for {name}");
        assert_eq!(unit.scale.to_bits(), scale.to_bits(), "scale for {name}");
        assert_eq!(unit.offset.to_bits(), offset.to_bits(), "offset for {name}");
        let expected_family = match family {
            "si" => emath_ir::UnitFamily::Si,
            "info" => emath_ir::UnitFamily::Information,
            other => panic!("unknown family {other}"),
        };
        assert_eq!(unit.family, expected_family, "family for {name}");
    }
    for alias in subfield("aliases=").split('|') {
        let (alias, canonical) = alias.split_once('>').expect("alias has a target");
        assert_eq!(
            emath_ir::lookup_unit(alias).unwrap().identity(),
            emath_ir::lookup_unit(canonical).unwrap().identity(),
            "alias {alias} is an identity for {canonical}"
        );
    }
    for refused in subfield("refusals=").split('|') {
        let error = emath_ir::lookup_unit(refused).unwrap_err();
        assert_eq!(
            error.code,
            emath_ir::E_UNIT_CURRENCY_CORE,
            "{refused} keeps its typed policy refusal"
        );
    }
    let unknown = emath_ir::lookup_unit("furlong").unwrap_err();
    assert_eq!(unknown.code, "E-UNIT-104");
}

#[test]
fn orientation_and_noncommutativity_are_not_erased() {
    let distribution =
        load_language_distribution(&distribution_root()).expect("load authored language image");
    install_language_distribution(&distribution).expect("install capsule-active kernel bindings");

    let cross = native_kernel("std.capability.geometry.cross-3").unwrap();
    assert_eq!(
        (cross.handler)(&[
            Value::Vector(vec![1.0, 0.0, 0.0]),
            Value::Vector(vec![0.0, 1.0, 0.0]),
        ]),
        Ok(Value::Vector(vec![0.0, 0.0, 1.0]))
    );

    let product = native_kernel("std.capability.geometry.quaternion-product").unwrap();
    let i = Value::Vector(vec![0.0, 1.0, 0.0, 0.0]);
    let j = Value::Vector(vec![0.0, 0.0, 1.0, 0.0]);
    assert_eq!(
        (product.handler)(&[i.clone(), j.clone()]),
        Ok(Value::Vector(vec![0.0, 0.0, 0.0, 1.0]))
    );
    assert_eq!(
        (product.handler)(&[j, i]),
        Ok(Value::Vector(vec![0.0, 0.0, 0.0, -1.0]))
    );
}
