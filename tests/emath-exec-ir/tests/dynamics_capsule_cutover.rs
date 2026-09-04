use std::fs;
use std::path::Path;

use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::language_image::load_language_distribution;
use emath_exec_ir::native_kernel::{
    binding_semantic_hash, install_language_distribution, native_kernel,
};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_ir::ExprNode;
use emath_sema::CompilerSession;

const ACTIVE: &[(&str, &str)] = &[
    (
        "std.capability.control.transfer-eval",
        "checked-polynomial-ratio",
    ),
    (
        "std.capability.control.dc-gain",
        "checked-linear-projection",
    ),
    ("std.capability.control.poles-stable", "checked-sign-table"),
    ("std.capability.pde.laplacian", "second-difference-clamp"),
    (
        "std.capability.pde.laplacian-neumann",
        "second-difference-neumann",
    ),
    (
        "std.capability.pde.laplacian-dirichlet",
        "second-difference-dirichlet",
    ),
    (
        "std.capability.pde.gradient-1d",
        "centered-first-difference",
    ),
    (
        "std.capability.pde.divergence-1d",
        "centered-first-difference",
    ),
    ("std.capability.pde.laplacian-2d", "five-point-sum-clamp"),
    (
        "std.capability.pde.laplacian-2d-neumann",
        "five-point-sum-neumann",
    ),
    (
        "std.capability.pde.gradient-2d-x",
        "axis-0-first-difference",
    ),
    (
        "std.capability.pde.gradient-2d-y",
        "axis-1-first-difference",
    ),
    (
        "std.capability.pde.divergence-2d",
        "sum-axis-first-differences",
    ),
    (
        "std.capability.pde.stencil-3d-clamp",
        "checked-stencil-3d-clamp",
    ),
    (
        "std.capability.pde.stencil-3d-neumann",
        "checked-stencil-3d-neumann",
    ),
    (
        "std.capability.pde.stencil-3d-one-sided",
        "checked-stencil-3d-one-sided",
    ),
];

const CANDIDATES: &[&str] = &[
    "std.capability.dynamics.simulation-world",
    "std.capability.pde.tensor-and-divergence",
    "std.capability.calculus.goals",
];

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

#[test]
fn active_inventory_is_hash_pinned_and_value_executable() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
    let distribution = load_language_distribution(&root).expect("valid semantic hashes");
    install_language_distribution(&distribution).expect("all active kernels bind");

    for (feature_id, kernel_id) in ACTIVE {
        let capsule = distribution
            .capsules
            .iter()
            .find(|capsule| capsule.feature_id.as_str() == *feature_id)
            .expect("active capsule exists");
        assert_eq!(
            distribution.authority.entries[&capsule.feature_id]
                .state
                .as_str(),
            "capsule-active"
        );
        let kernel = native_kernel(feature_id).expect("active FeatureID resolves");
        assert_eq!(kernel.kernel_id, *kernel_id);
        assert_eq!(
            binding_semantic_hash(feature_id).as_deref(),
            Some(capsule.semantic_hash.as_str())
        );
    }

    for feature_id in CANDIDATES {
        let capsule = distribution
            .capsules
            .iter()
            .find(|capsule| capsule.feature_id.as_str() == *feature_id)
            .expect("candidate capsule exists");
        assert_eq!(
            distribution.authority.entries[&capsule.feature_id]
                .state
                .as_str(),
            "capsule-candidate"
        );
        assert!(native_kernel(feature_id).is_none());
    }
}

#[test]
fn active_aliases_lower_to_feature_ids_and_keep_literal_spacing_refusal() {
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
    assert!(
        !literal.diagnostics.has_errors(),
        "{:?}",
        literal.diagnostics
    );
    let capability = literal
        .package
        .capabilities
        .iter()
        .position(|capability| capability.name.0 == "std.capability.pde.laplacian")
        .expect("laplacian FeatureID mounted");
    assert!(literal.package.exprs.iter().any(|expression| {
        matches!(expression, ExprNode::Apply { capability: id, arguments } if id.0 as usize == capability && arguments.len() == 2)
    }));

    let mut variable_session = CompilerSession::new(emath_core::limits::Limits::default());
    let variable = variable_session.check_owned(
        "VariableStencil.emath",
        "emath function VariableStencil:\n    inputs:\n        dx: Float64\n    outputs:\n        result: Vector[3]\n    definitions:\n        result = laplacian([1.0, 2.0, 4.0], dx)\n",
    );
    assert!(
        variable.diagnostics.has_errors(),
        "runtime spacing must retain the legacy literal-only refusal"
    );
}

