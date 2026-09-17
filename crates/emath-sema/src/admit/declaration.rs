//! Declaration admission: the constructor declaration entry point and
//! its `AdmitResult` type.

use emath_core::tree::{Section, StmtKind};
use emath_core::{Diagnostics, QualifiedName, Span};
use emath_ir::constructor::{Field, TestCase, Visibility};
use emath_ir::{
    CompileSpec, Declaration, ExprNode, KindSchema, LawMetadata, Provenance, RepeatPolicy,
    TypeNode,
};
use std::collections::{BTreeMap, BTreeSet};

use super::{Admitter, PHASE1_SECTIONS, TraceEntry};

mod setup;

use setup::admit_declaration_setup;

/// Admit one declaration into SIR. Returns (declaration, test cases, type
/// arena, expression arena, trace, diagnostics, law metadata, binding
/// provenance).
pub(super) type AdmitResult = (
    Option<Declaration>,
    Vec<TestCase>,
    Vec<TypeNode>,
    Vec<(ExprNode, Span)>,
    Vec<TraceEntry>,
    Diagnostics,
    Option<LawMetadata>,
    BTreeMap<String, Provenance>,
);

/// Constructor signature shell: kind, sections, and typed `inputs:` /
/// `outputs:` fields. Expression typing is `admit_tree`. Definitions
/// are not lowered through capability cells.
pub(super) fn admit_constructor_declaration(
    decl: &emath_core::tree::Declaration,
) -> AdmitResult {
    let host_types = BTreeSet::new();
    let (mut admitter, kind_label, _, _, _, _, _) =
        admit_declaration_setup(decl, &host_types);
    let inputs = constructor_io_fields(&mut admitter, decl, "inputs");
    let outputs = constructor_io_fields(&mut admitter, decl, "outputs");
    let declaration = Declaration {
        id: emath_ir::DeclarationId(0),
        name: QualifiedName::single(decl.name.clone()),
        kind: QualifiedName::single(decl.as_kind.clone()),
        kind_label,
        inputs,
        outputs,
        state: Vec::new(),
        algebraic: Vec::new(),
        constructors: Vec::new(),
        definitions: BTreeMap::new(),
        invariants: Vec::new(),
        goals: Vec::new(),
        tests: Vec::new(),
        exports: Vec::new(),
        compile_spec: CompileSpec::default(),
        about: None,
        evidence: Vec::new(),
        host: Vec::new(),
        source: decl.source,
    };
    (
        Some(declaration),
        Vec::new(),
        admitter.types,
        admitter.exprs,
        admitter.trace,
        admitter.diagnostics,
        None,
        BTreeMap::new(),
    )
}

fn constructor_io_fields(
    admitter: &mut Admitter,
    decl: &emath_core::tree::Declaration,
    section_name: &str,
) -> Vec<Field> {
    let mut fields = Vec::new();
    for section in decl.sections().filter(|section| section.name == section_name) {
        for stmt in &section.suite.statements {
            let StmtKind::FieldDecl { name, ty, .. } = &stmt.kind else {
                continue;
            };
            if let Some(catalog) = leftover_catalog_type_name(ty) {
                admitter.error(
                    "E-KIND-GONE",
                    format!(
                        "type `{catalog}` is not a constructor primitive; write Bool, Int, Rat, Float64, or a user `emath object`"
                    ),
                    stmt.source,
                );
            }
            let ty_id = admitter.type_id(constructor_type_node(ty));
            fields.push(Field {
                name: name.clone(),
                ty: ty_id,
                visibility: Visibility::Public,
                source: stmt.source,
            });
        }
    }
    fields
}

fn leftover_catalog_type_name(ty: &emath_core::tree::TypeExpr) -> Option<&str> {
    use emath_core::tree::TypeKind;
    match &ty.kind {
        TypeKind::Path { segments, .. } => match segments.last().map(String::as_str) {
            Some(name) if leftover_catalog_type(name) => Some(name),
            _ => None,
        },
        TypeKind::In { base, .. } | TypeKind::Domain { base, .. } | TypeKind::Pow { base, .. } => {
            leftover_catalog_type_name(base)
        }
        TypeKind::Product { left, right, .. } => leftover_catalog_type_name(left)
            .or_else(|| leftover_catalog_type_name(right)),
        TypeKind::List(items) | TypeKind::Tuple(items) => {
            items.iter().find_map(leftover_catalog_type_name)
        }
        TypeKind::Fn { domain, codomain } => leftover_catalog_type_name(domain)
            .or_else(|| leftover_catalog_type_name(codomain)),
        TypeKind::Ref(inner) => leftover_catalog_type_name(inner),
    }
}

fn leftover_catalog_type(name: &str) -> bool {
    matches!(
        name,
        "Real"
            | "Matrix"
            | "Vector"
            | "Tensor"
            | "Graph"
            | "Field"
            | "GF"
            | "Interval"
            | "Set"
            | "Option"
            | "Result"
            | "Measured"
            | "NonNegative"
            | "Duration"
            | "Bytes"
    )
}

fn constructor_type_node(ty: &emath_core::tree::TypeExpr) -> TypeNode {
    use emath_core::tree::TypeKind;
    match &ty.kind {
        TypeKind::Path { segments, .. } => match segments.last().map(String::as_str) {
            Some("Int") | Some("Nat") => TypeNode::Int,
            Some("Bool") => TypeNode::Bool,
            Some("Rat") | Some("Rational") => TypeNode::Rational,
            Some("Float64") | Some("F64") => TypeNode::Float64,
            Some(name) => TypeNode::Other(QualifiedName::single(name)),
            None => TypeNode::Other(QualifiedName::single("Unknown")),
        },
        TypeKind::Fn { .. } => TypeNode::Other(QualifiedName::single("Fn")),
        TypeKind::List(_) => TypeNode::Other(QualifiedName::single("sequence")),
        TypeKind::Tuple(_) => TypeNode::Other(QualifiedName::single("tuple")),
        _ => TypeNode::Other(QualifiedName::single("Unknown")),
    }
}

