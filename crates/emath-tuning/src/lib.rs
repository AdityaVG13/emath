#![forbid(unsafe_code)]

//! Semantic and joint tuning.
//!
//! Semantic tuning varies World IR components (carriers, symbols, signature,
//! operator meanings, constants, constructors, laws, or effects/capabilities)
//! while protecting declared laws and held-out examples. Joint tuning varies
//! the world and the implementation together:
//!
//! ```text
//! candidate = WorldDelta + ExecutionDelta
//! ```
//!
//! Promotion requires (1) semantic admission, (2) evidence threshold,
//! (3) resource envelope (protected host metrics), (4) fallback
//! availability, and (5) a deterministic receipt. A world that merely
//! memorizes construction examples must not promote as a general meaning,
//! so every candidate is tested against held-out references before it can
//! be selected.

pub mod campaign;
pub mod frontier;

use emath_term::{Signature, SymbolId};
use emath_world_ir::{Fixity, MeaningHoleId, OperatorSemantics, SymbolDef, WorldId, WorldIr};

/// Separates prior and next payloads in an operational [`SemanticChange::description`].
pub const PATCH_SEPARATOR: char = '\u{1f}';

/// Encodes a reversible semantic patch (`prior` then `next`).
#[must_use]
pub fn encode_patch(prior: &str, next: &str) -> String {
    format!("{prior}{PATCH_SEPARATOR}{next}")
}

/// Which World IR component a tuning variable or semantic delta may vary.
///
/// Declaration order is the deterministic [`Ord`] order and matches the
/// World IR contract components (carriers, symbols, signature, meanings,
/// constants, constructors, laws, effects/capabilities).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SemanticVariableKind {
    /// A carrier/domain choice.
    Carrier,
    /// A symbol declaration (id, fixity, precedence, type scheme).
    Symbol,
    /// The first-order signature.
    Signature,
    /// An operator's meaning.
    Operator,
    /// A constant's meaning.
    Constant,
    /// A constructor contract.
    Constructor,
    /// A law's shape.
    Law,
    /// A declared effect or capability.
    Effect,
}

impl SemanticVariableKind {
    /// Deterministic canonical name.
    #[must_use]
    pub fn canonical(self) -> &'static str {
        match self {
            Self::Carrier => "carrier",
            Self::Symbol => "symbol",
            Self::Signature => "signature",
            Self::Operator => "operator",
            Self::Constant => "constant",
            Self::Constructor => "constructor",
            Self::Law => "law",
            Self::Effect => "effect",
        }
    }

    /// Inverse of [`Self::canonical`].
    #[must_use]
    pub fn from_canonical(name: &str) -> Option<Self> {
        Some(match name {
            "carrier" => Self::Carrier,
            "symbol" => Self::Symbol,
            "signature" => Self::Signature,
            "operator" => Self::Operator,
            "constant" => Self::Constant,
            "constructor" => Self::Constructor,
            "law" => Self::Law,
            "effect" => Self::Effect,
            _ => return None,
        })
    }

    /// World IR component this kind touches (field name on `WorldIr`).
    #[must_use]
    pub fn component(self) -> &'static str {
        match self {
            Self::Carrier => "carriers",
            Self::Symbol => "symbols",
            Self::Signature => "signature",
            Self::Operator => "operators",
            Self::Constant => "constants",
            Self::Constructor => "constructors",
            Self::Law => "laws",
            Self::Effect => "effects",
        }
    }
}

/// One tunable semantic variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticVariable {
    /// What is varied.
    pub kind: SemanticVariableKind,
    /// Symbol being varied, when the kind targets a symbol.
    pub symbol: Option<SymbolId>,
    /// Canonical domain text (allowable variation), e.g.
    /// `finite tables over Bool`.
    pub domain: String,
}

impl SemanticVariable {
    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "{}:{}:{}",
            self.kind.canonical(),
            self.symbol
                .as_ref()
                .map_or_else(String::new, |s| s.0.clone()),
            self.domain
        )
    }
}

