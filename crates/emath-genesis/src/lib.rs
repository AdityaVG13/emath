#![forbid(unsafe_code)]

//! Minimal Semantic Genesis evaluator and built-in example worlds.
//!
//! Hosts the G1 world-side stage: [`forest`] builds the bounded parse
//! forest and infers world signatures; `emath-syntax` keeps the G0
//! parser and re-exports this module at its root.
//!
//! Facade SPI note (rpme/C054): each family module carries its own
//! `check_version` (analogue/binder/meaning_provider/morphism/synth/
//! tuning) with its OWN error type — six deliberate homonyms, not an
//! incomplete facade. A root export would either collide six times or
//! need six aliases; the module path IS the family address. Documented
//! intentional SPI, like `core::tree` (C051).

pub mod analogue;
pub mod binder;
pub mod csa;
pub mod forest;
pub mod specialization;
pub mod synth;
pub mod meaning_provider;
pub mod morphism;
pub mod vm;
pub mod joint_tuning;
pub mod world_decl;
pub mod world_result;

pub use world_result::{
    Disposition, NakedResultRefusal, ResultBundle, WorldResult, WORLD_RESULT_SCHEMA,
    WORLD_RESULT_VERSION, evaluate_labeled,
};

pub use analogue::{
    ANALOGUE_NO_CLAIM, ANALOGUE_SCHEMA, ANALOGUE_VERSION, AnalogueDomain, AnalogueError,
    AnalogueReceipt, AnalogueRequest, AnalogueSample, AnalogueVerdict, analogue_id,
};
pub use binder::{
    BINDER_SCHEMA, BINDER_VERSION, BinderBudget, BinderDomain, BinderError, BinderFamily,
    BinderKind, BinderTerm, ScopedBinder, binder_id,
};
pub use csa::{CSA_MEANING_CLAIM, CSA_SCHEMA, CSA_SCHEMA_VERSION, OnePointWorld, SeededCsaWorld};
pub use specialization::{SpecializationCache, SpecializationChallenge, SpecializationStats};
pub use synth::{
    MAX_CARRIER_SIZE, SYNTH_SCHEMA, SYNTH_VERSION, LawViolation, OpTable, SynthBudget, SynthError,
    SynthExample, SynthLaw, SynthReceipt, SynthRequest, check_table, synth_id,
};
pub use meaning_provider::{
    admit, challenge, proposal_id, AdmissionStatus, AgentProposal, ChallengeRefusal,
    ChallengeStatus, MeaningChecker, MeaningReceipt, MeaningVerdict, ProviderError,
    QuarantinedCandidate, CheckedCandidate, AUTHORITY_NONE, AUTHORITY_STRUCTURAL_CHECKED,
    PROVIDER_SCHEMA, PROVIDER_VERSION, REQUIRED_CAPABILITY,
};
pub use morphism::{
    MAX_ISO_CANDIDATES, MAX_ISO_SEARCH_SIZE, MORPHISM_SCHEMA, MORPHISM_VERSION, DedupeGroup,
    DedupeReceipt, DroppedDuplicate, InvariantReport, LawPortfolioVerdict, MorphismError,
    MorphismViolation, QuotientReceipt, WorldMorphism, dedupe, find_isomorphism, mine_invariants,
    morphism_id, quotient, verify,
};
pub use vm::{
    VM_SCHEMA, VM_SCHEMA_VERSION, VmBudget, VmContinuation, VmOutcome, VmTrace, resume, run,
};
pub use world_decl::{
    MAX_WORLD_SIZE, ModelClaim, OperationTable, SynthesisError, UserDefinedWorld, WorldDecl,
    WorldDeclError, WorldLaw, WorldOrigin, WorldSourceClass, attach_world, synthesize_world,
    user_defined_world,
};
pub use joint_tuning::{
    candidate_id, classify, semantic_dna, tune, tuning_id, CandidateStatus, Disqualification,
    HostExample, ImplVariant, ProtectedObjective, TuningBudget, TuningError, TuningReceipt,
    TuningRequest, IMPL_VARIANT_COUNT, TUNING_SCHEMA, TUNING_VERSION,
};

use std::collections::BTreeMap;
use std::fmt;

use emath_term::{Signature, SymbolId, Term, VariableId};
use emath_world_ir::{
    CarrierDef, Fixity, MeaningOrigin, OperatorDef, OperatorSemantics, SymbolDef, WorldIr,
};

/// Environment for free variables.
pub type Environment<V> = BTreeMap<VariableId, V>;

/// Generic first-order world implementation (the World ABI, fjxh.7):
/// carrier ([`Self::Value`]), constants, variables (via
/// [`Environment`] in [`evaluate`]), apply, effects, budgets
/// ([`evaluate_bounded`]) and evidence. A NEW world implements this
/// trait only — the evaluator gains no match arm for it.
pub trait FirstOrderWorld {
    /// Runtime value.
    type Value: Clone;
    /// Evaluation error.
    type Error;

