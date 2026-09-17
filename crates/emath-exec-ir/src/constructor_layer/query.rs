use super::*;
use super::prelude::*;

/// Evaluate a named query.
pub fn evaluate_query(
    tree: &SyntaxTree,
    name: &str,
    inputs: &BTreeMap<String, CValue>,
) -> Result<Receipt, ConstructorError> {
    evaluate_query_at(tree, name, inputs, None)
}

pub fn evaluate_query_at(
    tree: &SyntaxTree,
    name: &str,
    inputs: &BTreeMap<String, CValue>,
    source: Option<&Path>,
) -> Result<Receipt, ConstructorError> {
    let mut engine = engine_from_tree_at(tree, source)?;
    engine.run_query(name, inputs)
}

/// L1 closed-code evaluation as a compiler service: evaluate a quoted
/// program against a tree with an explicit input environment (ordered
/// by `order`). This is the engine seam for callers holding a captured
/// `Code` value — stale-dependency verification and fuel bounding are
/// exercised through it directly.
pub fn evaluate_code_at(
    tree: &SyntaxTree,
    code: &Code,
    inputs: &BTreeMap<String, CValue>,
    order: &[String],
    source: Option<&Path>,
) -> Result<CValue, ConstructorError> {
    let mut engine = engine_from_tree_at(tree, source)?;
    engine.eval_closed_code(code, inputs, order)
}

/// Evaluate a function under a work budget, returning a checkpoint on suspension.
pub fn evaluate_function_budgeted(
    tree: &SyntaxTree,
    name: &str,
    inputs: &BTreeMap<String, CValue>,
    work_limit: u64,
    checkpoint: Option<&Checkpoint>,
    source_id: &str,
) -> Result<CValue, (ConstructorError, Checkpoint)> {
    evaluate_function_budgeted_at(tree, name, inputs, work_limit, checkpoint, source_id, None, "")
}

/// Budgeted evaluation that also reports the work units a completed run
/// consumed, for `--work` tuning telemetry (run receipts; a suspension
/// still returns the checkpoint, whose `work`/`remaining` fields carry
/// the same accounting).
pub fn evaluate_function_budgeted_reported(
    tree: &SyntaxTree,
    name: &str,
    inputs: &BTreeMap<String, CValue>,
    work_limit: u64,
    checkpoint: Option<&Checkpoint>,
    source_id: &str,
    source_path: Option<&Path>,
    source_text: &str,
) -> Result<(CValue, u64), (ConstructorError, Checkpoint)> {
    let mut engine = engine_from_tree_at(tree, source_path).map_err(|err| {
        (
            err,
            Checkpoint {
                schema: CHECKPOINT_SCHEMA.into(),
                source_id: source_id.into(),
                image: IMAGE_IDENTITY.into(),
                abi: CHECKPOINT_ABI.into(),
                work: 0,
                remaining: work_limit,
                accounting: ACCOUNTING_VERSION.into(),
                memo: BTreeMap::new(),
                frames: Vec::new(),
                scopes: BTreeSet::new(),
                function: name.into(),
                source: source_text.into(),
                inputs: inputs.clone(),
                next_ref: 0,
            },
        )
    })?;
    engine.work_limit = work_limit;
    engine.source_id = source_id.into();
    engine.entry = name.into();
    engine.source_text = source_text.into();
    engine.inputs = inputs.clone();
    if let Some(checkpoint) = checkpoint {
        engine.restore(checkpoint).map_err(|err| (err, checkpoint.clone()))?;
    }
    match evaluate_function_on(&mut engine, name, inputs) {
        Ok(value) => Ok((value, engine.work)),
        Err(err) => Err((err, engine.snapshot())),
    }
}

/// Budgeted evaluation with an optional source path (for `use`) and source text (for CLI resume).
pub fn evaluate_function_budgeted_at(
    tree: &SyntaxTree,
    name: &str,
    inputs: &BTreeMap<String, CValue>,
    work_limit: u64,
    checkpoint: Option<&Checkpoint>,
    source_id: &str,
    source_path: Option<&Path>,
    source_text: &str,
) -> Result<CValue, (ConstructorError, Checkpoint)> {
    match evaluate_function_budgeted_reported(
        tree,
        name,
        inputs,
        work_limit,
        checkpoint,
        source_id,
        source_path,
        source_text,
    ) {
        Ok((value, _)) => Ok(value),
        Err(err) => Err(err),
    }
}

pub(super) fn evaluate_function_on(
    engine: &mut Engine,
    name: &str,
    inputs: &BTreeMap<String, CValue>,
) -> Result<CValue, ConstructorError> {
    if !engine.resume_frames.is_empty() {
        return engine.finish_from_stack();
    }
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
    engine.eval_fn(name, &decl, &args)
}

