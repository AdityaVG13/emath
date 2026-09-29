//! The headless research-loop host core (bead emath-kz3ll).
//!
//! A host opens a module's authored session surface - the `Step<Name>`
//! function that runs one `research_batch` over explicit state with
//! the target's closures lifted to module level, paired with the
//! `Seed<Name>` function that builds the initial state - and drives it
//! one batch per call. The host owns no loop policy: proposal order,
//! charging, freshness, promotion, and verdicts are the authored
//! module's. The host owns session bookkeeping: batch ledger
//! projection, the scratch checkpoint, the module-semantic ledger law
//! (the state's own batch counter equals the committed revision), and
//! the named lift diagnostic when a module has no surface to drive.
//!
//! State contract: every state carries Int `batch` and `verdict`. The
//! research fields are optional: `used` (0 when absent), `archive` with
//! `incumbent` (a plain loop state projects no incumbent or records),
//! and `case_set` (no frozen cases; `grow_case` refuses).
//!
//! Error model: `HostFault` carries a stable `code`. Engine and scratch
//! faults pass through with their own codes (`ConstructorError.code`,
//! e.g. `scratch_identity`, `scratch_ledger`); host-owned refusals use
//! `loop_*` codes. No silent guessing: an ambiguous or missing surface
//! refuses with what to author.
//!
//! Determinism class: identical module, surface, budget schedule, and
//! scratch produce identical sessions (the engine is deterministic and
//! the ledger projection reads state fields only).

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use emath_core::tree::{Declaration, GenericArg, Item, StmtKind, SyntaxTree, TypeExpr, TypeKind};
use emath_exec_ir::constructor_layer::{
    evaluate_function_carrying, fact_identity, load_scratch, prepare_module, save_scratch, BatchLedgerEntry, PreparedModule,
    CValue, ConstructorError, ScratchIdentity, DEFAULT_WORK,
};
use emath_exec_ir::exact_int::ExactInt;
use emath_sema::session::CompilerSession;
use emath_syntax::parse_str;

/// The engine image label a scratch carries as its language identity
/// (the constructor-layer checkpoint family's own constant).
const ENGINE_IMAGE: &str = emath_exec_ir::constructor_layer::IMAGE_IDENTITY;

/// The verdict vocabulary, for display only; the numbers are the
/// authored module's data.
#[must_use]
pub fn verdict_name(verdict: i128) -> &'static str {
    match verdict {
        0 => "running",
        1 => "goal_attained",
        2 => "domain_exhausted",
        3 => "plateau",
        4 => "budget_exhausted",
        _ => "unknown",
    }
}

/// A typed host refusal. Engine and scratch faults keep their own
/// codes; host-owned refusals use `loop_*`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostFault {
    pub code: String,
    pub message: String,
}

