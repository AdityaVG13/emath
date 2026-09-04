//! Focused authority, ABI, replay, and refusal checks for the probability cutover.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use emath_exec_ir::interp::Value;
use emath_exec_ir::language_image::load_language_distribution;
use emath_exec_ir::native_kernel::{
    KernelArity, binding_semantic_hash, install_language_distribution, native_kernel,
};
use emath_schema::parse_feature_capsule;

fn language_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../language")
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn install() -> emath_exec_ir::language_image::LanguageDistribution {
    let distribution = load_language_distribution(&language_root()).expect("language distribution");
    install_language_distribution(&distribution).expect("probability kernels install");
    distribution
}

fn kernel(feature_id: &str) -> &'static emath_exec_ir::native_kernel::NativeKernel {
    native_kernel(feature_id).unwrap_or_else(|| panic!("missing kernel for {feature_id}"))
}

#[test]
fn every_live_alias_has_one_canonical_feature_and_kernel_signature() {
    let distribution = install();
    let expected = [
        ("std.capability.special.gamma", "stirling-reflection-value"),
        (
            "std.capability.special.gamma-error-bound",
            "stirling-reflection-bound",
        ),
        ("std.capability.special.beta", "gamma-ratio-value"),
        (
            "std.capability.special.beta-error-bound",
            "gamma-ratio-bound",
        ),
        ("std.capability.special.erf", "alternating-odd-series-value"),
        (
            "std.capability.special.erf-error-bound",
            "alternating-odd-series-bound",
        ),
        ("std.capability.special.zeta", "eta-series-value"),
        (
            "std.capability.special.zeta-error-bound",
            "eta-series-bound",
        ),
        (
            "std.capability.special.lambert-w0",
            "principal-product-log-inverse-value",
        ),
        (
            "std.capability.special.lambert-w0-error-bound",
            "principal-product-log-inverse-bound",
        ),
        (
            "std.capability.special.elliptic-k",
            "agm-first-integral-value",
        ),
        (
            "std.capability.special.elliptic-k-error-bound",
            "agm-first-integral-bound",
        ),
        (
            "std.capability.special.elliptic-e",
            "agm-second-integral-value",
        ),
        (
            "std.capability.special.elliptic-e-error-bound",
            "agm-second-integral-bound",
        ),
        (
            "std.capability.probability.normal-sample",
            "counter-stream-gaussian-transform",
        ),
        (
            "std.capability.probability.uniform-sample",
            "counter-stream-affine-transform",
        ),
        (
            "std.capability.probability.bernoulli-sample",
            "counter-stream-threshold-transform",
        ),
        (
            "std.capability.probability.normal-density",
            "gaussian-closed-form",
        ),
        (
            "std.capability.probability.uniform-density",
            "affine-support-closed-form",
        ),
        (
            "std.capability.probability.bernoulli-pmf",
            "binary-mass-closed-form",
        ),
    ];
    let mut keys = BTreeSet::new();
    for (feature_id, kernel_id) in expected {
        let capsule = distribution
            .capsules
            .iter()
            .find(|capsule| capsule.feature_id.as_str() == feature_id)
            .unwrap_or_else(|| panic!("missing capsule {feature_id}"));
        assert_eq!(
            distribution.authority.entries[&capsule.feature_id]
                .state
                .as_str(),
            "capsule-active"
        );
        let descriptor = kernel(feature_id);
        assert_eq!(descriptor.kernel_id, kernel_id);
        assert!(keys.insert((descriptor.kernel_id, descriptor.signature)));
        assert_eq!(
            binding_semantic_hash(feature_id).as_deref(),
            Some(capsule.semantic_hash.as_str())
        );
        assert!(!descriptor.kernel_id.starts_with("std."));
    }

    assert!(native_kernel("std.capability.special.elliptic-pi").is_none());
    assert!(native_kernel("std.capability.statistics.mean").is_none());
}

#[test]
fn central_abi_represents_exact_and_bounded_arity_without_feature_switches() {
    install();
    assert_eq!(
        kernel("std.capability.special.gamma").arity_contract(),
        KernelArity::Exact(1)
    );
    let sampling = kernel("std.capability.probability.normal-sample");
    assert_eq!(
        sampling.arity_contract(),
        KernelArity::Bounded { min: 3, max: 4 }
    );
    assert!(sampling.admits_arity(3));
    assert!(sampling.admits_arity(4));
    assert!(!sampling.admits_arity(2));
    assert!(!sampling.admits_arity(5));
}

