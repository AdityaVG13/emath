//! Proof obligations as machine records.
//!
//! Each verified-complete outline lowers to `emath.proof-obligation
//! v1` records — the stable, provider-agnostic machine target. A
//! `ProofChecker` is a CONTRACT, not a runtime dependency: checkers
//! implement [`ProofChecker`] and are handed the records; nothing in
//! this module executes a checker (proofs are additive authority —
//! a missing checker never blocks compilation, and no verdict is
//! fabricated).
//!
//! Determinism class: pure functions of the outline; the same outline
//! produces byte-identical record JSON (the obligation hash pins it).

use emath_core::{content_id_of_str, json_quote_into};

/// The versioned machine schema (receipt-class artifact; stable).
pub const PROOF_OBLIGATION_SCHEMA: &str = "emath.proof-obligation v1";

/// One obligation step, lowered from the outline surface.
#[derive(Debug, Clone, PartialEq)]
pub struct ProofObligation {
    pub outline: String,
    /// `assumption | lemma`
    pub kind: &'static str,
    pub name: String,
    /// The claim text as written (data, not evaluated).
    pub claim: Option<String>,
    /// Obligations this step may rely on (assumptions declared earlier
    /// in the same outline).
    pub hypotheses: Vec<String>,
}

impl ProofObligation {
    /// Canonical `emath.proof-obligation v1` JSON for this record:
    /// fixed key order, minimal escaping, no ambient formatting.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = String::with_capacity(160);
        out.push_str("{\"schema\":\"");
        out.push_str(PROOF_OBLIGATION_SCHEMA);
        out.push_str("\",\"outline\":");
        json_quote_into(&self.outline, &mut out);
        out.push_str(",\"kind\":");
        json_quote_into(self.kind, &mut out);
        out.push_str(",\"name\":");
        json_quote_into(&self.name, &mut out);
        out.push_str(",\"claim\":");
        match &self.claim {
            Some(claim) => json_quote_into(claim, &mut out),
            None => out.push_str("null"),
        }
        out.push_str(",\"hypotheses\":[");
        for (i, h) in self.hypotheses.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            json_quote_into(h, &mut out);
        }
        out.push_str("]}");
        out
    }

    /// Deterministic content hash of the canonical JSON (no ambient
    /// formatting): the same outline = the same obligation hash.
    #[must_use]
    pub fn obligation_hash(&self) -> String {
        content_id_of_str(&self.to_json()).0
    }
}

/// Provider-agnostic checker CONTRACT. Implementations receive the
/// lowered records and return a typed verdict per obligation. Nothing
/// in emath executes a checker automatically; wiring a specific
/// checker (e.g. a Lean oracle adapter) is a provider decision.
pub trait ProofChecker {
    fn name(&self) -> &'static str;
    /// Verdict for one obligation record. `Ok(true)` = discharged,
    /// `Ok(false)` = refuted, `Err` = cannot decide (never a guess).
    fn check(&self, obligation: &ProofObligation) -> Result<bool, String>;
}

/// A verdict record: which checker, which obligation hash, what
/// outcome. Evidence, not authority — it never gates admission.
#[derive(Debug, Clone, PartialEq)]
pub struct ProofVerdict {
    pub checker: String,
    pub obligation_hash: String,
    pub discharged: bool,
}

/// Lower a verified-complete outline (already validated by admission:
/// kinds are the four, ends with qed, references resolve) into the
/// machine records. `outline_name` is the outline's name; `steps` are
/// the (kind, name, claim) triples in source order. Assumptions
/// accumulate as hypotheses for later lemmas; `check`/`qed` are
/// references to earlier obligations, not new records.
#[must_use]
pub fn lower_outline(
    outline_name: &str,
    steps: &[(&'static str, &str, Option<&str>)],
) -> Vec<ProofObligation> {
    let mut obligations = Vec::new();
    let mut hypotheses: Vec<String> = Vec::new();
    for (kind, name, claim) in steps {
        match *kind {
            "assumption" => {
                hypotheses.push((*name).to_string());
                obligations.push(ProofObligation {
                    outline: outline_name.to_string(),
                    kind: "assumption",
                    name: (*name).to_string(),
                    claim: claim.map(str::to_string),
                    hypotheses: Vec::new(),
                });
            }
            "lemma" => {
                obligations.push(ProofObligation {
                    outline: outline_name.to_string(),
                    kind: "lemma",
                    name: (*name).to_string(),
                    claim: claim.map(str::to_string),
                    hypotheses: hypotheses.clone(),
                });
            }
            _ => {}
        }
    }
    obligations
}

/// Canonical multi-record envelope (an outline's full record set).
#[must_use]
pub fn outline_records_json(outline: &str, obligations: &[ProofObligation]) -> String {
    let mut out = String::with_capacity(200);
    out.push_str("{\"schema\":");
    json_quote_into(PROOF_OBLIGATION_SCHEMA, &mut out);
    out.push_str(",\"outline\":");
    json_quote_into(outline, &mut out);
    out.push_str(",\"records\":[");
    for (i, o) in obligations.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&o.to_json());
    }
    out.push_str("]}");
    out
}

/// Run a checker over the lowered records and collect verdicts. The
/// checker is fully in control of the verdict; emath records it —
/// a checker that cannot decide stays silent (no fabricated
/// verdicts, no guessed discharges), and a missing checker is an
/// empty verdict list, never a fabricated discharge.
#[must_use]
pub fn check_with(
    checker: &dyn ProofChecker,
    obligations: &[ProofObligation],
) -> Vec<ProofVerdict> {
    obligations
        .iter()
        .filter(|o| o.kind == "lemma")
        .filter_map(|o| match checker.check(o) {
            Ok(discharged) => Some(ProofVerdict {
                checker: checker.name().to_string(),
                obligation_hash: o.obligation_hash(),
                discharged,
            }),
            Err(_) => None,
        })
        .collect()
}