/// One concrete world change proposed by a candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticChange {
    /// What the change targets.
    pub kind: SemanticVariableKind,
    /// Symbol targeted, when applicable.
    pub symbol: Option<SymbolId>,
    /// Canonical description of the new meaning.
    pub description: String,
    /// Provenance of the proposal (`synthesized`, `agent-proposal`,
    /// `evolutionary`, `fitted`, ...).
    pub provenance: String,
}

impl SemanticChange {
    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.kind.canonical(),
            self.symbol
                .as_ref()
                .map_or_else(String::new, |s| s.0.clone()),
            self.description,
            self.provenance
        )
    }

    /// Operational replace whose `description` encodes `prior` then `next`.
    ///
    /// [`WorldDelta::apply`] writes `next`; [`WorldDelta::revert`] restores `prior`.
    #[must_use]
    pub fn replace(
        kind: SemanticVariableKind,
        symbol: Option<SymbolId>,
        prior: impl Into<String>,
        next: impl Into<String>,
        provenance: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            symbol,
            description: encode_patch(&prior.into(), &next.into()),
            provenance: provenance.into(),
        }
    }
}

/// A world delta: a base world plus concrete semantic changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldDelta {
    /// Base world identity.
    pub base_world: WorldId,
    /// Concrete changes, sorted by canonical form.
    pub changes: Vec<SemanticChange>,
}

impl WorldDelta {
    /// Builds a delta with deterministically sorted changes.
    #[must_use]
    pub fn new(base_world: WorldId, mut changes: Vec<SemanticChange>) -> Self {
        changes.sort_by_key(SemanticChange::canonical);
        Self {
            base_world,
            changes,
        }
    }

    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "world-delta:base={}:{}",
            self.base_world.0,
            self.changes
                .iter()
                .map(SemanticChange::canonical)
                .collect::<Vec<_>>()
                .join(";")
        )
    }

    /// World IR components touched by this change set, sorted uniquely.
    #[must_use]
    pub fn locality(&self) -> Vec<&'static str> {
        locality_of(self.changes.iter().map(|change| change.kind))
    }

    /// Deterministic receipt over this delta's base fingerprint and changes.
    #[must_use]
    pub fn receipt(&self) -> DeltaReceipt {
        DeltaReceipt::new(self.base_world.0, &self.changes)
    }

    /// Applies this delta to `base`, returning a new world.
    ///
    /// `base.identity()` must equal [`Self::base_world`]. Each change targets an
    /// existing component; a missing target is [`DeltaError::MissingTarget`],
    /// never a silent no-op. A non-empty change set that does not alter
    /// content identity is refused. Apply is deterministic: changes run in
    /// canonical sort order.
    pub fn apply(&self, base: &WorldIr) -> Result<WorldIr, DeltaError> {
        apply_or_revert(self, base, false)
    }

    /// Inverse of [`Self::apply`]: restores the pre-image world.
    ///
    /// `revert(apply(base))` equals `base` in canonical form and identity.
    /// Operational changes encode a prior payload (via [`PATCH_SEPARATOR`] or,
    /// for constructor/law/effect, the target [`SymbolId`]); a description that
    /// is not reversible is [`DeltaError::NotReversible`].
    pub fn revert(&self, applied: &WorldIr) -> Result<WorldIr, DeltaError> {
        apply_or_revert(self, applied, true)
    }
}

/// Typed refusal while applying or reverting a [`WorldDelta`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaError {
    /// `base.identity()` did not match [`WorldDelta::base_world`].
    BaseMismatch {
        /// Identity recorded on the delta.
        expected: WorldId,
        /// Identity of the supplied world.
        actual: WorldId,
    },
    /// The addressed component is absent from the world.
    MissingTarget {
        /// Delta kind that failed to resolve.
        kind: SemanticVariableKind,
        /// Target id (symbol or list entry).
        target: String,
    },
    /// Live payload did not match the encoded prior (apply) or next (revert).
    PriorMismatch {
        /// Delta kind that failed the check.
        kind: SemanticVariableKind,
        /// Target id.
        target: String,
        /// Payload the delta expected to find.
        expected: String,
        /// Payload present on the world.
        actual: String,
    },
    /// Patch payload could not be parsed for this kind.
    MalformedPatch {
        /// Delta kind whose payload failed.
        kind: SemanticVariableKind,
        /// Human-readable parse reason.
        reason: String,
    },
    /// Revert was asked to invert a description that has no prior payload.
    NotReversible {
        /// Delta kind that cannot be inverted.
        kind: SemanticVariableKind,
    },
    /// A non-empty change set left [`WorldIr::identity`] unchanged.
    IdentityUnchanged,
    /// Revert did not restore [`WorldDelta::base_world`].
    DidNotRestore {
        /// Expected restored identity.
        expected: WorldId,
        /// Identity after revert.
        actual: WorldId,
    },
}

