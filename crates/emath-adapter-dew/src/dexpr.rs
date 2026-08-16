//!: Dew neutral expression mapping.
//!
//! Exact scalar mapping over the strict-Float64 subset (bit-exact
//! literals, IEEE-754 strict arithmetic, conditionals, one-to-one
//! function naming) and explicit linear-algebra mapping for fixed
//! vectors/matrices with shape and layout conversions. Unsupported
//! emath nodes are refused (`E-PROV-030`) before Dew execution —
//! nothing is silently accepted (gate 2).

use emath_core::QualifiedName;
use emath_ir::{BinaryOp, ExprId, ExprNode, Literal, SemanticPackage, UnaryOp};

/// Comparison class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl CmpOp {
    /// Stable token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Ne => "ne",
            Self::Lt => "lt",
            Self::Le => "le",
            Self::Gt => "gt",
            Self::Ge => "ge",
        }
    }
}

/// Memory layout of a fixed matrix.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Layout {
    RowMajor,
    ColMajor,
}

impl Layout {
    /// Stable token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RowMajor => "row-major",
            Self::ColMajor => "col-major",
        }
    }
}

/// Fixed matrix/vector value (LA mapping; vectors are single-column).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DewMatrix {
    pub rows: usize,
    pub cols: usize,
    /// `rows * cols` scalar expressions; vector = `cols == 1`.
    pub data: Vec<DewExpr>,
    pub layout: Layout,
}

/// Neutral Dew expression (no upstream type).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DewExpr {
    /// Bit-exact f64 literal.
    Float64Bits(u64),
    Bool(bool),
    Int(String),
    Var(String),
    Add(Box<DewExpr>, Box<DewExpr>),
    Sub(Box<DewExpr>, Box<DewExpr>),
    Mul(Box<DewExpr>, Box<DewExpr>),
    Div(Box<DewExpr>, Box<DewExpr>),
    Pow(Box<DewExpr>, Box<DewExpr>),
    Neg(Box<DewExpr>),
    Not(Box<DewExpr>),
    Sqrt(Box<DewExpr>),
    Exp(Box<DewExpr>),
    Ln(Box<DewExpr>),
    Sin(Box<DewExpr>),
    Cos(Box<DewExpr>),
    Tan(Box<DewExpr>),
    Tanh(Box<DewExpr>),
    Abs(Box<DewExpr>),
    Floor(Box<DewExpr>),
    Ceil(Box<DewExpr>),
    IsFinite(Box<DewExpr>),
    Min(Box<DewExpr>, Box<DewExpr>),
    Max(Box<DewExpr>, Box<DewExpr>),
    Atan2(Box<DewExpr>, Box<DewExpr>),
    And(Box<DewExpr>, Box<DewExpr>),
    Or(Box<DewExpr>, Box<DewExpr>),
    Cmp(CmpOp, Box<DewExpr>, Box<DewExpr>),
    If {
        condition: Box<DewExpr>,
        then_value: Box<DewExpr>,
        else_value: Box<DewExpr>,
    },
    /// Fixed matrix/vector value.
    Matrix(DewMatrix),
    /// Linear-algebra operation.
    Linear(LinearOp, Box<DewExpr>, Box<DewExpr>),
}

/// Linear algebra operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LinearOp {
    Dot,
    MatVec,
    MatAdd,
    Scale,
}

impl LinearOp {
    /// Stable token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dot => "dot",
            Self::MatVec => "matvec",
            Self::MatAdd => "mat-add",
            Self::Scale => "scale",
        }
    }
}

/// One mapping refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappingIssue {
    /// `E-PROV-030` unsupported before Dew execution; `E-PROV-033`
    /// LA shape mismatch.
    pub code: &'static str,
    /// Emath node id.
    pub node: ExprId,
    pub detail: String,
}

