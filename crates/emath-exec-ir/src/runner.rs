//! Declaration runner: constructor requires → Self state → definitions →
//! example `given`/`expect` verdicts.
//!
//! Binding rules copy the Rust backend's generated `#[test]`:
//! - `given` values are lowered in `BTreeMap` key order; later givens may
//!   reference earlier ones.
//! - Constructor parameters and declaration inputs must appear in `given`
//!   (a constant-only declaration has neither, so an empty `given` is
//!   enough).
//! - Definitions lower against declaration inputs, previously computed
//!   definition names (source order — let-binding semantics, matching
//!   admission), and `state.<name>`.
//! - `expect` (when present) lowers against the given names plus each
//!   computed definition name, matching the generated `let y = actual;`
//!   bindings. `expect: None` is a worked example (`Computed`): values
//!   are displayed, no pass/fail claim.
//! - A declaration with no tests still produces a `_pane` worked run when
//!   every input is bound (zero inputs = trivially bound). `extra_given`
//!   adds that `_pane` run in addition to any source examples.

use crate::interp::{evaluate, EvalFault, Value};
use crate::{lower_definition, lower_requirement};
use emath_ir::{BinaryOp, Declaration, ExprId, ExprNode, Literal, SemanticPackage, TypeNode, UnaryOp};
use std::collections::BTreeMap;
use std::fmt;

/// Hint stored on declarations that have no `tests:` examples and cannot
/// be computed directly (an input or constructor parameter is unbound).
pub const ZERO_TEST_NOTE: &str = "no examples; add a worked example or use input fields";

/// Synthetic worked-run name used when the pane supplies givens or when a
/// declaration has no examples and every input is already bound.
pub const PANE_TEST_NAME: &str = "_pane";

/// Outcome of one example test.
#[derive(Clone, Debug, PartialEq)]
pub enum TestVerdict {
    /// `expect` evaluated to `true`.
    Passed,
    /// `expect` evaluated to `false`.
    Failed,
    /// No `expect`: values were computed, no assertion claim.
    Computed,
    /// A constructor `require` / `ensure` evaluated to `false`.
    ConstructorRefused {
        /// Source-like obligation text (`require scale >= 0`).
        obligation: String,
    },
    /// EMIR lowering refused the given, require, assignment, definition, or
    /// expect expression.
    LoweringRefused {
        /// Lowering error text.
        detail: String,
    },
    /// Interpreter fault (type confusion, missing slot, bad register).
    Fault {
        /// The typed fault.
        fault: EvalFault,
    },
}

impl TestVerdict {
    /// Whether this verdict is a passing expect.
    #[must_use]
    pub const fn expect_passed(&self) -> bool {
        matches!(self, Self::Passed)
    }

    /// Whether this verdict is a typed refusal rather than a Boolean fail.
    #[must_use]
    pub const fn is_refused(&self) -> bool {
        matches!(
            self,
            Self::ConstructorRefused { .. } | Self::LoweringRefused { .. } | Self::Fault { .. }
        )
    }

    /// Stable refusal tag for JSON (`constructor-refused` / …), if any.
    #[must_use]
    pub const fn refusal_tag(&self) -> Option<&'static str> {
        match self {
            Self::ConstructorRefused { .. } => Some("constructor-refused"),
            Self::LoweringRefused { .. } => Some("lowering-refused"),
            Self::Fault { .. } => Some("fault"),
            Self::Passed | Self::Failed | Self::Computed => None,
        }
    }

    /// Whether this is a worked example (no `expect`).
    #[must_use]
    pub const fn is_computed(&self) -> bool {
        matches!(self, Self::Computed)
    }

    /// Human-readable refusal / fault text.
    #[must_use]
    pub fn reason_text(&self) -> Option<String> {
        match self {
            Self::ConstructorRefused { obligation } => Some(obligation.clone()),
            Self::LoweringRefused { detail } => Some(detail.clone()),
            Self::Fault { fault } => Some(fault.to_string()),
            Self::Passed | Self::Failed | Self::Computed => None,
        }
    }
}

impl fmt::Display for TestVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Passed => f.write_str("passed"),
            Self::Failed => f.write_str("failed"),
            Self::Computed => f.write_str("computed"),
            Self::ConstructorRefused { obligation } => {
                write!(f, "constructor refused: {obligation}")
            }
            Self::LoweringRefused { detail } => write!(f, "lowering refused: {detail}"),
            Self::Fault { fault } => write!(f, "fault: {fault}"),
        }
    }
}

/// One example test after interpretation.
#[derive(Clone, Debug, PartialEq)]
pub struct TestRun {
    /// Example name (`three_squared`).
    pub name: String,
    /// Evaluated `given` map (name → typed [`Value`]), `BTreeMap` order.
    pub given: BTreeMap<String, Value>,
    /// Constructor `Self:` fields when construction succeeded.
    pub state: BTreeMap<String, Value>,
    /// Each definition's computed value, declaration-map order.
    pub definitions: BTreeMap<String, Value>,
    /// Declared outputs that have a computed definition.
    pub outputs: BTreeMap<String, Value>,
    /// Pass / fail / typed refusal.
    pub verdict: TestVerdict,
}

/// Aggregate counts over every example that was attempted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunSummary {
    /// Tests attempted (excludes zero-test declarations).
    pub tests: u32,
    /// `expect` was true.
    pub passed: u32,
    /// `expect` was false.
    pub failed: u32,
    /// Constructor / lowering / fault refusal.
    pub refused: u32,
    /// Worked examples (`expect` omitted).
    pub computed: u32,
}

/// Per-declaration run.
#[derive(Clone, Debug, PartialEq)]
pub struct DeclarationRun {
    /// Declaration leaf name.
    pub name: String,
    /// Example results in declaration test-id order.
    pub tests: Vec<TestRun>,
    /// Present when `tests` is empty (the wasm layer surfaces this as a hint).
    pub note: Option<String>,
}

/// Package-wide report. Declaration order matches [`SemanticPackage`].
#[derive(Clone, Debug, PartialEq)]
pub struct RunReport {
    /// One entry per declaration, source order.
    pub declarations: Vec<DeclarationRun>,
    /// Counts over every attempted example.
    pub summary: RunSummary,
}

/// Run every declaration's example tests.
#[must_use]
pub fn run_package(package: &SemanticPackage) -> RunReport {
    run_package_with_given(package, None)
}

/// Run example tests, plus a synthetic `_pane` worked run when `extra_given`
/// is present (or when a declaration has no tests and every input is bound).
#[must_use]
pub fn run_package_with_given(
    package: &SemanticPackage,
    extra_given: Option<&BTreeMap<String, Value>>,
) -> RunReport {
    let declarations: Vec<DeclarationRun> = package
        .declarations
        .iter()
        .map(|declaration| run_declaration_with_given(package, declaration, extra_given))
        .collect();
    let mut summary = RunSummary::default();
    for declaration in &declarations {
        for test in &declaration.tests {
            summary.tests = summary.tests.saturating_add(1);
            if test.verdict.expect_passed() {
                summary.passed = summary.passed.saturating_add(1);
            } else if test.verdict.is_refused() {
                summary.refused = summary.refused.saturating_add(1);
            } else if test.verdict.is_computed() {
                summary.computed = summary.computed.saturating_add(1);
            } else {
                summary.failed = summary.failed.saturating_add(1);
            }
        }
    }
    RunReport {
        declarations,
        summary,
    }
}

