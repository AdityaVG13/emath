//! The dream driver (bead emath-gav6o): open-ended self-escalating
//! research over an authored dream target.
//!
//! A dream target is an ordinary module: a Step/Seed session surface
//! (the host contract) whose module owns every loop law - charging,
//! freshness, promotion, the self-escalating curriculum, graduation.
//! The dream driver is thin host-side orchestration over that surface
//! and owns only the dream laws:
//!
//! - Resume: a budget-exhausted batch (verdict 4) is partial but
//!   committed; the driver raises the logical-unit watermark to
//!   double the committed spend plus the configured step and
//!   continues. The doubling shape is the law: the freshness law
//!   re-charges the whole retained archive every batch, so a linear
//!   ladder smaller than the per-batch refresh cost locksteps (the
//!   same first records re-probed forever); doubling always outruns
//!   any finite per-batch cost. A dead-start budget climbs the same
//!   ladder.
//! - Plateau: consecutive batches with no promotion and no audit
//!   growth (verdict 3) close the level at the configured count
//!   (default 3). Any progress verdict (0) resets the counter: a
//!   level only closes after the plateau survives every growth cycle
//!   the module's own curriculum can attempt.
//! - Close: domain exhaustion (verdict 2) and goal attainment
//!   (verdict 1) close the level immediately.
//! - Stop: `max_batches` is the external stop button - a run that
//!   never closes stops with no level and no emitted file. There is
//!   no self-scheduler.
//! - Emission: at close the driver emits the level file - the
//!   discovery audit trail. The incumbent key, the frozen case set,
//!   the incumbent's observed predictions, and the observed world
//!   table are pinned as literals with authored tests, and the driver
//!   verifies that certificate in-process before reporting the level
//!   verified. Re-verification later is cheap: `emath test` on the
//!   level file. The emission seam is the authored pair
//!   `dream_world` (one Int input, the case table) and `dream_pred`
//!   (two Int inputs, the candidate family's prediction); a module
//!   without them refuses by name at open when emission is
//!   configured.
//!
//! One run drives one level (one module). Chaining levels across
//! worlds is caller orchestration; the level number is this run's own.
//!
//! Error model: engine, scratch, and host faults pass through with
//! their own codes; dream-owned refusals use `dream_*` codes.
//!
//! Determinism class: identical module, target, config, and out_dir
//! produce an identical walk, level record, and level file (the
//! engine is deterministic; emission renders observed values only).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use emath_core::tree::{Item, StmtKind, SyntaxTree, TypeKind};
use emath_exec_ir::constructor_layer::{evaluate_function_at, evaluate_tree_at, CValue};
use emath_exec_ir::exact_int::ExactInt;
use emath_syntax::parse_str;

use crate::host::{value_rational, HostFault, LoopHost, LoopSession};

/// The close verdicts, as the level journal spells them (the verdict
/// vocabulary is module data; these are the driver's close reasons).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DreamStopReason {
    /// The level closed (plateau, domain exhaustion, or goal).
    Closed,
    /// The external stop button fired before any close verdict.
    MaxBatches,
}

/// The driver's configuration: the budget ladder, the plateau close
/// count, the stop button, and the level emission directory.
#[derive(Clone, Debug)]
pub struct DreamConfig {
    /// The first logical-unit watermark (the total `used` allowance,
    /// not a per-batch allowance: `used` accumulates across batches).
    pub budget0: i128,
    /// The resume increment: on every budget-exhausted batch the
    /// watermark becomes `2 * used + budget_step` (the guaranteed
    /// new-units margin on top of the doubled committed spend).
    pub budget_step: i128,
    /// The external stop button: the maximum committed batches
    /// (budget-exhausted batches count; they commit too).
    pub max_batches: u64,
    /// Consecutive plateau batches that close a level.
    pub plateau_close: u32,
    /// Where `level_001.emath` is emitted; `None` drives without
    /// emission (and without needing the emission seam).
    pub out_dir: Option<PathBuf>,
}

