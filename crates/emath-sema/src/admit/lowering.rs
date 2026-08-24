//! Expression lowering: lowers parsed `.emath` expressions into typed
//! EMIR expression nodes with stable inference.

use emath_core::tree::{
    BinaryOp as SynBinOp, Expr, ExprKind,
    UnaryOp as SynUnOp,
};
use emath_core::QualifiedName;
use emath_ir::{ExprId, ExprNode, Extent, Literal, lookup_unit};

mod helpers;

use super::Admitter;
use super::equations::*;
use super::expr_helpers::*;
use super::infer::*;
use super::{
    E_UNKNOWN_FUNCTION, E_UNKNOWN_VARIABLE, E_UNSUPPORTED_TYPE,
};

impl super::Admitter {
    pub(super) fn lower_expr(&mut self, expr: &Expr) -> Option<(ExprId, Infer)> {
        match &expr.kind {
            ExprKind::Int(text) => {
                let id = self.push_expr(
                    ExprNode::Literal(Literal::Integer(text.clone())),
                    expr.source,
                );
                let infer = if text.starts_with('-') {
                    Infer::Int
                } else {
                    Infer::Nat
                };
                Some((id, infer))
            }
            ExprKind::Float(text) => {
                let value = parse_float_constant(text);
                match value {
                    Some(value) if value.is_finite() => {
                        self.record(
                            "sema",
                            format!("constant `{text}` → strict f64"),
                            expr.source,
                        );
                        let id = self.push_expr(
                            ExprNode::Literal(Literal::FloatBits(value.to_bits())),
                            expr.source,
                        );
                        Some((id, Infer::F64))
                    }
                    _ => {
                        self.error(
                            "E-TYPE-011",
                            format!("non-finite constant `{text}` refused under strict-f64 policy"),
                            expr.source,
                        );
                        None
                    }
                }
            }
            ExprKind::Bool(value) => {
                let id = self.push_expr(ExprNode::Literal(Literal::Bool(*value)), expr.source);
                Some((id, Infer::Bool))
            }
            ExprKind::Str(_) => {
                self.error(
                    E_UNSUPPORTED_TYPE,
                    "string values are outside the Phase 1 subset",
                    expr.source,
                );
                None
            }
            ExprKind::Quantity { value, unit } => {
                let name = unit.last().map_or("", String::as_str);
                match lookup_unit(name) {
                    Ok(looked_up) => {
                        let inner = match &value.kind {
                            ExprKind::Int(text) | ExprKind::Float(text) => text.as_str(),
                            _ => {
                                self.error(
                                    "E-UNIT-105",
                                    "quantity value must be a numeric literal",
                                    expr.source,
                                );
                                return None;
                            }
                        };
                        let parsed = parse_float_constant(inner);
                        match parsed {
                            Some(number) if number.is_finite() => {
                                self.record(
                                    "sema",
                                    format!("quantity `{inner} {name}` → {}", looked_up.name),
                                    expr.source,
                                );
                                let id = self.push_expr(
                                    ExprNode::Literal(Literal::FloatBits(number.to_bits())),
                                    expr.source,
                                );
                                Some((id, Infer::from_unit(&looked_up)))
                            }
                            _ => {
                                self.error(
                                    "E-TYPE-011",
                                    format!(
                                        "non-finite quantity `{inner} {name}` refused under the selected numeric model"
                                    ),
                                    expr.source,
                                );
                                None
                            }
                        }
                    }
                    Err(error) => {
                        self.error(error.code, error.message, expr.source);
                        None
                    }
                }
            }
            ExprKind::Path { segments, .. } => {
                let name = segments.join(".");
                if segments.len() == 1 {
                    if let Some(value) = self.index_locals.get(&name).copied() {
                        let id = self.push_expr(
                            ExprNode::Literal(Literal::Integer(value.to_string())),
                            expr.source,
                        );
                        let infer = if value < 0 { Infer::Int } else { Infer::Nat };
                        return Some((id, infer));
                    }
                }
                if let Some(infer) = self.lookup(&name) {
                    let ir_name = state_variable_name(self, segments, &name);
                    let id =
                        self.push_expr(ExprNode::Variable(QualifiedName(ir_name)), expr.source);
                    return Some((id, infer));
                }
                if segments.len() >= 2 {
                    if matches!(self.lookup(&segments[0]), Some(Infer::Opaque)) {
                        self.record(
                            "sema",
                            format!("host field `{name}` deferred to the host boundary"),
                            expr.source,
                        );
                        let id =
                            self.push_expr(ExprNode::Variable(QualifiedName(name)), expr.source);
                        return Some((id, Infer::HostDeferred));
                    }
                }
                if segments.len() == 1 {
                    if let Ok(unit) = lookup_unit(&segments[0]) {
                        self.record(
                            "sema",
                            format!("unit literal `{}` → {}", segments[0], unit.name),
                            expr.source,
                        );
                        let id = self.push_expr(
                            ExprNode::Literal(Literal::FloatBits(1.0_f64.to_bits())),
                            expr.source,
                        );
                        return Some((id, Infer::from_unit(&unit)));
                    }
                }
                self.error(
                    E_UNKNOWN_VARIABLE,
                    format!("unknown variable `{name}`"),
                    expr.source,
                );
                None
            }
            ExprKind::Call { function, args } => {
                let ExprKind::Path { segments, .. } = &function.kind else {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "callable must be a plain path in the Phase 1 subset",
                        function.source,
                    );
                    return None;
                };
                let name = segments.join(".");
                if matches!(name.as_str(), "sum" | "product") {
                    if args.len() != 1 {
                        self.error(
                            "E-TYPE-012",
                            format!("`{name}` expects 1 argument, found {}", args.len()),
                            expr.source,
                        );
                        return None;
                    }
                    return self.lower_reduction(expr, &name, &args[0]);
                }
                let arity: Option<usize> = match name.as_str() {
                    "is_finite" | "exp" | "ln" | "log" | "sqrt" | "sin" | "cos" | "tan"
                    | "tanh" | "abs" | "floor" | "ceil" | "round" | "sign" | "log2" | "log10" | "sinh" | "cosh" | "atan" | "cbrt" | "recip" | "fract"
                    | "norm" | "transpose" | "length" | "len" | "mean" => Some(1),
                    "min" | "max" | "atan2" | "pow" | "mod" | "hypot" | "dot" | "laplacian" | "laplacian_neumann" | "laplacian_2d" | "laplacian_2d_neumann" | "gradient" | "gradient_2d_x" | "gradient_2d_y" => Some(2),
                    "lerp" | "clamp" => Some(3),
                    "laplacian_dirichlet" => Some(4),
                    "einsum" => {
                        // einsum(subscripts, tensor1, ...) — variable arity, min 2.
                        if args.len() < 2 {
                            self.error(
                                "E-TYPE-012",
                                "`einsum` expects at least 2 arguments (subscripts + tensors)",
                                expr.source,
                            );
                            return None;
                        }
                        // First arg must be a string literal.
                        if !matches!(&args[0].kind, ExprKind::Str(_)) {
                            self.error(
                                "E-TYPE-012",
                                "`einsum` first argument must be a string literal",
                                args[0].source,
                            );
                            return None;
                        }
                        // Lower as Einsum op.
                        return self.lower_einsum(expr, &name, &args);
                    }
                    _ => {
                        self.error(
                            E_UNKNOWN_FUNCTION,
                            format!(
                                "unknown function `{name}` (Phase 1 builtins: exp, ln, log, sqrt, sin, cos, tan, tanh, abs, floor, ceil, round, sign, log2, log10, sinh, cosh, atan, cbrt, recip, fract, min, max, atan2, pow, mod, hypot, lerp, clamp, is_finite, norm, transpose, dot, length, sum, product, mean, laplacian, laplacian_neumann, laplacian_dirichlet, laplacian_2d, laplacian_2d_neumann, gradient, gradient_2d_x, gradient_2d_y, einsum)"
                            ),
                            function.source,
                        );
                        return None;
                    }
                };
                if arity != Some(args.len()) {
                    self.error(
                        "E-TYPE-012",
                        format!(
                            "`{name}` expects {arity:?} argument(s), found {}",
                            args.len()
                        ),
                        expr.source,
                    );
                    return None;
                }
                match name.as_str() {
                    "norm" => {
                        let (arg_id, arg_infer) = self.lower_expr(&args[0])?;
                        if !matches!(arg_infer, Infer::Vector { .. } | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`norm` expects a Vector argument",
                                args[0].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![arg_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::F64))
                    }
                    "laplacian" => {
                        let (vec_id, vec_infer) = self.lower_expr(&args[0])?;
                        let extent = match vec_infer {
                            Infer::Vector { extent } => extent,
                            Infer::HostDeferred => None,
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`laplacian` expects a Vector first argument",
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let (dx_id, dx_infer) = self.lower_expr(&args[1])?;
                        if !matches!(dx_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`laplacian` expects a Float64 cell width (dx) as the second argument",
                                args[1].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![vec_id, dx_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent }))
                    }
                    "laplacian_neumann" => {
                        let (vec_id, vec_infer) = self.lower_expr(&args[0])?;
                        let extent = match vec_infer {
                            Infer::Vector { extent } => extent,
                            Infer::HostDeferred => None,
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`laplacian_neumann` expects a Vector first argument",
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let (dx_id, dx_infer) = self.lower_expr(&args[1])?;
                        if !matches!(dx_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`laplacian_neumann` expects a Float64 cell width (dx) as the second argument",
                                args[1].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![vec_id, dx_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent }))
                    }
                    "laplacian_dirichlet" => {
                        let (vec_id, vec_infer) = self.lower_expr(&args[0])?;
                        let extent = match vec_infer {
                            Infer::Vector { extent } => extent,
                            Infer::HostDeferred => None,
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`laplacian_dirichlet` expects a Vector first argument",
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let (dx_id, dx_infer) = self.lower_expr(&args[1])?;
                        if !matches!(dx_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`laplacian_dirichlet` expects a Float64 cell width (dx) as the second argument",
                                args[1].source,
                            );
                            return None;
                        }
                        let (g_left_id, g_left_infer) = self.lower_expr(&args[2])?;
                        if !matches!(g_left_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`laplacian_dirichlet` expects a Float64 left boundary value as the third argument",
                                args[2].source,
                            );
                            return None;
                        }
                        let (g_right_id, g_right_infer) = self.lower_expr(&args[3])?;
                        if !matches!(g_right_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`laplacian_dirichlet` expects a Float64 right boundary value as the fourth argument",
                                args[3].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![vec_id, dx_id, g_left_id, g_right_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent }))
                    }
                    "laplacian_2d" | "laplacian_2d_neumann" => {
                        let (mat_id, mat_infer) = self.lower_expr(&args[0])?;
                        let (rows, cols) = match mat_infer {
                            Infer::Matrix { rows, cols } => (rows, cols),
                            Infer::HostDeferred => (None, None),
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    format!("`{name}` expects a Matrix first argument"),
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let (dx_id, dx_infer) = self.lower_expr(&args[1])?;
                        if !matches!(dx_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                format!("`{name}` expects a Float64 cell width (dx) as the second argument"),
                                args[1].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![mat_id, dx_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Matrix { rows, cols }))
                    }
                    "gradient" => {
                        let (vec_id, vec_infer) = self.lower_expr(&args[0])?;
                        let extent = match vec_infer {
                            Infer::Vector { extent } => extent,
                            Infer::HostDeferred => None,
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    format!("`{name}` expects a Vector first argument"),
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let (dx_id, dx_infer) = self.lower_expr(&args[1])?;
                        if !matches!(dx_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                format!("`{name}` expects a Float64 cell width (dx) as the second argument"),
                                args[1].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![vec_id, dx_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Vector { extent }))
                    }
                    "gradient_2d_x" | "gradient_2d_y" => {
                        let (mat_id, mat_infer) = self.lower_expr(&args[0])?;
                        let (rows, cols) = match mat_infer {
                            Infer::Matrix { rows, cols } => (rows, cols),
                            Infer::HostDeferred => (None, None),
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    format!("`{name}` expects a Matrix first argument"),
                                    args[0].source,
                                );
                                return None;
                            }
                        };
                        let (dx_id, dx_infer) = self.lower_expr(&args[1])?;
                        if !matches!(dx_infer, Infer::F64 | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                format!("`{name}` expects a Float64 cell width (dx) as the second argument"),
                                args[1].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![mat_id, dx_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::Matrix { rows, cols }))
                    }
                    "transpose" => {
                        let (arg_id, arg_infer) = self.lower_expr(&args[0])?;
                        match arg_infer {
                            Infer::Matrix { rows, cols } => {
                                let id = self.push_expr(
                                    ExprNode::Call {
                                        function: QualifiedName(name.clone()),
                                        arguments: vec![arg_id],
                                    },
                                    expr.source,
                                );
                                Some((id, Infer::Matrix { rows: cols, cols: rows }))
                            }
                            Infer::HostDeferred => {
                                let id = self.push_expr(
                                    ExprNode::Call {
                                        function: QualifiedName(name.clone()),
                                        arguments: vec![arg_id],
                                    },
                                    expr.source,
                                );
                                Some((id, Infer::Matrix { rows: None, cols: None }))
                            }
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`transpose` expects a Matrix argument",
                                    args[0].source,
                                );
                                None
                            }
                        }
                    }
                    "length" | "len" => {
                        let (arg_id, arg_infer) = self.lower_expr(&args[0])?;
                        if !matches!(arg_infer, Infer::Vector { .. } | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`length` expects a Vector argument",
                                args[0].source,
                            );
                            return None;
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: vec![arg_id],
                            },
                            expr.source,
                        );
                        Some((id, Infer::F64))
                    }
                    "dot" => {
                        let (l_id, l_infer) = self.lower_expr(&args[0])?;
                        let (r_id, r_infer) = self.lower_expr(&args[1])?;
                        match (&l_infer, &r_infer) {
                            (Infer::Vector { extent: e1 }, Infer::Vector { extent: e2 }) => {
                                if let (Some(ext1), Some(ext2)) = (e1, e2) {
                                    if ext1 != ext2 {
                                        self.error(
                                            "E-SHAPE-002",
                                            format!("dimension mismatch in dot product: {ext1:?} vs {ext2:?}"),
                                            expr.source,
                                        );
                                        return None;
                                    }
                                }
                                let id = self.push_expr(
                                    ExprNode::Binary {
                                        operation: emath_ir::BinaryOp::VectorDot,
                                        left: l_id,
                                        right: r_id,
                                    },
                                    expr.source,
                                );
                                Some((id, Infer::F64))
                            }
                            (Infer::HostDeferred, _) | (_, Infer::HostDeferred) => {
                                let id = self.push_expr(
                                    ExprNode::Binary {
                                        operation: emath_ir::BinaryOp::VectorDot,
                                        left: l_id,
                                        right: r_id,
                                    },
                                    expr.source,
                                );
                                Some((id, Infer::F64))
                            }
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`dot` expects two Vector arguments",
                                    expr.source,
                                );
                                None
                            }
                        }
                    }
                    "mean" => {
                        let (arg_id, arg_infer) = self.lower_expr(&args[0])?;
                        if !matches!(arg_infer, Infer::Vector { .. } | Infer::HostDeferred) {
                            self.error(
                                "E-TYPE-012",
                                "`mean` expects a Vector argument",
                                args[0].source,
                            );
                            return None;
                        }
                        // mean = sum(arg) / length(arg), reusing the known-shape fold and len.
                        let (sum_id, _) = self.lower_reduction(expr, "sum", &args[0])?;
                        let length_id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName("length".to_string()),
                                arguments: vec![arg_id],
                            },
                            expr.source,
                        );
                        let id = self.push_expr(
                            ExprNode::Binary {
                                operation: emath_ir::BinaryOp::StrictFloatDiv,
                                left: sum_id,
                                right: length_id,
                            },
                            expr.source,
                        );
                        Some((id, Infer::F64))
                    }
                    "abs" => {
                        let (arg_id, arg_infer) = self.lower_expr(&args[0])?;
                        match arg_infer {
                            Infer::F64 | Infer::HostDeferred => {
                                let id = self.push_expr(
                                    ExprNode::Call {
                                        function: QualifiedName("abs".to_string()),
                                        arguments: vec![arg_id],
                                    },
                                    expr.source,
                                );
                                Some((id, Infer::F64))
                            }
                            Infer::Vector { extent: Some(Extent::Fixed(n)) } => {
                                let mut elems = Vec::with_capacity(n);
                                for i in 0..n {
                                    let idx = self.push_expr(
                                        ExprNode::Literal(Literal::Integer(i.to_string())),
                                        expr.source,
                                    );
                                    let term = self.push_expr(
                                        ExprNode::Index {
                                            value: arg_id,
                                            indices: vec![idx],
                                        },
                                        expr.source,
                                    );
                                    let abs_term = self.push_expr(
                                        ExprNode::Call {
                                            function: QualifiedName("abs".to_string()),
                                            arguments: vec![term],
                                        },
                                        expr.source,
                                    );
                                    elems.push(abs_term);
                                }
                                let id = self.push_expr(ExprNode::Vector(elems), expr.source);
                                Some((
                                    id,
                                    Infer::Vector { extent: Some(Extent::Fixed(n)) },
                                ))
                            }
                            Infer::Vector { extent: None } => {
                                self.error(
                                    "E-TYPE-012",
                                    "`abs` on a vector needs a known size",
                                    args[0].source,
                                );
                                None
                            }
                            _ => {
                                self.error(
                                    "E-TYPE-012",
                                    "`abs` expects a scalar or vector argument",
                                    args[0].source,
                                );
                                None
                            }
                        }
                    }
                    _ => {
                        let mut lowered = Vec::new();
                        for arg in args {
                            let (id, infer) = self.lower_expr(arg)?;
                            if !matches!(infer, Infer::F64 | Infer::HostDeferred) {
                                self.error(
                                    "E-TYPE-012",
                                    format!("argument to `{name}` must be Float64"),
                                    arg.source,
                                );
                                return None;
                            }
                            lowered.push(id);
                        }
                        let id = self.push_expr(
                            ExprNode::Call {
                                function: QualifiedName(name.clone()),
                                arguments: lowered,
                            },
                            expr.source,
                        );
                        let result = if name == "is_finite" {
                            Infer::Bool
                        } else {
                            Infer::F64
                        };
                        Some((id, result))
                    }
                }
            }
            ExprKind::Unary { op, value } => {
                let (id, infer) = self.lower_expr(value)?;
                match (op, &infer) {
                    (SynUnOp::Neg, Infer::F64 | Infer::Nat | Infer::Int | Infer::Unit { .. } | Infer::HostDeferred) => {
                        self.record("sema", "negate → strict negate", expr.source);
                        let result = if matches!(infer, Infer::Nat) {
                            Infer::Int
                        } else {
                            infer
                        };
                        Some((
                            self.push_expr(
                                ExprNode::Unary {
                                    operation: emath_ir::UnaryOp::Negate,
                                    value: id,
                                },
                                expr.source,
                            ),
                            result,
                        ))
                    }
                    (SynUnOp::Pos, Infer::F64 | Infer::Nat | Infer::Int | Infer::Unit { .. } | Infer::HostDeferred) => {
                        Some((id, infer))
                    }
                    (SynUnOp::Not, Infer::Bool) => Some((
                        self.push_expr(
                            ExprNode::Unary {
                                operation: emath_ir::UnaryOp::Not,
                                value: id,
                            },
                            expr.source,
                        ),
                        Infer::Bool,
                    )),
                    _ => {
                        self.error(
                            "E-TYPE-012",
                            "unary operator applied to an incompatible value",
                            expr.source,
                        );
                        None
                    }
                }
            }
            ExprKind::Binary { op, left, right } => {
                let (l, l_infer) = self.lower_expr(left)?;
                let (r, r_infer) = self.lower_expr(right)?;
                let arithmetic = |admitter: &mut Admitter,
                                  operation: emath_ir::BinaryOp,
                                  expr: &Expr,
                                  l: ExprId,
                                  r: ExprId,
                                  result: Infer| {
                    Some((
                        admitter.push_expr(
                            ExprNode::Binary {
                                operation,
                                left: l,
                                right: r,
                            },
                            expr.source,
                        ),
                        result,
                    ))
                };
                match op {
                    SynBinOp::Add => {
                        match (&l_infer, &r_infer) {
                            (Infer::Vector { extent: ext_l }, Infer::Vector { extent: ext_r }) => {
                                if let (Some(l_e), Some(r_e)) = (ext_l, ext_r) {
                                    if l_e != r_e {
                                        self.error(
                                            "E-SHAPE-005",
                                            format!("dimension mismatch in vector addition: {l_e:?} vs {r_e:?}"),
                                            expr.source,
                                        );
                                        return None;
                                    }
                                }
                                let res_extent = ext_l.clone().or_else(|| ext_r.clone());
                                self.record("sema", "vector add", expr.source);
                                arithmetic(self, emath_ir::BinaryOp::VectorAdd, expr, l, r, Infer::Vector { extent: res_extent })
                            }
                            (Infer::Matrix { rows: r1, cols: c1 }, Infer::Matrix { rows: r2, cols: c2 }) => {
                                if let (Some(r1_e), Some(r2_e)) = (r1, r2) {
                                    if r1_e != r2_e {
                                        self.error("E-SHAPE-005", "matrix row dimension mismatch in addition", expr.source);
                                        return None;
                                    }
                                }
                                if let (Some(c1_e), Some(c2_e)) = (c1, c2) {
                                    if c1_e != c2_e {
                                        self.error("E-SHAPE-005", "matrix col dimension mismatch in addition", expr.source);
                                        return None;
                                    }
                                }
                                self.record("sema", "matrix add", expr.source);
                                arithmetic(self, emath_ir::BinaryOp::MatrixAdd, expr, l, r, Infer::Matrix { rows: r1.clone().or_else(|| r2.clone()), cols: c1.clone().or_else(|| c2.clone()) })
                            }
                            (Infer::Tensor { shape: left_shape }, Infer::Tensor { shape: right_shape }) => {
                                let shape = broadcast_tensor_shapes(self, left_shape, right_shape, expr)?;
                                self.record("sema", "tensor add", expr.source);
                                arithmetic(self, emath_ir::BinaryOp::TensorAdd, expr, l, r, Infer::Tensor { shape })
                            }
                            _ => {
                                self.record("sema", "add → strict f64 add", expr.source);
                                let result = combine_numeric(&l_infer, &r_infer, NumericCombine::Add, expr, self)?;
                                arithmetic(self, emath_ir::BinaryOp::StrictFloatAdd, expr, l, r, result)
                            }
                        }
                    }
                    SynBinOp::Sub => {
                        match (&l_infer, &r_infer) {
                            (Infer::Vector { extent: ext_l }, Infer::Vector { extent: ext_r }) => {
                                if let (Some(l_e), Some(r_e)) = (ext_l, ext_r) {
                                    if l_e != r_e {
                                        self.error(
                                            "E-SHAPE-005",
                                            format!("dimension mismatch in vector subtraction: {l_e:?} vs {r_e:?}"),
                                            expr.source,
                                        );
                                        return None;
                                    }
                                }
                                let res_extent = ext_l.clone().or_else(|| ext_r.clone());
                                self.record("sema", "vector subtract", expr.source);
                                arithmetic(self, emath_ir::BinaryOp::VectorSub, expr, l, r, Infer::Vector { extent: res_extent })
                            }
                            (Infer::Matrix { rows: r1, cols: c1 }, Infer::Matrix { rows: r2, cols: c2 }) => {
                                if let (Some(r1_e), Some(r2_e)) = (r1, r2) {
                                    if r1_e != r2_e {
                                        self.error("E-SHAPE-005", "matrix row dimension mismatch in subtraction", expr.source);
                                        return None;
                                    }
                                }
                                if let (Some(c1_e), Some(c2_e)) = (c1, c2) {
                                    if c1_e != c2_e {
                                        self.error("E-SHAPE-005", "matrix col dimension mismatch in subtraction", expr.source);
                                        return None;
                                    }
                                }
                                self.record("sema", "matrix subtract", expr.source);
                                arithmetic(self, emath_ir::BinaryOp::MatrixSub, expr, l, r, Infer::Matrix { rows: r1.clone().or_else(|| r2.clone()), cols: c1.clone().or_else(|| c2.clone()) })
                            }
                            (Infer::Tensor { shape: left_shape }, Infer::Tensor { shape: right_shape }) => {
                                let shape = broadcast_tensor_shapes(self, left_shape, right_shape, expr)?;
                                self.record("sema", "tensor subtract", expr.source);
                                arithmetic(self, emath_ir::BinaryOp::TensorSub, expr, l, r, Infer::Tensor { shape })
                            }
                            _ => {
                                self.record("sema", "subtract → strict f64 subtract", expr.source);
                                let result = combine_numeric(&l_infer, &r_infer, NumericCombine::Add, expr, self)?;
                                arithmetic(self, emath_ir::BinaryOp::StrictFloatSub, expr, l, r, result)
                            }
                        }
                    }
                    SynBinOp::Mul => {
                        match (&l_infer, &r_infer) {
                            (Infer::Vector { extent }, Infer::F64 | Infer::HostDeferred) => {
                                self.record("sema", "vector scale", expr.source);
                                arithmetic(self, emath_ir::BinaryOp::VectorScale, expr, l, r, Infer::Vector { extent: extent.clone() })
                            }
                            (Infer::F64 | Infer::HostDeferred, Infer::Vector { extent }) => {
                                self.record("sema", "vector scale", expr.source);
                                arithmetic(self, emath_ir::BinaryOp::VectorScale, expr, r, l, Infer::Vector { extent: extent.clone() })
                            }
                            (Infer::Matrix { rows, cols }, Infer::F64 | Infer::HostDeferred) => {
                                self.record("sema", "matrix scale", expr.source);
                                arithmetic(self, emath_ir::BinaryOp::MatrixScale, expr, l, r, Infer::Matrix { rows: rows.clone(), cols: cols.clone() })
                            }
                            (Infer::F64 | Infer::HostDeferred, Infer::Matrix { rows, cols }) => {
                                self.record("sema", "matrix scale", expr.source);
                                arithmetic(self, emath_ir::BinaryOp::MatrixScale, expr, r, l, Infer::Matrix { rows: rows.clone(), cols: cols.clone() })
                            }
                            (Infer::Matrix { rows, cols }, Infer::Vector { extent }) => {
                                if let (Some(c_e), Some(v_e)) = (cols, extent) {
                                    if c_e != v_e {
                                        self.error(
                                            "E-SHAPE-002",
                                            format!("dimension mismatch in matrix-vector multiplication: matrix columns {c_e:?} != vector length {v_e:?}"),
                                            expr.source,
                                        );
                                        return None;
                                    }
                                }
                                self.record("sema", "matrix mul vector", expr.source);
                                arithmetic(self, emath_ir::BinaryOp::MatrixMulVector, expr, l, r, Infer::Vector { extent: rows.clone() })
                            }
                            (Infer::Matrix { rows: r1, cols: c1 }, Infer::Matrix { rows: r2, cols: c2 }) => {
                                if let (Some(c1_e), Some(r2_e)) = (c1, r2) {
                                    if c1_e != r2_e {
                                        self.error(
                                            "E-SHAPE-002",
                                            format!("dimension mismatch in matrix multiplication: left columns {c1_e:?} != right rows {r2_e:?}"),
                                            expr.source,
                                        );
                                        return None;
                                    }
                                }
                                self.record("sema", "matrix mul matrix", expr.source);
                                arithmetic(self, emath_ir::BinaryOp::MatrixMulMatrix, expr, l, r, Infer::Matrix { rows: r1.clone(), cols: c2.clone() })
                            }
                            _ => {
                                self.record("sema", "multiply → strict f64 multiply", expr.source);
                                let result = combine_numeric(&l_infer, &r_infer, NumericCombine::Mul, expr, self)?;
                                arithmetic(self, emath_ir::BinaryOp::StrictFloatMul, expr, l, r, result)
                            }
                        }
                    }
                    SynBinOp::Div => {
                        self.record("sema", "divide → strict f64 divide", expr.source);
                        let result = combine_numeric(&l_infer, &r_infer, NumericCombine::Div, expr, self)?;
                        arithmetic(self, emath_ir::BinaryOp::StrictFloatDiv, expr, l, r, result)
                    }
                    SynBinOp::Pow => {
                        self.record("sema", "power → strict f64 powf", expr.source);
                        if !matches!(
                            (l_infer, r_infer),
                            (
                                Infer::F64 | Infer::HostDeferred,
                                Infer::F64 | Infer::HostDeferred
                            )
                        ) {
                            self.error(
                                "E-TYPE-012",
                                "operator `^` requires dimensionless Float64 operands",
                                expr.source,
                            );
                            return None;
                        }
                        arithmetic(self, emath_ir::BinaryOp::StrictFloatPow, expr, l, r, Infer::F64)
                    }
                    SynBinOp::Eq
                    | SynBinOp::Ne
                    | SynBinOp::Lt
                    | SynBinOp::Le
                    | SynBinOp::Gt
                    | SynBinOp::Ge => {
                        let operation = match op {
                            SynBinOp::Eq => emath_ir::BinaryOp::Equal,
                            SynBinOp::Ne => emath_ir::BinaryOp::NotEqual,
                            SynBinOp::Lt => emath_ir::BinaryOp::Less,
                            SynBinOp::Le => emath_ir::BinaryOp::LessEqual,
                            SynBinOp::Gt => emath_ir::BinaryOp::Greater,
                            _ => emath_ir::BinaryOp::GreaterEqual,
                        };
                        if matches!(
                            op,
                            SynBinOp::Lt | SynBinOp::Le | SynBinOp::Gt | SynBinOp::Ge
                        ) && !comparable_numeric(&l_infer, &r_infer)
                        {
                            self.error(
                                "E-UNIT-101",
                                "ordered comparisons require dimensionally compatible numeric operands",
                                expr.source,
                            );
                            return None;
                        }
                        Some((
                            self.push_expr(
                                ExprNode::Binary {
                                    operation,
                                    left: l,
                                    right: r,
                                },
                                expr.source,
                            ),
                            Infer::Bool,
                        ))
                    }
                    SynBinOp::And | SynBinOp::Or | SynBinOp::Imply | SynBinOp::Iff => {
                        if !matches!(l_infer, Infer::Bool) || !matches!(r_infer, Infer::Bool) {
                            self.error(
                                "E-TYPE-012",
                                "boolean operators require Boolean operands",
                                expr.source,
                            );
                            return None;
                        }
                        Some((
                            self.push_expr(
                                ExprNode::Binary {
                                    operation: match op {
                                        SynBinOp::And => emath_ir::BinaryOp::And,
                                        SynBinOp::Or => emath_ir::BinaryOp::Or,
                                        SynBinOp::Imply => emath_ir::BinaryOp::Imply,
                                        SynBinOp::Iff => emath_ir::BinaryOp::Iff,
                                        _ => unreachable!(),
                                    },
                                    left: l,
                                    right: r,
                                },
                                expr.source,
                            ),
                            Infer::Bool,
                        ))
                    }
                }
            }
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                let (cond, cond_infer) = self.lower_expr(condition)?;
                if !matches!(cond_infer, Infer::Bool) {
                    self.error(
                        "E-TYPE-012",
                        "`if` condition must be Boolean",
                        condition.source,
                    );
                    return None;
                }
                let (then_id, then_infer) = self.lower_expr(then_value)?;
                let (else_id, else_infer) = self.lower_expr(else_value)?;
                if then_infer != else_infer {
                    self.error(
                        "E-TYPE-012",
                        "`if` branches must have the same type",
                        expr.source,
                    );
                    return None;
                }
                Some((
                    self.push_expr(
                        ExprNode::If {
                            condition: cond,
                            then_value: then_id,
                            else_value: else_id,
                        },
                        expr.source,
                    ),
                    then_infer,
                ))
            }
            ExprKind::List(items) => self.lower_list_literal(expr, items),
            ExprKind::Index { value, indices } => self.lower_index(expr, value, indices),
            ExprKind::Binder {
                kind,
                binders,
                body,
                guard,
            } => self.lower_finite_binder(expr, *kind, binders, body, guard.as_deref()),
            ExprKind::Derivative { .. } => {
                // The parser may produce nested Derivative nodes:
                // `derivative x wrt y` becomes Derivative(Derivative(x)) wrt y.
                // Unwrap to get the inner value and the wrt clause.
                let Some((value, wrt)) = unwrap_derivative(expr) else {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "derivative could not be unwrapped",
                        expr.source,
                    );
                    return None;
                };
                let Some(vars) = wrt else {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "derivative requires `wrt` clause: derivative(expr) wrt var",
                        expr.source,
                    );
                    return None;
                };
                if vars.len() != 1 {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "derivative wrt supports a single variable in Phase 1",
                        expr.source,
                    );
                    return None;
                }
                let Some(segments) = path_segments(&vars[0]) else {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "derivative variable must be a plain name",
                        expr.source,
                    );
                    return None;
                };
                let var_name = segments[0].clone();
                if !self.inputs.contains_key(&var_name) {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        format!("derivative variable `{var_name}` must be an input"),
                        expr.source,
                    );
                    return None;
                }
                // Lower the value expression, then inline definition
                // references so the EMIR dual-number evaluator sees the
                // full computation chain.
                let (body_id, body_infer) = match self.lower_expr(value) {
                    Some(result) => result,
                    None => return None,
                };
                if !is_numeric_element(&body_infer) {
                    self.error(
                        "E-TYPE-012",
                        "derivative body must be numeric",
                        value.source,
                    );
                    return None;
                }
                let inlined = self.inline_defs(body_id);
                let id = self.push_expr(
                    ExprNode::Differentiate { body: inlined, var: var_name.clone() },
                    expr.source,
                );
                self.record(
                    "sema",
                    format!("derivative wrt {var_name} → forward-mode autodiff"),
                    expr.source,
                );
                Some((id, Infer::F64))
            }
            ExprKind::Solve { value, wrt } => {
                let Some(vars) = wrt.as_deref() else {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "solve requires `wrt` clause: solve(expr) wrt var",
                        expr.source,
                    );
                    return None;
                };
                if vars.len() != 1 {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "solve wrt supports a single variable in Phase 1",
                        expr.source,
                    );
                    return None;
                }
                let Some(segments) = path_segments(&vars[0]) else {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "solve variable must be a plain name",
                        expr.source,
                    );
                    return None;
                };
                let var_name = segments[0].clone();
                if !self.inputs.contains_key(&var_name) {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        format!("solve variable `{var_name}` must be an input"),
                        expr.source,
                    );
                    return None;
                }
                let (body_id, body_infer) = match self.lower_expr(value) {
                    Some(result) => result,
                    None => return None,
                };
                if !is_numeric_element(&body_infer) {
                    self.error(
                        "E-TYPE-012",
                        "solve body must be numeric",
                        value.source,
                    );
                    return None;
                }
                let inlined = self.inline_defs(body_id);
                let id = self.push_expr(
                    ExprNode::Solve { body: inlined, var: var_name.clone() },
                    expr.source,
                );
                self.record(
                    "sema",
                    format!("solve wrt {var_name} → Newton's method root-finding"),
                    expr.source,
                );
                Some((id, Infer::F64))
            }
            ExprKind::Optimize { value, wrt, maximize } => {
                let Some(vars) = wrt.as_deref() else {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "minimize/maximize requires `wrt` clause: minimize(expr) wrt var",
                        expr.source,
                    );
                    return None;
                };
                if vars.is_empty() {
                    self.error(
                        E_UNSUPPORTED_TYPE,
                        "minimize/maximize requires at least one `wrt` variable",
                        expr.source,
                    );
                    return None;
                }
                let mut var_names = Vec::with_capacity(vars.len());
                for var in vars {
                    let Some(segments) = path_segments(var) else {
                        self.error(
                            E_UNSUPPORTED_TYPE,
                            "optimization variable must be a plain name",
                            var.source,
                        );
                        return None;
                    };
                    let name = segments[0].clone();
                    if !self.inputs.contains_key(&name) {
                        self.error(
                            E_UNSUPPORTED_TYPE,
                            format!("optimization variable `{name}` must be an input"),
                            var.source,
                        );
                        return None;
                    }
                    var_names.push(name);
                }
                let (body_id, body_infer) = match self.lower_expr(value) {
                    Some(result) => result,
                    None => return None,
                };
                if !is_numeric_element(&body_infer) {
                    self.error(
                        "E-TYPE-012",
                        "optimization body must be numeric",
                        value.source,
                    );
                    return None;
                }
                let inlined = self.inline_defs(body_id);
                let body_with_penalty = self.add_constraint_penalties(inlined, expr.source);
                let id = self.push_expr(
                    ExprNode::Optimize { body: body_with_penalty, vars: var_names.clone(), maximize: *maximize },
                    expr.source,
                );
                let direction = if *maximize { "maximize" } else { "minimize" };
                self.record(
                    "sema",
                    format!("{direction} wrt {} → gradient-descent optimization", var_names.join(", ")),
                    expr.source,
                );
                Some((id, Infer::F64))
            }
            other => {
                self.error(
                    E_UNSUPPORTED_TYPE,
                    format!(
                        "expression form `{}` is outside the Phase 1 strict-f64 subset",
                        expr_form_name(other)
                    ),
                    expr.source,
                );
                None
            }
        }
    }

    pub(super) fn lower_requirement(&mut self, expr: &Expr) -> Option<ExprId> {
        let (id, infer) = self.lower_expr(expr)?;
        if !matches!(infer, Infer::Bool) {
            self.error(
                "E-CTOR-032",
                "`require` must be a Boolean expression",
                expr.source,
            );
            return None;
        }
        Some(id)
    }

    /// Lower an `einsum("subscripts", A, B, ...)` call.
    /// The subscripts string is carried as the first Call argument
    /// (a Literal::Text). The emitter extracts it and emits EmirOp::Einsum.
    fn lower_einsum(
        &mut self,
        expr: &Expr,
        name: &str,
        args: &[Expr],
    ) -> Option<(ExprId, Infer)> {
        use emath_ir::ExprNode;

        // Lower all arguments (including the subscripts string literal).
        let mut arg_ids = Vec::with_capacity(args.len());
        for arg in args {
            let (id, _) = self.lower_expr(arg)?;
            arg_ids.push(id);
        }

        // Determine the output rank from the subscripts string.
        let subscripts = if let ExprKind::Str(s) = &args[0].kind {
            s.clone()
        } else {
            // Already checked in the caller, but defensive.
            self.error(
                "E-TYPE-012",
                "`einsum` first argument must be a string literal",
                args[0].source,
            );
            return None;
        };

        let output_spec = if let Some((_, rhs)) = subscripts.split_once("->") {
            rhs.trim().to_string()
        } else {
            // Implicit mode: non-repeated indices.
            let inputs: Vec<&str> = subscripts.split(',').map(str::trim).collect();
            let mut counts: std::collections::HashMap<char, usize> = std::collections::HashMap::new();
            for spec in &inputs {
                for c in spec.chars() {
                    *counts.entry(c).or_insert(0) += 1;
                }
            }
            inputs.iter()
                .flat_map(|spec| spec.chars())
                .filter(|c| counts.get(c) == Some(&1))
                .collect::<std::collections::HashSet<_>>()
                .into_iter().collect::<String>()
        };

        let infer = match output_spec.len() {
            0 => Infer::F64,
            1 => Infer::Vector { extent: None },
            2 => Infer::Matrix { rows: None, cols: None },
            _ => Infer::HostDeferred,
        };

        let id = self.push_expr(
            ExprNode::Call {
                function: QualifiedName(name.to_string()),
                arguments: arg_ids,
            },
            expr.source,
        );
        Some((id, infer))
    }
}
