use super::*;

/// Collect typed fields per section and derive inputs/outputs/state
/// (moved verbatim from `admit_declaration`).
pub(super) fn admit_declaration_fields(
    mut admitter: &mut Admitter,
    by_name: &BTreeMap<&str, &Section>,
    decl: &emath_core::tree::Declaration,
    is_model: bool,
    is_law: bool,
    kind_label: &String,
) -> (
    BTreeMap<String, Infer>,
    Vec<Field>,
    Vec<Field>,
    Vec<Field>,
    Vec<Field>,
    bool,
) {
    let mut fields_infer: BTreeMap<String, Infer> = BTreeMap::new();
    let mut fields_by_section: BTreeMap<&str, Vec<Field>> = BTreeMap::new();
    let mut outputs_from_head = false;
    if let Some(signature) = &decl.signature {
        let stateful = by_name.contains_key("state") || by_name.contains_key("constructors");
        let refuse_head = !matches!(kind_label.as_str(), "function" | "law") || stateful;
        if refuse_head {
            admitter.error(
                "E-SYN-123",
                "declaration head arguments are only admitted on stateless `emath function` or `emath law` declarations (no `state:` or `constructors:`)",
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
                format!(
                    "`algebraic:` (implicit unknowns solved alongside the ODE states) is only admitted on `emath model` declarations; you used `emath {kind_label}` — did you mean `emath model`?"
                ),
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

    // A law may pair prose assumptions with machine-checkable `require`
    // expressions over its inputs. They reuse invariant IR so execution can
    // refuse before evaluating a partial formula.
    if is_law && let Some(section) = by_name.get("assumptions") {
        for stmt in &section.suite.statements {
            if let StmtKind::Require(expr) = &stmt.kind
                && let Some(id) = admitter.lower_requirement(expr)
            {
                admitter.constraints.push(id);
            }
        }
    }

    // Hybrid events (ch7): `events:` declares the
    // discrete event surface — `event Name(field: Type)` declarations
    // (FnDecl head `event`) or no-arg `event Name` commands. Events are
    // named surface: the same event name twice refuses through the
    // duplicate lane (E-NAME-022), and anything that is not an event
    // declaration refuses typed. Payload suites (`if <condition>:`
    // actions) are admitted later by `admit_event_payloads`, after
    // definitions and equations lower, so their expressions may name
    // declared definitions.
    (
        fields_infer,
        inputs,
        outputs_raw,
        state,
        algebraic_fields,
        outputs_omitted,
    )
}
