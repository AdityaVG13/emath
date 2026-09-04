//! Migrate receipt contract core (05 §5).
//!
//! "Breaking change" in emath means precisely: a change under which a
//! previously admitted artifact can no longer be produced, or
//! previously admitted source changes identity without the author's
//! explicit, receipted action. Only three categories are acceptable:
//! honesty repairs, tamper/security repairs, and receipted semantic
//! corrections (edition-major). Everything else goes through the
//! deprecation ladder, never migrate.
//!
//! This module is the CONTRACT CORE: rule self-classification, the
//! canonical stable-JSON receipt, and LOAD-BEARING identity
//! verification — a respell-class rewrite is emitted only when the
//! re-lowered semantic identity is byte-identical. A registered
//! semantic correction instead requires two admitted, different
//! identities and receipts the checked delta. The concrete rule
//! binding (canonical-format respell via the lossless formatter) and
//! the `emath migrate` CLI subcommand are the named follow-ups; the
//! formatter lives in `emath-syntax`, which production `emath-sema`
//! deliberately does not link (kernel seam), so callers inject the
//! rewrite.
//!
//! Determinism class: pure functions of the inputs; same input =
//! byte-identical receipt (replay). No I/O, no clocks, no locale.

use crate::session::CompilerSession;
use emath_core::limits::Limits;
use emath_ir::meaning::meaning_id;

/// Refusal codes (migrate-specific; documented in the diagnostics
/// contract).
pub const E_MIG_SOURCE_REFUSES: &str = "E-MIG-SOURCE-REFUSES";
pub const E_MIG_VERIFY_FAIL: &str = "E-MIG-VERIFY-FAIL";
pub const E_MIG_AMBIGUOUS_SITE: &str = "E-MIG-AMBIGUOUS-SITE";
pub const E_MIG_RULE_INVALID: &str = "E-MIG-RULE-INVALID";

/// The receipt schema is a versioned artifact; stable from day one.
pub const RECEIPT_SCHEMA: &str = "emath.migration-receipt v1";

/// The edition identity rides with the edition machinery when it
/// exists; today there is exactly one edition.
const CURRENT_EDITION: &str = "2026";

/// Registered rule ids (the E-MIG-RULE registry): a rule ships only
/// with a stable id, a self-classification, and its proof-obligation
/// class documented. The registry is the machine-readable target the
/// CLI prints; rewrite functions stay caller-side (kernel seam).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleSpec {
    pub id: &'static str,
    pub kind: RuleKind,
    pub description: &'static str,
}

/// E-MIG-RULE-001 `canonical-format`: the lossless formatter rewrite.
/// Identity-preserving by construction AND verified by re-lowering
/// (the engine refuses if the formatted text changes meaning, which
/// would be a formatter bug — the verify gate makes that impossible
/// to ship silently).
pub const RULE_CANONICAL_FORMAT: RuleSpec = RuleSpec {
    id: "E-MIG-RULE-001",
    kind: RuleKind::Respell,
    description: "canonical-format respell: lossless formatter rewrite, \
                  identity verified by re-lowering both sides",
};

/// E-MIG-RULE-002 `semantic-correction`: an explicitly authorized
/// edition-major correction. Unlike a respell, this rule must change
/// MeaningId and records both checked identities in the receipt.
pub const RULE_SEMANTIC_CORRECTION: RuleSpec = RuleSpec {
    id: "E-MIG-RULE-002",
    kind: RuleKind::Semantic,
    description: "edition-major semantic correction: both meanings must admit and the checked MeaningId delta is receipted",
};

/// The registry, in id order. Stable: rules are never renumbered (a
/// receipt cites its rule ids; receipts are versioned artifacts).
#[must_use]
pub fn registered_rules() -> &'static [RuleSpec] {
    &[RULE_CANONICAL_FORMAT, RULE_SEMANTIC_CORRECTION]
}

