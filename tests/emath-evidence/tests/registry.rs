//! Certificate-registry tests.

use emath_evidence::{CertificateKind, CertificateRegistry, CheckerContract, lookup_contract};
use emath_test_harness::Probe;

fn contract(kind: CertificateKind, version: &str) -> CheckerContract {
    CheckerContract {
        kind,
        version: version.into(),
        checker_id: format!("checker-{}", kind.as_str()),
        admits: vec!["correctness".into(), "equivalence".into()],
        input_artifacts: vec!["ir.bin".into()],
        output_certificate: format!("{}.cert", kind.as_str()),
        determinism_required: true,
    }
}

#[test]
fn certificate_registry() {
    let mut p = Probe::new("certificate registry lookup admits and refusals are typed");
    let mut registry = CertificateRegistry::default();
    p.case("missing-refused", |p| {
        let missing = registry.lookup(CertificateKind::Proof, "1.0.0").unwrap_err();
        p.eq("code", missing.code, "E-EVID-401");
    });
    p.case("empty-admits-refused", |p| {
        let empty = CheckerContract { admits: vec![], ..contract(CertificateKind::Witness, "1.0.0") };
        p.eq("code", registry.register(empty).unwrap_err().code, "E-EVID-403");
    });
    p.case("lookup-and-admits", |p| {
        registry.register(contract(CertificateKind::Proof, "1.0.0")).unwrap();
        p.eq("checker", lookup_contract(&registry, CertificateKind::Proof, "1.0.0").unwrap().checker_id.clone(), "checker-proof".to_string());
        p.eq("admits-correctness", registry.admits(CertificateKind::Proof, "1.0.0", "correctness").unwrap(), true);
        p.eq("refuses-safety", registry.admits(CertificateKind::Proof, "1.0.0", "safety").unwrap(), false);
    });
    p.case("duplicate-refused", |p| {
        p.eq("code", registry.register(contract(CertificateKind::Proof, "1.0.0")).unwrap_err().code, "E-EVID-402");
    });
    p.finish();
}
