//! SG-15 agent meaning provider: quarantined proposal admission and a
//! producer-distinct challenge loop.
//!
//! [`admit`] performs well-formedness only, granting no authority.
//! [`challenge`] is the only promotion path: producer-distinct checker
//! declaring [`REQUIRED_CAPABILITY`] (same-id is refused); success
//! promotes to `structural-checked`, never higher. Determinism: pure
//! integer identity; receipts are BTreeMap-ordered JSON.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use emath_world_ir::fnv1a64;

use crate::synth::{check_table, LawViolation, OpTable, SynthLaw, MAX_CARRIER_SIZE};

/// Agent-meaning schema id for artifacts and receipts.
pub const PROVIDER_SCHEMA: &str = "emath.agent-meaning";
/// Agent-meaning schema version. Bump on changes to proposal encoding,
/// admission/challenge semantics, or the receipt layout.
pub const PROVIDER_VERSION: u32 = 1;
/// Capability a checker must declare to run the law-check challenge.
pub const REQUIRED_CAPABILITY: &str = "law-check/v1";
/// Authority token for an unchallenged (quarantined) or refused candidate.
pub const AUTHORITY_NONE: &str = "none";
/// Authority token after a producer-distinct structural law check.
/// Never upgraded to a higher evidence kind.
pub const AUTHORITY_STRUCTURAL_CHECKED: &str = "structural-checked";

/// Refuses schema versions this module does not understand.
pub fn check_version(version: u32) -> Result<(), ProviderError> {
    if version == PROVIDER_VERSION {
        Ok(())
    } else {
        Err(ProviderError::UnknownVersion { version })
    }
}

/// Typed refusals for proposal admission (well-formedness only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    /// Schema version this module does not understand.
    UnknownVersion {
        /// Offending version.
        version: u32,
    },
    /// Proposal failed a well-formedness check. See CONTRACT.md for
    /// the reason-token inventory.
    InvalidProposal {
        /// Stable reason token.
        reason: &'static str,
    },
}

/// Protocol refusals for [`challenge`]. These are not law-check
/// outcomes: the challenge never ran (or must not run).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChallengeRefusal {
    /// Checker id equals producer id. Self-certification is impossible
    /// by construction.
    SelfCertification {
        /// Producer that attempted to check its own proposal.
        producer: String,
    },
    /// Checker did not declare [`REQUIRED_CAPABILITY`].
    MissingCapability {
        /// Required capability token.
        required: &'static str,
    },
}

impl ChallengeRefusal {
    /// Stable reason token recorded in refusal receipts.
    #[must_use]
    pub fn reason_token(&self) -> &'static str {
        match self {
            Self::SelfCertification { .. } => "self-certification",
            Self::MissingCapability { .. } => "missing-capability",
        }
    }
}

/// An agent-proposed finite world: a concrete operation table plus
/// claimed laws. The free-text rationale is not trusted and is excluded
/// from [`proposal_id`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProposal {
    /// Schema version the producer claims to speak.
    pub version: u32,
    /// Producer identity. Compared byte-exactly against the checker id.
    pub producer_id: String,
    /// Proposed binary operation table on `{0, …, n−1}`.
    pub table: OpTable,
    /// Laws the producer claims the table satisfies. Admission does
    /// not evaluate these.
    pub laws: Vec<SynthLaw>,
    /// Free-text rationale. Never checked; excluded from identity.
    pub rationale: String,
}

impl AgentProposal {
    /// Canonical proposal text (rationale omitted).
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            "proposal({},table({},[",
            escape(&self.producer_id),
            self.table.carrier_size
        );
        for (index, cell) in self.table.cells.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let _ = write!(out, "{cell}");
        }
        out.push_str("]),[");
        for (index, law) in self.laws.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&law.canonical());
        }
        out.push_str("])");
        out
    }
}

/// A checker that may challenge a quarantined candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeaningChecker {
    /// Checker identity. Must differ from the producer id.
    pub id: String,
    /// Declared capabilities. Must include [`REQUIRED_CAPABILITY`].
    pub capabilities: Vec<String>,
}

