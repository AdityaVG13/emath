use emath_core::tree::{BinaryOp as SynBinOp, ExprKind};

use super::events::admit_event_payloads;
use super::transitions::admit_transitions;
use super::*;

/// Admit the `definitions:` section and `constructors:` sections
/// (moved verbatim from `admit_declaration`).
pub(super) fn admit_declaration_definitions(
    mut admitter: &mut Admitter,
    by_name: &BTreeMap<&str, &Section>,
    decl: &emath_core::tree::Declaration,
    kind_label: &String,
    is_policy: bool,
    is_model: bool,
    schema: &KindSchema,
    fields_infer: &BTreeMap<String, Infer>,
    outputs_raw: &mut Vec<Field>,
    outputs_omitted: bool,
    state: &[Field],
    algebraic_fields: &[Field],
    observation_names: &BTreeSet<String>,
) -> (BTreeMap<String, ExprId>, Vec<Constructor>) {
    // Definitions.
    let mut definitions: BTreeMap<String, ExprId> = BTreeMap::new();
    if let Some(section) = by_name.get("definitions") {
        for stmt in &section.suite.statements {
            let StmtKind::Assign { target, value } = &stmt.kind else {
                // F6: `=` vs `==` causalization.
                // A `==` statement here is a comparison, not a definition:
                // name both readings instead of a generic shape error.
                let is_eqeq = match &stmt.kind {
                    StmtKind::Expr(expr) => matches!(
                        &expr.kind,
                        ExprKind::Binary {
                            op: SynBinOp::Eq,
                            ..
                        }
                    ),
                    _ => false,
                };
                admitter.error(
                    "E-SYN-101",
                    if is_eqeq {
                        "`definitions:` binds with `=` (the left name takes the value); `==` is a comparison/equation and does not define a name — write `name = lhs == rhs` as a definition, or move the `==` row to `equations:`/`invariant:` where it constrains"
                    } else {
                        "only `name = expression` definitions are allowed in Phase 1"
                    },
                    stmt.source,
                );
                continue;
            };
            if target.segments.len() != 1 {
                admitter.error(
                    E_UNSUPPORTED_TYPE,
                    "nested definition targets are not supported",
                    target.source,
                );
                continue;
            }
            let name = &target.segments[0];
            if !target.indices.is_empty() {
                admitter.error(
                    E_UNSUPPORTED_TYPE,
                    "indexed definition targets require a capsule-provided feature",
                    target.source,
                );
                continue;
            }
            // 04 §5.2: the model/observation
            // line — a definition binding an observation name would let
            // model output silently overwrite measured data.
            if observation_names.contains(name.as_str()) {
                admitter.error(
                    "E-OBS-WRITE",
                    format!(
                        "`{name}` is an observation: observations are read-only measured evidence and are never written by the model — bind a different name for the model quantity"
                    ),
                    target.source,
                );
                continue;
            }
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
                    | Infer::Rat
                    | Infer::BigInt
                    | Infer::Complex
                    | Infer::Bool
                    | Infer::Text
                    | Infer::Set(_)
                    | Infer::Record(_)
                    | Infer::Unit { .. }
                    | Infer::HostDeferred
                    | Infer::Series
                    | Infer::Sequence
                    | Infer::Vector { .. }
                    | Infer::Matrix { .. }
                    | Infer::Tensor { .. }
                    | Infer::OptionCarrier
                    | Infer::ResultCarrier),
                )) => {
                    if let Some(output) = outputs_raw.iter().find(|output| output.name == *name) {
                        let declared = admitter.type_of(output.ty);
                        if !infer_conforms(&infer, &declared) {
                            admitter.error(
                                "E-TYPE-012",
                                format!(
                                    "definition `{name}` has type {infer}, expected {declared}"
                                ),
                                value.source,
                            );
                        }
                        // FieldPrime exactness: a prime-field output is an
                        // exact integer type; a float definition must not
                        // numerically widen into it. Plain Int keeps the
                        // legacy F64→Int widening (untouched).
                        if matches!(
                            admitter.node_of(output.ty),
                            Some(emath_ir::TypeNode::FieldPrime { .. })
                        ) && matches!(infer, Infer::F64)
                        {
                            admitter.error(
                                "E-TYPE-012",
                                format!(
                                    "definition `{name}` has type {infer}, expected an exact integer field element ({declared}); a float does not conform to a `Field` type"
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
    admit_event_payloads(&mut admitter, &by_name, &algebraic_fields);
    admit_transitions(&mut admitter, &by_name, &algebraic_fields);
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
    // Kind coaching (F4): a `emath model` with no dynamics payload is
    // almost certainly a stateless formula; suggest the right kind as a
    // note (never a refusal — the declaration still admits).
    if is_model
        && !by_name.contains_key("state")
        && !by_name.contains_key("equations")
        && !by_name.contains_key("equation")
        && !by_name.contains_key("algebraic")
        && !by_name.contains_key("constructors")
        && by_name.contains_key("definitions")
    {
        admitter.note(
            "N-KIND-001",
            "this `emath model` has only `definitions:` and no `state:`, `equations:`, or `algebraic:` — a stateless formula should be `emath function`",
            decl.head_source,
        );
    }
    if is_model && (by_name.contains_key("equations") || by_name.contains_key("equation")) {
        let residual_rates: BTreeSet<String> = admitter
            .residuals
            .iter()
            .flat_map(|residual| residual.rates.iter().cloned())
            .collect();
        for field in state {
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
        for field in algebraic_fields {
            match fields_infer.get(&field.name) {
                Some(Infer::F64) => unknown_dims.push((field.name.clone(), 1)),
                Some(Infer::Vector {
                    extent: Some(Extent::Fixed(n)),
                    ..
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
                    ..
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
        for field in algebraic_fields {
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
    for output in outputs_raw.iter() {
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
                Infer::Set(element) => TypeNode::Set(Box::new(match *element {
                    Infer::Bool => TypeNode::Bool,
                    Infer::Nat => TypeNode::Nat,
                    Infer::Int => TypeNode::Int,
                    Infer::Text => TypeNode::Other(QualifiedName("Text".into())),
                    _ => TypeNode::Float64,
                })),
                Infer::Record(name) => TypeNode::Record(QualifiedName(name)),
                Infer::Text => TypeNode::Other(QualifiedName("Text".into())),
                Infer::Vector { extent, .. } => TypeNode::Vector {
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
            for field in state {
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
            format!(
                "`constructors:` (stateful objects built by `public fn new`) are not admitted on `emath {kind_label}` — did you mean `emath policy`?"
            ),
            section.source,
        );
    }
    if !is_policy && !is_model && !state.is_empty() {
        admitter.error(
            "E-KIND-010",
            format!(
                "`emath {kind_label}` cannot carry `state:` — state belongs on `emath model` (continuous ODEs simulated over time) or `emath policy` (stateful object with constructors); did you mean one of those?"
            ),
            decl.head_source,
        );
    }

    (definitions, constructors)
}
