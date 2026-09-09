use std::path::Path;

use emath_exec_ir::interp::{evaluate_with_budget, EvalFault, Value};
use emath_exec_ir::language_image::load_language_distribution;
use emath_exec_ir::native_kernel::install_language_distribution;
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_ir::ExprNode;
use emath_sema::CompilerSession;
use emath_test_harness::Probe;



fn seam_eval(capability: &str, inputs: &[Value]) -> Result<Value, EvalFault> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
    let distribution = load_language_distribution(&root).expect("language distribution");
    install_language_distribution(&distribution).expect("active kernels bind on this eval thread");

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
        &EmirProgram {
            ops,
            result: EmirValue(count as u32),
            input_count: count as u16,
            state_count: 0,
            domain_obligations: Vec::new(),
        },
        inputs,
        &[],
        EvalBudget::default(),
    )
}



fn identity_program() -> EmirProgram {
    EmirProgram {
        ops: vec![(EmirOp::LoadInput(0), Default::default())],
        result: EmirValue(0),
        input_count: 1,
        state_count: 0,
        domain_obligations: Vec::new(),
    }
}

#[test]
fn probe() {
    let mut probe = Probe::new("dynamics_capsule_cutover.rs: every check in one probe");
    
    probe.case("active_aliases_lower_to_feature_ids_and_keep_literal_spacing_refusal", |probe| {

        emath_syntax::install_source_parser();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
        let distribution = load_language_distribution(&root).expect("language distribution");
        emath_sema::language::install_language_distribution(&distribution)
            .expect("install active language bindings");

        let mut session = CompilerSession::new(emath_core::limits::Limits::default());
        let literal = session.check_owned(
            "LiteralStencil.emath",
            "emath function LiteralStencil:\n    inputs:\n        witness: Float64\n    outputs:\n        result: Vector[3]\n    definitions:\n        result = laplacian([1.0, 2.0, 4.0], 1.0)\n",
        );
        probe.demand("literal.diagnostics", !literal.diagnostics.has_errors(), format!("{:?}", literal.diagnostics));
        let capability = literal
            .package
            .capabilities
            .iter()
            .position(|capability| capability.name.0 == "std.capability.pde.laplacian")
            .expect("laplacian FeatureID mounted");
        probe.demand("literal.package.exprs.iter().any(|expression| { matches!(expression, ExprNode::...", literal.package.exprs.iter().any(|expression| {
            matches!(expression, ExprNode::Apply { capability: id, arguments } if id.0 as usize == capability && arguments.len() == 2)
        }), "literal.package.exprs.iter().any(|expression| { matches!(expression, ExprNode::...");

        let mut variable_session = CompilerSession::new(emath_core::limits::Limits::default());
        let variable = variable_session.check_owned(
            "VariableStencil.emath",
            "emath function VariableStencil:\n    inputs:\n        dx: Float64\n    outputs:\n        result: Vector[3]\n    definitions:\n        result = laplacian([1.0, 2.0, 4.0], dx)\n",
        );
        probe.demand("\"runtime spacing must retain the legacy literal-only refusal\"", variable.diagnostics.has_errors(), "runtime spacing must retain the legacy literal-only refusal");
    });
    probe.case("control_calls_preserve_values_and_typed_refusals_through_the_seam", |probe| {

        probe.eq("seam_eval( \"std.capability.control.transfer-eval\", &[ Value::Vector(vec![6.0, 1...", &(seam_eval(
                "std.capability.control.transfer-eval",
                &[
                    Value::Vector(vec![6.0, 1.0]),
                    Value::Vector(vec![2.0, 3.0, 1.0]),
                    Value::F64(0.0),
                ],
            )), &(Ok(Value::F64(3.0))));
        probe.demand("\"a pole hit remains a refusal\"", seam_eval(
                "std.capability.control.transfer-eval",
                &[
                    Value::Vector(vec![1.0]),
                    Value::Vector(vec![0.0, 1.0]),
                    Value::F64(0.0),
                ],
            )
            .is_err(), "a pole hit remains a refusal");

        probe.eq("seam_eval( \"std.capability.control.poles-stable\", &[Value::Vector(vec![2.0, 3.0...", &(seam_eval(
                "std.capability.control.poles-stable",
                &[Value::Vector(vec![2.0, 3.0, 1.0])],
            )), &(Ok(Value::Bool(true))));
        probe.eq("seam_eval( \"std.capability.control.dc-gain\", &[ Value::Matrix { rows: 2, cols: ...", &(seam_eval(
                "std.capability.control.dc-gain",
                &[
                    Value::Matrix {
                        rows: 2,
                        cols: 2,
                        data: vec![0.0, 1.0, -2.0, -3.0],
                    },
                    Value::Vector(vec![0.0, 1.0]),
                    Value::Vector(vec![1.0, 0.0]),
                ],
            )), &(Ok(Value::F64(0.5))));
        probe.eq("seam_eval(transfer-eval, [2,1]/[2,3,1] at 1)", &(seam_eval(
                "std.capability.control.transfer-eval",
                &[
                    Value::Vector(vec![2.0, 1.0]),
                    Value::Vector(vec![2.0, 3.0, 1.0]),
                    Value::F64(1.0),
                ],
            )), &(Ok(Value::F64(0.5))));
        probe.demand("pole at -2 refuses", seam_eval(
                "std.capability.control.transfer-eval",
                &[
                    Value::Vector(vec![2.0, 1.0]),
                    Value::Vector(vec![2.0, 3.0, 1.0]),
                    Value::F64(-2.0),
                ],
            )
            .unwrap_err()
            .to_string()
            .contains("E-CONTROL-002"), "pole at -2 refuses");
        probe.eq("seam_eval(dc-gain, stable carrier)", &(seam_eval(
                "std.capability.control.dc-gain",
                &[
                    Value::Matrix {
                        rows: 2,
                        cols: 2,
                        data: vec![-3.0, -2.0, 1.0, 0.0],
                    },
                    Value::Vector(vec![1.0, 0.0]),
                    Value::Vector(vec![1.0, 2.0]),
                ],
            )), &(Ok(Value::F64(1.0))));
        probe.demand("unstable carrier refuses", seam_eval(
                "std.capability.control.dc-gain",
                &[
                    Value::Matrix {
                        rows: 2,
                        cols: 2,
                        data: vec![1.0, 0.0, 0.0, 1.0],
                    },
                    Value::Vector(vec![1.0, 0.0]),
                    Value::Vector(vec![1.0, 2.0]),
                ],
            )
            .unwrap_err()
            .to_string()
            .contains("E-CONTROL-003"), "unstable carrier refuses");
        probe.demand("ragged carrier refuses", seam_eval(
                "std.capability.control.dc-gain",
                &[
                    Value::Matrix {
                        rows: 2,
                        cols: 2,
                        data: vec![1.0, 0.0, 0.0, 1.0, 0.0],
                    },
                    Value::Vector(vec![1.0, 0.0]),
                    Value::Vector(vec![1.0]),
                ],
            )
            .unwrap_err()
            .to_string()
            .contains("E-CONTROL-004"), "ragged carrier refuses");
        probe.eq("seam_eval(poles-stable, linear)", &(seam_eval(
                "std.capability.control.poles-stable",
                &[Value::Vector(vec![1.0, 1.0])],
            )), &(Ok(Value::Bool(true))));
        probe.demand("zero polynomial refuses", seam_eval(
                "std.capability.control.poles-stable",
                &[Value::Vector(vec![0.0, 0.0])],
            )
            .unwrap_err()
            .to_string()
            .contains("E-CONTROL-002"), "zero polynomial refuses");
        probe.demand("marginal table refuses", seam_eval(
                "std.capability.control.poles-stable",
                &[Value::Vector(vec![1.0, 0.0, 1.0])],
            )
            .unwrap_err()
            .to_string()
            .contains("E-CONTROL-005"), "marginal table refuses");
    });
    probe.case("fixed_stencils_execute_from_feature_ids_without_name_dispatch", |probe| {

        let field = Value::Vector(vec![1.0, 2.0, 4.0]);
        probe.eq("seam_eval( \"std.capability.pde.laplacian\", &[field.clone(), Value::F64(1.0)], )", &(seam_eval(
                "std.capability.pde.laplacian",
                &[field.clone(), Value::F64(1.0)],
            )), &(Ok(Value::Vector(vec![1.0, 1.0, -2.0]))));
        probe.eq("seam_eval( \"std.capability.pde.laplacian-neumann\", &[field.clone(), Value::F64(...", &(seam_eval(
                "std.capability.pde.laplacian-neumann",
                &[field.clone(), Value::F64(1.0)],
            )), &(Ok(Value::Vector(vec![2.0, 1.0, -4.0]))));
        probe.eq("seam_eval(\"std.capability.pde.gradient-1d\", &[field, Value::F64(1.0)],)", &(seam_eval("std.capability.pde.gradient-1d", &[field, Value::F64(1.0)],)), &(Ok(Value::Vector(vec![1.0, 1.5, 2.0]))));
        probe.demand("\"nonpositive spacing remains a typed refusal\"", seam_eval(
                "std.capability.pde.laplacian",
                &[Value::Vector(vec![1.0]), Value::F64(0.0)],
            )
            .is_err(), "nonpositive spacing remains a typed refusal");

        let vx = Value::Matrix {
            rows: 2,
            cols: 2,
            data: vec![0.0, 1.0, 0.0, 1.0],
        };
        let vy = Value::Matrix {
            rows: 2,
            cols: 2,
            data: vec![0.0, 0.0, 1.0, 1.0],
        };
        probe.eq("seam_eval( \"std.capability.pde.divergence-2d\", &[vx, vy, Value::F64(1.0)], )", &(seam_eval(
                "std.capability.pde.divergence-2d",
                &[vx, vy, Value::F64(1.0)],
            )), &(Ok(Value::Matrix {
                rows: 2,
                cols: 2,
                data: vec![2.0; 4],
            })));

        let tensor = Value::Tensor {
            shape: vec![2, 2, 2],
            data: (0..8).map(f64::from).collect(),
        };
        let mut identity_weights = vec![0.0; 27];
        identity_weights[13] = 1.0;
        probe.eq("seam_eval( \"std.capability.pde.stencil-3d-one-sided\", &[ tensor.clone(), Value:...", &(seam_eval(
                "std.capability.pde.stencil-3d-one-sided",
                &[
                    tensor.clone(),
                    Value::Vector(identity_weights),
                    Value::I64(1),
                    Value::I64(1),
                    Value::I64(1),
                ],
            )), &(Ok(tensor)));

        let constant = Value::Tensor {
            shape: vec![2, 2, 2],
            data: vec![1.0; 8],
        };
        probe.eq("seam_eval( \"std.capability.pde.laplacian-3d\", &[constant.clone(), Value::F64(1....", &(seam_eval(
                "std.capability.pde.laplacian-3d",
                &[constant.clone(), Value::F64(1.0)],
            )), &(Ok(Value::Tensor {
                shape: vec![2, 2, 2],
                data: vec![0.0; 8],
            })));
        probe.demand("\"nonpositive spacing remains a typed refusal\"", seam_eval(
                "std.capability.pde.laplacian-3d",
                &[constant, Value::F64(0.0)],
            )
            .is_err(), "nonpositive spacing remains a typed refusal");
    });
    
    probe.case("program_callback_step_and_goals_execute", |probe| {

        let program = Value::program(identity_program());
        probe.eq("seam_eval( \"std.capability.dynamics.simulation-world\", &[ program.clone(), Valu...", &(seam_eval(
                "std.capability.dynamics.simulation-world",
                &[
                    program.clone(),
                    Value::Vector(vec![1.0]),
                    Value::F64(1.0),
                    Value::Text("euler".into()),
                ],
            )), &(Ok(Value::Vector(vec![2.0]))));
        let Value::Vector(rk4) = seam_eval(
            "std.capability.dynamics.simulation-world",
            &[
                program.clone(),
                Value::Vector(vec![1.0]),
                Value::F64(1.0),
                Value::Text("rk4".into()),
            ],
        )
        .expect("rk4 identity rate") else {
            panic!("rk4 returns a state vector")
        };
        probe.demand("(rk4[0] - (1.0 + 10.25 / 6.0)).abs() < 1e-12", (rk4[0] - (1.0 + 10.25 / 6.0)).abs() < 1e-12, "(rk4[0] - (1.0 + 10.25 / 6.0)).abs() < 1e-12");
        probe.demand("seam_eval( \"std.capability.dynamics.simulation-world\", &[ program.clone(), Valu...", seam_eval(
                "std.capability.dynamics.simulation-world",
                &[
                    program.clone(),
                    Value::Vector(vec![1.0]),
                    Value::F64(1.0),
                    Value::Text("rk45".into()),
                ],
            )
            .unwrap_err()
            .to_string()
            .contains("E-TYPE-012"), "seam_eval( \"std.capability.dynamics.simulation-world\", &[ program.clone(), Valu...");

        probe.eq("seam_eval( \"std.capability.calculus.goals\", &[ program, Value::Vector(vec![3.0]...", &(seam_eval(
                "std.capability.calculus.goals",
                &[
                    program,
                    Value::Vector(vec![3.0]),
                    Value::Text("differentiate".into()),
                    Value::Set(vec![Value::I64(0)]),
                ],
            )), &(Ok(Value::F64(1.0))));
    });
    probe.finish();
}