/// Run the example tests of one declaration.
#[must_use]
pub fn run_declaration(package: &SemanticPackage, declaration: &Declaration) -> DeclarationRun {
    run_declaration_with_given(package, declaration, None)
}

/// Run one declaration, optionally adding a `_pane` worked run.
#[must_use]
pub fn run_declaration_with_given(
    package: &SemanticPackage,
    declaration: &Declaration,
    extra_given: Option<&BTreeMap<String, Value>>,
) -> DeclarationRun {
    let name = declaration.name.leaf().to_string();
    let mut tests: Vec<TestRun> = declaration
        .tests
        .iter()
        .filter_map(|test_id| package.tests.get(test_id.index()))
        .map(|test| run_test(package, declaration, test))
        .collect();
    if let Some(given) = extra_given {
        tests.push(run_direct(package, declaration, given));
    } else if declaration.tests.is_empty() {
        let empty = BTreeMap::new();
        if missing_binding(declaration, &empty).is_none() {
            tests.push(run_direct(package, declaration, &empty));
        }
    }
    let note = if tests.is_empty() {
        Some(ZERO_TEST_NOTE.to_string())
    } else {
        None
    };
    DeclarationRun { name, tests, note }
}

fn run_test(
    package: &SemanticPackage,
    declaration: &Declaration,
    test: &emath_ir::TestCase,
) -> TestRun {
    let mut run = TestRun {
        name: test.name.clone(),
        given: BTreeMap::new(),
        state: BTreeMap::new(),
        definitions: BTreeMap::new(),
        outputs: BTreeMap::new(),
        verdict: TestVerdict::Failed,
    };

    if declaration.constructors.len() > 1 {
        run.verdict = TestVerdict::LoweringRefused {
            detail: format!(
                "declaration `{}` has multiple constructors (Phase 1 supports one)",
                declaration.name.leaf()
            ),
        };
        return run;
    }

    match eval_givens(package, test) {
        Ok(given) => run.given = given,
        Err(verdict) => {
            run.verdict = verdict;
            return run;
        }
    }

    if let Some(constructor) = declaration.constructors.first() {
        match eval_constructor(package, declaration, constructor, &run.given) {
            Ok(state) => run.state = state,
            Err(verdict) => {
                run.verdict = verdict;
                return run;
            }
        }
    } else if !declaration.state.is_empty() {
        match seed_state_from_given(declaration, &run.given) {
            Ok(state) => run.state = state,
            Err(verdict) => {
                run.verdict = verdict;
                return run;
            }
        }
    }

    match eval_definitions(package, declaration, &run.given, &run.state) {
        Ok(definitions) => {
            run.outputs = outputs_of(package, declaration, &definitions);
            run.definitions = definitions;
        }
        Err(verdict) => {
            run.verdict = verdict;
            return run;
        }
    }

    run.verdict = eval_expect(
        package,
        declaration,
        test,
        &run.given,
        &run.definitions,
        &run.state,
    );
    run
}

fn run_direct(
    package: &SemanticPackage,
    declaration: &Declaration,
    given: &BTreeMap<String, Value>,
) -> TestRun {
    let mut run = TestRun {
        name: PANE_TEST_NAME.to_string(),
        given: given.clone(),
        state: BTreeMap::new(),
        definitions: BTreeMap::new(),
        outputs: BTreeMap::new(),
        verdict: TestVerdict::Computed,
    };

    if let Some(missing) = missing_binding(declaration, given) {
        run.verdict = TestVerdict::LoweringRefused {
            detail: format!("missing input `{missing}`"),
        };
        return run;
    }

    if declaration.constructors.len() > 1 {
        run.verdict = TestVerdict::LoweringRefused {
            detail: format!(
                "declaration `{}` has multiple constructors (Phase 1 supports one)",
                declaration.name.leaf()
            ),
        };
        return run;
    }

    if let Some(constructor) = declaration.constructors.first() {
        match eval_constructor(package, declaration, constructor, &run.given) {
            Ok(state) => run.state = state,
            Err(verdict) => {
                run.verdict = verdict;
                return run;
            }
        }
    } else if !declaration.state.is_empty() {
        match seed_state_from_given(declaration, &run.given) {
            Ok(state) => run.state = state,
            Err(verdict) => {
                run.verdict = verdict;
                return run;
            }
        }
    }

    match eval_definitions(package, declaration, &run.given, &run.state) {
        Ok(definitions) => {
            run.outputs = outputs_of(package, declaration, &definitions);
            run.definitions = definitions;
            run.verdict = TestVerdict::Computed;
        }
        Err(verdict) => {
            run.verdict = verdict;
        }
    }
    run
}

fn missing_binding(declaration: &Declaration, given: &BTreeMap<String, Value>) -> Option<String> {
    for field in &declaration.inputs {
        if !given.contains_key(&field.name) {
            return Some(field.name.clone());
        }
    }
    if let Some(constructor) = declaration.constructors.first() {
        for parameter in &constructor.parameters {
            if !given.contains_key(&parameter.name) {
                return Some(parameter.name.clone());
            }
        }
    }
    None
}

fn eval_givens(
    package: &SemanticPackage,
    test: &emath_ir::TestCase,
) -> Result<BTreeMap<String, Value>, TestVerdict> {
    let mut given = BTreeMap::new();
    let mut seen: Vec<String> = Vec::new();
    let mut seen_values: Vec<Value> = Vec::new();
    for name in test.given.keys() {
        let expr = test.given[name];
        let program = lower_definition(package, expr, &seen, &[])
            .map_err(|detail| TestVerdict::LoweringRefused { detail })?;
        match evaluate(&program, &seen_values, &[]) {
            Ok(value) => {
                given.insert(name.clone(), value.clone());
                seen.push(name.clone());
                seen_values.push(value);
            }
            Err(fault) => return Err(TestVerdict::Fault { fault }),
        }
    }
    Ok(given)
}

