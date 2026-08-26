//! Declaration admission: the `admit_declaration` entry point and its
//! `AdmitResult` type, extracted from `admit.rs` isomorphically.

use emath_core::tree::{Section, StmtKind};
use emath_core::{Diagnostics, QualifiedName, Span};
use emath_ir::constructor::{Constructor, Field, TestCase, Visibility};
use emath_ir::{
    BinaryOp, Declaration, ExprId, ExprNode, Extent, KindSchema, ModelResidual, RepeatPolicy,
    TypeNode,
};
use std::collections::{BTreeMap, BTreeSet};

use super::equations::{admit_equations, collect_node_names, residual_span};
use super::infer::{Infer, infer_conforms};
use super::sections::{admit_compile_spec, admit_constructor, admit_named_field};
use super::sections_meta::{admit_about, admit_evidence, admit_host};
use super::{
    Admitter, E_DUPLICATE_FIELD, E_UNKNOWN_VARIABLE, E_UNSUPPORTED_TYPE, PHASE1_SECTIONS,
    TraceEntry,
};

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
);

/// Admit one declaration into SIR. Returns (declaration, test cases, type
/// arena, expression arena, trace, diagnostics).
pub(super) fn admit_declaration(
    decl: &emath_core::tree::Declaration,
    host_types: &BTreeSet<String>,
) -> AdmitResult {
    let mut admitter = Admitter::new();
    admitter.host_types = host_types.clone();
    let kind_label = decl.as_kind.clone();
    let is_policy = kind_label == "policy";
    let is_model = kind_label == "model";
    let schema = if is_policy {
        KindSchema::core_policy()
    } else if is_model {
        KindSchema::core_model()
    } else {
        KindSchema::core_function()
    };

    // Section collection with duplicate detection (E-SYN-103).
    let mut by_name: BTreeMap<&str, &Section> = BTreeMap::new();
    for section in decl.sections() {
        if let Some(previous) = by_name.get(section.name.as_str()) {
            admitter.error(
                "E-SYN-103",
                format!(
                    "duplicate section `{}` (first declared at bytes {}..{})",
                    section.name, previous.source.start, previous.source.end
                ),
                section.source,
            );
        } else {
            by_name.insert(&section.name, section);
        }
    }

    // Kind schema is the required/optional source of truth (`E-KIND-011`).
    for (name, section_schema) in schema.sections() {
        if section_schema.repeat == RepeatPolicy::ExactlyOne && !by_name.contains_key(name) {
            admitter.error(
                "E-KIND-011",
                format!("kind `{}` requires section `{name}`", schema.name()),
                decl.head_source,
            );
        }
    }

    // Phase 1 whitelist: a section outside the subset is a typed refusal,
    // never a silent drop (AGENTS.md rule 6). `request:` / `requests:`
    // are the pre-`goals:` spellings; refuse with a migration hint.
    for section in decl.sections() {
        if matches!(section.name.as_str(), "request" | "requests") {
            admitter.error(
                "E-SEC-101",
                format!(
                    "section `{}:` was renamed to `goals:`; use `goals:`",
                    section.name
                ),
                section.head_source,
            );
            continue;
        }
        if !PHASE1_SECTIONS.contains(&section.name.as_str()) {
            admitter.error(
                "E-SEC-101",
                format!(
                    "section `{}` is outside the Phase 1 subset (known: {})",
                    section.name,
                    PHASE1_SECTIONS.join(", ")
                ),
                section.head_source,
            );
        }
    }

    // Fields: inputs, outputs, state. Head-args lower into the same Field
    // IR as an `inputs:` section. `-> T` declares a single output named
    // after the declaration (the example `square = x * x` binds the
    // declaration name). Mixing the head spelling with the equivalent
    // section forks identity and is refused.
    let mut fields_infer: BTreeMap<String, Infer> = BTreeMap::new();
    let mut fields_by_section: BTreeMap<&str, Vec<Field>> = BTreeMap::new();
    let mut outputs_from_head = false;
    if let Some(signature) = &decl.signature {
        let stateful = by_name.contains_key("state") || by_name.contains_key("constructors");
        let refuse_head = kind_label != "function" || stateful;
        if refuse_head {
            admitter.error(
                "E-SYN-123",
                "declaration head arguments are only admitted on stateless `emath function` declarations (no `state:` or `constructors:`)",
                decl.head_source,
            );
        }
        if by_name.contains_key("inputs") {
            admitter.error(
                "E-SYN-122",
                "declaration head arguments cannot be mixed with an `inputs:` section; use one spelling",
                decl.head_source,
            );
        }
        if signature.ret.is_some() && by_name.contains_key("outputs") {
            admitter.error(
                "E-SYN-122",
                "declaration head `->` return type cannot be mixed with an `outputs:` section; use one spelling",
                decl.head_source,
            );
        }
        let mix_inputs = by_name.contains_key("inputs");
        let mix_outputs = signature.ret.is_some() && by_name.contains_key("outputs");
        if !refuse_head && !mix_inputs {
            for param in &signature.params {
                if param.by_ref {
                    admitter.error(
                        "E-SYN-101",
                        "by-ref declaration head arguments are outside the Phase 1 subset",
                        param.source,
                    );
                    continue;
                }
                if param.default.is_some() {
                    admitter.error(
                        "E-SYN-101",
                        "default values on declaration head arguments are outside the Phase 1 subset",
                        param.source,
                    );
                    continue;
                }
                let _ = admit_named_field(
                    &mut admitter,
                    &mut fields_infer,
                    &mut fields_by_section,
                    "inputs",
                    &param.name,
                    &param.ty,
                    param.source,
                    true,
                );
            }
        }
        if !refuse_head && !mix_outputs {
            if let Some(ret) = &signature.ret {
                outputs_from_head = admit_named_field(
                    &mut admitter,
                    &mut fields_infer,
                    &mut fields_by_section,
                    "outputs",
                    &decl.name,
                    ret,
                    ret.source,
                    false,
                );
            }
        }
    }

    for section_name in ["inputs", "outputs", "state"] {
        if let Some(section) = by_name.get(section_name) {
            for stmt in &section.suite.statements {
                let StmtKind::FieldDecl { name, ty, .. } = &stmt.kind else {
                    admitter.error(
                        "E-SYN-101",
                        format!("only `name: Type` declarations are allowed in `{section_name}`"),
                        stmt.source,
                    );
                    continue;
                };
                let _ = admit_named_field(
                    &mut admitter,
                    &mut fields_infer,
                    &mut fields_by_section,
                    section_name,
                    name,
                    ty,
                    stmt.source,
                    section_name == "inputs",
                );
            }
        }
    }

    let inputs = fields_by_section.get("inputs").cloned().unwrap_or_default();
    let outputs_omitted = !by_name.contains_key("outputs") && !outputs_from_head;
    let mut outputs_raw = fields_by_section
        .get("outputs")
        .cloned()
        .unwrap_or_default();
    let state = fields_by_section.get("state").cloned().unwrap_or_default();
    // `algebraic:` variables are the unknowns of the implicit residual
    // system (causalized DAEs); initial guesses are supplied at simulate
    // time in the same map as `inputs:` values.
    if let Some(section) = by_name.get("algebraic") {
        if !is_model {
            admitter.error(
                "E-KIND-010",
                "`algebraic:` is only admitted on `emath model` declarations",
                section.source,
            );
        } else {
            for stmt in &section.suite.statements {
                let StmtKind::FieldDecl { name, ty, .. } = &stmt.kind else {
                    admitter.error(
                        "E-SYN-101",
                        "only `name: Type` declarations are allowed in `algebraic:`",
                        stmt.source,
                    );
                    continue;
                };
                let _ = admit_named_field(
                    &mut admitter,
                    &mut fields_infer,
                    &mut fields_by_section,
                    "algebraic",
                    name,
                    ty,
                    stmt.source,
                    false,
                );
            }
        }
    }
    let algebraic_fields = fields_by_section
        .get("algebraic")
        .cloned()
        .unwrap_or_default();
    admitter.inputs = inputs
        .iter()
        .map(|f| (f.name.clone(), admitter.type_of(f.ty)))
        .collect();
    for field in &algebraic_fields {
        // Algebraic variables resolve like inputs inside definitions and
        // residuals; the runner binds their guesses from the same value
        // map. They stay out of `Declaration.inputs` (I/O contract).
        admitter
            .inputs
            .entry(field.name.clone())
            .or_insert_with(|| fields_infer.get(&field.name).cloned().unwrap_or(Infer::F64));
    }
    admitter.states = state
        .iter()
        .map(|f| (f.name.clone(), admitter.type_of(f.ty)))
        .collect();

    // Constraints section: process before definitions so the optimizer
    // can access them during definition lowering.  Each statement is an
    // expression that must infer as Bool.
    if let Some(section) = by_name.get("constraints") {
        for stmt in &section.suite.statements {
            let StmtKind::Expr(expr) = &stmt.kind else {
                admitter.error(
                    "E-SYN-101",
                    "only expressions are allowed in `constraints:`",
                    stmt.source,
                );
                continue;
            };
            match admitter.lower_expr(expr) {
                Some((id, Infer::Bool)) => admitter.constraints.push(id),
                Some((_, infer)) => {
                    admitter.error(
                        "E-TYPE-012",
                        format!("constraint must be Bool, got {infer}"),
                        expr.source,
                    );
                }
                None => {}
            }
        }
    }

    // Invariant section: each statement is a claim (Bool) that must hold.
    // Uses lower_requirement so claim expressions (limit, series, asymp)
    // are admitted as Bool(true) rather than erroring.
    if let Some(section) = by_name.get("invariant") {
        for stmt in &section.suite.statements {
            let expr = match &stmt.kind {
                StmtKind::Expr(e) => e,
                StmtKind::Require(e) | StmtKind::Ensure(e) | StmtKind::Invariant(e) => e,
                _ => {
                    admitter.error(
                        "E-SYN-101",
                        "only expressions are allowed in `invariant:`",
                        stmt.source,
                    );
                    continue;
                }
            };
            if let Some(id) = admitter.lower_requirement(expr) {
                admitter.constraints.push(id);
            }
        }
    }

    // Definitions.
    let mut definitions: BTreeMap<String, ExprId> = BTreeMap::new();
    if let Some(section) = by_name.get("definitions") {
        for stmt in &section.suite.statements {
            let StmtKind::Assign { target, value } = &stmt.kind else {
                admitter.error(
                    "E-SYN-101",
                    "only `name = expression` definitions are allowed in Phase 1",
                    stmt.source,
                );
                continue;
            };
            if target.segments.len() != 1 || !target.indices.is_empty() {
                admitter.error(
                    E_UNSUPPORTED_TYPE,
                    "indexed and nested definitions are outside the Phase 1 subset",
                    target.source,
                );
                continue;
            }
            let name = &target.segments[0];
            if definitions.contains_key(name) {
                admitter.error(
                    E_DUPLICATE_FIELD,
                    format!("duplicate definition `{name}`"),
                    target.source,
                );
                continue;
            }
            match admitter.lower_expr(value) {
                Some((
                    id,
                    infer @ (Infer::F64
                    | Infer::Nat
                    | Infer::Int
                    | Infer::Complex
                    | Infer::Bool
                    | Infer::Unit { .. }
                    | Infer::HostDeferred
                    | Infer::Vector { .. }
                    | Infer::Matrix { .. }
                    | Infer::Tensor { .. }),
                )) => {
                    if let Some(declared) = outputs_raw
                        .iter()
                        .find(|output| output.name == *name)
                        .map(|output| admitter.type_of(output.ty))
                    {
                        if !infer_conforms(&infer, &declared) {
                            admitter.error(
                                "E-TYPE-012",
                                format!(
                                    "definition `{name}` has type {infer}, expected {declared}"
                                ),
                                value.source,
                            );
                        }
                    }
                    admitter.record("sema", format!("definition `{name}` typed"), value.source);
                    definitions.insert(name.clone(), id);
                    // Later definitions may name earlier ones (`b = a * a`).
                    admitter.definitions.insert(name.clone(), (id, infer));
                }
                Some((_, Infer::Opaque)) => {
                    admitter.error(
                        "E-TYPE-012",
                        format!("definition `{name}` must be numeric; opaque host values are not scalars"),
                        value.source,
                    );
                }
                None => {}
            }
        }
    }
    admit_equations(&mut admitter, &by_name, &mut definitions, is_model);
    if is_model
        && definitions.is_empty()
        && !by_name.contains_key("definitions")
        && !by_name.contains_key("equations")
        && !by_name.contains_key("equation")
    {
        admitter.error(
            "E-KIND-011",
            "kind `model` requires section `definitions` or `equations`",
            decl.head_source,
        );
    }
    if is_model && (by_name.contains_key("equations") || by_name.contains_key("equation")) {
        let residual_rates: BTreeSet<String> = admitter
            .residuals
            .iter()
            .flat_map(|residual| residual.rates.iter().cloned())
            .collect();
        for field in &state {
            let rate_name = format!("der_{}", field.name);
            if !definitions.contains_key(&rate_name)
                && !residual_rates.contains(field.name.as_str())
            {
                admitter.error(
                    "E-NAME-025",
                    format!(
                        "state `{}` has no `derivative({})` equation",
                        field.name, field.name
                    ),
                    field.source,
                );
            }
        }
    }
    // Causalization validation: the implicit residual system must be
    // square (unknown components == residual components) and every
    // declared `algebraic:` variable must be referenced by a residual.
    if is_model && !admitter.residuals.is_empty() {
        let mut unknown_dims: Vec<(String, usize)> = Vec::new();
        for field in &algebraic_fields {
            match fields_infer.get(&field.name) {
                Some(Infer::F64) => unknown_dims.push((field.name.clone(), 1)),
                Some(Infer::Vector {
                    extent: Some(Extent::Fixed(n)),
                }) => unknown_dims.push((field.name.clone(), *n)),
                _ => {
                    admitter.error(
                        E_UNSUPPORTED_TYPE,
                        format!(
                            "algebraic variable `{}` must be a Float64 scalar or a fixed-length vector of Float64",
                            field.name
                        ),
                        field.source,
                    );
                    unknown_dims.push((field.name.clone(), 0));
                }
            }
        }
        let rate_unknowns: Vec<(String, ExprId)> = admitter
            .residuals
            .iter()
            .flat_map(|residual| {
                residual
                    .rates
                    .iter()
                    .map(|rate| (rate.clone(), residual.expr))
            })
            .collect();
        for (rate, residual_expr) in &rate_unknowns {
            match admitter.states.get(rate) {
                Some(Infer::F64) => unknown_dims.push((format!("der({rate})"), 1)),
                Some(Infer::Vector {
                    extent: Some(Extent::Fixed(n)),
                }) => unknown_dims.push((format!("der({rate})"), *n)),
                _ => {
                    admitter.error(
                        E_UNSUPPORTED_TYPE,
                        format!(
                            "rate unknown `der({rate})` must derive a Float64 scalar or fixed-length vector state"
                        ),
                        residual_span(&admitter, *residual_expr),
                    );
                    unknown_dims.push((format!("der({rate})"), 0));
                }
            }
        }
        let unknown_total: usize = unknown_dims.iter().map(|(_, dims)| dims).sum();
        let residual_total: usize = admitter
            .residuals
            .iter()
            .map(|residual| residual.components as usize)
            .sum();
        if unknown_total == 0 {
            admitter.error(
                E_UNSUPPORTED_TYPE,
                format!(
                    "implicit residual has no unknown to solve for; declare `algebraic:` variables or write explicit `der(state) = rhs` rates"
                ),
                residual_span(&admitter, admitter.residuals[0].expr),
            );
        } else if unknown_total != residual_total {
            admitter.error(
                E_UNSUPPORTED_TYPE,
                format!(
                    "implicit residual system is not square: {} unknown component(s) ([{}]) vs {} residual component(s); every `algebraic:` variable must participate in the residual equations",
                    unknown_total,
                    unknown_dims
                        .iter()
                        .map(|(name, dims)| format!("{name}:{dims}"))
                        .collect::<Vec<_>>()
                        .join(", "),
                    residual_total
                ),
                residual_span(&admitter, admitter.residuals[0].expr),
            );
        }
        let mut referenced: BTreeSet<String> = BTreeSet::new();
        for residual in &admitter.residuals {
            if let Some((node, _)) = admitter.exprs.get(residual.expr.0 as usize) {
                collect_node_names(&admitter.exprs, node, &mut referenced);
            }
        }
        for field in &algebraic_fields {
            if !referenced.contains(&field.name) {
                admitter.error(
                    E_UNKNOWN_VARIABLE,
                    format!(
                        "algebraic variable `{}` is not referenced by any implicit residual equation",
                        field.name
                    ),
                    field.source,
                );
            }
        }
        for residual in &mut admitter.residuals {
            residual.algebraic = algebraic_fields
                .iter()
                .map(|field| field.name.clone())
                .collect();
        }
    } else if is_model && !algebraic_fields.is_empty() {
        admitter.error(
            E_UNSUPPORTED_TYPE,
            "`algebraic:` variables are only solved by at least one implicit residual equation in `equations:`",
            algebraic_fields[0].source,
        );
    }
    for output in &outputs_raw {
        if !definitions.contains_key(&output.name) {
            admitter.error(
                "E-NAME-023",
                format!("output `{}` has no definition", output.name),
                output.source,
            );
        }
    }
    if outputs_omitted && schema.default_for("outputs") == Some("definitions") {
        for name in definitions.keys() {
            if name.starts_with("der_") {
                continue;
            }
            let infer = admitter
                .definitions
                .get(name)
                .map(|(_, inf)| inf.clone())
                .unwrap_or(Infer::F64);
            let node = match infer {
                Infer::Bool => TypeNode::Bool,
                Infer::Complex => TypeNode::Complex(Box::new(TypeNode::Float64)),
                Infer::Vector { extent } => TypeNode::Vector {
                    element: Box::new(TypeNode::Float64),
                    extent,
                },
                Infer::Matrix { rows, cols } => TypeNode::Matrix {
                    element: Box::new(TypeNode::Float64),
                    rows,
                    cols,
                },
                Infer::Tensor { shape } => TypeNode::Tensor {
                    element: Box::new(TypeNode::Float64),
                    shape,
                },
                Infer::Nat => TypeNode::Nat,
                Infer::Int => TypeNode::Int,
                _ => TypeNode::Float64,
            };
            let ty = admitter.type_id(node);
            outputs_raw.push(Field {
                name: name.clone(),
                ty,
                visibility: Visibility::Public,
                source: decl.source,
            });
        }
    }

    // Constructors.
    let mut constructors: Vec<Constructor> = Vec::new();
    if is_policy || is_model {
        if let Some(section) = by_name.get("constructors") {
            for stmt in &section.suite.statements {
                if let StmtKind::FnDecl {
                    visibility,
                    name,
                    params,
                    ret,
                    suite,
                    ..
                } = &stmt.kind
                {
                    if name != "new"
                        || !matches!(visibility, Some(emath_core::tree::Visibility::Public))
                    {
                        admitter.error(
                            "E-CTOR-036",
                            format!(
                                "Phase 1 admits exactly one public `new` constructor, found `{name}`"
                            ),
                            stmt.source,
                        );
                        continue;
                    }
                    if !constructors.is_empty() {
                        admitter.error(
                            "E-CTOR-036",
                            "multiple public `new` constructors are outside the Phase 1 subset",
                            stmt.source,
                        );
                        continue;
                    }
                    let mut constructor = admit_constructor(
                        &mut admitter,
                        params,
                        ret.as_ref(),
                        suite.as_ref(),
                        stmt.source,
                    );
                    constructor.name.clone_from(name);
                    constructor.is_public = true;
                    constructors.push(constructor);
                } else {
                    admitter.error(
                        "E-SYN-101",
                        "expected `public fn new(...)` inside `constructors:`",
                        stmt.source,
                    );
                }
            }
        } else if is_policy {
            admitter.error(
                "E-CTOR-031",
                "policy declarations require a `constructors:` section with a public `new`",
                decl.head_source,
            );
        }
        // Constructor assignments must cover all state fields.
        if let Some(constructor) = constructors.first() {
            for field in &state {
                if !constructor.assignments.contains_key(&field.name) {
                    admitter.error(
                        "E-CTOR-030",
                        format!("missing state assignment for `{}`", field.name),
                        decl.head_source,
                    );
                }
            }
        }
    } else if let Some(section) = by_name.get("constructors") {
        admitter.error(
            "E-KIND-010",
            "function declarations cannot have state or constructors in Phase 1",
            section.source,
        );
    }
    if !is_policy && !is_model && !state.is_empty() {
        admitter.error(
            "E-KIND-010",
            "function declarations cannot have state in Phase 1",
            decl.head_source,
        );
    }

    // Compile spec.
    let compile_spec = admit_compile_spec(&mut admitter, by_name.get("compile").copied());

    // Exports.
    let mut exports = Vec::new();
    if let Some(section) = by_name.get("exports") {
        for stmt in &section.suite.statements {
            let StmtKind::Command { head, .. } = &stmt.kind else {
                admitter.error(
                    "E-SYN-101",
                    "exports must be `public <kind> <name>` commands",
                    stmt.source,
                );
                continue;
            };
            let mut words = head.iter().map(String::as_str);
            let visibility_word = words.next().unwrap_or("");
            let kind = words.next().unwrap_or("");
            let name = words.next().unwrap_or("");
            let public = visibility_word == "public";
            if !public {
                admitter.error(
                    "E-NAME-021",
                    "Phase 1 exports must be `public`",
                    stmt.source,
                );
                continue;
            }
            match kind {
                "constructor" => {
                    if name != "new" || constructors.is_empty() {
                        admitter.error(
                            "E-NAME-021",
                            format!("exported constructor `{name}` does not exist"),
                            stmt.source,
                        );
                        continue;
                    }
                    exports.push(emath_ir::goal::Export {
                        kind: "constructor".into(),
                        name: name.to_string(),
                        is_public: true,
                    });
                }
                "function" => {
                    let from_diff = name.strip_prefix("gradient_").is_some_and(|target| {
                        by_name.get("goals").is_some_and(|section| {
                            section.suite.statements.iter().any(|stmt| {
                                matches!(
                                    &stmt.kind,
                                    StmtKind::Section(goal)
                                        if goal.name == "differentiate"
                                            && goal.generic.as_deref() == Some(target)
                                )
                            })
                        })
                    });
                    if !definitions.contains_key(name)
                        && !outputs_raw.iter().any(|o| o.name == *name)
                        && !from_diff
                    {
                        admitter.error(
                            "E-NAME-021",
                            format!("exported function `{name}` does not exist"),
                            stmt.source,
                        );
                        continue;
                    }
                    exports.push(emath_ir::goal::Export {
                        kind: "function".into(),
                        name: name.to_string(),
                        is_public: true,
                    });
                }
                "type" => {
                    if name != decl.name {
                        admitter.error(
                            "E-NAME-021",
                            format!("exported type `{name}` does not exist"),
                            stmt.source,
                        );
                        continue;
                    }
                    exports.push(emath_ir::goal::Export {
                        kind: "type".into(),
                        name: name.to_string(),
                        is_public: true,
                    });
                }
                other => {
                    admitter.error(
                        "E-NAME-021",
                        format!("unsupported export kind `{other}`"),
                        stmt.source,
                    );
                }
            }
        }
    }

    // Tests.
    let mut tests: Vec<TestCase> = Vec::new();
    if let Some(section) = by_name.get("tests") {
        for stmt in &section.suite.statements {
            let StmtKind::Section(example) = &stmt.kind else {
                admitter.error(
                    "E-SYN-101",
                    "expected `example <name>:` blocks inside `tests:`",
                    stmt.source,
                );
                continue;
            };
            if example.name != "example" {
                admitter.error(
                    "E-SYN-101",
                    format!("unknown test block `{}`", example.name),
                    example.source,
                );
                continue;
            }
            let mut given: BTreeMap<String, ExprId> = BTreeMap::new();
            let mut expect: Option<ExprId> = None;
            for inner in &example.suite.statements {
                match &inner.kind {
                    StmtKind::Given { name, value } => {
                        if !admitter.inputs.contains_key(name)
                            && !admitter.params.contains_key(name)
                            && !(is_model && admitter.states.contains_key(name))
                        {
                            admitter.error(
                                "E-NAME-026",
                                format!(
                                    "`given` name `{name}` is not an input, constructor parameter, or model state field"
                                ),
                                inner.source,
                            );
                            continue;
                        }
                        match admitter.lower_expr(value) {
                            Some((
                                id,
                                Infer::F64
                                | Infer::Nat
                                | Infer::Int
                                | Infer::Complex
                                | Infer::Unit { .. }
                                | Infer::HostDeferred
                                | Infer::Vector { .. }
                                | Infer::Matrix { .. }
                                | Infer::Tensor { .. },
                            )) => {
                                given.insert(name.clone(), id);
                            }
                            Some((_, Infer::Bool)) => {
                                admitter.error(
                                    "E-TYPE-012",
                                    format!("`given {name}` must be numeric or tensor"),
                                    inner.source,
                                );
                            }
                            Some((_, Infer::Opaque)) => {
                                admitter.error(
                                    "E-TYPE-012",
                                    format!("`given {name}` must be numeric; opaque host values are not scalars"),
                                    inner.source,
                                );
                            }
                            None => {}
                        }
                    }
                    StmtKind::Expect(expr) => match admitter.lower_expr(expr) {
                        Some((id, Infer::Bool)) => {
                            // Multiple `expect` lines are a conjunction; keeping
                            // only the last one silently dropped earlier checks.
                            expect = Some(match expect {
                                Some(prev) => admitter.push_expr(
                                    ExprNode::Binary {
                                        operation: BinaryOp::And,
                                        left: prev,
                                        right: id,
                                    },
                                    inner.source,
                                ),
                                None => id,
                            });
                        }
                        Some((
                            _,
                            Infer::F64
                            | Infer::Nat
                            | Infer::Int
                            | Infer::Complex
                            | Infer::Vector { .. }
                            | Infer::Matrix { .. }
                            | Infer::Tensor { .. }
                            | Infer::Unit { .. }
                            | Infer::HostDeferred
                            | Infer::Opaque,
                        )) => {
                            admitter.error(
                                "E-TYPE-012",
                                "`expect` must be a Boolean comparison",
                                inner.source,
                            );
                        }
                        None => {}
                    },
                    other => {
                        let _ = other;
                        admitter.error(
                            "E-SYN-101",
                            "only `given x = ...` and `expect ...` are allowed in example blocks",
                            inner.source,
                        );
                    }
                }
            }
            if is_policy || (is_model && !constructors.is_empty()) {
                // constructor parameters must be supplied by `given` values
                let constructor_params: Vec<String> = constructors
                    .first()
                    .map(|c| c.parameters.iter().map(|p| p.name.clone()).collect())
                    .unwrap_or_default();
                for param in &constructor_params {
                    if !given.contains_key(param) {
                        admitter.error(
                            "E-NAME-027",
                            format!(
                                "policy example `{}` must supply constructor parameter `{param}` via `given`",
                                example.generic.clone().unwrap_or_default()
                            ),
                            example.source,
                        );
                    }
                }
            }
            if is_model && constructors.is_empty() {
                for field in &state {
                    if !given.contains_key(&field.name) {
                        admitter.error(
                            "E-NAME-027",
                            format!(
                                "model example `{}` must supply state `{name}` via `given`",
                                example.generic.clone().unwrap_or_default(),
                                name = field.name
                            ),
                            example.source,
                        );
                    }
                }
            }
            tests.push(TestCase {
                name: example
                    .generic
                    .clone()
                    .unwrap_or_else(|| format!("test_{}", tests.len())),
                given,
                expect,
                source: example.source,
            });
        }
    }

    // Rebuild inputs/outputs/state as neutral fields.
    let input_fields = inputs.clone();
    let output_fields = outputs_raw.clone();
    let state_fields = state.clone();

    let about = admit_about(&mut admitter, by_name.get("about").copied());
    let evidence = admit_evidence(&mut admitter, by_name.get("evidence").copied());
    let host = admit_host(&mut admitter, by_name.get("host").copied());

    let declaration = Declaration {
        id: emath_ir::DeclarationId(0),
        name: QualifiedName::single(decl.name.clone()),
        kind: QualifiedName::single(if is_policy {
            "policy"
        } else if is_model {
            "model"
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
    )
}
