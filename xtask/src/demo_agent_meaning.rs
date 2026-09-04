//! SG-15 production path: admit an agent proposal to quarantine, refuse
//! self-certification, promote via a distinct capable checker, and reject
//! a seeded non-associative table with its counterexample.

use emath_genesis::{
    AUTHORITY_NONE, AUTHORITY_STRUCTURAL_CHECKED, AdmissionStatus, AgentProposal, ChallengeRefusal,
    ChallengeStatus, MeaningChecker, OpTable, PROVIDER_VERSION, REQUIRED_CAPABILITY, SynthLaw,
    admit, challenge,
};
use emath_world_ir::fnv1a64;

pub fn demo() -> u8 {
    println!("== demo agent-meaning ==");
    match run_demo() {
        Ok(()) => {
            println!("agent-meaning demo: ok");
            0
        }
        Err(error) => {
            eprintln!("agent-meaning demo FAILED: {error}");
            1
        }
    }
}

fn run_demo() -> Result<(), String> {
    emath_genesis::meaning_provider::check_version(PROVIDER_VERSION)
        .map_err(|error| format!("provider version handshake refused: {error:?}"))?;

    let proposal = AgentProposal {
        version: PROVIDER_VERSION,
        producer_id: "agent-0".to_string(),
        table: OpTable {
            carrier_size: 2,
            cells: vec![0, 1, 1, 0],
        },
        laws: vec![SynthLaw::Commutative, SynthLaw::Identity { element: None }],
        rationale: "xor is a commutative monoid on {0,1}".to_string(),
    };
    let candidate = admit(proposal).map_err(|error| format!("admit refused: {error:?}"))?;
    if candidate.status != AdmissionStatus::Quarantined {
        return Err(format!("admit must quarantine, got {:?}", candidate.status));
    }
    let quarantine = candidate.receipt();
    if quarantine.checker.is_some() || quarantine.authority != AUTHORITY_NONE {
        return Err("quarantine receipt must carry no checker and no authority".to_string());
    }
    let quarantine_json = quarantine.to_json();
    if quarantine_json != candidate.receipt().to_json() {
        return Err("quarantine receipts must be byte-identical across rebuilds".to_string());
    }
    println!("agent-meaning|quarantine|{quarantine_json}");

    let mut rows: Vec<String> = Vec::new();
    rows.push(format!(
        "quarantine|id={:016x}|verdict=quarantined|authority={AUTHORITY_NONE}",
        candidate.proposal_id
    ));

    match challenge(
        &candidate,
        &MeaningChecker {
            id: "agent-0".to_string(),
            capabilities: vec![REQUIRED_CAPABILITY.to_string()],
        },
    ) {
        Err(ChallengeRefusal::SelfCertification { producer }) if producer == "agent-0" => {
            rows.push(format!("self-cert|refused|producer={producer}"));
            println!("agent-meaning|self-cert|refused|producer={producer}");
        }
        other => {
            return Err(format!(
                "same-producer challenge must be SelfCertification, got {other:?}"
            ));
        }
    }

    let checked = challenge(
        &candidate,
        &MeaningChecker {
            id: "checker-1".to_string(),
            capabilities: vec![REQUIRED_CAPABILITY.to_string()],
        },
    )
    .map_err(|error| format!("distinct checker refused: {error:?}"))?;
    let ChallengeStatus::Checked { checker_id } = &checked.status else {
        return Err(format!(
            "xor table must promote to Checked, got {:?}",
            checked.status
        ));
    };
    let checked_receipt = checked.receipt();
    if checked_receipt.authority != AUTHORITY_STRUCTURAL_CHECKED {
        return Err("checked authority must be structural-checked".to_string());
    }
    if checked_receipt.to_json() != checked.receipt().to_json() {
        return Err("checked receipts must be byte-identical across rebuilds".to_string());
    }
    rows.push(format!(
        "checked|checker={checker_id}|verdict=checked|authority={AUTHORITY_STRUCTURAL_CHECKED}"
    ));
    println!(
        "agent-meaning|checked|checker={checker_id}|id={:016x}",
        checked.proposal_id
    );

    let planted = AgentProposal {
        version: PROVIDER_VERSION,
        producer_id: "agent-bad".to_string(),
        table: OpTable {
            carrier_size: 2,
            cells: vec![1, 1, 1, 0],
        },
        laws: vec![SynthLaw::Associative],
        rationale: "nand is associative (it is not)".to_string(),
    };
    let planted_candidate =
        admit(planted).map_err(|error| format!("planted NAND must admit: {error:?}"))?;
    let rejected = challenge(
        &planted_candidate,
        &MeaningChecker {
            id: "checker-1".to_string(),
            capabilities: vec![REQUIRED_CAPABILITY.to_string()],
        },
    )
    .map_err(|error| format!("planted NAND challenge refused: {error:?}"))?;
    let ChallengeStatus::Rejected { violation, .. } = &rejected.status else {
        return Err(format!(
            "planted NAND must demote to Rejected, got {:?}",
            rejected.status
        ));
    };
    if violation.counterexample != [0, 0, 1] {
        return Err(format!(
            "expected NAND counterexample [0,0,1], got {:?}",
            violation.counterexample
        ));
    }
    rows.push(format!(
        "rejected|associative|counterexample={},{},{}",
        violation.counterexample[0], violation.counterexample[1], violation.counterexample[2]
    ));
    println!(
        "agent-meaning|rejected|associative|counterexample={},{},{}",
        violation.counterexample[0], violation.counterexample[1], violation.counterexample[2]
    );

    let body = rows.join("\n");
    let receipt_id = fnv1a64(body.as_bytes());
    println!(
        "agent-meaning: rows={} receipt={receipt_id:016x}",
        rows.len()
    );
    Ok(())
}
