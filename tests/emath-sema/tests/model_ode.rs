//! Continuous `emath model` admission: explicit `derivative(state) = rhs`.
use std::collections::BTreeMap;

use emath_exec_ir::interp::{Value, evaluate};
use emath_exec_ir::{StepMethod, TrajectorySample, lower_definition, simulate_continuous, step_continuous_values};
use emath_ir::{Declaration, ExprNode, SemanticPackage};
use emath_test_harness::{boot, Probe, Source};

fn decay_model() -> &'static str {
    "\
emath model Decay:
    inputs:
        k: Float64
    state:
        x: Float64
    equations:
        derivative(x) = -k * x
"
}

fn algebraic_residual_max(
    p: &mut Probe,
    name: &str,
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
) -> f64 {
    let Some(residuals) = package.residuals.get(&declaration.id) else {
        return 0.0;
    };
    let mut bind_names: Vec<String> = declaration
        .inputs
        .iter()
        .map(|field| field.name.clone())
        .collect();
    for field in &declaration.algebraic {
        bind_names.push(field.name.clone());
    }
    let state_names: Vec<String> = declaration
        .state
        .iter()
        .map(|field| field.name.clone())
        .collect();
    let bind_values: Vec<Value> = bind_names
        .iter()
        .map(|bind| match state.get(bind).cloned().or_else(|| inputs.get(bind).cloned()) {
            Some(value) => value,
            None => {
                p.fail(name, format!("missing bind `{bind}`"));
                Value::F64(f64::NAN)
            }
        })
        .collect();
    let state_values: Vec<Value> = state_names
        .iter()
        .map(|bind| match state.get(bind).cloned() {
            Some(value) => value,
            None => {
                p.fail(name, format!("missing state `{bind}`"));
                Value::F64(f64::NAN)
            }
        })
        .collect();
    let mut max = 0.0_f64;
    for residual in residuals {
        let program = match lower_definition(package, residual.expr, &bind_names, &state_names) {
            Ok(program) => program,
            Err(error) => {
                p.fail(name, format!("residual lowering: {error}"));
                continue;
            }
        };
        match evaluate(&program, &bind_values, &state_values) {
            Ok(Value::F64(value)) => max = max.max(value.abs()),
            Ok(Value::I64(value)) => max = max.max((value as f64).abs()),
            Ok(Value::Vector(items)) => {
                for item in items {
                    max = max.max(item.abs());
                }
            }
            Ok(other) => {
                p.fail(name, format!("residual must be numeric, got {other:?}"));
            }
            Err(error) => {
                p.fail(name, format!("residual eval: {error:?}"));
            }
        }
    }
    max
}

fn undamped_spring(p: &mut Probe) -> Option<(SemanticPackage, BTreeMap<String, Value>, BTreeMap<String, Value>)> {
    let result = Source::from_workspace("tests/fixtures/language/numerical/explicit-mass-spring.emath").must_admit(p);
    if result.diagnostics.has_errors() {
        return None;
    }
    let mut inputs = BTreeMap::new();
    inputs.insert("m".into(), Value::F64(1.0));
    inputs.insert("c".into(), Value::F64(0.0));
    inputs.insert("k".into(), Value::F64(1.0));
    let mut state = BTreeMap::new();
    state.insert("s".into(), Value::Vector(vec![1.0, 0.0]));
    Some((result.package, inputs, state))
}

fn spring_xv(p: &mut Probe, name: &str, sample: &TrajectorySample) -> (f64, f64) {
    match sample.state.get("s") {
        Some(Value::Vector(components)) if components.len() == 2 => (components[0], components[1]),
        other => {
            p.fail(name, format!("expected s=[x, v], got {other:?}"));
            (f64::NAN, f64::NAN)
        }
    }
}

fn f64_cell(p: &mut Probe, name: &str, state: &BTreeMap<String, Value>, key: &str) -> f64 {
    match state.get(key) {
        Some(Value::F64(value)) => *value,
        other => {
            p.fail(name, format!("expected F64 `{key}`, got {other:?}"));
            f64::NAN
        }
    }
}

fn numeric_cells(p: &mut Probe, name: &str, value: Option<&Value>) -> Vec<f64> {
    match value {
        Some(Value::Vector(values)) => values.clone(),
        Some(Value::Matrix { data, .. }) => data.clone(),
        Some(Value::Tensor { data, .. }) => data.clone(),
        other => {
            p.fail(name, format!("expected numeric state, got {other:?}"));
            Vec::new()
        }
    }
}