/// Exact scalar mapping: SIR expression -> Dew expression. Any node
/// outside the strict-f64 subset is refused (gate 2).
pub fn map_expression(package: &SemanticPackage, id: ExprId) -> Result<DewExpr, MappingIssue> {
    let node = package.expr(id).ok_or_else(|| MappingIssue {
        code: "E-PROV-030",
        node: id,
        detail: format!("unknown expression id {id:?}"),
    })?;
    match node {
        ExprNode::Literal(Literal::FloatBits(bits)) => Ok(DewExpr::Float64Bits(*bits)),
        ExprNode::Literal(Literal::Integer(text)) => Ok(DewExpr::Int(text.clone())),
        ExprNode::Literal(Literal::Bool(value)) => Ok(DewExpr::Bool(*value)),
        ExprNode::Literal(Literal::Rational(_) | Literal::Text(_)) => Err(MappingIssue {
            code: "E-PROV-030",
            node: id,
            detail: "rational/text literals are outside the strict-f64 subset".into(),
        }),
        ExprNode::Variable(name) => Ok(DewExpr::Var(name.0.clone())),
        ExprNode::Call {
            function,
            arguments,
        } => map_call(package, id, function, arguments),
        ExprNode::Unary { operation, value } => {
            let inner = map_expression(package, *value)?;
            let mapped = match operation {
                UnaryOp::Negate => DewExpr::Neg(Box::new(inner)),
                UnaryOp::Not => DewExpr::Not(Box::new(inner)),
                UnaryOp::Sqrt => DewExpr::Sqrt(Box::new(inner)),
                UnaryOp::Exp => DewExpr::Exp(Box::new(inner)),
                UnaryOp::Log => DewExpr::Ln(Box::new(inner)),
                UnaryOp::Sin => DewExpr::Sin(Box::new(inner)),
                UnaryOp::Cos => DewExpr::Cos(Box::new(inner)),
                UnaryOp::Tan => DewExpr::Tan(Box::new(inner)),
                UnaryOp::Tanh => DewExpr::Tanh(Box::new(inner)),
                UnaryOp::Abs => DewExpr::Abs(Box::new(inner)),
                UnaryOp::Floor => DewExpr::Floor(Box::new(inner)),
                UnaryOp::Ceil => DewExpr::Ceil(Box::new(inner)),
            };
            Ok(mapped)
        }
        ExprNode::Binary {
            operation,
            left,
            right,
        } => {
            let l = map_expression(package, *left)?;
            let r = map_expression(package, *right)?;
            let mapped = match operation {
                BinaryOp::StrictFloatAdd => DewExpr::Add(Box::new(l), Box::new(r)),
                BinaryOp::StrictFloatSub => DewExpr::Sub(Box::new(l), Box::new(r)),
                BinaryOp::StrictFloatMul => DewExpr::Mul(Box::new(l), Box::new(r)),
                BinaryOp::StrictFloatDiv => DewExpr::Div(Box::new(l), Box::new(r)),
                BinaryOp::StrictFloatPow => DewExpr::Pow(Box::new(l), Box::new(r)),
                BinaryOp::Equal => DewExpr::Cmp(CmpOp::Eq, Box::new(l), Box::new(r)),
                BinaryOp::NotEqual => DewExpr::Cmp(CmpOp::Ne, Box::new(l), Box::new(r)),
                BinaryOp::Less => DewExpr::Cmp(CmpOp::Lt, Box::new(l), Box::new(r)),
                BinaryOp::LessEqual => DewExpr::Cmp(CmpOp::Le, Box::new(l), Box::new(r)),
                BinaryOp::Greater => DewExpr::Cmp(CmpOp::Gt, Box::new(l), Box::new(r)),
                BinaryOp::GreaterEqual => DewExpr::Cmp(CmpOp::Ge, Box::new(l), Box::new(r)),
                BinaryOp::And => DewExpr::And(Box::new(l), Box::new(r)),
                BinaryOp::Or => DewExpr::Or(Box::new(l), Box::new(r)),
                BinaryOp::Min => DewExpr::Min(Box::new(l), Box::new(r)),
                BinaryOp::Max => DewExpr::Max(Box::new(l), Box::new(r)),
                BinaryOp::Atan2 => DewExpr::Atan2(Box::new(l), Box::new(r)),
                BinaryOp::ExactAdd
                | BinaryOp::ExactSub
                | BinaryOp::ExactMul
                | BinaryOp::ExactDiv => {
                    return Err(MappingIssue {
                        code: "E-PROV-030",
                        node: id,
                        detail: format!(
                            "exact arithmetic `{}` is outside the strict-f64 subset",
                            operation.name()
                        ),
                    });
                }
            };
            Ok(mapped)
        }
        ExprNode::If {
            condition,
            then_value,
            else_value,
        } => Ok(DewExpr::If {
            condition: Box::new(map_expression(package, *condition)?),
            then_value: Box::new(map_expression(package, *then_value)?),
            else_value: Box::new(map_expression(package, *else_value)?),
        }),
        other => Err(MappingIssue {
            code: "E-PROV-030",
            node: id,
            detail: format!("node form `{other:?}` is outside the Dew scalar subset"),
        }),
    }
}

