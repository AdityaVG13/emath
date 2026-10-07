use super::{SyntaxTree, ModuleReport, ConstructorError, Path, DEFAULT_WORK, BTreeSet, Item, BTreeMap, CValue, Expr, StmtKind, Arc, TestObservation, ExprKind};
use super::prelude::*;

/// Evaluate a parsed constructor-layer module.
pub fn evaluate_tree(tree: &SyntaxTree) -> Result<ModuleReport, ConstructorError> {
    evaluate_tree_at(tree, None)
}

/// Evaluate a module at the default work budget, resolving `use` paths
/// from `source` or the repo roots.
pub fn evaluate_tree_at(
    tree: &SyntaxTree,
    source: Option<&Path>,
) -> Result<ModuleReport, ConstructorError> {
    evaluate_tree_with(tree, source, DEFAULT_WORK)
}

/// Evaluate a module under an explicit work budget: every test example
/// runs with `work_limit` available, so large-instance rows are pinnable
/// in the test lane instead of refusing `budget_exhausted` at the
/// default budget.
pub fn evaluate_tree_budgeted_at(
    tree: &SyntaxTree,
    source: Option<&Path>,
    work_limit: u64,
) -> Result<ModuleReport, ConstructorError> {
    evaluate_tree_with(tree, source, work_limit)
}

fn evaluate_tree_with(
    tree: &SyntaxTree,
    source: Option<&Path>,
    work_limit: u64,
) -> Result<ModuleReport, ConstructorError> {
    super::prepare_module(tree, source)?.evaluate_tests(work_limit)
}

impl super::PreparedModule {
    /// Run authored examples from this snapshot, without resolving imports again.
    pub fn evaluate_tests(&self, work_limit: u64) -> Result<ModuleReport, ConstructorError> {
        if !self.admitted {
            super::admit::admit_loaded(&self.fresh_engine(), &self.tree).map_err(|mut err| {
                err.code = super::constructor_admit_code(&err.code).to_string();
                err
            })?;
        }
        evaluate_tests_on(self.fresh_engine(), &self.tree, work_limit)
    }
}

