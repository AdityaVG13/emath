//! Generic call lowering.
//!
//! Executable mathematical calls resolve through capsule-installed FeatureID
//! bindings. The only direct call families retained here are declared sibling
//! functions and the language's structural Option/Result carriers.

use emath_core::tree::{Expr, ExprKind};
use emath_ir::{CapabilityId, ExprId, ExprNode};

use super::super::infer::*;
use super::super::{E_UNKNOWN_FUNCTION, E_UNSUPPORTED_TYPE};
use super::{capability_input_admits, capability_result_infer};

mod carriers;

use carriers::carrier_arity;

/// The universal unary operator surface: fixed Float64 function
/// spellings with a universal machine op (`emath_ir::UnaryOp`). `not`
/// and `neg` are operator spellings, not call forms, and are excluded.
fn universal_unary_op(name: &str) -> Option<emath_ir::UnaryOp> {
    match name {
        "sqrt" => Some(emath_ir::UnaryOp::Sqrt),
        "exp" => Some(emath_ir::UnaryOp::Exp),
        "ln" => Some(emath_ir::UnaryOp::Log),
        "sin" => Some(emath_ir::UnaryOp::Sin),
        "cos" => Some(emath_ir::UnaryOp::Cos),
        "tan" => Some(emath_ir::UnaryOp::Tan),
        "tanh" => Some(emath_ir::UnaryOp::Tanh),
        "abs" => Some(emath_ir::UnaryOp::Abs),
        "floor" => Some(emath_ir::UnaryOp::Floor),
        "ceil" => Some(emath_ir::UnaryOp::Ceil),
        _ => None,
    }
}

/// The universal binary operator surface: fixed Float64/vector function
/// spellings with a universal machine op (`emath_ir::BinaryOp`).
fn universal_binary_op(name: &str) -> Option<emath_ir::BinaryOp> {
    match name {
        "min" => Some(emath_ir::BinaryOp::Min),
        "max" => Some(emath_ir::BinaryOp::Max),
        "atan2" => Some(emath_ir::BinaryOp::Atan2),
        "dot" => Some(emath_ir::BinaryOp::VectorDot),
        _ => None,
    }
}

fn capability_call_bounds(arity: Option<usize>, inputs: &[String]) -> (usize, usize) {    if let Some(exact) = arity {
        return (exact, exact);
    }
    if inputs.is_empty() {
        return (0, usize::MAX);
    }
    let optional = inputs
        .iter()
        .rev()
        .take_while(|input| input.trim().ends_with('?'))
        .count();
    (inputs.len().saturating_sub(optional), inputs.len())
}

fn declared_capability_input_admits(input: &str, infer: &Infer, expr: &Expr) -> bool {
    let input = input.trim().strip_suffix('?').unwrap_or(input.trim());
    match input {
        "Text" => matches!(infer, Infer::Text),
        "Scalar" => matches!(
            infer,
            Infer::F64 | Infer::Nat | Infer::Int | Infer::HostDeferred
        ),
        "LiteralFloat64" => matches!(infer, Infer::F64) && matches!(&expr.kind, ExprKind::Float(_)),
        "PositiveLiteralFloat64" => {
            matches!(infer, Infer::F64)
                && matches!(
                    &expr.kind,
                    ExprKind::Float(text)
                        if text.replace('_', "").parse::<f64>().is_ok_and(
                            |value| value.is_finite() && value > 0.0
                        )
                )
        }
        "SameMatrix<Float64>" => matches!(infer, Infer::Matrix { .. } | Infer::HostDeferred),
        _ => capability_input_admits(input, infer),
    }
}