impl Default for DreamConfig {
    fn default() -> Self {
        Self {
            budget0: 64,
            budget_step: 64,
            max_batches: 10_000,
            plateau_close: 3,
            out_dir: None,
        }
    }
}

/// One level's journal entry: what closed, what was discovered, what
/// it cost, and where the audit trail lives.
#[derive(Clone, Debug)]
pub struct LevelRecord {
    /// This run's level number (one run drives one level).
    pub level: u32,
    /// The close reason: `plateau`, `domain_exhausted`, or
    /// `goal_attained`.
    pub close_reason: &'static str,
    /// Batches committed at close (budget-exhausted batches included).
    pub batches: u64,
    /// Logical units charged at close (the loop's own meter).
    pub used: i128,
    /// The incumbent's candidate key at close.
    pub incumbent_key: i128,
    /// The incumbent's score at close, canonical `(num, den)`.
    pub incumbent_score: (i128, i128),
    /// The frozen case set at close.
    pub case_ids: Vec<i128>,
    /// The emitted level file (`None` when no out_dir was configured).
    pub path: Option<PathBuf>,
    /// Whether the emitted level file's authored tests all passed
    /// in-process verification (false when nothing was emitted).
    pub certificate: bool,
}

/// The dream run's outcome: the stop reason, the level (if closed),
/// and the budget resumes performed.
#[derive(Clone, Debug)]
pub struct DreamOutcome {
    pub stop: DreamStopReason,
    pub level: Option<LevelRecord>,
    pub resumes: u64,
}

/// The authored emission seam: `dream_world` (the case table) and
/// `dream_pred` (the candidate family's prediction), with their
/// author-chosen Int input names.
struct EmissionSeam {
    world_input: String,
    pred_k_input: String,
    pred_i_input: String,
}

/// The Int input names of one named function, in declaration order.
/// Any non-Int input makes the seam unmet (the dream's emission pair
/// is Int-keyed).
fn int_inputs(tree: &SyntaxTree, function: &str) -> Option<Vec<String>> {
    for item in &tree.items {
        let Item::Declaration(decl) = item else {
            continue;
        };
        if decl.as_kind != "function" || decl.name != function {
            continue;
        }
        let mut names = Vec::new();
        for section in decl.sections().filter(|section| section.name == "inputs") {
            for stmt in &section.suite.statements {
                match &stmt.kind {
                    StmtKind::FieldDecl { name, ty, .. } => {
                        let is_int = matches!(
                            &ty.kind,
                            TypeKind::Path { segments, .. }
                                if segments.last().is_some_and(|s| s == "Int")
                        );
                        if !is_int {
                            return None;
                        }
                        names.push(name.clone());
                    }
                    _ => {}
                }
            }
        }
        return Some(names);
    }
    None
}

/// Resolve the emission seam (`dream_world`, `dream_pred`) or refuse
/// by name with what to author.
fn resolve_emission(tree: &SyntaxTree) -> Result<EmissionSeam, HostFault> {
    let world_inputs = int_inputs(tree, "dream_world");
    let pred_inputs = int_inputs(tree, "dream_pred");
    match (world_inputs, pred_inputs) {
        (Some(world), Some(pred)) if world.len() == 1 && pred.len() == 2 => {
            let mut pred = pred.into_iter();
            Ok(EmissionSeam {
                world_input: world.into_iter().next().unwrap_or_default(),
                pred_k_input: pred.next().unwrap_or_default(),
                pred_i_input: pred.next().unwrap_or_default(),
            })
        }
        _ => Err(HostFault::fault(
            "dream_emission_surface",
            "level emission needs the authored dream emission seam: \
             `emath function dream_world` with one Int input returning the case \
             table as sequence(Int), and `emath function dream_pred` with two Int \
             inputs (key, case ordinal) returning the candidate family's \
             prediction as Int. Author both, or run without an out_dir.",
        )),
    }
}

