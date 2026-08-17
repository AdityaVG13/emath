//! Adversarial trust witnesses for the adapter and provider front doors.
//!
//! Every test here pins a gate that previously admitted untrusted input:
//! the Rumoca validation gate, the certify-the-certifier UTF-8 seam, the
//! empty-admits refusal, isolation-policy enforcement on the advertised
//! table, the maturity ladder, the Dew capability inventory and the plugin
//! permission gate.

use emath_adapter_dew::capability::{Backend, OptimizationEvidence, provide_capability};
use emath_adapter_dew::map_expression;
use emath_adapter_rumoca::{
    Dimensions, EqExpr, Equation, LowerError, StructuralModel, Unit, VariableDecl, VariableKind,
    provide_dae_plan,
};
use emath_core::{QualifiedName, Span};
use emath_evidence::{
    CertificateKind, CertificateRegistry, CheckerContract, EvidenceKind, EvidenceRecord, Freshness,
    ProducerRole, reject_unsound_certifier_output, verify_proof_optional,
};
use emath_ir::package::SemanticPackage;
use emath_ir::{ClaimVerdict, EvidenceClaim, EvidenceLevel, ExprNode, Literal, TypeNode};
use emath_plugin_sdk::{PluginDescriptor, SandboxPolicy, Trust, admit, execute};
use emath_provider_api::{
    CapabilityTable, ConstellationProvider, MaturityLevel, ProviderIsolation, ProviderLock,
    ProviderRegistry, RegistryConfig, default_constellation,
};
use emath_runtime::{Budget, Outcome};

fn state_var(name: &str) -> VariableDecl {
    VariableDecl {
        name: name.to_string(),
        kind: VariableKind::State,
        unit: Unit::new(name.to_string(), Dimensions::meters()),
        ty: TypeNode::Float64,
    }
}

// ---------------------------------------------------------------------------
// Rumoca: provider output is untrusted until checked.
// ---------------------------------------------------------------------------

