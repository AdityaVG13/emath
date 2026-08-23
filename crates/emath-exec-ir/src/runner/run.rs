use super::eval::{
    eval_constructor, eval_definitions, eval_expect, eval_givens, outputs_of,
    seed_state_from_given,
};
use super::{
    DeclarationRun, PANE_TEST_NAME, RunReport, RunSummary, TestRun, TestVerdict, ZERO_TEST_NOTE,
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