fn evaluate_tests_on(
    mut engine: super::Engine,
    tree: &SyntaxTree,
    work_limit: u64,
) -> Result<ModuleReport, ConstructorError> {
    engine.work_limit = work_limit;
    // The test lane never resumes a suspended evaluation, so it skips
    // continuation snapshots and the all-calls memo (both exist only to
    // make a suspended run resumable).
    engine.facts_only_memo = true;
    let mut uses = Vec::new();
    for item in &tree.items {
        if let Item::Use { path, .. } = item {
            uses.push(path.join("."));
        }
    }
    let mut tests = Vec::new();
    for item in &tree.items {
        let Item::Declaration(decl) = item else {
            continue;
        };
        for section in decl.sections().filter(|section| section.name == "tests") {
            // every `example <label>:` block is its own case. Loose
            // givens/expects outside any example form one anonymous case.
            // Never merge examples: later givens must not leak into earlier
            // expects (constitution: separately identified cases).
            let rat_inputs: BTreeSet<String> = section_typed_fields(decl, "inputs")
                .into_iter()
                .filter(|(_, ty)| ctype_from_type(ty) == CType::Rat)
                .map(|(name, _)| name)
                .collect();
            let mut cases: Vec<(String, Vec<(String, Expr)>, Vec<Expr>)> = Vec::new();
            let mut loose_givens = Vec::new();
            let mut loose_expects: Vec<Expr> = Vec::new();
            let mut has_loose = false;
            for stmt in &section.suite.statements {
                match &stmt.kind {
                    StmtKind::Section(example) if example.name == "example" => {
                        let mut label = decl.name.clone();
                        if let Some(generic) = &example.generic {
                            label = generic.clone();
                        }
                        let mut givens = Vec::new();
                        let mut expects = Vec::new();
                        for inner in &example.suite.statements {
                            match &inner.kind {
                                StmtKind::Given { name, value } => {
                                    givens.push((name.clone(), bind_exact_given(name, value, &rat_inputs)));
                                }
                                StmtKind::Expect(expr) => expects.push(expr.clone()),
                                StmtKind::Assign { target, value }
                                    if target.segments.first().map(String::as_str) == Some("given")
                                    => {}
                                _ => {}
                            }
                        }
                        cases.push((label, givens, expects));
                    }
                    StmtKind::Given { name, value } => {
                        has_loose = true;
                        loose_givens.push((name.clone(), bind_exact_given(name, value, &rat_inputs)));
                    }
                    StmtKind::Expect(expr) => {
                        has_loose = true;
                        loose_expects.push(expr.clone());
                    }
                    _ => {}
                }
            }
            if has_loose {
                cases.push((decl.name.clone(), loose_givens, loose_expects));
            }
            for (label, given_exprs, expects) in cases {
                // Each example gets the whole work limit, and a fault in its
                // givens or query run fails that example alone. A budget
                // fault keeps its frames and depth for resume; the test lane
                // never resumes, so a later example must not inherit them.
                engine.work = 0;
                engine.call_depth = 0;
                engine.frames.clear();
                let mut givens = BTreeMap::new();
                let mut given_fault = None;
                for (name, expr) in &given_exprs {
                    match engine.eval(expr) {
                        Ok(value) => {
                            givens.insert(name.clone(), value);
                        }
                        Err(err) => {
                            given_fault = Some((format!("given `{name}`: {}: {}", err.code, err.message), err.code));
                            break;
                        }
                    }
                }
                if let Some((detail, code)) = given_fault {
                    tests.push(TestObservation { label, passed: false, detail, receipt: None, fault: Some(code) });
                    continue;
                }
                if decl.as_kind == "query" {
                // The query's `receipt`/`diagnostic` bindings and evaluated
                // givens/defs are THIS case's world: restore the env after
                // the example so nothing leaks into later declarations'
                // examples (a leaked `diagnostic` would mask their fault
                // records; same discipline as the function lane below).
                let saved = engine.env.clone();
                let receipt = match engine.run_query(&decl.name, &givens) {
                    Ok(receipt) => receipt,
                    Err(err) => {
                        engine.env = saved;
                        tests.push(TestObservation {
                            label,
                            passed: false,
                            detail: format!("{}: {}", err.code, err.message),
                            receipt: None,
                            fault: Some(err.code),
                        });
                        continue;
                    }
                };
                for (name, value) in &givens {
                    engine.env.insert(name.clone(), value.clone());
                }
                if let Some(qdecl) = engine.queries.get(&decl.name).cloned() {
                    for (name, expr) in &qdecl.defs {
                        if let Ok(value) = engine.eval(expr) {
                            engine.env.insert(name.clone(), value);
                        }
                    }
                }
                engine.env.insert(
                    "receipt".into(),
                    CValue::Receipt(Box::new(receipt.clone())),
                );
                engine.env.insert(
                    "diagnostic".into(),
                    CValue::Record {
                        type_name: "Diagnostic".into(),
                        fields: Arc::new(BTreeMap::from([(
                            "code".into(),
                            CValue::Record {
                                type_name: receipt
                                    .diagnostic_code
                                    .as_deref()
                                    .unwrap_or_default()
                                    .into(),
                                fields: Arc::new(BTreeMap::new()),
                            },
                        )])),
                    },
                );
                let mut passed = true;
                let mut detail = receipt.fulfillment.clone();
                let mut fault = None;
                engine.expect_atoms.set(true);
                for expect in expects {
                    match engine.eval(&expect) {
                        Ok(CValue::Bool(true)) => {}
                        Ok(other) => {
                            passed = false;
                            detail = format!("expect produced {other}");
                        }
                        Err(err) => {
                            passed = false;
                            detail = err.message;
                            fault = Some(err.code);
                        }
                    }
                }
                engine.expect_atoms.set(false);
                tests.push(TestObservation {
                    label,
                    passed,
                    detail,
                    receipt: Some(receipt),
                    fault,
                });
                engine.env = saved;
            } else if let Some(fndecl) = engine.functions.get(&decl.name).cloned() {
                let saved = engine.env.clone();
                for (name, value) in &givens {
                    engine.env.insert(name.clone(), value.clone());
                }
                let mut ok = true;
                let mut detail = String::new();
                let mut fault_code: Option<String> = None;
                let mut fault_message = String::new();
                match engine.eval_fn(
                    &decl.name,
                    fndecl.clone(),
                    &fndecl
                        .inputs
                        .iter()
                        .map(|name| Expr {
                            kind: ExprKind::Path {
                                segments: vec![name.clone()],
                                generics: None,
                            },
                            source: decl.source,
                        })
                        .collect::<Vec<_>>(),
                ) {
                    Ok(value) => {
                        // Outputs come off the returned value (a multi-output
                        // result is the record of its outputs). Only the
                        // intermediate definitions an expect reads, and the
                        // ones those read, are evaluated again.
                        // `result` names the whole value unless an output
                        // is itself called `result`.
                        engine.env.insert("result".into(), value.clone());
                        if fndecl.outputs.len() > 1 {
                            if let CValue::Record { fields, .. } = &value {
                                for (name, field) in fields.iter() {
                                    engine.env.insert(name.clone(), field.clone());
                                }
                            }
                        } else if let Some(output) = &fndecl.output {
                            engine.env.insert(output.clone(), value);
                        }
                        let mut read = BTreeSet::new();
                        for expect in &expects {
                            super::identity::free_path_names(expect, &mut Vec::new(), &mut read);
                        }
                        let mut rebind = vec![false; fndecl.defs.len()];
                        for (index, (name, expr)) in fndecl.defs.iter().enumerate().rev() {
                            let mentioned = read
                                .iter()
                                .any(|path| path.split('.').next() == Some(name.as_str()));
                            if mentioned && !fndecl.outputs.contains(name) {
                                rebind[index] = true;
                                super::identity::free_path_names(expr, &mut Vec::new(), &mut read);
                            }
                        }
                        for ((name, expr), wanted) in fndecl.defs.iter().zip(rebind) {
                            if wanted {
                                if let Ok(def) = engine.eval(expr) {
                                    engine.env.insert(name.clone(), def);
                                }
                            }
                        }
                    }
                    Err(err) => {
                        fault_code = Some(err.code.clone());
                        fault_message = err.message;
                    }
                }
                // `diagnostic` mirrors the query binding: on a fault the
                // code atom carries the fault name; on success it is empty,
                // so a stray fault-demand evaluates false instead of
                // erroring. A faulting example passes only when its expect
                // rows demand that exact fault; with no expect rows a
                // fault is a failed example, as before. A given that binds
                // the name itself keeps its own binding.
                if !engine.env.contains_key("diagnostic") {
                    engine.env.insert(
                        "diagnostic".into(),
                        CValue::Record {
                            type_name: "Diagnostic".into(),
                            fields: Arc::new(BTreeMap::from([(
                                "code".into(),
                                CValue::Record {
                                    type_name: fault_code.as_deref().unwrap_or_default().into(),
                                    fields: Arc::new(BTreeMap::new()),
                                },
                            )])),
                        },
                    );
                }
                if fault_code.is_some() && expects.is_empty() {
                    ok = false;
                }
                let mut fault = fault_code.clone();
                engine.expect_atoms.set(true);
                for expect in expects {
                    match engine.eval(&expect) {
                        Ok(CValue::Bool(true)) => {}
                        Ok(other) => {
                            ok = false;
                            detail = format!("expect produced {other}");
                        }
                        Err(err) => {
                            ok = false;
                            detail = err.message;
                            fault.get_or_insert(err.code);
                        }
                    }
                }
                engine.expect_atoms.set(false);
                if let Some(code) = &fault_code {
                    // An unbound output reads as an atom under `expect`, so
                    // an undemanded fault would otherwise surface only as
                    // "expect produced false".
                    detail = if ok {
                        format!("demanded fault `{code}`")
                    } else {
                        format!("{code}: {fault_message}")
                    };
                }
                engine.env = saved;
                tests.push(TestObservation {
                    label,
                    passed: ok,
                    detail,
                    receipt: None,
                    fault,
                });
                }
            }
        }
    }

    let mut bindings = BTreeMap::new();
    for name in engine.functions.keys() {
        bindings.insert(name.clone(), CValue::Unit);
    }
    Ok(ModuleReport {
        bindings,
        tests,
        uses,
    })
}

/// Evaluate a named function from a parsed module.
pub fn evaluate_function(
    tree: &SyntaxTree,
    name: &str,
    inputs: &BTreeMap<String, CValue>,
) -> Result<CValue, ConstructorError> {
    evaluate_function_at(tree, name, inputs, None)
}

/// Evaluate a named function, resolving `use` from `source`. No
/// checkpoint comes back, so the run never suspends (see
/// `evaluate_function_budgeted` for a resumable one).
pub fn evaluate_function_at(
    tree: &SyntaxTree,
    name: &str,
    inputs: &BTreeMap<String, CValue>,
    source: Option<&Path>,
) -> Result<CValue, ConstructorError> {
    evaluate_function_with_work_at(tree, name, inputs, source, DEFAULT_WORK)
}

/// Non-suspending evaluation under an explicit host work limit. Authored
/// cost/world measurements keep their own budgets inside this outer limit.
pub fn evaluate_function_with_work_at(
    tree: &SyntaxTree,
    name: &str,
    inputs: &BTreeMap<String, CValue>,
    source: Option<&Path>,
    work_limit: u64,
) -> Result<CValue, ConstructorError> {
    super::prepare_module(tree, source)?.evaluate_function(name, inputs, work_limit)
}