/// One-to-one function naming for the well-known builtins.
fn map_call(
    package: &SemanticPackage,
    id: ExprId,
    function: &QualifiedName,
    arguments: &[ExprId],
) -> Result<DewExpr, MappingIssue> {
    let leaf = function
        .0
        .rsplit("::")
        .next()
        .unwrap_or(&function.0)
        .to_string();
    let mapped = match leaf.as_str() {
        "is_finite" => single(package, arguments)?.map(|arg| DewExpr::IsFinite(Box::new(arg))),
        "exp" => single(package, arguments)?.map(|arg| DewExpr::Exp(Box::new(arg))),
        "ln" | "log" => single(package, arguments)?.map(|arg| DewExpr::Ln(Box::new(arg))),
        "sqrt" => single(package, arguments)?.map(|arg| DewExpr::Sqrt(Box::new(arg))),
        "sin" => single(package, arguments)?.map(|arg| DewExpr::Sin(Box::new(arg))),
        "cos" => single(package, arguments)?.map(|arg| DewExpr::Cos(Box::new(arg))),
        "tan" => single(package, arguments)?.map(|arg| DewExpr::Tan(Box::new(arg))),
        "tanh" => single(package, arguments)?.map(|arg| DewExpr::Tanh(Box::new(arg))),
        "abs" => single(package, arguments)?.map(|arg| DewExpr::Abs(Box::new(arg))),
        "floor" => single(package, arguments)?.map(|arg| DewExpr::Floor(Box::new(arg))),
        "ceil" => single(package, arguments)?.map(|arg| DewExpr::Ceil(Box::new(arg))),
        "min" | "max" | "atan2" | "pow" => {
            let pair = pair(package, arguments)?;
            Some(match leaf.as_str() {
                "min" => DewExpr::Min(Box::new(pair.0), Box::new(pair.1)),
                "max" => DewExpr::Max(Box::new(pair.0), Box::new(pair.1)),
                "atan2" => DewExpr::Atan2(Box::new(pair.0), Box::new(pair.1)),
                _ => DewExpr::Pow(Box::new(pair.0), Box::new(pair.1)),
            })
        }
        _ => None,
    };
    mapped.ok_or_else(|| MappingIssue {
        code: "E-PROV-030",
        node: id,
        detail: format!(
            "call `{}` is outside the Dew function naming table",
            function.0
        ),
    })
}

fn single(
    package: &SemanticPackage,
    arguments: &[ExprId],
) -> Result<Option<DewExpr>, MappingIssue> {
    if arguments.len() != 1 {
        return Ok(None);
    }
    map_expression(package, arguments[0]).map(Some)
}

fn pair(
    package: &SemanticPackage,
    arguments: &[ExprId],
) -> Result<(DewExpr, DewExpr), MappingIssue> {
    if arguments.len() != 2 {
        return Err(MappingIssue {
            code: "E-PROV-030",
            node: arguments.first().copied().unwrap_or(ExprId(0)),
            detail: "binary function requires exactly two arguments".into(),
        });
    }
    Ok((
        map_expression(package, arguments[0])?,
        map_expression(package, arguments[1])?,
    ))
}

/// Fixed matrix shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Shape {
    pub rows: usize,
    pub cols: usize,
}

impl Shape {
    /// Vector shape helper.
    #[must_use]
    pub const fn vector(len: usize) -> Self {
        Self { rows: len, cols: 1 }
    }

    /// Whether this is a vector shape.
    #[must_use]
    pub const fn is_vector(self) -> bool {
        self.cols == 1
    }

