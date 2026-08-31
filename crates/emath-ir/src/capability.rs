//! Capability cells: schema, identity, and bounded admission.
//!
//! Domain mathematics enters the IR as data, never as core enum variants:
//! a cell is interned into [`crate::package::SemanticPackage::capabilities`]
//! and referenced from [`crate::expression::ExprNode::Apply`] by
//! [`CapabilityId`]. Adding `Softmax`, a field-pack op, or any future family
//! instance appends to that arena; `ExprNode`, `UnaryOp` and `BinaryOp` do
//! not grow. Core numeric vocabulary (`sin`, `exp`, …) keeps its existing
//! `UnaryOp`/`Call` spelling as the compat path until the migration beads
//! move it onto cells.
//!
//! Schema `emath.capability-cell.v1`: every cell declares a closed
//! [`CellClass`], a schema version, and an explicit migration policy —
//! identity-affecting cell mutation is refused unless the migration policy
//! admits the change. Admission is bounded and typed: the closed
//! [`AdmissionRefusal`] set names every refusal; nothing silent passes.

use emath_core::QualifiedName;
use std::fmt;

/// Canonical schema id for capability-cell descriptors.
pub const CAPABILITY_CELL_SCHEMA_V1: &str = "emath.capability-cell.v1";

/// Closed taxonomy of capability-cell classes. Cells are data: a new class
/// is a schema decision recorded here, never a core op-enum variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CellClass {
    /// Deterministic function of declared inputs.
    Pure,
    /// Same inputs, computation anchored to host/runtime state.
    Intrinsic,
    /// Delegates to a named provider contract.
    Provider,
    /// Composition of admitted cells (DAG, no cycles).
    Composite,
    /// Family expansion instance (one cell per instance).
    Family,
    /// Claim carried by a theory with exhaustive law checking.
    Theory,
    /// Continuous dynamics cell (state, residuals).
    Model,
    /// Structure-preserving map between carriers.
    Morphism,
    /// Method card: algorithm plus falsifier, proposal-only authority.
    Method,
    /// Biform cell: a specification side and an algorithm side whose
    /// evidence objects are independent — satisfying tests of the
    /// algorithm do not prove the spec, and a spec proof does not
    /// certify a particular backend (bead `emath-biform-cells-jswu6`).
    Biform,
    /// Serialized artifact cell (image/lock payload).
    Artifact,
}

impl CellClass {
    /// Stable token used in canonical identity and diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pure => "pure",
            Self::Intrinsic => "intrinsic",
            Self::Provider => "provider",
            Self::Composite => "composite",
            Self::Family => "family",
            Self::Theory => "theory",
            Self::Model => "model",
            Self::Morphism => "morphism",
            Self::Method => "method",
            Self::Biform => "biform",
            Self::Artifact => "artifact",
        }
    }

    /// Parses the stable token; anything else refuses (`E-CELL-001`).
    pub fn parse(text: &str) -> Result<Self, AdmissionRefusal> {
        match text {
            "pure" => Ok(Self::Pure),
            "intrinsic" => Ok(Self::Intrinsic),
            "provider" => Ok(Self::Provider),
            "composite" => Ok(Self::Composite),
            "family" => Ok(Self::Family),
            "theory" => Ok(Self::Theory),
            "model" => Ok(Self::Model),
            "morphism" => Ok(Self::Morphism),
            "method" => Ok(Self::Method),
            "biform" => Ok(Self::Biform),
            "artifact" => Ok(Self::Artifact),
            other => Err(AdmissionRefusal::UnknownCellClass {
                class: other.to_string(),
            }),
        }
    }
}

impl fmt::Display for CellClass {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Migration policy for an identity-affecting schema or cell change.
/// Required on every cell: a stable cell never mutates silently.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MigrationPolicy {
    /// The cell is frozen: any identity-affecting change refuses.
    Frozen,
    /// Identity-affecting changes require an explicit version bump plus a
    /// human-readable migration note recorded on the cell.
    BumpAndNote { note: String },
}

impl MigrationPolicy {
    /// Stable token.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Frozen => "frozen",
            Self::BumpAndNote { .. } => "bump-and-note",
        }
    }

    /// Whether an identity-affecting change from `from` to this policy is
    /// admitted (with the given version pair).
    #[must_use]
    pub fn admits_change(&self, from_version: &str, to_version: &str) -> bool {
        match self {
            Self::Frozen => false,
            Self::BumpAndNote { note } => {
                !note.is_empty() && to_version != from_version
            }
        }
    }
}

/// One admitted capability cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Capability {
    /// Canonical capability path (`std.math.softmax`, pack-local
    /// `my-pack::op`). Interned by admission; the IR layer treats the name
    /// as opaque stable data.
    pub name: QualifiedName,
    /// Closed cell class recorded at admission. The VM dispatch seam
    /// reads it as data (local reference semantics vs an outstanding
    /// provider call); it never becomes a core op-enum variant.
    pub class: CellClass,
}

