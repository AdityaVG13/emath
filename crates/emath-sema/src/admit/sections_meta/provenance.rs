//! About/evidence/law-metadata/binding-provenance section admission.

use super::*;

pub(in crate::admit) fn admit_about(
    admitter: &mut Admitter,
    section: Option<&Section>,
) -> Option<String> {
    let section = section?;
    let mut summary = None;
    for stmt in &section.suite.statements {
        match &stmt.kind {
            StmtKind::Command { head, argument }
                if head.first().map(String::as_str) == Some("summary") =>
            {
                if let Some(CommandArgument::Expr(expr)) = argument {
                    if let ExprKind::Str(text) = &expr.kind {
                        summary = Some(text.clone());
                        admitter.record("sema", "about summary retained", expr.source);
                        continue;
                    }
                }
                admitter.error(
                    "E-SYN-101",
                    "`about.summary` must be a string literal",
                    stmt.source,
                );
            }
            _ => {
                admitter.error(
                    "E-SYN-101",
                    "`about:` admits `summary: \"...\"` in Phase 1",
                    stmt.source,
                );
            }
        }
    }
    summary
}

pub(in crate::admit) fn admit_evidence(
    admitter: &mut Admitter,
    section: Option<&Section>,
) -> Vec<EvidenceClaim> {
    let Some(section) = section else {
        return Vec::new();
    };
    let mut claims = Vec::new();
    for stmt in &section.suite.statements {
        let StmtKind::Section(claim) = &stmt.kind else {
            admitter.error(
                "E-SYN-101",
                "expected `claim <name>:` blocks inside `evidence:`",
                stmt.source,
            );
            continue;
        };
        if claim.name != "claim" {
            admitter.error(
                "E-SYN-101",
                format!("unknown evidence block `{}`", claim.name),
                claim.head_source,
            );
            continue;
        }
        let id = claim.generic.clone().unwrap_or_default();
        if id.is_empty() {
            admitter.error(
                "E-SYN-101",
                "`claim` requires a name in `<...>`",
                claim.head_source,
            );
            continue;
        }
        let mut statement = String::new();
        let mut class = String::new();
        let mut level = EvidenceLevel::E1;
        let mut has_level = false;
        for inner in &claim.suite.statements {
            match &inner.kind {
                StmtKind::Command { head, argument }
                    if head.first().map(String::as_str) == Some("statement") =>
                {
                    statement = match argument {
                        Some(CommandArgument::Expr(expr)) => expr_text(expr),
                        _ if head.len() > 1 => head[1..].join(" "),
                        _ => String::new(),
                    };
                }
                StmtKind::Require(expr) => {
                    class = expr_text(expr);
                }
                StmtKind::Command { head, .. }
                    if head.first().map(String::as_str) == Some("require") =>
                {
                    class = head.get(1).cloned().unwrap_or_default();
                }
                StmtKind::Command { head, argument }
                    if head.first().map(String::as_str) == Some("level") =>
                {
                    if has_level {
                        admitter.error(
                            "E-SYN-103",
                            "evidence claim declares `level` more than once",
                            inner.source,
                        );
                        continue;
                    }
                    has_level = true;
                    let value = head.get(1).cloned().or_else(|| match argument {
                        Some(CommandArgument::Expr(expr)) => Some(expr_text(expr)),
                        _ => None,
                    });
                    match value.as_deref().and_then(|value| value.parse().ok()) {
                        Some(parsed) => level = parsed,
                        None => admitter.error(
                            "E-EVID-115",
                            "unknown evidence level; expected E0, E1, E2, E3, E4, or E5",
                            inner.source,
                        ),
                    }
                }
                _ => {
                    admitter.error(
                        "E-SYN-101",
                        "evidence claims admit `statement ...`, `require ...`, and `level E0` through `level E5`",
                        inner.source,
                    );
                }
            }
        }
        admitter.record(
            "sema",
            format!("evidence claim `{id}` recorded (verdict not-run)"),
            claim.head_source,
        );
        claims.push(EvidenceClaim {
            id,
            statement,
            class,
            scope: "declaration".into(),
            assumptions: Vec::new(),
            producer: "source".into(),
            checker: None,
            verdict: ClaimVerdict::NotRun,
            level,
            falsifiers: Vec::new(),
            artifacts: Vec::new(),
            fresh_until: None,
        });
    }
    claims
}

