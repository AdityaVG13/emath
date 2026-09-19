//! Generic call lowering.
//!
//! Executable mathematical calls resolve through capsule-installed FeatureID
//! bindings; the language's structural Option/Result carriers lower directly.

use emath_core::tree::{Expr, ExprKind};
use emath_ir::{ExprId, ExprNode, Extent, Literal};

use super::super::infer::*;
use super::super::{E_UNKNOWN_FUNCTION, E_UNSUPPORTED_TYPE};

mod carriers;

use carriers::carrier_arity;

/// The universal unary operator surface: fixed Float64 function
/// spellings with a universal machine op (`emath_ir::UnaryOp`). `not`
/// and `neg` are operator spellings, not call forms, and are excluded.
fn operator_leaf(name: &str) -> &str {
    name.rsplit("::").next().unwrap_or(name)
}

fn universal_unary_op(_name: &str) -> Option<emath_ir::UnaryOp> {
    None
}

#[allow(dead_code)]
fn leftover_universal_unary_op(name: &str) -> Option<emath_ir::UnaryOp> {
    let leaf = operator_leaf(name);
    const ROWS: &[(&str, emath_ir::UnaryOp)] = &[
        ("sqrt", emath_ir::UnaryOp::Sqrt),
        ("exp", emath_ir::UnaryOp::Exp),
        ("ln", emath_ir::UnaryOp::Log),
        ("sin", emath_ir::UnaryOp::Sin),
        ("cos", emath_ir::UnaryOp::Cos),
        ("tan", emath_ir::UnaryOp::Tan),
        ("tanh", emath_ir::UnaryOp::Tanh),
        ("abs", emath_ir::UnaryOp::Abs),
        ("floor", emath_ir::UnaryOp::Floor),
        ("ceil", emath_ir::UnaryOp::Ceil),
        ("sign", emath_ir::UnaryOp::Sign),
        ("cbrt", emath_ir::UnaryOp::Cbrt),
        ("recip", emath_ir::UnaryOp::Recip),
        ("log", emath_ir::UnaryOp::Log),
        ("log2", emath_ir::UnaryOp::Log2),
        ("log10", emath_ir::UnaryOp::Log10),
        ("is_finite", emath_ir::UnaryOp::IsFinite),
        ("neg", emath_ir::UnaryOp::Negate),
    ];
    ROWS.iter()
        .find(|(spelling, _)| *spelling == leaf)
        .map(|(_, operation)| *operation)
}

/// The universal binary operator surface: fixed Float64/vector function
/// spellings with a universal machine op (`emath_ir::BinaryOp`).
fn universal_binary_op(_name: &str) -> Option<emath_ir::BinaryOp> {
    None
}

#[allow(dead_code)]
fn leftover_universal_binary_op(name: &str) -> Option<emath_ir::BinaryOp> {
    let leaf = operator_leaf(name);
    const ROWS: &[(&str, emath_ir::BinaryOp)] = &[
        ("min", emath_ir::BinaryOp::Min),
        ("max", emath_ir::BinaryOp::Max),
        ("atan2", emath_ir::BinaryOp::Atan2),
        ("hypot", emath_ir::BinaryOp::Hypot),
        ("pow", emath_ir::BinaryOp::StrictFloatPow),
        ("mod", emath_ir::BinaryOp::Mod),
        ("add", emath_ir::BinaryOp::StrictFloatAdd),
        ("sub", emath_ir::BinaryOp::StrictFloatSub),
        ("mul", emath_ir::BinaryOp::StrictFloatMul),
        ("div", emath_ir::BinaryOp::StrictFloatDiv),
    ];
    ROWS.iter()
        .find(|(spelling, _)| *spelling == leaf)
        .map(|(_, operation)| *operation)
}

fn is_scalar_numeric(infer: &Infer) -> bool {
    matches!(
        infer,
        Infer::F64 | Infer::Nat | Infer::Int | Infer::Complex | Infer::HostDeferred
    )
}

fn known_length(infer: &Infer) -> Option<usize> {
    match infer {
        Infer::Vector {
            extent: Some(Extent::Fixed(n)),
            ..
        } => Some(*n),
        Infer::Matrix {
            rows: Some(Extent::Fixed(rows)),
            cols: Some(Extent::Fixed(cols)),
        } => Some(*rows * *cols),
        _ => None,
    }
}

