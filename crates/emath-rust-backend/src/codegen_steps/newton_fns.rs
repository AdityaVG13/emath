//! Newton runtime support fns embedded in generated crates.

use super::*;

/// Module-level helper: max-abs of a residual vector (interpreter parity
/// with `newton.rs`'s `max_abs`).
pub(super) fn newton_max_abs_fn() -> FnDef {
    FnDef {
        name: "__emath_max_abs".to_string(),
        generics: vec![],
        params: vec![Param {
            name: "values".to_string(),
            ty: Ty::Ref(Box::new(Ty::Named("Vec<f64>".to_string()))),
        }],
        ret: Ty::F64,
        body: Stmt::Block(Block {
            statements: vec![Stmt::Expr(Expr::Raw(
                "values.iter().fold(0.0f64, |acc, value| acc.max(value.abs()))".to_string(),
            ))],
        }),
        doc: vec!["Generated Newton helper: max-abs of a residual vector.".to_string()],
        visibility: Visibility::Private,
        attrs: Vec::new(),
    }
}

/// Module-level helper: partial-pivot Gaussian elimination, ported
/// verbatim from the interpreter's `causal_newton` solver so generated
/// steps match `emath simulate` bit-for-bit on the same inputs.
pub(super) fn newton_gaussian_solve_fn() -> FnDef {
    FnDef {
        name: "__emath_gaussian_solve".to_string(),
        generics: vec![],
        params: vec![
            Param {
                name: "matrix".to_string(),
                ty: Ty::Ref(Box::new(Ty::Named("Vec<Vec<f64>>".to_string()))),
            },
            Param {
                name: "rhs".to_string(),
                ty: Ty::Ref(Box::new(Ty::Named("Vec<f64>".to_string()))),
            },
        ],
        ret: Ty::Result {
            ok: Box::new(Ty::Named("Vec<f64>".to_string())),
            error: Box::new(Ty::Named("String".to_string())),
        },
        body: newton_raw_body(
            "let n = rhs.len();\n\
             if matrix.len() != n || matrix.iter().any(|row| row.len() != n) {\n\
             \x20   return Err(\"Jacobian is not square\".to_string());\n\
             }\n\
             if n == 0 { return Ok(Vec::new()); }\n\
             let mut a: Vec<Vec<f64>> = matrix.clone();\n\
             let mut b: Vec<f64> = rhs.clone();\n\
             for col in 0..n {\n\
             \x20   let mut pivot = col;\n\
             \x20   let mut best = a[col][col].abs();\n\
             \x20   for row in (col + 1)..n {\n\
             \x20       let candidate = a[row][col].abs();\n\
             \x20       if candidate > best { best = candidate; pivot = row; }\n\
             \x20   }\n\
             \x20   if best < 1e-300 { return Err(format!(\"near-zero pivot in column {col}\")); }\n\
             \x20   a.swap(col, pivot);\n\
             \x20   b.swap(col, pivot);\n\
             \x20   for row in (col + 1)..n {\n\
             \x20       let factor = a[row][col] / a[col][col];\n\
             \x20       if factor == 0.0 { continue; }\n\
             \x20       for k in col..n { a[row][k] -= factor * a[col][k]; }\n\
             \x20       b[row] -= factor * b[col];\n\
             \x20   }\n\
             }\n\
             let mut x = vec![0.0f64; n];\n\
             for row in (0..n).rev() {\n\
             \x20   let mut acc = b[row];\n\
             \x20   for k in (row + 1)..n { acc -= a[row][k] * x[k]; }\n\
             \x20   x[row] = acc / a[row][row];\n\
             }\n\
             Ok(x)",
        ),
        doc: vec![
            "Generated Newton helper: Gaussian elimination with partial pivoting.".to_string(),
        ],
        visibility: Visibility::Private,
        attrs: Vec::new(),
    }
}

/// Wrap generated statements in a block expression (`{ ... }`) that can
/// serve as a function body or a `let` value with a trailing expression.
pub(super) fn newton_raw_body(text: &str) -> Stmt {
    Stmt::Block(Block {
        statements: vec![Stmt::Expr(Expr::Raw(format!("{{ {text} }}")))],
    })
}

