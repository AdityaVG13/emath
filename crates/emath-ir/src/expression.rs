//! Provider-free semantic expressions (SIR expressions) with typed numeric
//! operations. Phase 1 lowers to strict Float64; the operation vocabulary is
//! the canonical set for exact/wrapping/checked/fast semantics.

use crate::ids::{ExprId, TypeId};
use emath_core::QualifiedName;

#[derive(Clone, Debug, PartialEq)]
pub enum Literal {
    Bool(bool),
    /// Integer spelling (canonical decimal, no underscores) or `NaN`/`Inf`.
    Integer(String),
    Rational(String),
    FloatBits(u64),
    /// Complex constant stored as IEEE-754 bit patterns. B14.
    Complex {
        re_bits: u64,
        im_bits: u64,
    },
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprNode {
    Literal(Literal),
    Variable(QualifiedName),
    Call {
        function: QualifiedName,
        arguments: Vec<ExprId>,
    },
    Unary {
        operation: UnaryOp,
        value: ExprId,
    },
    Binary {
        operation: BinaryOp,
        left: ExprId,
        right: ExprId,
    },
    If {
        condition: ExprId,
        then_value: ExprId,
        else_value: ExprId,
    },
    Record {
        ty: TypeId,
        fields: std::collections::BTreeMap<String, ExprId>,
    },
    Index {
        value: ExprId,
        indices: Vec<ExprId>,
    },
    /// Rank-preserving or mixed slice. Point axes drop rank; range axes keep it.
    Slice {
        value: ExprId,
        axes: Vec<SliceAxis>,
    },
    Binder {
        kind: BinderKind,
        variables: Vec<BinderVariable>,
        body: ExprId,
    },
    Vector(Vec<ExprId>),
    Matrix(Vec<Vec<ExprId>>),
    /// Rank-3+ tensor of Float64, stored in row-major nested order.
    Tensor {
        shape: Vec<usize>,
        elements: Vec<ExprId>,
    },
    /// Forward-mode autodiff: derivative of `body` wrt input `var`;
    /// the EMIR emitter lowers it to `EmirOp::Differentiate`.
    Differentiate {
        body: ExprId,
        var: String,
    },
    /// Newton's-method root-finding: value of input `var` driving
    /// `body` to zero; forward-mode autodiff for the Jacobian step.
    Solve {
        body: ExprId,
        var: String,
    },
    /// Newton-on-∇f optimization: values of inputs `vars` at a
    /// stationary point of `body`; dual-number gradient, FD Hessian.
    Optimize {
        body: ExprId,
        vars: Vec<String>,
        maximize: bool,
    },
    /// Numerical limit approximation (B04): samples `body` approaching
    /// `target` from `direction` (0 = two-sided, 1 = above, -1 = below).
    SampleLimit {
        body: ExprId,
        var: String,
        target: ExprId,
        direction: ExprId,
    },
}

/// One axis of [`ExprNode::Slice`]: a scalar point or a half-open range.
#[derive(Clone, Debug, PartialEq)]
pub enum SliceAxis {
    Point(ExprId),
    Range { start: ExprId, end: ExprId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Negate,
    Not,
    Sqrt,
    Exp,
    Log,
    Sin,
    Cos,
    Tan,
    Tanh,
    Abs,
    Floor,
    Ceil,
}

impl UnaryOp {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Negate => "neg",
            Self::Not => "not",
            Self::Sqrt => "sqrt",
            Self::Exp => "exp",
            Self::Log => "ln",
            Self::Sin => "sin",
            Self::Cos => "cos",
            Self::Tan => "tan",
            Self::Tanh => "tanh",
            Self::Abs => "abs",
            Self::Floor => "floor",
            Self::Ceil => "ceil",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    /// Exact integer arithmetic (not lowered in Phase 1).
    ExactAdd,
    ExactSub,
    ExactMul,
    ExactDiv,
    /// Strict IEEE-754 Float64 arithmetic.
    StrictFloatAdd,
    StrictFloatSub,
    StrictFloatMul,
    StrictFloatDiv,
    StrictFloatPow,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
    /// `==>` — logical implication: `!a || b`.
    Imply,
    /// `<==>` — logical biconditional: `a == b` for Bool.
    Iff,
    Min,
    Max,
    Atan2,
    VectorAdd,
    VectorSub,
    VectorScale,
    VectorDot,
    MatrixAdd,
    MatrixSub,
    MatrixScale,
    MatrixMulVector,
    MatrixMulMatrix,
    TensorAdd,
    TensorSub,
}

impl BinaryOp {
    pub const fn name(self) -> &'static str {
        match self {
            Self::ExactAdd => "exact-add",
            Self::ExactSub => "exact-sub",
            Self::ExactMul => "exact-mul",
            Self::ExactDiv => "exact-div",
            Self::StrictFloatAdd => "f64-add",
            Self::StrictFloatSub => "f64-sub",
            Self::StrictFloatMul => "f64-mul",
            Self::StrictFloatDiv => "f64-div",
            Self::StrictFloatPow => "f64-pow",
            Self::Equal => "f64-eq",
            Self::NotEqual => "f64-ne",
            Self::Less => "f64-lt",
            Self::LessEqual => "f64-le",
            Self::Greater => "f64-gt",
            Self::GreaterEqual => "f64-ge",
            Self::And => "bool-and",
            Self::Or => "bool-or",
            Self::Imply => "bool-imply",
            Self::Iff => "bool-iff",
            Self::Min => "f64-min",
            Self::Max => "f64-max",
            Self::Atan2 => "f64-atan2",
            Self::VectorAdd => "vec-add",
            Self::VectorSub => "vec-sub",
            Self::VectorScale => "vec-scale",
            Self::VectorDot => "vec-dot",
            Self::MatrixAdd => "mat-add",
            Self::MatrixSub => "mat-sub",
            Self::MatrixScale => "mat-scale",
            Self::MatrixMulVector => "mat-mul-vec",
            Self::MatrixMulMatrix => "mat-mul-mat",
            Self::TensorAdd => "tensor-add",
            Self::TensorSub => "tensor-sub",
        }
    }

