//! EMIR lowering emitter: converts semantic IR expressions into a linear
//! list of [`crate::EmirOp`]s per output definition.

mod call;

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
    let value = emitter.emit(package, expr)?;
    Ok(EmirProgram {
        ops: emitter.ops,
        result: value,
        input_count: u16_count(inputs.len(), "input")?,
        state_count: u16_count(states.len(), "state")?,
        domain_obligations: emitter.obligations,
    })
}

fn u16_count(n: usize, what: &str) -> Result<u16, String> {
    u16::try_from(n).map_err(|_| format!("{what} count {n} exceeds u16::MAX"))
}

fn u16_index(n: usize, what: &str) -> Result<u16, String> {
    u16::try_from(n).map_err(|_| format!("{what} index {n} exceeds u16::MAX"))
}

pub(crate) struct Emitter {
    ops: Vec<(EmirOp, Span)>,
    inputs: Vec<String>,
    states: Vec<String>,
    obligations: Vec<DomainObligation>,
}

impl Emitter {
    fn push(&mut self, op: EmirOp, span: Span) -> Result<EmirValue, String> {
        let id = u32::try_from(self.ops.len())
            .map_err(|_| format!("EMIR op count {} exceeds u32::MAX", self.ops.len()))?;
        self.ops.push((op, span));
        Ok(EmirValue(id))
    }

    fn state_index(&self, name: &str) -> Result<u16, String> {
        self.states
            .iter()
            .position(|s| s == name)
            .ok_or_else(|| format!("unknown state field `{name}`"))
            .and_then(|i| u16_index(i, "state"))
    }

    fn input_index(&self, name: &str) -> Result<u16, String> {
        self.inputs
            .iter()
            .position(|s| s == name)
            .ok_or_else(|| format!("unknown input `{name}`"))
            .and_then(|i| u16_index(i, "input"))
    }

    /// Create a sub-emitter that shares this emitter's input and state
    /// tables.  Used by nested ops (`Differentiate`, `Solve`,
    /// `Optimize`) so the sub-program can reference the same inputs.
    fn sub_emitter(&self) -> Emitter {
        Emitter {
            ops: Vec::new(),
            inputs: self.inputs.clone(),
            states: self.states.clone(),
            obligations: Vec::new(),
        }
    }

    /// Finalize this emitter into an `EmirProgram` with the given result
    /// value.  The state count comes from the parent emitter since
    /// sub-programs share state tables.
    fn finish(mut self, result: EmirValue, state_count: usize) -> Result<EmirProgram, String> {
        Ok(EmirProgram {
            ops: std::mem::take(&mut self.ops),
            result,
            input_count: u16_count(self.inputs.len(), "input")?,
            state_count: u16_count(state_count, "state")?,
            domain_obligations: std::mem::take(&mut self.obligations),
        })
    }