#[test]
fn control_calls_preserve_values_and_typed_refusals_through_the_seam() {
    assert_eq!(
        seam_eval(
            "std.capability.control.transfer-eval",
            &[
                Value::Vector(vec![6.0, 1.0]),
                Value::Vector(vec![2.0, 3.0, 1.0]),
                Value::F64(0.0),
            ],
        ),
        Ok(Value::F64(3.0))
    );
    assert!(
        seam_eval(
            "std.capability.control.transfer-eval",
            &[
                Value::Vector(vec![1.0]),
                Value::Vector(vec![0.0, 1.0]),
                Value::F64(0.0),
            ],
        )
        .is_err(),
        "a pole hit remains a refusal"
    );

    assert_eq!(
        seam_eval(
            "std.capability.control.poles-stable",
            &[Value::Vector(vec![2.0, 3.0, 1.0])],
        ),
        Ok(Value::Bool(true))
    );
    assert_eq!(
        seam_eval(
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
        ),
        Ok(Value::F64(0.5))
    );
}

#[test]
fn fixed_stencils_execute_from_feature_ids_without_name_dispatch() {
    let field = Value::Vector(vec![1.0, 2.0, 4.0]);
    assert_eq!(
        seam_eval(
            "std.capability.pde.laplacian",
            &[field.clone(), Value::F64(1.0)],
        ),
        Ok(Value::Vector(vec![1.0, 1.0, -2.0]))
    );
    assert_eq!(
        seam_eval(
            "std.capability.pde.laplacian-neumann",
            &[field.clone(), Value::F64(1.0)],
        ),
        Ok(Value::Vector(vec![2.0, 1.0, -4.0]))
    );
    assert_eq!(
        seam_eval("std.capability.pde.gradient-1d", &[field, Value::F64(1.0)],),
        Ok(Value::Vector(vec![1.0, 1.5, 2.0]))
    );
    assert!(
        seam_eval(
            "std.capability.pde.laplacian",
            &[Value::Vector(vec![1.0]), Value::F64(0.0)],
        )
        .is_err(),
        "nonpositive spacing remains a typed refusal"
    );

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
    assert_eq!(
        seam_eval(
            "std.capability.pde.divergence-2d",
            &[vx, vy, Value::F64(1.0)],
        ),
        Ok(Value::Matrix {
            rows: 2,
            cols: 2,
            data: vec![2.0; 4],
        })
    );

    let tensor = Value::Tensor {
        shape: vec![2, 2, 2],
        data: (0..8).map(f64::from).collect(),
    };
    let mut identity_weights = vec![0.0; 27];
    identity_weights[13] = 1.0;
    assert_eq!(
        seam_eval(
            "std.capability.pde.stencil-3d-one-sided",
            &[
                tensor.clone(),
                Value::Vector(identity_weights),
                Value::I64(1),
                Value::I64(1),
                Value::I64(1),
            ],
        ),
        Ok(tensor)
    );
}

#[test]
fn legacy_residue_is_limited_to_declared_candidate_holes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let sema_dispatch =
        fs::read_to_string(root.join("crates/emath-sema/src/admit/lowering/call.rs"))
            .expect("sema call dispatch");
    let emitter_dispatch =
        fs::read_to_string(root.join("crates/emath-exec-ir/src/emitter/call.rs"))
            .expect("emitter call dispatch");

    for migrated in [
        "laplacian",
        "laplacian_neumann",
        "laplacian_dirichlet",
        "laplacian_2d",
        "laplacian_2d_neumann",
        "gradient",
        "gradient_2d_x",
        "gradient_2d_y",
        "div_1d",
        "div_2d",
        "transfer_eval",
        "dc_gain",
        "poles_stable",
        "stencil_3d_clamp",
        "stencil_3d_neumann",
        "stencil_3d_one_sided",
    ] {
        assert!(!sema_dispatch.contains(&format!("\"{migrated}\"")));
        assert!(!emitter_dispatch.contains(&format!("\"{migrated}\"")));
    }

    for residue in ["laplacian_3d", "gradient_3d_x", "div", "div_3d"] {
        assert!(!sema_dispatch.contains(&format!("\"{residue}\"")));
        assert!(emitter_dispatch.contains(&format!("\"{residue}\"")));
    }
}