pub(super) fn law_entries(
    admitter: &mut Admitter,
    section: Option<&Section>,
    section_name: &str,
    command: &str,
    missing_span: emath_core::Span,
) -> Vec<String> {
    let Some(section) = section else {
        admitter.error(
            "E-LAW-002",
            format!("`emath law` requires a `{section_name}:` section"),
            missing_span,
        );
        return Vec::new();
    };
    let mut entries = Vec::new();
    for stmt in &section.suite.statements {
        match &stmt.kind {
            StmtKind::Command {
                head,
                argument: Some(CommandArgument::Expr(expr)),
            } if head.first().map(String::as_str) == Some(command)
                && matches!(&expr.kind, ExprKind::Str(_)) =>
            {
                let ExprKind::Str(value) = &expr.kind else {
                    unreachable!()
                };
                entries.push(value.clone());
            }
            StmtKind::Require(_) if section_name == "assumptions" => {}
            _ => admitter.error(
                "E-SYN-101",
                format!("`{section_name}:` admits `{command} \"...\"` entries"),
                stmt.source,
            ),
        }
    }
    if entries.is_empty() {
        admitter.error(
            "E-LAW-002",
            format!("`emath law` requires at least one `{command} \"...\"` entry"),
            section.source,
        );
    }
    entries
}

pub(in crate::admit) fn admit_law_metadata(
    admitter: &mut Admitter,
    assumptions: Option<&Section>,
    domain: Option<&Section>,
    provenance: Option<&Section>,
    citations: Option<&Section>,
    declaration_span: emath_core::Span,
) -> LawMetadata {
    let assumptions = law_entries(
        admitter,
        assumptions,
        "assumptions",
        "assume",
        declaration_span,
    );
    let domains = law_entries(admitter, domain, "domain", "name", declaration_span);
    if domains.len() > 1 {
        admitter.error(
            "E-LAW-002",
            "`domain:` requires exactly one `name \"...\"` entry",
            domain.map_or(emath_core::Span::default(), |section| section.source),
        );
    }
    LawMetadata {
        assumptions,
        domain: domains.into_iter().next().unwrap_or_default(),
        provenance: law_entries(
            admitter,
            provenance,
            "provenance",
            "source",
            declaration_span,
        ),
        citations: law_entries(admitter, citations, "citations", "cite", declaration_span),
    }
}

pub(super) fn required_provenance_value(
    admitter: &mut Admitter,
    values: &BTreeMap<String, String>,
    key: &str,
    binding: &str,
    span: emath_core::Span,
) -> Option<String> {
    values
        .get(key)
        .filter(|value| !value.is_empty())
        .cloned()
        .or_else(|| {
            admitter.error(
                "E-SYN-152",
                format!("provenance for `{binding}` requires a non-empty `{key}: \"...\"`"),
                span,
            );
            None
        })
}

