//! Dual-number (tangent/adjoint) string rendering for AD ops.

use super::*;

pub(super) fn rust_format_template(template: &str) -> String {
    let mut output = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                output.push_str("{{");
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                output.push_str("}}");
            }
            '{' => {
                let mut field = String::new();
                for next in chars.by_ref() {
                    if next == '}' {
                        break;
                    }
                    field.push(next);
                }
                output.push('{');
                if let Some((_, spec)) = field.split_once(':') {
                    if let Some(precision) = spec
                        .strip_prefix('.')
                        .and_then(|value| value.strip_suffix('f'))
                    {
                        output.push_str(":.");
                        output.push_str(precision);
                    }
                }
                output.push('}');
            }
            _ => output.push(ch),
        }
    }
    output
}

/// Forward-mode tangent source for an EMIR op; `__e{N}`/`__d{N}` naming.
pub(crate) fn dual_tangent_str(op: &EmirOp, var_index: u16, idx: usize) -> String {
    // Primal registers may be i64 (`ConstI64`); tangent math is always f64.
    tangent_str(op, var_index, idx, &|n| format!("(__e{n} as f64)"), &|n| {
        format!("__d{n}")
    })
}

/// Shared tangent generator: `e`/`d` map register → primal/tangent refs.
pub(super) fn tangent_str(
    op: &EmirOp,
    var_index: u16,
    idx: usize,
    e: &dyn Fn(u32) -> String,
    d: &dyn Fn(u32) -> String,
) -> String {
    match op {
        EmirOp::ConstF64(_) => "0.0".to_string(),
        EmirOp::ConstI64(_) => "0.0".to_string(),
        EmirOp::ConstBool(_) => "0.0".to_string(),
        EmirOp::LoadInput(i) => {
            if *i == var_index {
                "1.0".to_string()
            } else {
                "0.0".to_string()
            }
        }
        EmirOp::LoadState(_) => "0.0".to_string(),
        EmirOp::F64Add(a, b) => format!("{} + {}", d(a.0), d(b.0)),
        EmirOp::F64Sub(a, b) => format!("{} - {}", d(a.0), d(b.0)),
        EmirOp::F64Mul(a, b) => {
            format!("{} * {} + {} * {}", d(a.0), e(b.0), e(a.0), d(b.0))
        }
        EmirOp::F64Div(a, b) => format!(
            "({} * {} - {} * {}) / ({} * {})",
            d(a.0),
            e(b.0),
            e(a.0),
            d(b.0),
            e(b.0),
            e(b.0)
        ),
        EmirOp::Neg(a) => format!("-{}", d(a.0)),
        EmirOp::UnaryBuiltin(id, a) => id.rust_tangent_unary(e, d, idx as u32, a.0),
        // Match interpreter: constant-exponent form when db==0 (avoids ln
        // for a<=0); otherwise general a^b * (b*a'/a + b'*ln(a)).
        EmirOp::F64Pow(a, b) => format!(
            "if {} == 0.0 {{ if {} == 0.0 {{ 0.0 }} else {{ {} * {}.powf({} - 1.0) * {} }} }} else {{ {} * ({} * {} / {} + {} * {}.ln()) }}",
            d(b.0),
            e(b.0),
            e(b.0),
            e(a.0),
            e(b.0),
            d(a.0),
            e(idx as u32),
            e(b.0),
            d(a.0),
            e(a.0),
            d(b.0),
            e(a.0)
        ),
        EmirOp::BinaryBuiltin(id, a, b) => id.rust_tangent_binary(e, d, idx as u32, a.0, b.0),
        EmirOp::Select {
            condition: c,
            then_value: t,
            else_value: ev,
        } => format!(
            "if {} != 0.0 {{ {} }} else {{ {} }}",
            e(c.0),
            d(t.0),
            d(ev.0)
        ),
        EmirOp::IsFinite(_) => "0.0".to_string(),
        EmirOp::Eq(..)
        | EmirOp::Ne(..)
        | EmirOp::Lt(..)
        | EmirOp::Le(..)
        | EmirOp::Gt(..)
        | EmirOp::Ge(..)
        | EmirOp::And(..)
        | EmirOp::Or(..)
        | EmirOp::Imply(..)
        | EmirOp::Iff(..)
        | EmirOp::Not(..) => "0.0".to_string(),
        _ => "0.0".to_string(),
    }
}

