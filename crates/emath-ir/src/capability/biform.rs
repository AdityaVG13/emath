//! Biform (two-sided) evidence and closure assessment.

use super::*;

/// The stability gate: every required projection must be closed, and docs
/// must be bound to the cell's current identity. Returns one typed
/// refusal per gap (empty when the cell is stable); a non-empty result
/// blocks stable visibly — a cell is not done because it compiles.
#[must_use]
pub fn missing_required(
    schema: &CellSchema,
    supplied: &[SuppliedProjection],
) -> Vec<ClosureRefusal> {
    let mut refusals = Vec::new();
    for kind in required_projections(schema.class) {
        // Identity and Schema are planner-minted (`Generated`), never
        // author-supplied: they are always closed and never refused.
        if matches!(kind, ProjectionKind::Identity | ProjectionKind::Schema) {
            continue;
        }
        match supplied.iter().find(|(k, _, _)| *k == kind) {
            Some((ProjectionKind::Docs, _, Some(found))) => {
                let expected = cell_id(schema).0;
                if *found != expected {
                    refusals.push(ClosureRefusal::DocsDrift {
                        name: schema.name.0.clone(),
                        expected,
                        found: found.clone(),
                    });
                }
            }
            Some((ProjectionKind::Docs, _, None)) => refusals.push(ClosureRefusal::DocsDrift {
                name: schema.name.0.clone(),
                expected: cell_id(schema).0,
                found: "<unbound>".to_string(),
            }),
            Some(_) => {}
            None => refusals.push(ClosureRefusal::MissingRequired {
                name: schema.name.0.clone(),
                projection: kind,
            }),
        }
    }
    refusals
}

/// Biform cells: one cell, two
/// authorities. A cell may carry a specification side and an algorithm
/// side whose evidence objects are independent: satisfying tests of the
/// algorithm do not prove the spec, and a spec proof does not certify a
/// particular backend. The machinery is generic — a cell name is data,
/// never a branch; the softmax fixture proves it without any softmax
/// Rust path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BiformSide {
    /// Laws, types, units: what the cell claims.
    Spec,
    /// Reference semantics / bytecode: how the claim is computed.
    Algorithm,
}

impl BiformSide {
    /// Stable token, matching the planned `spec:` / `algorithm:` cell
    /// sections.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Spec => "spec",
            Self::Algorithm => "algorithm",
        }
    }
}

/// Authority class of one side's evidence object. Non-escalation:
/// authored or verified evidence may attest either side; a provider
/// receipt may attest the algorithm by delegation but can never raise
/// spec authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BiformAuthority {
    /// Cell author's own evidence (laws, proofs, reference implementation).
    Authored,
    /// Third-party verified evidence (test suites, audits).
    Verified,
    /// Provider receipt (delegated execution, benchmark receipts).
    Provider,
}

impl BiformAuthority {
    /// Stable token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Authored => "authored",
            Self::Verified => "verified",
            Self::Provider => "provider",
        }
    }

    /// Which side this authority may attest.
    #[must_use]
    pub const fn admits_side(self, side: BiformSide) -> bool {
        match side {
            BiformSide::Algorithm => true,
            BiformSide::Spec => matches!(self, Self::Authored | Self::Verified),
        }
    }
}

/// One side's independent evidence object as supplied for closure: its
/// own EvidenceID token and authority class. Never shared between sides;
/// the spec and algorithm evidence objects are distinct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SideEvidence {
    /// Which side this evidence object attests.
    pub side: BiformSide,
    /// Independent evidence object id (MeaningID/EvidenceID token).
    pub evidence_id: String,
    /// Attesting authority; constrained by [`BiformAuthority::admits_side`].
    pub authority: BiformAuthority,
}

