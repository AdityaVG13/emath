//! Lower semantic expressions to the universal executable machine.

use emath_core::Span;
use emath_ir::{BinaryOp, BinderKind, ExprNode, Literal, SemanticPackage, SliceAxis, UnaryOp};

use crate::{
    BuiltinId, DomainObligation, EmirExprRef, EmirOp, EmirProgram, EmirSliceAxis, EmirValue,
    FoldCombine,
};

pub(crate) fn lower(
    package: &SemanticPackage,
    expr: EmirExprRef,
    inputs: &[String],
    states: &[String],
) -> Result<EmirProgram, String> {
    let mut emitter = Emitter {
        ops: Vec::new(),
        inputs: inputs.to_vec(),
        states: states.to_vec(),
        obligations: Vec::new(),
    };
    let result = emitter.emit(package, expr)?;
    Ok(EmirProgram {
        ops: emitter.ops,
        result,
        input_count: count_u16(inputs.len(), "input")?,
        state_count: count_u16(states.len(), "state")?,
        domain_obligations: emitter.obligations,
    })
}

fn count_u16(value: usize, subject: &str) -> Result<u16, String> {
    u16::try_from(value).map_err(|_| format!("{subject} count {value} exceeds u16::MAX"))
}

struct Emitter {
    ops: Vec<(EmirOp, Span)>,
    inputs: Vec<String>,
    states: Vec<String>,
    obligations: Vec<DomainObligation>,
}

impl Emitter {
    fn push(&mut self, op: EmirOp, span: Span) -> Result<EmirValue, String> {
        let index = u32::try_from(self.ops.len())
            .map_err(|_| format!("EMIR op count {} exceeds u32::MAX", self.ops.len()))?;
        self.ops.push((op, span));
        Ok(EmirValue(index))
    }

    fn input_index(&self, name: &str) -> Result<u16, String> {
        self.inputs
            .iter()
            .rposition(|candidate| candidate == name)
            .ok_or_else(|| format!("unknown input `{name}`"))
            .and_then(|index| count_u16(index, "input index"))
    }

    fn state_index(&self, name: &str) -> Result<u16, String> {
        self.states
            .iter()
            .position(|candidate| candidate == name)
            .ok_or_else(|| format!("unknown state field `{name}`"))
            .and_then(|index| count_u16(index, "state index"))
    }

