//! Provider-neutral symbolic contracts and a bounded native exact-algebra slice.

use crate::{BinaryOp, ExprId, ExprNode, Literal, SemanticPackage, UnaryOp};
use emath_core::{QualifiedName, SchemaId, Span};
use std::collections::BTreeMap;

pub const SYMBOLIC_SCHEMA_V1: &str = "emath.symbolic/v1";
pub const MAX_POLYNOMIAL_DEGREE: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolicError {
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for SymbolicError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SymbolicError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SymbolicExpr {
    Integer(i128),
    Variable(String),
    Unary {
        operation: UnaryOp,
        value: Box<Self>,
    },
    Binary {
        operation: BinaryOp,
        left: Box<Self>,
        right: Box<Self>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RewritePattern {
    Capture(String),
    Integer(i128),
    Variable(String),
    Unary {
        operation: UnaryOp,
        value: Box<Self>,
    },
    Binary {
        operation: BinaryOp,
        left: Box<Self>,
        right: Box<Self>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RewriteAuthority {
    StructuralChecked,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RewriteRule {
    pub id: String,
    pub pattern: RewritePattern,
    pub replacement: RewritePattern,
    pub authority: RewriteAuthority,
}

impl RewriteRule {
    pub fn new(
        id: impl Into<String>,
        pattern: RewritePattern,
        replacement: RewritePattern,
        authority: &str,
    ) -> Result<Self, SymbolicError> {
        if authority != "structural-checked" {
            return Err(symbolic_error(
                "E-SYM-004",
                "native rewrite rules are `structural-checked`; `proved` requires a checkable certificate",
            ));
        }
        Ok(Self {
            id: id.into(),
            pattern,
            replacement,
            authority: RewriteAuthority::StructuralChecked,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Simplification {
    pub expression: ExprId,
    pub rewrites: Vec<String>,
    pub authority: RewriteAuthority,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolynomialDecision {
    pub equal: bool,
    pub left_coefficients: Vec<i128>,
    pub right_coefficients: Vec<i128>,
    pub authority: RewriteAuthority,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolicOracleContract {
    pub schema: SchemaId,
    pub operations: Vec<String>,
    pub result_authority: String,
}

#[must_use]
pub fn symbolic_oracle_contract() -> SymbolicOracleContract {
    SymbolicOracleContract {
        schema: SchemaId(SYMBOLIC_SCHEMA_V1.into()),
        operations: vec![
            "simplify".into(),
            "rewrite".into(),
            "decide-univariate-polynomial-identity".into(),
        ],
        result_authority: "structural-checked-or-certified".into(),
    }
}

pub fn expression_from_package(
    package: &SemanticPackage,
    expression: ExprId,
) -> Result<SymbolicExpr, SymbolicError> {
    let node = package.expr(expression).ok_or_else(|| {
        symbolic_error(
            "E-SYM-001",
            format!(
                "symbolic expression id {} is outside the arena",
                expression.0
            ),
        )
    })?;
    match node {
        ExprNode::Literal(Literal::Integer(value)) => value
            .parse()
            .map(SymbolicExpr::Integer)
            .map_err(|_| symbolic_error("E-SYM-001", "integer literal exceeds exact i128 range")),
        ExprNode::Variable(name) => Ok(SymbolicExpr::Variable(name.0.clone())),
        ExprNode::Unary { operation, value } => Ok(SymbolicExpr::Unary {
            operation: *operation,
            value: Box::new(expression_from_package(package, *value)?),
        }),
        ExprNode::Binary {
            operation,
            left,
            right,
        } => Ok(SymbolicExpr::Binary {
            operation: *operation,
            left: Box::new(expression_from_package(package, *left)?),
            right: Box::new(expression_from_package(package, *right)?),
        }),
        other => Err(symbolic_error(
            "E-SYM-003",
            format!(
                "native symbolic v1 supports exact integer scalar expressions, not `{}`",
                expression_kind(other)
            ),
        )),
    }
}

pub fn expression_into_package(
    package: &mut SemanticPackage,
    expression: &SymbolicExpr,
    source: Span,
) -> ExprId {
    let node = match expression {
        SymbolicExpr::Integer(value) => ExprNode::Literal(Literal::Integer(value.to_string())),
        SymbolicExpr::Variable(name) => ExprNode::Variable(QualifiedName(name.clone())),
        SymbolicExpr::Unary { operation, value } => {
            let value = expression_into_package(package, value, source);
            ExprNode::Unary {
                operation: *operation,
                value,
            }
        }
        SymbolicExpr::Binary {
            operation,
            left,
            right,
        } => {
            let left = expression_into_package(package, left, source);
            let right = expression_into_package(package, right, source);
            ExprNode::Binary {
                operation: *operation,
                left,
                right,
            }
        }
    };
    package.push_expr(node, source)
}

pub fn apply_rewrite(
    expression: &SymbolicExpr,
    rule: &RewriteRule,
) -> Result<Option<SymbolicExpr>, SymbolicError> {
    let mut captures = BTreeMap::new();
    if !matches_pattern(&rule.pattern, expression, &mut captures) {
        return Ok(None);
    }
    instantiate(&rule.replacement, &captures).map(Some)
}

pub fn simplify_expression(
    package: &mut SemanticPackage,
    expression: ExprId,
) -> Result<Simplification, SymbolicError> {
    let source = package.expr_span(expression);
    let expression = expression_from_package(package, expression)?;
    let mut rewrites = Vec::new();
    let simplified = simplify(&expression, &mut rewrites)?;
    let expression = expression_into_package(package, &simplified, source);
    Ok(Simplification {
        expression,
        rewrites,
        authority: RewriteAuthority::StructuralChecked,
    })
}

pub fn simplify_integer_expression(
    package: &mut SemanticPackage,
    expression: ExprId,
) -> Result<Simplification, SymbolicError> {
    let source = package.expr_span(expression);
    let expression = normalize_integer_operations(expression_from_package(package, expression)?)?;
    let mut rewrites = Vec::new();
    let simplified = simplify(&expression, &mut rewrites)?;
    let expression = expression_into_package(package, &simplified, source);
    Ok(Simplification {
        expression,
        rewrites,
        authority: RewriteAuthority::StructuralChecked,
    })
}

pub fn decide_univariate_polynomial_identity(
    package: &SemanticPackage,
    left: ExprId,
    right: ExprId,
    variable: &str,
) -> Result<PolynomialDecision, SymbolicError> {
    let left = polynomial(&expression_from_package(package, left)?, variable)?;
    let right = polynomial(&expression_from_package(package, right)?, variable)?;
    Ok(PolynomialDecision {
        equal: left == right,
        left_coefficients: left,
        right_coefficients: right,
        authority: RewriteAuthority::StructuralChecked,
    })
}

fn simplify(
    expression: &SymbolicExpr,
    rewrites: &mut Vec<String>,
) -> Result<SymbolicExpr, SymbolicError> {
    match expression {
        SymbolicExpr::Integer(_) | SymbolicExpr::Variable(_) => Ok(expression.clone()),
        SymbolicExpr::Unary { operation, value } => {
            let value = simplify(value, rewrites)?;
            if *operation == UnaryOp::Negate {
                if let SymbolicExpr::Integer(value) = value {
                    rewrites.push("negate-integer".into());
                    return value
                        .checked_neg()
                        .map(SymbolicExpr::Integer)
                        .ok_or_else(|| symbolic_error("E-SYM-002", "exact negation overflow"));
                }
            }
            Ok(SymbolicExpr::Unary {
                operation: *operation,
                value: Box::new(value),
            })
        }
        SymbolicExpr::Binary {
            operation,
            left,
            right,
        } => {
            let left = simplify(left, rewrites)?;
            let right = simplify(right, rewrites)?;
            simplify_binary(*operation, left, right, rewrites)
        }
    }
}

fn simplify_binary(
    operation: BinaryOp,
    left: SymbolicExpr,
    right: SymbolicExpr,
    rewrites: &mut Vec<String>,
) -> Result<SymbolicExpr, SymbolicError> {
    match (operation, &left, &right) {
        (_, _, SymbolicExpr::Integer(0)) if is_symbolic_add(operation) => {
            rewrites.push("add-zero-right".into());
            return Ok(left);
        }
        (_, SymbolicExpr::Integer(0), _) if is_symbolic_add(operation) => {
            rewrites.push("add-zero-left".into());
            return Ok(right);
        }
        (_, _, SymbolicExpr::Integer(0)) if is_symbolic_sub(operation) => {
            rewrites.push("subtract-zero".into());
            return Ok(left);
        }
        (_, _, SymbolicExpr::Integer(1)) if is_symbolic_mul(operation) => {
            rewrites.push("multiply-one-right".into());
            return Ok(left);
        }
        (_, SymbolicExpr::Integer(1), _) if is_symbolic_mul(operation) => {
            rewrites.push("multiply-one-left".into());
            return Ok(right);
        }
        (_, _, SymbolicExpr::Integer(0)) | (_, SymbolicExpr::Integer(0), _)
            if is_symbolic_mul(operation) =>
        {
            rewrites.push("multiply-zero".into());
            return Ok(SymbolicExpr::Integer(0));
        }
        _ => {}
    }
    if let (SymbolicExpr::Integer(left), SymbolicExpr::Integer(right)) = (&left, &right) {
        let folded = match operation {
            operation if is_symbolic_add(operation) => left.checked_add(*right),
            operation if is_symbolic_sub(operation) => left.checked_sub(*right),
            operation if is_symbolic_mul(operation) => left.checked_mul(*right),
            _ => None,
        };
        if let Some(value) = folded {
            rewrites.push("fold-exact-integers".into());
            return Ok(SymbolicExpr::Integer(value));
        }
        if is_symbolic_add(operation) || is_symbolic_sub(operation) || is_symbolic_mul(operation) {
            return Err(symbolic_error(
                "E-SYM-002",
                "exact integer simplification overflow",
            ));
        }
    }
    Ok(SymbolicExpr::Binary {
        operation,
        left: Box::new(left),
        right: Box::new(right),
    })
}

const fn is_symbolic_add(operation: BinaryOp) -> bool {
    matches!(operation, BinaryOp::ExactAdd)
}

const fn is_symbolic_sub(operation: BinaryOp) -> bool {
    matches!(operation, BinaryOp::ExactSub)
}

const fn is_symbolic_mul(operation: BinaryOp) -> bool {
    matches!(operation, BinaryOp::ExactMul)
}

fn normalize_integer_operations(expression: SymbolicExpr) -> Result<SymbolicExpr, SymbolicError> {
    match expression {
        SymbolicExpr::Integer(_) | SymbolicExpr::Variable(_) => Ok(expression),
        SymbolicExpr::Unary { operation, value } => Ok(SymbolicExpr::Unary {
            operation,
            value: Box::new(normalize_integer_operations(*value)?),
        }),
        SymbolicExpr::Binary {
            operation,
            left,
            right,
        } => {
            let operation = match operation {
                BinaryOp::StrictFloatAdd => BinaryOp::ExactAdd,
                BinaryOp::StrictFloatSub => BinaryOp::ExactSub,
                BinaryOp::StrictFloatMul => BinaryOp::ExactMul,
                BinaryOp::ExactAdd | BinaryOp::ExactSub | BinaryOp::ExactMul => operation,
                _ => {
                    return Err(symbolic_error(
                        "E-SYM-003",
                        "integer simplification supports +, -, and * in native v1",
                    ));
                }
            };
            Ok(SymbolicExpr::Binary {
                operation,
                left: Box::new(normalize_integer_operations(*left)?),
                right: Box::new(normalize_integer_operations(*right)?),
            })
        }
    }
}

fn matches_pattern(
    pattern: &RewritePattern,
    expression: &SymbolicExpr,
    captures: &mut BTreeMap<String, SymbolicExpr>,
) -> bool {
    match (pattern, expression) {
        (RewritePattern::Capture(name), expression) => match captures.get(name) {
            Some(prior) => prior == expression,
            None => {
                captures.insert(name.clone(), expression.clone());
                true
            }
        },
        (RewritePattern::Integer(left), SymbolicExpr::Integer(right)) => left == right,
        (RewritePattern::Variable(left), SymbolicExpr::Variable(right)) => left == right,
        (
            RewritePattern::Unary {
                operation: left_operation,
                value: left,
            },
            SymbolicExpr::Unary {
                operation: right_operation,
                value: right,
            },
        ) => left_operation == right_operation && matches_pattern(left, right, captures),
        (
            RewritePattern::Binary {
                operation: left_operation,
                left: left_left,
                right: left_right,
            },
            SymbolicExpr::Binary {
                operation: right_operation,
                left: right_left,
                right: right_right,
            },
        ) => {
            left_operation == right_operation
                && matches_pattern(left_left, right_left, captures)
                && matches_pattern(left_right, right_right, captures)
        }
        _ => false,
    }
}

fn instantiate(
    pattern: &RewritePattern,
    captures: &BTreeMap<String, SymbolicExpr>,
) -> Result<SymbolicExpr, SymbolicError> {
    match pattern {
        RewritePattern::Capture(name) => captures.get(name).cloned().ok_or_else(|| {
            symbolic_error(
                "E-SYM-001",
                format!("replacement references uncaptured variable `{name}`"),
            )
        }),
        RewritePattern::Integer(value) => Ok(SymbolicExpr::Integer(*value)),
        RewritePattern::Variable(name) => Ok(SymbolicExpr::Variable(name.clone())),
        RewritePattern::Unary { operation, value } => Ok(SymbolicExpr::Unary {
            operation: *operation,
            value: Box::new(instantiate(value, captures)?),
        }),
        RewritePattern::Binary {
            operation,
            left,
            right,
        } => Ok(SymbolicExpr::Binary {
            operation: *operation,
            left: Box::new(instantiate(left, captures)?),
            right: Box::new(instantiate(right, captures)?),
        }),
    }
}

fn polynomial(expression: &SymbolicExpr, variable: &str) -> Result<Vec<i128>, SymbolicError> {
    match expression {
        SymbolicExpr::Integer(value) => Ok(vec![*value]),
        SymbolicExpr::Variable(name) if name == variable => Ok(vec![0, 1]),
        SymbolicExpr::Variable(name) => Err(symbolic_error(
            "E-SYM-003",
            format!("polynomial decision expected variable `{variable}`, found `{name}`"),
        )),
        SymbolicExpr::Unary {
            operation: UnaryOp::Negate,
            value,
        } => polynomial(value, variable)?
            .into_iter()
            .map(|coefficient| {
                coefficient
                    .checked_neg()
                    .ok_or_else(|| symbolic_error("E-SYM-002", "polynomial coefficient overflow"))
            })
            .collect(),
        SymbolicExpr::Binary {
            operation,
            left,
            right,
        } if is_symbolic_add(*operation)
            || is_symbolic_sub(*operation)
            || is_symbolic_mul(*operation) =>
        {
            let left = polynomial(left, variable)?;
            let right = polynomial(right, variable)?;
            match operation {
                operation if is_symbolic_add(*operation) => add_polynomials(&left, &right, false),
                operation if is_symbolic_sub(*operation) => add_polynomials(&left, &right, true),
                _ => multiply_polynomials(&left, &right),
            }
        }
        SymbolicExpr::Binary {
            operation: BinaryOp::StrictFloatPow,
            left,
            right,
        } => {
            let SymbolicExpr::Integer(exponent) = right.as_ref() else {
                return Err(symbolic_error(
                    "E-SYM-003",
                    "polynomial exponent must be a non-negative integer literal",
                ));
            };
            let exponent = usize::try_from(*exponent).map_err(|_| {
                symbolic_error(
                    "E-SYM-003",
                    "polynomial exponent must be a non-negative integer",
                )
            })?;
            if exponent > MAX_POLYNOMIAL_DEGREE {
                return Err(symbolic_error(
                    "E-SYM-002",
                    format!("polynomial degree exceeds {MAX_POLYNOMIAL_DEGREE}"),
                ));
            }
            let base = polynomial(left, variable)?;
            let mut value = vec![1];
            for _ in 0..exponent {
                value = multiply_polynomials(&value, &base)?;
            }
            Ok(value)
        }
        _ => Err(symbolic_error(
            "E-SYM-003",
            "claim is outside the native univariate exact-polynomial fragment",
        )),
    }
}

fn add_polynomials(
    left: &[i128],
    right: &[i128],
    subtract: bool,
) -> Result<Vec<i128>, SymbolicError> {
    let mut result = vec![0; left.len().max(right.len())];
    for (index, slot) in result.iter_mut().enumerate() {
        let left = left.get(index).copied().unwrap_or(0);
        let right = right.get(index).copied().unwrap_or(0);
        *slot = if subtract {
            left.checked_sub(right)
        } else {
            left.checked_add(right)
        }
        .ok_or_else(|| symbolic_error("E-SYM-002", "polynomial coefficient overflow"))?;
    }
    trim_polynomial(&mut result);
    Ok(result)
}

fn multiply_polynomials(left: &[i128], right: &[i128]) -> Result<Vec<i128>, SymbolicError> {
    let degree = left.len().saturating_add(right.len()).saturating_sub(2);
    if degree > MAX_POLYNOMIAL_DEGREE {
        return Err(symbolic_error(
            "E-SYM-002",
            format!("polynomial degree exceeds {MAX_POLYNOMIAL_DEGREE}"),
        ));
    }
    let mut result = vec![0_i128; degree + 1];
    for (left_degree, left_coefficient) in left.iter().enumerate() {
        for (right_degree, right_coefficient) in right.iter().enumerate() {
            let term = left_coefficient
                .checked_mul(*right_coefficient)
                .ok_or_else(|| symbolic_error("E-SYM-002", "polynomial coefficient overflow"))?;
            let slot = &mut result[left_degree + right_degree];
            *slot = slot
                .checked_add(term)
                .ok_or_else(|| symbolic_error("E-SYM-002", "polynomial coefficient overflow"))?;
        }
    }
    trim_polynomial(&mut result);
    Ok(result)
}

fn trim_polynomial(polynomial: &mut Vec<i128>) {
    while polynomial.len() > 1 && polynomial.last() == Some(&0) {
        polynomial.pop();
    }
}

fn expression_kind(expression: &ExprNode) -> &'static str {
    match expression {
        ExprNode::Literal(_) => "non-integer literal",
        ExprNode::Variable(_) => "variable",
        ExprNode::Call { .. } => "function call",
        ExprNode::Unary { .. } => "unary expression",
        ExprNode::Binary { .. } => "binary expression",
        ExprNode::If { .. } => "conditional",
        ExprNode::Record { .. } => "record",
        ExprNode::Index { .. } => "index",
        ExprNode::Slice { .. } => "slice",
        ExprNode::Binder { .. } => "binder",
        ExprNode::Vector(_) => "vector",
        ExprNode::Matrix(_) => "matrix",
        ExprNode::Tensor { .. } => "tensor",
        ExprNode::Differentiate { .. } => "differentiate",
        ExprNode::Solve { .. } => "solve",
        ExprNode::Optimize { .. } => "optimize",
        ExprNode::SampleLimit { .. } => "sample-limit",
    }
}

fn symbolic_error(code: &'static str, message: impl Into<String>) -> SymbolicError {
    SymbolicError {
        code,
        message: message.into(),
    }
}
