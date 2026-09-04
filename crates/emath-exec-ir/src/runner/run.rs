use super::eval::{
    check_obligation, coerce_bindings, eval_constructor, eval_definitions,
    eval_definitions_symbolic, eval_expect, eval_givens, outputs_of, seed_state_from_given,
};
use super::{
    DeclarationRun, HOLE_OPEN, PANE_TEST_NAME, RunReport, RunSummary, SYMBOLIC_ONLY, TestRun,
    TestVerdict, ZERO_TEST_NOTE,
};
use crate::interp::Value;
use emath_ir::{Declaration, SemanticPackage};
use std::collections::BTreeMap;

pub fn run_package(package: &SemanticPackage) -> RunReport {
    run_package_with_given(package, None)
}

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
            } else if test.verdict.is_symbolic() {
                summary.symbolic = summary.symbolic.saturating_add(1);
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

pub fn run_declaration(package: &SemanticPackage, declaration: &Declaration) -> DeclarationRun {
    run_declaration_with_given(package, declaration, None)
}

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
        // nothing-returns-nothing: a zero-test declaration always
        // produces its `_pane` run; unbound inputs yield the labeled
        // symbolic form instead of skipping the run entirely.
        let empty = BTreeMap::new();
        tests.push(run_direct(package, declaration, &empty));
    }
    let note = if tests.is_empty() {
        Some(ZERO_TEST_NOTE.to_string())
    } else {
        None
    };
    DeclarationRun {
        name,
        tests,
        law_metadata: package.law_metadata.get(&declaration.id).cloned(),
        note,
    }
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
        Ok(mut given) => {
            coerce_bindings(package, declaration, &mut given);
            run.given = given;
        }
        Err(verdict) => {
            run.verdict = verdict;
            return run;
        }
    }
    if let Err(verdict) = check_law_assumptions(package, declaration, &run.given) {
        run.verdict = verdict;
        return run;
    }

    if let Some(constructor) = declaration.constructors.first() {
        match eval_constructor(package, declaration, constructor, &run.given) {
            Ok(mut state) => {
                coerce_bindings(package, declaration, &mut state);
                run.state = state;
            }
            Err(verdict) => {
                run.verdict = verdict;
                return run;
            }
        }
    } else if !declaration.state.is_empty() {
        match seed_state_from_given(declaration, &run.given) {
            Ok(mut state) => {
                coerce_bindings(package, declaration, &mut state);
                run.state = state;
            }
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
    let mut given = given.clone();
    coerce_bindings(package, declaration, &mut given);
    let mut run = TestRun {
        name: PANE_TEST_NAME.to_string(),
        given,
        state: BTreeMap::new(),
        definitions: BTreeMap::new(),
        outputs: BTreeMap::new(),
        verdict: TestVerdict::Computed,
    };

    if !missing_bindings(declaration, &run.given).is_empty() {
        // nothing-returns-nothing: no world in scope can bind every
        // input, so the run returns the labeled symbolic form instead of
        // a refusal. Typed refusals survive for genuinely impossible
        // lowering (handled inside the symbolic lane).
        return run_symbolic(package, declaration, run);
    }
    if let Err(verdict) = check_law_assumptions(package, declaration, &run.given) {
        run.verdict = verdict;
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
            Ok(mut state) => {
                coerce_bindings(package, declaration, &mut state);
                run.state = state;
            }
            Err(verdict) => {
                run.verdict = verdict;
                return run;
            }
        }
    } else if !declaration.state.is_empty() {
        let state_unbound = declaration
            .state
            .iter()
            .any(|field| !run.given.contains_key(&field.name));
        if state_unbound {
            // State no world can seed: the labeled symbolic form instead
            // of a refusal.
            return run_symbolic(package, declaration, run);
        }
        match seed_state_from_given(declaration, &run.given) {
            Ok(mut state) => {
                coerce_bindings(package, declaration, &mut state);
                run.state = state;
            }
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

/// nothing-returns-nothing lane: evaluate every definition that has an
/// evaluable world, return the symbolic form for the rest, and attach
/// the meaning label to the verdict. Never a naked refusal for a world
/// gap; only genuinely impossible lowering still refuses typed.
fn run_symbolic(package: &SemanticPackage, declaration: &Declaration, mut run: TestRun) -> TestRun {
    match eval_definitions_symbolic(package, declaration, &run.given) {
        Ok(symbolic) => {
            run.outputs = outputs_of(package, declaration, &symbolic.definitions);
            run.definitions = symbolic.definitions;
            let label = if symbolic.forms.is_empty() {
                HOLE_OPEN
            } else {
                SYMBOLIC_ONLY
            };
            run.verdict = TestVerdict::Symbolic {
                label,
                forms: symbolic.forms,
                holes: symbolic.holes,
            };
        }
        Err(verdict) => {
            run.verdict = verdict;
        }
    }
    run
}

fn check_law_assumptions(
    package: &SemanticPackage,
    declaration: &Declaration,
    given: &BTreeMap<String, Value>,
) -> Result<(), TestVerdict> {
    if declaration.kind_label != "law" {
        return Ok(());
    }
    let mut names = Vec::with_capacity(declaration.inputs.len());
    let mut values = Vec::with_capacity(declaration.inputs.len());
    for field in &declaration.inputs {
        let Some(value) = given.get(&field.name).cloned() else {
            return Err(TestVerdict::LoweringRefused {
                detail: format!("test body does not supply input `{}`", field.name),
            });
        };
        names.push(field.name.clone());
        values.push(value);
    }
    for assumption in &declaration.invariants {
        check_obligation(package, *assumption, &names, &values, "assume")?;
    }
    Ok(())
}

/// Every input or constructor parameter the given map does not bind, in
/// declaration order (inputs first, then constructor parameters).
fn missing_bindings(declaration: &Declaration, given: &BTreeMap<String, Value>) -> Vec<String> {
    let mut missing: Vec<String> = declaration
        .inputs
        .iter()
        .filter(|field| !given.contains_key(&field.name))
        .map(|field| field.name.clone())
        .collect();
    if let Some(constructor) = declaration.constructors.first() {
        for parameter in &constructor.parameters {
            if !given.contains_key(&parameter.name) {
                missing.push(parameter.name.clone());
            }
        }
    }
    missing
}