/// Admission status. A quarantined candidate carries this mark and
/// no authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionStatus {
    /// Well-formed, unchallenged, no authority.
    Quarantined,
}

/// Outcome of a producer-distinct capable challenge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChallengeStatus {
    /// Laws held under [`check_table`]. Authority is structural-checked.
    Checked {
        /// Producer-distinct checker that ran the law check.
        checker_id: String,
    },
    /// Laws failed; the first lexicographic counterexample is retained.
    Rejected {
        /// Producer-distinct checker that found the counterexample.
        checker_id: String,
        /// Concrete law violation from [`check_table`].
        violation: LawViolation,
    },
}

/// A well-formed proposal that has not been challenged. Status is
/// always [`AdmissionStatus::Quarantined`]; authority is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuarantinedCandidate {
    /// FNV-1a64 of the versioned canonical proposal.
    pub proposal_id: u64,
    /// Always [`AdmissionStatus::Quarantined`].
    pub status: AdmissionStatus,
    proposal: AgentProposal,
}

impl QuarantinedCandidate {
    /// Producer identity.
    #[must_use]
    pub fn producer_id(&self) -> &str {
        &self.proposal.producer_id
    }

    /// The admitted proposal (laws still unverified).
    #[must_use]
    pub fn proposal(&self) -> &AgentProposal {
        &self.proposal
    }

    /// Receipt for an unchallenged candidate: verdict `quarantined`,
    /// no checker id, authority `none`.
    #[must_use]
    pub fn receipt(&self) -> MeaningReceipt {
        MeaningReceipt {
            version: PROVIDER_VERSION,
            proposal_id: self.proposal_id,
            producer: self.proposal.producer_id.clone(),
            checker: None,
            verdict: MeaningVerdict::Quarantined,
            counterexample: None,
            authority: AUTHORITY_NONE,
            reason: None,
        }
    }
}

/// A candidate after a producer-distinct capable challenge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedCandidate {
    /// FNV-1a64 of the versioned canonical proposal.
    pub proposal_id: u64,
    /// [`ChallengeStatus::Checked`] or [`ChallengeStatus::Rejected`].
    pub status: ChallengeStatus,
    proposal: AgentProposal,
}

impl CheckedCandidate {
    /// Producer identity.
    #[must_use]
    pub fn producer_id(&self) -> &str {
        &self.proposal.producer_id
    }

    /// Checker that ran the law check.
    #[must_use]
    pub fn checker_id(&self) -> &str {
        match &self.status {
            ChallengeStatus::Checked { checker_id }
            | ChallengeStatus::Rejected { checker_id, .. } => checker_id,
        }
    }

    /// Receipt: `checked` or `rejected`, authority `structural-checked`.
    #[must_use]
    pub fn receipt(&self) -> MeaningReceipt {
        match &self.status {
            ChallengeStatus::Checked { checker_id } => MeaningReceipt {
                version: PROVIDER_VERSION,
                proposal_id: self.proposal_id,
                producer: self.proposal.producer_id.clone(),
                checker: Some(checker_id.clone()),
                verdict: MeaningVerdict::Checked,
                counterexample: None,
                authority: AUTHORITY_STRUCTURAL_CHECKED,
                reason: None,
            },
            ChallengeStatus::Rejected {
                checker_id,
                violation,
            } => MeaningReceipt {
                version: PROVIDER_VERSION,
                proposal_id: self.proposal_id,
                producer: self.proposal.producer_id.clone(),
                checker: Some(checker_id.clone()),
                verdict: MeaningVerdict::Rejected,
                counterexample: Some(violation.counterexample),
                authority: AUTHORITY_STRUCTURAL_CHECKED,
                reason: None,
            },
        }
    }
}

