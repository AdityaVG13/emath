//! Capability-cell admission: bounded cell descriptors and biform sides.
//!
//! Restored from the pre-contraction recognition lane. Source-declared
//! `emath capability` cells intern into the package arena as data; math
//! meaning stays in the cell, never in a Rust feature branch.

use std::collections::BTreeSet;

use emath_core::Diagnostics;
use emath_core::tree::{CommandArgument, Declaration, ExprKind, Section, StmtKind};

use crate::admit::SemanticTrace;
use super::declaration::package_entry;
use super::text::type_text;

fn quoted_string_argument(argument: Option<&CommandArgument>) -> Option<&str> {
    match argument {
        Some(CommandArgument::Expr(expr)) => match &expr.kind {
            ExprKind::Str(text) => Some(text),
            _ => None,
        },
        _ => None,
    }
}

/// Admit a source-declared capability cell against the IR authority model.
pub(crate) fn admit_capability(
    decl: &Declaration,
    package: &mut emath_ir::SemanticPackage,
    diagnostics: &mut Diagnostics,
    trace: &mut SemanticTrace,
) {
    use emath_ir::capability::{
        BiformAuthority, BiformSide, CellClass, CellSchema, MigrationPolicy, SideEvidence,
        admit_cell, assess_biform_closure,
    };

    // Fail-closed from the first check: a capability whose admission
    // produced ANY error (bad metadata row, wrong section shape, closure
    // refusal) records nothing — the same rule as `admit_field_pack` and
    // the custom-kind marker path. A malformed cell never lands in the
    // package's capability arena.
    let errors_before = diagnostics.errors().count();

    // --- metadata rows -----------------------------------------------------
    let mut class_text: Option<String> = None;
    let mut version_text: Option<String> = None;
    let mut migration_text: Option<String> = None;
    let mut spec_section: Option<&Section> = None;
    let mut algorithm_section: Option<&Section> = None;
    let mut seen_sections: BTreeSet<&str> = BTreeSet::new();

    for stmt in &decl.body {
        match &stmt.kind {
            StmtKind::FieldDecl { name, ty, .. } if name == "class" => {
                class_text = Some(type_text(ty));
            }
            StmtKind::FieldDecl { name, ty, .. } if name == "migration" => {
                migration_text = Some(type_text(ty));
            }
            StmtKind::Command { head, argument }
                if head.first().map(String::as_str) == Some("version") =>
            {
                match quoted_string_argument(argument.as_ref()) {
                    Some(text) => version_text = Some(text.to_string()),
                    None => {
                        diagnostics.error(
                            "E-SYN-101",
                            "`version:` takes a quoted schema version string, e.g. `version: \"1.0.0\"`",
                            stmt.source,
                        );
                    }
                }
            }
            StmtKind::Command { head, argument }
                if head.first().map(String::as_str) == Some("migration") =>
            {
                match quoted_string_argument(argument.as_ref()) {
                    Some(text) => migration_text = Some(text.to_string()),
                    None => {
                        diagnostics.error(
                            "E-SYN-101",
                            "`migration:` takes `frozen` or a quoted `\"bump-and-note\"`",
                            stmt.source,
                        );
                    }
                }
            }
            StmtKind::Section(section) => {
                // Every biform-relevant section is single-occurrence: a
                // repeated `spec:`/`algorithm:`/`inputs` must refuse, never
                // silently replace the first (a dropped side section's
                // evidence would skip the closure check entirely).
                if !seen_sections.insert(section.name.as_str())
                    && matches!(
                        section.name.as_str(),
                        "inputs" | "outputs" | "spec" | "algorithm"
                    )
                {
                    diagnostics.error(
                        "E-KIND-003",
                        format!(
                            "kind `capability` requires exactly one `{}` section; `{}` declares more",
                            section.name, decl.name
                        ),
                        section.head_source,
                    );
                }
                match section.name.as_str() {
                    "spec" => spec_section = Some(section),
                    "algorithm" => algorithm_section = Some(section),
                    "inputs" | "outputs" => {}
                    other => {
                        diagnostics.error(
                            "E-SYN-101",
                            format!(
                                "section `{other}` is not admitted for capability cells \
                                 (the biform surface admits `inputs`, `outputs`, `spec`, `algorithm`)"
                            ),
                            section.head_source,
                        );
                    }
                }
            }
            _ => {
                diagnostics.error(
                    "E-SYN-101",
                    "statement at declaration level is not admitted for capability cells \
                     (class/version/migration rows, then inputs/outputs/spec/algorithm sections)",
                    stmt.source,
                );
            }
        }
    }

    // `class:` was present (this lane routes here only on class/sides).
    let class = match class_text.as_deref().map(CellClass::parse) {
        Some(Ok(class)) => class,
        Some(Err(refusal)) => {
            diagnostics.error(refusal.code(), refusal.to_string(), decl.head_source);
            return;
        }
        // A `spec:`/`algorithm:` trigger without a class row: the sides
        // belong to no authority class — refuse instead of defaulting.
        None => {
            diagnostics.error(
                "E-CELL-001",
                format!(
                    "capability `{}` declares biform side sections but no `class:` row \
                     (E-CELL-001); side sections require `class: biform`",
                    decl.name
                ),
                decl.head_source,
            );
            return;
        }
    };

    // Required sections mirror the kind schema: `inputs:` present and
    // exactly one `outputs:`.
    let inputs_present = decl.sections().any(|section| section.name == "inputs");
    if !inputs_present {
        diagnostics.error(
            "E-KIND-003",
            format!(
                "kind `capability` requires an `inputs` section; application `{}` has none",
                decl.name
            ),
            decl.head_source,
        );
    }
    let outputs_count = decl.sections().filter(|section| section.name == "outputs").count();
    if outputs_count != 1 {
        diagnostics.error(
            "E-KIND-003",
            format!(
                "kind `capability` requires exactly one `outputs` section; application `{}` declares {outputs_count}",
                decl.name
            ),
            decl.head_source,
        );
    }

    // --- bounded descriptor (IR authority seam) ---------------------------
    let Some(version) = version_text else {
        diagnostics.error(
            "E-CELL-002",
            format!(
                "capability cell `{}` declares no schema version (E-CELL-002); \
                 add `version: \"…\"` (bounded admission refuses a version-less cell)",
                decl.name
            ),
            decl.head_source,
        );
        return;
    };
    let migration = match migration_text.as_deref() {
        Some("frozen") => Some(MigrationPolicy::Frozen),
        Some("bump-and-note") => Some(MigrationPolicy::BumpAndNote { note: String::new() }),
        Some(other) => {
            diagnostics.error(
                "E-SYN-101",
                format!(
                    "unknown migration policy `{other}` on capability `{}` (frozen | \"bump-and-note\")",
                    decl.name
                ),
                decl.head_source,
            );
            None
        }
        None => {
            diagnostics.error(
                "E-SYN-101",
                format!(
                    "capability `{}` declares no `migration:` policy (frozen | \"bump-and-note\"); \
                     a cell never mutates silently",
                    decl.name
                ),
                decl.head_source,
            );
            None
        }
    };
    let Some(migration) = migration else { return };
    let arity = decl
        .sections()
        .find(|section| section.name == "inputs")
        .map(|section| {
            let count = section
                .suite
                .statements
                .iter()
                .filter(|stmt| matches!(&stmt.kind, StmtKind::FieldDecl { .. }))
                .count();
            // Saturate, never wrap: an absurd input count must fail the
            // arity bound, not wrap into a small passing arity.
            u16::try_from(count).unwrap_or(u16::MAX)
        })
        .unwrap_or(0);
    let canonical = match &package.package_path {
        Some(path) if !path.is_empty() => format!("{}.{}", path.join("."), decl.name),
        _ => decl.name.clone(),
    };
    let schema = CellSchema {
        name: emath_core::QualifiedName(canonical.clone()),
        class,
        version: version,
        migration,
        arity,
        about: Some(
            "capability cell declared in source; bounded admission records the descriptor".into(),
        ),
    };
    if let Err(refusal) = admit_cell(&schema) {
        diagnostics.error(refusal.code(), refusal.to_string(), decl.head_source);
        return;
    }

    // --- biform sides: independent evidence, typed closure ----------------
    let mut sides: Vec<SideEvidence> = Vec::new();
    if class == CellClass::Biform {
        for (side, section) in [
            (BiformSide::Spec, spec_section),
            (BiformSide::Algorithm, algorithm_section),
        ] {
            let Some(section) = section else { continue };
            let default_authority = match side {
                BiformSide::Spec => BiformAuthority::Authored,
                BiformSide::Algorithm => BiformAuthority::Verified,
            };
            let mut evidence_id: Option<String> = None;
            let mut authority: Option<BiformAuthority> = None;
            for stmt in &section.suite.statements {
                match &stmt.kind {
                    StmtKind::Command { head, argument }
                        if head.first().map(String::as_str) == Some("evidence") =>
                    {
                        match quoted_string_argument(argument.as_ref()) {
                            Some(text) => evidence_id = Some(text.to_string()),
                            None => {
                                diagnostics.error(
                                    "E-SYN-101",
                                    format!(
                                        "the `{}` side binds `evidence: \"…\"` (a string evidence-object token)",
                                        side.as_str()
                                    ),
                                    stmt.source,
                                );
                            }
                        }
                    }
                    StmtKind::FieldDecl { name, ty, .. } if name == "authority" => {
                        let word = type_text(ty);
                        authority = match word.as_str() {
                            "authored" => Some(BiformAuthority::Authored),
                            "verified" => Some(BiformAuthority::Verified),
                            "provider" => Some(BiformAuthority::Provider),
                            other => {
                                diagnostics.error(
                                    "E-SYN-101",
                                    format!(
                                        "unknown authority `{other}` on the {} side \
                                         (authored | verified | provider)",
                                        side.as_str()
                                    ),
                                    stmt.source,
                                );
                                None
                            }
                        };
                    }
                    _ => {
                        diagnostics.error(
                            "E-SYN-101",
                            format!(
                                "the `{}` side admits `evidence: \"…\"` and `authority: <word>` entries",
                                side.as_str()
                            ),
                            stmt.source,
                        );
                    }
                }
            }
            if let Some(evidence_id) = evidence_id {
                sides.push(SideEvidence {
                    side,
                    evidence_id,
                    authority: authority.unwrap_or(default_authority),
                });
            }
            // A side with no evidence object: the closure reports the
            // typed missing-side refusal below.
        }
        for refusal in assess_biform_closure(&schema, &sides) {
            diagnostics.error(refusal.code(), refusal.to_string(), decl.head_source);
        }
    } else if spec_section.is_some() || algorithm_section.is_some() {
        // Side sections are biform-only: a non-biform cell declaring
        // them would smuggle two authorities into one class. The rows
        // were accepted as sections above; refuse now, typed.
        diagnostics.error(
            "E-SYN-101",
            format!(
                "capability `{}` declares `spec:`/`algorithm:` side sections but class `{}` \
                 is not `biform`; side sections carry independent evidence and only a biform \
                 cell may claim them",
                decl.name,
                class.as_str()
            ),
            decl.head_source,
        );
    }
    if diagnostics.errors().count() > errors_before {
        return;
    }

    // --- record ------------------------------------------------------------
    let mut declaration = package_entry(decl, "capability");
    declaration.about = Some(format!(
        "capability cell `{canonical}` ({}); biform sides carry independent evidence objects",
        class.as_str()
    ));
    for evidence in &sides {
        declaration.evidence.push(emath_ir::EvidenceClaim {
            id: evidence.evidence_id.clone(),
            statement: format!(
                "biform side `{}` of `{}` attested by `{}` authority as evidence object `{}`",
                evidence.side.as_str(),
                canonical,
                evidence.authority.as_str(),
                evidence.evidence_id
            ),
            class: "biform-side-evidence".into(),
            scope: "declaration".into(),
            assumptions: Vec::new(),
            producer: "source".into(),
            checker: None,
            verdict: emath_ir::ClaimVerdict::NotRun,
            level: emath_ir::EvidenceLevel::E1,
            falsifiers: vec![
                "the side's evidence object shared across sides or authority escalated".into(),
            ],
            artifacts: Vec::new(),
            fresh_until: None,
        });
    }
    declaration.id =
        emath_ir::DeclarationId(u32::try_from(package.declarations.len()).unwrap_or(u32::MAX));
    package.declarations.push(declaration);
    package.capabilities.push(emath_ir::capability::Capability {
        name: emath_core::QualifiedName(canonical.clone()),
        class,
    });
    trace.record(
        "recognize:capability",
        format!(
            "capability `{canonical}` admitted as `{}` with {} biform side(s) from `{}`",
            class.as_str(),
            sides.len(),
            decl.name
        ),
        Some(decl.head_source),
    );
}