    #[must_use]
    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            Self::Equal
                | Self::NotEqual
                | Self::Less
                | Self::LessEqual
                | Self::Greater
                | Self::GreaterEqual
        )
    }

    #[must_use]
    pub fn is_boolean(self) -> bool {
        self.is_comparison() || matches!(self, Self::And | Self::Or)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinderKind {
    Sum,
    Product,
    Integral,
    ForAll,
    Exists,
    /// `series n in 0..inf: a[n]` — series convergence claim (B06).
    Series,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BinderVariable {
    pub name: String,
    pub domain: ExprId,
}

/// Well-known function names reachable as `QualifiedName`s.
pub mod builtins {
    use emath_core::QualifiedName;

    pub const IS_FINITE: &str = "core::math::is_finite";
    pub const POW: &str = "core::math::pow";

    #[must_use]
    pub fn is_finite() -> QualifiedName {
        QualifiedName(IS_FINITE.to_string())
    }

    #[must_use]
    pub fn pow() -> QualifiedName {
        QualifiedName(POW.to_string())
    }
}

impl ExprNode {
    /// Collect variable names bound/free for scope checks (free traversal).
    #[must_use]
    pub fn free_variables(&self, exprs: &[ExprNode]) -> Vec<QualifiedName> {
        let mut out = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        self.collect_free(exprs, &mut seen, &mut out);
        out
    }

    fn collect_free(
        &self,
        exprs: &[ExprNode],
        seen: &mut std::collections::BTreeSet<String>,
        out: &mut Vec<QualifiedName>,
    ) {
        match self {
            Self::Variable(name) => {
                if seen.insert(name.0.clone()) {
                    out.push(name.clone());
                }
            }
            Self::Call {
                function,
                arguments,
            } => {
                let _ = function;
                for &arg in arguments {
                    exprs[arg.index()].collect_free(exprs, seen, out);
                }
            }
            Self::Unary { value, .. } => exprs[value.index()].collect_free(exprs, seen, out),
            Self::Binary { left, right, .. } => {
                exprs[left.index()].collect_free(exprs, seen, out);
                exprs[right.index()].collect_free(exprs, seen, out);
            }
            Self::If {
                condition,
                then_value,
                else_value,
            } => {
                exprs[condition.index()].collect_free(exprs, seen, out);
                exprs[then_value.index()].collect_free(exprs, seen, out);
                exprs[else_value.index()].collect_free(exprs, seen, out);
            }
            Self::Record { fields, .. } => {
                for &field in fields.values() {
                    exprs[field.index()].collect_free(exprs, seen, out);
                }
            }
            Self::Index { value, indices } => {
                exprs[value.index()].collect_free(exprs, seen, out);
                for &index in indices {
                    exprs[index.index()].collect_free(exprs, seen, out);
                }
            }
            Self::Slice { value, axes } => {
                exprs[value.index()].collect_free(exprs, seen, out);
                for axis in axes {
                    match axis {
                        SliceAxis::Point(index) => {
                            exprs[index.index()].collect_free(exprs, seen, out);
                        }
                        SliceAxis::Range { start, end } => {
                            exprs[start.index()].collect_free(exprs, seen, out);
                            exprs[end.index()].collect_free(exprs, seen, out);
                        }
                    }
                }
            }
            Self::Binder {
                variables, body, ..
            } => {
                // Bound names come into scope left-to-right: each variable's
                // domain may mention earlier bound names; the body sees all
                // of them. Bound names are never reported as free.
                let mut bound = std::collections::BTreeSet::new();
                for variable in variables {
                    exprs[variable.domain.index()].collect_free(exprs, seen, out);
                    bound.insert(variable.name.clone());
                }
                for name in &bound {
                    seen.insert(name.clone());
                }
                exprs[body.index()].collect_free(exprs, seen, out);
            }
            Self::Vector(elements) => {
                for &element in elements {
                    exprs[element.index()].collect_free(exprs, seen, out);
                }
            }
            Self::Matrix(rows) => {
                for row in rows {
                    for &element in row {
                        exprs[element.index()].collect_free(exprs, seen, out);
                    }
                }
            }
            Self::Tensor { elements, .. } => {
                for &element in elements {
                    exprs[element.index()].collect_free(exprs, seen, out);
                }
            }
            Self::Literal(_) => {}
            Self::Differentiate { body, .. }
            | Self::Solve { body, .. }
            | Self::Optimize { body, .. } => {
                exprs[body.index()].collect_free(exprs, seen, out);
            }
            Self::SampleLimit { body, target, .. } => {
                exprs[target.index()].collect_free(exprs, seen, out);
                exprs[body.index()].collect_free(exprs, seen, out);
            }
        }
    }
}