fn eval_constructor(
    package: &SemanticPackage,
    declaration: &Declaration,
    constructor: &emath_ir::Constructor,
    given: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>, TestVerdict> {
    let param_names: Vec<String> = constructor
        .parameters
        .iter()
        .map(|parameter| parameter.name.clone())
        .collect();
    let mut param_values = Vec::with_capacity(param_names.len());
    for name in &param_names {
        let Some(value) = given.get(name).cloned() else {
            return Err(TestVerdict::LoweringRefused {
                detail: format!("test body does not supply constructor parameter `{name}`"),
            });
        };
        param_values.push(value);
    }

    for precondition in &constructor.preconditions {
        check_obligation(
            package,
            *precondition,
            &param_names,
            &param_values,
            "require",
        )?;
    }

    let mut state = BTreeMap::new();
    for field in &declaration.state {
        let Some(expr) = constructor.assignments.get(&field.name).copied() else {
            return Err(TestVerdict::LoweringRefused {
                detail: format!("no `Self:` assignment for `{}`", field.name),
            });
        };
        let program = lower_definition(package, expr, &param_names, &[])
            .map_err(|detail| TestVerdict::LoweringRefused { detail })?;
        match evaluate(&program, &param_values, &[]) {
            Ok(value) => {
                state.insert(field.name.clone(), value);
            }
            Err(fault) => return Err(TestVerdict::Fault { fault }),
        }
    }

    for postcondition in &constructor.postconditions {
        check_obligation(
            package,
            *postcondition,
            &param_names,
            &param_values,
            "ensure",
        )?;
    }
    Ok(state)
}

fn check_obligation(
    package: &SemanticPackage,
    expr: ExprId,
    param_names: &[String],
    param_values: &[Value],
    keyword: &'static str,
) -> Result<(), TestVerdict> {
    let program = lower_requirement(package, expr, param_names)
        .map_err(|detail| TestVerdict::LoweringRefused { detail })?;
    match evaluate(&program, param_values, &[]) {
        Ok(Value::Bool(true)) => Ok(()),
        Ok(Value::Bool(false)) => Err(TestVerdict::ConstructorRefused {
            obligation: format!("{keyword} {}", expr_text(package, expr)),
        }),
        Ok(Value::F64(_))
        | Ok(Value::I64(_))
        | Ok(Value::Vector(_))
        | Ok(Value::Matrix { .. })
        | Ok(Value::Tensor { .. }) => {
            Err(TestVerdict::Fault {
                fault: EvalFault::TypeConfusion {
                    register: program.result.0,
                    op: keyword,
                },
            })
        }
        Err(fault) => Err(TestVerdict::Fault { fault }),
    }
}

fn eval_definitions(
    package: &SemanticPackage,
    declaration: &Declaration,
    given: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>, TestVerdict> {
    eval_definitions_values(package, declaration, given, state)
}

/// Definitions are let-bindings: admission validates each name against the
/// definitions admitted before it *in source order*, so evaluation must
/// follow the same order. The IR stores definitions name-keyed; expression
/// spans recover source order. Programmatic IR with default spans keeps the
/// name-keyed order (the sort is stable).
pub fn definition_order<'d>(
    package: &SemanticPackage,
    declaration: &'d Declaration,
) -> Vec<(&'d String, ExprId)> {
    let mut entries: Vec<(&'d String, ExprId)> = declaration
        .definitions
        .iter()
        .map(|(name, expr)| (name, *expr))
        .collect();
    entries.sort_by_key(|(_, expr)| package.expr_span(*expr).start);
    entries
}

fn seed_state_from_given(
    declaration: &Declaration,
    given: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>, TestVerdict> {
    let mut state = BTreeMap::new();
    for field in &declaration.state {
        let Some(value) = given.get(&field.name).cloned() else {
            return Err(TestVerdict::LoweringRefused {
                detail: format!("test body does not supply state `{name}`", name = field.name),
            });
        };
        state.insert(field.name.clone(), value);
    }
    Ok(state)
}

/// Explicit first-order stepper for `emath model` rates stored as `der_<state>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepMethod {
    /// Forward Euler: `x += h * f(x)`.
    Euler,
    /// Classic RK4.
    Rk4,
    /// Cash-Karp RK45. Fixed-step uses the 5th-order update; adaptive
    /// mode compares 4th vs 5th and accepts/rejects the step.
    Rk45,
}

/// Optional adaptive / event controls. Absent tolerances keep fixed `dt`.
#[derive(Clone, Debug, PartialEq)]
pub struct SimulateOptions {
    pub atol: Option<f64>,
    pub rtol: Option<f64>,
    pub dt_max: Option<f64>,
    /// Stop at the first crossing of `state[name] - value`.
    pub event: Option<(String, f64)>,
}

impl Default for SimulateOptions {
    fn default() -> Self {
        Self {
            atol: None,
            rtol: None,
            dt_max: None,
            event: None,
        }
    }
}

impl SimulateOptions {
    fn adaptive(&self) -> bool {
        self.atol.is_some() || self.rtol.is_some()
    }
}

/// One sample on a simulated trajectory.
#[derive(Clone, Debug, PartialEq)]
pub struct TrajectorySample {
    pub t: f64,
    pub state: BTreeMap<String, Value>,
}

/// Explicit trajectory from `t0` through `t1` at step `dt`.
#[derive(Clone, Debug, PartialEq)]
pub struct Trajectory {
    pub method: StepMethod,
    pub dt: f64,
    pub samples: Vec<TrajectorySample>,
}

/// Advance one explicit step. Rates come from admitted `der_<name>` definitions.
pub fn step_continuous(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, f64>,
    state: &BTreeMap<String, f64>,
    dt: f64,
    method: StepMethod,
) -> Result<BTreeMap<String, f64>, String> {
    let inputs = scalar_map_to_values(inputs);
    let state = scalar_map_to_values(state);
    let next = step_continuous_values(package, declaration, &inputs, &state, dt, method)?;
    values_to_scalars(&next)
}

/// Advance one explicit step, allowing vector-valued state and rates.
pub fn step_continuous_values(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    dt: f64,
    method: StepMethod,
) -> Result<BTreeMap<String, Value>, String> {
    if !dt.is_finite() || dt <= 0.0 {
        return Err("step size must be a positive finite Float64".to_string());
    }
    match method {
        StepMethod::Euler => {
            let rates = eval_rates(package, declaration, inputs, state)?;
            apply_scaled(state, &[(1.0, &rates)], dt)
        }
        StepMethod::Rk4 => {
            let k1 = eval_rates(package, declaration, inputs, state)?;
            let s2 = apply_scaled(state, &[(1.0, &k1)], dt / 2.0)?;
            let k2 = eval_rates(package, declaration, inputs, &s2)?;
            let s3 = apply_scaled(state, &[(1.0, &k2)], dt / 2.0)?;
            let k3 = eval_rates(package, declaration, inputs, &s3)?;
            let s4 = apply_scaled(state, &[(1.0, &k3)], dt)?;
            let k4 = eval_rates(package, declaration, inputs, &s4)?;
            apply_scaled(
                state,
                &[
                    (1.0 / 6.0, &k1),
                    (2.0 / 6.0, &k2),
                    (2.0 / 6.0, &k3),
                    (1.0 / 6.0, &k4),
                ],
                dt,
            )
        }
        StepMethod::Rk45 => {
            let stages = cash_karp_stages(package, declaration, inputs, state, dt)?;
            Ok(stages.fifth)
        }
    }
}

/// Integrate from `t0` to `t1` with fixed `dt`. Includes the sample at `t0`.
pub fn simulate_continuous(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    t0: f64,
    t1: f64,
    dt: f64,
    method: StepMethod,
) -> Result<Trajectory, String> {
    simulate_continuous_with(
        package,
        declaration,
        inputs,
        state,
        t0,
        t1,
        dt,
        method,
        &SimulateOptions::default(),
    )
}

/// Integrate from `t0` to `t1`. Adaptive dt and one event locator are optional.
pub fn simulate_continuous_with(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    t0: f64,
    t1: f64,
    dt: f64,
    method: StepMethod,
    options: &SimulateOptions,
) -> Result<Trajectory, String> {
    if !t0.is_finite() || !t1.is_finite() {
        return Err("trajectory times must be finite Float64".to_string());
    }
    if t1 < t0 {
        return Err("trajectory end must be >= start".to_string());
    }
    if !dt.is_finite() || dt <= 0.0 {
        return Err("step size must be a positive finite Float64".to_string());
    }
    if let Some(atol) = options.atol {
        if !atol.is_finite() || atol <= 0.0 {
            return Err("atol must be a positive finite Float64".to_string());
        }
    }
    if let Some(rtol) = options.rtol {
        if !rtol.is_finite() || rtol <= 0.0 {
            return Err("rtol must be a positive finite Float64".to_string());
        }
    }
    if let Some(dt_max) = options.dt_max {
        if !dt_max.is_finite() || dt_max <= 0.0 {
            return Err("dt-max must be a positive finite Float64".to_string());
        }
    }
    if options.adaptive() && method != StepMethod::Rk45 {
        return Err("adaptive dt requires --method rk45".to_string());
    }
    let mut samples = vec![TrajectorySample {
        t: t0,
        state: state.clone(),
    }];
    let mut current = state.clone();
    let mut t = t0;
    let mut h = match options.dt_max {
        Some(dt_max) => dt.min(dt_max),
        None => dt,
    };
    let mut guard = 0_u32;
    const MAX_STEPS: u32 = 1_000_000;
    while t < t1 {
        let remaining = t1 - t;
        if remaining <= 0.0 {
            break;
        }
        let step = h.min(remaining);
        if step <= 0.0 {
            break;
        }
        let (next, used, err) = if options.adaptive() {
            adaptive_rk45_try(package, declaration, inputs, &current, step, options)?
        } else {
            let next = step_continuous_values(package, declaration, inputs, &current, step, method)?;
            (next, step, 0.0)
        };
        if options.adaptive() && used < step && used <= 0.0 {
            return Err("adaptive step collapsed to a non-positive dt".to_string());
        }
        if options.adaptive() && used < step * 0.999 {
            h = used;
            guard += 1;
            if guard > MAX_STEPS {
                return Err("trajectory exceeded 1_000_000 steps".to_string());
            }
            continue;
        }
        if let Some((name, value)) = &options.event {
            if let Some((event_t, event_state)) = locate_event(
                package,
                declaration,
                inputs,
                &current,
                &next,
                t,
                used,
                name,
                *value,
                method,
            )?
            {
                samples.push(TrajectorySample {
                    t: event_t,
                    state: event_state,
                });
                return Ok(Trajectory {
                    method,
                    dt: h,
                    samples,
                });
            }
        }
        current = next;
        t += used;
        samples.push(TrajectorySample {
            t,
            state: current.clone(),
        });
        if options.adaptive() {
            h = grow_step(h, err, options, remaining);
        }
        guard += 1;
        if guard > MAX_STEPS {
            return Err("trajectory exceeded 1_000_000 steps".to_string());
        }
        if (t - t1).abs() <= h * 1e-12 {
            break;
        }
    }
    Ok(Trajectory {
        method,
        dt: h,
        samples,
    })
}

struct CashKarp {
    fourth: BTreeMap<String, Value>,
    fifth: BTreeMap<String, Value>,
}

fn cash_karp_stages(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    dt: f64,
) -> Result<CashKarp, String> {
    let k1 = eval_rates(package, declaration, inputs, state)?;
    let s2 = apply_scaled(state, &[(1.0 / 5.0, &k1)], dt)?;
    let k2 = eval_rates(package, declaration, inputs, &s2)?;
    let s3 = apply_scaled(state, &[(3.0 / 40.0, &k1), (9.0 / 40.0, &k2)], dt)?;
    let k3 = eval_rates(package, declaration, inputs, &s3)?;
    let s4 = apply_scaled(
        state,
        &[
            (3.0 / 10.0, &k1),
            (-9.0 / 10.0, &k2),
            (6.0 / 5.0, &k3),
        ],
        dt,
    )?;
    let k4 = eval_rates(package, declaration, inputs, &s4)?;
    let s5 = apply_scaled(
        state,
        &[
            (-11.0 / 54.0, &k1),
            (5.0 / 2.0, &k2),
            (-70.0 / 27.0, &k3),
            (35.0 / 27.0, &k4),
        ],
        dt,
    )?;
    let k5 = eval_rates(package, declaration, inputs, &s5)?;
    let s6 = apply_scaled(
        state,
        &[
            (1631.0 / 55296.0, &k1),
            (175.0 / 512.0, &k2),
            (575.0 / 13824.0, &k3),
            (44275.0 / 110592.0, &k4),
            (253.0 / 4096.0, &k5),
        ],
        dt,
    )?;
    let k6 = eval_rates(package, declaration, inputs, &s6)?;
    let fifth = apply_scaled(
        state,
        &[
            (37.0 / 378.0, &k1),
            (250.0 / 621.0, &k3),
            (125.0 / 594.0, &k4),
            (512.0 / 1771.0, &k6),
        ],
        dt,
    )?;
    let fourth = apply_scaled(
        state,
        &[
            (2825.0 / 27648.0, &k1),
            (18575.0 / 48384.0, &k3),
            (13525.0 / 55296.0, &k4),
            (277.0 / 14336.0, &k5),
            (1.0 / 4.0, &k6),
        ],
        dt,
    )?;
    Ok(CashKarp { fourth, fifth })
}

fn adaptive_rk45_try(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    dt: f64,
    options: &SimulateOptions,
) -> Result<(BTreeMap<String, Value>, f64, f64), String> {
    let stages = cash_karp_stages(package, declaration, inputs, state, dt)?;
    let err = state_error(&stages.fourth, &stages.fifth);
    let scale = error_scale(state, &stages.fifth, options);
    let rel = if scale > 0.0 { err / scale } else { err };
    if rel <= 1.0 {
        Ok((stages.fifth, dt, rel))
    } else {
        let next = (0.9 * dt * rel.powf(-0.2)).max(dt * 0.2);
        if next >= dt {
            return Err("adaptive step rejected but could not shrink dt".to_string());
        }
        Ok((stages.fifth, next, rel))
    }
}

fn grow_step(dt: f64, rel: f64, options: &SimulateOptions, remaining: f64) -> f64 {
    let grown = if rel <= 0.0 {
        dt * 5.0
    } else {
        (0.9 * dt * rel.powf(-0.2)).min(dt * 5.0).max(dt)
    };
    let grown = match options.dt_max {
        Some(dt_max) => grown.min(dt_max),
        None => grown,
    };
    grown.min(remaining.max(dt))
}

fn state_error(left: &BTreeMap<String, Value>, right: &BTreeMap<String, Value>) -> f64 {
    let mut max = 0.0_f64;
    for (name, a) in left {
        if let Some(b) = right.get(name) {
            max = max.max(value_abs_diff(a, b));
        }
    }
    max
}

fn error_scale(
    start: &BTreeMap<String, Value>,
    end: &BTreeMap<String, Value>,
    options: &SimulateOptions,
) -> f64 {
    let atol = options.atol.unwrap_or(1e-6);
    let rtol = options.rtol.unwrap_or(1e-3);
    let mut max = atol;
    for (name, a) in start {
        let mag = value_abs_max(a).max(end.get(name).map(value_abs_max).unwrap_or(0.0));
        max = max.max(atol + rtol * mag);
    }
    max
}

fn value_abs_diff(left: &Value, right: &Value) -> f64 {
    match (left, right) {
        (Value::F64(a), Value::F64(b)) => (a - b).abs(),
        (Value::I64(a), Value::I64(b)) => (*a as f64 - *b as f64).abs(),
        (Value::I64(a), Value::F64(b)) => (*a as f64 - b).abs(),
        (Value::F64(a), Value::I64(b)) => (a - *b as f64).abs(),
        (Value::Vector(a), Value::Vector(b)) => a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y).abs())
            .fold(0.0, f64::max),
        (
            Value::Matrix { data: a, .. },
            Value::Matrix { data: b, .. },
        )
        | (
            Value::Tensor { data: a, .. },
            Value::Tensor { data: b, .. },
        ) => a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y).abs())
            .fold(0.0, f64::max),
        _ => f64::INFINITY,
    }
}