    fn emit(&mut self, package: &SemanticPackage, id: EmirExprRef) -> Result<EmirValue, String> {
        let node = package
            .expr(id)
            .ok_or_else(|| "expression id out of range".to_string())?;
        let span = package.expr_span(id);
        match node {
            ExprNode::Literal(Literal::FloatBits(bits)) => {
                let value = f64::from_bits(*bits);
                if !value.is_finite() {
                    return Err("non-finite constant refused under strict-f64 policy".to_string());
                }
                self.push(EmirOp::ConstF64(*bits), span)
            }
            ExprNode::Literal(Literal::Integer(text)) => {
                let spelling = text.replace('_', "");
                if let Ok(value) = spelling.parse::<i64>() {
                    self.push(EmirOp::ConstI64(value), span)
                } else {
                    let value = emath_rt::UBig::parse_decimal(&spelling)
                        .map_err(|_| format!("invalid integer literal `{text}`"))?;
                    if value.bits() > emath_rt::LIMIT_BITS {
                        return Err(
                            "integer literal exceeds the executable carrier bound".to_string()
                        );
                    }
                    self.push(EmirOp::ConstBigInt(value.to_decimal()), span)
                }
            }
            ExprNode::Literal(Literal::Bool(value)) => self.push(EmirOp::ConstBool(*value), span),
            ExprNode::Literal(Literal::Text(value)) => {
                self.push(EmirOp::ConstText(value.clone()), span)
            }
            ExprNode::Literal(Literal::Complex { re_bits, im_bits }) => {
                let re = f64::from_bits(*re_bits);
                let im = f64::from_bits(*im_bits);
                if !re.is_finite() || !im.is_finite() {
                    return Err("non-finite complex constant refused".to_string());
                }
                self.push(EmirOp::ConstComplex(re, im), span)
            }
            ExprNode::Literal(Literal::Rational(_)) => {
                Err("rational construction must be a capability application".to_string())
            }
            ExprNode::Variable(name) => {
                if let Some(state) = name.0.strip_prefix("state.") {
                    self.push(EmirOp::LoadState(self.state_index(state)?), span)
                } else {
                    self.push(EmirOp::LoadInput(self.input_index(&name.0)?), span)
                }
            }
            ExprNode::Apply {
                capability,
                arguments,
            } => {
                let admitted = package.capability(*capability).ok_or_else(|| {
                    format!("capability cell id {} is not interned", capability.index())
                })?;
                let mut args = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    args.push(self.emit(package, *argument)?);
                }
                self.push(
                    EmirOp::ApplyCapability {
                        capability: admitted.name.0.clone(),
                        class: admitted.class,
                        args,
                    },
                    span,
                )
            }
            ExprNode::Call { function, .. } => Err(format!(
                "legacy named call `{}` reached executable lowering; admission must resolve a FeatureID application",
                function.0
            )),
            ExprNode::Unary { operation, value } => {
                let value = self.emit(package, *value)?;
                let op = match operation {
                    UnaryOp::Negate => EmirOp::Neg(value),
                    UnaryOp::Not => EmirOp::Not(value),
                    UnaryOp::Sqrt => EmirOp::UnaryBuiltin(BuiltinId::Sqrt, value),
                    UnaryOp::Exp => EmirOp::UnaryBuiltin(BuiltinId::Exp, value),
                    UnaryOp::Log => EmirOp::UnaryBuiltin(BuiltinId::Ln, value),
                    UnaryOp::Sin => EmirOp::UnaryBuiltin(BuiltinId::Sin, value),
                    UnaryOp::Cos => EmirOp::UnaryBuiltin(BuiltinId::Cos, value),
                    UnaryOp::Tan => EmirOp::UnaryBuiltin(BuiltinId::Tan, value),
                    UnaryOp::Tanh => EmirOp::UnaryBuiltin(BuiltinId::Tanh, value),
                    UnaryOp::Abs => EmirOp::UnaryBuiltin(BuiltinId::Abs, value),
                    UnaryOp::Floor => EmirOp::UnaryBuiltin(BuiltinId::Floor, value),
                    UnaryOp::Ceil => EmirOp::UnaryBuiltin(BuiltinId::Ceil, value),
                };
                self.push(op, span)
            }
            ExprNode::Binary {
                operation,
                left,
                right,
            } => {
                let left = self.emit(package, *left)?;
                let right = self.emit(package, *right)?;
                let op = match operation {
                    BinaryOp::StrictFloatAdd => EmirOp::F64Add(left, right),
                    BinaryOp::StrictFloatSub => EmirOp::F64Sub(left, right),
                    BinaryOp::StrictFloatMul => EmirOp::F64Mul(left, right),
                    BinaryOp::StrictFloatDiv => EmirOp::F64Div(left, right),
                    BinaryOp::StrictFloatPow => EmirOp::F64Pow(left, right),
                    BinaryOp::Equal => EmirOp::Eq(left, right),
                    BinaryOp::NotEqual => EmirOp::Ne(left, right),
                    BinaryOp::Less => EmirOp::Lt(left, right),
                    BinaryOp::LessEqual => EmirOp::Le(left, right),
                    BinaryOp::Greater => EmirOp::Gt(left, right),
                    BinaryOp::GreaterEqual => EmirOp::Ge(left, right),
                    BinaryOp::And => EmirOp::And(left, right),
                    BinaryOp::Or => EmirOp::Or(left, right),
                    BinaryOp::Imply => EmirOp::Imply(left, right),
                    BinaryOp::Iff => EmirOp::Iff(left, right),
                    BinaryOp::SetContains => EmirOp::SetContains {
                        element: left,
                        set: right,
                    },
                    BinaryOp::Min => EmirOp::BinaryBuiltin(BuiltinId::Min, left, right),
                    BinaryOp::Max => EmirOp::BinaryBuiltin(BuiltinId::Max, left, right),
                    BinaryOp::Atan2 => EmirOp::BinaryBuiltin(BuiltinId::Atan2, left, right),
                    other => {
                        return Err(format!(
                            "legacy operation {:?} reached executable lowering; admission must resolve a FeatureID application",
                            other
                        ));
                    }
                };
                self.push(op, span)
            }
            ExprNode::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition = self.emit(package, *condition)?;
                let then_value = self.emit(package, *then_value)?;
                let else_value = self.emit(package, *else_value)?;
                self.push(
                    EmirOp::Select {
                        condition,
                        then_value,
                        else_value,
                    },
                    span,
                )
            }
            ExprNode::Series {
                points,
                interpolation,
                extrapolation,
            } => self.push(
                EmirOp::SeriesCreate {
                    points: points.clone(),
                    interpolation: interpolation.clone(),
                    extrapolation: extrapolation.clone(),
                },
                span,
            ),
            ExprNode::Set { elements, guards } => {
                let mut values = Vec::with_capacity(elements.len());
                for element in elements {
                    values.push(self.emit(package, *element)?);
                }
                let mut conditions = Vec::with_capacity(guards.len());
                for guard in guards {
                    conditions.push(match guard {
                        Some(guard) => Some(self.emit(package, *guard)?),
                        None => None,
                    });
                }
                self.push(
                    EmirOp::SetCreate {
                        elements: values,
                        guards: conditions,
                    },
                    span,
                )
            }
            ExprNode::Record { ty, fields } => {
                let type_name = match package.ty(*ty) {
                    Some(emath_ir::TypeNode::Record(name)) => name.0.clone(),
                    _ => return Err("record expression has no nominal record type".to_string()),
                };
                let mut values = Vec::with_capacity(fields.len());
                for (name, value) in fields {
                    values.push((name.clone(), self.emit(package, *value)?));
                }
                self.push(
                    EmirOp::RecordCreate {
                        type_name,
                        fields: values,
                    },
                    span,
                )
            }
            ExprNode::Vector(elements) => {
                let mut values = Vec::with_capacity(elements.len());
                for element in elements {
                    values.push(self.emit(package, *element)?);
                }
                self.push(EmirOp::VectorCreate(values), span)
            }
            ExprNode::Matrix(rows) => {
                let row_count = rows.len();
                let col_count = rows.first().map_or(0, Vec::len);
                if rows.iter().any(|row| row.len() != col_count) {
                    return Err("jagged matrix rows".to_string());
                }
                let mut values = Vec::with_capacity(row_count.saturating_mul(col_count));
                for row in rows {
                    for value in row {
                        values.push(self.emit(package, *value)?);
                    }
                }
                self.push(
                    EmirOp::MatrixCreate {
                        rows: row_count,
                        cols: col_count,
                        elements: values,
                    },
                    span,
                )
            }
            ExprNode::Tensor { shape, elements } => {
                let expected = shape
                    .iter()
                    .try_fold(1usize, |product, value| product.checked_mul(*value))
                    .ok_or_else(|| "tensor shape product overflow".to_string())?;
                if expected != elements.len() {
                    return Err("tensor element count does not match shape".to_string());
                }
                let mut values = Vec::with_capacity(elements.len());
                for value in elements {
                    values.push(self.emit(package, *value)?);
                }
                self.push(
                    EmirOp::TensorCreate {
                        shape: shape.clone(),
                        elements: values,
                    },
                    span,
                )
            }
            ExprNode::Index { value, indices } => {
                let value = self.emit(package, *value)?;
                let mut emitted = Vec::with_capacity(indices.len());
                for index in indices {
                    emitted.push(self.emit(package, *index)?);
                }
                match emitted.as_slice() {
                    [index] => self.push(
                        EmirOp::VectorIndex {
                            vector: value,
                            index: *index,
                        },
                        span,
                    ),
                    [row, col] => self.push(
                        EmirOp::MatrixIndex {
                            matrix: value,
                            row: *row,
                            col: *col,
                        },
                        span,
                    ),
                    _ => self.push(
                        EmirOp::TensorIndex {
                            tensor: value,
                            indices: emitted,
                        },
                        span,
                    ),
                }
            }
            ExprNode::Slice { value, axes } => {
                let value = self.emit(package, *value)?;
                let mut emitted = Vec::with_capacity(axes.len());
                for axis in axes {
                    emitted.push(match axis {
                        SliceAxis::Point(index) => {
                            EmirSliceAxis::Point(self.emit(package, *index)?)
                        }
                        SliceAxis::Range { start, end } => EmirSliceAxis::Range {
                            start: self.emit(package, *start)?,
                            end: self.emit(package, *end)?,
                        },
                    });
                }
                self.push(
                    EmirOp::TensorSlice {
                        tensor: value,
                        axes: emitted,
                    },
                    span,
                )
            }
            ExprNode::Binder {
                kind,
                variables,
                body,
            } => self.emit_fold(package, *kind, variables, *body, span),
            ExprNode::Differentiate { .. }
            | ExprNode::Solve { .. }
            | ExprNode::Optimize { .. }
            | ExprNode::SampleLimit { .. } => Err(
                "domain computation reached executable lowering without a FeatureID application"
                    .to_string(),
            ),
        }
    }

    fn emit_fold(
        &mut self,
        package: &SemanticPackage,
        kind: BinderKind,
        variables: &[emath_ir::BinderVariable],
        body: EmirExprRef,
        span: Span,
    ) -> Result<EmirValue, String> {
        let combine = match kind {
            BinderKind::Sum => FoldCombine::Add,
            BinderKind::Product => FoldCombine::Mul,
            BinderKind::ForAll => FoldCombine::And,
            BinderKind::Exists => FoldCombine::Or,
            BinderKind::Integral | BinderKind::Series => {
                return Err("binder computation requires a FeatureID application".to_string());
            }
        };
        let [binder] = variables else {
            return Err("only a single machine fold variable is supported".to_string());
        };
        let Some(ExprNode::Vector(bounds)) = package.expr(binder.domain) else {
            return Err("fold domain must be a two-element range vector".to_string());
        };
        let [start, end] = bounds.as_slice() else {
            return Err("fold domain must be a two-element range vector".to_string());
        };
        let start = self.emit(package, *start)?;
        let end = self.emit(package, *end)?;
        let loop_var_index = count_u16(self.inputs.len(), "loop variable index")?;
        let mut body_emitter = Emitter {
            ops: Vec::new(),
            inputs: {
                let mut inputs = self.inputs.clone();
                inputs.push(binder.name.clone());
                inputs
            },
            states: self.states.clone(),
            obligations: Vec::new(),
        };
        let result = body_emitter.emit(package, body)?;
        let body = EmirProgram {
            ops: body_emitter.ops,
            result,
            input_count: loop_var_index.saturating_add(1),
            state_count: count_u16(self.states.len(), "state")?,
            domain_obligations: body_emitter.obligations,
        };
        let init = match combine {
            FoldCombine::Add => self.push(EmirOp::ConstI64(0), span)?,
            FoldCombine::Mul => self.push(EmirOp::ConstI64(1), span)?,
            FoldCombine::And => self.push(EmirOp::ConstBool(true), span)?,
            FoldCombine::Or => self.push(EmirOp::ConstBool(false), span)?,
        };
        self.push(
            EmirOp::Fold {
                start,
                end,
                init,
                combine,
                loop_var_index,
                body,
            },
            span,
        )
    }
}