/// Refuse a malformed registry before it can ship. `RuleKind` makes
/// an unclassified rule unrepresentable; this gate additionally
/// rejects unstable ids, empty proof descriptions, and duplicates.
pub fn validate_rule_registry(rules: &[RuleSpec]) -> Result<(), &'static str> {
    for (index, rule) in rules.iter().enumerate() {
        if !rule.id.starts_with("E-MIG-RULE-") || rule.description.trim().is_empty() {
            return Err(E_MIG_RULE_INVALID);
        }
        if rules[..index].iter().any(|previous| previous.id == rule.id) {
            return Err(E_MIG_RULE_INVALID);
        }
    }
    Ok(())
}

/// A rule that cannot classify itself does not ship: the type makes
/// unclassified rules unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleKind {
    /// Identity-preserving by construction; verified by re-lowering
    /// both sides and comparing semantic identity byte-for-byte.
    Respell,
    /// Identity-changing; requires the target version's explicit rule
    /// and records the checked identity delta.
    Semantic,
}

impl RuleKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            RuleKind::Respell => "respell",
            RuleKind::Semantic => "semantic",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuleApplied {
    /// Registered rule id (`E-MIG-RULE-*`). The registry is the named
    /// follow-up; callers use their documented rule names.
    pub rule: String,
    pub kind: RuleKind,
    /// Site the rule touched; whole-file rules record `name:whole-file`.
    pub span: String,
    /// Semantic identity (MeaningId) before / after.
    pub before_hash: String,
    pub after_hash: String,
    /// `"none"` for verified respells; semantic rules record the delta.
    pub identity_delta: String,
}

#[derive(Debug, Clone)]
pub struct Refusal {
    pub code: &'static str,
    pub reason: String,
    /// Deterministically ordered author choices for an ambiguous
    /// migration site. Empty for non-ambiguity refusals.
    pub candidates: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MigrationReceipt {
    pub source_edition: &'static str,
    pub target_edition: &'static str,
    pub rules_applied: Vec<RuleApplied>,
    pub refusals: Vec<Refusal>,
    /// `complete | partial-refused` — partial migration is
    /// first-class, never a crash.
    pub verdict: String,
    /// A checked claim: true only when every applied respell verified
    /// byte-identical identity.
    pub identity_verified: bool,
}

impl MigrationReceipt {
    /// Canonical stable JSON: fixed key order, minimal escaping, no
    /// ambient formatting. Replay = byte-identical output.
    #[must_use]
    pub fn to_canonical_json(&self) -> String {
        let mut out = String::with_capacity(256);
        out.push_str("{\"schema\":\"");
        out.push_str(RECEIPT_SCHEMA);
        out.push_str("\",\"source_edition\":\"");
        out.push_str(self.source_edition);
        out.push_str("\",\"target_edition\":\"");
        out.push_str(self.target_edition);
        out.push_str("\",\"rules_applied\":[");
        for (i, r) in self.rules_applied.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("{\"rule\":\"");
            push_escaped(&mut out, &r.rule);
            out.push_str("\",\"kind\":\"");
            out.push_str(r.kind.as_str());
            out.push_str("\",\"span\":\"");
            push_escaped(&mut out, &r.span);
            out.push_str("\",\"before_hash\":\"");
            push_escaped(&mut out, &r.before_hash);
            out.push_str("\",\"after_hash\":\"");
            push_escaped(&mut out, &r.after_hash);
            out.push_str("\",\"identity_delta\":\"");
            push_escaped(&mut out, &r.identity_delta);
            out.push_str("\"}");
        }
        out.push_str("],\"refusals\":[");
        for (i, r) in self.refusals.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("{\"code\":\"");
            out.push_str(r.code);
            out.push_str("\",\"reason\":\"");
            push_escaped(&mut out, &r.reason);
            out.push_str("\",\"candidates\":[");
            for (candidate_index, candidate) in r.candidates.iter().enumerate() {
                if candidate_index > 0 {
                    out.push(',');
                }
                out.push('"');
                push_escaped(&mut out, candidate);
                out.push('"');
            }
            out.push_str("]}");
        }
        out.push_str("],\"verdict\":\"");
        push_escaped(&mut out, &self.verdict);
        out.push_str("\",\"identity_verified\":");
        out.push_str(if self.identity_verified {
            "true"
        } else {
            "false"
        });
        out.push('}');
        out
    }
}

