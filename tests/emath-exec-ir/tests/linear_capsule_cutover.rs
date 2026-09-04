#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::path::Path;

use emath_artifact::AuthorityState;
use emath_exec_ir::interp::Value;
use emath_exec_ir::language_image::compile_language_directory;
use emath_ir::CapsuleSlot;

// The production registry intentionally does not include this module in this
// cutover slice. Compile the handoff module here so its descriptors and handlers
// are checked before native_kernel.rs performs the mechanical integration.
mod interp {
    pub use emath_exec_ir::interp::Value;
}

mod native_kernel {
    pub use emath_exec_ir::native_kernel::NativeKernel;
}

#[path = "../../../crates/emath-exec-ir/src/native_kernels/linear.rs"]
mod linear;

const ACTIVE: &[(&str, &str, &str, usize)] = &[
    (
        "std.capability.linear.vector-norm",
        "vector-l2",
        "(Vector<Float64>)->Float64",
        1,
    ),
    (
        "std.capability.linear.symmetric-eigenvalues",
        "symmetric-spectrum",
        "(Matrix<Float64>)->Vector<Float64>",
        1,
    ),
    (
        "std.capability.linear.symmetric-eigenvectors",
        "symmetric-basis",
        "(Matrix<Float64>)->Matrix<Float64>",
        1,
    ),
    (
        "std.capability.linear.singular-values",
        "rectangular-spectrum",
        "(Matrix<Float64>)->Vector<Float64>",
        1,
    ),
    (
        "std.capability.linear.svd-factors",
        "rectangular-factors",
        "(Matrix<Float64>)->Matrix<Float64>",
        1,
    ),
    (
        "std.capability.linear.iterative-solve",
        "convergent-system-solve",
        "(Matrix<Float64>,Vector<Float64>)->Vector<Float64>",
        2,
    ),
    (
        "std.capability.linear.vector-add",
        "dense-vector-add",
        "(Vector<Float64>,Vector<Float64>)->Vector<Float64>",
        2,
    ),
    (
        "std.capability.linear.matrix-add",
        "dense-matrix-add",
        "(Matrix<Float64>,Matrix<Float64>)->Matrix<Float64>",
        2,
    ),
    (
        "std.capability.linear.matrix-product",
        "dense-matrix-product",
        "(Matrix<Float64>,Matrix<Float64>)->Matrix<Float64>",
        2,
    ),
    (
        "std.capability.linear.matrix-vector-product",
        "dense-matrix-vector-product",
        "(Matrix<Float64>,Vector<Float64>)->Vector<Float64>",
        2,
    ),
    (
        "std.capability.linear.transpose",
        "dense-transpose",
        "(Matrix<Float64>)->Matrix<Float64>",
        1,
    ),
    (
        "std.capability.tensor.add",
        "dense-tensor-add",
        "(Tensor<Float64>,Tensor<Float64>)->Tensor<Float64>",
        2,
    ),
    (
        "std.capability.poly.mul",
        "polynomial-multiply",
        "(Vector<Float64>,Vector<Float64>)->Vector<Float64>",
        2,
    ),
    (
        "std.capability.poly.eval",
        "polynomial-evaluate",
        "(Vector<Float64>,Float64)->Float64",
        2,
    ),
];

fn semantic_field<'a>(semantics: &'a str, field: &str) -> Option<&'a str> {
    semantics
        .split(';')
        .find_map(|part| part.trim().strip_prefix(field)?.strip_prefix('='))
}

fn matrix(rows: usize, cols: usize, data: &[f64]) -> Value {
    Value::Matrix {
        rows,
        cols,
        data: data.to_vec(),
    }
}

#[test]
fn active_capsules_match_the_native_handoff_table() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
    let distribution = compile_language_directory(&root).expect("compile language distribution");
    let descriptors = linear::LINEAR_KERNELS
        .iter()
        .map(|descriptor| (descriptor.kernel_id, descriptor))
        .collect::<BTreeMap<_, _>>();

    assert_eq!(
        descriptors.len(),
        ACTIVE.len(),
        "one descriptor per active capsule"
    );
    for (feature_id, kernel_id, signature, arity) in ACTIVE {
        let capsule = distribution
            .capsules
            .iter()
            .find(|capsule| capsule.feature_id.as_str() == *feature_id)
            .expect("linear capsule exists");
        assert_eq!(
            distribution.authority.entries[&capsule.feature_id].state,
            AuthorityState::CapsuleActive
        );
        let CapsuleSlot::Value(semantics) = &capsule.slots["semantics"] else {
            panic!("active capsule semantics must be provided")
        };
        assert_eq!(semantic_field(semantics, "kernel"), Some(*kernel_id));
        let arity_text = arity.to_string();
        assert_eq!(
            semantic_field(semantics, "arity"),
            Some(arity_text.as_str())
        );
        let declared_signature = format!(
            "({})->{}",
            semantic_field(semantics, "inputs").expect("inputs"),
            semantic_field(semantics, "output").expect("output")
        );
        assert_eq!(declared_signature, *signature);
        assert!(semantic_field(semantics, "diagnostic").is_some());

        let descriptor = descriptors[*kernel_id];
        assert_eq!(descriptor.signature, *signature);
        assert_eq!(descriptor.arity, *arity);
    }
}