/// Evaluate `dream_world` at key 0: the observed case table.
fn observed_world(
    host: &LoopHost,
    seam: &EmissionSeam,
) -> Result<Vec<i128>, HostFault> {
    let inputs = BTreeMap::from([(seam.world_input.clone(), CValue::Int(ExactInt::from(0)))]);
    let value = evaluate_function_at(
        host.tree(),
        "dream_world",
        &inputs,
        Some(host.module_path()),
    )
    .map_err(HostFault::from)?;
    match value {
        CValue::Sequence(items) => items
            .iter()
            .map(|item| match item {
                CValue::Int(n) => n.to_i128().ok_or_else(|| {
                    HostFault::fault(
                        "dream_emission_surface",
                        "a dream_world case value exceeds the i128 host projection",
                    )
                }),
                other => Err(HostFault::fault(
                    "dream_emission_surface",
                    format!("dream_world must return sequence(Int), found {other:?}"),
                )),
            })
            .collect(),
        other => Err(HostFault::fault(
            "dream_emission_surface",
            format!("dream_world must return sequence(Int), found {other:?}"),
        )),
    }
}

/// Evaluate `dream_pred(key, case)`: one observed prediction.
fn observed_prediction(
    host: &LoopHost,
    seam: &EmissionSeam,
    key: i128,
    case: i128,
) -> Result<i128, HostFault> {
    let inputs = BTreeMap::from([
        (seam.pred_k_input.clone(), CValue::Int(ExactInt::from(key))),
        (seam.pred_i_input.clone(), CValue::Int(ExactInt::from(case))),
    ]);
    let value = evaluate_function_at(
        host.tree(),
        "dream_pred",
        &inputs,
        Some(host.module_path()),
    )
    .map_err(HostFault::from)?;
    match value {
        CValue::Int(n) => n.to_i128().ok_or_else(|| {
            HostFault::fault(
                "dream_emission_surface",
                "a dream_pred value exceeds the i128 host projection",
            )
        }),
        other => Err(HostFault::fault(
            "dream_emission_surface",
            format!("dream_pred must return Int, found {other:?}"),
        )),
    }
}

fn seq_literal(values: &[i128]) -> String {
    let inner = values
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    if inner.is_empty() {
        "[]".into()
    } else {
        format!("[{inner}]")
    }
}

/// Render the level file: the discovery audit trail with authored
/// tests pinning exactly what the driver observed.
#[allow(clippy::too_many_arguments)]
fn render_level(
    module: &Path,
    target: &str,
    close_reason: &str,
    batches: u64,
    used: i128,
    resumes: u64,
    incumbent_key: i128,
    score: (i128, i128),
    case_ids: &[i128],
    predictions: &[i128],
    world: &[i128],
    frozen_errors: i128,
) -> String {
    let cases = seq_literal(case_ids);
    let preds = seq_literal(predictions);
    let table = seq_literal(world);
    format!(
        r#"# Dream level 1 - {module} ({target})
# close: {close_reason} after {batches} batches, {used} logical units, {resumes} budget resume(s)
# incumbent key {incumbent_key}, score {num}/{den} over the frozen case set {cases}
# the discovery audit trail, emitted by the dream driver from observed
# values only; re-verify cheaply: emath test <this file>

emath function LevelData:
    # The pinned discovery: what the driver observed at close.
    inputs:
        unused: Int
    outputs:
        incumbent_key: Int
        case_ids: sequence(Int)
        predictions: sequence(Int)
        world: sequence(Int)
    definitions:
        incumbent_key = {incumbent_key}
        case_ids = {cases}
        predictions = {preds}
        world = {table}
    tests:
        example <the_pinned_discovery>:
            given unused = 0
            expect incumbent_key == {incumbent_key}
            expect case_ids == {cases}
            expect predictions == {preds}
            expect world == {table}

emath function frozen_errors:
    # The certificate's law: the incumbent's pinned predictions
    # against the pinned world over the frozen cases, recomputed from
    # the pinned data alone ({frozen_errors} at emission).
    inputs:
        case_ids: sequence(Int)
        predictions: sequence(Int)
        world: sequence(Int)
        j: Int
    outputs:
        result: Int
    definitions:
        done = j >= length(case_ids)
        c = if done: 0 else: case_ids[j]
        one = if done: 0 else: if predictions[c] == world[c]: 0 else: 1
        result = if done: 0 else: one + frozen_errors(case_ids, predictions, world, j + 1)
    tests:
        example <the_pinned_discovery_recomputes>:
            given case_ids = {cases}
            given predictions = {preds}
            given world = {table}
            given j = 0
            expect result == {frozen_errors}
"#,
        module = module.display(),
        num = score.0,
        den = score.1,
    )
}

