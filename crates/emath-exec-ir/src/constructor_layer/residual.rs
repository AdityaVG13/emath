use super::{CValue, Expr, SyntaxTree, ConstructorError, BTreeSet, BTreeMap, Engine, Code, ExprKind};
use super::prelude::*;

pub(super) enum Folded {
    Value(CValue),
    Residual(Expr),
}

/// Quote-eliminate a function to residual expressions over its runtime inputs.
///
/// Compile-time `quote` operations run on the constructor VM. `quote.evaluate`
/// of transformed code becomes the residual body that emission can lower.
/// Opaque or missing transformation rules stay unresolved.
pub fn residual_output_exprs(
    tree: &SyntaxTree,
    name: &str,
) -> Result<Vec<(String, Expr)>, ConstructorError> {
    let mut engine = engine_from_tree(tree)?;
    let decl = engine
        .functions
        .get(name)
        .cloned()
        .ok_or_else(|| fault("unbound", format!("unknown function `{name}`")))?;
    let runtime: BTreeSet<String> = decl.inputs.iter().cloned().collect();
    let mut residuals = BTreeMap::new();
    // Definitions whose folded code has runtime residuals spliced in
    // (`quote.substitute(t, "c", k)` with `k` an input): the splice
    // holds the input's NAME where the VM holds its value, so the code
    // agrees with the VM only when it is evaluated or substituted
    // further. Any other use abandons the fold for native lowering.
    let mut staged = BTreeSet::new();
    for (def_name, expr) in &decl.defs {
        if observes_staged(expr, &staged) {
            return Err(fault(
                "unresolved",
                format!("`{def_name}` inspects code staged over runtime inputs"),
            ));
        }
        match fold_expr(&mut engine, expr, &runtime, &residuals)? {
            Folded::Value(value) => {
                if matches!(value, CValue::Code(_))
                    && mentions_any(expr, |name| {
                        runtime.contains(name)
                            || residuals.contains_key(name)
                            || staged.contains(name)
                    })
                {
                    staged.insert(def_name.clone());
                }
                engine.env.insert(def_name.clone(), value);
            }
            Folded::Residual(residual) => {
                residuals.insert(def_name.clone(), residual);
            }
        }
    }
    let wanted = if decl.outputs.is_empty() {
        decl.defs
            .last()
            .map(|(def_name, _)| vec![def_name.clone()])
            .unwrap_or_default()
    } else {
        decl.outputs.clone()
    };
    let mut outputs = Vec::new();
    for output in wanted {
        if let Some(residual) = residuals.get(&output) {
            outputs.push((output, residual.clone()));
            continue;
        }
        let value = engine.env.get(&output).ok_or_else(|| {
            fault("unbound", format!("missing output `{output}`"))
        })?;
        if !cvalue_emittable(value) {
            return Err(fault(
                "unresolved",
                format!("output `{output}` is not a scalar residual"),
            ));
        }
        outputs.push((output, value_to_expr(value)));
    }
    Ok(outputs)
}

fn mentions_any(expr: &Expr, hit: impl Fn(&str) -> bool) -> bool {
    let mut free = BTreeSet::new();
    super::identity::free_path_names(expr, &mut Vec::new(), &mut free);
    free.iter()
        .any(|name| hit(name.split('.').next().unwrap_or(name)))
}

/// Whether `expr` uses a staged definition anywhere other than as the
/// code operand of `quote.evaluate` or `quote.substitute`.
fn observes_staged(expr: &Expr, staged: &BTreeSet<String>) -> bool {
    if staged.is_empty() {
        return false;
    }
    if let Some(name) = call_path(expr)
        && (name == "quote.evaluate" || name == "quote.substitute")
        && let ExprKind::Call { args, .. } = &expr.kind
    {
        return args.iter().enumerate().any(|(position, arg)| {
            let staged_operand = position == 0
                && matches!(&arg.kind, ExprKind::Path { segments, .. }
                    if segments.len() == 1 && staged.contains(&segments[0]));
            !staged_operand && observes_staged(arg, staged)
        });
    }
    mentions_any(expr, |name| staged.contains(name))
}

pub(super) fn cvalue_emittable(value: &CValue) -> bool {
    match value {
        CValue::Bool(_) | CValue::Int(_) | CValue::Rat { .. } | CValue::Float64(_) => true,
        CValue::Sequence(items) => items.iter().all(cvalue_emittable),
        CValue::Tuple(items) => items.iter().all(cvalue_emittable),
        CValue::Record { fields, .. } => fields.values().all(cvalue_emittable),
        _ => false,
    }
}