impl std::fmt::Display for DeltaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BaseMismatch { expected, actual } => {
                write!(
                    f,
                    "world delta base mismatch: expected {}, actual {}",
                    expected.0, actual.0
                )
            }
            Self::MissingTarget { kind, target } => {
                write!(
                    f,
                    "world delta target missing: {} {target}",
                    kind.canonical()
                )
            }
            Self::PriorMismatch {
                kind,
                target,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "world delta prior mismatch: {} {target}: expected {expected}, actual {actual}",
                    kind.canonical()
                )
            }
            Self::MalformedPatch { kind, reason } => {
                write!(
                    f,
                    "world delta malformed patch: {}: {reason}",
                    kind.canonical()
                )
            }
            Self::NotReversible { kind } => {
                write!(f, "world delta is not reversible: {}", kind.canonical())
            }
            Self::IdentityUnchanged => {
                write!(f, "world delta did not change WorldId")
            }
            Self::DidNotRestore { expected, actual } => {
                write!(
                    f,
                    "world delta revert did not restore identity: expected {}, actual {}",
                    expected.0, actual.0
                )
            }
        }
    }
}

impl std::error::Error for DeltaError {}

/// Receipt of a world delta: base fingerprint, applied changes, identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeltaReceipt {
    /// Base world fingerprint (`WorldIr::identity` / `WorldId.0`).
    pub base_fingerprint: u64,
    /// Applied change canonical strings, sorted.
    pub applied: Vec<String>,
    /// FNV-1a64 over [`Self::canonical`].
    pub identity: u64,
}

impl DeltaReceipt {
    /// Builds a receipt with a deterministic identity.
    ///
    /// The same `(base_fingerprint, changes)` pair always yields the same
    /// identity. Change order is normalized by sorting canonical strings.
    #[must_use]
    pub fn new(base_fingerprint: u64, changes: &[SemanticChange]) -> Self {
        let mut applied: Vec<String> = changes.iter().map(SemanticChange::canonical).collect();
        applied.sort();
        let receipt = Self {
            base_fingerprint,
            applied,
            identity: 0,
        };
        let identity = emath_world_ir::fnv1a64(receipt.canonical().as_bytes());
        Self {
            identity,
            ..receipt
        }
    }

    /// Deterministic canonical preimage for [`Self::identity`].
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "delta-receipt:base={}:{}",
            self.base_fingerprint,
            self.applied.join(";")
        )
    }

    /// World IR components touched by the applied changes, sorted uniquely.
    #[must_use]
    pub fn locality(&self) -> Vec<&'static str> {
        locality_of(self.applied.iter().filter_map(|applied| {
            applied
                .split(':')
                .next()
                .and_then(SemanticVariableKind::from_canonical)
        }))
    }
}

/// Sorted unique World IR component names for a set of delta kinds.
fn locality_of(kinds: impl IntoIterator<Item = SemanticVariableKind>) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = kinds
        .into_iter()
        .map(SemanticVariableKind::component)
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

