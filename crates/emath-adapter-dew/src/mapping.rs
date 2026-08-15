//!: source mapping from SIR/EMIR through Dew nodes to
//! generated symbols, plus typed refusals for non-admitted construct
//! classes ( equivalence boundary).

use emath_ir::{BinaryOp, ExprNode, UnaryOp};

/// Construct classes the Dew adapter refuses in Phase 1. Refusals are
/// typed (AGENTS.md rule 6): the adapter never silently accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedKind {
    /// Exact (integer) arithmetic not lowered in Phase 1.
    ExactInteger,
    /// Records / record field access.
    Record,
    /// Indexing (access [`ExprNode::Index`]).
    Index,
    /// Binders (sum/product/integral/for-all/exists).
    Binder,
    /// Calls to named functions outside the admitted builtins.
    ExternalCall,
    /// Literal forms the strict Float64 profile does not admit.
    NonScalarLiteral,
}

impl UnsupportedKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExactInteger => "exact-integer",
            Self::Record => "record",
            Self::Index => "index",
            Self::Binder => "binder",
            Self::ExternalCall => "external-call",
            Self::NonScalarLiteral => "non-scalar-literal",
        }
    }
}

/// A position in the mirrored program: which EMIR op produced it, and the
/// generated symbol that carries its value (`__e<i>` in the Rust backend).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SirNodePosition {
    pub emir_op_index: usize,
    pub symbol: String,
}

/// The outcome of classifying one SIR expression node for adapter mapping.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MappingResult {
    /// Node admitted to the strict subset.
    Admitted,
    /// Node refused, with the class and the reason.
    Refused {
        kind: UnsupportedKind,
        detail: String,
    },
}

/// Classifies SIR nodes at adapter entry; the mirror then performs the
/// 1:1 op mapping. Kept separate so classification is unit-testable
/// without a lowering pass.
#[derive(Clone, Copy, Debug, Default)]
pub struct SourceMapper;

impl SourceMapper {
    #[must_use]
    pub fn classify(&self, node: &ExprNode) -> MappingResult {
        match node {
            ExprNode::Literal(lit) => match lit {
                emath_ir::Literal::Bool(_) | emath_ir::Literal::FloatBits(_) => {
                    MappingResult::Admitted
                }
                emath_ir::Literal::Integer(_)
                | emath_ir::Literal::Rational(_)
                | emath_ir::Literal::Text(_) => MappingResult::Refused {
                    kind: UnsupportedKind::NonScalarLiteral,
                    detail: format!("literal {:?}", lit),
                },
            },
            ExprNode::Variable(_) => MappingResult::Admitted,
            ExprNode::Call { function, .. } => MappingResult::Refused {
                kind: UnsupportedKind::ExternalCall,
                detail: function.0.clone(),
            },
            ExprNode::Unary { operation, .. } => match operation {
                UnaryOp::Negate
                | UnaryOp::Not
                | UnaryOp::Sqrt
                | UnaryOp::Exp
                | UnaryOp::Log
                | UnaryOp::Sin
                | UnaryOp::Cos
                | UnaryOp::Tan
                | UnaryOp::Tanh
                | UnaryOp::Abs
                | UnaryOp::Floor
                | UnaryOp::Ceil => MappingResult::Admitted,
            },
            ExprNode::Binary { operation, .. } => match operation {
                BinaryOp::StrictFloatAdd
                | BinaryOp::StrictFloatSub
                | BinaryOp::StrictFloatMul
                | BinaryOp::StrictFloatDiv
                | BinaryOp::StrictFloatPow
                | BinaryOp::Equal
                | BinaryOp::NotEqual
                | BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual
                | BinaryOp::And
                | BinaryOp::Or
                | BinaryOp::Min
                | BinaryOp::Max
                | BinaryOp::Atan2 => MappingResult::Admitted,
                BinaryOp::ExactAdd
                | BinaryOp::ExactSub
                | BinaryOp::ExactMul
                | BinaryOp::ExactDiv => MappingResult::Refused {
                    kind: UnsupportedKind::ExactInteger,
                    detail: format!("{:?}", operation),
                },
            },
            ExprNode::If { .. } => MappingResult::Admitted,
            ExprNode::Record { .. } => MappingResult::Refused {
                kind: UnsupportedKind::Record,
                detail: "record constructor".into(),
            },
            ExprNode::Index { .. } => MappingResult::Refused {
                kind: UnsupportedKind::Index,
                detail: "index expression".into(),
            },
            ExprNode::Binder { .. } => MappingResult::Refused {
                kind: UnsupportedKind::Binder,
                detail: "binder expression".into(),
            },
        }
    }

