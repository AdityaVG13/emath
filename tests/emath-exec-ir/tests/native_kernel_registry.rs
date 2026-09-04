//! — the immutable static
//! native-kernel registry (approved generic ABI).
//!
//! These tests prove that capsule FeatureIDs bind immutable kernels by verified
//! identity/signature/hash, refusals cannot mutate the installed distribution,
//! and unknown names never fabricate handlers.

use std::path::Path;

use emath_exec_ir::interp::{EvalFault, Value, evaluate_with_budget};
use emath_exec_ir::language_image::{LanguageDistribution, load_language_distribution};
use emath_exec_ir::native_kernel::{
    KernelBindingError, binding_semantic_hash, install_language_distribution, native_kernel,
};
use emath_exec_ir::{CellClass, EmirOp, EmirProgram, EmirValue, EvalBudget};
use emath_ir::CapsuleSlot;

/// Unknown names keep `None` — the registry never fabricates a handler.
#[test]
fn unknown_name_keeps_none() {
    assert!(native_kernel("std.stochastic.does_not_exist").is_none());
    assert!(native_kernel("").is_none());
}

/// --- The interpreter seam (ApplyCapability → native registry) ---
///
/// The capability-application path consults compiled-cell data FIRST;
/// a miss falls to the immutable native-kernel registry with the SAME
/// arity/refusal discipline and NO new EmirOp or domain switch.
/// Unknown names keep the exact pre-existing refusal.

/// The seam shape: load inputs, then one ApplyCapability.
fn seam_eval(capability: &str, inputs: &[Value]) -> Result<Value, EvalFault> {
    let count = inputs.len();
    let mut ops: Vec<(EmirOp, emath_core::Span)> = (0..count)
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
    let program = EmirProgram {
        ops,
        result: EmirValue(count as u32),
        input_count: count as u16,
        state_count: 0,
        domain_obligations: Vec::new(),
    };
    evaluate_with_budget(&program, inputs, &[], EvalBudget::default())
}

fn reference_pow_mod(base: i64, exponent: i64, modulus: i64) -> Result<Value, EvalFault> {
    // The reference execution is the capsule seam itself: the retired
    // `EmirOp::PowMod` caller now dispatches the real
    // `std.capability.exact.pow-mod` FeatureID through ApplyCapability.
    seam_eval(
        "std.capability.exact.pow-mod",
        &[Value::I64(base), Value::I64(exponent), Value::I64(modulus)],
    )
}

fn mutate_add_semantics(
    distribution: &LanguageDistribution,
    from: &str,
    to: &str,
) -> LanguageDistribution {
    let mut mutated = distribution.clone();
    let semantics = mutated
        .capsules
        .iter_mut()
        .find(|capsule| capsule.feature_id.as_str() == "std.capability.math.add")
        .and_then(|capsule| capsule.slots.get_mut("semantics"));
    let Some(CapsuleSlot::Value(semantics)) = semantics else {
        panic!("add capsule carries semantics")
    };
    *semantics = semantics.replace(from, to);
    mutated
}