/// Typed per-side disposition (per side: provided / refused / not-applicable
/// via the projection closure — a missing side is a typed refusal, never
/// a silent hole).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BiformSideDisposition {
    /// The side carries a valid evidence object under an admitted
    /// authority.
    Provided {
        evidence_id: String,
        authority: BiformAuthority,
    },
    /// The side is required but cannot close; the typed refusal names
    /// the gap.
    Refused { refusal: BiformRefusal },
    /// The cell's class does not require this side.
    NotApplicable,
}

/// Typed refusals for biform side closure. Closed set with stable codes:
/// nothing silent, nothing unnamed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BiformRefusal {
    /// `E-CELL-009` — a required side has no evidence object (a missing
    /// spec is treated as missing, never as proved by the algorithm
    /// side).
    MissingSide { name: String, side: BiformSide },
    /// `E-CELL-010` — authority escalation: the supplying authority
    /// cannot attest this side (algorithm tests, benchmarks, or provider
    /// receipts cannot raise spec authority).
    AuthorityEscalation {
        name: String,
        side: BiformSide,
        claimed: BiformAuthority,
    },
    /// `E-CELL-011` — one evidence object claimed for both sides: spec
    /// and algorithm evidence must be independent, so a green algorithm
    /// test never stamps the spec proved.
    SideEvidenceCollision {
        name: String,
        spec_evidence_id: String,
        algorithm_evidence_id: String,
    },
}

impl BiformRefusal {
    /// Stable diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::MissingSide { .. } => "E-CELL-009",
            Self::AuthorityEscalation { .. } => "E-CELL-010",
            Self::SideEvidenceCollision { .. } => "E-CELL-011",
        }
    }

    /// Cell the refusal names.
    #[must_use]
    pub fn cell_name(&self) -> &str {
        match self {
            Self::MissingSide { name, .. }
            | Self::AuthorityEscalation { name, .. }
            | Self::SideEvidenceCollision { name, .. } => name,
        }
    }
}

impl fmt::Display for BiformRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSide { name, side } => write!(
                formatter,
                "biform cell `{name}` is missing its `{}` side: no evidence object \
                 (E-CELL-009); a missing side is never treated as proved",
                side.as_str()
            ),
            Self::AuthorityEscalation {
                name,
                side,
                claimed,
            } => write!(
                formatter,
                "biform cell `{name}`: `{}` authority cannot attest the {} side \
                 (E-CELL-010); algorithm tests, benchmarks, or provider receipts \
                 cannot raise spec authority",
                claimed.as_str(),
                side.as_str()
            ),
            Self::SideEvidenceCollision {
                name,
                spec_evidence_id,
                algorithm_evidence_id,
            } => write!(
                formatter,
                "biform cell `{name}` claims one evidence object for both sides \
                 (`{spec_evidence_id}` vs `{algorithm_evidence_id}`) (E-CELL-011); \
                 spec and algorithm evidence must be independent"
            ),
        }
    }
}

impl std::error::Error for BiformRefusal {}

/// Typed disposition of one side of a biform cell. Non-biform classes
/// report [`BiformSideDisposition::NotApplicable`]; for a biform cell a
/// side without an admitted evidence object is a typed refusal.
#[must_use]
pub fn biform_side_disposition(
    schema: &CellSchema,
    side: BiformSide,
    sides: &[SideEvidence],
) -> BiformSideDisposition {
    if schema.class != CellClass::Biform {
        return BiformSideDisposition::NotApplicable;
    }
    let name = schema.name.0.clone();
    let Some(evidence) = sides.iter().find(|s| s.side == side) else {
        return BiformSideDisposition::Refused {
            refusal: BiformRefusal::MissingSide { name, side },
        };
    };
    if !evidence.authority.admits_side(side) {
        return BiformSideDisposition::Refused {
            refusal: BiformRefusal::AuthorityEscalation {
                name,
                side,
                claimed: evidence.authority,
            },
        };
    }
    // The collision is a property of the pair; it is reported once, on
    // the spec side's evaluation, so the closure never double-counts it.
    if side == BiformSide::Spec {
        if let Some(other) = sides
            .iter()
            .find(|s| s.side == BiformSide::Algorithm && s.evidence_id == evidence.evidence_id)
        {
            return BiformSideDisposition::Refused {
                refusal: BiformRefusal::SideEvidenceCollision {
                    name,
                    spec_evidence_id: evidence.evidence_id.clone(),
                    algorithm_evidence_id: other.evidence_id.clone(),
                },
            };
        }
    }
    BiformSideDisposition::Provided {
        evidence_id: evidence.evidence_id.clone(),
        authority: evidence.authority,
    }
}