/// Emit and verify the level file. Returns the path and the
/// in-process certificate verdict (all authored tests passed).
fn emit_level(
    host: &LoopHost,
    seam: &EmissionSeam,
    out_dir: &Path,
    close_reason: &str,
    batches: u64,
    used: i128,
    resumes: u64,
    session: &LoopSession,
) -> Result<(PathBuf, bool), HostFault> {
    let incumbent = session.incumbent_record()?;
    let incumbent_key = crate::host::value_int(&incumbent, "key")?;
    let score = value_rational(&incumbent, "score")?;
    let case_ids = session.case_ids()?;
    let world = observed_world(host, seam)?;
    if world.is_empty() {
        return Err(HostFault::fault(
            "dream_emission_surface",
            "dream_world returned an empty case table: the certificate has nothing to pin",
        ));
    }
    let mut predictions = Vec::with_capacity(world.len());
    for case in 0..world.len() as i128 {
        predictions.push(observed_prediction(host, seam, incumbent_key, case)?);
    }
    let mut frozen_errors: i128 = 0;
    for case in &case_ids {
        let idx = usize::try_from(*case).map_err(|_| {
            HostFault::fault(
                "dream_emission_surface",
                format!("the frozen case ordinal {case} is negative"),
            )
        })?;
        let prediction = predictions.get(idx).ok_or_else(|| {
            HostFault::fault(
                "dream_emission_surface",
                format!(
                    "the frozen case ordinal {case} is outside the world table \
                     (length {}): the curriculum froze a case the world does not have",
                    world.len()
                ),
            )
        })?;
        if *prediction != world[idx] {
            frozen_errors += 1;
        }
    }
    let text = render_level(
        host.module_path(),
        host.surface().step.as_str(),
        close_reason,
        batches,
        used,
        resumes,
        incumbent_key,
        score,
        &case_ids,
        &predictions,
        &world,
        frozen_errors,
    );

    let path = out_dir.join("level_001.emath");
    if path.exists() {
        return Err(HostFault::fault(
            "dream_level_exists",
            format!(
                "{} already exists: the dream never overwrites a level file; \
                 choose a fresh out_dir per dream",
                path.display()
            ),
        ));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            HostFault::fault(
                "dream_level_write",
                format!("cannot create {}: {err}", path.display()),
            )
        })?;
    }
    std::fs::write(&path, text).map_err(|err| {
        HostFault::fault(
            "dream_level_write",
            format!("cannot write {}: {err}", path.display()),
        )
    })?;

    // In-process certificate verification: parse and evaluate the
    // emitted file; every authored test must pass before the level is
    // reported verified.
    let source = std::fs::read_to_string(&path).map_err(|err| {
        HostFault::fault(
            "dream_level_write",
            format!("cannot read back {}: {err}", path.display()),
        )
    })?;
    emath_syntax::install_source_parser();
    let (tree, diagnostics) = parse_str(&source);
    if diagnostics.has_errors() {
        return Err(HostFault::fault(
            "dream_certificate",
            format!(
                "the emitted level does not parse: {:?} (driver emission bug)",
                diagnostics
            ),
        ));
    }
    let report = evaluate_tree_at(&tree, None).map_err(|err| {
        HostFault::fault(
            "dream_certificate",
            format!("the emitted level does not evaluate: {err} (driver emission bug)"),
        )
    })?;
    let failed: Vec<&str> = report
        .tests
        .iter()
        .filter(|test| !test.passed)
        .map(|test| test.label.as_str())
        .collect();
    if !failed.is_empty() || report.tests.is_empty() {
        return Err(HostFault::fault(
            "dream_certificate",
            format!(
                "the emitted level's authored tests failed in-process: {failed:?} \
                 (driver emission bug)"
            ),
        ));
    }
    Ok((path, true))
}