/// One stage block of a causalized Newton step: state locals, the flat
/// `__x` solve vector (unknowns in order: algebraic then rate), residual
/// closures, and the interpreter-mirroring Newton loop. The block value
/// is the flattened per-state rates vector in state order.
pub(super) fn newton_stage_text(
    receiver: &str,
    state_names: &[String],
    state_widths: &[usize],
    algebraic: &[emath_ir::Field],
    rate_names: &[String],
    unknown_widths: &[usize],
    algebraic_width_total: usize,
    residual_sources: &[(u16, String)],
    def_sources: &[(String, String)],
) -> String {
    let mut out = String::new();
    // State locals: `st_<state>` reads with the stage receiver, so the
    // residual closures and definition lets are pure-of-`self` and can be
    // re-emitted for each shifted stage.
    for (index, name) in state_names.iter().enumerate() {
        let scalar = state_widths[index] == 1;
        out.push_str(&format!(
            "let st_{name} = ({receiver}).{name}{};\n",
            if scalar { "" } else { ".clone()" }
        ));
    }
    // Flat solve vector. Algebraic guesses come from the stage receiver
    // (extended DAE state), not from extra step parameters.
    out.push_str("let mut __x: Vec<f64> = Vec::new();\n");
    let mut slot = 0usize;
    for field in algebraic {
        let width = unknown_widths[slot];
        let name = escape_ident(&field.name);
        if width == 1 {
            out.push_str(&format!("__x.push(({receiver}).{name});\n"));
        } else {
            out.push_str(&format!("__x.extend(({receiver}).{name}.clone());\n"));
        }
        slot += 1;
    }
    for width in &unknown_widths[algebraic.len()..] {
        if *width == 1 {
            out.push_str("__x.push(0.0);\n");
        } else {
            out.push_str(&format!("__x.extend(vec![0.0; {width}]);\n"));
        }
    }
    // Residual closures.
    for (index, (components, source)) in residual_sources.iter().enumerate() {
        let ret = if *components == 1 { "f64" } else { "Vec<f64>" };
        out.push_str(&format!(
            "let __r{index} = |x: &[f64]| -> {ret} {{ {source} }};\n"
        ));
    }
    // F assembly (mirrors `eval_residuals`).
    out.push_str("let __eval = |x: &[f64]| -> Vec<f64> {\n");
    out.push_str("    let mut out = Vec::new();\n");
    for (index, (components, _)) in residual_sources.iter().enumerate() {
        if *components == 1 {
            out.push_str(&format!("    out.push(__r{index}(x));\n"));
        } else {
            out.push_str(&format!("    out.extend(__r{index}(x));\n"));
        }
    }
    out.push_str("    out\n};\n");
    // Bring the interpreter's constants into scope by literal, not by
    // reference: generated crates are self-contained.
    out.push_str(&format!(
        "let mut __f = __eval(&__x);\n\
         let mut __converged = __emath_max_abs(&__f) < {NEWTON_TOL};\n\
         for _ in 0..{NEWTON_MAX_ITER}u32 {{\n\
         \x20   if __converged {{ break; }}\n\
         \x20   let n = __x.len();\n\
         \x20   let mut __jac = vec![vec![0.0f64; n]; __f.len()];\n\
         \x20   for col in 0..n {{\n\
         \x20       let h = 1e-7 * (1.0 + __x[col].abs());\n\
         \x20       let saved = __x[col];\n\
         \x20       __x[col] += h;\n\
         \x20       let __plus = __eval(&__x);\n\
         \x20       for (row, value) in __plus.iter().enumerate() {{\n\
         \x20           __jac[row][col] = (value - __f[row]) / h;\n\
         \x20       }}\n\
         \x20       __x[col] = saved;\n\
         \x20   }}\n\
         \x20   let __delta = match __emath_gaussian_solve(&__jac, &__f) {{\n\
         \x20       Ok(delta) => delta,\n\
         \x20       Err(message) => {{\n\
         \x20           return Err(format!(\"implicit residual Jacobian is singular ({{message}}); check that the residual equations are independent\"));\n\
         \x20       }}\n\
         \x20   }};\n\
         \x20   for (index, step) in __delta.iter().enumerate() {{\n\
         \x20       __x[index] -= *step;\n\
         \x20   }}\n\
         \x20   __f = __eval(&__x);\n\
         \x20   let __scale = __x.iter().fold(1.0f64, |acc, value| acc.max(value.abs()));\n\
         \x20   __converged = __emath_max_abs(&__f) < {NEWTON_TOL} || __emath_max_abs(&__delta) < 1e-12 * (1.0 + __scale);\n\
         }}\n\
         if __emath_max_abs(&__f) > {NEWTON_FINAL_TOL} {{\n\
         \x20   return Err(format!(\"implicit residual system did not converge within {NEWTON_MAX_ITER} Newton iterations (max residual {{:.3e}}); check the model equations and `algebraic:` guesses\", __emath_max_abs(&__f)));\n\
         }}\n"
    ));
    // Definition lets (see solved algebraic values).
    for (name, source) in def_sources {
        out.push_str(&format!("let {} = {source};\n", escape_ident(name)));
    }
    // Flattened per-state rates in state order.
    out.push_str("let mut __rates: Vec<f64> = Vec::new();\n");
    let mut rate_start = algebraic_width_total;
    for name in state_names {
        if let Some(rate_index) = rate_names.iter().position(|rate| rate == name) {
            let width = unknown_widths[algebraic.len() + rate_index];
            if width == 1 {
                out.push_str(&format!("__rates.push(__x[{rate_start}]);\n"));
            } else {
                out.push_str(&format!(
                    "__rates.extend(__x[{rate_start}..{}+{width}].to_vec());\n",
                    rate_start
                ));
            }
            rate_start += width;
        } else {
            // Explicit rate definition: the chain above bound it as a let
            // (`der_<state>`), and the solved algebraic values are inside
            // `__x`, so the definition reads them exactly like the
            // interpreter's "solve then re-evaluate definitions" step.
            out.push_str(&format!(
                "__rates.push({});\n",
                escape_ident(&format!("der_{name}"))
            ));
        }
    }
    out.push_str("let mut __alg: Vec<f64> = Vec::new();\n");
    if algebraic_width_total > 0 {
        out.push_str(&format!(
            "__alg.extend(__x[..{algebraic_width_total}].iter().copied());\n"
        ));
    }
    out.push_str("(__rates, __alg)\n");
    out
}