    /// Resolves a nullary symbol.
    fn constant(&self, symbol: &SymbolId) -> Result<Self::Value, Self::Error>;

    /// Applies an operator to evaluated arguments.
    fn apply(
        &self,
        operator: &SymbolId,
        arguments: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;

    /// Whether this world binds every symbol of `signature` with the
    /// declared arity (portfolio applicability). Defaults to `false`:
    /// a world claims a signature explicitly, never by omission.
    fn admits(&self, signature: &Signature) -> bool {
        let _ = signature;
        false
    }

    /// Declared world effects. Seed worlds are pure: the default is the
    /// empty list; an effectful world must name its effects here.
    fn effects(&self) -> &'static [&'static str] {
        &[]
    }

    /// Stable evidence record for result bundles (fjxh.8: no naked
    /// answers — every custom-world value names the world that produced
    /// it).
    fn evidence(&self) -> WorldEvidence;
}

/// Stable per-world evidence: identity, origin class, and claimed laws.
/// OWNED: runtime-authored worlds (fjxh.13) cannot borrow `'static`
/// names; static seed worlds keep their one-line shape through
/// [`WorldEvidence::seed`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldEvidence {
    /// Stable world token (`free-symbolic`, `boolean-alien`, …).
    pub world: String,
    /// Origin class (`seed`, `user-defined`, `synthesized`).
    pub origin: String,
    /// Laws the world claims (checked by law-checking beads, not here).
    pub laws: Vec<String>,
}

impl WorldEvidence {
    /// One-line constructor for static seed worlds.
    #[must_use]
    pub fn seed(world: &'static str, laws: &'static [&'static str]) -> Self {
        Self {
            world: world.to_string(),
            origin: "seed".to_string(),
            laws: laws.iter().map(|law| (*law).to_string()).collect(),
        }
    }
}

/// World-evaluation budget (ABI budget seam): resource exhaustion is the
/// typed [`EvalError::BudgetExhausted`] — never a partial value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldBudget {
    /// Maximum evaluation steps (node visits).
    pub max_steps: u32,
}

/// Whether `signature` binds exactly the reference alien symbol set with
/// matching arities (the concrete seed worlds' admitted signature).
#[must_use]
pub fn is_reference_alien_signature(signature: &Signature) -> bool {
    let (reference, _) = reference_alien_term();
    reference.iter().eq(signature.iter())
}

/// Portfolio identity of the default custom worlds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WorldName {
    /// Free symbolic: values remain terms; always applicable.
    FreeSymbolic,
    /// Boolean interpretation of the reference alien signature.
    BooleanAlien,
    /// Modular-17 interpretation of the reference alien signature.
    ModularAlien,
}

impl WorldName {
    /// Stable token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FreeSymbolic => "free-symbolic",
            Self::BooleanAlien => "boolean-alien",
            Self::ModularAlien => "modular-17",
        }
    }
}

/// Default custom portfolio order (bead doctrine): free symbolic, then
/// the canonical-finite concrete worlds — Boolean when applicable,
/// modular when applicable. A typed disposition replaces a silent
/// fallthrough when nothing applicable remains.
#[must_use]
pub fn default_portfolio_order() -> &'static [WorldName] {
    &[WorldName::FreeSymbolic, WorldName::BooleanAlien, WorldName::ModularAlien]
}

/// Typed portfolio disposition: the selected world (if any) plus the
/// per-candidate verdict trail (`applicable` / `not applicable` /
/// `excluded`). `selected == None` is a typed refusal, never an
/// invented world.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldDisposition {
    /// The world selected from the order, if one applies.
    pub selected: Option<WorldName>,
    /// Per-candidate verdicts in portfolio order.
    pub trail: Vec<String>,
}

/// Select a world for `signature` from the default portfolio, skipping
/// every name in `exclude` (the caller's demand for a different carrier).
/// Doctrine order is followed; EVERY candidate's verdict is recorded in
/// the trail (evidence, never swallowed) and the first applicable world
/// in order is selected.
#[must_use]
pub fn select_world(signature: &Signature, exclude: &[WorldName]) -> WorldDisposition {
    let mut trail = Vec::new();
    let mut selected = None;
    for name in default_portfolio_order() {
        if exclude.contains(name) {
            trail.push(format!("{}: excluded", name.as_str()));
            continue;
        }
        let applicable = match name {
            WorldName::FreeSymbolic => FreeTermWorld.admits(signature),
            WorldName::BooleanAlien => BooleanAlienWorld.admits(signature),
            WorldName::ModularAlien => ModularAlienWorld.admits(signature),
        };
        trail.push(format!(
            "{}: {}",
            name.as_str(),
            if applicable { "applicable" } else { "not applicable" }
        ));
        if applicable && selected.is_none() {
            selected = Some(*name);
        }
    }
    WorldDisposition { selected, trail }
}

