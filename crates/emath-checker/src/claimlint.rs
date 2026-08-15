//!: claim-language linter.
//!
//! Documentation and generated README text cannot use wording stronger
//! than the current evidence supports. Each statement is scored against
//! a fixed wording table; a statement whose wording demands a higher
//! evidence level than the bundle provides is flagged with
//! `E-EVID-201` and a suggestion to downgrade.

use emath_ir::EvidenceLevel;

/// One lint finding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LintIssue {
    /// Stable code (`E-EVID-201` for overclaims).
    pub code: &'static str,
    /// 1-based line number of the offending statement.
    pub line: usize,
    /// Detail with the suggested downgrade.
    pub detail: String,
}

/// Claim-language linter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClaimLinter;

/// Evidence-level ordering: `E0 <= E1 <= ... <= E5`.
fn level_ordinal(level: EvidenceLevel) -> u8 {
    match level {
        EvidenceLevel::E0 => 0,
        EvidenceLevel::E1 => 1,
        EvidenceLevel::E2 => 2,
        EvidenceLevel::E3 => 3,
        EvidenceLevel::E4 => 4,
        EvidenceLevel::E5 => 5,
    }
}

/// Wording → minimum evidence level required, in descending strength so
/// the strongest matched term wins.
const WORDING: &[(&str, u8, &str)] = &[
    ("guaranteed", 5, "reported for tested inputs"),
    ("proven", 5, "verified by available checkers"),
    ("always", 4, "observed on the frozen corpus"),
    ("certified", 4, "checked by an available checker"),
    ("exact error bound", 3, "bounded error on tested inputs"),
    ("never fails", 3, "no failure observed"),
    ("optimal", 3, "best found"),
    ("strongly believed", 2, "measured"),
    ("expected", 1, "reported"),
    ("approximate", 0, "approximate"),
];

impl ClaimLinter {
    /// Lints documentation lines against the bundle's evidence level.
    /// Statements are matched case-insensitively; the strongest matched
    /// term decides. Overclaims are flagged deterministically in line
    /// order with a downgrade suggestion.
    #[must_use]
    pub fn lint(&self, level: EvidenceLevel, statements: &[String]) -> Vec<LintIssue> {
        let mut issues = Vec::new();
        let available = level_ordinal(level);
        for (index, statement) in statements.iter().enumerate() {
            let lowered = statement.to_ascii_lowercase();
            let best = WORDING
                .iter()
                .filter(|(term, _, _)| lowered.contains(term))
                .max_by_key(|(_, minimum, _)| *minimum);
            if let Some((term, minimum, suggestion)) = best {
                if *minimum > available {
                    issues.push(LintIssue {
                        code: "E-EVID-201",
                        line: index + 1,
                        detail: format!(
                            "statement uses {term:?} which requires E{minimum} evidence but only E{available} is available; suggest {suggestion:?}"
                        ),
                    });
                }
            }
        }
        issues
    }
}

/// Free-function entry point.
#[must_use]
pub fn lint_claims(level: EvidenceLevel, statements: &[String]) -> Vec<LintIssue> {
    ClaimLinter.lint(level, statements)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str) -> String {
        text.to_string()
    }

    #[test]
    fn overclaims_are_downgraded() {
        let statements = vec![
            line("The generated evaluator is guaranteed correct."),
            line("Results are expected within tolerance."),
        ];
        let issues = ClaimLinter.lint(EvidenceLevel::E1, &statements);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "E-EVID-201");
        assert_eq!(issues[0].line, 1);
        assert!(issues[0].detail.contains("requires E5"));
        assert!(issues[0].detail.contains("reported for tested inputs"));
    }

    #[test]
    fn adequate_evidence_passes_cleanly() {
        let statements = vec![
            line("The generated evaluator is guaranteed correct."),
            line("The optimizer is certified."),
        ];
        let issues = ClaimLinter.lint(EvidenceLevel::E5, &statements);
        assert!(issues.is_empty());
    }

    #[test]
    fn strongest_term_wins() {
        // Both "approximate" (E0) and "guaranteed" (E5) appear; the
        // strongest term decides and the line is flagged at E2.
        let statements = vec![line("approximately guaranteed results")];
        let issues = ClaimLinter.lint(EvidenceLevel::E2, &statements);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].detail.contains("E5"));
    }

    #[test]
    fn line_order_is_deterministic() {
        let statements = vec![
            line("optimal performance"),
            line("proven bounds"),
            line("approximately timed"),
        ];
        let issues = ClaimLinter.lint(EvidenceLevel::E0, &statements);
        let lines: Vec<usize> = issues.iter().map(|issue| issue.line).collect();
        assert_eq!(lines, [1, 2]);
    }
}
