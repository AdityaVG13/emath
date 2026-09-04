use emath_core::tree::ExprKind;

use super::*;

/// Validate the declaration shell and index its sections by name
/// (moved verbatim from `admit_declaration`).
pub(super) fn admit_declaration_setup<'a>(
    decl: &'a emath_core::tree::Declaration,
    host_types: &BTreeSet<String>,
    capability_cells: &[CapabilityCallBinding],
    sibling_functions: &BTreeMap<String, SiblingFunction>,
) -> (
    Admitter,
    String,
    bool,
    bool,
    bool,
    KindSchema,
    BTreeMap<&'a str, &'a Section>,
) {
    let mut admitter = Admitter::new();
    admitter.host_types = host_types.clone();
    admitter.capability_cells = capability_cells.to_vec();
    admitter.sibling_functions = sibling_functions.clone();
    let kind_label = decl.as_kind.clone();
    let is_policy = kind_label == "policy";
    let is_model = kind_label == "model";
    let is_law = kind_label == "law";
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

    // L3 section rules.
    //
    // R5 (E-NAME-020): a name bound in BOTH `inputs:` and `outputs:` forks
    // the contract's identity for that slot. The generic duplicate-field
    // check already rejects it ("duplicate field ... declared in section
    // ..."), so no local rule is needed here; the LOCAL rule below covers
    // the case nothing else catches: a `definitions:` name shadowing an
    // `inputs:` name.
    // R6 (E-SEC-130): contract mode with `outputs:`/`goals:` but NO `inputs:`
    // section leaves the I/O surface unnamed — refuse.
    // R4 (E-SEC-133): contract mode without `goals:` is legal (every
    // definition defaults to evaluate) but the default is made visible.
    // Evidence (E-EV-140): only ASSERTION verbs (`prove`) claim truth
    // without computing it; Phase 1 goal verbs are operational and never
    // demand `evidence:`. The rule keys on the CLAIM_VERBS list below.
    if by_name.contains_key("inputs")
        || by_name.contains_key("outputs")
        || by_name.contains_key("definitions")
        || by_name.contains_key("goals")
        || by_name.contains_key("evidence")
    {
        let input_names: BTreeSet<String> = by_name
            .get("inputs")
            .map(|section| {
                section
                    .suite
                    .statements
                    .iter()
                    .filter_map(|stmt| match &stmt.kind {
                        StmtKind::FieldDecl { name, .. } => Some(name.clone()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        // R5 (continued): a `definitions:` name that shadows an `inputs:`
        // name silently overwrites the declared input inside the component
        // body — same identity fork as inputs/outputs, same refusal.
        if let Some(definitions) = by_name.get("definitions") {
            for stmt in &definitions.suite.statements {
                if let StmtKind::Assign { target, .. } = &stmt.kind
                    && target.segments.len() == 1
                    && input_names.contains(&target.segments[0])
                {
                    let name = &target.segments[0];
                    admitter.error(
                        "E-NAME-020",
                        format!(
                            "definition `{name}` shadows the `inputs:` name `{name}` — \
                             a definition cannot overwrite a declared input"
                        ),
                        stmt.source,
                    );
                }
            }
        }
        let has_inputs = by_name.contains_key("inputs");
        // A declared hole (`name = ?`) IS the named unknown — the contract
        // may leave the I/O surface implicit when the hole names it.
        let declares_hole = decl
            .body
            .iter()
            .chain(
                decl.sections()
                    .flat_map(|section| section.suite.statements.iter()),
            )
            .any(|stmt| {
                matches!(
                    &stmt.kind,
                    StmtKind::Assign { value, .. }
                        if matches!(
                            &value.kind,
                            ExprKind::Path { segments, .. }
                                if segments.len() == 1 && segments[0] == "Hole"
                        )
                )
            });
        let has_outputs_or_goals = by_name.contains_key("outputs") || by_name.contains_key("goals");
        if has_outputs_or_goals && !has_inputs && !declares_hole {
            admitter.error(
                "E-SEC-130",
                "contract-mode declaration has `outputs:`/`goals:` but no `inputs:` \
                 section — add `inputs:` to name the I/O surface",
                decl.head_source,
            );
        }
        let goals_nonempty = by_name
            .get("goals")
            .is_some_and(|section| !section.suite.statements.is_empty());
        if !by_name.contains_key("goals") || !goals_nonempty {
            admitter.warning(
                "E-SEC-133",
                "no `goals:` section — every definition defaults to `evaluate`; \
                 declare `goals:` to pin intent",
                decl.head_source,
            );
        }
        // Evidence (E-EV-140): an ASSERTION verb states truth without
        // computing it; Phase 1 goal verbs (evaluate, differentiate,
        // benchmark, fit, simplify) are operational — they compute, they
        // do not claim, so they never demand `evidence:` (demanding it
        // broke the fit goals). `prove` is the first claim verb; when the
        // goals grammar accepts it, listing it in CLAIM_VERBS activates
        // the rule.
        const CLAIM_VERBS: &[&str] = &[];
        if let Some(goals) = by_name.get("goals") {
            let claim_bearing = goals
                .suite
                .statements
                .iter()
                .filter_map(|stmt| match &stmt.kind {
                    StmtKind::Section(nested) => Some(nested.name.as_str()),
                    _ => None,
                })
                .any(|verb| CLAIM_VERBS.contains(&verb));
            let evidence_present = by_name
                .get("evidence")
                .is_some_and(|section| !section.suite.statements.is_empty());
            if claim_bearing && !evidence_present {
                admitter.error(
                    "E-EV-140",
                    "claim-bearing goal verb requires an `evidence:` section with \
                     at least one row (a claim without evidence is a silent assertion)",
                    goals.head_source,
                );
            }
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
        if matches!(
            section.name.as_str(),
            "assumptions" | "domain" | "citations"
        ) && !is_law
        {
            admitter.error(
                "E-SEC-101",
                format!(
                    "section `{}` is admitted only on `emath law` declarations",
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
    (
        admitter, kind_label, is_policy, is_model, is_law, schema, by_name,
    )
}
