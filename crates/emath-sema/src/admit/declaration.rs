//! Declaration admission: the `admit_declaration` entry point and its
//! `AdmitResult` type, extracted from `admit.rs` isomorphically.

use emath_core::tree::{Section, StmtKind};
use emath_core::{Diagnostics, QualifiedName, Span};
use emath_ir::constructor::{Constructor, Field, TestCase, Visibility};
use emath_ir::{
    BinaryOp, CompileSpec, Declaration, EventAction, EventDecl, ExprId, ExprNode, Extent,
    KindSchema, LawMetadata, ModelResidual, Provenance, RepeatPolicy, TransitionAction,
    TransitionDecl, TypeNode,
};
use std::collections::{BTreeMap, BTreeSet};

use super::equations::{admit_equations, collect_node_names, residual_span};
use super::infer::{Infer, infer_conforms, infer_from_node};
use super::sections::{admit_compile_spec, admit_constructor, admit_named_field};
use super::sections_meta::{
    admit_about, admit_binding_provenance, admit_evidence, admit_host, admit_law_metadata,
};
use super::types::map_type;
use super::{
    Admitter, E_DUPLICATE_FIELD, E_UNKNOWN_VARIABLE, E_UNSUPPORTED_TYPE, PHASE1_SECTIONS,
    TraceEntry,
};
use super::{CapabilityCallBinding, SiblingFunction};

mod clauses;
mod definitions;
mod events;
mod exports_tests;
mod fields;
mod setup;
mod transitions;

use clauses::admit_declaration_clauses;
use definitions::admit_declaration_definitions;
use exports_tests::admit_declaration_exports_tests;
use fields::admit_declaration_fields;
use setup::admit_declaration_setup;

/// Admit one declaration into SIR. Returns (declaration, test cases, type
/// arena, expression arena, trace, diagnostics).
pub(super) type AdmitResult = (
    Option<Declaration>,
    Vec<TestCase>,
    Vec<TypeNode>,
    Vec<(ExprNode, Span)>,
    Vec<TraceEntry>,
    Diagnostics,
    Vec<ModelResidual>,
    Vec<EventDecl>,
    Vec<TransitionDecl>,
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
    let cells: &[CapabilityCallBinding] = &[];
    let siblings = BTreeMap::new();
    let (mut admitter, kind_label, _, _, _, _, _) =
        admit_declaration_setup(decl, &host_types, cells, &siblings);
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
        Vec::new(),
        Vec::new(),
        Vec::new(),
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

pub(super) fn admit_declaration(
    decl: &emath_core::tree::Declaration,
    host_types: &BTreeSet<String>,
    capability_cells: &[CapabilityCallBinding],
    sibling_functions: &BTreeMap<String, SiblingFunction>,
) -> AdmitResult {
    let (mut admitter, kind_label, is_policy, is_model, is_law, schema, by_name) =
        admit_declaration_setup(decl, host_types, capability_cells, sibling_functions);
    let (fields_infer, inputs, mut outputs_raw, state, algebraic_fields, outputs_omitted) =
        admit_declaration_fields(&mut admitter, &by_name, decl, is_model, is_law, &kind_label);
    let observation_names = admit_declaration_clauses(&mut admitter, &by_name);
    let (definitions, constructors) = admit_declaration_definitions(
        &mut admitter,
        &by_name,
        decl,
        &kind_label,
        is_policy,
        is_model,
        &schema,
        &fields_infer,
        &mut outputs_raw,
        outputs_omitted,
        &state,
        &algebraic_fields,
        &observation_names,
    );
    let (compile_spec, exports, tests) = admit_declaration_exports_tests(
        &mut admitter,
        &by_name,
        decl,
        &kind_label,
        is_policy,
        is_model,
        &inputs,
        &outputs_raw,
        &state,
        &definitions,
        &constructors,
    );
    let input_fields = inputs.clone();
    let output_fields = outputs_raw.clone();
    let state_fields = state.clone();
    let known_bindings = input_fields
        .iter()
        .chain(&output_fields)
        .chain(&state_fields)
        .chain(&algebraic_fields)
        .map(|field| field.name.clone())
        .chain(definitions.keys().cloned())
        // Observations carry provenance too (04 §5.2): the instrument
        // run behind a measured datum is named like any other binding.
        .chain(observation_names.iter().cloned())
        .collect();
    let binding_provenance = if is_law {
        BTreeMap::new()
    } else {
        admit_binding_provenance(
            &mut admitter,
            by_name.get("provenance").copied(),
            &known_bindings,
        )
    };

    let about = admit_about(&mut admitter, by_name.get("about").copied());
    let mut evidence = admit_evidence(&mut admitter, by_name.get("evidence").copied());
    let law_metadata = is_law.then(|| {
        admit_law_metadata(
            &mut admitter,
            by_name.get("assumptions").copied(),
            by_name.get("domain").copied(),
            by_name.get("provenance").copied(),
            by_name.get("citations").copied(),
            decl.head_source,
        )
    });
    if let Some(metadata) = &law_metadata {
        if evidence.is_empty() {
            admitter.error(
                "E-LAW-002",
                "`emath law` requires at least one `evidence:` claim",
                decl.head_source,
            );
        }
        for claim in &mut evidence {
            claim.assumptions = metadata.assumptions.clone();
        }
    }
    let host = admit_host(&mut admitter, by_name.get("host").copied());

    let declaration = Declaration {
        id: emath_ir::DeclarationId(0),
        name: QualifiedName::single(decl.name.clone()),
        kind: QualifiedName::single(decl.as_kind.clone()),
        kind_label,
        inputs: input_fields,
        outputs: output_fields,
        state: state_fields,
        algebraic: algebraic_fields,
        constructors,
        definitions,
        invariants: admitter.constraints.clone(),
        goals: Vec::new(),
        tests: Vec::new(),
        exports,
        compile_spec,
        about,
        evidence,
        host,
        source: decl.source,
    };

    (
        Some(declaration),
        tests,
        admitter.types,
        admitter.exprs,
        admitter.trace,
        admitter.diagnostics,
        admitter.residuals,
        admitter.events,
        admitter.transitions,
        law_metadata,
        binding_provenance,
    )
}
