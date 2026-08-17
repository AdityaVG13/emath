//! Core: migration from bootstrap syntax.
//!
//! The bootstrap parser stays behind a compatibility tool, not the
//! production compiler. `migrate_declaration` rewrites a bootstrap-era
//! declaration into the open framework carrying its bootstrap schema:
//! `request` sections become `goals`, `input`/`output` singletons
//! become plural `inputs`/`outputs`, and inline constructors are lifted
//! into a `constructors:` section.

use emath_core::tree::Declaration;

/// One migration concern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationIssue {
    pub code: &'static str,
    pub detail: String,
}

/// A migrated declaration plus any concerns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Migrated {
    pub name: String,
    pub as_kind: String,
    pub schema: String,
    pub sections: Vec<String>,
    pub issues: Vec<MigrationIssue>,
}

/// Migrates a bootstrap declaration into the open framework.
/// Bootstrap artifacts remain readable under their schema but are
/// regenerated for the stable language (acceptance gate: V3/Phase1
/// examples migrate and preserve behavior).
#[must_use]
pub fn migrate_declaration(decl: &Declaration, bump: &str) -> Migrated {
    let mut issues = Vec::new();
    if bump == "legacy" {
        issues.push(MigrationIssue {
            code: "E-MIGR-001",
            detail: "legacy bootstrap schema written; regenerate with the stable edition".into(),
        });
    }

    let mut sections: Vec<String> = Vec::new();
    for section in decl.sections_vec() {
        let mapped = match section.name.as_str() {
            "request" => {
                issues.push(MigrationIssue {
                    code: "E-MIGR-002",
                    detail: "`request:` moved to `goals:` with a nested block".into(),
                });
                "goals".to_string()
            }
            "input" => {
                issues.push(MigrationIssue {
                    code: "E-MIGR-002",
                    detail: "`input:` moved to `inputs:` (plural)".into(),
                });
                "inputs".to_string()
            }
            "output" => {
                issues.push(MigrationIssue {
                    code: "E-MIGR-002",
                    detail: "`output:` moved to `outputs:` (plural)".into(),
                });
                "outputs".to_string()
            }
            other => other.to_string(),
        };
        if !sections.contains(&mapped) {
            sections.push(mapped);
        }
    }
    if decl.sections_vec().iter().any(|s| {
        s.suite
            .statements
            .iter()
            .any(|stmt| matches!(&stmt.kind, emath_core::tree::StmtKind::FnDecl { .. }))
    }) && !sections.contains(&"constructors".to_string())
    {
        sections.push("constructors".into());
        issues.push(MigrationIssue {
            code: "E-MIGR-003",
            detail: "inline constructor lifted into `constructors:` section".into(),
        });
    }

    Migrated {
        name: decl.name.clone(),
        as_kind: decl.as_kind.clone(),
        schema: format!(
            "emath.bootstrap.{}",
            if bump.is_empty() { "legacy" } else { bump }
        ),
        sections,
        issues,
    }
}