fn push_escaped(out: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MigrationOutcome {
    pub receipt: MigrationReceipt,
    /// The rewritten source, emitted ONLY when the rewrite verified.
    /// `None` = source untouched.
    pub rewritten_source: Option<String>,
}

fn lower_identity(name: &str, source: &str) -> Result<String, Vec<String>> {
    let mut session = CompilerSession::new(Limits::default());
    let result = session.check_owned(name, source);
    let errors: Vec<String> = result
        .diagnostics
        .errors()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    let id =
        meaning_id(&result.package, &[]).map_err(|e| vec![format!("meaning-id error: {e}")])?;
    Ok(id.as_str().to_string())
}

/// Verify and receipt one migration site: lower `before_source`
/// (must admit), lower `rewritten_source` (must admit), classify the
/// rewrite, and emit the rewrite ONLY when identity is preserved
/// byte-for-byte (respell) — an identity-changing rewrite refuses
/// (E-MIG-VERIFY-FAIL) and emits nothing: the migration itself is a
/// bug, never silently shipped. Equal input is an idempotent no-op
/// (empty rule list, `complete`). `rule_id` stamps the receipt (the
/// caller applies a registered `RuleSpec`; the engine does not own
/// the registry mapping id → rewrite).
///
/// Never returns `Err`: refusals are first-class receipt outcomes
/// (`verdict: "partial-refused"`), matching the contract that a
/// partial migration is a first-class outcome.
#[must_use]
pub fn migrate_verified_rewrite(
    file_name: &str,
    before_source: &str,
    rewritten_source: &str,
    rule_id: &str,
) -> MigrationOutcome {
    let refuse = |code: &'static str, reason: String| MigrationOutcome {
        receipt: MigrationReceipt {
            source_edition: CURRENT_EDITION,
            target_edition: CURRENT_EDITION,
            rules_applied: vec![],
            refusals: vec![Refusal {
                code,
                reason,
                candidates: vec![],
            }],
            verdict: "partial-refused".to_string(),
            identity_verified: false,
        },
        rewritten_source: None,
    };

    let before_id = match lower_identity(file_name, before_source) {
        Ok(id) => id,
        Err(errors) => {
            return refuse(
                E_MIG_SOURCE_REFUSES,
                format!(
                    "source does not admit; migrate never rewrites a refusing \
                     source (diagnostics: {})",
                    errors.join(" | ")
                ),
            );
        }
    };
    let after_id = match lower_identity(file_name, rewritten_source) {
        Ok(id) => id,
        Err(errors) => {
            return refuse(
                E_MIG_VERIFY_FAIL,
                format!(
                    "rewritten source does not admit; migration is not emitted \
                     (diagnostics: {})",
                    errors.join(" | ")
                ),
            );
        }
    };

    if rewritten_source == before_source {
        // No rule fired: idempotent no-op on already-canonical source.
        return MigrationOutcome {
            receipt: MigrationReceipt {
                source_edition: CURRENT_EDITION,
                target_edition: CURRENT_EDITION,
                rules_applied: vec![],
                refusals: vec![],
                verdict: "complete".to_string(),
                identity_verified: true,
            },
            rewritten_source: None,
        };
    }

    if before_id != after_id {
        return refuse(
            E_MIG_VERIFY_FAIL,
            format!(
                "rewrite changed semantic identity (before {before_id}, after \
                 {after_id}); rewritten source is not emitted"
            ),
        );
    }

    MigrationOutcome {
        receipt: MigrationReceipt {
            source_edition: CURRENT_EDITION,
            target_edition: CURRENT_EDITION,
            rules_applied: vec![RuleApplied {
                rule: rule_id.to_string(),
                kind: RuleKind::Respell,
                span: format!("{file_name}:whole-file"),
                before_hash: before_id,
                after_hash: after_id,
                identity_delta: "none".to_string(),
            }],
            refusals: vec![],
            verdict: "complete".to_string(),
            identity_verified: true,
        },
        rewritten_source: Some(rewritten_source.to_string()),
    }
}

