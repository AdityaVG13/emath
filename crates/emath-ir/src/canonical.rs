//! Canonical semantic bytes for identity (schema `emath.sir.v1`).
//!
//! Deterministic text encoding: schema prefix, sorted collections, numeric
//! spellings preserved from source. Presentation fields (spans, display
//! names, input order of order-insensitive sets) are excluded or sorted.
//! This is the Phase 1 bootstrap canonicalization; it will be versioned as
//! the durable identity scheme in later phases.

use crate::constructor::{Field, Visibility};
use crate::expression::{ExprNode, Literal};
use crate::goal::{DeterminismPolicy, ExactnessPolicy, FallbackPolicy};
use crate::package::SemanticPackage;
use emath_core::{ContentId, QualifiedName};

const SCHEMA: &str = "emath.sir.v1";

fn exactness_canonical(policy: &ExactnessPolicy) -> String {
    match policy {
        ExactnessPolicy::Exact => "exact".to_string(),
        ExactnessPolicy::Bounded { tolerance_literal } => {
            format!("bounded:{tolerance_literal}")
        }
        ExactnessPolicy::CheckedNumeric => "checked".to_string(),
        ExactnessPolicy::Estimate => "estimate".to_string(),
        ExactnessPolicy::AnyExplicit => "any-explicit".to_string(),
    }
}

fn encode_field(out: &mut String, package: &SemanticPackage, field: &Field, tag: &str) {
    out.push_str(tag);
    out.push(' ');
    out.push_str(&field.name);
    out.push(' ');
    out.push_str(&package.types[field.ty.index()].display_name());
    out.push(' ');
    out.push_str(match field.visibility {
        Visibility::Public => "public",
        Visibility::Package => "package",
        Visibility::Private => "private",
    });
    out.push('\n');
}

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
            Literal::Text(payload) => {
                out.push_str("literal-text ");
                out.push_str(payload);
                out.push('\n');
            }
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
        ExprNode::Record { fields, ty } => {
            push_str(out, "record");
            push_str(out, "record");
            out.push_str("record-ty=");
            out.push_str(&ty.index().to_string());
            out.push('\n');
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
    if let Some(path) = &package.package_path {
        push_str(&mut out, "package ");
        out.push_str(&path.join("."));
        out.push('\n');
    }
    let mut imports = package.imports.clone();
    imports.sort_by(|a, b| {
        a.path
            .join(".")
            .cmp(&b.path.join("."))
            .then_with(|| a.selection.canonical().cmp(&b.selection.canonical()))
    });
    for import in &imports {
        push_str(&mut out, "import ");
        out.push_str(&import.path.join("."));
        out.push(':');
        out.push_str(&import.selection.canonical());
        out.push('\n');
    }
    for declaration in &package.declarations {
        push_str(&mut out, "declaration");
        out.push_str(&declaration.name.0);
        out.push('\n');
        out.push_str(&declaration.kind.0);
        out.push('\n');
        for field in &declaration.inputs {
            encode_field(&mut out, package, field, "input");
        }
        for field in &declaration.outputs {
            encode_field(&mut out, package, field, "output");
        }
        for field in &declaration.state {
            encode_field(&mut out, package, field, "state");
        }
        for invariant in &declaration.invariants {
            push_str(&mut out, "invariant");
            encode_expr(&mut out, &package.exprs, *invariant);
        }
        push_str(&mut out, "compile");
        out.push_str(&declaration.compile_spec.target);
        out.push('\n');
        out.push_str(&declaration.compile_spec.profile);
        out.push('\n');
        out.push_str(declaration.compile_spec.numeric.as_str());
        out.push('\n');
        out.push_str(declaration.compile_spec.safety.as_str());
        out.push('\n');
        if let Some(unresolved) = &declaration.compile_spec.unresolved {
            out.push_str("unresolved ");
            out.push_str(unresolved);
            out.push('\n');
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
                out.push('\n');
                out.push_str(goal.requirements.evidence.as_str());
                out.push('\n');
                out.push_str(&exactness_canonical(&goal.requirements.exactness));
                out.push('\n');
                out.push_str(match goal.requirements.determinism {
                    DeterminismPolicy::Required => "required",
                    DeterminismPolicy::Preferred => "preferred",
                    DeterminismPolicy::Unspecified => "unspecified",
                });
                out.push('\n');
                out.push_str("target ");
                out.push_str(&goal.requirements.target.family);
                out.push('\n');
                if let Some(triple) = &goal.requirements.target.triple {
                    out.push_str("triple ");
                    out.push_str(triple);
                    out.push('\n');
                }
                out.push_str("features ");
                out.push_str(&goal.requirements.target.features.join(","));
                out.push('\n');
                out.push_str(match goal.requirements.fallback {
                    FallbackPolicy::NativeOnly => "native-only",
                    FallbackPolicy::Parametric => "parametric",
                    FallbackPolicy::Continuation => "continuation",
                    FallbackPolicy::Diagnostic => "diagnostic",
                    FallbackPolicy::ExplicitLadder => "explicit-ladder",
                });
                out.push('\n');
                if let Some(expression) = goal.expression {
                    push_str(&mut out, "goal-expression");
                    encode_expr(&mut out, &package.exprs, expression);
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
                for parameter in &constructor.parameters {
                    encode_field(&mut out, package, parameter, "param");
                }
                for precondition in &constructor.preconditions {
                    encode_expr(&mut out, &package.exprs, *precondition);
                }
                for (field, &value) in &constructor.assignments {
                    out.push_str("assign ");
                    out.push_str(field);
                    out.push('\n');
                    encode_expr(&mut out, &package.exprs, value);
                }
                for postcondition in &constructor.postconditions {
                    push_str(&mut out, "postcondition");
                    encode_expr(&mut out, &package.exprs, *postcondition);
                }
                for (field, &value) in &constructor.defaults {
                    out.push_str("default ");
                    out.push_str(field);
                    out.push('\n');
                    encode_expr(&mut out, &package.exprs, value);
                }
                if let Some(error_ty) = constructor.error_type {
                    out.push_str("error-type ");
                    out.push_str(&package.types[error_ty.index()].display_name());
                    out.push('\n');
                }
                out.push_str(if constructor.is_public {
                    "constructor-public"
                } else {
                    "constructor-internal"
                });
                out.push('\n');
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