impl HostFault {
    /// Mint a host-owned `loop_*` refusal (also used by the REPL for
    /// stream and command faults).
    pub(crate) fn fault(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for HostFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl From<ConstructorError> for HostFault {
    fn from(err: ConstructorError) -> Self {
        Self {
            code: err.code,
            message: err.message,
        }
    }
}

/// What to author when a module has no session surface: the named
/// diagnostic, with the complete worked example to copy from.
const SURFACE_LIFT: &str = r"a loop host drives one batch per call through an authored session surface:
  - `emath function Step<Name>` with inputs (state: LoopState, budget: Int)
    and exactly one output of the same state type; its body passes the
    target's module-level lifted closures to research_batch;
  - `emath function Seed<Name>` with exactly one Int input and one
    LoopState output; its body builds the initial state with research_seed.
The complete worked example (the valley target lifted) is
tests/fixtures/constructor/research_step_valley.emath - copy its
Make*/Seed*/Step* shape for your target:

    emath function StepValley:
        inputs:
            state: LoopState
            budget: Int
        outputs:
            result: LoopState
        definitions:
            value_of = MakeValueOf(0)
            # ... the target's lifted closures, one Make function each ...
            result = research_batch(state, value_of, admit, moves, family_of,
                                    quota, probe, verify, witness_of, hard_ok,
                                    goal_met, better, next_cases, budget)

    emath function SeedValley:
        inputs:
            unused: Int
        outputs:
            result: LoopState
        definitions:
            value_of = MakeValueOf(0)
            val = MakeLandscape(0)
            witness_of = MakeWitness(0)
            result = research_seed(0, value_of, CaseSet: {ids: []}, val,
                                   witness_of, target_id, evaluator_id,
                                   input_id, mode)";

/// One discovered session surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionSurface {
    /// The one-batch Step function (`StepValley`).
    pub step: String,
    /// The initial-state Seed function (`SeedValley`).
    pub seed: String,
    /// The loop state record type both functions carry (`LoopState`).
    pub state_type: String,
    /// The Step function's state input name (author-chosen).
    pub step_state_input: String,
    /// The Step function's budget input name (author-chosen).
    pub step_budget_input: String,
    /// The Step function's optional third input of type
    /// `sequence(Observation)`: every observation the host ingested.
    pub step_evidence_input: Option<String>,
    /// The Seed function's single Int input name (author-chosen).
    pub seed_input: String,
}

/// The result of scanning a module for session surfaces: complete
/// `Step`/`Seed` pairs, plus every near-miss as a problem string
/// (a `Step` with no `Seed`, a `Step`-named function with the wrong
/// signature, a state-type mismatch between a pair). Near-misses are
/// reported, never silently ignored: a typo'd surface must surface.
#[derive(Clone, Debug, Default)]
pub struct SurfaceScan {
    pub surfaces: Vec<SessionSurface>,
    pub problems: Vec<String>,
}

fn path_type_name(ty: &TypeExpr) -> Option<&str> {
    match &ty.kind {
        TypeKind::Path { segments, .. } => segments.last().map(String::as_str),
        _ => None,
    }
}

/// `(name, type name)` pairs of one section's field declarations.
pub(crate) fn section_fields(decl: &Declaration, section: &str) -> Vec<(String, Option<String>)> {
    decl.sections()
        .filter(|candidate| candidate.name == section)
        .flat_map(|candidate| candidate.suite.statements.iter())
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::FieldDecl { name, ty, .. } => {
                Some((name.clone(), path_type_name(ty).map(str::to_string)))
            }
            _ => None,
        })
        .collect()
}

/// Whether input `index` of `decl` is declared `sequence(Observation)`.
fn is_evidence_input(decl: &Declaration, index: usize) -> bool {
    let Some(ty) = decl
        .sections()
        .filter(|candidate| candidate.name == "inputs")
        .flat_map(|candidate| candidate.suite.statements.iter())
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::FieldDecl { ty, .. } => Some(ty),
            _ => None,
        })
        .nth(index)
    else {
        return false;
    };
    match &ty.kind {
        TypeKind::Path { segments, generic_args } => {
            segments.last().map(String::as_str) == Some("sequence")
                && matches!(
                    generic_args.as_slice(),
                    [GenericArg::Type(element)] if path_type_name(element) == Some("Observation")
                )
        }
        _ => false,
    }
}