#[test]
fn exact_number_theory_cutover_is_feature_bound_and_refusal_safe() {
    const ADD: &str = "std.capability.math.add";
    const POW_MOD: &str = "std.capability.exact.pow-mod";
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
    let distribution = load_language_distribution(&root).expect("load capsule distribution");
    install_language_distribution(&distribution).expect("install capsule-active kernels");

    let capsule = |feature_id: &str| {
        distribution
            .capsules
            .iter()
            .find(|capsule| capsule.feature_id.as_str() == feature_id)
            .expect("cutover capsule exists")
    };
    let add_capsule = capsule(ADD);
    let kernel = native_kernel(ADD).expect("FeatureID resolves its native kernel");
    assert_eq!(kernel.kernel_id, "checked-add");
    assert_eq!(
        binding_semantic_hash(ADD).as_deref(),
        Some(add_capsule.semantic_hash.as_str()),
        "the binding is pinned to the resolved capsule, not its alias or label"
    );
    let pow_capsule = capsule(POW_MOD);
    let pow_kernel = native_kernel(POW_MOD).expect("number-theory FeatureID resolves its kernel");
    assert_eq!(pow_kernel.arity, 3);
    assert_eq!(
        binding_semantic_hash(POW_MOD).as_deref(),
        Some(pow_capsule.semantic_hash.as_str())
    );

    for (left, right, expected) in [
        (0, 0, 0),
        (-5, 7, 2),
        (i64::MIN, 0, i64::MIN),
        (i64::MAX, 0, i64::MAX),
    ] {
        let native = (kernel.handler)(&[Value::I64(left), Value::I64(right)]);
        let reference = seam_eval(ADD, &[Value::I64(left), Value::I64(right)]);
        assert_eq!(native, Ok(Value::I64(expected)));
        assert_eq!(reference, Ok(Value::I64(expected)));
    }
    assert!(
        (kernel.handler)(&[Value::I64(i64::MAX), Value::I64(1)]).is_err(),
        "native exact addition must refuse overflow"
    );
    assert!(
        seam_eval(ADD, &[Value::I64(i64::MAX), Value::I64(1)]).is_err(),
        "reference execution must refuse the same overflow boundary"
    );

    for (base, exponent, modulus, expected) in [
        (2, 10, 1_000, 24),
        (-2, 5, 7, 3),
        (i64::MAX, 0, i64::MAX, 1),
    ] {
        let args = [Value::I64(base), Value::I64(exponent), Value::I64(modulus)];
        let native = (pow_kernel.handler)(&args).expect("native pow_mod");
        let reference = reference_pow_mod(base, exponent, modulus).expect("reference pow_mod");
        assert_eq!(native, Value::I64(expected));
        assert_eq!(reference, Value::I64(expected));
    }
    let zero_modulus = [Value::I64(2), Value::I64(3), Value::I64(0)];
    assert!((pow_kernel.handler)(&zero_modulus).is_err());
    assert!(
        reference_pow_mod(2, 3, 0).is_err(),
        "zero-modulus domain errors must never produce a value"
    );

    let wrong_kernel = mutate_add_semantics(&distribution, "checked-add", "scalar-double");
    assert_eq!(
        install_language_distribution(&wrong_kernel),
        Err(KernelBindingError::MissingKernel("scalar-double".to_string()))
    );
    let stale_signature = mutate_add_semantics(&distribution, "output=Int", "output=Float64");
    assert_eq!(
        install_language_distribution(&stale_signature),
        Err(KernelBindingError::SignatureMismatch(ADD.to_string()))
    );
    assert_eq!(
        binding_semantic_hash(ADD).as_deref(),
        Some(add_capsule.semantic_hash.as_str()),
        "refused installs cannot replace the last valid binding"
    );

    let mut forged_label = distribution.clone();
    let capsule = forged_label
        .capsules
        .iter_mut()
        .find(|capsule| capsule.feature_id.as_str() == ADD)
        .expect("add capsule exists");
    capsule.summary = "result=999".to_string();
    capsule.slots.insert(
        "presentation".to_string(),
        CapsuleSlot::Value("aliases=+,add;result=999".to_string()),
    );
    install_language_distribution(&forged_label).expect("labels do not define kernel meaning");
    let kernel = native_kernel(ADD).expect("FeatureID remains bound");
    assert_eq!(
        (kernel.handler)(&[Value::I64(2), Value::I64(1)]),
        Ok(Value::I64(3)),
        "kernel output comes from operands and semantics, never a result label"
    );

    install_language_distribution(&distribution).expect("restore canonical distribution");
}