impl super::super::Admitter {
    pub(super) fn lower_call_expr_arm(&mut self, expr: &Expr) -> Option<(ExprId, Infer)> {
        let ExprKind::Call { function, args } = &expr.kind else {
            unreachable!()
        };
        let ExprKind::Path { segments, .. } = &function.kind else {
            self.error(
                E_UNSUPPORTED_TYPE,
                "callable must be a plain path in the Phase 1 subset",
                function.source,
            );
            return None;
        };
        let name = segments.join("::");
        let dotted = name.contains("::").then(|| name.replace("::", "."));
        if let Some(binding) = self
            .capability_cells
            .iter()
            .find(|binding| binding.key == name || dotted.as_deref() == Some(binding.key.as_str()))
            .cloned()
        {
            let (min_arity, max_arity) = capability_call_bounds(binding.arity, &binding.inputs);
            if !(min_arity..=max_arity).contains(&args.len()) {
                self.error(
                    "E-TYPE-012",
                    format!(
                        "`{name}` expects {} argument(s), found {}",
                        if min_arity == max_arity {
                            min_arity.to_string()
                        } else {
                            format!("{min_arity}..{max_arity}")
                        },
                        args.len()
                    ),
                    expr.source,
                );
                return None;
            }
            let mut arguments = Vec::with_capacity(args.len());
            let mut inferred = Vec::with_capacity(args.len());
            for argument in args {
                let (argument_id, infer) = self.lower_expr(argument)?;
                arguments.push(argument_id);
                inferred.push(infer);
            }
            for (index, input) in binding.inputs.iter().enumerate() {
                if input.trim().strip_suffix('?').unwrap_or(input.trim()) != "SameMatrix<Float64>" {
                    continue;
                }
                let same_shape = match (inferred.first(), inferred.get(index)) {
                    (
                        Some(Infer::Matrix {
                            rows: expected_rows,
                            cols: expected_cols,
                        }),
                        Some(Infer::Matrix { rows, cols }),
                    ) => expected_rows == rows && expected_cols == cols,
                    (
                        Some(Infer::HostDeferred),
                        Some(Infer::Matrix { .. } | Infer::HostDeferred),
                    ) => true,
                    _ => false,
                };
                if !same_shape {
                    self.error(
                        "E-SHAPE-005",
                        format!("`{name}` matrix field arguments must have equal shapes"),
                        args[index].source,
                    );
                    return None;
                }
            }
            if !binding.inputs.is_empty()
                && binding.inputs.iter().zip(inferred.iter().zip(args)).any(
                    |(input, (infer, argument))| {
                        !declared_capability_input_admits(input, infer, argument)
                    },
                )
            {
                self.error(
                    "E-LANG-FEATURE",
                    format!(
                        "{}: `{name}` requires ({}) and refuses the supplied argument types",
                        binding.diagnostic.as_deref().unwrap_or("type-mismatch"),
                        binding.inputs.join(", ")
                    ),
                    expr.source,
                );
                return None;
            }
            let id = self.push_expr(
                ExprNode::Apply {
                    capability: CapabilityId(binding.capability),
                    arguments,
                },
                expr.source,
            );
            let result = match binding.output.as_deref() {
                Some("ExactInt") if inferred.iter().any(|infer| *infer == Infer::BigInt) => {
                    Infer::BigInt
                }
                Some("SameVector<Float64>") => match inferred.first() {
                    Some(Infer::Vector { extent, .. }) => Infer::Vector {
                        extent: extent.clone(),
                        element: None,
                    },
                    _ => Infer::HostDeferred,
                },
                Some("SameMatrix<Float64>") => match inferred.first() {
                    Some(Infer::Matrix { rows, cols }) => Infer::Matrix {
                        rows: rows.clone(),
                        cols: cols.clone(),
                    },
                    _ => Infer::HostDeferred,
                },
                _ => capability_result_infer(binding.output.as_deref()),
            };
            return Some((id, result));
        }
        if self.sibling_functions.contains_key(&name) {
            return self.lower_sibling_call(&name, args, expr.source);
        }
        // Universal operator surface: fixed scalar/vector function
        // spellings that belong to the language's operator layer, exactly
        // like `+` or `<`. They lower to the universal Unary/Binary SIR
        // forms — the same machine ops the parser's operator spellings
        // produce — so no named call ever reaches executable lowering and
        // no domain dispatch, registry, or capsule authority is involved.
        if let Some(operation) = universal_unary_op(&name) {
            if args.len() != 1 {
                self.error(
                    "E-TYPE-012",
                    format!("`{name}` expects 1 argument, found {}", args.len()),
                    expr.source,
                );
                return None;
            }
            let (value_id, value_infer) = self.lower_expr(&args[0])?;
            if !matches!(value_infer, Infer::F64 | Infer::HostDeferred) {
                self.error(
                    "E-TYPE-012",
                    format!("`{name}` argument must be Float64"),
                    args[0].source,
                );
                return None;
            }
            let id = self.push_expr(
                ExprNode::Unary {
                    operation,
                    value: value_id,
                },
                expr.source,
            );
            return Some((id, Infer::F64));
        }
        if let Some(operation) = universal_binary_op(&name) {
            if args.len() != 2 {
                self.error(
                    "E-TYPE-012",
                    format!("`{name}` expects 2 arguments, found {}", args.len()),
                    expr.source,
                );
                return None;
            }
            let (left_id, left_infer) = self.lower_expr(&args[0])?;
            let (right_id, right_infer) = self.lower_expr(&args[1])?;
            let result = match operation {
                emath_ir::BinaryOp::VectorDot => {
                    if !matches!(
                        left_infer,
                        Infer::Vector { .. } | Infer::HostDeferred
                    ) || !matches!(
                        right_infer,
                        Infer::Vector { .. } | Infer::HostDeferred
                    ) {
                        self.error(
                            "E-TYPE-012",
                            format!("`{name}` arguments must be vectors"),
                            expr.source,
                        );
                        return None;
                    }
                    Infer::F64
                }
                _ => {
                    if !matches!(left_infer, Infer::F64 | Infer::HostDeferred)
                        || !matches!(right_infer, Infer::F64 | Infer::HostDeferred)
                    {
                        self.error(
                            "E-TYPE-012",
                            format!("`{name}` arguments must be Float64"),
                            expr.source,
                        );
                        return None;
                    }
                    Infer::F64
                }
            };
            let id = self.push_expr(
                ExprNode::Binary {
                    operation,
                    left: left_id,
                    right: right_id,
                },
                expr.source,
            );
            return Some((id, result));
        }
        if let Some(arity) = carrier_arity(&name) {
            if args.len() != arity {
                self.error(
                    "E-TYPE-012",
                    format!("`{name}` expects {arity} argument(s), found {}", args.len()),
                    expr.source,
                );
                return None;
            }
            return self.lower_call_carriers(&name, expr, args);
        }
        if name.starts_with("std::") {
            self.error(
                "E-LANG-FEATURE",
                format!(
                    "FeatureID `{}` is not executable in the loaded Language Image",
                    name.replace("::", ".")
                ),
                expr.source,
            );
            return None;
        }
        self.error(
            E_UNKNOWN_FUNCTION,
            format!(
                "unknown function `{name}`: no declared function or executable FeatureID alias exists in the loaded Language Image"
            ),
            function.source,
        );
        None
    }
}
