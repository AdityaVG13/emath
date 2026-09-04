//! Projection planning and closure checks.

use super::*;

/// Closed set of closure projections for an admitted cell. A cell is not
/// done because it compiles: each projection is a required artifact with a
/// planner-visible status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProjectionKind {
    /// Durable identity (`CellId`) of the cell.
    Identity,
    /// The bounded admission descriptor itself.
    Schema,
    /// Reference semantics of the cell's computation.
    Semantics,
    /// User-facing documentation, bound to the cell identity.
    Docs,
    /// Law/assurance claims attached to the cell.
    Assurance,
    /// Evidence bundle backing the cell's claims.
    Evidence,
    /// Migration/evolution policy record for future change.
    Evolution,
    /// Language-reference projection (required for pure cells).
    Reference,
    /// Compilation projection (required for pure cells).
    Compilation,
    /// Specification side of a biform cell: laws, types, units — what
    /// the cell claims (required for biform cells).
    Specification,
    /// Algorithm side of a biform cell: reference semantics / bytecode —
    /// how the claim is computed (required for biform cells).
    Algorithm,
}

impl ProjectionKind {
    /// Stable token used in canonical output and diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Schema => "schema",
            Self::Semantics => "semantics",
            Self::Docs => "docs",
            Self::Assurance => "assurance",
            Self::Evidence => "evidence",
            Self::Evolution => "evolution",
            Self::Reference => "reference",
            Self::Compilation => "compilation",
            Self::Specification => "specification",
            Self::Algorithm => "algorithm",
        }
    }
}

/// Planner status of one projection. Closed set of exactly the five
/// statuses the closure law admits; nothing is silently "done".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionStatus {
    /// Minted by the planner from the cell itself.
    Generated,
    /// Supplied by the cell author and accepted as-is.
    Provided,
    /// Fulfilled through a named provider contract.
    Provider,
    /// Not required for this cell class (shown, never swallowed).
    NotApplicable,
    /// Refused: a required projection is missing or invalid; blocks stable.
    Refused,
}

impl ProjectionStatus {
    /// Stable token used in canonical output and diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Generated => "generated",
            Self::Provided => "provided",
            Self::Provider => "provider",
            Self::NotApplicable => "not-applicable",
            Self::Refused => "refused",
        }
    }
}

/// Per-class closure matrix: the projections each cell class requires.
/// Every class requires the seven universal projections; pure cells
/// additionally require the language-reference and compilation
/// projections (a pure cell without a reference projection is a visible
/// refusal, never a silent success), and biform cells instead require
/// the specification and algorithm projections (independent evidence,
/// never one object shared across sides).
#[must_use]
pub fn required_projections(class: CellClass) -> Vec<ProjectionKind> {
    let mut required = vec![
        ProjectionKind::Identity,
        ProjectionKind::Schema,
        ProjectionKind::Semantics,
        ProjectionKind::Docs,
        ProjectionKind::Assurance,
        ProjectionKind::Evidence,
        ProjectionKind::Evolution,
    ];
    if class == CellClass::Pure {
        required.push(ProjectionKind::Reference);
        required.push(ProjectionKind::Compilation);
    }
    if class == CellClass::Biform {
        required.push(ProjectionKind::Specification);
        required.push(ProjectionKind::Algorithm);
    }
    required
}

/// Typed refusal for closure planning. Closed set: every refusal has a
/// stable code; a missing required projection blocks stable visibly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClosureRefusal {
    /// `E-CELL-007` — a required projection is missing from the closure.
    MissingRequired {
        name: String,
        projection: ProjectionKind,
    },
    /// `E-CELL-008` — docs are not bound to the cell's current identity.
    DocsDrift {
        name: String,
        expected: String,
        found: String,
    },
}

impl ClosureRefusal {
    /// Stable diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::MissingRequired { .. } => "E-CELL-007",
            Self::DocsDrift { .. } => "E-CELL-008",
        }
    }

    /// Cell the refusal names.
    #[must_use]
    pub fn cell_name(&self) -> &str {
        match self {
            Self::MissingRequired { name, .. } | Self::DocsDrift { name, .. } => name,
        }
    }
}

impl fmt::Display for ClosureRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingRequired { name, projection } => write!(
                formatter,
                "capability cell `{name}` is missing required projection `{}` \
                 (E-CELL-007); missing required projections block stable",
                projection.as_str()
            ),
            Self::DocsDrift {
                name,
                expected,
                found,
            } => write!(
                formatter,
                "docs for capability cell `{name}` drifted from the cell identity: \
                 expected {expected}, found {found} (E-CELL-008)"
            ),
        }
    }
}

impl std::error::Error for ClosureRefusal {}

/// One supplied projection: kind, status, and (for docs) the cell id the
/// artifact claims to document. `(kind, status, docs_cell_id)`.
pub type SuppliedProjection = (ProjectionKind, ProjectionStatus, Option<String>);

/// Plan one cell's closure over the closed projection set. The planner
/// emits exactly one status per projection kind:
///
/// - `Identity` and `Schema` are always `Generated` (the planner mints
///   them from the cell itself);
/// - required projections present in `supplied` pass their status through
///   (including `Provider` where a provider contract fulfils the seam);
/// - required projections absent from `supplied` are `Refused` (blocks
///   stable, visible);
/// - projections the class does not require are `NotApplicable` (shown,
///   never swallowed: the closure matrix stays fully visible).
///
/// Zero core delta: this lives in the capability layer; no core enum
/// grows.
#[must_use]
pub fn plan_cell_closure(
    schema: &CellSchema,
    supplied: &[SuppliedProjection],
) -> Vec<(ProjectionKind, ProjectionStatus)> {
    const ALL: [ProjectionKind; 11] = [
        ProjectionKind::Identity,
        ProjectionKind::Schema,
        ProjectionKind::Semantics,
        ProjectionKind::Docs,
        ProjectionKind::Assurance,
        ProjectionKind::Evidence,
        ProjectionKind::Evolution,
        ProjectionKind::Reference,
        ProjectionKind::Compilation,
        ProjectionKind::Specification,
        ProjectionKind::Algorithm,
    ];
    ALL.into_iter()
        .map(|kind| {
            let status = match kind {
                ProjectionKind::Identity | ProjectionKind::Schema => ProjectionStatus::Generated,
                _ => {
                    let required = required_projections(schema.class).contains(&kind);
                    match supplied.iter().find(|(k, _, _)| *k == kind) {
                        Some((_, status, _)) if required => *status,
                        Some(_) => ProjectionStatus::NotApplicable,
                        None if required => ProjectionStatus::Refused,
                        None => ProjectionStatus::NotApplicable,
                    }
                }
            };
            (kind, status)
        })
        .collect()
}
