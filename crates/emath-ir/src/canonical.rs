//! Canonical semantic bytes for identity (schema `emath.sir.v1`).
//!
//! Deterministic text encoding: schema prefix, sorted collections, numeric
//! spellings preserved from source. Presentation fields (spans, display
//! names, input order of order-insensitive sets) are excluded or sorted.
//! This is the Phase 1 bootstrap canonicalization; it will be versioned as
//! the durable identity scheme in later phases.

use crate::expression::{ExprNode, Literal};
use crate::package::SemanticPackage;
use emath_core::{ContentId, QualifiedName};

const SCHEMA: &str = "emath.sir.v1";

fn push_str(out: &mut String, s: &str) {
    out.push_str(s);
    out.push('\n');
}

fn name(q: &QualifiedName) -> &str {
    &q.0
}

fn encode_expr(out: &mut String, exprs: &[ExprNode], id: crate::ids::ExprId) {
    let Some(expr) = exprs.get(id.index()) else {
        push_str(out, "<missing-expr>");
        return;
    };
    match expr {
        ExprNode::Literal(literal) => match literal {
            Literal::Bool(value) => push_str(
                out,
                if *value {
                    "literal true"
                } else {
                    "literal false"
                },
            ),
            Literal::Integer(text) => {
                out.push_str("literal-int ");
                out.push_str(text);
                out.push('\n');
            }
            Literal::Rational(text) => {
                out.push_str("literal-rat ");
                out.push_str(text);
                out.push('\n');
            }
            Literal::FloatBits(bits) => {
                use std::fmt::Write;
                let _ = writeln!(out, "literal-f64 {bits:016x}");
            }
            Literal::Text(_) => push_str(out, "literal-text"),
        },
        ExprNode::Variable(variable) => {
            out.push_str("var ");
            out.push_str(name(variable));
            out.push('\n');
        }
        ExprNode::Call {
            function,
            arguments,
        } => {
            out.push_str("call ");
            out.push_str(name(function));
            out.push('\n');
            for &arg in arguments {
                encode_expr(out, exprs, arg);
            }
        }
        ExprNode::Unary { operation, value } => {
            out.push_str("unary ");
            out.push_str(operation.name());
            out.push('\n');
            encode_expr(out, exprs, *value);
        }
        ExprNode::Binary {
            operation,
            left,
            right,
        } => {
            out.push_str("binary ");
            out.push_str(operation.name());
            out.push('\n');
            encode_expr(out, exprs, *left);
            encode_expr(out, exprs, *right);
        }
        ExprNode::If {
            condition,
            then_value,
            else_value,
        } => {
            push_str(out, "if");
            encode_expr(out, exprs, *condition);
            encode_expr(out, exprs, *then_value);
            encode_expr(out, exprs, *else_value);
        }
        ExprNode::Record { fields, .. } => {
            push_str(out, "record");
            for (key, &value) in fields {
                out.push_str("field ");
                out.push_str(key);
                out.push('\n');
                encode_expr(out, exprs, value);
            }
        }
        ExprNode::Index { value, indices } => {
            push_str(out, "index");
            encode_expr(out, exprs, *value);
            for &index in indices {
                encode_expr(out, exprs, index);
            }
        }
        ExprNode::Binder {
            kind,
            variables,
            body,
        } => {
            out.push_str("binder ");
            out.push_str(match kind {
                crate::expression::BinderKind::Sum => "sum",
                crate::expression::BinderKind::Product => "product",
                crate::expression::BinderKind::Integral => "integral",
                crate::expression::BinderKind::ForAll => "forall",
                crate::expression::BinderKind::Exists => "exists",
            });
            out.push('\n');
            for variable in variables {
                out.push_str("bound ");
                out.push_str(&variable.name);
                out.push('\n');
                encode_expr(out, exprs, variable.domain);
            }
            encode_expr(out, exprs, *body);
        }
    }
}