#[test]
fn authored_polynomial_and_spectral_equations() {
    for (rate, y0, h, expected) in [
        (vec![1.0], 0.0, 0.1, 0.1),
        (vec![0.0, 1.0], 1.0, 0.1, 1.0 / 0.9),
        (vec![0.0, 0.0, 1.0], 1.0, 0.1, (1.0 - 0.6_f64.sqrt()) / 0.2),
        (vec![], 7.0, 0.1, 7.0),
    ] {
        let actual = seam_eval(
            "std.capability.ode.backward-euler",
            &[Value::Vector(rate), Value::F64(y0), Value::F64(h)],
        )
        .expect("backward Euler has a finite implicit solution");
        let Value::F64(actual) = actual else {
            panic!("scalar solution expected")
        };
        assert!((actual - expected).abs() < 1e-11, "{actual} != {expected}");
    }
    for (load, expected) in [(1.0, 0.125), (2.0, 0.25)] {
        let actual = seam_eval(
            "std.capability.pde.poisson-sine",
            &[Value::Vector(vec![load])],
        )
        .expect("one-point Dirichlet solve");
        let Value::Vector(actual) = actual else {
            panic!("vector solution expected")
        };
        assert!(
            (actual[0] - expected).abs() < 1e-14,
            "{actual:?} != {expected}"
        );
    }
}

#[test]
fn authored_rk45_step_preserves_the_step_floor() {
    assert_eq!(
        seam_eval(
            "std.capability.dynamics.model-rk45-grow",
            &[
                Value::F64(0.1),
                Value::F64(1.0),
                Value::F64(0.0),
                Value::Bool(false),
                Value::F64(10.0),
            ]
        ),
        Ok(Value::F64(0.1))
    );
}
