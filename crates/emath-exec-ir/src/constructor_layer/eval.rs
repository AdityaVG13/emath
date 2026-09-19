use super::*;
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
    let mut engine = empty_engine();
    engine.work_limit = work_limit;
    let mut uses = Vec::new();
    install_local_items(&mut engine, tree)?;
    load_imports(
        &mut engine,
        tree,
        &module_roots_for(source),
        source,
        &mut BTreeSet::new(),
    )?;
    for item in &tree.items {
        if let Item::Use { path, .. } = item {
            uses.push(path.join("."));
        }
    }
    for item in &tree.items {
        match item {
            Item::Use { .. } => {}
            Item::Declaration(decl) => match decl.as_kind.as_str() {
                "function" | "query" | "object" => {}
                other => {
                    return Err(fault(
                        "E-KIND-GONE",
                        format!("declaration kind `{other}` is not a core kind"),
                    ));
                }
            },
            _ => {}
        }
    }

    let mut tests = Vec::new();
    for item in &tree.items {
        let Item::Declaration(decl) = item else {
            continue;
        };
        // The test lane does not run the admission pass, so the same
        // row-form refusal the check lane sees applies here directly.
        super::admit::refuse_unknown_test_rows(decl)?;
        for section in decl.sections().filter(|section| section.name == "tests") {
            // §3.1: every `example <label>:` block is its own case. Loose
            // givens/expects outside any example form one anonymous case.
            // Never merge examples: later givens must not leak into earlier
            // expects (constitution §6.3: separately identified cases).
            let rat_inputs: BTreeSet<String> = section_typed_fields(decl, "inputs")
                .into_iter()
                .filter(|(_, ty)| ctype_from_type(ty) == CType::Rat)
                .map(|(name, _)| name)
                .collect();
            let mut cases: Vec<(String, BTreeMap<String, CValue>, Vec<Expr>)> = Vec::new();
            let mut loose_givens = BTreeMap::new();
            let mut loose_expects: Vec<Expr> = Vec::new();
            let mut has_loose = false;
            for stmt in &section.suite.statements {
                match &stmt.kind {
                    StmtKind::Section(example) if example.name == "example" => {
                        let mut label = decl.name.clone();
                        if let Some(generic) = &example.generic {
                            label = generic.clone();
                        }
                        let mut givens = BTreeMap::new();
                        let mut expects = Vec::new();
                        for inner in &example.suite.statements {
                            match &inner.kind {
                                StmtKind::Given { name, value } => {
                                    let value = bind_exact_given(name, value, &rat_inputs);
                                    givens.insert(name.clone(), engine.eval(&value)?);
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
                        let value = bind_exact_given(name, value, &rat_inputs);
                        loose_givens.insert(name.clone(), engine.eval(&value)?);
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
            for (label, givens, expects) in cases {
                if decl.as_kind == "query" {
                // The query's `receipt`/`diagnostic` bindings and evaluated
                // givens/defs are THIS case's world: restore the env after
                // the example so nothing leaks into later declarations'
                // examples (a leaked `diagnostic` would mask their fault
                // records — same discipline as the function lane below).
                let saved = engine.env.clone();
                let receipt = engine.run_query(&decl.name, &givens)?;
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
                                    .clone()
                                    .unwrap_or_default(),
                                fields: Arc::new(BTreeMap::new()),
                            },
                        )])),
                    },
                );
                let mut passed = true;
                let mut detail = receipt.fulfillment.clone();
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
                        }
                    }
                }
                engine.expect_atoms.set(false);
                tests.push(TestObservation {
                    label,
                    passed,
                    detail,
                    receipt: Some(receipt),
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
                        if let Some(output) = &fndecl.output {
                            engine.env.insert(output.clone(), value.clone());
                        }
                        engine.env.insert("result".into(), value);
                        for (name, expr) in &fndecl.defs {
                            if let Ok(def) = engine.eval(expr) {
                                engine.env.insert(name.clone(), def);
                            }
                        }
                    }
                    Err(err) => {
                        fault_code = Some(err.code.clone());
                        detail = err.message;
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
                                    type_name: fault_code.clone().unwrap_or_default(),
                                    fields: Arc::new(BTreeMap::new()),
                                },
                            )])),
                        },
                    );
                }
                if fault_code.is_some() && expects.is_empty() {
                    ok = false;
                }
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
                        }
                    }
                }
                engine.expect_atoms.set(false);
                if ok && fault_code.is_some() {
                    detail = format!(
                        "demanded fault `{}`",
                        fault_code.as_deref().unwrap_or_default()
                    );
                }
                engine.env = saved;
                tests.push(TestObservation {
                    label,
                    passed: ok,
                    detail,
                    receipt: None,
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

/// Evaluate a named function, resolving `use` from `source`.
pub fn evaluate_function_at(
    tree: &SyntaxTree,
    name: &str,
    inputs: &BTreeMap<String, CValue>,
    source: Option<&Path>,
) -> Result<CValue, ConstructorError> {
    let mut engine = engine_from_tree_at(tree, source)?;
    let decl = engine
        .functions
        .get(name)
        .cloned()
        .ok_or_else(|| fault("unbound", format!("unknown function `{name}`")))?;
    let args: Vec<Expr> = decl
        .inputs
        .iter()
        .map(|input| {
            let value = inputs.get(input).cloned().unwrap_or(CValue::Absent);
            engine.env.insert(input.clone(), value);
            Expr {
                kind: ExprKind::Path {
                    segments: vec![input.clone()],
                    generics: None,
                },
                source: emath_core::Span::default(),
            }
        })
        .collect();
    engine.eval_fn(name, decl, &args)
}

