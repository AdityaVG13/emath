//! Declaration admission: the `admit_declaration` entry point and its
//! `AdmitResult` type, extracted from `admit.rs` isomorphically.

use emath_core::tree::{Section, StmtKind};
use emath_core::{Diagnostics, QualifiedName, Span};
use emath_ir::constructor::{Constructor, Field, TestCase, Visibility};
use emath_ir::{
    BinaryOp, Declaration, EventAction, EventDecl, ExprId, ExprNode, Extent, KindSchema,
    LawMetadata, ModelResidual, Provenance, RepeatPolicy, TransitionAction, TransitionDecl,
    TypeNode,
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

/// Admit one declaration into SIR. Returns (declaration, test cases, type
/// arena, expression arena, trace, diagnostics).
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
        kind: QualifiedName::single(if is_policy {
            "policy"
        } else if is_model {
            "model"
        } else if is_law {
            "law"
        } else {
            "function"
        }),
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