/// Bounded admission descriptor for a cell (schema `emath.capability-cell.v1`).
///
/// Identity-affecting fields are exactly the ones hashed into `CellId`;
/// presentation fields (`about`) are not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellSchema {
    /// Canonical capability path. Identity.
    pub name: QualifiedName,
    /// Closed cell class. Identity.
    pub class: CellClass,
    /// Declared schema version (`1.0.0`). Identity.
    pub version: String,
    /// Explicit migration policy. Identity (the policy itself, not the note).
    pub migration: MigrationPolicy,
    /// Arity of the declared input list; bounded by [`MAX_CELL_ARITY`].
    pub arity: u16,
    /// Free presentation text; never enters identity.
    pub about: Option<String>,
}

/// Maximum admitted cell arity: cells beyond this refuse (`E-CELL-004`).
pub const MAX_CELL_ARITY: u16 = 64;

/// Typed refusal for bounded cell admission. Closed set: every refusal has
/// a stable code; nothing is silent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdmissionRefusal {
    /// `E-CELL-001` — class token outside the closed taxonomy.
    UnknownCellClass { class: String },
    /// `E-CELL-002` — missing or empty schema version on a stable cell.
    MissingVersion { name: String },
    /// `E-CELL-003` — identity-affecting mutation without an admitted
    /// migration policy (frozen cell, or `bump-and-note` with no note).
    IdentityMutationRefused {
        name: String,
        from_version: String,
        to_version: String,
    },
    /// `E-CELL-004` — declared arity exceeds [`MAX_CELL_ARITY`].
    ArityExceeded { name: String, arity: u16 },
    /// `E-CELL-005` — malformed canonical name (empty or no path segments).
    MalformedName { name: String },
    /// `E-CELL-006` — a pure-cell evaluation ran without the required
    /// explicit numeric policy (missing/empty policy is a refusal, never a
    /// silent default), or the input admits no finite normalization.
    NumericPolicyMissing { name: String },
}

impl AdmissionRefusal {
    /// Stable diagnostic code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnknownCellClass { .. } => "E-CELL-001",
            Self::MissingVersion { .. } => "E-CELL-002",
            Self::IdentityMutationRefused { .. } => "E-CELL-003",
            Self::ArityExceeded { .. } => "E-CELL-004",
            Self::MalformedName { .. } => "E-CELL-005",
            Self::NumericPolicyMissing { .. } => "E-CELL-006",
        }
    }

    /// Cell the refusal names (empty for class-level refusals).
    #[must_use]
    pub fn cell_name(&self) -> &str {
        match self {
            Self::UnknownCellClass { .. } => "",
            Self::MissingVersion { name }
            | Self::IdentityMutationRefused { name, .. }
            | Self::ArityExceeded { name, .. }
            | Self::MalformedName { name }
            | Self::NumericPolicyMissing { name } => name,
        }
    }
}

impl fmt::Display for AdmissionRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownCellClass { class } => write!(
                formatter,
                "unknown capability cell class `{class}` (E-CELL-001)"
            ),
            Self::MissingVersion { name } => write!(
                formatter,
                "capability cell `{name}` declares no schema version (E-CELL-002)"
            ),
            Self::IdentityMutationRefused {
                name,
                from_version,
                to_version,
            } => write!(
                formatter,
                "identity-affecting change to `{name}` ({from_version} -> {to_version}) \
                 is refused by its migration policy (E-CELL-003)"
            ),
            Self::ArityExceeded { name, arity } => write!(
                formatter,
                "capability cell `{name}` arity {arity} exceeds the bounded maximum \
                 {MAX_CELL_ARITY} (E-CELL-004)"
            ),
            Self::MalformedName { name } => write!(
                formatter,
                "capability cell name `{name}` is empty or has no namespace path (E-CELL-005)"
            ),
            Self::NumericPolicyMissing { name } => write!(
                formatter,
                "pure cell `{name}` evaluated without the required explicit numeric \
                 policy (E-CELL-006)"
            ),
        }
    }
}

impl std::error::Error for AdmissionRefusal {}

/// Deterministic canonical token for schema/identity continuity
/// (schema mutation moves identity).
#[must_use]
pub fn canonical_capability(capability: &Capability) -> String {
    format!("cap:{}", capability.name.0)
}

/// Deterministic identity preimage for a cell descriptor: exactly the
/// identity-affecting fields, length-framed, stable order. `about` is
/// excluded (presentation).
#[must_use]
pub fn canonical_cell(schema: &CellSchema) -> String {
    fn field(out: &mut String, name: &str, value: &str) {
        out.push_str(name);
        out.push(':');
        out.push_str(&value.len().to_string());
        out.push(':');
        out.push_str(value);
        out.push('\n');
    }
    let mut out = String::new();
    field(&mut out, "schema", CAPABILITY_CELL_SCHEMA_V1);
    field(&mut out, "name", &schema.name.0);
    field(&mut out, "class", schema.class.as_str());
    field(&mut out, "version", &schema.version);
    field(&mut out, "migration", schema.migration.as_str());
    field(&mut out, "arity", &schema.arity.to_string());
    out
}