/// Unpack flattened algebraic components from `__{prefix}_alg` into
/// named locals `{prefix}_{field}` (scalar or `Vec<f64>`).
pub(super) fn newton_unpack_lets(
    prefix: &str,
    kind: &str,
    fields: &[emath_ir::Field],
    widths: &[usize],
) -> Vec<Stmt> {
    let mut offset = 0usize;
    let mut out = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        let width = widths[index];
        let src = if width == 1 {
            format!("__{prefix}_{kind}[{offset}]")
        } else {
            format!("__{prefix}_{kind}[{offset}..{offset}+{width}].to_vec()")
        };
        out.push(Stmt::Let {
            pattern: format!("{prefix}_{}", escape_ident(&field.name)),
            value: Box::new(Expr::Raw(src)),
        });
        offset += width;
    }
    out
}

pub(super) fn newton_alg_field_exprs(
    prefix: &str,
    algebraic: &[emath_ir::Field],
) -> Vec<(String, Expr)> {
    algebraic
        .iter()
        .map(|field| {
            (
                field.name.clone(),
                Expr::Var(format!("{prefix}_{}", escape_ident(&field.name))),
            )
        })
        .collect()
}

/// `(state, rate read)` pairs for one RK stage, in state order:
/// `__k1_rates[<off>]` for scalar states and
/// `__k1_rates[<off>..<off>+<width>]` for vector states (widths are
/// static, per admission).
pub(super) fn newton_rate_args(
    prefix: &str,
    state_names: &[String],
    state_widths: &[usize],
) -> Vec<(String, String)> {
    let mut offset = 0usize;
    let mut out = Vec::new();
    for (index, name) in state_names.iter().enumerate() {
        let width = state_widths[index];
        let read = if width == 1 {
            format!("__{prefix}_rates[{offset}]")
        } else {
            format!("__{prefix}_rates[{offset}..{}+{width}]", offset)
        };
        out.push((name.clone(), read));
        offset += width;
    }
    out
}

/// Rewrite `self.<state>` field accesses to `st_<state>` so a rendered
/// expression can be re-emitted inside a stage block that holds state
/// locals instead of `self`. Longest names first with an identifier
/// boundary guard, so `q` cannot corrupt `q2`.
pub(super) fn replace_state_receiver(source: &str, state_names: &[String]) -> String {
    let mut ordered: Vec<&String> = state_names.iter().collect();
    ordered.sort_by_key(|name| std::cmp::Reverse(name.len()));
    let mut out = source.to_string();
    for name in ordered {
        let needle = format!("self.{name}");
        let mut rebuilt = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(pos) = rest.find(&needle) {
            rebuilt.push_str(&rest[..pos]);
            rebuilt.push_str(&format!("st_{name}"));
            let after = &rest[pos + needle.len()..];
            if let Some(next) = after.chars().next() {
                if next.is_ascii_alphanumeric() || next == '_' {
                    // A longer state name owns this span; skip past the
                    // self. prefix and let the longer name match next.
                    rebuilt.push_str("self.");
                    rest = &rest[pos + "self.".len()..];
                    continue;
                }
            }
            rest = after;
        }
        rebuilt.push_str(rest);
        out = rebuilt;
    }
    out
}