/// Admit declaration-local provenance keyed by binding name.
///
/// Shape:
/// `provenance: / <binding>: / kind: "Citation" / reference: "doi:..."`.
pub(in crate::admit) fn admit_binding_provenance(
    admitter: &mut Admitter,
    section: Option<&Section>,
    known_bindings: &BTreeSet<String>,
) -> BTreeMap<String, Provenance> {
    let Some(section) = section else {
        return BTreeMap::new();
    };
    let mut admitted = BTreeMap::new();
    let mut seen_bindings = BTreeSet::new();
    for statement in &section.suite.statements {
        let StmtKind::Section(binding_section) = &statement.kind else {
            admitter.error(
                "E-SYN-152",
                "`provenance:` entries must be binding-named sections",
                statement.source,
            );
            continue;
        };
        let binding = binding_section.name.as_str();
        if !known_bindings.contains(binding) {
            admitter.error(
                "E-NAME-028",
                format!("provenance names unknown binding `{binding}`"),
                binding_section.head_source,
            );
            continue;
        }
        if !seen_bindings.insert(binding.to_string()) {
            admitter.error(
                "E-SYN-103",
                format!("duplicate provenance for binding `{binding}`"),
                binding_section.head_source,
            );
            continue;
        }

        let mut values = BTreeMap::new();
        for entry in &binding_section.suite.statements {
            let StmtKind::Command {
                head,
                argument: Some(CommandArgument::Expr(expr)),
            } = &entry.kind
            else {
                admitter.error(
                    "E-SYN-152",
                    "provenance fields use `key: \"value\"`",
                    entry.source,
                );
                continue;
            };
            let Some(key) = head.first() else {
                admitter.error("E-SYN-152", "empty provenance key", entry.source);
                continue;
            };
            if !matches!(
                key.as_str(),
                "kind"
                    | "source"
                    | "reference"
                    | "adjustment"
                    | "file"
                    | "processing"
                    | "fit_id"
                    | "reason"
                    // 04 §5.2: declared digest
                    // of the raw data file; re-hashed by --verify-data.
                    | "sha256"
            ) {
                admitter.error(
                    "E-SYN-152",
                    format!("unknown provenance key `{key}`"),
                    entry.source,
                );
                continue;
            }
            let ExprKind::Str(value) = &expr.kind else {
                admitter.error(
                    "E-SYN-152",
                    format!("provenance key `{key}` requires a string value"),
                    expr.source,
                );
                continue;
            };
            if values.insert(key.clone(), value.clone()).is_some() {
                admitter.error(
                    "E-SYN-103",
                    format!("duplicate provenance key `{key}` for `{binding}`"),
                    entry.source,
                );
            }
        }

        let Some(kind) =
            required_provenance_value(admitter, &values, "kind", binding, binding_section.source)
        else {
            continue;
        };
        let (provenance, allowed): (Option<Provenance>, &[&str]) = match kind
            .to_ascii_lowercase()
            .as_str()
        {
            "exact" => (
                required_provenance_value(
                    admitter,
                    &values,
                    "source",
                    binding,
                    binding_section.source,
                )
                .map(|source| Provenance::Exact { source }),
                &["kind", "source"],
            ),
            "citation" => (
                required_provenance_value(
                    admitter,
                    &values,
                    "reference",
                    binding,
                    binding_section.source,
                )
                .map(|reference| Provenance::Citation {
                    reference,
                    adjustment: values.get("adjustment").cloned(),
                }),
                &["kind", "reference", "adjustment"],
            ),
            "instrumentrun" | "instrument_run" => (
                required_provenance_value(
                    admitter,
                    &values,
                    "file",
                    binding,
                    binding_section.source,
                )
                .zip(required_provenance_value(
                    admitter,
                    &values,
                    "processing",
                    binding,
                    binding_section.source,
                ))
                .map(|(file, processing)| Provenance::InstrumentRun {
                    file,
                    processing,
                    sha256: values.get("sha256").cloned(),
                }),
                &["kind", "file", "processing", "sha256"],
            ),
            "fitted" => (
                required_provenance_value(
                    admitter,
                    &values,
                    "fit_id",
                    binding,
                    binding_section.source,
                )
                .map(|fit_id| Provenance::Fitted { fit_id }),
                &["kind", "fit_id"],
            ),
            "assumed" => (
                Some(Provenance::Assumed {
                    reason: values.get("reason").cloned(),
                }),
                &["kind", "reason"],
            ),
            "unstated" => (Some(Provenance::Unstated), &["kind"]),
            _ => {
                admitter.error(
                    "E-SYN-152",
                    format!(
                        "unknown provenance kind `{kind}`; expected Exact, Citation, InstrumentRun, Fitted, Assumed, or Unstated"
                    ),
                    binding_section.source,
                );
                (None, &["kind"])
            }
        };
        for key in values.keys() {
            if !allowed.contains(&key.as_str()) {
                admitter.error(
                    "E-SYN-152",
                    format!("provenance kind `{kind}` does not admit key `{key}`"),
                    binding_section.source,
                );
            }
        }
        if let Some(provenance) = provenance {
            admitted.insert(binding.to_string(), provenance);
        }
    }
    admitted
}