/// Scan a parsed module for `Step`/`Seed` session surfaces.
#[must_use]
pub fn scan_session_surfaces(tree: &SyntaxTree) -> SurfaceScan {
    let mut scan = SurfaceScan::default();
    // (suffix, (step name, state input, budget input, state type, evidence input))
    let mut steps: BTreeMap<String, (String, String, String, String, Option<String>)> =
        BTreeMap::new();
    let mut seeds: BTreeMap<String, (String, String, String)> = BTreeMap::new();

    for item in &tree.items {
        let Item::Declaration(decl) = item else {
            continue;
        };
        if decl.as_kind != "function" {
            continue;
        }
        if let Some(suffix) = decl.name.strip_prefix("Step") {
            let inputs = section_fields(decl, "inputs");
            let outputs = section_fields(decl, "outputs");
            let evidence = (inputs.len() == 3 && is_evidence_input(decl, 2))
                .then(|| inputs[2].0.clone());
            let step_shape = (inputs.len() == 2 || evidence.is_some())
                && inputs[0].1.is_some()
                && inputs[1].1.as_deref() == Some("Int")
                && outputs.len() == 1
                && outputs[0].1.is_some()
                && outputs[0].1 == inputs[0].1;
            if step_shape {
                steps.insert(
                    suffix.to_string(),
                    (
                        decl.name.clone(),
                        inputs[0].0.clone(),
                        inputs[1].0.clone(),
                        inputs[0].1.clone().unwrap_or_default(),
                        evidence,
                    ),
                );
            } else {
                scan.problems.push(format!(
                    "`{}` is named like a session Step but its signature is not \
                     (state: T, budget: Int[, evidence: sequence(Observation)]) -> result: T",
                    decl.name
                ));
            }
        } else if let Some(suffix) = decl.name.strip_prefix("Seed") {
            let inputs = section_fields(decl, "inputs");
            let outputs = section_fields(decl, "outputs");
            let seed_shape =
                inputs.len() == 1 && inputs[0].1.as_deref() == Some("Int") && outputs.len() == 1;
            if seed_shape {
                seeds.insert(
                    suffix.to_string(),
                    (decl.name.clone(), inputs[0].0.clone(), outputs[0].1.clone().unwrap_or_default()),
                );
            } else {
                scan.problems.push(format!(
                    "`{}` is named like a session Seed but its signature is not \
                     (one Int input) -> result: LoopState",
                    decl.name
                ));
            }
        }
    }

    for (suffix, (step, state_input, budget_input, state_type, evidence_input)) in steps {
        match seeds.remove(&suffix) {
            Some((seed, seed_input, seed_state_type)) => {
                if seed_state_type != state_type {
                    scan.problems.push(format!(
                        "`{seed}` returns `{seed_state_type}` but `{step}` drives `{state_type}`: \
                         a session pair must carry the same state type"
                    ));
                    continue;
                }
                scan.surfaces.push(SessionSurface {
                    step,
                    seed,
                    state_type,
                    step_state_input: state_input,
                    step_budget_input: budget_input,
                    step_evidence_input: evidence_input,
                    seed_input,
                });
            }
            None => {
                scan.problems.push(format!(
                    "`{step}` has no matching `Seed{suffix}`: a session surface is the pair"
                ));
            }
        }
    }
    for (suffix, (seed, ..)) in seeds {
        scan.problems.push(format!(
            "`{seed}` has no matching `Step{suffix}`: a session surface is the pair"
        ));
    }
    scan
}

/// An opened loop host: one module, one chosen session surface, the
/// identity its scratch files carry.
#[derive(Clone, Debug)]
pub struct LoopHost {
    tree: SyntaxTree,
    path: PathBuf,
    surface: SessionSurface,
    identity: ScratchIdentity,
    facts: Option<FactCarry>,
    /// The module with its imports loaded, built at the first
    /// evaluation and reused by every later one.
    prepared: RefCell<Option<PreparedModule>>,
    /// Engine steps allowed per Seed/Step evaluation.
    work: u64,
    /// The observations bound to the Step's evidence input.
    evidence: Arc<Vec<CValue>>,
}

/// Completed `fact_*` calls carried from one Seed/Step evaluation into
/// the next (`constructor_layer::carry`). A carried fact returns the
/// value its call computes, so carrying changes wall time and engine
/// work only: states, verdicts, and logical units are unchanged.
#[derive(Clone, Debug)]
struct FactCarry {
    id: String,
    carried: RefCell<BTreeMap<String, CValue>>,
    fresh: RefCell<Vec<(String, CValue)>>,
    hits: Cell<u64>,
}

/// The facts computed since the last draw, and how many carried facts
/// were used.
pub struct FactDraw {
    pub fresh: Vec<(String, CValue)>,
    pub hits: u64,
}