/// Drive one dream level over an authored dream target.
///
/// `target` is the Step function name; `None` requires exactly one
/// session surface (the host contract). The module owns every loop
/// law; the driver owns resume, plateau counting, closing, stopping,
/// and level emission.
pub fn run_dream(
    module: &Path,
    target: Option<&str>,
    config: &DreamConfig,
) -> Result<DreamOutcome, HostFault> {
    if config.budget0 < 1 || config.budget_step < 1 || config.plateau_close < 1 {
        return Err(HostFault::fault(
            "dream_config",
            "budget0, budget_step, and plateau_close must each be at least 1",
        ));
    }
    let host = LoopHost::open(module, target)?;
    let seam = match &config.out_dir {
        Some(_) => Some(resolve_emission(host.tree())?),
        None => None,
    };
    let mut session = host.begin()?;

    let mut budget = config.budget0;
    let mut plateaus: u32 = 0;
    let mut resumes: u64 = 0;
    let mut batches: u64 = 0;
    let mut close_reason: Option<&'static str> = None;

    while batches < config.max_batches {
        let entry = session.step(&host, budget)?;
        batches += 1;
        match entry.verdict {
            1 => {
                close_reason = Some("goal_attained");
                break;
            }
            2 => {
                close_reason = Some("domain_exhausted");
                break;
            }
            3 => {
                plateaus += 1;
                if plateaus >= config.plateau_close {
                    close_reason = Some("plateau");
                    break;
                }
            }
            4 => {
                // Resume law: the new watermark doubles the committed
                // spend plus one step. A linear ladder can lockstep:
                // the freshness law re-charges the whole retained
                // archive from ordinal 0 every batch, so a margin
                // smaller than the per-batch refresh cost is re-spent
                // on the same first records forever and the walk
                // never moves. Doubling the committed spend always
                // outruns any finite per-batch cost, so a dead-start
                // or mis-sized budget still climbs to the close -
                // while every unit stays charged (the loop's own
                // `used` meter is untouched; only the watermark
                // moves).
                budget = 2 * entry.used + config.budget_step;
                resumes += 1;
            }
            0 => {
                // Progress resets the plateau counter: a promotion or
                // an audit growth (verdict 0) is the module's own
                // proof the walk is still moving, so a level only
                // closes after the plateau survives every growth
                // cycle the module's curriculum can attempt.
                plateaus = 0;
            }
            other => {
                return Err(HostFault::fault(
                    "dream_verdict",
                    format!(
                        "the module reported verdict {other}, outside the dream's \
                         vocabulary (0 running, 1 goal, 2 exhausted, 3 plateau, \
                         4 budget): the driver cannot drive an unknown verdict"
                    ),
                ));
            }
        }
    }

    let Some(close_reason) = close_reason else {
        return Ok(DreamOutcome {
            stop: DreamStopReason::MaxBatches,
            level: None,
            resumes,
        });
    };

    let used = session.state_int("used")?;
    let incumbent = session.incumbent_record()?;
    let incumbent_key = crate::host::value_int(&incumbent, "key")?;
    let score = value_rational(&incumbent, "score")?;
    let case_ids = session.case_ids()?;

    let (path, certificate) = match (&config.out_dir, &seam) {
        (Some(out_dir), Some(seam)) => {
            let (path, certificate) = emit_level(
                &host,
                seam,
                out_dir,
                close_reason,
                batches,
                used,
                resumes,
                &session,
            )?;
            (Some(path), certificate)
        }
        _ => (None, false),
    };

    Ok(DreamOutcome {
        stop: DreamStopReason::Closed,
        level: Some(LevelRecord {
            level: 1,
            close_reason,
            batches,
            used,
            incumbent_key,
            incumbent_score: score,
            case_ids,
            path,
            certificate,
        }),
        resumes,
    })
}
