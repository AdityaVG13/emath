//! core: migration from bootstrap syntax.
//!
//! The bootstrap parser stays behind a compatibility tool, not the
//! production compiler. `migrate_declaration` rewrites a bootstrap-era
//! declaration into the open framework carrying its bootstrap schema:
//! `request` sections become `requests`, `input`/`output` singletons
//! become plural `inputs`/`outputs`, and inline constructors are lifted
//! into a `constructors:` section.

use emath_syntax::tree::Declaration;

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
    if bump == "v4" {
        issues.push(MigrationIssue {
            code: "E-MIGR-001",
            detail: "bootstrap schema v4 written; regenerate with the stable edition".into(),
        });
    }

    let mut sections: Vec<String> = Vec::new();
    for section in &decl.sections {
        let mapped = match section.name.as_str() {
            "request" => {
                issues.push(MigrationIssue {
                    code: "E-MIGR-002",
                    detail: "`request:` moved to `requests:` with a nested block".into(),
                });
                "requests".to_string()
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
    if decl.sections.iter().any(|s| {
        s.suite
            .statements
            .iter()
            .any(|stmt| matches!(&stmt.kind, emath_syntax::tree::StmtKind::FnDecl { .. }))
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
            if bump.is_empty() { "v3" } else { bump }
        ),
        sections,
        issues,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emath_syntax::{parse_str, tree::Item};

    fn declare(text: &str) -> Declaration {
        let (tree, diagnostics) = parse_str(text);
        assert!(diagnostics.is_empty());
        for item in &tree.items {
            if let Item::Declaration(decl) = item {
                return decl.clone();
            }
        }
        panic!("no declaration");
    }

    const BOOTSTRAP: &str = "emath custom <Legacy> as policy:\n    input:\n        x: Float64\n    output:\n        y: Float64\n    request:\n        evaluate <y>:\n            produce rust.library\n    constructors:\n        public fn new() -> Self:\n            Self:\n                scale = 1.0\n";

    #[test]
    fn migrates_bootstrap_sections_and_schema() {
        let decl = declare(BOOTSTRAP);
        let migrated = migrate_declaration(&decl, "");
        assert_eq!(migrated.schema, "emath.bootstrap.v3");
        assert!(migrated.sections.contains(&"inputs".to_string()));
        assert!(migrated.sections.contains(&"outputs".to_string()));
        assert!(migrated.sections.contains(&"requests".to_string()));
        assert!(migrated
            .issues
            .iter()
            .any(|issue| issue.code == "E-MIGR-002"));
    }

    #[test]
    fn new_schema_moves_the_migration_identity() {
        let decl = declare(BOOTSTRAP);
        let v3 = migrate_declaration(&decl, "");
        let v4 = migrate_declaration(&decl, "v4");
        assert_eq!(v3.schema, "emath.bootstrap.v3");
        assert_eq!(v4.schema, "emath.bootstrap.v4");
        assert_ne!(v3.schema, v4.schema);
        assert!(v4.issues.iter().any(|issue| issue.code == "E-MIGR-001"));
    }
}