fn apply_or_revert(
    delta: &WorldDelta,
    world: &WorldIr,
    reverse: bool,
) -> Result<WorldIr, DeltaError> {
    if !reverse {
        let actual = world.identity();
        if actual != delta.base_world {
            return Err(DeltaError::BaseMismatch {
                expected: delta.base_world,
                actual,
            });
        }
    }

    let mut next = world.clone();
    if reverse {
        for change in delta.changes.iter().rev() {
            apply_change(&mut next, change, true)?;
        }
    } else {
        for change in &delta.changes {
            apply_change(&mut next, change, false)?;
        }
    }

    let identity = next.identity();
    if reverse {
        if identity != delta.base_world {
            return Err(DeltaError::DidNotRestore {
                expected: delta.base_world,
                actual: identity,
            });
        }
    } else if !delta.changes.is_empty() && identity == delta.base_world {
        return Err(DeltaError::IdentityUnchanged);
    }
    Ok(next)
}

fn apply_change(
    world: &mut WorldIr,
    change: &SemanticChange,
    reverse: bool,
) -> Result<(), DeltaError> {
    let (expected, write) = directed_payloads(change, reverse)?;
    let target = change
        .symbol
        .as_ref()
        .ok_or_else(|| DeltaError::MissingTarget {
            kind: change.kind,
            target: String::new(),
        })?;
    match change.kind {
        SemanticVariableKind::Carrier => {
            apply_carrier(world, change.kind, &target.0, expected, write)
        }
        SemanticVariableKind::Symbol => apply_symbol(world, change.kind, target, expected, write),
        SemanticVariableKind::Signature => {
            apply_signature(world, change.kind, target, expected, write)
        }
        SemanticVariableKind::Operator => {
            apply_operator(world, change.kind, target, expected, write, false)
        }
        SemanticVariableKind::Constant => {
            apply_operator(world, change.kind, target, expected, write, true)
        }
        SemanticVariableKind::Constructor => {
            replace_list_item(&mut world.constructors, change.kind, expected, write)
        }
        SemanticVariableKind::Law => {
            replace_list_item(&mut world.laws, change.kind, expected, write)
        }
        SemanticVariableKind::Effect => {
            replace_list_item(&mut world.effects, change.kind, expected, write)
        }
    }
}

fn directed_payloads(
    change: &SemanticChange,
    reverse: bool,
) -> Result<(Option<&str>, &str), DeltaError> {
    let (prior, next) = operational_payloads(change)?;
    if reverse {
        let prior = prior.ok_or(DeltaError::NotReversible { kind: change.kind })?;
        Ok((Some(next), prior))
    } else {
        Ok((prior, next))
    }
}

fn operational_payloads(change: &SemanticChange) -> Result<(Option<&str>, &str), DeltaError> {
    match change.description.split_once(PATCH_SEPARATOR) {
        Some((prior, next)) => Ok((Some(prior), next)),
        None => match change.kind {
            SemanticVariableKind::Constructor
            | SemanticVariableKind::Law
            | SemanticVariableKind::Effect => {
                let prior = change
                    .symbol
                    .as_ref()
                    .ok_or_else(|| DeltaError::MissingTarget {
                        kind: change.kind,
                        target: String::new(),
                    })?;
                Ok((Some(prior.0.as_str()), change.description.as_str()))
            }
            _ => Ok((None, change.description.as_str())),
        },
    }
}

fn apply_carrier(
    world: &mut WorldIr,
    kind: SemanticVariableKind,
    name: &str,
    expected: Option<&str>,
    write: &str,
) -> Result<(), DeltaError> {
    let carrier = world
        .carriers
        .iter_mut()
        .find(|carrier| carrier.name == name)
        .ok_or_else(|| DeltaError::MissingTarget {
            kind,
            target: name.to_string(),
        })?;
    check_prior(kind, name, expected, &carrier.type_expression)?;
    carrier.type_expression = write.to_string();
    Ok(())
}

fn apply_symbol(
    world: &mut WorldIr,
    kind: SemanticVariableKind,
    target: &SymbolId,
    expected: Option<&str>,
    write: &str,
) -> Result<(), DeltaError> {
    let symbol = world
        .symbols
        .iter_mut()
        .find(|symbol| symbol.id == *target)
        .ok_or_else(|| DeltaError::MissingTarget {
            kind,
            target: target.0.clone(),
        })?;
    check_prior(kind, &target.0, expected, &encode_symbol_payload(symbol))?;
    write_symbol_payload(symbol, kind, write)
}