#[test]
fn special_value_and_bound_preserve_certified_boundary() {
    install();
    let value =
        (kernel("std.capability.special.gamma").handler)(&[Value::F64(5.0)]).expect("gamma(5)");
    let bound = (kernel("std.capability.special.gamma-error-bound").handler)(&[Value::F64(5.0)])
        .expect("gamma bound");
    let (Value::F64(value), Value::F64(bound)) = (value, bound) else {
        panic!("special leaves return scalars")
    };
    assert!((value - 24.0).abs() <= bound);
    assert!(bound > 0.0);
    assert_eq!(
        (kernel("std.capability.special.gamma").handler)(&[Value::F64(0.0)])
            .expect_err("gamma pole"),
        "E-SPECIAL-POLE"
    );
    assert_eq!(
        (kernel("std.capability.special.zeta").handler)(&[Value::F64(f64::NAN)])
            .expect_err("non-finite carrier"),
        "E-SPECIAL-DOMAIN"
    );
}

#[test]
fn seeded_streams_replay_and_split_with_preserved_refusals() {
    install();
    let descriptor = kernel("std.capability.probability.normal-sample");
    let rooted = [
        Value::Vector(vec![0.0, 1.0]),
        Value::F64(42.0),
        Value::F64(8.0),
    ];
    let split = [
        Value::Vector(vec![0.0, 1.0]),
        Value::F64(42.0),
        Value::F64(8.0),
        Value::Text("campaign.chain-a".to_string()),
    ];
    let first = (descriptor.handler)(&rooted).expect("root stream");
    assert_eq!(first, (descriptor.handler)(&rooted).expect("root replay"));
    assert_ne!(first, (descriptor.handler)(&split).expect("split stream"));
    assert_eq!(
        (descriptor.handler)(&[
            Value::Vector(vec![0.0, 0.0]),
            Value::F64(42.0),
            Value::F64(1.0),
        ])
        .expect_err("invalid scale"),
        "E-PROB-001"
    );
    assert_eq!(
        (descriptor.handler)(&[
            Value::Vector(vec![0.0, 1.0]),
            Value::F64(f64::NAN),
            Value::F64(1.0),
        ])
        .expect_err("non-finite seed"),
        "E-PROB-002"
    );
}

#[test]
fn every_authored_hash_recomputes_and_candidates_remain_honest() {
    let source =
        fs::read_to_string(language_root().join("spec/capabilities/probability-statistics.emath"))
            .expect("capsule source");
    let starts = source
        .match_indices("emath feature ")
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    assert_eq!(starts.len(), 27);
    for (position, start) in starts.iter().copied().enumerate() {
        let end = starts.get(position + 1).copied().unwrap_or(source.len());
        let block = &source[start..end];
        let (capsule, issues) = parse_feature_capsule(block);
        assert!(issues.is_empty(), "capsule hash/schema issues: {issues:?}");
        assert!(capsule.is_some());
    }

    let distribution = install();
    for feature_id in [
        "std.capability.special.elliptic-pi",
        "std.capability.special.elliptic-pi-error-bound",
        "std.capability.statistics.median",
        "std.capability.statistics.variance-sample",
        "std.capability.statistics.variance-population",
        "std.capability.statistics.quantile",
    ] {
        let capsule = distribution
            .capsules
            .iter()
            .find(|capsule| capsule.feature_id.as_str() == feature_id)
            .expect("candidate capsule");
        assert_eq!(
            distribution.authority.entries[&capsule.feature_id]
                .state
                .as_str(),
            "capsule-candidate"
        );
        assert!(native_kernel(feature_id).is_none());
    }
    let mean = distribution
        .capsules
        .iter()
        .find(|capsule| capsule.feature_id.as_str() == "std.capability.statistics.mean")
        .expect("mean capsule");
    assert_eq!(
        distribution.authority.entries[&mean.feature_id]
            .state
            .as_str(),
        "legacy-active-dual-run"
    );
}

#[test]
fn shared_sema_no_longer_owns_migrated_names_or_arities() {
    let root = repository_root();
    let call = fs::read_to_string(root.join("crates/emath-sema/src/admit/lowering/call.rs"))
        .expect("call lowering");
    let arity = fs::read_to_string(root.join("crates/emath-sema/src/admit/lowering/call/arity.rs"))
        .expect("arity lowering");
    let poly =
        fs::read_to_string(root.join("crates/emath-sema/src/admit/lowering/call/poly_prob.rs"))
            .expect("poly lowering");
    for alias in [
        "gamma",
        "gamma_error_bound",
        "normal_sample",
        "uniform_sample",
        "bernoulli_sample",
        "normal_density",
        "uniform_density",
        "bernoulli_pmf",
        "elliptic_pi",
    ] {
        let quoted = format!("\"{alias}\"");
        assert!(!arity.contains(&quoted), "arity still owns {alias}");
        assert!(!poly.contains(&quoted), "poly lowering still owns {alias}");
    }
    assert!(!call.contains("| \"gamma\""));
    assert!(!call.contains("| \"normal_sample\""));
    assert!(call.contains("capability_call_bounds"));
    assert!(call.contains("ends_with('?')"));
}