/// GCD/LCM cutover (emath-ehpal.7): the `std.capability.exact.gcd` /
/// `std.capability.exact.lcm` capsules are capsule-active and bind the
/// domain-neutral `euclidean-gcd` / `checked-lcm` kernels by verified
/// identity/signature/hash. Inactive (uninstalled) and stale (mutated
/// signature) images refuse typed; overflow refuses typed, never wraps.
#[test]
fn gcd_lcm_cutover_is_feature_bound_and_refusal_safe() {
    const GCD: &str = "std.capability.exact.gcd";
    const LCM: &str = "std.capability.exact.lcm";
    // Inactive image: no binding is installed on this thread, so the
    // registry fabricates no handler and the seam keeps its typed refusal.
    assert!(native_kernel(GCD).is_none());
    assert!(native_kernel(LCM).is_none());
    assert!(seam_eval(GCD, &[Value::I64(12), Value::I64(18)]).is_err());

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language");
    let distribution = load_language_distribution(&root).expect("load capsule distribution");
    install_language_distribution(&distribution).expect("install capsule-active kernels");

    let capsule = |feature_id: &str| {
        distribution
            .capsules
            .iter()
            .find(|capsule| capsule.feature_id.as_str() == feature_id)
            .expect("cutover capsule exists")
    };
    let gcd_capsule = capsule(GCD);
    let gcd_kernel = native_kernel(GCD).expect("gcd FeatureID resolves its native kernel");
    assert_eq!(gcd_kernel.kernel_id, "euclidean-gcd");
    assert_eq!(
        binding_semantic_hash(GCD).as_deref(),
        Some(gcd_capsule.semantic_hash.as_str()),
        "the binding is pinned to the resolved capsule, not its alias or label"
    );
    let lcm_capsule = capsule(LCM);
    let lcm_kernel = native_kernel(LCM).expect("lcm FeatureID resolves its native kernel");
    assert_eq!(lcm_kernel.kernel_id, "checked-lcm");
    assert_eq!(
        binding_semantic_hash(LCM).as_deref(),
        Some(lcm_capsule.semantic_hash.as_str())
    );

    // Happy paths execute through the capsule-active FeatureID seam.
    assert_eq!(
        seam_eval(GCD, &[Value::I64(12), Value::I64(18)]),
        Ok(Value::I64(6))
    );
    assert_eq!(
        seam_eval(LCM, &[Value::I64(4), Value::I64(6)]),
        Ok(Value::I64(12))
    );
    // Edges: gcd(0,0)=0 lattice meet, sign normalization on magnitudes,
    // lcm(0,x)=0.
    assert_eq!(
        seam_eval(GCD, &[Value::I64(0), Value::I64(0)]),
        Ok(Value::I64(0))
    );
    assert_eq!(
        seam_eval(GCD, &[Value::I64(-12), Value::I64(18)]),
        Ok(Value::I64(6))
    );
    assert_eq!(
        seam_eval(LCM, &[Value::I64(0), Value::I64(7)]),
        Ok(Value::I64(0))
    );
    assert_eq!(
        seam_eval(LCM, &[Value::I64(-4), Value::I64(6)]),
        Ok(Value::I64(12))
    );
    // gcd(i64::MIN, 0) = 2^63 has no i64 carrier — typed refusal.
    assert!(
        seam_eval(GCD, &[Value::I64(i64::MIN), Value::I64(0)]).is_err(),
        "gcd(i64::MIN, 0) must refuse: 2^63 exceeds the i64 carrier"
    );
    // lcm overflow refuses typed, never wraps.
    assert!(
        seam_eval(LCM, &[Value::I64(i64::MAX), Value::I64(i64::MAX - 1)]).is_err(),
        "lcm(i64::MAX, i64::MAX-1) must refuse overflow typed"
    );

    // Stale image: a mutated carrier signature refuses install and cannot
    // replace the last valid binding.
    let mut stale = distribution.clone();
    let semantics = stale
        .capsules
        .iter_mut()
        .find(|capsule| capsule.feature_id.as_str() == GCD)
        .and_then(|capsule| capsule.slots.get_mut("semantics"));
    let Some(CapsuleSlot::Value(semantics)) = semantics else {
        panic!("gcd capsule carries semantics")
    };
    *semantics = semantics.replace("output=Int", "output=Nat");
    assert_eq!(
        install_language_distribution(&stale),
        Err(KernelBindingError::SignatureMismatch(GCD.to_string()))
    );
    assert_eq!(
        binding_semantic_hash(GCD).as_deref(),
        Some(gcd_capsule.semantic_hash.as_str()),
        "refused installs cannot replace the last valid binding"
    );

    install_language_distribution(&distribution).expect("restore canonical distribution");
}