fn value_abs_max(value: &Value) -> f64 {
    match value {
        Value::F64(number) => number.abs(),
        Value::I64(number) => (*number as f64).abs(),
        Value::Bool(_) => 0.0,
        Value::Vector(items) => items.iter().fold(0.0, |acc, item| acc.max(item.abs())),
        Value::Matrix { data, .. } | Value::Tensor { data, .. } => {
            data.iter().fold(0.0, |acc, item| acc.max(item.abs()))
        }
    }
}

fn locate_event(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    start: &BTreeMap<String, Value>,
    end: &BTreeMap<String, Value>,
    t0: f64,
    dt: f64,
    name: &str,
    target: f64,
    method: StepMethod,
) -> Result<Option<(f64, BTreeMap<String, Value>)>, String> {
    let g0 = event_gap(start, name, target)?;
    let g1 = event_gap(end, name, target)?;
    if g0 == 0.0 {
        return Ok(Some((t0, start.clone())));
    }
    if g0 * g1 > 0.0 {
        return Ok(None);
    }
    let mut lo_t = t0;
    let mut hi_t = t0 + dt;
    let mut lo = start.clone();
    let mut hi = end.clone();
    let mut glo = g0;
    for _ in 0..40 {
        let mid_t = 0.5 * (lo_t + hi_t);
        let mid = step_continuous_values(package, declaration, inputs, &lo, mid_t - lo_t, method)?;
        let gmid = event_gap(&mid, name, target)?;
        if gmid == 0.0 || (hi_t - lo_t).abs() <= 1e-12 {
            return Ok(Some((mid_t, mid)));
        }
        if glo.signum() == gmid.signum() {
            lo_t = mid_t;
            lo = mid;
            glo = gmid;
        } else {
            hi_t = mid_t;
            hi = mid;
        }
    }
    Ok(Some((hi_t, hi)))
}