pub(super) fn fold_expr(
    engine: &mut Engine,
    expr: &Expr,
    runtime: &BTreeSet<String>,
    residuals: &BTreeMap<String, Expr>,
) -> Result<Folded, ConstructorError> {
    if let Some(name) = call_path(expr) {
        if name == "quote.evaluate" {
            let arg = call_arg(expr, 0)?;
            return match fold_expr(engine, arg, runtime, residuals)? {
                Folded::Value(CValue::Code(code)) => Ok(Folded::Residual(code.expr)),
                Folded::Value(other) => Ok(Folded::Value(other)),
                // A runtime code input keeps its guarded executor:
                // the call survives to emission (CodeEvaluate renders
                // the tree evaluator over the runtime carrier).
                // Collapsing onto the bare argument would drop the
                // evaluation - `literal_exp`'s Int became the un
                // evaluated code and every numeric use misrendered.
                Folded::Residual(_) => Ok(Folded::Residual(expr.clone())),
            };
        }
        if name == "quote.substitute" {
            let fragment = fold_expr(engine, call_arg(expr, 0)?, runtime, residuals)?;
            let fragment = match fragment {
                Folded::Value(value) => value,
                // A runtime fragment substitutes at runtime
                // (CodeSubstitute renders the structural lane); the
                // compile-time splice only applies to a folded
                // literal fragment.
                Folded::Residual(_) => {
                    return Ok(Folded::Residual(expr.clone()));
                }
            };
            let reference = substitute_reference(engine, call_arg(expr, 1)?, runtime, residuals)?;
            // A runtime replacement substitutes at runtime too: splicing
            // its residual expression would leave the input's NAME in
            // the code where the VM puts its value, which every later
            // view, make, or identity of that code observes.
            let replacement = match fold_expr(engine, call_arg(expr, 2)?, runtime, residuals)? {
                Folded::Value(value) => value_to_expr(&value),
                Folded::Residual(inner) => inner,
            };
            let term = fragment_term(fragment).map_err(|message| fault("type", message))?;
            let mut cap_mint: u64 = 0;
            let substituted = substitute_path(&term, &reference, &replacement, &mut cap_mint);
            return Ok(Folded::Value(CValue::Code(Box::new(Code {
                expr: substituted,
                deps: BTreeMap::new(),
            }))));
        }
    }
    if let ExprKind::Path { segments, .. } = &expr.kind {
        if segments.len() == 1 {
            if let Some(residual) = residuals.get(&segments[0]) {
                return Ok(Folded::Residual(residual.clone()));
            }
            if runtime.contains(&segments[0]) && !engine.env.contains_key(&segments[0]) {
                return Ok(Folded::Residual(expr.clone()));
            }
        }
        if segments.len() == 2 {
            if let Some(residual) = residuals.get(&segments[0]) {
                return project_residual(residual, &segments[1]);
            }
        }
    }
    match engine.eval(expr) {
        Ok(value) => Ok(Folded::Value(value)),
        Err(err) if err.code == "unbound" => {
            fold_runtime_expr(engine, expr, runtime, residuals, err)
        }
        Err(err) => Err(err),
    }
}

