//! meaning_provider tests migrated from the in-crate `#[cfg(test)]`
//! module: every symbol they exercise is public crate surface.

use emath_genesis::meaning_provider::{
    admit, challenge, check_version, proposal_id, AdmissionStatus, AgentProposal,
    ChallengeRefusal, ChallengeStatus, MeaningChecker, ProviderError, AUTHORITY_NONE,
    AUTHORITY_STRUCTURAL_CHECKED, PROVIDER_VERSION, REQUIRED_CAPABILITY,
};
use emath_genesis::synth::{OpTable, SynthLaw, MAX_CARRIER_SIZE};

fn xor_table() -> OpTable {
    OpTable {
        carrier_size: 2,
        cells: vec![0, 1, 1, 0],
    }
}

fn nand_table() -> OpTable {
    OpTable {
        carrier_size: 2,
        cells: vec![1, 1, 1, 0],
    }
}

fn good_proposal(producer: &str) -> AgentProposal {
    AgentProposal {
        version: PROVIDER_VERSION,
        producer_id: producer.to_string(),
        table: xor_table(),
        laws: vec![SynthLaw::Commutative, SynthLaw::Identity { element: None }],
        rationale: "xor is a commutative monoid on {0,1}".to_string(),
    }
}

fn capable(id: &str) -> MeaningChecker {
    MeaningChecker {
        id: id.to_string(),
        capabilities: vec![REQUIRED_CAPABILITY.to_string()],
    }
}

#[test]
fn happy_path_admit_then_distinct_checker_promotes() {
    let candidate = admit(good_proposal("agent-0")).expect("admit");
    assert_eq!(candidate.status, AdmissionStatus::Quarantined);
    assert_eq!(candidate.receipt().verdict.canonical(), "quarantined");
    assert_eq!(candidate.receipt().authority, AUTHORITY_NONE);
    assert_eq!(candidate.receipt().checker, None);

    let checked = challenge(&candidate, &capable("checker-1")).expect("challenge");
    assert_eq!(
        checked.status,
        ChallengeStatus::Checked {
            checker_id: "checker-1".to_string()
        }
    );
    let receipt = checked.receipt();
    assert_eq!(receipt.verdict.canonical(), "checked");
    assert_eq!(receipt.checker.as_deref(), Some("checker-1"));
    assert_eq!(receipt.authority, AUTHORITY_STRUCTURAL_CHECKED);
    assert_eq!(receipt.producer, "agent-0");
}

#[test]
fn self_certification_is_the_named_negative_control() {
    let candidate = admit(good_proposal("agent-0")).expect("admit");
    assert_eq!(
        challenge(&candidate, &capable("agent-0")),
        Err(ChallengeRefusal::SelfCertification {
            producer: "agent-0".to_string()
        })
    );
}

#[test]
fn seeded_nand_claiming_associativity_is_rejected() {
    let proposal = AgentProposal {
        version: PROVIDER_VERSION,
        producer_id: "agent-bad".to_string(),
        table: nand_table(),
        laws: vec![SynthLaw::Associative],
        rationale: "nand is associative (it is not)".to_string(),
    };
    let candidate = admit(proposal).expect("well-formed NAND table admits");
    assert_eq!(candidate.status, AdmissionStatus::Quarantined);
    let checked = challenge(&candidate, &capable("checker-1")).expect("challenge ran");
    match &checked.status {
        ChallengeStatus::Rejected {
            checker_id,
            violation,
        } => {
            assert_eq!(checker_id, "checker-1");
            assert_eq!(violation.law, "associative");
            assert_eq!(violation.counterexample, [0, 0, 1]);
        }
        other => panic!("expected Rejected, got {other:?}"),
    }
    let receipt = checked.receipt();
    assert_eq!(receipt.verdict.canonical(), "rejected");
    assert_eq!(receipt.counterexample, Some([0, 0, 1]));
    assert_eq!(receipt.authority, AUTHORITY_STRUCTURAL_CHECKED);
}

#[test]
fn checker_without_required_capability_is_refused() {
    let candidate = admit(good_proposal("agent-0")).expect("admit");
    let incapable = MeaningChecker {
        id: "checker-1".to_string(),
        capabilities: vec!["something-else".to_string()],
    };
    assert_eq!(
        challenge(&candidate, &incapable),
        Err(ChallengeRefusal::MissingCapability {
            required: REQUIRED_CAPABILITY
        })
    );
}