    fn emit(&mut self, package: &SemanticPackage, id: EmirExprRef) -> Result<EmirValue, String> {
        let expr = package
            .expr(id)
            .ok_or_else(|| "expression id out of range".to_string())?;
        let span = package.expr_span(id);
        match expr {
            ExprNode::Literal(Literal::FloatBits(bits)) => {
                // NaN/Infinity policy: strict-f64 refuses non-finite constants
                let value = f64::from_bits(*bits);
                if !value.is_finite() {
                    return Err(format!(
                        "non-finite constant {value:?} refused under strict-f64 policy"
                    ));
                }
                self.push(EmirOp::ConstF64(*bits), span)
            }
            ExprNode::Literal(Literal::Integer(text)) => {
                let parsed: f64 = text
                    .replace('_', "")
                    .parse()
                    .map_err(|_| format!("invalid integer literal `{text}`"))?;
                if !parsed.is_finite() {
                    return Err(format!(
                        "integer literal `{text}` exceeds the strict-f64 finite range"
                    ));
                }
                let value: f64 = parsed;
                self.push(EmirOp::ConstF64(value.to_bits()), span)
            }
            ExprNode::Literal(Literal::Bool(on)) => {
                let value: f64 = if *on { 1.0 } else { 0.0 };
                self.push(EmirOp::ConstF64(value.to_bits()), span)
            }
            ExprNode::Literal(Literal::Complex { re_bits, im_bits }) => {
                let re = f64::from_bits(*re_bits);
                let im = f64::from_bits(*im_bits);
                if !re.is_finite() || !im.is_finite() {
                    return Err(format!(
                        "non-finite complex constant {{re: {re:?}, im: {im:?}}} refused under strict-f64 policy"
                    ));
                }
                self.push(EmirOp::ConstComplex(re, im), span)
            }
            ExprNode::Literal(_) => Err("unsupported literal in Phase 1 subset".to_string()),
            ExprNode::Variable(name) => {
                let name = &name.0;
                if let Some(stripped) = name.strip_prefix("state.") {
                    let index = self.state_index(stripped)?;
                    self.push(EmirOp::LoadState(index), span)
                } else {
                    let index = self.input_index(name)?;
                    self.push(EmirOp::LoadInput(index), span)
                }
            }
            ExprNode::Call {
                function,
                arguments,
            } => self.emit_call(package, &function.0, arguments, span),
            ExprNode::Unary { operation, value } => {
                let operand = self.emit(package, *value)?;
                let op = match operation {
                    UnaryOp::Negate => EmirOp::Neg(operand),
                    UnaryOp::Not => EmirOp::Not(operand),
                    UnaryOp::Sqrt => {
                        self.obligations.push(DomainObligation::SqrtNonNegative);
                        EmirOp::UnaryBuiltin(BuiltinId::Sqrt, operand)
                    }
                    UnaryOp::Exp => EmirOp::UnaryBuiltin(BuiltinId::Exp, operand),
                    UnaryOp::Log => {
                        self.obligations.push(DomainObligation::LogPositive);
                        EmirOp::UnaryBuiltin(BuiltinId::Ln, operand)
                    }
                    UnaryOp::Sin => EmirOp::UnaryBuiltin(BuiltinId::Sin, operand),
                    UnaryOp::Cos => EmirOp::UnaryBuiltin(BuiltinId::Cos, operand),
                    UnaryOp::Tan => EmirOp::UnaryBuiltin(BuiltinId::Tan, operand),
                    UnaryOp::Tanh => EmirOp::UnaryBuiltin(BuiltinId::Tanh, operand),
                    UnaryOp::Abs => EmirOp::UnaryBuiltin(BuiltinId::Abs, operand),
                    UnaryOp::Floor => EmirOp::UnaryBuiltin(BuiltinId::Floor, operand),
                    UnaryOp::Ceil => EmirOp::UnaryBuiltin(BuiltinId::Ceil, operand),
                };
                self.push(op, span)
            }
            ExprNode::Binary {
                operation,
                left,
                right,
            } => {
                let l = self.emit(package, *left)?;
                let r = self.emit(package, *right)?;
                let op = match operation {
                    BinaryOp::StrictFloatAdd => EmirOp::F64Add(l, r),
                    BinaryOp::StrictFloatSub => EmirOp::F64Sub(l, r),
                    BinaryOp::StrictFloatMul => EmirOp::F64Mul(l, r),
                    BinaryOp::StrictFloatDiv => {
                        self.obligations.push(DomainObligation::DivisionNonZero);
                        EmirOp::F64Div(l, r)
                    }
                    BinaryOp::StrictFloatPow => {
                        self.obligations.push(DomainObligation::PowFiniteResult);
                        EmirOp::F64Pow(l, r)
                    }
                    BinaryOp::Equal => EmirOp::Eq(l, r),
                    BinaryOp::NotEqual => EmirOp::Ne(l, r),
                    BinaryOp::Less => EmirOp::Lt(l, r),
                    BinaryOp::LessEqual => EmirOp::Le(l, r),
                    BinaryOp::Greater => EmirOp::Gt(l, r),
                    BinaryOp::GreaterEqual => EmirOp::Ge(l, r),
                    BinaryOp::And => EmirOp::And(l, r),
                    BinaryOp::Or => EmirOp::Or(l, r),
                    BinaryOp::Imply => EmirOp::Imply(l, r),
                    BinaryOp::Iff => EmirOp::Iff(l, r),
                    BinaryOp::Min => EmirOp::BinaryBuiltin(BuiltinId::Min, l, r),
                    BinaryOp::Max => EmirOp::BinaryBuiltin(BuiltinId::Max, l, r),
                    BinaryOp::Atan2 => EmirOp::BinaryBuiltin(BuiltinId::Atan2, l, r),
                    BinaryOp::VectorAdd => EmirOp::VectorAdd(l, r),
                    BinaryOp::VectorSub => EmirOp::VectorSub(l, r),
                    BinaryOp::VectorScale => EmirOp::VectorScale(l, r),
                    BinaryOp::VectorDot => EmirOp::VectorDot(l, r),
                    BinaryOp::MatrixAdd => EmirOp::MatrixAdd(l, r),
                    BinaryOp::MatrixSub => EmirOp::MatrixSub(l, r),
                    BinaryOp::MatrixScale => EmirOp::MatrixScale(l, r),
                    BinaryOp::MatrixMulVector => EmirOp::MatrixMulVector(l, r),
                    BinaryOp::MatrixMulMatrix => EmirOp::MatrixMulMatrix(l, r),
                    BinaryOp::TensorAdd => EmirOp::TensorAdd(l, r),
                    BinaryOp::TensorSub => EmirOp::TensorSub(l, r),
                    BinaryOp::ExactAdd
                    | BinaryOp::ExactSub
                    | BinaryOp::ExactMul
                    | BinaryOp::ExactDiv => {
                        return Err(
                            "exact arithmetic is outside the Phase 1 strict-f64 subset".to_string()
                        );
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
            ExprNode::Vector(elements) => {
                let mut emitted = Vec::with_capacity(elements.len());
                for &element in elements {
                    emitted.push(self.emit(package, element)?);
                }
                self.push(EmirOp::VectorCreate(emitted), span)
            }
            ExprNode::Matrix(rows) => {
                let r = rows.len();
                let c = rows.first().map_or(0, |row| row.len());
                if rows.iter().any(|row| row.len() != c) {
                    return Err("jagged matrix rows (unequal column counts)".into());
                }
                let mut elements = Vec::with_capacity(r.saturating_mul(c));
                for row in rows {
                    for &element in row {
                        elements.push(self.emit(package, element)?);
                    }
                }
                self.push(
                    EmirOp::MatrixCreate {
                        rows: r,
                        cols: c,
                        elements,
                    },
                    span,
                )
            }
            ExprNode::Tensor { shape, elements } => {
                let expected = shape
                    .iter()
                    .try_fold(1usize, |acc, &dim| acc.checked_mul(dim));
                let Some(expected) = expected else {
                    return Err("tensor shape product overflow".into());
                };
                if elements.len() != expected {
                    return Err("tensor element count does not match shape product".into());
                }
                let mut emitted = Vec::with_capacity(elements.len());
                for &element in elements {
                    emitted.push(self.emit(package, element)?);
                }
                self.push(
                    EmirOp::TensorCreate {
                        shape: shape.clone(),
                        elements: emitted,
                    },
                    span,
                )
            }
            ExprNode::Index { value, indices } => {
                let target = self.emit(package, *value)?;
                if indices.len() == 1 {
                    let idx = self.emit(package, indices[0])?;
                    self.push(EmirOp::VectorIndex { vector: target, index: idx }, span)
                } else if indices.len() == 2 {
                    let row = self.emit(package, indices[0])?;
                    let col = self.emit(package, indices[1])?;
                    self.push(EmirOp::MatrixIndex { matrix: target, row, col }, span)
                } else {
                    let mut emitted = Vec::with_capacity(indices.len());
                    for &index in indices {
                        emitted.push(self.emit(package, index)?);
                    }
                    self.push(
                        EmirOp::TensorIndex {
                            tensor: target,
                            indices: emitted,
                        },
                        span,
                    )
                }
            }
            ExprNode::Slice { value, axes } => {
                let target = self.emit(package, *value)?;
                let mut emitted = Vec::with_capacity(axes.len());
                for axis in axes {
                    emitted.push(match axis {
                        SliceAxis::Point(index) => EmirSliceAxis::Point(self.emit(package, *index)?),
                        SliceAxis::Range { start, end } => EmirSliceAxis::Range {
                            start: self.emit(package, *start)?,
                            end: self.emit(package, *end)?,
                        },
                    });
                }
                self.push(
                    EmirOp::TensorSlice {
                        tensor: target,
                        axes: emitted,
                    },
                    span,
                )
            }
            ExprNode::Binder {
                kind,
                variables,
                body,
            } => {
                if variables.len() != 1 {
                    return Err(
                        "only a single binder variable is computed".to_string()
                    );
                }
                let binder = &variables[0];
                let domain_expr = package
                    .expr(binder.domain)
                    .ok_or_else(|| "binder domain out of range".to_string())?;
                let (start_id, end_id) = match domain_expr {
                    ExprNode::Vector(els) if els.len() == 2 => (els[0], els[1]),
                    _ => {
                        return Err(
                            "binder domain must be a range vector".to_string()
                        )
                    }
                };
                let start_val = self.emit(package, start_id)?;
                let end_val = self.emit(package, end_id)?;
                let loop_var_index = u16_index(self.inputs.len(), "loop variable")?;
                let mut body_inputs = self.inputs.clone();
                body_inputs.push(binder.name.clone());
                let mut body_emitter = Emitter {
                    ops: Vec::new(),
                    inputs: body_inputs,
                    states: self.states.clone(),
                    obligations: Vec::new(),
                };
                let body_result = body_emitter.emit(package, *body)?;
                let body_program = EmirProgram {
                    ops: std::mem::take(&mut body_emitter.ops),
                    result: body_result,
                    input_count: u16_count(body_emitter.inputs.len(), "input")?,
                    state_count: u16_count(self.states.len(), "state")?,
                    domain_obligations: std::mem::take(&mut body_emitter.obligations),
                };
                match kind {
                    BinderKind::Sum
                    | BinderKind::Product
                    | BinderKind::ForAll
                    | BinderKind::Exists => {
                        let combine = match kind {
                            BinderKind::Sum => FoldCombine::Add,
                            BinderKind::Product => FoldCombine::Mul,
                            BinderKind::ForAll => FoldCombine::And,
                            BinderKind::Exists => FoldCombine::Or,
                            BinderKind::Integral => {
                                return Err(
                                    "integral binder must lower via Integral op".to_string(),
                                );
                            }
                            BinderKind::Series => {
                                return Err(
                                    "series binder is a claim, not a computation".to_string(),
                                );
                            }
                        };
                        let init_val = match combine {
                            FoldCombine::Add => self.push(EmirOp::ConstI64(0), span)?,
                            FoldCombine::Mul => self.push(EmirOp::ConstI64(1), span)?,
                            FoldCombine::And => {
                                self.push(EmirOp::ConstF64(1.0f64.to_bits()), span)?
                            }
                            FoldCombine::Or => {
                                self.push(EmirOp::ConstF64(0.0f64.to_bits()), span)?
                            }
                        };
                        self.push(
                            EmirOp::Fold {
                                start: start_val,
                                end: end_val,
                                init: init_val,
                                combine,
                                loop_var_index,
                                body: body_program,
                            },
                            span,
                        )
                    }
                    BinderKind::Integral => self.push(
                        EmirOp::Integral {
                            start: start_val,
                            end: end_val,
                            steps: 1000,
                            loop_var_index,
                            integrand: body_program,
                        },
                        span,
                    ),
                    BinderKind::Series => {
                        return Err("series binder is a claim, not a computation".to_string());
                    }
                }
            }
            ExprNode::Differentiate { body, var } => {
                let var_index = self.input_index(var)?;
                let mut body_emitter = Emitter {
                    ops: Vec::new(),
                    inputs: self.inputs.clone(),
                    states: self.states.clone(),
                    obligations: Vec::new(),
                };
                let body_result = body_emitter.emit(package, *body)?;
                let body_program = EmirProgram {
                    ops: std::mem::take(&mut body_emitter.ops),
                    result: body_result,
                    input_count: u16_count(body_emitter.inputs.len(), "input")?,
                    state_count: u16_count(self.states.len(), "state")?,
                    domain_obligations: std::mem::take(&mut body_emitter.obligations),
                };
                self.push(
                    EmirOp::Differentiate {
                        body: body_program,
                        var_index,
                    },
                    span,
                )
            }
            ExprNode::Solve { body, var } => {
                let var_index = self.input_index(var)?;
                let sc = self.states.len();
                let mut body_emitter = self.sub_emitter();
                let body_result = body_emitter.emit(package, *body)?;
                let body_program = body_emitter.finish(body_result, sc)?;
                self.push(
                    EmirOp::Solve {
                        body: body_program,
                        var_index,
                        tolerance: 1e-12,
                        max_iter: 100,
                    },
                    span,
                )
            }
            ExprNode::Optimize { body, vars, maximize } => {
                let mut var_indices = Vec::with_capacity(vars.len());
                for var in vars {
                    var_indices.push(self.input_index(var)?);
                }
                let sc = self.states.len();
                let mut body_emitter = self.sub_emitter();
                let body_result = body_emitter.emit(package, *body)?;
                let body_program = body_emitter.finish(body_result, sc)?;
                self.push(
                    EmirOp::Optimize {
                        body: body_program,
                        var_indices,
                        maximize: *maximize,
                        learning_rate: 0.01,
                        tolerance: 1e-6,
                        max_iter: 1000,
                    },
                    span,
                )
            }
            ExprNode::SampleLimit { body, var, target, direction } => {
                // The limit variable may be a declared input (sampled in
                // place) or binder-introduced like a fold loop variable
                // (registered as a phantom input at the end of the body's
                // input table, mirroring the Binder arm).
                let var_index = match self.inputs.iter().position(|n| n == var) {
                    Some(pos) => u16_index(pos, "input")?,
                    None => u16_index(self.inputs.len(), "limit variable")?,
                };
                let target_val = self.emit(package, *target)?;
                let direction_val = self.emit(package, *direction)?;
                let mut body_inputs = self.inputs.clone();
                if body_inputs.len() as u16 <= var_index {
                    body_inputs.push(var.clone());
                }
                let mut body_emitter = Emitter {
                    ops: Vec::new(),
                    inputs: body_inputs,
                    states: self.states.clone(),
                    obligations: Vec::new(),
                };
                let body_result = body_emitter.emit(package, *body)?;
                let body_program = EmirProgram {
                    ops: std::mem::take(&mut body_emitter.ops),
                    result: body_result,
                    input_count: u16_count(body_emitter.inputs.len(), "input")?,
                    state_count: u16_count(self.states.len(), "state")?,
                    domain_obligations: std::mem::take(&mut body_emitter.obligations),
                };
                self.push(
                    EmirOp::SampleLimit {
                        body: body_program,
                        var_index,
                        target: target_val,
                        direction: direction_val,
                    },
                    span,
                )
            }
            other => Err(format!(
                "expression form {:?} is outside the Phase 1 strict-f64 subset",
                std::mem::discriminant(other)
            )),
        }
    }
}