/// Backward-pass adjoint update statements for an EMIR op, accumulating
/// into `__ra{N}` operand adjoints and `__ria{N}` input adjoints.
pub(crate) fn reverse_adjoint_str(op: &EmirOp, idx: usize) -> String {
    let adj = format!("__ra{idx}");
    let p = |n: u32| format!("(__re{n} as f64)");
    let a = |n: u32| format!("__ra{n}");
    let updates = match op {
        EmirOp::ConstF64(_)
        | EmirOp::ConstI64(_)
        | EmirOp::ConstBool(_)
        | EmirOp::ConstComplex(..) => String::new(),
        EmirOp::LoadInput(i) => format!("__ria{i} += {adj};\n"),
        EmirOp::LoadState(_) => String::new(),
        EmirOp::F64Add(x, y) => format!("{} += {adj};\n{} += {adj};\n", a(x.0), a(y.0)),
        EmirOp::F64Sub(x, y) => format!("{} += {adj};\n{} -= {adj};\n", a(x.0), a(y.0)),
        EmirOp::F64Mul(x, y) => format!(
            "{} += {adj} * {};\n{} += {adj} * {};\n",
            a(x.0),
            p(y.0),
            a(y.0),
            p(x.0)
        ),
        EmirOp::F64Div(x, y) => format!(
            "{} += {adj} / {};\n{} -= {adj} * {} / ({} * {});\n",
            a(x.0),
            p(y.0),
            a(y.0),
            p(x.0),
            p(y.0),
            p(y.0)
        ),
        EmirOp::Neg(x) => format!("{} -= {adj};\n", a(x.0)),
        EmirOp::UnaryBuiltin(id, x) => id
            .rust_adjoint_unary(&adj, &p, idx as u32, x.0)
            .unwrap_or_default(),
        EmirOp::F64Pow(x, y) => format!(
            "if {} != 0.0 {{\n\
                 if {} != 0.0 {{ {} += {adj} * __re{idx} * {} / {}; }}\n\
                 else {{ {} += {adj} * {} * {}.powf({} - 1.0); }}\n\
             }}\n\
             {} += {adj} * __re{idx} * {}.ln();\n",
            p(y.0),
            p(x.0),
            a(x.0),
            p(y.0),
            p(x.0),
            a(x.0),
            p(y.0),
            p(x.0),
            p(y.0),
            a(y.0),
            p(x.0)
        ),
        EmirOp::BinaryBuiltin(id, x, y) => id
            .rust_adjoint_binary(&adj, &p, idx as u32, x.0, y.0)
            .unwrap_or_default(),
        EmirOp::Select {
            condition: c,
            then_value: t,
            else_value: ev,
        } => format!(
            "if {} != 0.0 {{ {} += {adj}; }} else {{ {} += {adj}; }}\n",
            p(c.0),
            a(t.0),
            a(ev.0)
        ),
        // Non-differentiable ops: no adjoint contribution.
        EmirOp::IsFinite(_)
        | EmirOp::Eq(..)
        | EmirOp::Ne(..)
        | EmirOp::Lt(..)
        | EmirOp::Le(..)
        | EmirOp::Gt(..)
        | EmirOp::Ge(..)
        | EmirOp::And(..)
        | EmirOp::Or(..)
        | EmirOp::Not(_)
        | EmirOp::Imply(..)
        | EmirOp::Iff(..) => String::new(),
        _ => String::new(),
    };
    if updates.is_empty() {
        String::new()
    } else {
        format!("if {adj} != 0.0 {{\n{updates}}}\n")
    }
}
