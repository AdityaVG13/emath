//! Certificate-registry tests.
//!
//! Moved from #[cfg(test)] in crates/emath-evidence/src/registry.rs.

use emath_evidence::{
    CertificateKind, CertificateRegistry, CheckerContract, lookup_contract,
};

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
fn certificate_registry_lookup_and_refusal() {
    let mut registry = CertificateRegistry::default();
    let missing = registry
        .lookup(CertificateKind::Proof, "1.0.0")
        .unwrap_err();
    assert_eq!(missing.code, "E-EVID-401");

    let empty = CheckerContract {
        admits: vec![],
        ..contract(CertificateKind::Witness, "1.0.0")
    };
    let no_class = registry.register(empty).unwrap_err();
    assert_eq!(no_class.code, "E-EVID-403");

    registry
        .register(contract(CertificateKind::Proof, "1.0.0"))
        .unwrap();
    let found = lookup_contract(&registry, CertificateKind::Proof, "1.0.0").unwrap();
    assert_eq!(found.checker_id, "checker-proof");
    assert!(registry
        .admits(CertificateKind::Proof, "1.0.0", "correctness")
        .unwrap());
    assert!(!registry
        .admits(CertificateKind::Proof, "1.0.0", "safety")
        .unwrap());

    let duplicate = registry
        .register(contract(CertificateKind::Proof, "1.0.0"))
        .unwrap_err();
    assert_eq!(duplicate.code, "E-EVID-402");
}
