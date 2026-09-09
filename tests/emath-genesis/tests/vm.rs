//! Semantic VM tests.

use emath_genesis::{
    BooleanAlienWorld, Environment, EvalError, ModularAlienWorld, VmBudget, VmOutcome, evaluate,
    reference_alien_term, resume, run,
};
use emath_term::{SymbolId, Term, VariableId};
use emath_test_harness::Probe;

fn boolean_env() -> Environment<bool> {
    [(VariableId("a".into()), true), (VariableId("b".into()), false)].into()
}

fn modular_env() -> Environment<i64> {
    [(VariableId("a".into()), 4), (VariableId("b".into()), 7)].into()
}

#[test]
fn semantic_vm() {
    let mut p = Probe::new("the VM agrees with the evaluator, suspends, and types errors");
    p.case("agrees-with-evaluator", |p| {
        let (_, term) = reference_alien_term();
        let recursive = evaluate(&term, &ModularAlienWorld, &modular_env()).expect("evaluates");
        match run(&term, &ModularAlienWorld, &modular_env(), &VmBudget::seed_default()).expect("vm evaluates") {
            VmOutcome::Complete { value, steps, .. } => {
                p.eq("value", value, recursive);
                p.demand("steps", steps > 0, "steps advance");
            }
            VmOutcome::Suspended(_) => { p.fail("complete", "seed budget must complete the reference term"); }
        }
    });
    p.case("trace-deterministic", |p| {
        let (_, term) = reference_alien_term();
        let first = run(&term, &BooleanAlienWorld, &boolean_env(), &VmBudget::seed_default()).expect("vm evaluates");
        let second = run(&term, &BooleanAlienWorld, &boolean_env(), &VmBudget::seed_default()).expect("vm evaluates");
        let (VmOutcome::Complete { trace: a, .. }, VmOutcome::Complete { trace: b, .. }) = (first, second) else {
            p.fail("complete", "both runs must complete");
            return;
        };
        p.eq("canonical", a.canonical(), b.canonical());
        p.eq("identity", a.identity(), b.identity());
        p.demand("header", a.canonical().starts_with("emath.vm.v1\n"), "trace header");
    });
    p.case("suspend-resume-lossless", |p| {
        let (_, term) = reference_alien_term();
        let unmetered = match run(&term, &ModularAlienWorld, &modular_env(), &VmBudget::seed_default()).expect("vm evaluates") {
            VmOutcome::Complete { value, steps, trace } => (value, steps, trace),
            VmOutcome::Suspended(_) => {
                p.fail("complete", "seed budget must complete");
                return;
            }
        };
        let tiny = VmBudget { max_steps: 1 };
        let mut outcome = run(&term, &ModularAlienWorld, &modular_env(), &tiny).expect("metered vm never errors");
        let mut resumes = 0_u32;
        let completed = loop {
            match outcome {
                VmOutcome::Complete { value, steps, trace } => break (value, steps, trace),
                VmOutcome::Suspended(continuation) => {
                    resumes += 1;
                    p.demand("terminates", resumes < 1000, "resume loop must terminate");
                    if resumes >= 1000 { return; }
                    outcome = resume(continuation, &ModularAlienWorld, &modular_env(), &tiny).expect("resume never errors");
                }
            }
        };
        p.demand("suspended", resumes > 0, "a 1-step budget must suspend at least once");
        p.eq("value", completed.0, unmetered.0);
        p.eq("steps", completed.1, unmetered.1);
        p.eq("trace", completed.2.canonical(), unmetered.2.canonical());
    });
    p.case("missing-variable", |p| {
        let (_, term) = reference_alien_term();
        let error = run(&term, &ModularAlienWorld, &Environment::new(), &VmBudget::seed_default()).expect_err("free variables must refuse");
        p.eq("error", error, EvalError::MissingVariable(VariableId("a".into())));
    });
    p.case("unknown-symbol", |p| {
        let term = Term::Constant(SymbolId("☠".into()));
        let error = run(&term, &BooleanAlienWorld, &Environment::new(), &VmBudget::seed_default()).expect_err("unknown constants must refuse");
        p.eq("error", error, EvalError::UnknownSymbol(SymbolId("☠".into())));
    });
    p.finish();
}