/// Evaluation error shared by the seed worlds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalError {
    /// Missing free-variable valuation.
    MissingVariable(VariableId),
    /// Unknown symbol.
    UnknownSymbol(SymbolId),
    /// Incorrect runtime arity.
    Arity {
        /// Symbol.
        symbol: SymbolId,
        /// Expected arity.
        expected: usize,
        /// Actual arity.
        actual: usize,
    },
    /// World-evaluation budget exhausted before completion; no partial
    /// value escapes (World ABI budget seam, fjxh.7).
    BudgetExhausted {
        /// Steps successfully executed before the refusal.
        steps: u32,
    },
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for EvalError {}

/// Evaluates a term in any world implementing its symbols.
pub fn evaluate<W: FirstOrderWorld>(
    term: &Term,
    world: &W,
    environment: &Environment<W::Value>,
) -> Result<W::Value, W::Error>
where
    W::Error: From<EvalError>,
{
    evaluate_bounded(term, world, environment, WorldBudget { max_steps: u32::MAX })
}

/// [`evaluate`] under an explicit [`WorldBudget`] (ABI budget seam):
/// node-visits are metered and exhaustion is the typed
/// [`EvalError::BudgetExhausted`] — never a partial value.
pub fn evaluate_bounded<W: FirstOrderWorld>(
    term: &Term,
    world: &W,
    environment: &Environment<W::Value>,
    budget: WorldBudget,
) -> Result<W::Value, W::Error>
where
    W::Error: From<EvalError>,
{
    evaluate_counted(term, world, environment, budget).map(|(value, _)| value)
}

/// [`evaluate_bounded`] with the step count: the producer seam for the
/// world-result envelope's `cost_steps` label (fjxh.8).
pub fn evaluate_counted<W: FirstOrderWorld>(
    term: &Term,
    world: &W,
    environment: &Environment<W::Value>,
    budget: WorldBudget,
) -> Result<(W::Value, u32), W::Error>
where
    W::Error: From<EvalError>,
{
    let mut steps = 0_u32;
    let result = evaluate_metered(term, world, environment, budget, &mut steps)?;
    Ok((result, steps))
}

fn evaluate_metered<W: FirstOrderWorld>(
    term: &Term,
    world: &W,
    environment: &Environment<W::Value>,
    budget: WorldBudget,
    steps: &mut u32,
) -> Result<W::Value, W::Error>
where
    W::Error: From<EvalError>,
{
    let visited = steps.checked_add(1).ok_or(EvalError::BudgetExhausted {
        steps: *steps,
    })?;
    if visited > budget.max_steps {
        return Err(EvalError::BudgetExhausted { steps: *steps }.into());
    }
    *steps = visited;
    match term {
        Term::Variable(variable) => environment
            .get(variable)
            .cloned()
            .ok_or_else(|| EvalError::MissingVariable(variable.clone()).into()),
        Term::Constant(symbol) => world.constant(symbol),
        Term::Apply {
            operator,
            arguments,
        } => {
            let values = arguments
                .iter()
                .map(|argument| evaluate_metered(argument, world, environment, budget, steps))
                .collect::<Result<Vec<_>, _>>()?;
            world.apply(operator, values)
        }
    }
}

/// Returns a provider-neutral free symbolic World IR.
#[must_use]
pub fn free_symbolic_world(name: &str, signature: Signature) -> WorldIr {
    let symbols = signature
        .iter()
        .map(|(symbol, arity)| SymbolDef {
            id: symbol.clone(),
            display: symbol.0.clone(),
            fixity: if *arity == 0 {
                Fixity::Constant
            } else {
                Fixity::Function
            },
            precedence: None,
            type_scheme: format!("Term^{arity} -> Term"),
        })
        .collect::<Vec<_>>();
    let operators = signature
        .iter()
        .map(|(symbol, _)| OperatorDef {
            symbol: symbol.clone(),
            semantics: OperatorSemantics::StructuralConstructor,
            origin: MeaningOrigin::Derived,
        })
        .collect::<Vec<_>>();
    WorldIr {
        version: 1,
        name: name.into(),
        signature,
        carriers: vec![CarrierDef {
            name: "Term".into(),
            type_expression: "FreeTerm".into(),
        }],
        symbols,
        operators,
        constructors: vec!["Term::Variable/Constant/Apply".into()],
        laws: vec!["structural-totality".into()],
        effects: vec![],
        holes: vec![],
        capabilities: vec!["pure".into()],
    }
}

/// A free symbolic world whose values remain terms.
#[derive(Debug, Default, Clone, Copy)]
pub struct FreeTermWorld;

