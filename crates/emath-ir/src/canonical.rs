//! Canonical semantic bytes for identity (schema `emath.sir`).
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
use crate::types::TypeNode;
use emath_core::{ContentId, QualifiedName};

const SCHEMA: &str = "emath.sir";

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

/// Structural type encoding for identity. The display name collapses
/// distinct type nodes (`Record("m")` vs `Other("m")` both display as
/// `m`), so identity must encode the node structurally and never discard
/// the node kind. Numeric aliases Real/Float64/f64 intentionally share
/// `TypeNode::Float64` (one node, one identity).
fn encode_type(out: &mut String, ty: &TypeNode) {
    match ty {
        TypeNode::Bool => out.push_str("bool"),
        TypeNode::Nat => out.push_str("nat"),
        TypeNode::Int => out.push_str("int"),
        TypeNode::Rational => out.push_str("rational"),
        TypeNode::Float64 => out.push_str("float64"),
        TypeNode::Refinement { base, predicate } => {
            out.push_str("refinement:");
            out.push_str(predicate);
            out.push(':');
            encode_type(out, base);
        }
        TypeNode::Interval(inner) => {
            out.push_str("interval:");
            encode_type(out, inner);
        }
        TypeNode::Complex(inner) => {
            out.push_str("complex:");
            encode_type(out, inner);
        }
        TypeNode::Vector { element, extent } => {
            out.push_str("vector:");
            encode_type(out, element);
            out.push(':');
            out.push_str(
                &extent
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "-".to_string()),
            );
        }
        TypeNode::Matrix {
            element,
            rows,
            cols,
        } => {
            out.push_str("matrix:");
            encode_type(out, element);
            out.push(':');
            out.push_str(
                &rows
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "-".to_string()),
            );
            out.push(':');
            out.push_str(
                &cols
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "-".to_string()),
            );
        }
        TypeNode::Tensor { element, shape } => {
            out.push_str("tensor:");
            encode_type(out, element);
            out.push(':');
            out.push_str(
                &shape
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("x"),
            );
        }
        TypeNode::Record(name) => {
            out.push_str("record:");
            out.push_str(&name.0);
        }
        TypeNode::Variant(name) => {
            out.push_str("variant:");
            out.push_str(&name.0);
        }
        TypeNode::Result { ok, error } => {
            out.push_str("result:");
            encode_type(out, ok);
            out.push(':');
            encode_type(out, error);
        }
        TypeNode::OptionType(inner) => {
            out.push_str("option:");
            encode_type(out, inner);
        }
        TypeNode::Opaque {
            name,
            provider_contract,
        } => {
            out.push_str("opaque:");
            out.push_str(&name.0);
            out.push(':');
            out.push_str(provider_contract.as_ref().map_or("-", |s| &s.0));
        }
        TypeNode::UnitRef { name } => {
            out.push_str("unit-ref:");
            out.push_str(name);
        }
        TypeNode::Other(name) => {
            out.push_str("other:");
            out.push_str(&name.0);
        }
    }
}

fn encode_field(out: &mut String, package: &SemanticPackage, field: &Field, tag: &str) {
    out.push_str(tag);
    out.push(' ');
    out.push_str(&field.name);
    out.push(' ');
    encode_type(out, &package.types[field.ty.index()]);
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
        ExprNode::Vector(elements) => {
            push_str(out, "vector");
            for &element in elements {
                encode_expr(out, exprs, element);
            }
        }
        ExprNode::Matrix(rows) => {
            push_str(out, "matrix");
            for row in rows {
                push_str(out, "row");
                for &element in row {
                    encode_expr(out, exprs, element);
                }
            }
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
        if let Some(about) = &declaration.about {
            out.push_str("about ");
            out.push_str(about);
            out.push('\n');
        }
        for claim in &declaration.evidence {
            out.push_str("claim ");
            out.push_str(&claim.id);
            out.push(' ');
            out.push_str(&claim.statement);
            out.push(' ');
            out.push_str(&claim.class);
            out.push('\n');
        }
        for binding in &declaration.host {
            out.push_str("host ");
            out.push_str(&binding.language);
            out.push(' ');
            out.push_str(&binding.trait_path);
            out.push(' ');
            out.push_str(&binding.target);
            out.push('\n');
            for method in &binding.methods {
                out.push_str("host-method ");
                out.push_str(&method.name);
                out.push('\n');
            }
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
                if !goal.payload.wrt.is_empty() {
                    out.push_str("wrt ");
                    out.push_str(&goal.payload.wrt.join(","));
                    out.push('\n');
                }
                if let Some(order) = goal.payload.order {
                    out.push_str("order ");
                    out.push_str(&order.to_string());
                    out.push('\n');
                }
                if let Some(against) = &goal.payload.against {
                    out.push_str("against ");
                    out.push_str(against);
                    out.push('\n');
                }
                if !goal.payload.measure.is_empty() {
                    out.push_str("measure ");
                    out.push_str(&goal.payload.measure.join(","));
                    out.push('\n');
                }
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
                    encode_type(&mut out, &package.types[error_ty.index()]);
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
                    if let Some(expect) = test.expect {
                        out.push_str("expect\n");
                        encode_expr(&mut out, &package.exprs, expect);
                    }
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

// Identity tests moved to `tests/emath-ir` (canonical.rs + src/lib.rs).
