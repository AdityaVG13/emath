//! Agent meaning-provider admission and challenge tests.

use emath_genesis::meaning_provider::{
    AUTHORITY_NONE, AUTHORITY_STRUCTURAL_CHECKED, AdmissionStatus, AgentProposal, ChallengeRefusal,
    ChallengeStatus, MeaningChecker, PROVIDER_VERSION, ProviderError, REQUIRED_CAPABILITY, admit,
    challenge, check_version, proposal_id,
};
use emath_genesis::synth::{MAX_CARRIER_SIZE, OpTable, SynthLaw};
use emath_test_harness::Probe;

fn xor_table() -> OpTable {
    OpTable { carrier_size: 2, cells: vec![0, 1, 1, 0] }
}

fn nand_table() -> OpTable {
    OpTable { carrier_size: 2, cells: vec![1, 1, 1, 0] }
}

fn good_proposal(producer: &str) -> AgentProposal {
    AgentProposal { version: PROVIDER_VERSION, producer_id: producer.to_string(), table: xor_table(), laws: vec![SynthLaw::Commutative, SynthLaw::Identity { element: None }], rationale: "xor is a commutative monoid on {0,1}".to_string() }
}

fn capable(id: &str) -> MeaningChecker {
    MeaningChecker { id: id.to_string(), capabilities: vec![REQUIRED_CAPABILITY.to_string()] }
}

