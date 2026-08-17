//! Engine identity: honest comparator separation.
//!
//! Every statistical comparison in the lab is a comparison between two
//! *engine identities* (subject vs oracle / baseline vs mutant). A
//! comparator that cannot prove its two inputs are distinct identities
//! can silently compare an engine with itself; that is refused with
//! `E-HOST-016` (never a silent self-comparison).

use crate::error::LabError;

/// Role of one side of a comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EngineRole {
    /// The engine under test (e.g. `emath-HEAD-<hash>`).
    Subject,
    /// The reference the subject is compared against (spec oracle,
    /// pinned prior commit, or trusted reference engine).
    Oracle,
    /// A baseline engine outside the subject/oracle pairing.
    Baseline,
    /// A deliberately mutated transform (mutation-kill campaigns).
    Mutant,
}

impl EngineRole {
    /// Stable role token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Subject => "subject",
            Self::Oracle => "oracle",
            Self::Baseline => "baseline",
            Self::Mutant => "mutant",
        }
    }
}

/// One engine identity: role + stable label (e.g. `emath-HEAD-a1401c0`
/// vs `emath-spec-oracle`). The pair is the honest comparator input.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EngineIdentity {
    /// Role in the comparison.
    pub role: EngineRole,
    /// Stable engine label; must distinguish engines within a role.
    pub label: String,
}

impl EngineIdentity {
    /// Subject identity convenience.
    #[must_use]
    pub fn subject(label: &str) -> Self {
        Self {
            role: EngineRole::Subject,
            label: label.to_string(),
        }
    }

    /// Oracle identity convenience.
    #[must_use]
    pub fn oracle(label: &str) -> Self {
        Self {
            role: EngineRole::Oracle,
            label: label.to_string(),
        }
    }

    /// Stable token for receipts (`role:label`).
    #[must_use]
    pub fn token(&self) -> String {
        format!("{}:{}", self.role.as_str(), self.label)
    }

    /// Refuses a comparison between two identities that do not differ
    /// (`E-HOST-016`). A comparator that could run with `subject ==
    /// oracle` would be comparing an engine with itself: the refusal is
    /// typed, never a silent self-comparison.
    pub fn require_distinct(
        &self,
        other: &EngineIdentity,
        comparator: &str,
    ) -> Result<(), LabError> {
        if self == other {
            return Err(LabError::new(
                "E-HOST-016",
                format!(
                    "comparator `{comparator}` got the same engine identity on both sides (`{}`); \
                     a subject-oracle self-comparison is refused",
                    self.token()
                ),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_identities_pass_the_comparator_gate() {
        let subject = EngineIdentity::subject("emath-HEAD-a1401c0");
        let oracle = EngineIdentity::oracle("emath-spec-oracle");
        assert!(subject.require_distinct(&oracle, "evaluate_paired").is_ok());
        assert!(oracle.require_distinct(&subject, "evaluate_paired").is_ok());
    }

    #[test]
    fn identical_identities_are_refused_by_the_comparator_gate() {
        let subject = EngineIdentity::subject("emath-HEAD-a1401c0");
        let clone = EngineIdentity::subject("emath-HEAD-a1401c0");
        let error = subject
            .require_distinct(&clone, "evaluate_paired")
            .unwrap_err();
        assert_eq!(error.code, "E-HOST-016");
        assert!(error.message.contains("evaluate_paired"));
        // Same role, different labels: still a valid comparison.
        let other = EngineIdentity::subject("emath-HEAD-b2e8d00");
        assert!(subject.require_distinct(&other, "evaluate_paired").is_ok());
    }

    #[test]
    fn tokens_are_role_label_stable() {
        assert_eq!(
            EngineIdentity::oracle("emath-spec-oracle").token(),
            "oracle:emath-spec-oracle"
        );
        assert_eq!(
            EngineIdentity::subject("emath-HEAD-a1401c0").token(),
            "subject:emath-HEAD-a1401c0"
        );
    }
}