impl LoopHost {
    /// Open a module and choose its session surface. `target` is the
    /// Step function name; `None` requires exactly one surface. A
    /// missing, non-surface, or ambiguous target refuses with the
    /// named lift diagnostic.
    pub fn open(module: &Path, target: Option<&str>) -> Result<Self, HostFault> {
        let source = std::fs::read_to_string(module).map_err(|err| {
            HostFault::fault(
                "loop_read",
                format!("cannot read module {}: {err}", module.display()),
            )
        })?;
        emath_syntax::install_source_parser();
        let (tree, diagnostics) = parse_str(&source);
        if diagnostics.has_errors() {
            return Err(HostFault::fault(
                "loop_parse",
                format!("{} does not parse: {:?}", module.display(), diagnostics),
            ));
        }
        let scan = scan_session_surfaces(&tree);
        let problems = if scan.problems.is_empty() {
            String::new()
        } else {
            format!("\nnear-miss surfaces:\n  - {}", scan.problems.join("\n  - "))
        };
        let surface = match target {
            Some(name) => scan
                .surfaces
                .iter()
                .find(|surface| surface.step == name)
                .cloned()
                .ok_or_else(|| {
                    HostFault::fault(
                        "loop_target",
                        format!(
                            "`{name}` is not a session surface of {}{}.\n\n{SURFACE_LIFT}",
                            module.display(),
                            problems
                        ),
                    )
                })?,
            None if scan.surfaces.len() == 1 => scan.surfaces[0].clone(),
            None if scan.surfaces.is_empty() => {
                return Err(HostFault::fault(
                    "loop_surface",
                    format!(
                        "{} declares no Step/Seed session surface.{problems}\n\n{SURFACE_LIFT}",
                        module.display()
                    ),
                ));
            }
            None => {
                return Err(HostFault::fault(
                    "loop_ambiguous",
                    format!(
                        "{} lifts multiple session surfaces; choose one: {}",
                        module.display(),
                        scan.surfaces
                            .iter()
                            .map(|surface| surface.step.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ));
            }
        };

        let identity = ScratchIdentity {
            meaning_id: admitted_meaning_id(module, &source)?,
            language_id: ENGINE_IMAGE.into(),
            target: surface.step.clone(),
        };
        let facts = fact_identity(&tree, Some(module))
            .map_err(HostFault::from)?
            .map(|id| FactCarry {
                id,
                carried: RefCell::new(BTreeMap::new()),
                fresh: RefCell::new(Vec::new()),
                hits: Cell::new(0),
            });
        Ok(Self {
            tree,
            path: module.to_path_buf(),
            surface,
            identity,
            facts,
            prepared: RefCell::new(None),
            work: DEFAULT_WORK,
            evidence: Arc::new(Vec::new()),
        })
    }

    /// Whether the Step declares an evidence input.
    #[must_use]
    pub fn takes_evidence(&self) -> bool {
        self.surface.step_evidence_input.is_some()
    }

    /// The observations every later Step call receives (`Observation`
    /// records, in ingestion order).
    pub fn set_evidence(&mut self, evidence: Vec<CValue>) {
        self.evidence = Arc::new(evidence);
    }

    /// Allow `work` engine steps per Seed/Step evaluation (a resource
    /// bound: exceeding it faults the evaluation; it never changes a
    /// value that fits).
    pub fn set_work(&mut self, work: u64) {
        self.work = work;
    }

    /// The identity of the module's fact functions and everything they
    /// reach (`None`: the module declares no `fact_*` function).
    #[must_use]
    pub fn fact_id(&self) -> Option<&str> {
        self.facts.as_ref().map(|facts| facts.id.as_str())
    }

    /// Carry previously computed facts (a ledger's contents, which must
    /// belong to `fact_id`). Facts already computed in this process and
    /// not yet drawn are dropped from the pending draw when the loaded
    /// set holds them. Returns how many facts are now carried.
    pub fn carry_facts(&self, loaded: Vec<(String, CValue)>) -> usize {
        let Some(facts) = &self.facts else {
            return 0;
        };
        let mut carried = facts.carried.borrow_mut();
        let known: std::collections::BTreeSet<String> =
            loaded.iter().map(|(key, _)| key.clone()).collect();
        facts.fresh.borrow_mut().retain(|(key, _)| !known.contains(key));
        for (key, value) in loaded {
            carried.entry(key).or_insert(value);
        }
        carried.len()
    }

    /// Take the facts computed since the last draw.
    pub fn take_facts(&self) -> FactDraw {
        match &self.facts {
            Some(facts) => FactDraw {
                fresh: std::mem::take(&mut *facts.fresh.borrow_mut()),
                hits: facts.hits.replace(0),
            },
            None => FactDraw { fresh: Vec::new(), hits: 0 },
        }
    }

    pub(crate) fn evaluate(&self, name: &str, inputs: &BTreeMap<String, CValue>) -> Result<CValue, HostFault> {
        if self.prepared.borrow().is_none() {
            let module = prepare_module(&self.tree, Some(&self.path)).map_err(HostFault::from)?;
            *self.prepared.borrow_mut() = Some(module);
        }
        let prepared = self.prepared.borrow();
        let module = prepared.as_ref().expect("prepared above");
        let Some(facts) = &self.facts else {
            return evaluate_function_carrying(module, name, inputs, &mut BTreeMap::new(), self.work)
                .map_err(HostFault::from)?
                .value
                .map_err(HostFault::from);
        };
        let run = evaluate_function_carrying(module, name, inputs, &mut facts.carried.borrow_mut(), self.work)
            .map_err(HostFault::from)?;
        facts.hits.set(facts.hits.get() + run.hits);
        facts.fresh.borrow_mut().extend(run.fresh);
        run.value.map_err(HostFault::from)
    }

    /// The chosen surface.
    #[must_use]
    pub fn surface(&self) -> &SessionSurface {
        &self.surface
    }

    /// The identity this host's scratch files carry.
    #[must_use]
    pub fn identity(&self) -> &ScratchIdentity {
        &self.identity
    }

    /// The module file this host runs (the path `open` was given).
    #[must_use]
    pub fn module_path(&self) -> &Path {
        &self.path
    }

    /// The host's own parsed tree (same admission the session and the
    /// native export were opened against). The dream driver re-uses
    /// it to observe the emission seam (`dream_world`/`dream_pred`)
    /// in-process.
    pub(crate) fn tree(&self) -> &SyntaxTree {
        &self.tree
    }

    /// Begin a session at the surface's seed state.
    pub fn begin(&self) -> Result<LoopSession, HostFault> {
        let inputs = BTreeMap::from([(
            self.surface.seed_input.clone(),
            CValue::Int(ExactInt::from(0)),
        )]);
        let state = self.evaluate(&self.surface.seed, &inputs)?;
        demand_state_record(&state, &self.surface.state_type)?;
        Ok(LoopSession {
            state,
            ledger: Vec::new(),
        })
    }

    fn call_step(&self, state: &CValue, budget: i128) -> Result<CValue, HostFault> {
        let mut inputs = BTreeMap::from([
            (
                self.surface.step_state_input.clone(),
                state.clone(),
            ),
            (
                self.surface.step_budget_input.clone(),
                CValue::Int(ExactInt::from(budget)),
            ),
        ]);
        if let Some(name) = &self.surface.step_evidence_input {
            inputs.insert(name.clone(), CValue::Sequence(Arc::clone(&self.evidence)));
        }
        let next = self.evaluate(&self.surface.step, &inputs)?;
        demand_state_record(&next, &self.surface.state_type)?;
        Ok(next)
    }
}

/// The meaning id a fresh admission of `source` would mint. The host
/// uses it at open time; the native export re-runs it to prove the
/// module on disk still admits to the SAME meaning before emitting an
/// artifact over it (a session and its export must never pair a stale
/// meaning id with edited math).
pub(crate) fn admitted_meaning_id(module: &Path, source: &str) -> Result<String, HostFault> {
    let mut session = CompilerSession::new(emath_core::limits::Limits::default());
    let result = session.check_owned(&module.display().to_string(), source);
    if result.diagnostics.has_errors() {
        return Err(HostFault::fault(
            "loop_identity",
            format!(
                "{} does not admit: {:?}",
                module.display(),
                result.diagnostics
            ),
        ));
    }
    match result.package.meaning_id(&[]) {
        Ok(id) => Ok(id.as_str().to_string()),
        Err(err) => Err(HostFault::fault(
            "loop_identity",
            format!("{} has no admitted meaning id: {err}", module.display()),
        )),
    }
}

fn demand_state_record(state: &CValue, state_type: &str) -> Result<(), HostFault> {
    match state {
        CValue::Record { type_name, .. } if &**type_name == state_type => Ok(()),
        other => Err(HostFault::fault(
            "loop_state_contract",
            format!(
                "the session state is {other:?}, not a `{state_type}` record: the Step/Seed \
                 pair must carry the loop state contract"
            ),
        )),
    }
}

// ---- value readers (the loop state contract fields) --------------------

fn record_fields(value: &CValue) -> Result<&BTreeMap<String, CValue>, HostFault> {
    match value {
        CValue::Record { fields, .. } => Ok(fields),
        other => Err(HostFault::fault(
            "loop_state_contract",
            format!("expected a record, found {other:?}"),
        )),
    }
}

fn field_i128(value: &CValue, name: &str) -> Result<i128, HostFault> {
    match record_fields(value)?.get(name) {
        Some(CValue::Int(n)) => n.to_i128().ok_or_else(|| {
            HostFault::fault(
                "loop_state_contract",
                format!("field `{name}` exceeds the i128 host projection"),
            )
        }),
        other => Err(HostFault::fault(
            "loop_state_contract",
            format!("field `{name}` is not an Int: {other:?}"),
        )),
    }
}

/// Read an Int field of a record (the loop state or an archive record).
pub fn value_int(value: &CValue, name: &str) -> Result<i128, HostFault> {
    field_i128(value, name)
}

/// Read a Rat field of a record as canonical `(num, den)`.
///
/// Prefer [`value_rational`] for authored values: targets may return
/// exact integers where a rational is declared.
pub fn value_rat(value: &CValue, name: &str) -> Result<(i128, i128), HostFault> {
    match record_fields(value)?.get(name) {
        Some(CValue::Rat { num, den }) => {
            let num = num.to_i128().ok_or_else(|| {
                HostFault::fault(
                    "loop_state_contract",
                    format!("field `{name}` numerator exceeds the i128 host projection"),
                )
            })?;
            let den = den.to_i128().ok_or_else(|| {
                HostFault::fault(
                    "loop_state_contract",
                    format!("field `{name}` denominator exceeds the i128 host projection"),
                )
            })?;
            Ok((num, den))
        }
        other => Err(HostFault::fault(
            "loop_state_contract",
            format!("field `{name}` is not a Rat: {other:?}"),
        )),
    }
}

/// Read a Bool field of a record.
pub fn value_bool(value: &CValue, name: &str) -> Result<bool, HostFault> {
    match record_fields(value)?.get(name) {
        Some(CValue::Bool(b)) => Ok(*b),
        other => Err(HostFault::fault(
            "loop_state_contract",
            format!("field `{name}` is not a Bool: {other:?}"),
        )),
    }
}

/// Read a sequence field of a record (cloned; archive-scale data).
pub fn value_sequence(value: &CValue, name: &str) -> Result<Vec<CValue>, HostFault> {
    match record_fields(value)?.get(name) {
        Some(CValue::Sequence(items)) => Ok(items.as_ref().clone()),
        other => Err(HostFault::fault(
            "loop_state_contract",
            format!("field `{name}` is not a Sequence: {other:?}"),
        )),
    }
}

fn has_field(value: &CValue, name: &str) -> bool {
    matches!(value, CValue::Record { fields, .. } if fields.contains_key(name))
}

/// The state's archive; empty for a plain loop state (no `archive`
/// field). A present archive must be a sequence.
pub fn state_archive(state: &CValue) -> Result<Vec<CValue>, HostFault> {
    if has_field(state, "archive") {
        value_sequence(state, "archive")
    } else {
        Ok(Vec::new())
    }
}

/// The state's `used` meter; 0 for a loop that does not meter.
fn state_used(state: &CValue) -> Result<i128, HostFault> {
    if has_field(state, "used") {
        field_i128(state, "used")
    } else {
        Ok(0)
    }
}

/// The state's frozen case ordinals; empty without a `case_set`.
fn state_case_ids(state: &CValue) -> Result<Vec<i128>, HostFault> {
    let Some(case_set) = record_fields(state)?.get("case_set") else {
        return Ok(Vec::new());
    };
    value_sequence(case_set, "ids")?
        .iter()
        .map(|id| match id {
            CValue::Int(n) => n.to_i128().ok_or_else(|| {
                HostFault::fault("loop_state_contract", "a case id exceeds the i128 host projection")
            }),
            other => Err(HostFault::fault(
                "loop_state_contract",
                format!("a case id is not an Int: {other:?}"),
            )),
        })
        .collect()
}

/// The incumbent's archive record; `None` for a plain loop state.
fn state_incumbent(state: &CValue) -> Result<Option<CValue>, HostFault> {
    if !has_field(state, "archive") {
        return Ok(None);
    }
    let incumbent = field_i128(state, "incumbent")?;
    let archive = value_sequence(state, "archive")?;
    let ordinal = usize::try_from(incumbent).map_err(|_| {
        HostFault::fault(
            "loop_state_contract",
            format!("incumbent ordinal {incumbent} is not a valid archive index"),
        )
    })?;
    archive.get(ordinal).cloned().map(Some).ok_or_else(|| {
        HostFault::fault(
            "loop_state_contract",
            format!("incumbent ordinal {ordinal} is outside the archive"),
        )
    })
}

// ---- session -----------------------------------------------------------

/// One loop session: the current state and the immutable batch ledger.
#[derive(Clone, Debug)]
pub struct LoopSession {
    state: CValue,
    ledger: Vec<BatchLedgerEntry>,
}

impl LoopSession {
    /// The current loop state.
    #[must_use]
    pub fn state(&self) -> &CValue {
        &self.state
    }

    /// The committed batch ledger.
    #[must_use]
    pub fn ledger(&self) -> &[BatchLedgerEntry] {
        &self.ledger
    }

    /// The committed revision (the ledger length).
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.ledger.len() as u64
    }

    /// The session's frozen case ordinals (the host curriculum seam);
    /// empty when the state has no `case_set`.
    pub fn case_ids(&self) -> Result<Vec<i128>, HostFault> {
        state_case_ids(&self.state)
    }

    /// Read an Int field of the current state.
    pub fn state_int(&self, name: &str) -> Result<i128, HostFault> {
        value_int(&self.state, name)
    }

    /// The state's `used` meter (0 when the loop does not meter).
    pub fn used(&self) -> Result<i128, HostFault> {
        state_used(&self.state)
    }

    /// The incumbent's archive record (cloned); `None` for a plain
    /// loop state with no archive.
    pub fn incumbent_record(&self) -> Result<Option<CValue>, HostFault> {
        state_incumbent(&self.state)
    }

    /// Run one batch through the host's Step function, project the
    /// ledger entry, and enforce the module-semantic ledger law: the
    /// state's own batch counter must equal the committed revision.
    pub fn step(&mut self, host: &LoopHost, budget: i128) -> Result<&BatchLedgerEntry, HostFault> {
        let next = host.call_step(&self.state, budget)?;
        let entry = project_batch(&self.state, &next)?;
        let batch = field_i128(&next, "batch")?;
        let expected = self.ledger.len() as i128 + 1;
        if batch != expected {
            return Err(HostFault::fault(
                "loop_ledger_law",
                format!(
                    "the state's batch counter is {batch} after committing {expected} batches: \
                     the Step function must advance the batch by exactly one per call"
                ),
            ));
        }
        self.state = next;
        self.ledger.push(entry);
        Ok(self.ledger.last().expect("just pushed"))
    }

    /// Freeze one more case ordinal into the session's case set (the
    /// host curriculum seam: the case set is explicit host-owned data,
    /// X1). Duplicates refuse by name. The next batch's freshness law
    /// re-probes and re-scores the whole archive on the grown set.
    pub fn grow_case(&mut self, case_id: i128) -> Result<(), HostFault> {
        let (type_name, fields) = match &self.state {
            CValue::Record { type_name, fields } => {
                (type_name.clone(), fields.as_ref().clone())
            }
            other => {
                return Err(HostFault::fault(
                    "loop_state_contract",
                    format!("expected a record state, found {other:?}"),
                ))
            }
        };
        let case_set = match fields.get("case_set") {
            Some(case_set) => case_set.clone(),
            None => {
                return Err(HostFault::fault(
                    "loop_state_contract",
                    "the state has no `case_set` field",
                ))
            }
        };
        let (case_type, mut case_fields) = match &case_set {
            CValue::Record { type_name, fields } => {
                (type_name.clone(), fields.as_ref().clone())
            }
            other => {
                return Err(HostFault::fault(
                    "loop_state_contract",
                    format!("`case_set` is not a record: {other:?}"),
                ))
            }
        };
        let mut ids = match case_fields.get("ids") {
            Some(CValue::Sequence(items)) => items.as_ref().clone(),
            other => {
                return Err(HostFault::fault(
                    "loop_state_contract",
                    format!("`case_set.ids` is not a Sequence: {other:?}"),
                ))
            }
        };
        if ids
            .iter()
            .any(|id| matches!(id, CValue::Int(n) if n.to_i128() == Some(case_id)))
        {
            return Err(HostFault::fault(
                "loop_case_duplicate",
                format!("case {case_id} is already frozen"),
            ));
        }
        ids.push(CValue::Int(ExactInt::from(case_id)));
        case_fields.insert("ids".into(), CValue::Sequence(Arc::new(ids)));
        let mut fields = fields;
        fields.insert(
            "case_set".into(),
            CValue::Record {
                type_name: case_type,
                fields: Arc::new(case_fields),
            },
        );
        self.state = CValue::Record {
            type_name,
            fields: Arc::new(fields),
        };
        Ok(())
    }

    /// Save the session as an `emath.scratch.v1` checkpoint.
    pub fn save(&self, host: &LoopHost, path: &Path) -> Result<(), HostFault> {
        save_scratch(
            path,
            host.identity(),
            self.revision(),
            &self.state,
            &self.ledger,
        )
        .map_err(HostFault::from)
    }

    /// Load a session from a scratch checkpoint. The scratch contract's
    /// own identity and file-consistency laws apply (through
    /// `load_scratch`), plus the host's module-semantic law: the
    /// state's batch counter must equal the checkpoint revision, and
    /// the state must be this surface's state type.
    pub fn load(host: &LoopHost, path: &Path) -> Result<Self, HostFault> {
        let checkpoint = load_scratch(path, host.identity()).map_err(HostFault::from)?;
        demand_state_record(&checkpoint.state, &host.surface.state_type)?;
        let batch = field_i128(&checkpoint.state, "batch")?;
        if batch < 0 || batch as u64 != checkpoint.revision {
            return Err(HostFault::fault(
                "loop_ledger_law",
                format!(
                    "the checkpoint's state batch counter is {batch} but its ledger commits {} \
                     batches: the scratch and the loop state disagree",
                    checkpoint.revision
                ),
            ));
        }
        Ok(Self {
            state: checkpoint.state,
            ledger: checkpoint.ledger,
        })
    }
}

/// Read a field as an exact rational: `Rat` as-is, `Int` as `n/1`
/// (an authored score closure may be Int-typed, and an exact integer
/// is an exact rational). Rational arithmetic itself keeps the
/// rational carrier even on integer-valued results (the
/// operand-carrier rule), so a Rat-declared score arrives as a Rat.
pub fn value_rational(value: &CValue, name: &str) -> Result<(i128, i128), HostFault> {
    let field = record_fields(value)?.get(name).ok_or_else(|| {
        HostFault::fault(
            "loop_state_contract",
            format!("field `{name}` is missing"),
        )
    })?;
    match field {
        CValue::Rat { num, den } => {
            let num = num.to_i128().ok_or_else(|| {
                HostFault::fault(
                    "loop_state_contract",
                    format!("field `{name}` numerator exceeds the i128 host projection"),
                )
            })?;
            let den = den.to_i128().ok_or_else(|| {
                HostFault::fault(
                    "loop_state_contract",
                    format!("field `{name}` denominator exceeds the i128 host projection"),
                )
            })?;
            Ok((num, den))
        }
        CValue::Int(n) => {
            let num = n.to_i128().ok_or_else(|| {
                HostFault::fault(
                    "loop_state_contract",
                    format!("field `{name}` exceeds the i128 host projection"),
                )
            })?;
            Ok((num, 1))
        }
        other => Err(HostFault::fault(
            "loop_state_contract",
            format!("field `{name}` is not an exact scalar: {other:?}"),
        )),
    }
}

/// Project one committed batch from the previous and next states:
/// host observation, never loop interpretation. Archive ordinals are
/// append-only, so acceptance flips diff by ordinal. A plain loop state
/// (no archive) projects only its batch, verdict, and `used`.
fn project_batch(previous: &CValue, next: &CValue) -> Result<BatchLedgerEntry, HostFault> {
    let previous_archive = state_archive(previous)?;
    let next_archive = state_archive(next)?;
    if next_archive.len() < previous_archive.len() {
        return Err(HostFault::fault(
            "loop_state_contract",
            "the archive shrank across a batch: archive ordinals are append-only",
        ));
    }
    let mut promoted = Vec::new();
    let mut quarantined = Vec::new();
    for (ordinal, record) in next_archive.iter().enumerate() {
        let was_accepted = match previous_archive.get(ordinal) {
            Some(prev) => value_bool(prev, "accepted")?,
            None => false,
        };
        let is_accepted = value_bool(record, "accepted")?;
        let key = value_int(record, "key")?;
        if is_accepted && !was_accepted {
            promoted.push(key);
        }
        if !is_accepted && was_accepted {
            quarantined.push(key);
        }
    }
    let (incumbent_key, (score_num, score_den)) = match state_incumbent(next)? {
        Some(record) => (value_int(&record, "key")?, value_rational(&record, "score")?),
        None => (0, (0, 1)),
    };
    let case_ids = state_case_ids(next)?;
    let verdict = field_i128(next, "verdict")?;
    Ok(BatchLedgerEntry {
        batch: field_i128(next, "batch")? as u64,
        verdict,
        used: state_used(next)?,
        incumbent_key,
        incumbent_score_num: score_num,
        incumbent_score_den: score_den,
        archive_len: next_archive.len() as u64,
        promoted,
        quarantined,
        case_ids,
        halted: verdict == 4,
    })
}
