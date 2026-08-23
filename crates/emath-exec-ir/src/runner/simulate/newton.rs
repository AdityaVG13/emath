//! Causalized implicit-DAE Newton solver: forward-difference Jacobian
//! and Gaussian elimination.

use crate::interp::{evaluate, Value};
use crate::{lower_definition, EmirProgram};
use emath_ir::{Declaration, ModelResidual, SemanticPackage};
use std::collections::BTreeMap;

/// One unknown of the causalized residual system.
#[derive(Clone, Copy, Debug)]
struct NewtonUnknown {
    /// Bind-table slot (`LoadInput` index) of the unknown's value.
    bind_index: usize,
    /// Component count (1 for a scalar, `n` for a vector).
    width: usize,
}

/// Solve a model's implicit residual system at the current state.
///
/// Causalization: every equation that is not an explicit rate or an
/// algebraic definition is a residual `left - right`. The unknowns are the
/// declaration's `algebraic:` variables (guesses from `inputs`) plus the
/// implicit state rates `der(x)`. Newton's method iterates
/// `x -= J⁻¹ F(x)`; the Jacobian comes from forward differences (one
/// residual evaluation per unknown component), so residuals may mix any
/// builtin function and vector shapes.
///
/// Returns the solved algebraic values (fed back into definitions) and
/// the solved rate values keyed as `der_<state>`.
pub(super) fn causal_newton(
    package: &SemanticPackage,
    declaration: &Declaration,
    inputs: &BTreeMap<String, Value>,
    state: &BTreeMap<String, Value>,
    residuals: &[ModelResidual],
    algebraic_names: &[String],
    rate_names: &[String],
) -> Result<(BTreeMap<String, Value>, BTreeMap<String, Value>), String> {
    const MAX_ITER: usize = 30;
    const TOL: f64 = 1e-9;

    let input_names: Vec<String> = declaration
        .inputs
        .iter()
        .map(|field| field.name.clone())
        .collect();
    let state_names: Vec<String> = declaration
        .state
        .iter()
        .map(|field| field.name.clone())
        .collect();
    let mut state_values = Vec::with_capacity(state_names.len());
    for name in &state_names {
        let Some(value) = state.get(name) else {
            return Err(format!("missing state `{name}`"));
        };
        state_values.push(value.clone());
    }

    let mut bind_names = input_names.clone();
    for name in algebraic_names {
        if !bind_names.iter().any(|existing| existing == name) {
            bind_names.push(name.clone());
        }
    }
    let rate_offset = bind_names.len();
    for rate in rate_names {
        bind_names.push(format!("__rate_{rate}"));
    }

    // Declaration inputs must be present — silent `0.0` defaults invent
    // parameter values and can converge to a wrong DAE solution (same
    // refuse-silent-defaults rule as Optimize in interp). Rate unknowns
    // (`__rate_*`) start at 0.0 by construction below.
    let mut bind_values: Vec<Value> = Vec::with_capacity(bind_names.len());
    for name in &bind_names {
        if let Some(value) = inputs.get(name) {
            bind_values.push(value.clone());
            continue;
        }
        if name.starts_with("__rate_") {
            bind_values.push(Value::F64(0.0));
            continue;
        }
        return Err(format!("missing input `{name}`"));
    }

    let mut unknowns: Vec<NewtonUnknown> = Vec::new();
    let mut x: Vec<f64> = Vec::new();
    for (index, name) in algebraic_names.iter().enumerate() {
        let value = inputs
            .get(name)
            .cloned()
            .ok_or_else(|| format!("missing algebraic-variable guess `{name}` in the simulate inputs map"))?;
        let width = value_width(&value, name)?;
        unknowns.push(NewtonUnknown {
            bind_index: input_names.len() + index,
            width,
        });
        append_flatten(&mut x, &value)?;
    }
    for (index, rate) in rate_names.iter().enumerate() {
        let width = match state.get(rate) {
            Some(Value::F64(_)) => 1,
            Some(Value::Vector(items)) => items.len(),
            other => {
                return Err(format!(
                    "rate unknown `der({rate})` needs a scalar or vector state, found {other:?}"
                ));
            }
        };
        let start = value_of_width(width)?;
        unknowns.push(NewtonUnknown {
            bind_index: rate_offset + index,
            width,
        });
        append_flatten(&mut x, &start)?;
        bind_values[rate_offset + index] = start;
    }

    let programs: Vec<EmirProgram> = residuals
        .iter()
        .map(|residual| {
            lower_definition(package, residual.expr, &bind_names, &state_names)
                .map_err(|detail| format!("residual lowering failed: {detail}"))
        })
        .collect::<Result<_, _>>()?;

    let mut f = eval_residuals(&programs, &bind_values, &state_values)?;
    let total = x.len();
    let mut converged = max_abs(&f) < TOL;
    for _ in 0..MAX_ITER {
        if converged {
            break;
        }
        // Forward-difference Jacobian.
        let mut jac = vec![vec![0.0; total]; f.len()];
        let mut offset = 0_usize;
        for unknown in &unknowns {
            for component in 0..unknown.width {
                let column = offset + component;
                let h = 1e-7 * (1.0 + x[column].abs());
                let saved = x[column];
                x[column] += h;
                set_unknowns(&mut bind_values, &unknowns, &x);
                let plus = eval_residuals(&programs, &bind_values, &state_values)?;
                for (i, row) in jac.iter_mut().enumerate() {
                    row[column] = (plus[i] - f[i]) / h;
                }
                x[column] = saved;
            }
            offset += unknown.width;
        }
        set_unknowns(&mut bind_values, &unknowns, &x);

        let delta = gaussian_solve(&jac, &f).map_err(|message| {
            format!("implicit residual Jacobian is singular ({message}); check that the residual equations are independent")
        })?;
        for (column, step) in delta.iter().enumerate() {
            x[column] -= step;
        }
        set_unknowns(&mut bind_values, &unknowns, &x);
        f = eval_residuals(&programs, &bind_values, &state_values)?;
        let scale = x.iter().fold(1.0_f64, |acc, value| acc.max(value.abs()));
        converged = max_abs(&f) < TOL || max_abs(&delta) < 1e-12 * (1.0 + scale);
    }
    if max_abs(&f) > 1e-6 {
        return Err(format!(
            "implicit residual system did not converge within {MAX_ITER} Newton iterations (max residual {:.3e}); check the model equations and `algebraic:` guesses",
            max_abs(&f)
        ));
    }

    let mut algebraic_solved = BTreeMap::new();
    for (index, name) in algebraic_names.iter().enumerate() {
        algebraic_solved.insert(name.clone(), bind_values[input_names.len() + index].clone());
    }
    let mut rate_solved = BTreeMap::new();
    for (index, rate) in rate_names.iter().enumerate() {
        rate_solved.insert(format!("der_{rate}"), bind_values[rate_offset + index].clone());
    }
    Ok((algebraic_solved, rate_solved))
}