impl FirstOrderWorld for FreeTermWorld {
    type Value = Term;
    type Error = EvalError;

    fn constant(&self, symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
        Ok(Term::Constant(symbol.clone()))
    }

    fn apply(
        &self,
        operator: &SymbolId,
        arguments: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error> {
        Ok(Term::Apply {
            operator: operator.clone(),
            arguments,
        })
    }

    fn admits(&self, _signature: &Signature) -> bool {
        // The free symbolic world binds any signature structurally.
        true
    }

    fn evidence(&self) -> WorldEvidence {
        WorldEvidence::seed("free-symbolic", &["structural-totality"])
    }
}

/// Boolean interpretation for the reference alien signature.
#[derive(Debug, Default, Clone, Copy)]
pub struct BooleanAlienWorld;

impl FirstOrderWorld for BooleanAlienWorld {
    type Value = bool;
    type Error = EvalError;

    fn constant(&self, symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
        match symbol.0.as_str() {
            "ζ" => Ok(true),
            _ => Err(EvalError::UnknownSymbol(symbol.clone())),
        }
    }

    fn apply(
        &self,
        operator: &SymbolId,
        arguments: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error> {
        match (operator.0.as_str(), arguments.as_slice()) {
            ("⋈", [left, right]) => Ok(*left ^ *right),
            ("⧖", [value]) => Ok(!*value),
            ("⊛", [left, right]) => Ok(*left && *right),
            ("⋈" | "⊛", values) => Err(EvalError::Arity {
                symbol: operator.clone(),
                expected: 2,
                actual: values.len(),
            }),
            ("⧖", values) => Err(EvalError::Arity {
                symbol: operator.clone(),
                expected: 1,
                actual: values.len(),
            }),
            _ => Err(EvalError::UnknownSymbol(operator.clone())),
        }
    }

    fn admits(&self, signature: &Signature) -> bool {
        is_reference_alien_signature(signature)
    }

    fn evidence(&self) -> WorldEvidence {
        WorldEvidence::seed("boolean-alien", &["xor-not-and-table"])
    }
}

/// Modular-17 interpretation for the reference alien signature.
#[derive(Debug, Default, Clone, Copy)]
pub struct ModularAlienWorld;

impl FirstOrderWorld for ModularAlienWorld {
    type Value = i64;
    type Error = EvalError;

    fn constant(&self, symbol: &SymbolId) -> Result<Self::Value, Self::Error> {
        match symbol.0.as_str() {
            "ζ" => Ok(3),
            _ => Err(EvalError::UnknownSymbol(symbol.clone())),
        }
    }

    fn apply(
        &self,
        operator: &SymbolId,
        arguments: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error> {
        let modulo = |value: i64| value.rem_euclid(17);
        match (operator.0.as_str(), arguments.as_slice()) {
            ("⋈", [left, right]) => Ok(modulo(*left + *right)),
            ("⧖", [value]) => Ok(modulo(*value * *value)),
            ("⊛", [left, right]) => Ok(modulo(*left * *right)),
            ("⋈" | "⊛", values) => Err(EvalError::Arity {
                symbol: operator.clone(),
                expected: 2,
                actual: values.len(),
            }),
            ("⧖", values) => Err(EvalError::Arity {
                symbol: operator.clone(),
                expected: 1,
                actual: values.len(),
            }),
            _ => Err(EvalError::UnknownSymbol(operator.clone())),
        }
    }

    fn admits(&self, signature: &Signature) -> bool {
        is_reference_alien_signature(signature)
    }

    fn evidence(&self) -> WorldEvidence {
        WorldEvidence::seed("modular-17", &["ring-mod-17-table"])
    }
}

/// Constructs the reference term and signature.
#[must_use]
pub fn reference_alien_term() -> (Signature, Term) {
    let mut signature = Signature::default();
    // ubs:ignore — static distinct symbols; Signature::insert only errs on arity conflict.
    let _ = signature.insert(SymbolId("⧖".into()), 1);
    let _ = signature.insert(SymbolId("⋈".into()), 2);
    let _ = signature.insert(SymbolId("⊛".into()), 2);
    let _ = signature.insert(SymbolId("ζ".into()), 0);

    let joined = Term::Apply {
        operator: SymbolId("⋈".into()),
        arguments: vec![
            Term::Variable(VariableId("a".into())),
            Term::Variable(VariableId("b".into())),
        ],
    };
    let transformed = Term::Apply {
        operator: SymbolId("⧖".into()),
        arguments: vec![joined],
    };
    let term = Term::Apply {
        operator: SymbolId("⊛".into()),
        arguments: vec![transformed, Term::Constant(SymbolId("ζ".into()))],
    };
    (signature, term)
}

pub mod tuning;