/// Apply one registered semantic correction. Both sources must admit,
/// and the rule must produce a different MeaningId. The checked delta
/// is explicit in the receipt; treating an identity-preserving change
/// as semantic is a classification error and refuses.
#[must_use]
pub fn migrate_semantic_rewrite(
    file_name: &str,
    before_source: &str,
    rewritten_source: &str,
    rule_id: &str,
) -> MigrationOutcome {
    let refuse = |code: &'static str, reason: String| MigrationOutcome {
        receipt: MigrationReceipt {
            source_edition: "2026",
            target_edition: "2030",
            rules_applied: vec![],
            refusals: vec![Refusal {
                code,
                reason,
                candidates: vec![],
            }],
            verdict: "partial-refused".to_string(),
            identity_verified: false,
        },
        rewritten_source: None,
    };

    let Some(rule) = registered_rules()
        .iter()
        .find(|rule| rule.id == rule_id && rule.kind == RuleKind::Semantic)
    else {
        return refuse(
            E_MIG_RULE_INVALID,
            format!("semantic migration rule `{rule_id}` is not registered as semantic"),
        );
    };
    if validate_rule_registry(registered_rules()).is_err() {
        return refuse(
            E_MIG_RULE_INVALID,
            "migration rule registry failed validation".to_string(),
        );
    }

    let before_id = match lower_identity(file_name, before_source) {
        Ok(id) => id,
        Err(errors) => {
            return refuse(
                E_MIG_SOURCE_REFUSES,
                format!(
                    "source does not admit; semantic migration is not emitted \
                     (diagnostics: {})",
                    errors.join(" | ")
                ),
            );
        }
    };
    let after_id = match lower_identity(file_name, rewritten_source) {
        Ok(id) => id,
        Err(errors) => {
            return refuse(
                E_MIG_VERIFY_FAIL,
                format!(
                    "semantic correction does not admit; migration is not emitted \
                     (diagnostics: {})",
                    errors.join(" | ")
                ),
            );
        }
    };
    if before_source == rewritten_source {
        return MigrationOutcome {
            receipt: MigrationReceipt {
                source_edition: "2026",
                target_edition: "2030",
                rules_applied: vec![],
                refusals: vec![],
                verdict: "complete".to_string(),
                identity_verified: true,
            },
            rewritten_source: None,
        };
    }
    if before_id == after_id {
        return refuse(
            E_MIG_RULE_INVALID,
            format!(
                "rule `{}` classified an identity-preserving rewrite as semantic",
                rule.id
            ),
        );
    }

    let identity_delta = format!("{before_id} -> {after_id}");
    MigrationOutcome {
        receipt: MigrationReceipt {
            source_edition: "2026",
            target_edition: "2030",
            rules_applied: vec![RuleApplied {
                rule: rule.id.to_string(),
                kind: RuleKind::Semantic,
                span: format!("{file_name}:whole-file"),
                before_hash: before_id,
                after_hash: after_id,
                identity_delta,
            }],
            refusals: vec![],
            verdict: "complete".to_string(),
            identity_verified: true,
        },
        rewritten_source: Some(rewritten_source.to_string()),
    }
}

/// Produce the required no-guess outcome for a semantic site that has
/// multiple valid rewrites. Candidate order is part of deterministic
/// replay and is therefore preserved in the canonical receipt.
#[must_use]
pub fn refuse_ambiguous_site(file_name: &str, site: &str, candidates: &[&str]) -> MigrationOutcome {
    MigrationOutcome {
        receipt: MigrationReceipt {
            source_edition: "2026",
            target_edition: "2030",
            rules_applied: vec![],
            refusals: vec![Refusal {
                code: E_MIG_AMBIGUOUS_SITE,
                reason: format!(
                    "{file_name}: semantic site `{site}` has multiple valid migrations; choose one candidate explicitly"
                ),
                candidates: candidates
                    .iter()
                    .map(|candidate| (*candidate).to_string())
                    .collect(),
            }],
            verdict: "partial-refused".to_string(),
            identity_verified: false,
        },
        rewritten_source: None,
    }
}
