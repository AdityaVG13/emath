use emath_core::tree::ExprKind;

use super::*;

/// Validate the declaration shell and index its sections by name
/// (moved verbatim from `admit_declaration`).
pub(super) fn admit_declaration_setup<'a>(
    decl: &'a emath_core::tree::Declaration,
    host_types: &BTreeSet<String>,
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
    let kind_label = decl.as_kind.clone();
    if matches!(
        kind_label.as_str(),
        "model"
            | "policy"
            | "kind"
            | "law"
            | "feature"
            | "capability"
            | "field_pack"
            | "reaction_network"
            | "search"
            | "experiment"
            | "widget"
    ) {
        admitter.error(
            "E-KIND-GONE",
            format!(
                "declaration kind `{kind_label}` is not a core kind; write `emath object`, `emath function`, or `emath query`"
            ),
            decl.source,
        );
    }
    let is_policy = false;
    let is_model = false;
    let is_law = false;
    let schema = if kind_label == "object" {
        KindSchema::core_object()
    } else if kind_label == "query" {
        KindSchema::core_query()
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
    // R6 (E-SEC-130): `outputs:` without `inputs:` leaves the I/O surface
    // unnamed — refuse. `goals:` is not a constructor section (E-SEC-101).
    if by_name.contains_key("inputs")
        || by_name.contains_key("outputs")
        || by_name.contains_key("definitions")
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
        if by_name.contains_key("outputs") && !has_inputs && !declares_hole {
            admitter.error(
                "E-SEC-130",
                "declaration has `outputs:` but no `inputs:` \
                 section — add `inputs:` to name the I/O surface",
                decl.head_source,
            );
        }
    }
    refuse_implicit_and_unit_fields(&mut admitter, &by_name);

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
    // never a silent drop. `request:` / `requests:` / `goals:` are not
    // constructor sections.
    for section in decl.sections() {
        if matches!(section.name.as_str(), "request" | "requests") {
            admitter.error(
                "E-SEC-101",
                format!(
                    "section `{}:` is not a constructor section",
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
                    "section `{}` is not a constructor section; write `emath object`, `emath function`, or `emath query`",
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
                    "section `{}` is not a constructor section (known: {})",
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

fn refuse_implicit_and_unit_fields(
    admitter: &mut Admitter,
    by_name: &BTreeMap<&str, &Section>,
) {
    for section_name in ["inputs", "outputs", "parameters"] {
        let Some(section) = by_name.get(section_name) else {
            continue;
        };
        for stmt in &section.suite.statements {
            let StmtKind::FieldDecl { name, ty, .. } = &stmt.kind else {
                continue;
            };
            if matches!(
                &ty.kind,
                emath_core::tree::TypeKind::Path { segments, .. }
                    if segments.len() == 1 && segments[0] == "Infer"
            ) {
                admitter.error(
                    "E-TYPE-001",
                    format!("`{name}` needs an explicit constructor type, not an inferred default"),
                    stmt.source,
                );
            }
            if matches!(
                &ty.kind,
                emath_core::tree::TypeKind::In { .. }
                    | emath_core::tree::TypeKind::Product { .. }
                    | emath_core::tree::TypeKind::Pow { .. }
                    | emath_core::tree::TypeKind::Domain { .. }
            ) {
                admitter.error(
                    "E-KIND-GONE",
                    format!(
                        "`{name}` uses a unit or domain annotation; write a constructor carrier type and an ordinary module if you need units"
                    ),
                    stmt.source,
                );
            }
        }
    }
}
