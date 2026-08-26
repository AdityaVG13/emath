//! Text helper functions — pure rendering of expressions and types for
//! trace text.

use emath_core::tree::{CommandArgument, Expr, ExprKind, Place, TypeExpr, TypeKind};

pub(super) fn place_text(place: &Place) -> String {
    place.segments.join(".")
}

pub(super) fn argument_text(argument: &CommandArgument) -> String {
    match argument {
        CommandArgument::Expr(expr) => expr_text(expr),
        CommandArgument::Assignment { name, value } => {
            format!("{name} = {}", expr_text(value))
        }
        CommandArgument::List(items) => format!(
            "[{}]",
            items.iter().map(expr_text).collect::<Vec<_>>().join(", ")
        ),
    }
}

/// Compact deterministic rendering of an expression for trace text.
#[must_use]
pub fn expr_text(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Int(value) | ExprKind::Float(value) | ExprKind::Str(value) => value.clone(),
        ExprKind::Rational { numer, denom } => format!("{numer}//{denom}"),
        ExprKind::Bool(value) => value.to_string(),
        ExprKind::Path { segments, generics } => {
            let mut out = segments.join("::");
            if let Some(generics) = generics {
                out.push('<');
                out.push_str(
                    &generics
                        .iter()
                        .map(generic_arg_text)
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                out.push('>');
            }
            out
        }
        ExprKind::Call { function, args } => format!(
            "{}({})",
            expr_text(function),
            args.iter().map(expr_text).collect::<Vec<_>>().join(", ")
        ),
        ExprKind::Index { value, indices } => format!(
            "{}[{}]",
            expr_text(value),
            indices.iter().map(expr_text).collect::<Vec<_>>().join(", ")
        ),
        ExprKind::Slice { start, end } => format!(
            "{}:{}",
            start.as_ref().map_or_else(String::new, |e| expr_text(e)),
            end.as_ref().map_or_else(String::new, |e| expr_text(e))
        ),
        ExprKind::Unary { op, value } => format!("{op:?}({})", expr_text(value)),
        ExprKind::Binary { op, left, right } => {
            format!("({} {op:?} {})", expr_text(left), expr_text(right))
        }
        ExprKind::Quantity { value, unit } => {
            format!("{} {}", expr_text(value), unit.to_string())
        }
        ExprKind::List(items) => format!(
            "[{}]",
            items.iter().map(expr_text).collect::<Vec<_>>().join(", ")
        ),
        ExprKind::Tuple(items) => format!(
            "({})",
            items.iter().map(expr_text).collect::<Vec<_>>().join(", ")
        ),
        ExprKind::Range {
            start,
            end,
            inclusive,
        } => format!(
            "{}..{}{}",
            start.as_ref().map_or_else(String::new, |e| expr_text(e)),
            if *inclusive { "=" } else { "" },
            end.as_ref().map_or_else(String::new, |e| expr_text(e))
        ),
        ExprKind::Derivative {
            value,
            wrt,
            kind,
            holding,
        } => {
            let prefix = match kind {
                emath_core::tree::DerivativeKind::Plain => "derivative",
                emath_core::tree::DerivativeKind::Partial => "partial",
                emath_core::tree::DerivativeKind::Total => "total",
            };
            let wrt_text = wrt.as_ref().map_or_else(String::new, |w| {
                format!(
                    " wrt {}",
                    w.iter().map(expr_text).collect::<Vec<_>>().join(", ")
                )
            });
            let holding_text = if holding.is_empty() {
                String::new()
            } else {
                format!(
                    " holding {}",
                    holding.iter().map(expr_text).collect::<Vec<_>>().join(", ")
                )
            };
            format!(
                "{}({}){}{}",
                prefix,
                expr_text(value),
                wrt_text,
                holding_text
            )
        }
        ExprKind::Solve { value, wrt } => {
            let wrt_text = wrt.as_ref().map_or_else(String::new, |w| {
                format!(
                    " wrt {}",
                    w.iter().map(expr_text).collect::<Vec<_>>().join(", ")
                )
            });
            format!("solve({}){}", expr_text(value), wrt_text)
        }
        ExprKind::Optimize {
            value,
            wrt,
            maximize,
        } => {
            let kw = if *maximize { "maximize" } else { "minimize" };
            let wrt_text = wrt.as_ref().map_or_else(String::new, |w| {
                format!(
                    " wrt {}",
                    w.iter().map(expr_text).collect::<Vec<_>>().join(", ")
                )
            });
            format!("{kw}({}){}", expr_text(value), wrt_text)
        }
        ExprKind::At { value, location } => {
            format!("{} at {}", expr_text(value), expr_text(location))
        }
        ExprKind::On { value, location } => {
            format!("{} on {}", expr_text(value), expr_text(location))
        }
        ExprKind::Conditioned { value, condition } => {
            format!("{} if {}", expr_text(value), expr_text(condition))
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => format!(
            "if {} then {} else {}",
            expr_text(condition),
            expr_text(then_value),
            expr_text(else_value)
        ),
        ExprKind::Binder { kind, .. } => format!("binder({kind:?})"),
        ExprKind::UnitQuery { kind, expr } => {
            let kw = match kind {
                emath_core::tree::UnitQueryKind::Unit => "unit of",
                emath_core::tree::UnitQueryKind::Dimension => "dimension of",
            };
            format!("{} {}", kw, expr_text(expr))
        }
        ExprKind::Limit {
            var,
            target,
            direction,
            body,
        } => {
            let dir = match direction {
                emath_core::tree::LimitDirection::TwoSided => "",
                emath_core::tree::LimitDirection::FromAbove => "+",
                emath_core::tree::LimitDirection::FromBelow => "-",
            };
            format!(
                "limit {var} -> {}{dir}: {}",
                expr_text(target),
                expr_text(body)
            )
        }
        ExprKind::SampleLimit {
            var,
            target,
            direction,
            body,
        } => {
            let dir = match direction {
                emath_core::tree::LimitDirection::TwoSided => "",
                emath_core::tree::LimitDirection::FromAbove => "+",
                emath_core::tree::LimitDirection::FromBelow => "-",
            };
            format!(
                "sample_limit {var} -> {}{dir}: {}",
                expr_text(target),
                expr_text(body)
            )
        }
        ExprKind::Cases {
            subject,
            arms,
            else_arm,
        } => {
            let subj = subject
                .as_ref()
                .map_or_else(String::new, |s| format!("{}: ", expr_text(s)));
            let arms_text = arms
                .iter()
                .map(|(c, v)| format!("| {} => {}", expr_text(c), expr_text(v)))
                .collect::<Vec<_>>()
                .join(" ");
            format!("cases {subj}{arms_text} | else => {}", expr_text(else_arm))
        }
    }
}

/// Compact deterministic rendering of a type for trace text.
#[must_use]
pub fn type_text(ty: &TypeExpr) -> String {
    match &ty.kind {
        TypeKind::Path {
            segments,
            generic_args,
        } => {
            let mut out = segments.join("::");
            if !generic_args.is_empty() {
                out.push('<');
                out.push_str(
                    &generic_args
                        .iter()
                        .map(generic_arg_text)
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                out.push('>');
            }
            out
        }
        TypeKind::List(items) => format!(
            "[{}]",
            items.iter().map(type_text).collect::<Vec<_>>().join(", ")
        ),
        TypeKind::Tuple(items) => format!(
            "({})",
            items.iter().map(type_text).collect::<Vec<_>>().join(", ")
        ),
        TypeKind::Ref(inner) => format!("&{}", type_text(inner)),
        TypeKind::Product { left, op, right } => {
            format!("{}{}{}", type_text(left), op.as_str(), type_text(right))
        }
        TypeKind::Pow { base, exponent } => {
            if matches!(base.kind, TypeKind::Product { .. }) {
                format!("({})^{exponent}", type_text(base))
            } else {
                format!("{}^{exponent}", type_text(base))
            }
        }
        TypeKind::In { base, unit } => format!("{} in {}", type_text(base), type_text(unit)),
        TypeKind::Domain { base, lo, hi } => {
            format!(
                "{} in [{}, {}]",
                type_text(base),
                expr_text(lo),
                expr_text(hi)
            )
        }
    }
}

pub fn generic_arg_text(arg: &emath_core::tree::GenericArg) -> String {
    match arg {
        emath_core::tree::GenericArg::Type(ty) => type_text(ty),
        emath_core::tree::GenericArg::Value(expr) => expr_text(expr),
        emath_core::tree::GenericArg::Named { name, arg } => {
            format!("{name} = {}", generic_arg_text(arg))
        }
    }
}