fn event_gap(
    state: &BTreeMap<String, Value>,
    name: &str,
    target: f64,
) -> Result<f64, String> {
    let Some(value) = state.get(name) else {
        return Err(format!("event state `{name}` is missing"));
    };
    match value {
        Value::F64(number) => Ok(*number - target),
        Value::I64(number) => Ok(*number as f64 - target),
        _ => Err(format!("event state `{name}` must be a scalar")),
    }
}

fn scalar_map_to_values(map: &BTreeMap<String, f64>) -> BTreeMap<String, Value> {
    map.iter()
        .map(|(name, value)| (name.clone(), Value::F64(*value)))
        .collect()
}

fn values_to_scalars(map: &BTreeMap<String, Value>) -> Result<BTreeMap<String, f64>, String> {
    let mut out = BTreeMap::new();
    for (name, value) in map {
        match value {
            Value::F64(number) => {
                out.insert(name.clone(), *number);
            }
            Value::I64(number) => {
                out.insert(name.clone(), *number as f64);
            }
            _ => return Err(format!("state `{name}` is not a scalar")),
        }
    }
    Ok(out)
}

fn apply_scaled(
    state: &BTreeMap<String, Value>,
    terms: &[(f64, &BTreeMap<String, Value>)],
    dt: f64,
) -> Result<BTreeMap<String, Value>, String> {
    let mut next = BTreeMap::new();
    for (name, value) in state {
        let mut acc = value.clone();
        for (weight, rates) in terms {
            let rate = rates
                .get(name)
                .ok_or_else(|| format!("missing rate `der_{name}`"))?;
            acc = add_scaled(&acc, rate, dt * *weight)?;
        }
        next.insert(name.clone(), acc);
    }
    Ok(next)
}

fn add_scaled(value: &Value, rate: &Value, scale: f64) -> Result<Value, String> {
    match (value, rate) {
        (Value::F64(x), Value::F64(r)) => Ok(Value::F64(x + scale * r)),
        (Value::I64(x), Value::F64(r)) => Ok(Value::F64(*x as f64 + scale * r)),
        (Value::F64(x), Value::I64(r)) => Ok(Value::F64(x + scale * *r as f64)),
        (Value::I64(x), Value::I64(r)) => Ok(Value::F64(*x as f64 + scale * *r as f64)),
        (Value::Vector(x), Value::Vector(r)) if x.len() == r.len() => Ok(Value::Vector(
            x.iter()
                .zip(r.iter())
                .map(|(a, b)| a + scale * b)
                .collect(),
        )),
        (
            Value::Matrix {
                rows: r1,
                cols: c1,
                data: d1,
            },
            Value::Matrix {
                rows: r2,
                cols: c2,
                data: d2,
            },
        ) if r1 == r2 && c1 == c2 && d1.len() == d2.len() => Ok(Value::Matrix {
            rows: *r1,
            cols: *c1,
            data: d1
                .iter()
                .zip(d2.iter())
                .map(|(a, b)| a + scale * b)
                .collect(),
        }),
        (
            Value::Tensor {
                shape: s1,
                data: d1,
            },
            Value::Tensor {
                shape: s2,
                data: d2,
            },
        ) if s1 == s2 && d1.len() == d2.len() => Ok(Value::Tensor {
            shape: s1.clone(),
            data: d1
                .iter()
                .zip(d2.iter())
                .map(|(a, b)| a + scale * b)
                .collect(),
        }),
        _ => Err("state and rate must have the same scalar/vector/matrix/tensor shape".to_string()),
    }
}