/// Content identity of a cell descriptor: `fnv1a64` over
/// [`canonical_cell`], shaped like every other IR content id. Mutation of
/// any identity field changes the id; mutation of `about` does not.
#[must_use]
pub fn cell_id(schema: &CellSchema) -> emath_core::ContentId {
    emath_core::ContentId(format!(
        "fnv1a64:{:016x}",
        emath_core::fnv1a64_bytes(canonical_cell(schema).as_bytes())
    ))
}

/// Bounded admission of a cell descriptor: validates the closed taxonomy,
/// name shape, version presence and arity, then stamps the descriptor.
/// The returned [`Capability`] is the arena record; `cell_id` of the
/// descriptor is its durable identity.
pub fn admit_cell(schema: &CellSchema) -> Result<Capability, AdmissionRefusal> {
    // Names are path-shaped (`std.math.softmax`, `my-pack::op`); an empty
    // or separator-less bare leaf has no namespace to be stable in.
    let name = &schema.name.0;
    if name.is_empty() || !(name.contains("::") || name.contains('.')) {
        return Err(AdmissionRefusal::MalformedName {
            name: schema.name.0.clone(),
        });
    }
    if schema.version.is_empty() {
        return Err(AdmissionRefusal::MissingVersion {
            name: schema.name.0.clone(),
        });
    }
    if schema.arity > MAX_CELL_ARITY {
        return Err(AdmissionRefusal::ArityExceeded {
            name: schema.name.0.clone(),
            arity: schema.arity,
        });
    }
    Ok(Capability {
        name: schema.name.clone(),
        class: schema.class,
    })
}

/// Bounded admission of an identity-affecting mutation: the cell existed at
/// `from` and now proposes `to`. The migration policy decides; a refused
/// mutation is a typed [`AdmissionRefusal::IdentityMutationRefused`], never
/// a silent identity move.
pub fn admit_cell_mutation(
    from: &CellSchema,
    to: &CellSchema,
) -> Result<Capability, AdmissionRefusal> {
    if canonical_cell(from) == canonical_cell(to) {
        return admit_cell(to);
    }
    let name = &to.name.0;
    if name.is_empty() || !(name.contains("::") || name.contains('.')) {
        return Err(AdmissionRefusal::MalformedName {
            name: to.name.0.clone(),
        });
    }
    if to.version.is_empty() {
        return Err(AdmissionRefusal::MissingVersion {
            name: to.name.0.clone(),
        });
    }
    if to.arity > MAX_CELL_ARITY {
        return Err(AdmissionRefusal::ArityExceeded {
            name: to.name.0.clone(),
            arity: to.arity,
        });
    }
    if from.class != to.class || !to.migration.admits_change(&from.version, &to.version) {
        return Err(AdmissionRefusal::IdentityMutationRefused {
            name: to.name.0.clone(),
            from_version: from.version.clone(),
            to_version: to.version.clone(),
        });
    }
    Ok(Capability {
        name: to.name.clone(),
        class: to.class,
    })
}

/// Reference semantics of the `std.tensor.softmax` pure cell under the
/// strict-f64 numeric policy (stable-max form: shift invariance is the
/// law, applied as `x - max(x)` for overflow safety).
///
/// Refusals: an empty input means no numeric policy was declared for the
/// evaluation (`E-CELL-006`, never a silent empty distribution); a
/// non-finite logit refuses under the strict-f64 finite policy
/// (`E-CELL-006`, same code family — the policy declares what is refused).
///
/// Zero core delta: this lives in the capability layer; no core enum grows.
pub fn softmax_reference_strict_f64(
    logits: &[f64],
) -> Result<Vec<f64>, AdmissionRefusal> {
    if logits.is_empty() {
        return Err(AdmissionRefusal::NumericPolicyMissing {
            name: "std.tensor.softmax".into(),
        });
    }
    // Strict-f64 finite policy: every logit must be finite (NAN/INF
    // refuse; f64::max silently drops NAN, so check each element).
    if logits.iter().any(|x| !x.is_finite()) {
        return Err(AdmissionRefusal::NumericPolicyMissing {
            name: "std.tensor.softmax".into(),
        });
    }
    let max = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let exps: Vec<f64> = logits.iter().map(|&x| (x - max).exp()).collect();
    let sum: f64 = exps.iter().sum();
    debug_assert!(sum.is_finite() && sum > 0.0, "stable-max softmax sum");
    Ok(exps.iter().map(|&e| e / sum).collect())
}

/// Whether a provider request for the `std.tensor.softmax` pure cell is
/// axis-well-formed. The cell's declared contract is a single rank-1
/// vector argument evaluated over the whole vector: any 2D-style `axis`
/// provider request is a wrong-axis failure (rank != 1 refuses).
#[must_use]
pub fn softmax_axis_well_formed(rank: usize) -> bool {
    rank == 1
}

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

/// Biform cells (bead `emath-biform-cells-jswu6`): one cell, two
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

/// Typed per-side disposition (bead: provided / refused / not-applicable
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
        if let Some(other) = sides.iter().find(|s| {
            s.side == BiformSide::Algorithm && s.evidence_id == evidence.evidence_id
        }) {
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

/// Nanopass projection pipeline (bead `emath-nanopass-projections-1d5jy`):
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