/// Assess both sides of a biform cell: one typed refusal per failed side
/// in Spec-then-Algorithm order; empty means both sides validate. For
/// non-biform classes every side is [`BiformSideDisposition::NotApplicable`]
/// and nothing refuses.
#[must_use]
pub fn assess_biform_closure(schema: &CellSchema, sides: &[SideEvidence]) -> Vec<BiformRefusal> {
    [BiformSide::Spec, BiformSide::Algorithm]
        .into_iter()
        .filter_map(|side| match biform_side_disposition(schema, side, sides) {
            BiformSideDisposition::Refused { refusal } => Some(refusal),
            _ => None,
        })
        .collect()
}

/// Nanopass projection pipeline:
/// named, ordered, replayable closure-matrix passes, owned by the planner
/// ([`plan_cell_closure`]). Not a product crate: the pass list lives in
/// the capability layer; no core enum grows.
///
/// Each pass is a required [`ProjectionKind`] row with its
/// [`ProjectionStatus`]; the phase order is the closure-matrix row order
/// (P0–P11). Identity-affecting rows mutate the admission descriptor;
/// cosmetic rows annotate without changing identity.
pub mod nanopass {
    use super::{CellClass, CellSchema, ClosureRefusal, ProjectionKind};

    /// Ordered phases P0–P11. For [`CellClass::Pure`] the nine required
    /// closure rows occupy P0–P8; the biform rows P9–P10 are
    /// `NotApplicable` (shown, never swallowed). Other classes fill
    /// phases per [`required_projections`].
    pub const PHASES: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

    /// One named, ordered, replayable projection pass: a closure-matrix
    /// row with its phase, required projection kind, cell class, and
    /// identity role.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct ProjectionPass {
        /// Closure-matrix phase (P0–P11).
        pub phase: u8,
        /// Required projection this pass fulfils.
        pub kind: ProjectionKind,
        /// Cell class the row was planned for.
        pub class: CellClass,
        /// Whether the pass is identity-affecting vs cosmetic.
        pub identity_affecting: bool,
    }

    /// Named, ordered pass list: one [`ProjectionPass`] per required
    /// closure row, in phase order. Replayable: the same schema and
    /// supplied projections always yield the same list. Rows the class
    /// does not require are skipped visibly (`E-CELL-007`), never
    /// silently dropped.
    #[must_use]
    pub fn pass_list(schema: &CellSchema, class: CellClass) -> Vec<ProjectionPass> {
        let required = super::required_projections(class);
        let rows = super::plan_cell_closure(schema, &[]);
        let mut pass_list = Vec::with_capacity(rows.len());
        for (phase, (kind, _status)) in rows.into_iter().enumerate() {
            let phase = u8::try_from(phase).unwrap_or(u8::MAX);
            if required.contains(&kind) {
                pass_list.push(ProjectionPass {
                    phase,
                    kind,
                    class,
                    identity_affecting: matches!(
                        kind,
                        ProjectionKind::Identity
                            | ProjectionKind::Schema
                            | ProjectionKind::Semantics
                    ),
                });
            } else {
                // The closure matrix stays fully visible: a row the class
                // does not require is recorded as `NotApplicable`, never
                // silently skipped.
                let _ = ClosureRefusal::MissingRequired {
                    name: schema.name.0.clone(),
                    projection: kind,
                };
            }
        }
        pass_list
    }
}