/// Receipt verdict token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeaningVerdict {
    /// Admitted, unchallenged, no authority.
    Quarantined,
    /// Producer-distinct checker confirmed the claimed laws.
    Checked,
    /// Producer-distinct checker found a counterexample.
    Rejected,
    /// Challenge refused (self-certification or missing capability).
    Refused,
}

impl MeaningVerdict {
    /// Canonical verdict token.
    #[must_use]
    pub fn canonical(self) -> &'static str {
        match self {
            Self::Quarantined => "quarantined",
            Self::Checked => "checked",
            Self::Rejected => "rejected",
            Self::Refused => "refused",
        }
    }
}

/// Deterministic machine-readable meaning-provider receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeaningReceipt {
    /// Schema version echoed into the JSON.
    pub version: u32,
    /// FNV-1a64 of the versioned canonical proposal.
    pub proposal_id: u64,
    /// Producer identity.
    pub producer: String,
    /// Checker identity when a challenge ran and promoted.
    pub checker: Option<String>,
    /// Verdict token.
    pub verdict: MeaningVerdict,
    /// Counterexample triple when the verdict is [`MeaningVerdict::Rejected`].
    pub counterexample: Option<[u8; 3]>,
    /// Authority token: `none` or `structural-checked`.
    pub authority: &'static str,
    /// Refusal reason token when the verdict is [`MeaningVerdict::Refused`]
    /// (`self-certification` or `missing-capability`); otherwise absent.
    pub reason: Option<&'static str>,
}

impl MeaningReceipt {
    /// Receipt for a protocol refusal (self-certification or missing
    /// capability). Authority stays `none`; reason is always recorded.
    #[must_use]
    pub fn refused(proposal_id: u64, producer: &str, reason: &ChallengeRefusal) -> Self {
        Self {
            version: PROVIDER_VERSION,
            proposal_id,
            producer: producer.to_string(),
            checker: None,
            verdict: MeaningVerdict::Refused,
            counterexample: None,
            authority: AUTHORITY_NONE,
            reason: Some(reason.reason_token()),
        }
    }

    /// BTreeMap-ordered JSON, byte-identical across runs.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut root = BTreeMap::new();
        root.insert("authority", Json::Str(self.authority.to_string()));
        root.insert(
            "checker",
            match &self.checker {
                Some(id) => Json::Str(id.clone()),
                None => Json::Null,
            },
        );
        root.insert(
            "counterexample",
            match self.counterexample {
                Some([a, b, c]) => Json::Array(vec![
                    Json::Number(a.to_string()),
                    Json::Number(b.to_string()),
                    Json::Number(c.to_string()),
                ]),
                None => Json::Null,
            },
        );
        root.insert("producer", Json::Str(self.producer.clone()));
        root.insert(
            "proposal_id",
            Json::Str(format!("{:016x}", self.proposal_id)),
        );
        root.insert(
            "reason",
            match self.reason {
                Some(token) => Json::Str(token.to_string()),
                None => Json::Null,
            },
        );
        root.insert("schema", Json::Str(PROVIDER_SCHEMA.to_string()));
        root.insert("verdict", Json::Str(self.verdict.canonical().to_string()));
        root.insert("version", Json::Number(self.version.to_string()));
        emit_object(&root)
    }
}

/// Agent-meaning identity: FNV-1a64 over the versioned canonical
/// proposal. Rationale is excluded.
#[must_use]
pub fn proposal_id(proposal: &AgentProposal) -> u64 {
    fnv1a64(
        format!(
            "{PROVIDER_SCHEMA}.v{PROVIDER_VERSION}:{}",
            proposal.canonical()
        )
        .as_bytes(),
    )
}

/// Admit a proposal as a quarantined candidate. Well-formedness only:
/// carrier bounds, cell ranges, version. Never evaluates claimed laws.
pub fn admit(proposal: AgentProposal) -> Result<QuarantinedCandidate, ProviderError> {
    check_version(proposal.version)?;
    validate(&proposal)?;
    Ok(QuarantinedCandidate {
        proposal_id: proposal_id(&proposal),
        status: AdmissionStatus::Quarantined,
        proposal,
    })
}

