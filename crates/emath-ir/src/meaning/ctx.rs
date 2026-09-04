//! The meaning encoder context and slot allocation.

use super::*;

#[derive(Default)]
pub(super) struct Encoder {
    pub(super) bytes: Vec<u8>,
}

impl Encoder {
    pub(super) fn tag(&mut self, tag: u8) {
        self.bytes.push(tag);
    }

    pub(super) fn bool(&mut self, value: bool) {
        self.bytes.push(u8::from(value));
    }

    pub(super) fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    pub(super) fn usize(&mut self, value: usize) {
        self.u64(u64::try_from(value).unwrap_or(u64::MAX));
    }

    /// IEEE-754 bit patterns: exact, deterministic, NaN-signature stable.
    pub(super) fn f64(&mut self, value: f64) {
        self.u64(value.to_bits());
    }

    pub(super) fn text(&mut self, value: &str) {
        self.usize(value.len());
        self.bytes.extend_from_slice(value.as_bytes());
    }

    pub(super) fn blob(&mut self, value: &[u8]) {
        self.usize(value.len());
        self.bytes.extend_from_slice(value);
    }

    pub(super) fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

#[derive(Clone)]
pub(super) struct LocalSlot {
    pub(super) role: u8,
    pub(super) index: usize,
}

pub(super) struct MeaningContext<'a> {
    pub(super) package: &'a SemanticPackage,
    pub(super) locals: BTreeMap<String, LocalSlot>,
    pub(super) definitions: &'a BTreeMap<String, ExprId>,
    pub(super) aliases: BTreeMap<String, String>,
    pub(super) bound: Vec<String>,
    pub(super) active_exprs: BTreeSet<ExprId>,
    pub(super) active_definitions: BTreeSet<String>,
}