const FN_STATE: &str = "emath function Bad:\n    state:\n        x: Float64\n    definitions:\n        y = x\n";
const FN_CTOR: &str = "emath function Bad:\n    constructors:\n        public fn new() -> Self\n";
const FN_EQUATIONS: &str = "emath function Bad:\n    definitions:\n        y = 1\n    equations:\n        derivative(x) = 0\n";
const FN_ALGEBRAIC: &str = "emath function Bad:\n    inputs:\n        x: Float64\n    algebraic:\n        y: Float64\n    definitions:\n        y = x\n";
const MODEL_DEFS_ONLY: &str = "emath model Stateless:\n    definitions:\n        y = 1\n";
const SPRING_DER_WRT: &str = "emath model Spring:\n    inputs:\n        m: Float64\n        c: Float64\n        k: Float64\n    state:\n        x: Float64\n        v: Float64\n    equations:\n        der(x) = v\n        derivative v wrt t = (-c * v - k * x) / m\n";
const MASS_SPRING_SCALAR: &str = "emath model MassSpring:\n    inputs:\n        m: Float64\n        c: Float64\n        k: Float64\n    state:\n        x: Float64\n        v: Float64\n    equations:\n        der(x) = v\n        m * derivative(v) = -c * v - k * x\n";
const MASS_SPRING_IMPLICIT: &str = "emath model MassSpring:\n    inputs:\n        m: Float64\n        c: Float64\n        k: Float64\n    state:\n        x: Float64\n        v: Float64\n    equations:\n        der(x) = v\n        m * der(v) = -c * v - k * x\n";
const RESIDUAL_DECAY: &str = "emath model ResidualDecay:\n    inputs:\n        m: Float64\n    state:\n        v: Float64\n    equations:\n        0 = m * derivative(v) + v\n";
const CAUSAL_CIRCUIT: &str = "emath model CausalCircuit:\n    inputs:\n        V: Float64\n        R: Float64\n        C: Float64\n    algebraic:\n        I: Float64\n    state:\n        q: Float64\n    equations:\n        V - R * I - q / C == 0\n        der(q) = I\n";
const COUPLED_SYS: &str = "emath model CoupledSys:\n    algebraic:\n        a: Float64\n        b: Float64\n    state:\n        q: Float64\n    equations:\n        a + b == 10\n        a - b - 2 == 0\n        der(q) = a + b - 4\n";
const MATRIX_MASS: &str = "emath model MatrixMass:\n    inputs:\n        M: Matrix[2, 2]\n        f: Vector[2]\n    state:\n        x: Vector[2]\n        v: Vector[2]\n    equations:\n        der(x) = v\n        M * der(v) == f\n";
const NO_UNKNOWNS: &str = "emath model NoUnknowns:\n    inputs:\n        V: Float64\n        R: Float64\n    state:\n        q: Float64\n    equations:\n        V * R - q == 0\n";
const UNDERDETERMINED: &str = "emath model Underdetermined:\n    algebraic:\n        a: Float64\n        b: Float64\n    state:\n        q: Float64\n    equations:\n        a + b == 10\n        der(q) = 0\n";
const UNUSED_ALGEBRAIC: &str = "emath model UnusedAlgebraic:\n    algebraic:\n        a: Float64\n        b: Float64\n    state:\n        q: Float64\n    equations:\n        a == 5\n        der(q) = 0\n";
const BARE_ALGEBRAIC: &str = "emath model BareAlgebraic:\n    algebraic:\n        I: Float64\n    state:\n        q: Float64\n    equations:\n        der(q) = I\n";
const RATE_CONFLICT: &str = "emath model RateConflict:\n    inputs:\n        m: Float64\n    state:\n        v: Float64\n    equations:\n        der(v) = -v\n        0 = m * derivative(v) + v\n";
const FN_NOT_A_MODEL: &str = "emath function NotAModel:\n    inputs:\n        x: Float64\n    outputs:\n        y: Float64\n    definitions:\n        y = x\n    equations:\n        derivative(x) = 0\n";
const INCOMPLETE: &str = "emath model Incomplete:\n    state:\n        x: Float64\n        v: Float64\n    equations:\n        der(x) = v\n";
const UNIT_SPRING: &str = "emath model UnitSpring:\n    inputs:\n        v: Float64 in m/s\n    state:\n        x: Float64 in m\n    equations:\n        der(x) = v\n";
const BAD_UNITS: &str = "emath model BadUnits:\n    inputs:\n        v: Float64 in m\n    state:\n        x: Float64 in m\n    equations:\n        der(x) = v\n";
const EMPTY_MODEL: &str = "emath model Empty:\n    inputs:\n        x: Float64\n";
const RC_CIRCUIT: &str = "emath model RCCircuit:\n    inputs:\n        V: Float64\n        R: Float64\n        C: Float64\n    state:\n        q: Float64\n    equations:\n        I = (V - q / C) / R\n        der(q) = I\n";
const IMPLICIT_CIRCUIT: &str = "emath model ImplicitCircuit:\n    inputs:\n        V: Float64\n        R: Float64\n        C: Float64\n        I: Float64\n    state:\n        q: Float64\n    equations:\n        I_solved = solve(V - R * I - q / C) wrt I\n        der(q) = I_solved\n";
const BLOWUP: &str = "emath model Blowup:\n    state:\n        x: Float64\n    equations:\n        derivative(x) = x * x\n";
fn coaching(p: &mut Probe, name: &str, source: &str, codes: &[&str], phrases: &[&str]) {
    let result = Source::from_str(name, source).check();
    let messages: Vec<String> = result.diagnostics.errors().map(|d| d.to_string()).collect();
    let joined = messages.join("\n");
    p.demand(
        name.to_string() + ":refused",
        result.diagnostics.has_errors(),
        format!("must refuse, got {messages:?}"),
    );
    for code in codes {
        p.contains(name.to_string() + ":code", &joined, code);
    }
    for phrase in phrases {
        p.contains(name.to_string() + ":coach", &joined, phrase);
    }
}