/// Challenge a quarantined candidate. Producer-distinct capable checkers
/// run [`check_table`]; same-id or incapable checkers are typed refusals.
pub fn challenge(
    candidate: &QuarantinedCandidate,
    checker: &MeaningChecker,
) -> Result<CheckedCandidate, ChallengeRefusal> {
    if checker.id == candidate.proposal.producer_id {
        return Err(ChallengeRefusal::SelfCertification {
            producer: candidate.proposal.producer_id.clone(),
        });
    }
    if !checker
        .capabilities
        .iter()
        .any(|cap| cap == REQUIRED_CAPABILITY)
    {
        return Err(ChallengeRefusal::MissingCapability {
            required: REQUIRED_CAPABILITY,
        });
    }
    let status = match check_table(&candidate.proposal.table, &candidate.proposal.laws) {
        Ok(()) => ChallengeStatus::Checked {
            checker_id: checker.id.clone(),
        },
        Err(violation) => ChallengeStatus::Rejected {
            checker_id: checker.id.clone(),
            violation,
        },
    };
    Ok(CheckedCandidate {
        proposal_id: candidate.proposal_id,
        status,
        proposal: candidate.proposal.clone(),
    })
}

fn validate(proposal: &AgentProposal) -> Result<(), ProviderError> {
    if proposal.producer_id.is_empty() {
        return Err(ProviderError::InvalidProposal {
            reason: "empty-producer",
        });
    }
    let n = proposal.table.carrier_size;
    if n == 0 {
        return Err(ProviderError::InvalidProposal {
            reason: "empty-carrier",
        });
    }
    if n > MAX_CARRIER_SIZE {
        return Err(ProviderError::InvalidProposal {
            reason: "carrier-too-large",
        });
    }
    let expected = usize::from(n).saturating_mul(usize::from(n));
    if proposal.table.cells.len() != expected {
        return Err(ProviderError::InvalidProposal {
            reason: "cell-count-mismatch",
        });
    }
    if proposal.table.cells.iter().any(|cell| *cell >= n) {
        return Err(ProviderError::InvalidProposal {
            reason: "cell-out-of-range",
        });
    }
    for law in &proposal.laws {
        let named = match law {
            SynthLaw::LeftIdentity { element }
            | SynthLaw::RightIdentity { element }
            | SynthLaw::Identity { element } => *element,
            SynthLaw::Commutative | SynthLaw::Associative => None,
        };
        if let Some(element) = named {
            if element >= n {
                return Err(ProviderError::InvalidProposal {
                    reason: "identity-out-of-range",
                });
            }
        }
    }
    Ok(())
}

enum Json {
    Str(String),
    Number(String),
    Null,
    Array(Vec<Json>),
}

fn emit_object(fields: &BTreeMap<&str, Json>) -> String {
    let mut out = String::from("{");
    for (index, (key, value)) in fields.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(out, "\"{}\":", json_escape(key));
        emit_json(value, &mut out);
    }
    out.push('}');
    out
}

fn emit_json(value: &Json, out: &mut String) {
    match value {
        Json::Str(text) => {
            let _ = write!(out, "\"{}\"", json_escape(text));
        }
        Json::Number(text) => out.push_str(text),
        Json::Null => out.push_str("null"),
        Json::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                emit_json(item, out);
            }
            out.push(']');
        }
    }
}

fn json_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            ',' => out.push_str("\\,"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        admit, challenge, check_version, proposal_id, AdmissionStatus, AgentProposal,
        ChallengeRefusal, ChallengeStatus, MeaningChecker, ProviderError, AUTHORITY_NONE,
        AUTHORITY_STRUCTURAL_CHECKED, PROVIDER_VERSION, REQUIRED_CAPABILITY,
    };
    use crate::synth::{OpTable, SynthLaw, MAX_CARRIER_SIZE};

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
            super::MeaningReceipt::refused(candidate.proposal_id, "agent-0", &self_cert);
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
}