impl MeaningContext<'_> {
    pub(super) fn encode_name(
        &mut self,
        out: &mut Encoder,
        name: &str,
    ) -> Result<(), MeaningError> {
        if let Some(position) = self.bound.iter().rposition(|bound| bound == name) {
            out.tag(0);
            out.usize(self.bound.len() - position - 1);
            return Ok(());
        }
        let local_name = name.strip_prefix("state.").unwrap_or(name);
        if let Some(slot) = self.locals.get(local_name) {
            out.tag(1);
            out.tag(slot.role);
            out.usize(slot.index);
            return Ok(());
        }
        if let Some(expression) = self.definitions.get(name).copied() {
            if !self.active_definitions.insert(name.to_string()) {
                return Err(MeaningError::CyclicDefinition(name.to_string()));
            }
            out.tag(2);
            self.encode_expr(out, expression)?;
            self.active_definitions.remove(name);
            return Ok(());
        }
        out.tag(3);
        out.text(self.aliases.get(name).map_or(name, String::as_str));
        Ok(())
    }

    pub(super) fn encode_expr(
        &mut self,
        out: &mut Encoder,
        id: ExprId,
    ) -> Result<(), MeaningError> {
        if !self.active_exprs.insert(id) {
            return Err(MeaningError::CyclicExpr(id));
        }
        let expr = self
            .package
            .exprs
            .get(id.index())
            .ok_or(MeaningError::MissingExpr(id))?;
        match expr {
            ExprNode::Literal(literal) => {
                out.tag(0);
                match literal {
                    Literal::Bool(value) => {
                        out.tag(0);
                        out.bool(*value);
                    }
                    Literal::Integer(value) => {
                        out.tag(1);
                        out.text(value);
                    }
                    Literal::Rational(value) => {
                        out.tag(2);
                        out.text(value);
                    }
                    Literal::FloatBits(bits) => {
                        out.tag(3);
                        out.u64(*bits);
                    }
                    Literal::Complex { re_bits, im_bits } => {
                        out.tag(4);
                        out.u64(*re_bits);
                        out.u64(*im_bits);
                    }
                    Literal::Text(value) => {
                        out.tag(5);
                        out.text(value);
                    }
                }
            }
            ExprNode::Variable(name) => {
                out.tag(1);
                self.encode_name(out, &name.0)?;
            }
            ExprNode::Call {
                function,
                arguments,
            } => {
                out.tag(2);
                self.encode_name(out, &function.0)?;
                out.usize(arguments.len());
                for argument in arguments {
                    self.encode_expr(out, *argument)?;
                }
            }
            ExprNode::Unary { operation, value } => {
                out.tag(3);
                out.text(operation.name());
                self.encode_expr(out, *value)?;
            }
            ExprNode::Binary {
                operation,
                left,
                right,
            } => {
                out.tag(4);
                out.text(operation.name());
                self.encode_expr(out, *left)?;
                self.encode_expr(out, *right)?;
            }
            ExprNode::If {
                condition,
                then_value,
                else_value,
            } => {
                out.tag(5);
                self.encode_expr(out, *condition)?;
                self.encode_expr(out, *then_value)?;
                self.encode_expr(out, *else_value)?;
            }
            ExprNode::Record { fields, ty } => {
                out.tag(6);
                encode_type_id(out, self.package, *ty)?;
                out.usize(fields.len());
                for (field, value) in fields {
                    out.text(field);
                    self.encode_expr(out, *value)?;
                }
            }
            ExprNode::Index { value, indices } => {
                out.tag(7);
                self.encode_expr(out, *value)?;
                out.usize(indices.len());
                for index in indices {
                    self.encode_expr(out, *index)?;
                }
            }
            ExprNode::Slice { value, axes } => {
                out.tag(8);
                self.encode_expr(out, *value)?;
                out.usize(axes.len());
                for axis in axes {
                    match axis {
                        SliceAxis::Point(index) => {
                            out.tag(0);
                            self.encode_expr(out, *index)?;
                        }
                        SliceAxis::Range { start, end } => {
                            out.tag(1);
                            self.encode_expr(out, *start)?;
                            self.encode_expr(out, *end)?;
                        }
                    }
                }
            }
            ExprNode::Binder {
                kind,
                variables,
                body,
            } => {
                out.tag(9);
                out.tag(match kind {
                    BinderKind::Sum => 0,
                    BinderKind::Product => 1,
                    BinderKind::Integral => 2,
                    BinderKind::ForAll => 3,
                    BinderKind::Exists => 4,
                    BinderKind::Series => 5,
                });
                out.usize(variables.len());
                let bound_len = self.bound.len();
                for variable in variables {
                    self.encode_expr(out, variable.domain)?;
                    self.bound.push(variable.name.clone());
                }
                self.encode_expr(out, *body)?;
                self.bound.truncate(bound_len);
            }
            ExprNode::Vector(elements) => {
                out.tag(10);
                out.usize(elements.len());
                for element in elements {
                    self.encode_expr(out, *element)?;
                }
            }
            ExprNode::Set { elements, guards } => {
                out.tag(19);
                out.usize(elements.len());
                for (element, guard) in elements.iter().zip(guards) {
                    out.bool(guard.is_some());
                    if let Some(guard) = guard {
                        self.encode_expr(out, *guard)?;
                    }
                    self.encode_expr(out, *element)?;
                }
            }
            ExprNode::Matrix(rows) => {
                out.tag(11);
                out.usize(rows.len());
                for row in rows {
                    out.usize(row.len());
                    for element in row {
                        self.encode_expr(out, *element)?;
                    }
                }
            }
            ExprNode::Tensor { shape, elements } => {
                out.tag(12);
                out.usize(shape.len());
                for extent in shape {
                    out.usize(*extent);
                }
                out.usize(elements.len());
                for element in elements {
                    self.encode_expr(out, *element)?;
                }
            }
            ExprNode::Differentiate { body, var } => {
                out.tag(13);
                self.encode_name(out, var)?;
                self.encode_expr(out, *body)?;
            }
            ExprNode::Solve { body, var } => {
                out.tag(14);
                self.encode_name(out, var)?;
                self.encode_expr(out, *body)?;
            }
            ExprNode::Optimize {
                body,
                vars,
                maximize,
            } => {
                out.tag(15);
                out.bool(*maximize);
                out.usize(vars.len());
                for var in vars {
                    self.encode_name(out, var)?;
                }
                self.encode_expr(out, *body)?;
            }
            ExprNode::SampleLimit {
                body,
                var,
                target,
                direction,
            } => {
                out.tag(16);
                self.encode_name(out, var)?;
                self.encode_expr(out, *target)?;
                self.encode_expr(out, *direction)?;
                self.encode_expr(out, *body)?;
            }
            ExprNode::Apply {
                capability,
                arguments,
            } => {
                // Capability applications encode the admitted cell name:
                // the cell is meaning, the arena slot is not.
                // RECONSTRUCTED 2026-08-29: rebuilt after an
                // accidental `git checkout --` reverted an uncommitted
                // foreign change. Diff-reviewed 2026-08-29: CONFIRMED against the
                // slot-stability differential (same cell name → same
                // meaning across different arena slots; renamed cell →
                // different meaning) and the dangling-cell pin
                // (enforced by the integrity gate in
                // `canonical_meaning_bytes`).
                let cell = self
                    .package
                    .capability(*capability)
                    .ok_or(MeaningError::MissingCapability(*capability))?;
                out.tag(17);
                out.text(&cell.name.0);
                out.usize(arguments.len());
                for argument in arguments {
                    self.encode_expr(out, *argument)?;
                }
            }
            ExprNode::Series {
                points,
                interpolation,
                extrapolation,
            } => {
                // 04 §5.4 slice 1: the pairs and the DECLARED policy are
                // identity — two series differing only in interpolation
                // mode are different artifacts.
                out.tag(18);
                out.usize(points.len());
                for (time, value) in points {
                    out.f64(*time);
                    out.f64(*value);
                }
                out.text(interpolation);
                out.text(extrapolation);
            }
        }
        self.active_exprs.remove(&id);
        Ok(())
    }
}
