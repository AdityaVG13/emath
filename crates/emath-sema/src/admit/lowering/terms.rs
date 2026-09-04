//! Leaf-form lowering: unary, path, quantity, series policy, measured, string, graph tuple.

use super::*;

impl super::super::Admitter {
    pub(super) fn lower_unary_expr_arm(&mut self, expr: &Expr) -> Option<(ExprId, Infer)> {
        let ExprKind::Unary { op, value } = &expr.kind else {
            unreachable!()
        };
        let (id, infer) = self.lower_expr(value)?;
        match (op, &infer) {
            (
                SynUnOp::Neg,
                Infer::F64
                | Infer::Nat
                | Infer::Int
                | Infer::Complex
                | Infer::Unit { .. }
                | Infer::HostDeferred,
            ) => {
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
            (
                SynUnOp::Pos,
                Infer::F64
                | Infer::Nat
                | Infer::Int
                | Infer::Complex
                | Infer::Unit { .. }
                | Infer::HostDeferred,
            ) => Some((id, infer)),
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

    pub(super) fn lower_path_expr_arm(&mut self, expr: &Expr) -> Option<(ExprId, Infer)> {
        let ExprKind::Path { segments, .. } = &expr.kind else {
            unreachable!()
        };
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
            let id = self.push_expr(ExprNode::Variable(QualifiedName(ir_name)), expr.source);
            return Some((id, infer));
        }
        if segments.len() >= 2 {
            if matches!(self.lookup(&segments[0]), Some(Infer::Opaque)) {
                self.record(
                    "sema",
                    format!("host field `{name}` deferred to the host boundary"),
                    expr.source,
                );
                let id = self.push_expr(ExprNode::Variable(QualifiedName(name)), expr.source);
                return Some((id, Infer::HostDeferred));
            }
        }
        if segments.len() == 1 {
            if let Ok(unit) = lookup_unit(&segments[0]) {
                let si = unit.to_si(1.0);
                self.record(
                    "sema",
                    format!("unit literal `{}` → SI {si} ({})", segments[0], unit.name),
                    expr.source,
                );
                let id = self.push_expr(
                    ExprNode::Literal(Literal::FloatBits(si.to_bits())),
                    expr.source,
                );
                return Some((id, Infer::from_unit(&unit)));
            }
            // B14: `i` is the imaginary unit (0 + 1i). It is a
            // named constant, not a reserved keyword — only
            // recognized when not shadowed by an input/definition.
            if segments[0] == "Hole" {
                self.note(
                    "N-HOLE-001",
                    "open hole; meaning stays open and is not claimed exact",
                    expr.source,
                );
                let id = self.push_expr(
                    ExprNode::Variable(QualifiedName("Hole".to_string())),
                    expr.source,
                );
                return Some((id, Infer::HostDeferred));
            }
            if segments[0] == "i" {
                self.record("sema", "imaginary unit `i` → Complex(0, 1)", expr.source);
                let id = self.push_expr(
                    ExprNode::Literal(Literal::Complex {
                        re_bits: 0.0_f64.to_bits(),
                        im_bits: 1.0_f64.to_bits(),
                    }),
                    expr.source,
                );
                return Some((id, Infer::Complex));
            }
        }
        if self.undefined_output_already_refused(&name) {
            // Cascade suppression (emath-2bwk): an `E-NAME-023`
            // ("output `<name>` has no definition") is already on
            // record for this declared output — the empty
            // `definitions:` block already refused at the root.
            // Repeating "unknown variable" at every later use site
            // is consequent noise, not a second root cause.
            // Suppressed here because this use is not resolvable
            // AT ALL — the name is a declared output with no
            // definition; if it had one, `lookup` above would have
            // found it.
            return None;
        }
        self.error(
            E_UNKNOWN_VARIABLE,
            format!("unknown variable `{name}`"),
            expr.source,
        );
        None
    }

    pub(super) fn lower_quantity_arm(&mut self, expr: &Expr) -> Option<(ExprId, Infer)> {
        let ExprKind::Quantity { value, unit } = &expr.kind else {
            unreachable!()
        };
        let Some(magnitude) = parse_quantity_magnitude(value) else {
            self.error(
                "E-UNIT-105",
                "quantity value must be a numeric literal",
                expr.source,
            );
            return None;
        };
        // Flatten compound units to (name, power) pairs; combine
        // dimensions and SI scale so `1 km + 1 m` is 1001 m.
        let factors = unit.flatten();
        let mut combined_dims = UnitDim::one();
        let mut combined_family = UnitFamily::Si;
        let mut combined_scale = 1.0_f64;
        let mut combined_offset = 0.0_f64;
        let mut unit_label = String::new();
        for (name, power) in &factors {
            match lookup_unit(name) {
                Ok(looked_up) => {
                    if looked_up.is_affine() && (*power != 1 || factors.len() != 1) {
                        self.error(
                            "E-UNIT-102",
                            format!(
                                "affine unit misuse: `{}` cannot appear in a compound or powered unit",
                                looked_up.name
                            ),
                            expr.source,
                        );
                        return None;
                    }
                    if !unit_label.is_empty() && looked_up.family != combined_family {
                        self.error(
                            "E-UNIT-101",
                            format!(
                                "dimension mismatch: cannot combine `{}` ({}) with `{}` ({})",
                                unit_label,
                                combined_family.as_str(),
                                looked_up.name,
                                looked_up.family.as_str()
                            ),
                            expr.source,
                        );
                        return None;
                    }
                    if *power >= 0 {
                        combined_dims = combined_dims.mul(looked_up.dims.pow(*power));
                    } else {
                        combined_dims = combined_dims.div(looked_up.dims.pow(-*power));
                    }
                    combined_family = looked_up.family;
                    combined_scale *= looked_up.scale.powi(*power);
                    if factors.len() == 1 && *power == 1 {
                        combined_offset = looked_up.offset;
                    }
                    if !unit_label.is_empty() {
                        unit_label.push('*');
                    }
                    unit_label.push_str(&looked_up.name);
                }
                Err(error) => {
                    self.error(error.code, error.message, expr.source);
                    return None;
                }
            }
        }
        let si = (magnitude + combined_offset) * combined_scale;
        if si.is_finite() {
            self.record(
                "sema",
                format!(
                    "quantity `{} {unit_label}` → SI {si} dims {}",
                    crate::recognition::expr_text(value),
                    combined_dims.render()
                ),
                expr.source,
            );
            let id = self.push_expr(
                ExprNode::Literal(Literal::FloatBits(si.to_bits())),
                expr.source,
            );
            Some((
                id,
                Infer::from_dims_affine(combined_dims, combined_family, combined_offset != 0.0),
            ))
        } else {
            self.error(
                "E-TYPE-011",
                format!(
                    "non-finite quantity `{} {unit_label}` refused under the selected numeric model",
                    crate::recognition::expr_text(value)
                ),
                expr.source,
            );
            None
        }
    }

    pub(super) fn lower_with_series_policy_arm(&mut self, expr: &Expr) -> Option<(ExprId, Infer)> {
        let ExprKind::WithSeriesPolicy {
            value,
            interpolation,
            extrapolation,
        } = &expr.kind
        else {
            unreachable!()
        };
        // 04 §5.4: a series
        // literal is pure data — `[(<time>, <value>), ...]` of
        // quantity literals, SI-scaled — plus its DECLARED
        // interpretation policy. Interpolation has no silent
        // default (the mode changes every downstream number);
        // extrapolation defaults to `refuse`. Evaluation is the
        // named next slice: admitting a series never claims it
        // interpolates or extrapolates.
        let ExprKind::List(items) = &value.kind else {
            self.error(
                "E-SYN-101",
                "a series value is `[(<time quantity>, <value quantity>), ...]` — a list of pairs",
                value.source,
            );
            return None;
        };
        if items.is_empty() {
            self.error(
                "E-SYN-101",
                "a series needs at least one `(time, value)` pair",
                value.source,
            );
            return None;
        }
        let mut points: Vec<(f64, f64)> = Vec::with_capacity(items.len());
        for item in items {
            let ExprKind::Tuple(pair) = &item.kind else {
                self.error(
                    "E-SYN-101",
                    "series rows are `(<time quantity>, <value quantity>)` pairs",
                    item.source,
                );
                return None;
            };
            if pair.len() != 2 {
                self.error(
                    "E-SYN-101",
                    "series rows are exactly `(<time quantity>, <value quantity>)`",
                    item.source,
                );
                return None;
            }
            let (Some(time), Some(val)) = (
                self.series_pair_scalar(&pair[0]),
                self.series_pair_scalar(&pair[1]),
            ) else {
                return None;
            };
            points.push((time, val));
        }
        for window in points.windows(2) {
            if window[0].0 >= window[1].0 {
                self.error(
                    "E-SYN-101",
                    "series time axis must be strictly increasing — every interpolation mode orders the support by time",
                    value.source,
                );
                return None;
            }
        }
        let Some(interpolation) = interpolation else {
            self.error(
                "E-SYN-101",
                "declare `with interpolation: previous|linear|nearest|pwc|monotone_cubic` on the series — the mode changes every downstream number and is never guessed",
                expr.source,
            );
            return None;
        };
        let extrapolation = extrapolation.unwrap_or(emath_core::tree::SeriesExtrapolation::Refuse);
        let id = self.push_expr(
            ExprNode::Series {
                points,
                interpolation: interpolation.spelling().to_string(),
                extrapolation: extrapolation.spelling().to_string(),
            },
            expr.source,
        );
        Some((id, Infer::Series))
    }

    pub(super) fn lower_measured_arm(&mut self, expr: &Expr) -> Option<(ExprId, Infer)> {
        let ExprKind::Measured {
            value,
            uncertainty,
            uncertainty_digits,
            distribution,
        } = &expr.kind
        else {
            unreachable!()
        };
        // Measurement literal (spec 04 section 1.5). Phase 1 lowers
        // the central value to strict f64; the uncertainty and the
        // Unstated provenance are recorded loudly, never silently
        // merged into the value (a measured value used as exact is
        // the same lie of omission in reverse). Full Measured<T>
        // propagation is Phase 2 work.
        let Some(central) = parse_float_constant(value) else {
            self.error(
                "E-MEAS-001",
                format!("measurement literal value `{value}` is not a valid number"),
                expr.source,
            );
            return None;
        };
        let spread = if uncertainty_digits.is_empty() {
            parse_float_constant(uncertainty)
        } else {
            measured_digits_uncertainty(value, uncertainty_digits)
        };
        let Some(std_uncertainty) = spread.filter(|s| s.is_finite() && *s >= 0.0) else {
            self.error(
                "E-MEAS-001",
                format!(
                    "measurement literal `{value}` has an invalid uncertainty `{uncertainty}{uncertainty_digits}`"
                ),
                expr.source,
            );
            return None;
        };
        let kind = match distribution.as_deref() {
            None | Some("normal") => DistributionKind::Normal,
            Some("uniform") => DistributionKind::Uniform,
            Some("lognormal") => DistributionKind::Lognormal,
            Some(other) => {
                self.error(
                    "E-MEAS-002",
                    format!("unknown distribution tag `~ {other}` (normal | uniform | lognormal)"),
                    expr.source,
                );
                return None;
            }
        };
        self.record(
            "sema",
            format!(
                "measurement `{value} ± {std_uncertainty:e}` ({kind:?}, provenance Unstated) recorded; central value lowers strict"
            ),
            expr.source,
        );
        self.warning(
            "E-MEAS-003",
            format!(
                "measurement literal `{value} ± {std_uncertainty:e}` is used as a strict value; the uncertainty is recorded (provenance: Unstated) but not propagated in Phase 1"
            ),
            expr.source,
        );
        let id = self.push_expr(
            ExprNode::Literal(Literal::FloatBits(central.to_bits())),
            expr.source,
        );
        Some((id, Infer::F64))
    }

    pub(super) fn lower_str_literal_arm(&mut self, expr: &Expr) -> Option<(ExprId, Infer)> {
        let ExprKind::Str(template) = &expr.kind else {
            unreachable!()
        };
        let template = emath_core::normalize_nfc(template);
        let literal = self.push_expr(
            ExprNode::Literal(Literal::Text(template.clone())),
            expr.source,
        );
        let paths = interpolation_paths(&template);
        if paths.is_empty() {
            return Some((literal, Infer::Text));
        }
        let mut arguments = Vec::with_capacity(paths.len() + 1);
        arguments.push(literal);
        for path in paths {
            let value = Expr {
                kind: ExprKind::Path {
                    segments: path.split('.').map(str::to_string).collect(),
                    generics: None,
                },
                source: expr.source,
            };
            let (id, _) = self.lower_expr(&value)?;
            arguments.push(id);
        }
        let id = self.push_expr(
            ExprNode::Call {
                function: QualifiedName("__format_text".to_string()),
                arguments,
            },
            expr.source,
        );
        Some((id, Infer::Text))
    }
    pub(super) fn lower_graph_tuple(
        &mut self,
        expr: &Expr,
        items: &[Expr],
    ) -> Option<(ExprId, Infer)> {
        let (nodes, edges) = graph_tuple_parts(items)?;
        let nodes = nodes
            .iter()
            .map(signed_numeric_literal)
            .collect::<Option<Vec<_>>>()
            .or_else(|| {
                self.error(
                    "E-TYPE-012",
                    "graph nodes must be finite numeric literals",
                    expr.source,
                );
                None
            })?;
        if nodes.is_empty() {
            self.error("E-TYPE-012", "a graph needs at least one node", expr.source);
            return None;
        }
        let mut adjacency = vec![vec![0.0_f64; nodes.len()]; nodes.len()];
        for edge in edges {
            let ExprKind::List(parts) = &edge.kind else {
                return None;
            };
            let values = parts
                .iter()
                .map(signed_numeric_literal)
                .collect::<Option<Vec<_>>>()
                .or_else(|| {
                    self.error(
                        "E-TYPE-012",
                        "graph edge endpoints, weights, and direction must be finite literals",
                        edge.source,
                    );
                    None
                })?;
            let Some(from) = nodes.iter().position(|node| *node == values[0]) else {
                self.error(
                    "E-TYPE-012",
                    "graph edge names an unknown source node",
                    edge.source,
                );
                return None;
            };
            let Some(to) = nodes.iter().position(|node| *node == values[1]) else {
                self.error(
                    "E-TYPE-012",
                    "graph edge names an unknown target node",
                    edge.source,
                );
                return None;
            };
            adjacency[from][to] = values[2];
            if values[3] == 0.0 {
                adjacency[to][from] = values[2];
            }
        }
        let rows = adjacency
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|value| {
                        self.push_expr(
                            ExprNode::Literal(Literal::FloatBits(value.to_bits())),
                            expr.source,
                        )
                    })
                    .collect()
            })
            .collect();
        let id = self.push_expr(ExprNode::Matrix(rows), expr.source);
        Some((
            id,
            Infer::Matrix {
                rows: Some(Extent::Fixed(nodes.len())),
                cols: Some(Extent::Fixed(nodes.len())),
            },
        ))
    }
}
