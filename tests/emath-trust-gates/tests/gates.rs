//! Adversarial trust witnesses for the adapter and provider front doors.

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
use emath_provider_api::plugin_sdk::{PluginDescriptor, SandboxPolicy, Trust, admit, execute};
use emath_provider_api::runtime::{Budget, Outcome};
use emath_provider_api::{
    CapabilitySpec, CapabilityTable, ConstellationProvider, MaturityLevel, ProviderIsolation,
    ProviderLock, ProviderRegistry, RegistryConfig, RepresentationSpec, default_constellation,
};
use emath_test_harness::Probe;

fn state_var(name: &str) -> VariableDecl {
    VariableDecl { name: name.to_string(), kind: VariableKind::State, unit: Unit::new(name.to_string(), Dimensions::meters()), ty: TypeNode::Float64 }
}

fn sample_record() -> EvidenceRecord {
    EvidenceRecord {
        claim: EvidenceClaim { id: "claim-1".to_string(), statement: "rewrite preserves semantics".to_string(), class: "rewrite".to_string(), scope: "root".to_string(), assumptions: vec![], producer: "producer-1".to_string(), checker: None, verdict: ClaimVerdict::Pass, level: EvidenceLevel::E1, falsifiers: vec![], artifacts: vec![], fresh_until: None },
        kind: EvidenceKind::Witness,
        producer: ProducerRole { id: "producer-1".to_string(), kind: EvidenceKind::Witness, version: "1.0.0".to_string() },
        checker: None,
        freshness: Freshness { issued: "2026-01-01T00:00:00Z".to_string(), valid_until: "2026-12-31T00:00:00Z".to_string(), renews_with: vec![] },
        falsifiers: vec![],
        verdict: ClaimVerdict::Pass,
        incomplete: false,
    }
}

fn plugin(capabilities: Vec<&str>, permissions: Vec<&str>) -> PluginDescriptor {
    PluginDescriptor {
        id: "plugin-1".to_string(),
        kind: "evaluate".to_string(),
        interface_core: "emath.plugin.interface".to_string(),
        capabilities: capabilities.into_iter().map(str::to_string).collect(),
        sandbox: SandboxPolicy { fuel: Some(1000), permissions: permissions.into_iter().map(str::to_string).collect(), network: false, allowed_capabilities: vec!["fs-read".to_string(), "network".to_string()] },
    }
}

fn plugin_unmeasured(capabilities: Vec<&str>, permissions: Vec<&str>) -> PluginDescriptor {
    let mut d = plugin(capabilities, permissions);
    d.sandbox.fuel = None;
    d
}