    /// Translate a provider-origin diagnostic detail into an emath message,
    /// retaining the original detail verbatim (never lossy translation).
    #[must_use]
    pub fn translate_provider_detail(original: &str, context: &str) -> String {
        format!("{context}: provider detail retained verbatim: {original}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emath_core::QualifiedName;
    use emath_ir::Literal;

    fn var(name: &str) -> ExprNode {
        ExprNode::Variable(QualifiedName(name.to_string()))
    }

    #[test]
    fn strict_scalar_surface_is_admitted() {
        let mapper = SourceMapper;
        for node in [
            ExprNode::Literal(Literal::FloatBits(1.0_f64.to_bits())),
            ExprNode::Literal(Literal::Bool(true)),
            var("x"),
            ExprNode::Unary {
                operation: UnaryOp::Sqrt,
                value: emath_ir::ExprId(0),
            },
            ExprNode::Binary {
                operation: BinaryOp::StrictFloatMul,
                left: emath_ir::ExprId(0),
                right: emath_ir::ExprId(1),
            },
            ExprNode::If {
                condition: emath_ir::ExprId(0),
                then_value: emath_ir::ExprId(1),
                else_value: emath_ir::ExprId(2),
            },
        ] {
            assert_eq!(mapper.classify(&node), MappingResult::Admitted, "{node:?}");
        }
    }

    #[test]
    fn exact_arithmetic_is_refused_with_kind() {
        let mapper = SourceMapper;
        let node = ExprNode::Binary {
            operation: BinaryOp::ExactAdd,
            left: emath_ir::ExprId(0),
            right: emath_ir::ExprId(1),
        };
        assert_eq!(
            mapper.classify(&node),
            MappingResult::Refused {
                kind: UnsupportedKind::ExactInteger,
                detail: "ExactAdd".into(),
            }
        );
    }

    #[test]
    fn structural_kinds_are_refused() {
        let mapper = SourceMapper;
        for (node, kind) in [
            (
                ExprNode::Call {
                    function: QualifiedName("core::math::pow".into()),
                    arguments: vec![emath_ir::ExprId(0)],
                },
                UnsupportedKind::ExternalCall,
            ),
            (
                ExprNode::Record {
                    ty: emath_ir::TypeId(0),
                    fields: std::collections::BTreeMap::new(),
                },
                UnsupportedKind::Record,
            ),
            (
                ExprNode::Index {
                    value: emath_ir::ExprId(0),
                    indices: vec![emath_ir::ExprId(1)],
                },
                UnsupportedKind::Index,
            ),
            (
                ExprNode::Binder {
                    kind: emath_ir::BinderKind::Sum,
                    variables: Vec::new(),
                    body: emath_ir::ExprId(0),
                },
                UnsupportedKind::Binder,
            ),
        ] {
            assert!(
                matches!(mapper.classify(&node), MappingResult::Refused { kind: k, .. } if k == kind)
            );
        }
    }

    #[test]
    fn provider_diagnostics_keep_original() {
        let msg = SourceMapper::translate_provider_detail("dew node 7: div-by-zero", "at score");
        assert!(msg.contains("dew node 7: div-by-zero"));
        assert!(msg.contains("retained verbatim"));
    }
}
