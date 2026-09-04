//! Capability cells: classes, schemas, admission.

use super::*;

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
    /// certify a particular backend.
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
            Self::BumpAndNote { note } => !note.is_empty() && to_version != from_version,
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
pub fn softmax_reference_strict_f64(logits: &[f64]) -> Result<Vec<f64>, AdmissionRefusal> {
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