#[test]
fn dense_handlers_preserve_live_values_and_refusals() {
    let norm = linear::LINEAR_KERNELS
        .iter()
        .find(|descriptor| descriptor.kernel_id == "vector-l2")
        .unwrap();
    assert_eq!(
        (norm.handler)(&[Value::Vector(vec![3.0, 4.0])]),
        Ok(Value::F64(5.0))
    );

    let diagonal = matrix(2, 2, &[2.0, 0.0, 0.0, 5.0]);
    let eigen = linear::LINEAR_KERNELS
        .iter()
        .find(|descriptor| descriptor.kernel_id == "symmetric-spectrum")
        .unwrap();
    assert_eq!(
        (eigen.handler)(&[diagonal.clone()]),
        Ok(Value::Vector(vec![2.0, 5.0]))
    );
    let refusal = (eigen.handler)(&[matrix(2, 3, &[1.0; 6])]).unwrap_err();
    assert!(refusal.starts_with("E-LINALG-001"));
    let malformed = (eigen.handler)(&[matrix(2, 2, &[1.0; 3])]).unwrap_err();
    assert!(malformed.starts_with("E-LINALG-004"));

    let singular = linear::LINEAR_KERNELS
        .iter()
        .find(|descriptor| descriptor.kernel_id == "rectangular-spectrum")
        .unwrap();
    assert_eq!(
        (singular.handler)(&[diagonal]),
        Ok(Value::Vector(vec![5.0, 2.0]))
    );

    let solve = linear::LINEAR_KERNELS
        .iter()
        .find(|descriptor| descriptor.kernel_id == "convergent-system-solve")
        .unwrap();
    let Value::Vector(solution) = (solve.handler)(&[
        matrix(2, 2, &[4.0, 0.0, 0.0, 9.0]),
        Value::Vector(vec![8.0, 18.0]),
    ])
    .expect("diagonal system must solve")
    else {
        panic!("solver must return a vector")
    };
    assert_eq!(solution.len(), 2);
    assert!((solution[0] - 2.0).abs() <= 1e-12);
    assert!((solution[1] - 2.0).abs() <= 1e-12);
    let mismatch = (solve.handler)(&[
        matrix(2, 2, &[1.0, 0.0, 0.0, 1.0]),
        Value::Vector(vec![1.0]),
    ])
    .unwrap_err();
    assert!(mismatch.starts_with("E-LINALG-004"));
}

#[test]
fn rank_polymorphic_and_variadic_residue_stays_honest() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
    let distribution = compile_language_directory(&root).expect("compile language distribution");
    for (feature_id, expected_gate) in [
        (
            "std.capability.tensor.index",
            "rank-polymorphic-arity",
        ),
        (
            "std.capability.reduction.finite",
            "carrier-polymorphism",
        ),
    ] {
        let capsule = distribution
            .capsules
            .iter()
            .find(|capsule| capsule.feature_id.as_str() == feature_id)
            .expect("residue capsule exists");
        assert_eq!(
            distribution.authority.entries[&capsule.feature_id].state,
            AuthorityState::CapsuleCandidate
        );
        let CapsuleSlot::Hole { gate, reason } = &capsule.slots["semantics"] else {
            panic!("residue must carry an explicit hole")
        };
        assert_eq!(gate, expected_gate);
        assert!(!reason.is_empty());
        assert!(!reason.contains("kernel="));
    }
}

#[test]
fn migrated_exact_lowering_contains_no_linear_name_branches() {
    let source = include_str!("../../../crates/emath-sema/src/admit/lowering/call/exact.rs");
    for migrated in [
        "\"norm\" =>",
        "\"eigvals\" |",
        "\"eigvecs\" |",
        "\"solve_iterative\" =>",
    ] {
        assert!(
            !source.contains(migrated),
            "legacy branch remains: {migrated}"
        );
    }
}