#[test]
fn meaning_provider() {
    let mut p = Probe::new("proposals quarantine on admit and only distinct checkers promote");
    p.case("admit-then-promote", |p| {
        let candidate = admit(good_proposal("agent-0")).expect("admit");
        p.eq("status", candidate.status, AdmissionStatus::Quarantined);
        p.eq("verdict", candidate.receipt().verdict.canonical(), "quarantined");
        p.eq("authority", candidate.receipt().authority, AUTHORITY_NONE);
        p.eq("checker", candidate.receipt().checker, None);
        let checked = challenge(&candidate, &capable("checker-1")).expect("challenge");
        p.eq("status", checked.status.clone(), ChallengeStatus::Checked { checker_id: "checker-1".to_string() });
        let receipt = checked.receipt();
        p.eq("verdict", receipt.verdict.canonical(), "checked");
        p.eq("checker", receipt.checker.as_deref(), Some("checker-1"));
        p.eq("authority", receipt.authority, AUTHORITY_STRUCTURAL_CHECKED);
        p.eq("producer", receipt.producer, "agent-0".to_string());
    });
    p.case("self-cert-refused", |p| {
        let candidate = admit(good_proposal("agent-0")).expect("admit");
        p.eq("refusal", challenge(&candidate, &capable("agent-0")), Err(ChallengeRefusal::SelfCertification { producer: "agent-0".to_string() }));
    });
    p.case("false-claim-rejected", |p| {
        let proposal = AgentProposal { version: PROVIDER_VERSION, producer_id: "agent-bad".to_string(), table: nand_table(), laws: vec![SynthLaw::Associative], rationale: "nand is associative (it is not)".to_string() };
        let candidate = admit(proposal).expect("well-formed NAND table admits");
        p.eq("quarantined", candidate.status, AdmissionStatus::Quarantined);
        let checked = challenge(&candidate, &capable("checker-1")).expect("challenge ran");
        match &checked.status {
            ChallengeStatus::Rejected { checker_id, violation } => {
                p.eq("checker", checker_id.clone(), "checker-1".to_string());
                p.eq("law", violation.law.clone(), "associative".to_string());
                p.eq("triple", violation.counterexample, [0, 0, 1]);
            }
            other => { p.fail("rejected", format!("expected Rejected, got {other:?}")); }
        }
        let receipt = checked.receipt();
        p.eq("verdict", receipt.verdict.canonical(), "rejected");
        p.eq("triple", receipt.counterexample, Some([0, 0, 1]));
        p.eq("authority", receipt.authority, AUTHORITY_STRUCTURAL_CHECKED);
    });
    p.case("incapable-refused", |p| {
        let candidate = admit(good_proposal("agent-0")).expect("admit");
        let incapable = MeaningChecker { id: "checker-1".to_string(), capabilities: vec!["something-else".to_string()] };
        p.eq("refusal", challenge(&candidate, &incapable), Err(ChallengeRefusal::MissingCapability { required: REQUIRED_CAPABILITY }));
    });
    p.case("refused-receipts", |p| {
        let candidate = admit(good_proposal("agent-0")).expect("admit");
        let self_cert = challenge(&candidate, &capable("agent-0")).expect_err("self-cert");
        let receipt = emath_genesis::meaning_provider::MeaningReceipt::refused(candidate.proposal_id, "agent-0", &self_cert);
        p.eq("verdict", receipt.verdict.canonical(), "refused");
        p.eq("reason", receipt.reason, Some("self-certification"));
        p.eq("authority", receipt.authority, AUTHORITY_NONE);
        p.contains("json", &receipt.to_json(), "\"reason\":\"self-certification\"");
        let incapable = MeaningChecker { id: "checker-1".to_string(), capabilities: Vec::new() };
        p.eq("token", challenge(&candidate, &incapable).expect_err("missing capability").reason_token(), "missing-capability");
    });
    p.case("malformed-refused", |p| {
        let mut out_of_range = good_proposal("agent-0");
        out_of_range.table.cells = vec![0, 1, 1, 2];
        p.eq("range", admit(out_of_range), Err(ProviderError::InvalidProposal { reason: "cell-out-of-range" }));
        let empty = AgentProposal { version: PROVIDER_VERSION, producer_id: "agent-0".to_string(), table: OpTable { carrier_size: 0, cells: Vec::new() }, laws: vec![SynthLaw::Commutative], rationale: String::new() };
        p.eq("empty", admit(empty), Err(ProviderError::InvalidProposal { reason: "empty-carrier" }));
        p.eq("version", admit(AgentProposal { version: PROVIDER_VERSION + 1, ..good_proposal("agent-0") }), Err(ProviderError::UnknownVersion { version: PROVIDER_VERSION + 1 }));
        p.eq("version-ok", check_version(PROVIDER_VERSION), Ok(()));
        p.eq("version-unknown", check_version(PROVIDER_VERSION + 1), Err(ProviderError::UnknownVersion { version: PROVIDER_VERSION + 1 }));
        p.eq("too-large", admit(AgentProposal { table: OpTable { carrier_size: MAX_CARRIER_SIZE + 1, cells: vec![0; 81] }, ..good_proposal("agent-0") }), Err(ProviderError::InvalidProposal { reason: "carrier-too-large" }));
    });
    p.case("id-deterministic", |p| {
        let first = good_proposal("agent-0");
        let mut second = first.clone();
        second.rationale = "a completely different story".to_string();
        p.eq("id", proposal_id(&first), proposal_id(&second));
        let a = admit(first).expect("first");
        let b = admit(second).expect("second");
        p.eq("proposal-id", a.proposal_id, b.proposal_id);
        p.eq("receipt", a.receipt().to_json(), b.receipt().to_json());
        let json = a.receipt().to_json();
        p.demand("brace", json.starts_with('{'), "receipt is JSON");
        for needle in ["\"schema\":\"emath.agent-meaning\"", "\"verdict\":\"quarantined\"", "\"checker\":null"] {
            p.contains(needle, &json, needle);
        }
        p.eq("stable", json, a.receipt().to_json());
    });
    p.case("quarantine-carries-nothing", |p| {
        let candidate = admit(good_proposal("agent-0")).expect("admit");
        let receipt = candidate.receipt();
        p.eq("verdict", receipt.verdict.canonical(), "quarantined");
        p.eq("checker", receipt.checker.clone(), None);
        p.eq("authority", receipt.authority, AUTHORITY_NONE);
        p.eq("status", candidate.status, AdmissionStatus::Quarantined);
        let json = receipt.to_json();
        for needle in ["\"verdict\":\"quarantined\"", "\"checker\":null", "\"authority\":\"none\""] {
            p.contains(needle, &json, needle);
        }
        p.demand("no-checker", !json.contains("\"checker\":\""), "no checker value");
    });
    p.finish();
}