fn eval_rates(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>, String> {
    let definitions = eval_definitions_values(package, declaration, inputs, state).map_err(
        |verdict| {
            verdict
                .reason_text()
                .unwrap_or_else(|| verdict.to_string())
        },
    )?;
    let mut rates = BTreeMap::new();
    for field in &declaration.state {
        let key = format!("der_{}", field.name);
        let Some(value) = definitions.get(&key) else {
            return Err(format!("missing rate `{key}`"));
        };
        match value {
            Value::Bool(_) => return Err(format!("rate `{key}` is not numeric")),
            Value::I64(_) | Value::F64(_) | Value::Vector(_) | Value::Matrix { .. } | Value::Tensor { .. } => {
                rates.insert(field.name.clone(), value.clone());
            }
        }
    }
    Ok(rates)
}

fn eval_definitions_values(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>, TestVerdict> {
    let input_names: Vec<String> = declaration
        .inputs
        .iter()
        .map(|field| field.name.clone())
        .collect();
    let mut bind_names = input_names;
    let mut bind_values = Vec::with_capacity(bind_names.len());
    for name in &bind_names {
        let Some(value) = inputs.get(name).cloned() else {
            return Err(TestVerdict::LoweringRefused {
                detail: format!("test body does not supply input `{name}`"),
            });
        };
        bind_values.push(value);
    }
    let state_names: Vec<String> = declaration
        .state
        .iter()
        .map(|field| field.name.clone())
        .collect();
    let mut state_values = Vec::with_capacity(state_names.len());
    for name in &state_names {
        let Some(value) = state.get(name).cloned() else {
            return Err(TestVerdict::LoweringRefused {
                detail: format!("missing state `{name}`"),
            });
        };
        state_values.push(value);
    }

    let mut definitions = BTreeMap::new();
    for (name, expr) in definition_order(package, declaration) {
        let program = lower_definition(package, expr, &bind_names, &state_names)
            .map_err(|detail| TestVerdict::LoweringRefused { detail })?;
        match evaluate(&program, &bind_values, &state_values) {
            Ok(value) => {
                definitions.insert(name.clone(), value.clone());
                if !bind_names.iter().any(|existing| existing == name) {
                    bind_names.push(name.clone());
                    bind_values.push(value);
                }
            }
            Err(fault) => return Err(TestVerdict::Fault { fault }),
        }
    }
    Ok(definitions)
}

fn outputs_of(
    package: &SemanticPackage,
    declaration: &Declaration,
    definitions: &BTreeMap<String, Value>,
) -> BTreeMap<String, Value> {
    let mut outputs = BTreeMap::new();
    for field in &declaration.outputs {
        if let Some(value) = definitions.get(&field.name).cloned() {
            let value = match (&value, package.ty(field.ty)) {
                (Value::I64(n), Some(TypeNode::Float64)) => Value::F64(*n as f64),
                (Value::F64(n), Some(TypeNode::Int | TypeNode::Nat))
                    if n.is_finite() && n.fract() == 0.0 =>
                {
                    Value::I64(*n as i64)
                }
                (v, _) => v.clone(),
            };
            outputs.insert(field.name.clone(), value);
        }
    }
    outputs
}

fn eval_expect(
    package: &SemanticPackage,
    declaration: &Declaration,
    test: &emath_ir::TestCase,
    given: &BTreeMap<String, Value>,
    definitions: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
) -> TestVerdict {
    let given_names: Vec<String> = given.keys().cloned().collect();
    let mut expect_names = given_names.clone();
    let mut expect_values: Vec<Value> = given.values().cloned().collect();
    for name in declaration.definitions.keys() {
        if given_names.iter().any(|given_name| given_name == name) {
            continue;
        }
        let Some(value) = definitions.get(name) else {
            continue;
        };
        expect_names.push(name.clone());
        expect_values.push(value.clone());
    }
    let Some(expect) = test.expect else {
        return TestVerdict::Computed;
    };
    let state_names: Vec<String> = declaration
        .state
        .iter()
        .map(|field| field.name.clone())
        .collect();
    let state_values: Vec<Value> = state_names
        .iter()
        .map(|name| state.get(name).cloned().unwrap_or(Value::F64(f64::NAN)))
        .collect();
    let program = match lower_definition(package, expect, &expect_names, &state_names) {
        Ok(program) => program,
        Err(detail) => return TestVerdict::LoweringRefused { detail },
    };
    match evaluate(&program, &expect_values, &state_values) {
        Ok(Value::Bool(true)) => TestVerdict::Passed,
        Ok(Value::Bool(false)) => TestVerdict::Failed,
        Ok(Value::F64(_))
        | Ok(Value::I64(_))
        | Ok(Value::Vector(_))
        | Ok(Value::Matrix { .. })
        | Ok(Value::Tensor { .. }) => {
            TestVerdict::Fault {
                fault: EvalFault::TypeConfusion {
                    register: program.result.0,
                    op: "expect",
                },
            }
        }
        Err(fault) => TestVerdict::Fault { fault },
    }
}

fn expr_text(package: &SemanticPackage, id: ExprId) -> String {
    let Some(expr) = package.expr(id) else {
        return format!("<expr {}>", id.0);
    };
    match expr {
        ExprNode::Literal(Literal::Integer(text)) => text.clone(),
        ExprNode::Literal(Literal::FloatBits(bits)) => {
            crate::interp::format_f64(f64::from_bits(*bits))
        }
        ExprNode::Literal(Literal::Bool(flag)) => {
            if *flag {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        ExprNode::Literal(Literal::Text(text)) => format!("\"{text}\""),
        ExprNode::Literal(Literal::Rational(text)) => text.clone(),
        ExprNode::Variable(name) => name.0.clone(),
        ExprNode::Call {
            function,
            arguments,
        } => {
            let args: Vec<String> = arguments
                .iter()
                .map(|argument| expr_text(package, *argument))
                .collect();
            format!("{}({})", function.leaf(), args.join(", "))
        }
        ExprNode::Unary { operation, value } => {
            format!(
                "{}({})",
                unary_symbol(*operation),
                expr_text(package, *value)
            )
        }
        ExprNode::Binary {
            operation,
            left,
            right,
        } => match operation {
            BinaryOp::Min | BinaryOp::Max | BinaryOp::Atan2 => format!(
                "{}({}, {})",
                operation.name(),
                expr_text(package, *left),
                expr_text(package, *right)
            ),
            _ => format!(
                "{} {} {}",
                expr_text(package, *left),
                bin_symbol(*operation),
                expr_text(package, *right)
            ),
        },
        ExprNode::If {
            condition,
            then_value,
            else_value,
        } => format!(
            "if {} then {} else {}",
            expr_text(package, *condition),
            expr_text(package, *then_value),
            expr_text(package, *else_value)
        ),
        other => format!("{:?}", std::mem::discriminant(other)),
    }
}

fn unary_symbol(operation: UnaryOp) -> &'static str {
    match operation {
        UnaryOp::Negate => "-",
        UnaryOp::Not => "!",
        other => other.name(),
    }
}

fn bin_symbol(operation: BinaryOp) -> &'static str {
    match operation {
        BinaryOp::StrictFloatAdd => "+",
        BinaryOp::StrictFloatSub => "-",
        BinaryOp::StrictFloatMul => "*",
        BinaryOp::StrictFloatDiv => "/",
        BinaryOp::StrictFloatPow => "^",
        BinaryOp::Equal => "==",
        BinaryOp::NotEqual => "!=",
        BinaryOp::Less => "<",
        BinaryOp::LessEqual => "<=",
        BinaryOp::Greater => ">",
        BinaryOp::GreaterEqual => ">=",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
        other => other.name(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emath_core::{QualifiedName, Span};
    use emath_ir::{
        BinaryOp, Constructor, DeclarationId, ExprNode, Field, Literal, TypeNode, Visibility,
    };

    fn float_field(name: &str, ty: emath_ir::TypeId) -> Field {
        Field {
            name: name.to_string(),
            ty,
            visibility: Visibility::Public,
            source: Span::default(),
        }
    }

    fn square_package(expect_rhs: &str) -> SemanticPackage {
        let mut package = SemanticPackage::new();
        let ty = package.push_type(TypeNode::Float64);
        let x = package.push_expr(
            ExprNode::Variable(QualifiedName::single("x")),
            Span::default(),
        );
        let y_def = package.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::StrictFloatMul,
                left: x,
                right: x,
            },
            Span::default(),
        );
        let three = package.push_expr(
            ExprNode::Literal(Literal::Integer("3".to_string())),
            Span::default(),
        );
        let y = package.push_expr(
            ExprNode::Variable(QualifiedName::single("y")),
            Span::default(),
        );
        let nine = package.push_expr(
            ExprNode::Literal(Literal::Integer(expect_rhs.to_string())),
            Span::default(),
        );
        let expect = package.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::Equal,
                left: y,
                right: nine,
            },
            Span::default(),
        );
        let mut given = BTreeMap::new();
        given.insert("x".to_string(), three);
        let test_id = package.push_test(emath_ir::TestCase {
            name: "three_squared".to_string(),
            given,
            expect: Some(expect),
            source: Span::default(),
        });
        let mut definitions = BTreeMap::new();
        definitions.insert("y".to_string(), y_def);
        package.declarations.push(emath_ir::Declaration {
            id: DeclarationId(0),
            name: QualifiedName::single("Square"),
            kind: QualifiedName::single("function"),
            kind_label: "function".to_string(),
            inputs: vec![float_field("x", ty)],
            outputs: vec![float_field("y", ty)],
            state: Vec::new(),
            constructors: Vec::new(),
            definitions,
            invariants: Vec::new(),
            goals: Vec::new(),
            tests: vec![test_id],
            exports: Vec::new(),
            compile_spec: emath_ir::CompileSpec::default(),
            about: None,
            evidence: Vec::new(),
            host: Vec::new(),
            source: Span::default(),
        });
        package
    }

    fn square_worked(given_literal: &str) -> SemanticPackage {
        let mut package = SemanticPackage::new();
        let ty = package.push_type(TypeNode::Float64);
        let x = package.push_expr(
            ExprNode::Variable(QualifiedName::single("x")),
            Span::default(),
        );
        let y_def = package.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::StrictFloatMul,
                left: x,
                right: x,
            },
            Span::default(),
        );
        let given_expr = package.push_expr(
            ExprNode::Literal(Literal::Integer(given_literal.to_string())),
            Span::default(),
        );
        let mut given = BTreeMap::new();
        given.insert("x".to_string(), given_expr);
        let test_id = package.push_test(emath_ir::TestCase {
            name: "four_squared".to_string(),
            given,
            expect: None,
            source: Span::default(),
        });
        let mut definitions = BTreeMap::new();
        definitions.insert("y".to_string(), y_def);
        package.declarations.push(emath_ir::Declaration {
            id: DeclarationId(0),
            name: QualifiedName::single("Square"),
            kind: QualifiedName::single("function"),
            kind_label: "function".to_string(),
            inputs: vec![float_field("x", ty)],
            outputs: vec![float_field("y", ty)],
            state: Vec::new(),
            constructors: Vec::new(),
            definitions,
            invariants: Vec::new(),
            goals: Vec::new(),
            tests: vec![test_id],
            exports: Vec::new(),
            compile_spec: emath_ir::CompileSpec::default(),
            about: None,
            evidence: Vec::new(),
            host: Vec::new(),
            source: Span::default(),
        });
        package
    }

    #[test]
    fn runner_square_worked_example_computes() {
        let report = run_package(&square_worked("4"));
        assert_eq!(report.summary.tests, 1);
        assert_eq!(report.summary.computed, 1);
        assert_eq!(report.summary.passed, 0);
        assert_eq!(report.summary.failed, 0);
        let test = &report.declarations[0].tests[0];
        assert_eq!(test.verdict, TestVerdict::Computed);
        assert_eq!(test.given.get("x").cloned(), Some(Value::F64(4.0)));
        assert_eq!(test.definitions.get("y"), Some(&Value::F64(16.0)));
        assert_eq!(test.outputs.get("y"), Some(&Value::F64(16.0)));
    }

    #[test]
    fn runner_square_expect_passes() {
        let report = run_package(&square_package("9"));
        assert_eq!(report.summary.tests, 1);
        assert_eq!(report.summary.passed, 1);
        assert_eq!(report.summary.failed, 0);
        let test = &report.declarations[0].tests[0];
        assert_eq!(test.verdict, TestVerdict::Passed);
        assert_eq!(test.given.get("x").cloned(), Some(Value::F64(3.0)));
        assert_eq!(test.definitions.get("y"), Some(&Value::F64(9.0)));
        assert_eq!(test.outputs.get("y"), Some(&Value::F64(9.0)));
    }

    #[test]
    fn runner_square_expect_fails() {
        let report = run_package(&square_package("8"));
        assert_eq!(report.summary.failed, 1);
        assert!(!report.declarations[0].tests[0].verdict.expect_passed());
    }

    #[test]
    fn runner_constant_only_declaration_computes() {
        let mut package = SemanticPackage::new();
        let ty = package.push_type(TypeNode::Float64);
        let three = package.push_expr(
            ExprNode::Literal(Literal::Integer("3".to_string())),
            Span::default(),
        );
        let seven = package.push_expr(
            ExprNode::Literal(Literal::Integer("7".to_string())),
            Span::default(),
        );
        let y_def = package.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::StrictFloatMul,
                left: three,
                right: seven,
            },
            Span::default(),
        );
        let y = package.push_expr(
            ExprNode::Variable(QualifiedName::single("y")),
            Span::default(),
        );
        let twenty_one = package.push_expr(
            ExprNode::Literal(Literal::Integer("21".to_string())),
            Span::default(),
        );
        let expect = package.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::Equal,
                left: y,
                right: twenty_one,
            },
            Span::default(),
        );
        let test_id = package.push_test(emath_ir::TestCase {
            name: "worked".to_string(),
            given: BTreeMap::new(),
            expect: Some(expect),
            source: Span::default(),
        });
        let mut definitions = BTreeMap::new();
        definitions.insert("y".to_string(), y_def);
        package.declarations.push(emath_ir::Declaration {
            id: DeclarationId(0),
            name: QualifiedName::single("TwentyOne"),
            kind: QualifiedName::single("function"),
            kind_label: "function".to_string(),
            inputs: Vec::new(),
            outputs: vec![float_field("y", ty)],
            state: Vec::new(),
            constructors: Vec::new(),
            definitions,
            invariants: Vec::new(),
            goals: Vec::new(),
            tests: vec![test_id],
            exports: Vec::new(),
            compile_spec: emath_ir::CompileSpec::default(),
            about: None,
            evidence: Vec::new(),
            host: Vec::new(),
            source: Span::default(),
        });
        let report = run_package(&package);
        assert_eq!(report.summary.tests, 1);
        assert_eq!(report.summary.passed, 1);
        assert_eq!(report.summary.failed, 0);
        assert_eq!(report.summary.refused, 0);
        let test = &report.declarations[0].tests[0];
        assert!(test.given.is_empty());
        assert_eq!(test.verdict, TestVerdict::Passed);
        assert_eq!(test.definitions.get("y"), Some(&Value::F64(21.0)));
        assert_eq!(test.outputs.get("y"), Some(&Value::F64(21.0)));
    }

    fn square_no_tests() -> SemanticPackage {
        let mut package = SemanticPackage::new();
        let ty = package.push_type(TypeNode::Float64);
        let x = package.push_expr(
            ExprNode::Variable(QualifiedName::single("x")),
            Span::default(),
        );
        let y_def = package.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::StrictFloatMul,
                left: x,
                right: x,
            },
            Span::default(),
        );
        let mut definitions = BTreeMap::new();
        definitions.insert("y".to_string(), y_def);
        package.declarations.push(emath_ir::Declaration {
            id: DeclarationId(0),
            name: QualifiedName::single("Square"),
            kind: QualifiedName::single("function"),
            kind_label: "function".to_string(),
            inputs: vec![float_field("x", ty)],
            outputs: vec![float_field("y", ty)],
            state: Vec::new(),
            constructors: Vec::new(),
            definitions,
            invariants: Vec::new(),
            goals: Vec::new(),
            tests: Vec::new(),
            exports: Vec::new(),
            compile_spec: emath_ir::CompileSpec::default(),
            about: None,
            evidence: Vec::new(),
            host: Vec::new(),
            source: Span::default(),
        });
        package
    }

    #[test]
    fn runner_zero_tests_notes() {
        let report = run_package(&square_no_tests());
        assert_eq!(report.declarations[0].tests.len(), 0);
        assert_eq!(report.declarations[0].note.as_deref(), Some(ZERO_TEST_NOTE));
        assert_eq!(report.summary.tests, 0);
    }

    #[test]
    fn runner_zero_tests_computes_when_inputs_bound() {
        let mut package = SemanticPackage::new();
        let ty = package.push_type(TypeNode::Float64);
        let two = package.push_expr(
            ExprNode::Literal(Literal::Integer("2".to_string())),
            Span::default(),
        );
        let a_var = package.push_expr(
            ExprNode::Variable(QualifiedName::single("a")),
            Span::default(),
        );
        let b_def = package.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::StrictFloatMul,
                left: a_var,
                right: a_var,
            },
            Span::default(),
        );
        let mut definitions = BTreeMap::new();
        definitions.insert("a".to_string(), two);
        definitions.insert("b".to_string(), b_def);
        package.declarations.push(emath_ir::Declaration {
            id: DeclarationId(0),
            name: QualifiedName::single("Pane"),
            kind: QualifiedName::single("function"),
            kind_label: "function".to_string(),
            inputs: Vec::new(),
            outputs: vec![float_field("a", ty), float_field("b", ty)],
            state: Vec::new(),
            constructors: Vec::new(),
            definitions,
            invariants: Vec::new(),
            goals: Vec::new(),
            tests: Vec::new(),
            exports: Vec::new(),
            compile_spec: emath_ir::CompileSpec::default(),
            about: None,
            evidence: Vec::new(),
            host: Vec::new(),
            source: Span::default(),
        });
        let report = run_package(&package);
        assert_eq!(report.summary.tests, 1);
        assert_eq!(report.summary.computed, 1);
        let test = &report.declarations[0].tests[0];
        assert_eq!(test.name, PANE_TEST_NAME);
        assert_eq!(test.verdict, TestVerdict::Computed);
        assert_eq!(test.definitions.get("a"), Some(&Value::F64(2.0)));
        assert_eq!(test.definitions.get("b"), Some(&Value::F64(4.0)));
    }

    #[test]
    fn runner_definitions_evaluate_in_source_order_not_name_order() {
        // `z = 2` precedes `a = z * z` in the source; name order would
        // evaluate `a` first and misread `z` as an unbound input.
        let mut package = SemanticPackage::new();
        let ty = package.push_type(TypeNode::Float64);
        let file = Span::default().file;
        let two = package.push_expr(
            ExprNode::Literal(Literal::Integer("2".to_string())),
            Span::new(file, 10, 11),
        );
        let z_var = package.push_expr(
            ExprNode::Variable(QualifiedName::single("z")),
            Span::new(file, 20, 21),
        );
        let a_def = package.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::StrictFloatMul,
                left: z_var,
                right: z_var,
            },
            Span::new(file, 20, 25),
        );
        let mut definitions = BTreeMap::new();
        definitions.insert("z".to_string(), two);
        definitions.insert("a".to_string(), a_def);
        package.declarations.push(emath_ir::Declaration {
            id: DeclarationId(0),
            name: QualifiedName::single("Pane"),
            kind: QualifiedName::single("function"),
            kind_label: "function".to_string(),
            inputs: Vec::new(),
            outputs: vec![float_field("a", ty), float_field("z", ty)],
            state: Vec::new(),
            constructors: Vec::new(),
            definitions,
            invariants: Vec::new(),
            goals: Vec::new(),
            tests: Vec::new(),
            exports: Vec::new(),
            compile_spec: emath_ir::CompileSpec::default(),
            about: None,
            evidence: Vec::new(),
            host: Vec::new(),
            source: Span::default(),
        });
        let report = run_package(&package);
        assert_eq!(report.summary.computed, 1, "{report:?}");
        let test = &report.declarations[0].tests[0];
        assert_eq!(test.verdict, TestVerdict::Computed);
        assert_eq!(test.definitions.get("z"), Some(&Value::F64(2.0)));
        assert_eq!(test.definitions.get("a"), Some(&Value::F64(4.0)));
    }

    #[test]
    fn runner_pane_given_computes_and_missing_refuses() {
        let package = square_no_tests();
        let mut given = BTreeMap::new();
        given.insert("x".to_string(), Value::F64(5.0));
        let report = run_package_with_given(&package, Some(&given));
        assert_eq!(report.summary.computed, 1);
        let test = &report.declarations[0].tests[0];
        assert_eq!(test.name, PANE_TEST_NAME);
        assert_eq!(test.definitions.get("y"), Some(&Value::F64(25.0)));

        let empty = BTreeMap::new();
        let refused = run_package_with_given(&package, Some(&empty));
        assert_eq!(refused.summary.refused, 1);
        match &refused.declarations[0].tests[0].verdict {
            TestVerdict::LoweringRefused { detail } => {
                assert!(detail.contains("`x`"), "{detail}");
            }
            other => panic!("expected missing-input refusal, got {other:?}"),
        }
    }

    #[test]
    fn runner_constructor_refuses_false_require() {
        let mut package = SemanticPackage::new();
        let ty = package.push_type(TypeNode::Float64);
        let scale = package.push_expr(
            ExprNode::Variable(QualifiedName::single("scale")),
            Span::default(),
        );
        let zero = package.push_expr(
            ExprNode::Literal(Literal::Integer("0".to_string())),
            Span::default(),
        );
        let require = package.push_expr(
            ExprNode::Binary {
                operation: BinaryOp::GreaterEqual,
                left: scale,
                right: zero,
            },
            Span::default(),
        );
        let neg = package.push_expr(
            ExprNode::Literal(Literal::Integer("-1".to_string())),
            Span::default(),
        );
        let x = package.push_expr(
            ExprNode::Literal(Literal::Integer("1".to_string())),
            Span::default(),
        );
        let expect = package.push_expr(ExprNode::Literal(Literal::Bool(true)), Span::default());
        let mut given = BTreeMap::new();
        given.insert("scale".to_string(), neg);
        given.insert("x".to_string(), x);
        let test_id = package.push_test(emath_ir::TestCase {
            name: "bad_scale".to_string(),
            given,
            expect: Some(expect),
            source: Span::default(),
        });
        let mut assignments = BTreeMap::new();
        assignments.insert("scale".to_string(), scale);
        package.declarations.push(emath_ir::Declaration {
            id: DeclarationId(0),
            name: QualifiedName::single("Policy"),
            kind: QualifiedName::single("policy"),
            kind_label: "policy".to_string(),
            inputs: vec![float_field("x", ty)],
            outputs: Vec::new(),
            state: vec![float_field("scale", ty)],
            constructors: vec![Constructor {
                name: "new".to_string(),
                parameters: vec![float_field("scale", ty)],
                preconditions: vec![require],
                assignments,
                postconditions: Vec::new(),
                defaults: BTreeMap::new(),
                error_type: None,
                is_public: true,
                source: Span::default(),
            }],
            definitions: BTreeMap::new(),
            invariants: Vec::new(),
            goals: Vec::new(),
            tests: vec![test_id],
            exports: Vec::new(),
            compile_spec: emath_ir::CompileSpec::default(),
            about: None,
            evidence: Vec::new(),
            host: Vec::new(),
            source: Span::default(),
        });
        let report = run_package(&package);
        assert_eq!(report.summary.refused, 1);
        match &report.declarations[0].tests[0].verdict {
            TestVerdict::ConstructorRefused { obligation } => {
                assert!(obligation.contains("scale"), "{obligation}");
            }
            other => panic!("expected constructor refused, got {other:?}"),
        }
    }
}