/// Deterministic canonical bytes for a semantic package.
#[must_use]
pub fn canonical_package(package: &SemanticPackage) -> ContentId {
    let mut out = String::new();
    push_str(&mut out, SCHEMA);
    for declaration in &package.declarations {
        push_str(&mut out, "declaration");
        out.push_str(&declaration.name.0);
        out.push('\n');
        out.push_str(&declaration.kind.0);
        out.push('\n');
        for field in &declaration.inputs {
            let ty = package.types[field.ty.index()].display_name();
            let _ = ty;
        }
        for definition in &declaration.definitions {
            out.push_str("definition ");
            out.push_str(definition.0);
            out.push('\n');
            encode_expr(&mut out, &package.exprs, *definition.1);
        }
        for goal in &declaration.goals {
            if let Some(goal) = package.goals.get(goal.index()) {
                out.push_str("goal ");
                out.push_str(goal.kind.as_str());
                out.push('\n');
                out.push_str(&goal.target);
                out.push('\n');
                out.push_str(&goal.requirements.produce);
                out.push('\n');
            }
        }
        for export in &declaration.exports {
            out.push_str("export ");
            out.push_str(&export.kind);
            out.push(' ');
            out.push_str(&export.name);
            out.push('\n');
        }
        for constructor in &declaration.constructors {
            out.push_str("constructor ");
            out.push_str(&constructor.name);
            out.push('\n');
            for precondition in &constructor.preconditions {
                encode_expr(&mut out, &package.exprs, *precondition);
            }
            for (field, &value) in &constructor.assignments {
                out.push_str("assign ");
                out.push_str(field);
                out.push('\n');
                encode_expr(&mut out, &package.exprs, value);
            }
        }
        for test_id in &declaration.tests {
            if let Some(test) = package.tests.get(test_id.index()) {
                out.push_str("test ");
                out.push_str(&test.name);
                out.push('\n');
                for (input, &value) in &test.given {
                    out.push_str("given ");
                    out.push_str(input);
                    out.push('\n');
                    encode_expr(&mut out, &package.exprs, value);
                }
                out.push_str("expect\n");
                encode_expr(&mut out, &package.exprs, test.expect);
            }
        }
    }
    emath_core::hash::bootstrap_content_id(out.as_bytes())
}

/// Canonical bytes for a single expression (used by goal identity).
#[must_use]
pub fn canonical_expr(package: &SemanticPackage, id: crate::ids::ExprId) -> ContentId {
    let mut out = String::new();
    push_str(&mut out, SCHEMA);
    out.push_str("expr\n");
    encode_expr(&mut out, &package.exprs, id);
    emath_core::hash::bootstrap_content_id(out.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expression::{BinaryOp, ExprNode, Literal};
    use crate::ids::ExprId;

    fn node_package(operation: BinaryOp) -> SemanticPackage {
        let mut package = SemanticPackage::new();
        let left = package.push_expr(
            ExprNode::Literal(Literal::FloatBits(0x3ff0_0000_0000_0000)),
            emath_core::Span::default(),
        );
        let right = package.push_expr(
            ExprNode::Literal(Literal::FloatBits(0x4000_0000_0000_0000)),
            emath_core::Span::default(),
        );
        let _ = package.push_expr(
            ExprNode::Binary {
                operation,
                left,
                right,
            },
            emath_core::Span::default(),
        );
        package
    }

    #[test]
    fn canonical_identity_is_sensitive_to_operator_mutation() {
        // The package arena holds the exprs; identity is computed over the
        // canonical expression encoding.
        let add_pkg = node_package(BinaryOp::StrictFloatAdd);
        let sub_pkg = node_package(BinaryOp::StrictFloatSub);
        let root_add = ExprId(2);
        let root_sub = ExprId(2);
        let add = canonical_expr(&add_pkg, root_add);
        let sub = canonical_expr(&sub_pkg, root_sub);
        assert_ne!(add, sub);
        // Determinism: same input ⇒ same identity.
        assert_eq!(
            add,
            canonical_expr(&node_package(BinaryOp::StrictFloatAdd), root_add)
        );
    }
}