    /// Whether shapes are compatible for elementwise operations.
    #[must_use]
    pub const fn same(self, other: Self) -> bool {
        self.rows == other.rows && self.cols == other.cols
    }
}

/// Builds a fixed matrix value in the requested layout.
#[must_use]
pub fn matrix(rows: usize, cols: usize, data: Vec<DewExpr>, layout: Layout) -> DewExpr {
    debug_assert_eq!(rows * cols, data.len());
    DewExpr::Matrix(DewMatrix {
        rows,
        cols,
        data,
        layout,
    })
}

/// Explicit layout conversion: row-major <-> col-major
/// is a deterministic reorder; the conversion is reflected in the
/// matrix value so layouts never silently alias.
#[must_use]
pub fn convert_layout(value: &DewExpr, target: Layout) -> DewExpr {
    let DewExpr::Matrix(matrix) = value else {
        return value.clone();
    };
    if matrix.layout == target {
        return value.clone();
    }
    let mut reordered = Vec::with_capacity(matrix.data.len());
    let (rows, cols) = (matrix.rows, matrix.cols);
    if target == Layout::RowMajor {
        // col-major -> row-major: read column by column.
        for row in 0..rows {
            for col in 0..cols {
                reordered.push(matrix.data[col * rows + row].clone());
            }
        }
    } else {
        // row-major -> col-major: read row by row.
        for col in 0..cols {
            for row in 0..rows {
                reordered.push(matrix.data[row * cols + col].clone());
            }
        }
    }
    DewExpr::Matrix(DewMatrix {
        rows,
        cols,
        data: reordered,
        layout: target,
    })
}

/// Restricted linear algebra mapping: checks shapes explicitly and
/// composes the neutral linear node (`E-PROV-033` on mismatch).
pub fn map_linear(op: LinearOp, left: &DewExpr, right: &DewExpr) -> Result<DewExpr, MappingIssue> {
    let shape_of = |value: &DewExpr| match value {
        DewExpr::Matrix(matrix) => Some(Shape {
            rows: matrix.rows,
            cols: matrix.cols,
        }),
        // Non-matrix operands are scalars in the strict-f64 subset.
        _ => None,
    };
    let left_shape = shape_of(left);
    let right_shape = shape_of(right);
    let compatible = match op {
        // dot: vector . vector
        LinearOp::Dot => {
            let (Some(left), Some(right)) = (left_shape, right_shape) else {
                return Err(shape_mismatch(op, left_shape, right_shape));
            };
            left.is_vector() && right.is_vector() && left == right
        }
        // matvec: m x n . n x 1
        LinearOp::MatVec => {
            let (Some(left), Some(right)) = (left_shape, right_shape) else {
                return Err(shape_mismatch(op, left_shape, right_shape));
            };
            !left.is_vector() && right.is_vector() && left.cols == right.rows
        }
        // elementwise add: same shape
        LinearOp::MatAdd => {
            let (Some(left), Some(right)) = (left_shape, right_shape) else {
                return Err(shape_mismatch(op, left_shape, right_shape));
            };
            left.same(right)
        }
        // scale: exactly one side is a matrix, the other a scalar.
        // (matrix . matrix and scalar . scalar are refused: Dew scale
        // semantics are not claimed for them.)
        LinearOp::Scale => left_shape.is_some() != right_shape.is_some(),
    };
    if !compatible {
        return Err(shape_mismatch(op, left_shape, right_shape));
    }
    Ok(DewExpr::Linear(
        op,
        Box::new(left.clone()),
        Box::new(right.clone()),
    ))
}

/// Deterministic `E-PROV-033` shape-mismatch issue. Non-matrix operands
/// render as `<scalar>`.
fn shape_mismatch(op: LinearOp, left: Option<Shape>, right: Option<Shape>) -> MappingIssue {
    MappingIssue {
        code: "E-PROV-033",
        node: ExprId(0),
        detail: format!(
            "shape mismatch for `{}`: left={}, right={}",
            op.as_str(),
            left.map_or_else(|| "<scalar>".to_string(), |s| s.rows.to_string()),
            right.map_or_else(|| "<scalar>".to_string(), |s| s.rows.to_string()),
        ),
    }
}
