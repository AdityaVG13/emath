//! Engine identity: honest comparator separation.
//!
//! Every lab comparison runs between two distinct engine identities;
//! comparing an engine with itself is refused with `E-HOST-016`, never
//! a silent self-comparison.

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

    /// Refuse a comparison between identical identities (`E-HOST-016`):
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