#[test]
fn gates() {
    let mut p = Probe::new("untrusted adapter and provider inputs refuse by name");
    p.case("rumoca", |p| {
        let model = StructuralModel {
            variables: vec![state_var("x"), state_var("x"), state_var("v")],
            equations: vec![Equation { lhs: EqExpr::Der("ghost".to_string()), rhs: EqExpr::ConstF64(0.0f64.to_bits()), origin: "hostile".to_string() }],
            ..StructuralModel::default()
        };
        p.demand("invalid", !model.validate().is_empty(), "hostile model must not validate");
        match provide_dae_plan(&model, &Budget::default()) {
            Outcome::Failed(LowerError { code, .. }) => { p.eq("code", code, "E-PROV-237"); },
            other => { p.fail("plan", format!("must not plan: {other:?}")); },
        }
    });
    p.case("evidence", |p| {
        p.eq("binary", reject_unsound_certifier_output(&[0xFF, 0x00, 0x80]).unwrap_err().code, "E-EVID-507");
        p.eq("corpus", reject_unsound_certifier_output(b"witness-outside-domain").unwrap_err().code, "E-EVID-507");
        p.demand("clean", reject_unsound_certifier_output(b"clean bytes").is_ok(), "clean passes");
        p.eq("utf8-gate", verify_proof_optional(None, &sample_record(), &[0x00, 0xFF]).unwrap_err().code, "E-EVID-507");
        let mut registry = CertificateRegistry::default();
        let bad = CheckerContract { kind: CertificateKind::Witness, version: "1.0.0".to_string(), checker_id: "checker-1".to_string(), admits: vec![], input_artifacts: vec![], output_certificate: "out.cert".to_string(), determinism_required: true };
        p.eq("empty-admits", registry.register(bad).unwrap_err().code, "E-EVID-403");
        let ok = CheckerContract { kind: CertificateKind::Witness, version: "2.0.0".to_string(), checker_id: "checker-1".to_string(), admits: vec!["rewrite=rule-1".to_string()], input_artifacts: vec![], output_certificate: "out.cert".to_string(), determinism_required: true };
        p.demand("register-ok", registry.register(ok).is_ok(), "nonempty admits registers");
    });
    p.case("provider", |p| {
        let mut registry = ProviderRegistry::new(RegistryConfig::static_only());
        p.eq("remote", registry.register("remote-1", ProviderIsolation::Remote, CapabilityTable { isolation: ProviderIsolation::Remote, ..Default::default() }).unwrap_err().code, "E-PROV-510");
        p.demand("absent", registry.ids().is_empty(), "denied provider absent");
        let mut sneaky = ProviderRegistry::new(RegistryConfig::static_only());
        p.eq("mismatch", sneaky.register("sneaky", ProviderIsolation::Remote, CapabilityTable::default()).unwrap_err().code, "E-PROV-510");
        let mut constellation = default_constellation();
        let p5 = ConstellationProvider { id: "sneaky-p5".to_string(), wave: 'A', capability_summary: "claims everything".to_string(), no_claim_boundary: vec![], maturity: MaturityLevel::P5, disabled: false, lock: ProviderLock::Unlocked, promotion_owner: "attacker".to_string() };
        p.eq("p5", constellation.register(p5).unwrap_err().code, "E-PROV-524");
        let p0 = ConstellationProvider { id: "honest-p0".to_string(), wave: 'A', capability_summary: "descriptor only".to_string(), no_claim_boundary: vec![], maturity: MaturityLevel::P0, disabled: false, lock: ProviderLock::Unlocked, promotion_owner: "attacker".to_string() };
        p.demand("p0", constellation.register(p0).is_ok(), "P0 registers");
        let mut dup = ProviderRegistry::new(RegistryConfig::static_only());
        let table = CapabilityTable { capabilities: vec![CapabilitySpec { name: "evaluate".into(), semantic_subset: "host".into(), representations: vec![RepresentationSpec { name: "native".into(), exact_relation: "identity".into(), encode_cost: 0 }], exactness: vec!["exact".into()], failure_modes: vec![], checker_bindings: vec![] }], ..CapabilityTable::default() };
        dup.register("dup", ProviderIsolation::Static, table.clone()).unwrap();
        p.eq("dup-id", dup.register("dup", ProviderIsolation::Static, table).unwrap_err().code, "E-PROV-518");
        let mut maturity = default_constellation();
        maturity.register(ConstellationProvider { id: "dup-maturity".into(), wave: 'A', capability_summary: "once".into(), no_claim_boundary: vec![], maturity: MaturityLevel::P0, disabled: false, lock: ProviderLock::Unlocked, promotion_owner: "test".into() }).unwrap();
        p.eq("dup-maturity", maturity.register(ConstellationProvider { id: "dup-maturity".into(), wave: 'A', capability_summary: "overwrite attempt".into(), no_claim_boundary: vec![], maturity: MaturityLevel::P0, disabled: false, lock: ProviderLock::Unlocked, promotion_owner: "attacker".into() }).unwrap_err().code, "E-PROV-525");
    });
    p.case("dew", |p| {
        p.eq("backends", provide_capability().backends, vec![Backend::RustSource, Backend::TokenStream]);
        let evidence = OptimizationEvidence { certificates: vec!["fusion".to_string()], trusted_rules: vec![], requires_differential: vec!["fabricate-fma".to_string()] };
        p.demand("fusion", evidence.may_promote("fusion", false), "certified promotes");
        p.demand("no-fabricate", !evidence.may_promote("fabricate-fma", false), "differential gated");
        p.demand("with-diff", evidence.may_promote("fabricate-fma", true), "differential passes with evidence");
        let mut package = SemanticPackage::new();
        let arg = package.push_expr(ExprNode::Literal(Literal::FloatBits(1.0f64.to_bits())), Span::default());
        let call = package.push_expr(ExprNode::Call { function: QualifiedName::single("sin"), arguments: vec![arg, arg] }, Span::default());
        p.eq("arity", map_expression(&package, call).unwrap_err().code, "E-PROV-030");
    });
    p.case("plugin", |p| {
        p.eq("admit-fs", admit(&plugin(vec!["fs-read"], vec![]), Trust::Local).unwrap_err().code, "E-PLG-002");
        p.demand("admit-ok", admit(&plugin(vec!["fs-read"], vec!["fs-read"]), Trust::Local).is_ok(), "permitted admits");
        p.eq("admit-net", admit(&plugin(vec!["network"], vec![]), Trust::Local).unwrap_err().code, "E-PLG-002");
        p.eq("exec-gate", execute(&plugin(vec!["fs-read"], vec![]), b"input", Trust::Local).unwrap_err().code, "E-PLG-002");
        p.eq("untrusted-fuel", execute(&plugin_unmeasured(vec!["fs-read"], vec!["fs-read"]), b"input", Trust::Untrusted).unwrap_err().code, "E-PLG-002");
        p.demand("local-admit", admit(&plugin_unmeasured(vec!["fs-read"], vec!["fs-read"]), Trust::Local).is_ok(), "unmetered tolerated at admit");
        p.eq("local-fuel", execute(&plugin_unmeasured(vec!["fs-read"], vec!["fs-read"]), b"input", Trust::Local).unwrap_err().code, "E-PLG-002");
        p.eq("runtime", execute(&plugin(vec!["fs-read"], vec!["fs-read"]), b"input", Trust::Untrusted).unwrap_err().code, "E-PLG-001");
        let mut bad = plugin(vec!["fs-read"], vec!["fs-read"]);
        bad.id = "bad\nid".into();
        p.eq("control-char", admit(&bad, Trust::Local).unwrap_err().code, "E-PLG-005");
    });
    p.finish();
}
