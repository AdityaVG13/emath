use std::path::PathBuf;

use emath_exec_ir::interp::{evaluate_with_budget, EvalFault, Value};
use emath_exec_ir::language_image::load_language_distribution;
use emath_exec_ir::native_kernel::install_language_distribution;
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_test_harness::Probe;

fn seam_eval(capability: &str, inputs: &[Value]) -> Result<Value, EvalFault> {
    let count = inputs.len();
    let mut ops: Vec<_> = (0..count)
        .map(|index| (EmirOp::LoadInput(index as u16), Default::default()))
        .collect();
    ops.push((
        EmirOp::ApplyCapability {
            capability: capability.to_string(),
            class: CellClass::Pure,
            args: (0..count as u32).map(EmirValue).collect(),
        },
        Default::default(),
    ));
    evaluate_with_budget(
        &EmirProgram { ops, result: EmirValue(count as u32), input_count: count as u16, state_count: 0, domain_obligations: Vec::new() },
        inputs,
        &[],
        EvalBudget::default(),
    )
}

fn distribution_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../language")
}

#[test]
fn probe() {
    let mut probe = Probe::new("domain_science_capsule_cutover.rs: every check in one probe");
    let distribution = load_language_distribution(&distribution_root()).expect("load authored language image");
    install_language_distribution(&distribution).expect("install capsule-active bindings");
    probe.case("kernels_preserve_shape_domain_and_finiteness_diagnostics", |probe| {


        let inner = |args: &[Value]| seam_eval("std.capability.geometry.inner-product", args);
        probe.eq("(inner)(&[ Value::Vector(vec![1.0, 2.0, 3.0]), Value::Vector(vec![4.0, ...", &((inner)(&[
                Value::Vector(vec![1.0, 2.0, 3.0]),
                Value::Vector(vec![4.0, 5.0, 6.0]),
            ])), &(Ok(Value::F64(32.0))));
        let shape = (inner)(&[Value::Vector(vec![1.0]), Value::Vector(vec![1.0, 2.0])])
            .expect_err("zip truncation must refuse");
        let EvalFault::CarrierRefused { detail: shape_code, .. } = shape else { panic!("expected typed refusal"); };
        probe.eq("shape_code", &(shape_code), &("E-SHAPE-001".to_string()));

        let dimensions = |args: &[Value]| seam_eval("std.capability.units.dimension-compose", args);
        probe.eq("(dimensions)(&[ Value::Vector(vec![1.0, 0.0, -1.0]), Value::Vector(vec!...", &((dimensions)(&[
                Value::Vector(vec![1.0, 0.0, -1.0]),
                Value::Vector(vec![-1.0, 0.0, 1.0]),
            ])), &(Ok(Value::Vector(vec![0.0, 0.0, 0.0]))));
        let fractional = (dimensions)(&[Value::Vector(vec![0.5]), Value::Vector(vec![1.0])])
            .expect_err("dimension exponents stay integral");
        let EvalFault::CarrierRefused { detail: fractional_code, .. } = fractional else { panic!("expected typed refusal"); };
        probe.eq("fractional_code", &(fractional_code), &("E-UNIT-001".to_string()));

        let residual = |args: &[Value]| seam_eval("std.capability.chemistry.conservation-residual", args);
        probe.eq("(residual)(&[ Value::Matrix { rows: 2, cols: 3, data: vec![2.0, 0.0, 2....", &((residual)(&[
                Value::Matrix {
                    rows: 2,
                    cols: 3,
                    data: vec![2.0, 0.0, 2.0, 0.0, 2.0, 1.0],
                },
                Value::Vector(vec![2.0, 1.0, -2.0]),
            ])), &(Ok(Value::Vector(vec![0.0, 0.0]))));
        let malformed = (residual)(&[
            Value::Matrix {
                rows: 1,
                cols: 2,
                data: vec![1.0],
            },
            Value::Vector(vec![1.0, 1.0]),
        ])
        .expect_err("malformed matrix storage refuses");
        let EvalFault::CarrierRefused { detail: malformed_code, .. } = malformed else { panic!("expected typed refusal"); };
        probe.eq("malformed_code", &(malformed_code), &("E-SHAPE-001".to_string()));

        let affine = |args: &[Value]| seam_eval("std.capability.units.affine-scale", args);
        let non_finite =
            (affine)(&[Value::F64(f64::INFINITY), Value::F64(1.0), Value::F64(0.0)])
                .expect_err("non-finite quantities refuse");
        let EvalFault::CarrierRefused { detail: non_finite_code, .. } = non_finite else { panic!("expected typed refusal"); };
        probe.eq("non_finite_code", &(non_finite_code), &("E-CELL-006".to_string()));
    });
    probe.case("dimensional_analysis_kernels_compute_law_grade_algebra", |probe| {


        let negate = |args: &[Value]| seam_eval("std.capability.units.dimension-negate", args);
        probe.eq("(negate)(&[Value::Vector(vec![1.0, 0.0, -2.0])])", &((negate)(&[Value::Vector(vec![1.0, 0.0, -2.0])])), &(Ok(Value::Vector(vec![-1.0, 0.0, 2.0]))));
        let fractional = (negate)(&[Value::Vector(vec![0.5])])
            .expect_err("dimension exponents stay integral");
        let EvalFault::CarrierRefused { detail: fractional_code, .. } = fractional else { panic!("expected typed refusal"); };
        probe.eq("fractional_code", &(fractional_code), &("E-UNIT-001".to_string()));

        let power = |args: &[Value]| seam_eval("std.capability.units.dimension-power", args);
        probe.eq("(power)(&[Value::Vector(vec![1.0, 0.0, -1.0]), Value::I64(2)])", &((power)(&[Value::Vector(vec![1.0, 0.0, -1.0]), Value::I64(2)])), &(Ok(Value::Vector(vec![2.0, 0.0, -2.0]))));
        let negative = (power)(&[Value::Vector(vec![0.0, 1.0]), Value::I64(-1)]).unwrap();
        probe.eq("negative", &(negative), &(Value::Vector(vec![0.0, -1.0])));

        let witness = |args: &[Value]| seam_eval("std.capability.units.homogeneity-check", args);
        probe.eq("(witness)(&[ Value::Vector(vec![1.0, 1.0, -2.0]), Value::Vector(vec![1....", &((witness)(&[
                Value::Vector(vec![1.0, 1.0, -2.0]),
                Value::Vector(vec![1.0, 1.0, -2.0]),
            ])), &(Ok(Value::Vector(vec![1.0, 1.0, -2.0]))));
        let inhomogeneous = (witness)(&[
            Value::Vector(vec![1.0, 0.0, 0.0]),
            Value::Vector(vec![0.0, 0.0, 1.0]),
        ])
        .expect_err("inhomogeneous dimensions refuse");
        let EvalFault::CarrierRefused { detail: inhomogeneous_code, .. } = inhomogeneous else { panic!("expected typed refusal"); };
        probe.eq("inhomogeneous_code", &(inhomogeneous_code), &("E-UNIT-001".to_string()));

        // length, time, and speed (= length/time) span rank 2.
        let rank = |args: &[Value]| seam_eval("std.capability.units.dimension-rank", args);
        probe.eq("(rank)(&[Value::Matrix { rows: 3, cols: 7, data: vec![ 1.0, 0.0, 0.0, 0...", &((rank)(&[Value::Matrix {
                rows: 3,
                cols: 7,
                data: vec![
                    1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, // length
                    0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, // time
                    1.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, // speed
                ],
            }])), &(Ok(Value::I64(2))));

        // Pendulum variables (L, g, T, m) admit exactly one dimensionless
        // group: T^2 * g / L, sign-canonical with first nonzero positive.
        let groups = |args: &[Value]| seam_eval("std.capability.units.dimensionless-groups", args);
        probe.eq("(groups)(&[Value::Matrix { rows: 4, cols: 7, data: vec![ 1.0, 0.0, 0.0,...", &((groups)(&[Value::Matrix {
                rows: 4,
                cols: 7,
                data: vec![
                    1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, // L
                    1.0, 0.0, -2.0, 0.0, 0.0, 0.0, 0.0, // g
                    0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, // T
                    0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, // m
                ],
            }])), &(Ok(Value::Matrix {
                rows: 1,
                cols: 4,
                data: vec![1.0, -1.0, -2.0, 0.0],
            })));
        let fractional_matrix = (groups)(&[Value::Matrix {
            rows: 1,
            cols: 2,
            data: vec![0.5, 1.0],
        }])
        .expect_err("dimension exponents stay integral");
        let EvalFault::CarrierRefused { detail: fractional_matrix_code, .. } = fractional_matrix else { panic!("expected typed refusal"); };
        probe.eq("fractional_matrix_code", &(fractional_matrix_code), &("E-UNIT-001".to_string()));
    });
    probe.case("sigfig_kernels_implement_the_documented_display_contract", |probe| {


        let round = |args: &[Value]| seam_eval("std.capability.precision.sigfig-round", args);
        probe.eq("(round)(&[Value::F64(1.2345), Value::I64(3)])", &((round)(&[Value::F64(1.2345), Value::I64(3)])), &(Ok(Value::F64(1.23))));
        probe.eq("(round)(&[Value::F64(0.0), Value::I64(3)])", &((round)(&[Value::F64(0.0), Value::I64(3)])), &(Ok(Value::F64(0.0))));
        probe.eq("(round)(&[Value::F64(1230.0), Value::I64(0)])", &((round)(&[Value::F64(1230.0), Value::I64(0)])), &(Ok(Value::F64(1230.0))));
        let negative_sf =
            (round)(&[Value::F64(1.0), Value::I64(-1)]).expect_err("negative sf refuses");
        let EvalFault::CarrierRefused { detail: negative_sf_code, .. } = negative_sf else { panic!("expected typed refusal"); };
        probe.eq("negative_sf_code", &(negative_sf_code), &("E-PRECISION-001".to_string()));

        let count = |args: &[Value]| seam_eval("std.capability.precision.sigfig-count", args);
        for (literal, expected) in [
            ("0.0012", 2),
            ("1.230", 4),
            ("1230", 3),
            ("1000.", 4),
            ("-2.50e3", 3),
        ] {
            probe.eq("(count)(&[Value::Text(literal.to_string())])", &((count)(&[Value::Text(literal.to_string())])), &(Ok(Value::I64(expected))));
        }
        let no_precision = (count)(&[Value::Text("0.0".to_string())])
            .expect_err("zero carries no precision information");
        let EvalFault::CarrierRefused { detail: no_precision_code, .. } = no_precision else { panic!("expected typed refusal"); };
        probe.eq("no_precision_code", &(no_precision_code), &("E-PRECISION-001".to_string()));
    });
    probe.case("named_units_resolve_from_capsule_catalog_data", |probe| {

        let capsule = distribution
            .capsules
            .iter()
            .find(|capsule| capsule.feature_id.as_str() == "std.capability.units.catalog")
            .expect("unit catalog capsule exists");
        probe.eq("distribution .authority .entries .get(&capsule.feature_id) .map(|entry| entry.s...", &(distribution
                .authority
                .entries
                .get(&capsule.feature_id)
                .map(|entry| entry.state.as_str())), &(Some("capsule-active")));
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
            probe.eq("dims.len()", &(dims.len()), &(7));
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
            probe.eq("unit.dimensions()", &(unit.dimensions()), &(expected_dims));
            probe.eq("unit.scale.to_bits()", &(unit.scale.to_bits()), &(scale.to_bits()));
            probe.eq("unit.offset.to_bits()", &(unit.offset.to_bits()), &(offset.to_bits()));
            let expected_family = match family {
                "si" => emath_ir::UnitFamily::Si,
                "info" => emath_ir::UnitFamily::Information,
                other => panic!("unknown family {other}"),
            };
            probe.eq("unit.family", &(unit.family), &(expected_family));
        }
        for alias in subfield("aliases=").split('|') {
            let (alias, canonical) = alias.split_once('>').expect("alias has a target");
            probe.eq("emath_ir::lookup_unit(alias).unwrap().identity()", &(emath_ir::lookup_unit(alias).unwrap().identity()), &(emath_ir::lookup_unit(canonical).unwrap().identity()));
        }
        for refused in subfield("refusals=").split('|') {
            let error = emath_ir::lookup_unit(refused).unwrap_err();
            probe.eq("error.code", &(error.code), &(emath_ir::E_UNIT_CURRENCY_CORE));
        }
        let unknown = emath_ir::lookup_unit("furlong").unwrap_err();
        probe.eq("unknown.code", &(unknown.code), &("E-UNIT-104"));
    });
    probe.case("orientation_and_noncommutativity_are_not_erased", |probe| {


        let cross = |args: &[Value]| seam_eval("std.capability.geometry.cross-3", args);
        probe.eq("(cross)(&[ Value::Vector(vec![1.0, 0.0, 0.0]), Value::Vector(vec![0.0, ...", &((cross)(&[
                Value::Vector(vec![1.0, 0.0, 0.0]),
                Value::Vector(vec![0.0, 1.0, 0.0]),
            ])), &(Ok(Value::Vector(vec![0.0, 0.0, 1.0]))));

        let product = |args: &[Value]| seam_eval("std.capability.geometry.quaternion-product", args);
        let i = Value::Vector(vec![0.0, 1.0, 0.0, 0.0]);
        let j = Value::Vector(vec![0.0, 0.0, 1.0, 0.0]);
        probe.eq("(product)(&[i.clone(), j.clone()])", &((product)(&[i.clone(), j.clone()])), &(Ok(Value::Vector(vec![0.0, 0.0, 0.0, 1.0]))));
        probe.eq("(product)(&[j, i])", &((product)(&[j, i])), &(Ok(Value::Vector(vec![0.0, 0.0, 0.0, -1.0]))));
    });
    probe.finish();
}