fn apply_signature(
    world: &mut WorldIr,
    kind: SemanticVariableKind,
    target: &SymbolId,
    expected: Option<&str>,
    write: &str,
) -> Result<(), DeltaError> {
    let actual = world
        .signature
        .arity(target)
        .ok_or_else(|| DeltaError::MissingTarget {
            kind,
            target: target.0.clone(),
        })?;
    check_prior(kind, &target.0, expected, &actual.to_string())?;
    let arity = parse_arity(kind, write)?;
    world.signature = with_arity(&world.signature, target, arity)?;
    Ok(())
}

fn apply_operator(
    world: &mut WorldIr,
    kind: SemanticVariableKind,
    target: &SymbolId,
    expected: Option<&str>,
    write: &str,
    constant_only: bool,
) -> Result<(), DeltaError> {
    if constant_only && !is_constant_symbol(world, target) {
        return Err(DeltaError::MissingTarget {
            kind,
            target: target.0.clone(),
        });
    }
    let operator = world
        .operators
        .iter_mut()
        .find(|operator| operator.symbol == *target)
        .ok_or_else(|| DeltaError::MissingTarget {
            kind,
            target: target.0.clone(),
        })?;
    check_prior(
        kind,
        &target.0,
        expected,
        &encode_semantics(&operator.semantics),
    )?;
    operator.semantics = decode_semantics(kind, write)?;
    Ok(())
}

fn replace_list_item(
    items: &mut [String],
    kind: SemanticVariableKind,
    expected: Option<&str>,
    write: &str,
) -> Result<(), DeltaError> {
    let from = expected.ok_or(DeltaError::NotReversible { kind })?;
    let index =
        items
            .iter()
            .position(|item| item == from)
            .ok_or_else(|| DeltaError::MissingTarget {
                kind,
                target: from.to_string(),
            })?;
    items[index] = write.to_string();
    Ok(())
}

fn check_prior(
    kind: SemanticVariableKind,
    target: &str,
    expected: Option<&str>,
    actual: &str,
) -> Result<(), DeltaError> {
    if let Some(expected) = expected {
        if expected != actual {
            return Err(DeltaError::PriorMismatch {
                kind,
                target: target.to_string(),
                expected: expected.to_string(),
                actual: actual.to_string(),
            });
        }
    }
    Ok(())
}

fn with_arity(
    signature: &Signature,
    symbol: &SymbolId,
    arity: usize,
) -> Result<Signature, DeltaError> {
    let mut next = Signature::default();
    for (id, existing) in signature.iter() {
        let value = if id == symbol { arity } else { *existing };
        next.insert(id.clone(), value)
            .map_err(|err| DeltaError::MalformedPatch {
                kind: SemanticVariableKind::Signature,
                reason: format!("{err:?}"),
            })?;
    }
    Ok(next)
}

fn parse_arity(kind: SemanticVariableKind, payload: &str) -> Result<usize, DeltaError> {
    payload.parse().map_err(|_| DeltaError::MalformedPatch {
        kind,
        reason: format!("arity is not a usize: {payload}"),
    })
}

fn is_constant_symbol(world: &WorldIr, symbol: &SymbolId) -> bool {
    world
        .symbols
        .iter()
        .any(|item| item.id == *symbol && item.fixity == Fixity::Constant)
        || world.signature.arity(symbol) == Some(0)
}

fn encode_symbol_payload(symbol: &SymbolDef) -> String {
    format!(
        "{}:{}:{}",
        fixity_name(symbol.fixity),
        symbol
            .precedence
            .map_or_else(|| "-".to_string(), |p| p.to_string()),
        symbol.type_scheme
    )
}

