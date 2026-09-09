//! Generic call lowering.
//!
//! Executable mathematical calls resolve through capsule-installed FeatureID
//! bindings. The only direct call families retained here are declared sibling
//! functions and the language's structural Option/Result carriers.

use emath_core::tree::{Expr, ExprKind};
use emath_ir::{CapabilityId, ExprId, ExprNode, Extent, Literal};

use super::super::infer::*;
use super::super::{E_UNKNOWN_FUNCTION, E_UNSUPPORTED_TYPE};
use super::{capability_input_admits, capability_result_infer};

mod carriers;

use carriers::carrier_arity;

/// The universal unary operator surface: fixed Float64 function
/// spellings with a universal machine op (`emath_ir::UnaryOp`). `not`
/// and `neg` are operator spellings, not call forms, and are excluded.
fn operator_leaf(name: &str) -> &str {
    name.rsplit("::").next().unwrap_or(name)
}

fn universal_unary_op(name: &str) -> Option<emath_ir::UnaryOp> {
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
fn universal_binary_op(name: &str) -> Option<emath_ir::BinaryOp> {
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

fn last_input_is_sequence(inputs: &[String]) -> bool {
    inputs
        .last()
        .is_some_and(|input| input.trim().starts_with("Sequence"))
}

fn capability_call_bounds(arity: Option<usize>, inputs: &[String]) -> (usize, usize) {
    if last_input_is_sequence(inputs) {
        let prefix = inputs.len().saturating_sub(1);
        return (prefix.saturating_add(1), usize::MAX);
    }
    if let Some(exact) = arity {
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
        "SameTensor<Float64>" => matches!(infer, Infer::Tensor { .. } | Infer::HostDeferred),
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
                if universal_unary_op(&name).is_none() && universal_binary_op(&name).is_none() {
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
            } else {
                let mut arguments = Vec::with_capacity(args.len());
                let mut inferred = Vec::with_capacity(args.len());
                for (index, argument) in args.iter().enumerate() {
                    let (argument_id, infer) = if binding.inputs.get(index).is_some_and(|input| input == "Program") {
                        self.lower_program_argument(argument)?
                    } else {
                        self.lower_expr(argument)?
                    };
                    arguments.push(argument_id);
                    inferred.push(infer);
                }
                if last_input_is_sequence(&binding.inputs)
                    && arguments.len() >= binding.inputs.len()
                {
                    let prefix = binding.inputs.len() - 1;
                    let rest = arguments.split_off(prefix);
                    let guards = vec![None; rest.len()];
                    let packed = self.push_expr(
                        ExprNode::Set {
                            elements: rest,
                            guards,
                        },
                        expr.source,
                    );
                    arguments.push(packed);
                    inferred.truncate(prefix);
                    inferred.push(Infer::Set(Box::new(Infer::Opaque)));
                }
                for (index, input) in binding.inputs.iter().enumerate() {
                    let declared = input.trim().strip_suffix('?').unwrap_or(input.trim());
                    let same_shape = match declared {
                        "SameMatrix<Float64>" => match (inferred.first(), inferred.get(index)) {
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
                        },
                        "SameTensor<Float64>" => match (inferred.first(), inferred.get(index)) {
                            (
                                Some(Infer::Tensor { shape: expected }),
                                Some(Infer::Tensor { shape }),
                            ) => expected == shape,
                            (
                                Some(Infer::HostDeferred),
                                Some(Infer::Tensor { .. } | Infer::HostDeferred),
                            ) => true,
                            _ => false,
                        },
                        _ => continue,
                    };
                    if !same_shape {
                        self.error(
                            "E-SHAPE-005",
                            format!("`{name}` field arguments must have equal shapes"),
                            args[index].source,
                        );
                        return None;
                    }
                }
                let types_ok = binding.inputs.is_empty()
                    || !binding.inputs.iter().zip(inferred.iter().zip(args)).any(
                        |(input, (infer, argument))| {
                            !declared_capability_input_admits(input, infer, argument)
                        },
                    );
                let types_ok = types_ok || {
                    const DOMAIN_DIAGNOSTIC: &str = "factorial-domain";
                    binding.diagnostic.as_deref() == Some(DOMAIN_DIAGNOSTIC)
                        && inferred.iter().all(is_scalar_numeric)
                };
                if !types_ok {
                    if universal_unary_op(&name).is_none() && universal_binary_op(&name).is_none() {
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
                } else {
                    let id = self.push_expr(
                        ExprNode::Apply {
                            capability: CapabilityId(binding.capability),
                            arguments,
                        },
                        expr.source,
                    );
                    let result = match binding.output.as_deref() {
                        Some("ExactInt")
                            if inferred.iter().any(|infer| *infer == Infer::BigInt) =>
                        {
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
                        Some("SameTensor<Float64>") => match inferred.first() {
                            Some(Infer::Tensor { shape }) => Infer::Tensor {
                                shape: shape.clone(),
                            },
                            _ => Infer::HostDeferred,
                        },
                        _ => capability_result_infer(binding.output.as_deref()),
                    };
                    return Some((id, result));
                }
            }
        }
        if self.sibling_functions.contains_key(&name) {
            return self.lower_sibling_call(&name, args, expr.source);
        }
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
                "unknown function `{name}`: no declared function or executable FeatureID alias exists in the loaded Language Image"
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