pub(super) fn fold_runtime_expr(
    engine: &mut Engine,
    expr: &Expr,
    runtime: &BTreeSet<String>,
    residuals: &BTreeMap<String, Expr>,
    unbound: ConstructorError,
) -> Result<Folded, ConstructorError> {
    match &expr.kind {
        ExprKind::Binary { op, left, right } => {
            let left = folded_to_expr(fold_expr(engine, left, runtime, residuals)?);
            let right = folded_to_expr(fold_expr(engine, right, runtime, residuals)?);
            Ok(Folded::Residual(dummy_expr(ExprKind::Binary {
                op: *op,
                left: Box::new(left),
                right: Box::new(right),
            })))
        }
        ExprKind::Unary { op, value } => {
            let value = folded_to_expr(fold_expr(engine, value, runtime, residuals)?);
            Ok(Folded::Residual(dummy_expr(ExprKind::Unary {
                op: *op,
                value: Box::new(value),
            })))
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => Ok(Folded::Residual(dummy_expr(ExprKind::If {
            condition: Box::new(folded_to_expr(fold_expr(
                engine, condition, runtime, residuals,
            )?)),
            then_value: Box::new(folded_to_expr(fold_expr(
                engine, then_value, runtime, residuals,
            )?)),
            else_value: Box::new(folded_to_expr(fold_expr(
                engine, else_value, runtime, residuals,
            )?)),
        }))),
        ExprKind::List(items) => Ok(Folded::Residual(dummy_expr(ExprKind::List(fold_expr_list(
            engine, items, runtime, residuals,
        )?)))),
        ExprKind::Tuple(items) => Ok(Folded::Residual(dummy_expr(ExprKind::Tuple(fold_expr_list(
            engine, items, runtime, residuals,
        )?)))),
        ExprKind::Index { value, indices } => Ok(Folded::Residual(dummy_expr(ExprKind::Index {
            value: Box::new(folded_to_expr(fold_expr(engine, value, runtime, residuals)?)),
            indices: fold_expr_list(engine, indices, runtime, residuals)?,
        }))),
        _ => Err(unbound),
    }
}

pub(super) fn fold_expr_list(
    engine: &mut Engine,
    items: &[Expr],
    runtime: &BTreeSet<String>,
    residuals: &BTreeMap<String, Expr>,
) -> Result<Vec<Expr>, ConstructorError> {
    items
        .iter()
        .map(|item| fold_expr(engine, item, runtime, residuals).map(folded_to_expr))
        .collect()
}

pub(super) fn folded_to_expr(folded: Folded) -> Expr {
    match folded {
        Folded::Value(value) => value_to_expr(&value),
        Folded::Residual(expr) => expr,
    }
}

pub(super) fn project_residual(expr: &Expr, field: &str) -> Result<Folded, ConstructorError> {
    if let Some(index) = tuple_index(field) {
        match &expr.kind {
            ExprKind::Tuple(items) | ExprKind::List(items) => items
                .get(index)
                .cloned()
                .map(Folded::Residual)
                .ok_or_else(|| fault("invalid_index", format!("no element `{field}`"))),
            _ => Ok(Folded::Residual(dummy_expr(ExprKind::Index {
                value: Box::new(expr.clone()),
                indices: vec![dummy_expr(ExprKind::Int(index.to_string()))],
            }))),
        }
    } else {
        match &expr.kind {
            ExprKind::Record { fields, .. } => fields
                .iter()
                .find(|(name, _)| name == field)
                .map(|(_, value)| Folded::Residual(value.clone()))
                .ok_or_else(|| fault("unbound", format!("no field `{field}`"))),
            ExprKind::Path { segments, generics } => {
                let mut segments = segments.clone();
                segments.push(field.to_string());
                Ok(Folded::Residual(dummy_expr(ExprKind::Path {
                    segments,
                    generics: generics.clone(),
                })))
            }
            _ => Err(fault(
                "unresolved",
                format!("cannot project `{field}` from residual"),
            )),
        }
    }
}

pub(super) fn call_path(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Call { function, .. } => match &function.kind {
            ExprKind::Path { segments, .. } => Some(segments.join(".")),
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn call_arg(expr: &Expr, index: usize) -> Result<&Expr, ConstructorError> {
    match &expr.kind {
        ExprKind::Call { args, .. } => args.get(index).ok_or_else(|| {
            fault("arity", format!("call is missing argument {index}"))
        }),
        _ => Err(fault("type", "expected a call")),
    }
}

pub(super) fn substitute_reference(
    engine: &mut Engine,
    expr: &Expr,
    runtime: &BTreeSet<String>,
    residuals: &BTreeMap<String, Expr>,
) -> Result<String, ConstructorError> {
    match &expr.kind {
        ExprKind::Str(name) => Ok(name.clone()),
        ExprKind::Path { segments, .. } if segments.len() == 1 => Ok(segments[0].clone()),
        _ => match fold_expr(engine, expr, runtime, residuals)? {
            Folded::Value(CValue::Record { type_name, .. }) => Ok(type_name.to_string()),
            Folded::Value(CValue::Code(code)) => match &code.expr.kind {
                ExprKind::Path { segments, .. } => Ok(segments.join(".")),
                _ => Err(fault(
                    "type",
                    "quote.substitute reference must name a binder",
                )),
            },
            Folded::Residual(inner) => match &inner.kind {
                ExprKind::Path { segments, .. } => Ok(segments.join(".")),
                _ => Err(fault(
                    "type",
                    "quote.substitute reference must name a binder",
                )),
            },
            Folded::Value(other) => Err(fault(
                "type",
                format!("quote.substitute reference must name a binder, found {other}"),
            )),
        },
    }
}