#[test]
fn hostile_rumoca_model_cannot_plan_as_resolved() {
    // Two states with the same name and a derivative targeting a name that
    // exists nowhere must fail the structural gate before lowering.
    let model = StructuralModel {
        variables: vec![state_var("x"), state_var("x"), state_var("v")],
        equations: vec![Equation {
            lhs: EqExpr::Der("ghost".to_string()),
            rhs: EqExpr::ConstF64(0.0f64.to_bits()),
            origin: "hostile".to_string(),
        }],
        ..StructuralModel::default()
    };
    assert!(
        !model.validate().is_empty(),
        "hostile model must not validate"
    );
    let budget = Budget::default();
    match provide_dae_plan(&model, &budget) {
        Outcome::Failed(LowerError { code, .. }) => {
            assert_eq!(code, "E-PROV-237", "gate must refuse with E-PROV-237");
        }
        other => panic!("hostile model must not reach a plan outcome: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Evidence: the certify-the-certifier corpus is a live rejection seam.
// ---------------------------------------------------------------------------

#[test]
fn binary_certificate_is_e_evid_507() {
    let err = reject_unsound_certifier_output(&[0xFF, 0x00, 0x80]).unwrap_err();
    assert_eq!(err.code, "E-EVID-507");
}

#[test]
fn corpus_pattern_certificate_is_refused() {
    let err = reject_unsound_certifier_output(b"witness-outside-domain").unwrap_err();
    assert_eq!(err.code, "E-EVID-507");
    assert!(reject_unsound_certifier_output(b"clean bytes").is_ok());
}

fn sample_record() -> EvidenceRecord {
    EvidenceRecord {
        claim: EvidenceClaim {
            id: "claim-1".to_string(),
            statement: "rewrite preserves semantics".to_string(),
            class: "rewrite".to_string(),
            scope: "root".to_string(),
            assumptions: vec![],
            producer: "producer-1".to_string(),
            checker: None,
            verdict: ClaimVerdict::Pass,
            level: EvidenceLevel::E1,
            falsifiers: vec![],
            artifacts: vec![],
            fresh_until: None,
        },
        kind: EvidenceKind::Witness,
        producer: ProducerRole {
            id: "producer-1".to_string(),
            kind: EvidenceKind::Witness,
            version: "1.0.0".to_string(),
        },
        checker: None,
        freshness: Freshness {
            issued: "2026-01-01T00:00:00Z".to_string(),
            valid_until: "2026-12-31T00:00:00Z".to_string(),
            renews_with: vec![],
        },
        falsifiers: vec![],
        verdict: ClaimVerdict::Pass,
        incomplete: false,
    }
}

#[test]
fn verify_proof_optional_runs_the_utf8_gate() {
    // The corpus seam must be wired into the proof path, not dormant:
    // binary bytes never reach any kernel.
    let record = sample_record();
    let err = verify_proof_optional(None, &record, &[0x00, 0xFF]).unwrap_err();
    assert_eq!(err.code, "E-EVID-507");
}

#[test]
fn empty_admits_contract_is_refused_e_evid_403() {
    let mut registry = CertificateRegistry::default();
    let contract = CheckerContract {
        kind: CertificateKind::Witness,
        version: "1.0.0".to_string(),
        checker_id: "checker-1".to_string(),
        admits: vec![],
        input_artifacts: vec![],
        output_certificate: "out.cert".to_string(),
        determinism_required: true,
    };
    let err = registry.register(contract).unwrap_err();
    assert_eq!(err.code, "E-EVID-403");
    let ok_contract = CheckerContract {
        kind: CertificateKind::Witness,
        version: "2.0.0".to_string(),
        checker_id: "checker-1".to_string(),
        admits: vec!["rewrite=rule-1".to_string()],
        input_artifacts: vec![],
        output_certificate: "out.cert".to_string(),
        determinism_required: true,
    };
    assert!(registry.register(ok_contract).is_ok());
}

// ---------------------------------------------------------------------------
// Provider API: policy applies to the advertised table, and maturity climbs.
// ---------------------------------------------------------------------------

#[test]
fn remote_advertised_under_static_only_is_denied() {
    let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
    let table = CapabilityTable {
        isolation: ProviderIsolation::Remote,
        ..Default::default()
    };
    let err = registry
        .register("remote-1", ProviderIsolation::Remote, table)
        .unwrap_err();
    assert_eq!(err.code, "E-PROV-510");
    // The provider never appears in the candidate set.
    assert!(registry.ids().is_empty());
}

#[test]
fn isolation_claim_mismatch_is_denied() {
    let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
    let table = CapabilityTable::default(); // advertises Static
    let err = registry
        .register("sneaky", ProviderIsolation::Remote, table)
        .unwrap_err();
    assert_eq!(err.code, "E-PROV-510");
}

#[test]
fn maturity_register_requires_p0_and_climbs_via_promote() {
    let mut registry = default_constellation();
    let provider = ConstellationProvider {
        id: "sneaky-p5".to_string(),
        wave: 'A',
        capability_summary: "claims everything".to_string(),
        no_claim_boundary: vec![],
        maturity: MaturityLevel::P5,
        disabled: false,
        lock: ProviderLock::Unlocked,
        promotion_owner: "attacker".to_string(),
    };
    let err = registry.register(provider).unwrap_err();
    assert_eq!(err.code, "E-PROV-524");
    let p0 = ConstellationProvider {
        id: "honest-p0".to_string(),
        wave: 'A',
        capability_summary: "descriptor only".to_string(),
        no_claim_boundary: vec![],
        maturity: MaturityLevel::P0,
        disabled: false,
        lock: ProviderLock::Unlocked,
        promotion_owner: "attacker".to_string(),
    };
    assert!(registry.register(p0).is_ok());
}

// ---------------------------------------------------------------------------
// Dew: capability claims must match implemented backends; promotions gated.
// ---------------------------------------------------------------------------

#[test]
fn dew_capability_inventory_matches_implemented_backends() {
    let capability = provide_capability();
    assert_eq!(
        capability.backends,
        vec![Backend::RustSource, Backend::TokenStream],
        "JIT/WGSL/GLSL must not be claimed until they exist"
    );
}

#[test]
fn dew_may_promote_gates_differential_rewrites() {
    let evidence = OptimizationEvidence {
        certificates: vec!["fusion".to_string()],
        trusted_rules: vec![],
        requires_differential: vec!["fabricate-fma".to_string()],
    };
    assert!(evidence.may_promote("fusion", false));
    assert!(!evidence.may_promote("fabricate-fma", false));
    assert!(evidence.may_promote("fabricate-fma", true));
}

#[test]
fn dew_bad_arity_is_refusal_not_unknown_function() {
    let mut package = SemanticPackage::new();
    let arg = package.push_expr(
        ExprNode::Literal(Literal::FloatBits(1.0f64.to_bits())),
        Span::default(),
    );
    // sin with two arguments: an arity violation, not an unknown function.
    let call = package.push_expr(
        ExprNode::Call {
            function: QualifiedName::single("sin"),
            arguments: vec![arg, arg],
        },
        Span::default(),
    );
    let err = map_expression(&package, call).unwrap_err();
    assert_eq!(err.code, "E-PROV-030");
}

// ---------------------------------------------------------------------------
// Plugin SDK: permissions gate capabilities; execute uses caller trust.
// ---------------------------------------------------------------------------

fn plugin(capabilities: Vec<&str>, permissions: Vec<&str>) -> PluginDescriptor {
    PluginDescriptor {
        id: "plugin-1".to_string(),
        kind: "evaluate".to_string(),
        interface_core: "emath.plugin.interface".to_string(),
        capabilities: capabilities.into_iter().map(str::to_string).collect(),
        sandbox: SandboxPolicy {
            fuel: Some(1000),
            permissions: permissions.into_iter().map(str::to_string).collect(),
            network: false,
            allowed_capabilities: vec!["fs-read".to_string(), "network".to_string()],
        },
    }
}

#[test]
fn plugin_admit_gates_fs_capability_on_permission() {
    let descriptor = plugin(vec!["fs-read"], vec![]);
    let err = admit(&descriptor, Trust::Local).unwrap_err();
    assert_eq!(err.code, "E-PLG-002");

    let allowed = plugin(vec!["fs-read"], vec!["fs-read"]);
    assert!(admit(&allowed, Trust::Local).is_ok());

    let network = plugin(vec!["network"], vec![]);
    let err = admit(&network, Trust::Local).unwrap_err();
    assert_eq!(err.code, "E-PLG-002");
}

#[test]
fn plugin_execute_routes_through_the_gates() {
    let descriptor = plugin(vec!["fs-read"], vec![]);
    let err = execute(&descriptor, b"input", Trust::Local).unwrap_err();
    assert_eq!(err.code, "E-PLG-002");
}

/// A descriptor whose sandbox declares no fuel budget at all.
fn plugin_unmeasured(capabilities: Vec<&str>, permissions: Vec<&str>) -> PluginDescriptor {
    let mut descriptor = plugin(capabilities, permissions);
    descriptor.sandbox.fuel = None;
    descriptor
}

#[test]
fn plugin_execute_untrusted_without_fuel_is_e_plg_002() {
    let descriptor = plugin_unmeasured(vec!["fs-read"], vec!["fs-read"]);
    let err = execute(&descriptor, b"input", Trust::Untrusted).unwrap_err();
    assert_eq!(err.code, "E-PLG-002");
}

#[test]
fn plugin_execute_local_cannot_skip_the_fuel_gate() {
    // The bead's seam: this descriptor would pass a `Trust::Local` admit
    // (unmetered is tolerated at admission), so the old execute reported
    // E-PLG-001 as if the only problem were a missing runtime. Execution
    // must re-enforce the fuel gate — E-PLG-002 before E-PLG-001 — or
    // Phase 2 inherits an unmetered execution path.
    let descriptor = plugin_unmeasured(vec!["fs-read"], vec!["fs-read"]);
    assert!(admit(&descriptor, Trust::Local).is_ok());
    let err = execute(&descriptor, b"input", Trust::Local).unwrap_err();
    assert_eq!(err.code, "E-PLG-002");
}

#[test]
fn plugin_execute_with_positive_fuel_reaches_the_runtime_refusal() {
    // With fuel declared and permissions satisfied, execution proceeds to
    // the Phase 1 no-runtime refusal (E-PLG-001) — the fuel gate did not
    // over-refuse.
    let descriptor = plugin(vec!["fs-read"], vec!["fs-read"]);
    let err = execute(&descriptor, b"input", Trust::Untrusted).unwrap_err();
    assert_eq!(err.code, "E-PLG-001");
}
