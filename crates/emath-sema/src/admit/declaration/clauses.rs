use super::*;

/// Admit the events/observations/constraints/invariant/proofs/figures
/// clause sections (moved verbatim from `admit_declaration`).
pub(super) fn admit_declaration_clauses(
    admitter: &mut Admitter,
    by_name: &BTreeMap<&str, &Section>,
) -> BTreeSet<String> {
    if let Some(section) = by_name.get("events") {
        let mut seen_events: BTreeSet<String> = BTreeSet::new();
        for stmt in &section.suite.statements {
            match &stmt.kind {
                StmtKind::FnDecl { head, name, .. } if head == "event" => {
                    if !seen_events.insert(name.clone()) {
                        admitter.error(
                            "E-NAME-022",
                            format!("duplicate event name `{name}`"),
                            stmt.source,
                        );
                    }
                }
                StmtKind::Command { head, .. }
                    if head.first().map(String::as_str) == Some("event") && head.len() == 2 =>
                {
                    let name = head[1].clone();
                    if !seen_events.insert(name.clone()) {
                        admitter.error(
                            "E-NAME-022",
                            format!("duplicate event name `{name}`"),
                            stmt.source,
                        );
                    }
                }
                _ => {
                    admitter.error(
                        "E-SYN-101",
                        "only `event <Name>(field: Type)` declarations are allowed in `events:`",
                        stmt.source,
                    );
                }
            }
        }
    }

    // Measured evidence (04 §5.2): each row
    // is `obs <name>[: type] = <data>` (the parser stores it losslessly
    // as a FieldDecl with the value as its default). An observation is a
    // datum an instrument reported, not a model binding: it is
    // type-checked once here and named for provenance, but NEVER entered
    // into the definition environment — the E-OBS-WRITE refusal below
    // keeps model output from silently overwriting data. Reading
    // observations in comparisons (§5.3) is the named next slice.
    let mut observation_names: BTreeSet<String> = BTreeSet::new();
    if let Some(section) = by_name.get("observations") {
        for stmt in &section.suite.statements {
            let StmtKind::FieldDecl {
                name, ty, default, ..
            } = &stmt.kind
            else {
                admitter.error(
                    "E-SYN-101",
                    "observations rows are `obs <name>[: type] = <data>`",
                    stmt.source,
                );
                continue;
            };
            if !observation_names.insert(name.clone()) {
                admitter.error(
                    "E-NAME-022",
                    format!("duplicate observation name `{name}`"),
                    stmt.source,
                );
                continue;
            }
            let Some(value) = default else {
                admitter.error(
                    "E-SYN-101",
                    format!("observation `{name}` needs a value (`obs {name}[: type] = data`)"),
                    stmt.source,
                );
                continue;
            };
            let value_infer = admitter.lower_expr(value).map(|(_, infer)| infer);
            let declared = if let emath_core::tree::TypeKind::Path {
                segments,
                generic_args,
            } = &ty.kind
                && generic_args.is_empty()
                && segments.last().map(String::as_str) == Some("Infer")
            {
                None
            } else {
                map_type(ty, &mut admitter.diagnostics, &admitter.host_types)
                    .map(|node| infer_from_node(&node))
            };
            if let (Some(value_infer), Some(declared)) = (&value_infer, &declared)
                && !infer_conforms(value_infer, declared)
            {
                admitter.error(
                    "E-TYPE-012",
                    format!("observation `{name}` has type {value_infer}, expected {declared}"),
                    value.source,
                );
            }
        }
    }

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

    // Proof outlines (B13 + 05 §7.2): obligation
    // kinds as DATA. Proofs are additive authority, never admission
    // tickets — nothing here gates artifact production, and outline
    // claims are never lowered as definitions or constraints
    // (justification stays structurally separate from meaning).
    // Completeness is checked (an outline ends with its qed); NO
    // ProofChecker runs in this slice — `check` steps are data
    // obligations, and machine-record lowering is the named follow-up.
    if let Some(section) = by_name.get("proofs") {
        for stmt in &section.suite.statements {
            let StmtKind::Section(outline) = &stmt.kind else {
                admitter.error(
                    "E-SYN-101",
                    "only `outline <Name>:` proof outlines are allowed in `proofs:`",
                    stmt.source,
                );
                continue;
            };
            let outline_name = outline.generic.as_deref().unwrap_or("_");
            let steps = &outline.suite.statements;
            if steps.is_empty() {
                admitter.error(
                    "E-SYN-101",
                    format!("proof outline `{outline_name}` is empty"),
                    stmt.source,
                );
                continue;
            }
            let mut declared: Vec<String> = Vec::new();
            let mut ends_with_qed = false;
            for (i, step) in steps.iter().enumerate() {
                ends_with_qed = false;
                match &step.kind {
                    StmtKind::Section(s)
                        if matches!(s.name.as_str(), "assumption" | "lemma") =>
                    {
                        let target = s.generic.clone().unwrap_or_else(|| "_".into());
                        if s.name == "lemma" && s.suite.statements.is_empty() {
                            admitter.error(
                                "E-SYN-101",
                                format!(
                                    "lemma `{target}` in outline `{outline_name}` needs a claim after `:`"
                                ),
                                step.source,
                            );
                        }
                        declared.push(target);
                    }
                    StmtKind::Command { head, .. }
                        if head.first().map(String::as_str) == Some("check") =>
                    {
                        match head.get(1) {
                            Some(target) if declared.iter().any(|d| d == target) => {}
                            Some(target) => admitter.error(
                                "E-SYN-101",
                                format!(
                                    "`check {target}` in outline `{outline_name}` names an obligation not declared earlier in the outline"
                                ),
                                step.source,
                            ),
                            None => admitter.error(
                                "E-SYN-101",
                                format!(
                                    "`check` in outline `{outline_name}` must name an obligation: `check <name>`"
                                ),
                                step.source,
                            ),
                        }
                    }
                    StmtKind::Command { head, .. }
                        if head.first().map(String::as_str) == Some("qed") =>
                    {
                        ends_with_qed = i + 1 == steps.len();
                        match head.get(1) {
                            Some(target) if declared.iter().any(|d| d == target) => {}
                            Some(target) => admitter.error(
                                "E-SYN-101",
                                format!(
                                    "`qed {target}` in outline `{outline_name}` names an obligation not declared earlier in the outline"
                                ),
                                step.source,
                            ),
                            None => admitter.error(
                                "E-SYN-101",
                                format!(
                                    "`qed` in outline `{outline_name}` must name the concluding obligation: `qed <name>`"
                                ),
                                step.source,
                            ),
                        }
                        if i + 1 != steps.len() {
                            admitter.error(
                                "E-SYN-101",
                                format!(
                                    "`qed` must be the final step of outline `{outline_name}`"
                                ),
                                step.source,
                            );
                        }
                    }
                    _ => {
                        admitter.error(
                            "E-SYN-101",
                            format!(
                                "unknown proof step in outline `{outline_name}`; obligation kinds are data: `assumption <name>: <claim>`, `lemma <name>: <claim>`, `check <name>`, `qed <name>`"
                            ),
                            step.source,
                        );
                    }
                }
            }
            if !ends_with_qed {
                admitter.error(
                    "E-SYN-101",
                    format!(
                        "proof outline `{outline_name}` is incomplete: it must end with `qed <obligation>` (an outline without its concluding qed is not an obligation record); ProofChecker integration is the named follow-up"
                    ),
                    stmt.source,
                );
            }
            // Obligation data lands in the semantic trace; the
            // `emath.proof-obligation v1` machine-record lowering and the
            // ProofChecker contract are the named follow-ups.
            admitter.record(
                "proofs",
                format!(
                    "outline `{outline_name}`: {} obligation step(s) recorded as data (assumption/lemma/check/qed); no ProofChecker runs in this slice",
                    steps.len()
                ),
                stmt.source,
            );
        }
    }

    // Declarative figures (05 §7.4): the
    // section NAME + payload grammar slot is reserved — `figures:` is
    // out of the generic E-SEC-101 roster error and every payload row
    // refuses naming the design forks. The payload grammar is the
    // named follow-up; nothing draws in this seed.
    if let Some(section) = by_name.get("figures") {
        for stmt in &section.suite.statements {
            admitter.error(
                "E-SYN-101",
                "`figures:` payload rows are outside the Phase 1 subset — the declarative-figures design follow-up must first settle: sampling is tied to the budgets/continuation machinery from day one (unbounded adaptive sampling would be the first nondeterminism smuggled in through the front door), every figure artifact carries its sampling receipt (visual continuity is labeled OBSERVATIONAL, never proved smoothness), and the renderer is a provider contract (Renderer) — upstream never defines render semantics; the same spec must render in WASM, PNG, or paper",
                stmt.source,
            );
        }
    }

    observation_names
}