fn write_symbol_payload(
    symbol: &mut SymbolDef,
    kind: SemanticVariableKind,
    payload: &str,
) -> Result<(), DeltaError> {
    let mut parts = payload.splitn(3, ':');
    let Some(fixity_part) = parts.next() else {
        return Err(DeltaError::MalformedPatch {
            kind,
            reason: "empty symbol payload".to_string(),
        });
    };
    match (parts.next(), parts.next()) {
        (Some(precedence_part), Some(scheme)) => {
            symbol.fixity = parse_fixity(kind, fixity_part)?;
            symbol.precedence = if precedence_part == "-" {
                None
            } else {
                Some(
                    precedence_part
                        .parse()
                        .map_err(|_| DeltaError::MalformedPatch {
                            kind,
                            reason: format!("precedence is not a u16: {precedence_part}"),
                        })?,
                )
            };
            symbol.type_scheme = scheme.to_string();
        }
        _ => symbol.type_scheme = payload.to_string(),
    }
    Ok(())
}

fn fixity_name(fixity: Fixity) -> &'static str {
    match fixity {
        Fixity::Constant => "constant",
        Fixity::Prefix => "prefix",
        Fixity::Infix => "infix",
        Fixity::Postfix => "postfix",
        Fixity::Function => "function",
    }
}

fn parse_fixity(kind: SemanticVariableKind, name: &str) -> Result<Fixity, DeltaError> {
    match name {
        "constant" => Ok(Fixity::Constant),
        "prefix" => Ok(Fixity::Prefix),
        "infix" => Ok(Fixity::Infix),
        "postfix" => Ok(Fixity::Postfix),
        "function" => Ok(Fixity::Function),
        _ => Err(DeltaError::MalformedPatch {
            kind,
            reason: format!("unknown fixity: {name}"),
        }),
    }
}

fn encode_semantics(semantics: &OperatorSemantics) -> String {
    match semantics {
        OperatorSemantics::StructuralConstructor => "structural".to_string(),
        OperatorSemantics::DeclaredExpression(text) => format!("expr:{text}"),
        OperatorSemantics::FiniteTable(rows) => format!("table:{}", rows.join("\u{1e}")),
        OperatorSemantics::ProviderBinding(id) => format!("provider:{id}"),
        OperatorSemantics::Synthesized { program, receipt } => {
            format!("synth:{program}\u{1e}{receipt}")
        }
        OperatorSemantics::Parametric(MeaningHoleId(id)) => format!("hole:{id}"),
    }
}

fn decode_semantics(
    kind: SemanticVariableKind,
    payload: &str,
) -> Result<OperatorSemantics, DeltaError> {
    if payload == "structural" {
        return Ok(OperatorSemantics::StructuralConstructor);
    }
    if let Some(text) = payload.strip_prefix("expr:") {
        return Ok(OperatorSemantics::DeclaredExpression(text.to_string()));
    }
    if let Some(rows) = payload.strip_prefix("table:") {
        return Ok(OperatorSemantics::FiniteTable(if rows.is_empty() {
            Vec::new()
        } else {
            rows.split('\u{1e}').map(str::to_string).collect()
        }));
    }
    if let Some(id) = payload.strip_prefix("provider:") {
        return Ok(OperatorSemantics::ProviderBinding(id.to_string()));
    }
    if let Some(rest) = payload.strip_prefix("synth:") {
        let (program, receipt) =
            rest.split_once('\u{1e}')
                .ok_or_else(|| DeltaError::MalformedPatch {
                    kind,
                    reason: "synthesized payload needs program and receipt".to_string(),
                })?;
        return Ok(OperatorSemantics::Synthesized {
            program: program.to_string(),
            receipt: receipt.to_string(),
        });
    }
    if let Some(id) = payload.strip_prefix("hole:") {
        let id = id.parse().map_err(|_| DeltaError::MalformedPatch {
            kind,
            reason: format!("hole id is not a u64: {id}"),
        })?;
        return Ok(OperatorSemantics::Parametric(MeaningHoleId(id)));
    }
    Ok(OperatorSemantics::DeclaredExpression(payload.to_string()))
}

/// An implementation delta: lowering, precision, provider, target, schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionDelta {
    /// Lowering/algorithm label, e.g. `polynomial`.
    pub lowering: String,
    /// Precision label, e.g. `f64`, `f32_bounded`.
    pub precision: String,
    /// Provider label, e.g. `native`, `dew`.
    pub provider: String,
    /// Target label, e.g. `cpu.simd`, `gpu.wgsl`.
    pub target: String,
    /// Schedule label, e.g. `shadow-first`.
    pub schedule: String,
}