fn notes(p: &mut Probe, name: &str, source: &str) -> String {
    let result = Source::from_str(name, source).check();
    result
        .diagnostics
        .items()
        .iter()
        .map(|d| d.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn model_ode() {
    boot();
    let mut p = Probe::new("continuous emath model admission, kind coaching, and simulated trajectories");
    p.case("decay-admits", |p| {
        let result = Source::from_str("decay", decay_model()).must_admit(&mut *p);
        if result.diagnostics.has_errors() || result.package.declarations.is_empty() {
            return;
        }
        let decl = &result.package.declarations[0];
        p.eq("kind", decl.kind_label.clone(), "model".to_string());
        p.eq("state", decl.state.len(), 1);
        match decl.definitions.get("der_x") {
            Some(rate) => {
                p.demand(
                    "rate-binary",
                    matches!(result.package.expr(*rate), Some(ExprNode::Binary { .. })),
                    "rate -k*x must lower to a Binary node",
                );
            }
            None => {
                p.fail("der-x", "derivative(x) must lower to der_x");
            }
        }
    });
    p.case("state-coaches", |p| {
        coaching(&mut *p, "fn-state", FN_STATE, &["E-KIND-010"], &["`emath model`", "`emath policy`"]);
    });
    p.case("ctor-coaches", |p| {
        coaching(&mut *p, "fn-ctor", FN_CTOR, &["E-KIND-010"], &["`emath policy`"]);
    });
    p.case("equations-coach", |p| {
        coaching(&mut *p, "fn-eq-coach", FN_EQUATIONS, &["E-KIND-010"], &["`emath model`"]);
    });
    p.case("algebraic-coaches", |p| {
        coaching(
            &mut *p,
            "fn-algebraic",
            FN_ALGEBRAIC,
            &["E-KIND-010"],
            &["did you mean `emath model`?"],
        );
    });
    p.case("defs-only-note", |p| {
        // A definitions-only model admits, but the kind note suggests `emath function`.
        let result = Source::from_str("model-defs-only", MODEL_DEFS_ONLY).must_admit(&mut *p);
        if result.diagnostics.has_errors() {
            return;
        }
        let joined = notes(&mut *p, "model-defs-only", MODEL_DEFS_ONLY);
        p.contains("note", &joined, "N-KIND-001");
        p.contains("function", &joined, "`emath function`");
    });
    p.case("real-model-no-nag", |p| {
        // A genuine model (state + equations) must not be nagged toward `emath function`.
        let result = Source::from_str("model-real", decay_model()).must_admit(&mut *p);
        if result.diagnostics.has_errors() {
            return;
        }
        let joined = notes(&mut *p, "model-real", decay_model());
        p.demand(
            "no-nag",
            joined.find("N-KIND-001").is_none(),
            format!("genuine model must not be nagged toward function, got {joined:?}"),
        );
    });
    p.case("der-spellings", |p| {
        let result = Source::from_str("spring", SPRING_DER_WRT).must_admit(&mut *p);
        if result.diagnostics.has_errors() || result.package.declarations.is_empty() {
            return;
        }
        let decl = &result.package.declarations[0];
        p.demand("der-x", decl.definitions.contains_key("der_x"), "der(x) must admit".to_string());
        p.demand("der-v", decl.definitions.contains_key("der_v"), "derivative v wrt t must admit".to_string());
    });
    p.case("scalar-mass", |p| {
        let result = Source::from_str("mass-matrix", MASS_SPRING_SCALAR).must_admit(&mut *p);
        if result.diagnostics.has_errors() || result.package.declarations.is_empty() {
            return;
        }
        let decl = &result.package.declarations[0];
        p.demand("der-x", decl.definitions.contains_key("der_x"), "der(x) must admit".to_string());
        match decl.definitions.get("der_v") {
            Some(rate) => {
                p.demand(
                    "rate-binary",
                    matches!(result.package.expr(*rate), Some(ExprNode::Binary { .. })),
                    "scalar mass rewrite must lower to a Binary node",
                );
            }
            None => {
                p.fail("der-v", "m * derivative(v) must rewrite to der_v");
            }
        }
    });
    p.case("mass-matches-explicit", |p| {
        let implicit = Source::from_str("mass-sim", MASS_SPRING_IMPLICIT).must_admit(&mut *p);
        let explicit = Source::from_workspace("tests/fixtures/language/numerical/explicit-mass-spring.emath").must_admit(&mut *p);
        if implicit.diagnostics.has_errors()
            || explicit.diagnostics.has_errors()
            || implicit.package.declarations.is_empty()
            || explicit.package.declarations.is_empty()
        {
            return;
        }
        let mut inputs = BTreeMap::new();
        inputs.insert("m".into(), Value::F64(1.0));
        inputs.insert("c".into(), Value::F64(0.2));
        inputs.insert("k".into(), Value::F64(1.0));
        let mut state = BTreeMap::new();
        state.insert("x".into(), Value::F64(1.0));
        state.insert("v".into(), Value::F64(0.0));
        let mut vector_state = BTreeMap::new();
        vector_state.insert("s".into(), Value::Vector(vec![1.0, 0.0]));
        let left = match simulate_continuous(
            &implicit.package,
            &implicit.package.declarations[0],
            &inputs,
            &state,
            0.0,
            0.5,
            0.05,
            StepMethod::Rk4,
        ) {
            Ok(traj) => traj,
            Err(error) => {
                p.fail("implicit:run", format!("implicit simulation must run: {error}"));
                return;
            }
        };
        let right = match simulate_continuous(
            &explicit.package,
            &explicit.package.declarations[0],
            &inputs,
            &vector_state,
            0.0,
            0.5,
            0.05,
            StepMethod::Rk4,
        ) {
            Ok(traj) => traj,
            Err(error) => {
                p.fail("explicit:run", format!("explicit simulation must run: {error}"));
                return;
            }
        };
        let left_last = match left.samples.last() {
            Some(last) => last,
            None => {
                p.fail("implicit:samples", "trajectory must contain samples");
                return;
            }
        };
        let right_last = match right.samples.last() {
            Some(last) => last,
            None => {
                p.fail("explicit:samples", "trajectory must contain samples");
                return;
            }
        };
        let lx = f64_cell(&mut *p, "lx", &left_last.state, "x");
        let rv = numeric_cells(&mut *p, "rv", right_last.state.get("s"));
        p.eq("rv-len", rv.len(), 2);
        p.close("match", lx, rv.first().copied().unwrap_or(f64::NAN), 1e-12);
    });
    p.case("rk4-tracks-cos", |p| {
        // m=k=1, c=0, s(0)=[1,0]: x=cos(t), v=-sin(t). Classic RK4 at
        // dt=0.01 must stay close; a mislabeled Euler/Heun step is ~1e-2/1e-4 off.
        let trail = undamped_spring(&mut *p);
        let (package, inputs, state) = match trail {
            Some(ok) => ok,
            None => return,
        };
        if package.declarations.is_empty() {
            p.fail("decl", "spring example must admit a model");
            return;
        }
        let t1 = std::f64::consts::PI;
        let traj = match simulate_continuous(
            &package,
            &package.declarations[0],
            &inputs,
            &state,
            0.0,
            t1,
            0.01,
            StepMethod::Rk4,
        ) {
            Ok(traj) => traj,
            Err(error) => {
                p.fail("run", format!("RK4 simulation must run: {error}"));
                return;
            }
        };
        let last = match traj.samples.last() {
            Some(last) => last,
            None => {
                p.fail("samples", "trajectory must contain samples");
                return;
            }
        };
        p.close("lands-on-pi", last.t, t1, 1e-12);
        let (x, v) = spring_xv(&mut *p, "xv", last);
        p.close("x", x, t1.cos(), 1e-6);
        p.close("v", v, -t1.sin(), 1e-6);
        p.close("energy", 0.5 * (x * x + v * v), 0.5, 1e-6);
    });
    p.case("euler-grows-energy", |p| {
        // Forward Euler on x'' = -x multiplies energy by (1+dt^2) each step.
        let trail = undamped_spring(&mut *p);
        let (package, inputs, state) = match trail {
            Some(ok) => ok,
            None => return,
        };
        if package.declarations.is_empty() {
            p.fail("decl", "spring example must admit a model");
            return;
        }
        let dt = 0.1;
        let t1 = 10.0;
        let traj = match simulate_continuous(
            &package,
            &package.declarations[0],
            &inputs,
            &state,
            0.0,
            t1,
            dt,
            StepMethod::Euler,
        ) {
            Ok(traj) => traj,
            Err(error) => {
                p.fail("run", format!("Euler simulation must run: {error}"));
                return;
            }
        };
        let last = match traj.samples.last() {
            Some(last) => last,
            None => {
                p.fail("samples", "trajectory must contain samples");
                return;
            }
        };
        let (x, v) = spring_xv(&mut *p, "xv", last);
        let energy = 0.5 * (x * x + v * v);
        let predicted = 0.5 * (1.0 + dt * dt).powf((t1 / dt).round());
        p.demand("grows", energy > 1.0, format!("forward Euler must grow energy, got {energy}"));
        p.close("law", energy, predicted, 1e-9 * predicted);
    });
    p.case("residual-f5", |p| {
        // Non-greedy derivative operand: `m * derivative(v) + v` parses as
        // `(m * derivative(v)) + v`, a valid implicit ODE residual.
        Source::from_str("residual-decay", RESIDUAL_DECAY).must_admit(&mut *p);
    });
    p.case("causal-sim", |p| {
        // Full causalization: current I is declared in `algebraic:` and
        // found by the coupled Newton solve at each step.
        let result = Source::from_str("causal-circuit", CAUSAL_CIRCUIT).must_admit(&mut *p);
        if result.diagnostics.has_errors() || result.package.declarations.is_empty() {
            return;
        }
        let decl = &result.package.declarations[0];
        let residuals = match result.package.residuals.get(&decl.id) {
            Some(residuals) => residuals,
            None => {
                p.fail("residual", "residual must be recorded");
                return;
            }
        };
        if residuals.is_empty() {
            p.fail("residual", "residual must be recorded");
            return;
        }
        p.eq("algebraic", residuals[0].algebraic.clone(), vec!["I".to_string()]);
        p.eq("rates", residuals[0].rates.clone(), Vec::<String>::new());
        let mut inputs = BTreeMap::new();
        inputs.insert("V".into(), Value::F64(10.0));
        inputs.insert("R".into(), Value::F64(1.0));
        inputs.insert("C".into(), Value::F64(1.0));
        inputs.insert("I".into(), Value::F64(1.0));
        let mut state = BTreeMap::new();
        state.insert("q".into(), Value::F64(0.0));
        let traj = match simulate_continuous(
            &result.package,
            decl,
            &inputs,
            &state,
            0.0,
            1.0,
            0.01,
            StepMethod::Rk4,
        ) {
            Ok(traj) => traj,
            Err(error) => {
                p.fail("run", format!("causal simulation must run: {error}"));
                return;
            }
        };
        let last = match traj.samples.last() {
            Some(last) => last,
            None => {
                p.fail("samples", "trajectory must contain samples");
                return;
            }
        };
        let q_final = f64_cell(&mut *p, "q", &last.state, "q");
        // Analytical: q(t) = C*V*(1 - exp(-t/(R*C))) = 10*(1 - exp(-1)).
        let expected = 10.0 * (1.0 - (-1.0f64).exp());
        p.close("q", q_final, expected, 0.01);
        let residual = algebraic_residual_max(&mut *p, "residual", &result.package, decl, &inputs, &last.state);
        p.demand(
            "residual",
            residual < 1e-6,
            format!("algebraic residual must be ~0, got {residual:.3e} at t={}", last.t),
        );
    });
    p.case("causal-step", |p| {
        // After a successful DAE step the extended state (differential +
        // projected algebraic) must sit on the constraint.
        let result = Source::from_str("causal-step-residual", CAUSAL_CIRCUIT).must_admit(&mut *p);
        if result.diagnostics.has_errors() || result.package.declarations.is_empty() {
            return;
        }
        let decl = &result.package.declarations[0];
        let mut inputs = BTreeMap::new();
        inputs.insert("V".into(), Value::F64(10.0));
        inputs.insert("R".into(), Value::F64(1.0));
        inputs.insert("C".into(), Value::F64(1.0));
        inputs.insert("I".into(), Value::F64(0.0));
        let mut state = BTreeMap::new();
        state.insert("q".into(), Value::F64(0.0));
        for method in [StepMethod::Euler, StepMethod::Rk4] {
            let next = match step_continuous_values(&result.package, decl, &inputs, &state, 0.1, method) {
                Ok(next) => next,
                Err(error) => {
                    p.fail(format!("{method:?}:step"), format!("step must succeed, got {error}"));
                    continue;
                }
            };
            let q = f64_cell(&mut *p, &format!("{method:?}:q"), &next, "q");
            let i = f64_cell(&mut *p, &format!("{method:?}:i"), &next, "I");
            p.demand(format!("{method:?}:advances"), q > 0.0, format!("must advance charge, got q={q}"));
            p.demand(
                format!("{method:?}:projected"),
                (10.0 - i - q).abs() < 1e-6,
                format!("V-R*I-q/C must be ~0 after the step (q={q}, I={i})"),
            );
            let lowered = algebraic_residual_max(&mut *p, &format!("{method:?}:lowered"), &result.package, decl, &inputs, &next);
            p.demand(
                format!("{method:?}:lowered"),
                lowered < 1e-6,
                format!("lowered residual must be ~0, got {lowered:.3e}"),
            );
        }
    });
    p.case("coupled", |p| {
        // Two residuals, two unknowns: a=6, b=4 at every step, so
        // der(q) = 6 and q(0.1) = 0.6.
        let result = Source::from_str("coupled", COUPLED_SYS).must_admit(&mut *p);
        if result.diagnostics.has_errors() || result.package.declarations.is_empty() {
            return;
        }
        let mut inputs = BTreeMap::new();
        inputs.insert("a".into(), Value::F64(0.0));
        inputs.insert("b".into(), Value::F64(0.0));
        let mut state = BTreeMap::new();
        state.insert("q".into(), Value::F64(0.0));
        let traj = match simulate_continuous(
            &result.package,
            &result.package.declarations[0],
            &inputs,
            &state,
            0.0,
            0.1,
            0.1,
            StepMethod::Euler,
        ) {
            Ok(traj) => traj,
            Err(error) => {
                p.fail("run", format!("coupled simulation must run: {error}"));
                return;
            }
        };
        let last = match traj.samples.last() {
            Some(last) => last,
            None => {
                p.fail("samples", "trajectory must contain samples");
                return;
            }
        };
        let q = f64_cell(&mut *p, "q", &last.state, "q");
        p.close("q", q, 0.6, 1e-9);
    });
    p.case("matrix-mass", |p| {
        // M = diag(2): der(v) = f/2 = [0.5, 1.0], v(0.5) = [0.25, 0.5],
        // x(t) = a*t^2/2 -> [0.0625, 0.125].
        let result = Source::from_str("matrix-mass", MATRIX_MASS).must_admit(&mut *p);
        if result.diagnostics.has_errors() || result.package.declarations.is_empty() {
            return;
        }
        let residuals = match result.package.residuals.get(&result.package.declarations[0].id) {
            Some(residuals) => residuals,
            None => {
                p.fail("residual", "residual must be recorded");
                return;
            }
        };
        if residuals.is_empty() {
            p.fail("residual", "residual must be recorded");
            return;
        }
        p.eq("components", residuals[0].components, 2);
        p.eq("rates", residuals[0].rates.clone(), vec!["v".to_string()]);
        let mut inputs = BTreeMap::new();
        inputs.insert(
            "M".into(),
            Value::Matrix { rows: 2, cols: 2, data: vec![2.0, 0.0, 0.0, 2.0] },
        );
        inputs.insert("f".into(), Value::Vector(vec![1.0, 2.0]));
        let mut state = BTreeMap::new();
        state.insert("x".into(), Value::Vector(vec![0.0, 0.0]));
        state.insert("v".into(), Value::Vector(vec![0.0, 0.0]));
        let traj = match simulate_continuous(
            &result.package,
            &result.package.declarations[0],
            &inputs,
            &state,
            0.0,
            0.5,
            0.05,
            StepMethod::Rk4,
        ) {
            Ok(traj) => traj,
            Err(error) => {
                p.fail("run", format!("matrix-mass simulation must run: {error}"));
                return;
            }
        };
        let last = match traj.samples.last() {
            Some(last) => last,
            None => {
                p.fail("samples", "trajectory must contain samples");
                return;
            }
        };
        let vf = numeric_cells(&mut *p, "v", last.state.get("v"));
        let xf = numeric_cells(&mut *p, "x", last.state.get("x"));
        p.eq("v-len", vf.len(), 2);
        p.eq("x-len", xf.len(), 2);
        p.close("v0", vf.first().copied().unwrap_or(f64::NAN), 0.25, 1e-9);
        p.close("v1", vf.get(1).copied().unwrap_or(f64::NAN), 0.5, 1e-9);
        p.close("x0", xf.first().copied().unwrap_or(f64::NAN), 0.0625, 1e-9);
        p.close("x1", xf.get(1).copied().unwrap_or(f64::NAN), 0.125, 1e-9);
    });
    p.case("no-unknowns", |p| {
        Source::from_str("no-unknowns", NO_UNKNOWNS).must_refuse(&mut *p, &["E-TYPE-010"]);
    });
    p.case("underdetermined", |p| {
        Source::from_str("underdetermined", UNDERDETERMINED).must_refuse(&mut *p, &["E-TYPE-010"]);
    });
    p.case("unused-algebraic", |p| {
        Source::from_str("unused-algebraic", UNUSED_ALGEBRAIC).must_refuse(&mut *p, &["E-TYPE-002"]);
    });
    p.case("bare-algebraic", |p| {
        Source::from_str("bare-algebraic", BARE_ALGEBRAIC).must_refuse(&mut *p, &["E-TYPE-010"]);
    });
    p.case("rate-conflict", |p| {
        Source::from_str("rate-conflict", RATE_CONFLICT).must_refuse(&mut *p, &["E-TYPE-010"]);
    });
    p.case("fn-equations", |p| {
        Source::from_str("fn-eq", FN_NOT_A_MODEL).must_refuse(&mut *p, &["E-KIND-010"]);
    });
    p.case("incomplete-rates", |p| {
        Source::from_str("incomplete", INCOMPLETE).must_refuse(&mut *p, &["E-NAME-025"]);
    });
    p.case("explicit-vector-rate", |p| {
        let result = Source::from_workspace("tests/fixtures/language/numerical/explicit-mass-spring.emath").must_admit(&mut *p);
        if result.diagnostics.has_errors() || result.package.declarations.is_empty() {
            return;
        }
        let decl = &result.package.declarations[0];
        p.eq("kind", decl.kind_label.clone(), "model".to_string());
        p.demand("der-s", decl.definitions.contains_key("der_s"), "coupled pair must lower to one vector-state rate".to_string());
    });
    p.case("unit-rates", |p| {
        Source::from_str("unit-rates", UNIT_SPRING).must_admit(&mut *p);
    });
    p.case("unit-mismatch", |p| {
        Source::from_str("unit-mismatch", BAD_UNITS).must_refuse(&mut *p, &["E-UNIT-101"]);
    });
    p.case("empty-model", |p| {
        Source::from_str("empty-model", EMPTY_MODEL).must_refuse(&mut *p, &["E-KIND-011"]);
    });
    p.case("rc-definition", |p| {
        let result = Source::from_str("rc-circuit", RC_CIRCUIT).must_admit(&mut *p);
        if result.diagnostics.has_errors() || result.package.declarations.is_empty() {
            return;
        }
        let decl = &result.package.declarations[0];
        p.demand("I", decl.definitions.contains_key("I"), "algebraic var I must be in definitions".to_string());
        p.demand("der-q", decl.definitions.contains_key("der_q"), "rate der_q must be in definitions".to_string());
    });
    p.case("rc-sim", |p| {
        let result = Source::from_str("rc-sim", RC_CIRCUIT).must_admit(&mut *p);
        if result.diagnostics.has_errors() || result.package.declarations.is_empty() {
            return;
        }
        let mut inputs = BTreeMap::new();
        inputs.insert("V".into(), Value::F64(10.0));
        inputs.insert("R".into(), Value::F64(1.0));
        inputs.insert("C".into(), Value::F64(1.0));
        let mut state = BTreeMap::new();
        state.insert("q".into(), Value::F64(0.0));
        let traj = match simulate_continuous(
            &result.package,
            &result.package.declarations[0],
            &inputs,
            &state,
            0.0,
            1.0,
            0.01,
            StepMethod::Rk4,
        ) {
            Ok(traj) => traj,
            Err(error) => {
                p.fail("run", format!("RC simulation must run: {error}"));
                return;
            }
        };
        let last = match traj.samples.last() {
            Some(last) => last,
            None => {
                p.fail("samples", "trajectory must contain samples");
                return;
            }
        };
        let expected = 10.0 * (1.0 - (-1.0f64).exp());
        let q = f64_cell(&mut *p, "q", &last.state, "q");
        p.close("q", q, expected, 0.01);
    });
    p.case("implicit-solve", |p| {
        // Implicit DAE: I is found via Newton's method at each step from
        // the input initial guess; same analytical solution as rc-sim.
        let result = Source::from_str("implicit-circuit", IMPLICIT_CIRCUIT).must_admit(&mut *p);
        if result.diagnostics.has_errors() || result.package.declarations.is_empty() {
            return;
        }
        let mut inputs = BTreeMap::new();
        inputs.insert("V".into(), Value::F64(10.0));
        inputs.insert("R".into(), Value::F64(1.0));
        inputs.insert("C".into(), Value::F64(1.0));
        inputs.insert("I".into(), Value::F64(1.0));
        let mut state = BTreeMap::new();
        state.insert("q".into(), Value::F64(0.0));
        let traj = match simulate_continuous(
            &result.package,
            &result.package.declarations[0],
            &inputs,
            &state,
            0.0,
            1.0,
            0.01,
            StepMethod::Euler,
        ) {
            Ok(traj) => traj,
            Err(error) => {
                p.fail("run", format!("implicit simulation must run: {error}"));
                return;
            }
        };
        let last = match traj.samples.last() {
            Some(last) => last,
            None => {
                p.fail("samples", "trajectory must contain samples");
                return;
            }
        };
        let expected = 10.0 * (1.0 - (-1.0f64).exp());
        let q = f64_cell(&mut *p, "q", &last.state, "q");
        p.close("q", q, expected, 0.05);
    });
    p.case("heat-rod", |p| {
        // Insulated (Clamp) boundary: total heat is conserved and the hot
        // spot diffuses to both neighbors.
        let result = Source::from_workspace("language/examples/numerical/heat-rod-sim.emath").must_admit(&mut *p);
        if result.diagnostics.has_errors() || result.package.declarations.is_empty() {
            return;
        }
        let mut inputs = BTreeMap::new();
        inputs.insert("alpha".into(), Value::F64(1.0));
        let mut state = BTreeMap::new();
        state.insert("u".into(), Value::Vector(vec![0.0, 1.0, 0.0, 0.0, 0.0]));
        let traj = match simulate_continuous(
            &result.package,
            &result.package.declarations[0],
            &inputs,
            &state,
            0.0,
            0.5,
            0.01,
            StepMethod::Rk4,
        ) {
            Ok(traj) => traj,
            Err(error) => {
                p.fail("run", format!("heat-rod simulation must run: {error}"));
                return;
            }
        };
        let last = match traj.samples.last() {
            Some(last) => last,
            None => {
                p.fail("samples", "trajectory must contain samples");
                return;
            }
        };
        let u = numeric_cells(&mut *p, "u", last.state.get("u"));
        p.eq("cells", u.len(), 5);
        if u.len() == 5 {
            let total: f64 = u.iter().sum();
            p.close("conserved", total, 1.0, 1e-9);
            p.demand("diffuses", u[1] < 1.0, format!("hot spot must diffuse down, got u[1] = {}", u[1]));
            p.demand("left", u[0] > 0.0, format!("heat must reach the left neighbor, got u[0] = {}", u[0]));
            p.demand("right", u[2] > 0.0, format!("heat must reach the right neighbor, got u[2] = {}", u[2]));
        }
    });
    p.case("heat-plate", |p| {
        // 2D insulated boundary: total heat conserved, center diffuses to
        // all four neighbors (up=1, left=3, right=5, down=7 row-major).
        let result = Source::from_workspace("tests/fixtures/language/numerical/heat-plate-sim.emath").must_admit(&mut *p);
        if result.diagnostics.has_errors() || result.package.declarations.is_empty() {
            return;
        }
        let mut inputs = BTreeMap::new();
        inputs.insert("alpha".into(), Value::F64(1.0));
        let mut state = BTreeMap::new();
        state.insert(
            "u".into(),
            Value::Matrix { rows: 3, cols: 3, data: vec![0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0] },
        );
        let traj = match simulate_continuous(
            &result.package,
            &result.package.declarations[0],
            &inputs,
            &state,
            0.0,
            0.5,
            0.01,
            StepMethod::Rk4,
        ) {
            Ok(traj) => traj,
            Err(error) => {
                p.fail("run", format!("heat-plate simulation must run: {error}"));
                return;
            }
        };
        let last = match traj.samples.last() {
            Some(last) => last,
            None => {
                p.fail("samples", "trajectory must contain samples");
                return;
            }
        };
        let u = numeric_cells(&mut *p, "u", last.state.get("u"));
        p.eq("cells", u.len(), 9);
        if u.len() == 9 {
            let total: f64 = u.iter().sum();
            p.close("conserved", total, 1.0, 1e-9);
            p.demand("center", u[4] < 1.0, format!("center must diffuse down, got u[4] = {}", u[4]));
            for (name, index) in [("top", 1), ("left", 3), ("right", 5), ("bottom", 7)] {
                p.demand(name, u[index] > 0.0, format!("heat must reach the {name} neighbor"));
            }
        }
    });
    p.case("heat-volume", |p| {
        let result = Source::from_workspace("tests/fixtures/language/numerical/heat-volume-sim.emath").must_admit(&mut *p);
        if result.diagnostics.has_errors() || result.package.declarations.is_empty() {
            return;
        }
        let mut inputs = BTreeMap::new();
        inputs.insert("alpha".into(), Value::F64(1.0));
        let mut data = vec![0.0; 27];
        data[13] = 1.0;
        let mut state = BTreeMap::new();
        state.insert("u".into(), Value::Tensor { shape: vec![3, 3, 3], data });
        let traj = match simulate_continuous(
            &result.package,
            &result.package.declarations[0],
            &inputs,
            &state,
            0.0,
            0.5,
            0.005,
            StepMethod::Rk4,
        ) {
            Ok(traj) => traj,
            Err(error) => {
                p.fail("run", format!("3D heat simulation must run: {error}"));
                return;
            }
        };
        let last = match traj.samples.last() {
            Some(last) => last,
            None => {
                p.fail("samples", "trajectory must contain samples");
                return;
            }
        };
        let u = numeric_cells(&mut *p, "u", last.state.get("u"));
        p.eq("cells", u.len(), 27);
        if u.len() == 27 {
            let total: f64 = u.iter().sum();
            p.close("conserved", total, 1.0, 1e-9);
            p.demand("center", u[13] < 1.0, "center hot voxel must diffuse".to_string());
            for neighbor in [4, 10, 12, 14, 16, 22] {
                p.demand(
                    format!("neighbor-{neighbor}"),
                    u[neighbor] > 0.0,
                    format!("neighbor {neighbor} stayed cold"),
                );
            }
        }
    });
    p.case("blowup", |p| {
        // Non-finite guard: Euler on x' = x^2 from x0 = 2 overflows f64
        // and must FAIL the run, never return poisoned samples.
        let result = Source::from_str("blowup", BLOWUP).must_admit(&mut *p);
        if result.diagnostics.has_errors() || result.package.declarations.is_empty() {
            return;
        }
        let mut state = BTreeMap::new();
        state.insert("x".into(), Value::F64(2.0));
        match simulate_continuous(
            &result.package,
            &result.package.declarations[0],
            &BTreeMap::new(),
            &state,
            0.0,
            6.0,
            0.5,
            StepMethod::Euler,
        ) {
            Ok(_) => {
                p.fail("must-fail", "diverging Euler must error, not return inf/NaN samples");
            }
            Err(error) => {
                p.contains("non-finite", &error, "non-finite");
            }
        }
    });
    p.finish();
}