#[test]
fn refused_receipts_record_why() {
    let candidate = admit(good_proposal("agent-0")).expect("admit");
    let self_cert = challenge(&candidate, &capable("agent-0")).expect_err("self-cert");
    let receipt =
        emath_genesis::meaning_provider::MeaningReceipt::refused(candidate.proposal_id, "agent-0", &self_cert);
    assert_eq!(receipt.verdict.canonical(), "refused");
    assert_eq!(receipt.reason, Some("self-certification"));
    assert_eq!(receipt.authority, AUTHORITY_NONE);
    assert!(receipt
        .to_json()
        .contains("\"reason\":\"self-certification\""));

    let incapable = MeaningChecker {
        id: "checker-1".to_string(),
        capabilities: Vec::new(),
    };
    let missing = challenge(&candidate, &incapable).expect_err("missing capability");
    assert_eq!(missing.reason_token(), "missing-capability");
}

#[test]
fn malformed_proposals_are_typed_refusals() {
    let mut out_of_range = good_proposal("agent-0");
    out_of_range.table.cells = vec![0, 1, 1, 2];
    assert_eq!(
        admit(out_of_range),
        Err(ProviderError::InvalidProposal {
            reason: "cell-out-of-range"
        })
    );

    let empty = AgentProposal {
        version: PROVIDER_VERSION,
        producer_id: "agent-0".to_string(),
        table: OpTable {
            carrier_size: 0,
            cells: Vec::new(),
        },
        laws: vec![SynthLaw::Commutative],
        rationale: String::new(),
    };
    assert_eq!(
        admit(empty),
        Err(ProviderError::InvalidProposal {
            reason: "empty-carrier"
        })
    );

    let unknown = AgentProposal {
        version: PROVIDER_VERSION + 1,
        ..good_proposal("agent-0")
    };
    assert_eq!(
        admit(unknown),
        Err(ProviderError::UnknownVersion {
            version: PROVIDER_VERSION + 1
        })
    );
    assert_eq!(check_version(PROVIDER_VERSION), Ok(()));
    assert_eq!(
        check_version(PROVIDER_VERSION + 1),
        Err(ProviderError::UnknownVersion {
            version: PROVIDER_VERSION + 1
        })
    );

    let too_large = AgentProposal {
        table: OpTable {
            carrier_size: MAX_CARRIER_SIZE + 1,
            cells: vec![0; 81],
        },
        ..good_proposal("agent-0")
    };
    assert_eq!(
        admit(too_large),
        Err(ProviderError::InvalidProposal {
            reason: "carrier-too-large"
        })
    );
}

#[test]
fn proposal_id_and_receipt_are_deterministic_and_ignore_rationale() {
    let first = good_proposal("agent-0");
    let mut second = first.clone();
    second.rationale = "a completely different story".to_string();
    assert_eq!(proposal_id(&first), proposal_id(&second));

    let a = admit(first).expect("first");
    let b = admit(second).expect("second");
    assert_eq!(a.proposal_id, b.proposal_id);
    assert_eq!(a.receipt().to_json(), b.receipt().to_json());

    let json = a.receipt().to_json();
    assert!(json.starts_with('{'));
    assert!(json.contains("\"schema\":\"emath.agent-meaning\""));
    assert!(json.contains("\"verdict\":\"quarantined\""));
    assert!(json.contains("\"checker\":null"));
    assert_eq!(json, a.receipt().to_json());
}

#[test]
fn quarantine_receipt_carries_no_checker_and_no_authority() {
    let candidate = admit(good_proposal("agent-0")).expect("admit");
    let receipt = candidate.receipt();
    assert_eq!(receipt.verdict.canonical(), "quarantined");
    assert_eq!(receipt.checker, None);
    assert_eq!(receipt.authority, AUTHORITY_NONE);
    assert_eq!(candidate.status, AdmissionStatus::Quarantined);
    let json = receipt.to_json();
    assert!(json.contains("\"verdict\":\"quarantined\""));
    assert!(json.contains("\"checker\":null"));
    assert!(json.contains("\"authority\":\"none\""));
    assert!(!json.contains("\"checker\":\""));
}