impl ExecutionDelta {
    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "exec-delta:{}/{}:{}:{}:{}",
            self.lowering, self.precision, self.provider, self.target, self.schedule
        )
    }
}

/// A joint tuning candidate: world delta plus execution delta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JointCandidate {
    /// Stable label.
    pub label: String,
    /// World changes.
    pub world: WorldDelta,
    /// Implementation changes.
    pub execution: ExecutionDelta,
    /// Whether the candidate passed the held-out challenge.
    pub held_out_verified: bool,
    /// Evidence units admitting the candidate semantics.
    pub evidence_units: u32,
    /// Candidate content identity (FNV-1a64 over canonical form).
    pub identity: u64,
}

impl JointCandidate {
    /// Builds a candidate with deterministic identity.
    #[must_use]
    pub fn new(
        label: impl Into<String>,
        world: WorldDelta,
        execution: ExecutionDelta,
        held_out_verified: bool,
        evidence_units: u32,
    ) -> Self {
        let label = label.into();
        let candidate = Self {
            label,
            world,
            execution,
            held_out_verified,
            evidence_units,
            identity: 0,
        };
        let identity = emath_world_ir::fnv1a64(candidate.canonical().as_bytes());
        Self {
            identity,
            ..candidate
        }
    }

    /// Deterministic canonical form.
    #[must_use]
    pub fn canonical(&self) -> String {
        format!(
            "candidate:{}:{}:{}:held-out={}:evidence={}",
            self.label,
            self.world.canonical(),
            self.execution.canonical(),
            self.held_out_verified,
            self.evidence_units
        )
    }
}

/// Construction vs held-out coverage used to recalibrate meaning confidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoverageSample {
    /// Construction coverage in permille.
    pub construction_permille: u64,
    /// Held-out coverage in permille.
    pub held_out_permille: u64,
    /// Fitted table cell count (description complexity).
    pub table_cells: u64,
    /// Construction example count.
    pub construction_examples: u64,
}

/// Recalibrated meaning confidence after held-out challenge and complexity penalty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalibratedConfidence {
    /// Confidence in permille after subtracting the complexity penalty.
    pub permille: u64,
    /// Penalty for unused table capacity (memorization headroom).
    pub complexity_penalty_permille: u64,
    /// Whether the candidate is admitted as a general meaning.
    pub admitted: bool,
    /// Machine-readable reason: `construction:no-coverage`,
    /// `held-out:memorization`, `complexity-penalty`, or `passed`.
    pub reason: String,
}

/// Minimum held-out coverage (permille) to count as general rather than memorized.
pub const MIN_HELD_OUT_PERMILLE: u64 = 800;

/// Recalibrates meaning confidence against held-out outcomes.
///
/// A memorizing candidate (high construction, low held-out) is refused.
/// An oversized table relative to construction examples is penalized so a
/// lookup table that fits training rows cannot claim generality.
#[must_use]
pub fn calibrate_confidence(sample: CoverageSample) -> CalibratedConfidence {
    let complexity_penalty_permille = if sample.construction_examples == 0 {
        1000
    } else if sample.table_cells <= sample.construction_examples {
        0
    } else {
        ((sample.table_cells - sample.construction_examples) * 1000) / sample.table_cells
    };
    let held_out_after_penalty = sample
        .held_out_permille
        .saturating_sub(complexity_penalty_permille);
    let (admitted, reason) = if sample.construction_permille == 0 {
        (false, "construction:no-coverage")
    } else if sample.held_out_permille < MIN_HELD_OUT_PERMILLE {
        (false, "held-out:memorization")
    } else if held_out_after_penalty < MIN_HELD_OUT_PERMILLE {
        (false, "complexity-penalty")
    } else {
        (true, "passed")
    };
    CalibratedConfidence {
        permille: held_out_after_penalty.min(sample.construction_permille),
        complexity_penalty_permille,
        admitted,
        reason: reason.to_string(),
    }
}