fn value_width(value: &Value, name: &str) -> Result<usize, String> {
    match value {
        Value::F64(_) | Value::I64(_) => Ok(1),
        Value::Vector(items) => Ok(items.len()),
        _ => Err(format!(
            "algebraic variable `{name}` must be a scalar or vector, found {value:?}"
        )),
    }
}

fn value_of_width(width: usize) -> Result<Value, String> {
    if width == 1 {
        Ok(Value::F64(0.0))
    } else {
        Ok(Value::Vector(vec![0.0; width]))
    }
}

fn append_flatten(out: &mut Vec<f64>, value: &Value) -> Result<(), String> {
    match value {
        Value::F64(v) => out.push(*v),
        Value::I64(v) => out.push(*v as f64),
        Value::Vector(items) => out.extend_from_slice(items),
        _ => return Err("unknown must be a scalar or vector".to_string()),
    }
    Ok(())
}

fn set_unknowns(bind_values: &mut [Value], unknowns: &[NewtonUnknown], x: &[f64]) {
    let mut offset = 0_usize;
    for unknown in unknowns {
        bind_values[unknown.bind_index] = if unknown.width == 1 {
            Value::F64(x[offset])
        } else {
            Value::Vector(x[offset..offset + unknown.width].to_vec())
        };
        offset += unknown.width;
    }
}

fn eval_residuals(
    programs: &[EmirProgram],
    bind_values: &[Value],
    state_values: &[Value],
) -> Result<Vec<f64>, String> {
    let mut out = Vec::new();
    for program in programs {
        let value =
            evaluate(program, bind_values, state_values).map_err(|fault| {
                format!("residual evaluation fault: {fault:?}")
            })?;
        match value {
            Value::F64(v) => out.push(v),
            Value::I64(v) => out.push(v as f64),
            Value::Vector(items) => out.extend_from_slice(&items),
            other => {
                return Err(format!(
                    "residual must evaluate to a scalar or vector, found {other:?}"
                ));
            }
        }
    }
    Ok(out)
}

fn max_abs(values: &[f64]) -> f64 {
    values.iter().fold(0.0_f64, |acc, value| acc.max(value.abs()))
}

/// Solve `matrix * x = rhs` by Gaussian elimination with partial pivoting.
fn gaussian_solve(matrix: &[Vec<f64>], rhs: &[f64]) -> Result<Vec<f64>, String> {
    let n = rhs.len();
    if matrix.len() != n || matrix.iter().any(|row| row.len() != n) {
        return Err("Jacobian is not square".to_string());
    }
    if n == 0 {
        return Ok(Vec::new());
    }
    let mut a: Vec<Vec<f64>> = matrix.to_vec();
    let mut b: Vec<f64> = rhs.to_vec();
    for col in 0..n {
        let mut pivot = col;
        let mut best = a[col][col].abs();
        for row in (col + 1)..n {
            let candidate = a[row][col].abs();
            if candidate > best {
                best = candidate;
                pivot = row;
            }
        }
        if best < 1e-300 {
            return Err(format!("near-zero pivot in column {col}"));
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        for row in (col + 1)..n {
            let factor = a[row][col] / a[col][col];
            if factor == 0.0 {
                continue;
            }
            for k in col..n {
                a[row][k] -= factor * a[col][k];
            }
            b[row] -= factor * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let mut acc = b[row];
        for k in (row + 1)..n {
            acc -= a[row][k] * x[k];
        }
        x[row] = acc / a[row][row];
    }
    Ok(x)
}
