//! Semantic VM tests (origin `crates/emath-genesis/src/vm.rs`).

use emath_genesis::{
    BooleanAlienWorld, Environment, EvalError, ModularAlienWorld, VmBudget, VmOutcome, evaluate,
    reference_alien_term, resume, run,
};
use emath_term::{SymbolId, Term, VariableId};

fn boolean_env() -> Environment<bool> {
    [
        (VariableId("a".into()), true),
        (VariableId("b".into()), false),
    ]
    .into()
}

fn modular_env() -> Environment<i64> {
    [(VariableId("a".into()), 4), (VariableId("b".into()), 7)].into()
}

#[test]
fn vm_agrees_with_the_recursive_evaluator() {
    let (_, term) = reference_alien_term();
    let recursive = evaluate(&term, &ModularAlienWorld, &modular_env()).expect("evaluates");
    match run(
        &term,
        &ModularAlienWorld,
        &modular_env(),
        &VmBudget::seed_default(),
    )
    .expect("vm evaluates")
    {
        VmOutcome::Complete { value, steps, .. } => {
            assert_eq!(value, recursive);
            assert!(steps > 0);
        }
        VmOutcome::Suspended(_) => panic!("seed budget must complete the reference term"),
    }
}

#[test]
fn trace_is_deterministic_across_runs() {
    let (_, term) = reference_alien_term();
    let first = run(
        &term,
        &BooleanAlienWorld,
        &boolean_env(),
        &VmBudget::seed_default(),
    )
    .expect("vm evaluates");
    let second = run(
        &term,
        &BooleanAlienWorld,
        &boolean_env(),
        &VmBudget::seed_default(),
    )
    .expect("vm evaluates");
    let (VmOutcome::Complete { trace: a, .. }, VmOutcome::Complete { trace: b, .. }) =
        (first, second)
    else {
        panic!("both runs must complete");
    };
    assert_eq!(a.canonical(), b.canonical());
    assert_eq!(a.identity(), b.identity());
    assert!(a.canonical().starts_with("emath.vm.v1\n"));
}

#[test]
fn exhausted_budget_suspends_and_resume_completes_losslessly() {
    let (_, term) = reference_alien_term();
    let unmetered = match run(
        &term,
        &ModularAlienWorld,
        &modular_env(),
        &VmBudget::seed_default(),
    )
    .expect("vm evaluates")
    {
        VmOutcome::Complete {
            value,
            steps,
            trace,
        } => (value, steps, trace),
        VmOutcome::Suspended(_) => panic!("seed budget must complete"),
    };
    // One step per run: forces repeated suspension through every frame.
    let tiny = VmBudget { max_steps: 1 };
    let mut outcome = run(&term, &ModularAlienWorld, &modular_env(), &tiny)
        .expect("metered vm never errors on this term");
    let mut resumes = 0_u32;
    let completed = loop {
        match outcome {
            VmOutcome::Complete {
                value,
                steps,
                trace,
            } => break (value, steps, trace),
            VmOutcome::Suspended(continuation) => {
                resumes += 1;
                assert!(resumes < 1000, "resume loop must terminate");
                outcome = resume(continuation, &ModularAlienWorld, &modular_env(), &tiny)
                    .expect("resume never errors on this term");
            }
        }
    };
    assert!(resumes > 0, "a 1-step budget must suspend at least once");
    assert_eq!(completed.0, unmetered.0, "resumed value matches");
    assert_eq!(completed.1, unmetered.1, "total steps match");
    assert_eq!(
        completed.2.canonical(),
        unmetered.2.canonical(),
        "resumed trace matches the unmetered trace"
    );
}

#[test]
fn missing_variable_is_a_typed_error() {
    let (_, term) = reference_alien_term();
    let empty: Environment<i64> = Environment::new();
    let error = run(&term, &ModularAlienWorld, &empty, &VmBudget::seed_default())
        .expect_err("free variables without valuations must refuse");
    assert_eq!(
        error,
        EvalError::MissingVariable(VariableId("a".into())),
        "the leftmost unresolved variable is reported"
    );
}

#[test]
fn unknown_symbol_is_a_typed_error() {
    let term = Term::Constant(SymbolId("☠".into()));
    let error = run(
        &term,
        &BooleanAlienWorld,
        &Environment::new(),
        &VmBudget::seed_default(),
    )
    .expect_err("unknown constants must refuse");
    assert_eq!(error, EvalError::UnknownSymbol(SymbolId("☠".into())));
}