fn known_element_indices(infer: &Infer) -> Option<Vec<Vec<usize>>> {
    match infer {
        Infer::Vector {
            extent: Some(Extent::Fixed(n)),
            ..
        } if *n > 0 => Some((0..*n).map(|index| vec![index]).collect()),
        Infer::Matrix {
            rows: Some(Extent::Fixed(rows)),
            cols: Some(Extent::Fixed(cols)),
        } if *rows > 0 && *cols > 0 => Some(
            (0..*rows)
                .flat_map(|row| (0..*cols).map(move |col| vec![row, col]))
                .collect(),
        ),
        _ => None,
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
                "callable must be a plain path in the current subset",
                function.source,
            );
            return None;
        };
        let name = segments.join("::");
        // Carrier cardinality is a universal machine op, not a FeatureID.
        if operator_leaf(&name) == "length" {
            return self.lower_length_call(args, expr);
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
            if let Some(indices) = known_element_indices(&value_infer) {
                if !matches!(operation, emath_ir::UnaryOp::Abs) {
                    self.error(
                        "E-TYPE-012",
                        format!("`{name}` argument must be Float64"),
                        args[0].source,
                    );
                    return None;
                }
                let mapped = self.index_elements(value_id, &indices, expr.source);
                let mut elements = Vec::with_capacity(mapped.len());
                for element in mapped {
                    elements.push(self.push_expr(
                        ExprNode::Unary {
                            operation,
                            value: element,
                        },
                        expr.source,
                    ));
                }
                let id = self.push_expr(ExprNode::Vector(elements), expr.source);
                return Some((id, value_infer));
            }
            if !is_scalar_numeric(&value_infer) {
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
            let result = if matches!(operation, emath_ir::UnaryOp::IsFinite) {
                Infer::Bool
            } else {
                Infer::F64
            };
            return Some((id, result));
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
            if !is_scalar_numeric(&left_infer) || !is_scalar_numeric(&right_infer) {
                self.error(
                    "E-TYPE-012",
                    format!("`{name}` arguments must be Float64"),
                    expr.source,
                );
                return None;
            }
            let result = Infer::F64;
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
        if operator_leaf(&name) == "series_from_csv" {
            return self.lower_series_from_csv(args, expr);
        }
        if operator_leaf(&name) == "series_at" {
            return self.lower_series_at(args, expr);
        }
        if operator_leaf(&name) == "grad" {
            if args.len() != 1 {
                self.error(
                    "E-TYPE-012",
                    format!("`{name}` expects 1 argument, found {}", args.len()),
                    expr.source,
                );
                return None;
            }
            let (body_id, body_infer) = self.lower_expr(&args[0])?;
            if !is_scalar_numeric(&body_infer) {
                self.error(
                    "E-TYPE-012",
                    "`grad` expects a scalar numeric expression",
                    args[0].source,
                );
                return None;
            }
            let n = self.inputs.len();
            if n == 0 {
                self.error(
                    "E-TYPE-012",
                    "`grad` requires at least one input to differentiate",
                    expr.source,
                );
                return None;
            }
            let inlined = self.inline_defs(body_id);
            let program_inputs = self.program_input_names();
            let slot_ids: Vec<_> = (0..n)
                .map(|slot| self.push_f64(slot as f64, expr.source))
                .collect();
            let extra = vec![self.push_expr(ExprNode::Vector(slot_ids), expr.source)];
            if let Some((id, _)) = self.apply_program_kernel(
                "program-reverse-gradient",
                inlined,
                program_inputs,
                extra,
                expr.source,
            ) {
                return Some((
                    id,
                    Infer::Vector {
                        extent: Some(emath_ir::Extent::Fixed(n)),
                        element: None,
                    },
                ));
            }
            let id = self.push_expr(
                ExprNode::Call {
                    function: emath_core::QualifiedName(name.clone()),
                    arguments: vec![body_id],
                },
                expr.source,
            );
            return Some((
                id,
                Infer::Vector {
                    extent: Some(emath_ir::Extent::Fixed(n)),
                    element: None,
                },
            ));
        }
        if operator_leaf(&name) == "not" {
            if args.len() != 1 {
                self.error(
                    "E-TYPE-012",
                    format!("`{name}` expects 1 argument, found {}", args.len()),
                    expr.source,
                );
                return None;
            }
            let (value_id, value_infer) = self.lower_expr(&args[0])?;
            if !matches!(value_infer, Infer::Bool | Infer::HostDeferred) {
                self.error("E-TYPE-012", "`not` expects a Bool", args[0].source);
                return None;
            }
            let id = self.push_expr(
                ExprNode::Unary {
                    operation: emath_ir::UnaryOp::Not,
                    value: value_id,
                },
                expr.source,
            );
            return Some((id, Infer::Bool));
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
                "unknown function `{name}`: no declared function exists in the loaded constructor image or imported module"
            ),
            function.source,
        );
        None
    }
    fn expect_arity(&mut self, name: &str, args: &[Expr], expected: usize, expr: &Expr) -> bool {
        if args.len() == expected {
            true
        } else {
            self.error(
                "E-TYPE-012",
                format!(
                    "`{name}` expects {expected} argument(s), found {}",
                    args.len()
                ),
                expr.source,
            );
            false
        }
    }

    fn index_elements(
        &mut self,
        value: ExprId,
        indices: &[Vec<usize>],
        source: emath_core::Span,
    ) -> Vec<ExprId> {
        indices
            .iter()
            .map(|coords| {
                let index_ids = coords
                    .iter()
                    .map(|index| {
                        self.push_expr(
                            ExprNode::Literal(Literal::Integer(index.to_string())),
                            source,
                        )
                    })
                    .collect();
                self.push_expr(
                    ExprNode::Index {
                        value,
                        indices: index_ids,
                    },
                    source,
                )
            })
            .collect()
    }

    fn lower_length_call(&mut self, args: &[Expr], expr: &Expr) -> Option<(ExprId, Infer)> {
        if !self.expect_arity("length", args, 1, expr) {
            return None;
        }
        let (value_id, infer) = self.lower_expr(&args[0])?;
        if let Some(n) = known_length(&infer) {
            let id = self.push_expr(
                ExprNode::Literal(Literal::Integer(n.to_string())),
                expr.source,
            );
            return Some((id, Infer::Nat));
        }
        if !matches!(
            infer,
            Infer::Vector { .. }
                | Infer::Matrix { .. }
                | Infer::Tensor { .. }
                | Infer::HostDeferred
        ) {
            self.error(
                "E-TYPE-012",
                "`length` requires a vector or matrix with a known fixed extent",
                args[0].source,
            );
            return None;
        }
        let id = self.push_expr(
            ExprNode::Unary {
                operation: emath_ir::UnaryOp::Length,
                value: value_id,
            },
            expr.source,
        );
        Some((id, Infer::Nat))
    }
}
